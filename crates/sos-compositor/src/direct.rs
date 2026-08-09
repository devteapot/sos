// The direct backend is intentionally a single-seat, single-GPU implementation
// adapted from Smithay's MIT-licensed Anvil udev backend at tag v0.7.0. It keeps
// SOS policy independent from KMS and releases an activation fence only from the
// VBlank event corresponding to the queued shell buffer.

use std::{collections::HashMap, path::Path, time::Duration};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::{PresentationClock, PresentationEvidence};
use smithay::{
    backend::{
        allocator::{
            format::FormatSet,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc,
        },
        drm::{
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
            DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata, DrmEventTime, DrmNode,
        },
        egl::{EGLContext, EGLDevice, EGLDisplay},
        input::{Device as _, InputEvent},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            element::{surface::WaylandSurfaceRenderElement, RenderElementStates},
            gles::GlesRenderer,
            ImportEgl, ImportMemWl,
        },
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
        udev::{UdevBackend, UdevEvent},
    },
    desktop::{
        space::{space_render_elements, SpaceRenderElements},
        utils::OutputPresentationFeedback,
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop,
        },
        drm::control::{connector, crtc, ModeTypeFlags},
        input::Libinput,
        rustix::fs::OFlags,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{DeviceFd, Monotonic, Time, Transform},
    wayland::presentation::Refresh,
};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};

use crate::{mark_backend_ready, policy::QueuedRevision, state::SosCompositor, CompositorData};

const FRAME_INTERVAL: Duration = Duration::from_millis(8);
const CLEAR_COLOR: [f32; 4] = [0.025, 0.03, 0.035, 1.0];
const COLOR_FORMATS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Abgr8888];

type DirectOutput = DrmOutput<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    DirectFrame,
    DrmDeviceFd,
>;

struct DirectFrame {
    feedback: OutputPresentationFeedback,
    revision: Option<QueuedRevision>,
}

struct OutputData {
    output: Output,
    drm_output: DirectOutput,
    frame_pending: bool,
}

struct DeviceData {
    renderer: GlesRenderer,
    manager: DrmOutputManager<
        GbmAllocator<DrmDeviceFd>,
        GbmFramebufferExporter<DrmDeviceFd>,
        DirectFrame,
        DrmDeviceFd,
    >,
    scanner: DrmScanner,
    outputs: HashMap<crtc::Handle, OutputData>,
}

pub struct DirectBackend {
    session: LibSeatSession,
    devices: HashMap<DrmNode, DeviceData>,
}

pub fn init_direct(
    event_loop: &mut EventLoop<CompositorData>,
    data: &mut CompositorData,
) -> Result<()> {
    let (session, session_notifier) = LibSeatSession::new().context("open libseat session")?;
    let seat_name = session.seat();
    let udev = UdevBackend::new(&seat_name).context("initialize DRM udev monitor")?;
    let devices = udev
        .device_list()
        .map(|(device_id, path)| (device_id, path.to_owned()))
        .collect::<Vec<_>>();
    if devices.is_empty() {
        bail!("no DRM devices were found on seat {seat_name}");
    }

    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat_name)
        .map_err(|_| anyhow::anyhow!("assign libinput to seat {seat_name}"))?;
    let input_backend = LibinputInputBackend::new(libinput.clone());
    event_loop
        .handle()
        .insert_source(input_backend, |event, _, data| {
            match &event {
                InputEvent::DeviceAdded { device } => tracing::info!(
                    device_id = device.id(),
                    device_name = device.name(),
                    "libinput device added"
                ),
                InputEvent::DeviceRemoved { device } => tracing::info!(
                    device_id = device.id(),
                    device_name = device.name(),
                    "libinput device removed"
                ),
                _ => {}
            }
            data.state.process_input_event(event);
        })
        .map_err(|_| anyhow::anyhow!("insert libinput event source"))?;

    event_loop
        .handle()
        .insert_source(session_notifier, move |event, _, data| match event {
            SessionEvent::PauseSession => {
                libinput.suspend();
                if let Some(direct) = &mut data.direct {
                    for device in direct.devices.values_mut() {
                        device.manager.pause();
                        for output in device.outputs.values_mut() {
                            output.frame_pending = false;
                        }
                    }
                }
                tracing::info!("direct session paused");
            }
            SessionEvent::ActivateSession => {
                if let Err(error) = libinput.resume() {
                    tracing::error!(?error, "could not resume libinput");
                }
                if let Some(direct) = &mut data.direct {
                    for device in direct.devices.values_mut() {
                        if let Err(error) = device.manager.activate(false) {
                            tracing::error!(%error, "could not reactivate DRM device");
                        }
                    }
                }
                tracing::info!("direct session activated");
            }
        })
        .map_err(|_| anyhow::anyhow!("insert libseat event source"))?;

    data.direct = Some(DirectBackend {
        session,
        devices: HashMap::new(),
    });
    for (device_id, path) in devices {
        let node = DrmNode::from_dev_id(device_id).context("identify DRM node")?;
        add_device(event_loop, data, node, &path)?;
    }

    event_loop
        .handle()
        .insert_source(udev, |event, _, data| match event {
            UdevEvent::Added { device_id, path } => {
                let Ok(node) = DrmNode::from_dev_id(device_id) else {
                    return;
                };
                if data
                    .direct
                    .as_ref()
                    .is_some_and(|direct| direct.devices.contains_key(&node))
                {
                    return;
                }
                tracing::error!(?node, path = %path.display(), "DRM hot-add requires compositor restart");
            }
            UdevEvent::Changed { device_id } => {
                tracing::warn!(?device_id, "DRM connector change requires compositor restart");
            }
            UdevEvent::Removed { device_id } => {
                tracing::error!(?device_id, "active DRM device was removed");
                data.loop_signal.stop();
            }
        })
        .map_err(|_| anyhow::anyhow!("insert udev event source"))?;

    event_loop
        .handle()
        .insert_source(Timer::from_duration(FRAME_INTERVAL), |_, _, data| {
            render_all(data);
            TimeoutAction::ToDuration(FRAME_INTERVAL)
        })
        .map_err(|_| anyhow::anyhow!("insert DRM repaint timer"))?;

    tracing::info!(seat = seat_name, "initialized direct DRM/libinput session");
    render_all(data);
    Ok(())
}

fn add_device(
    event_loop: &mut EventLoop<CompositorData>,
    data: &mut CompositorData,
    node: DrmNode,
    path: &Path,
) -> Result<()> {
    let direct = data.direct.as_mut().context("direct backend is missing")?;
    if !direct.devices.is_empty() {
        bail!("direct backend currently supports exactly one DRM device");
    }
    let fd = direct
        .session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .with_context(|| format!("open DRM device through libseat: {}", path.display()))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, notifier) = DrmDevice::new(fd.clone(), true).context("initialize DRM device")?;
    event_loop
        .handle()
        .insert_source(notifier, move |event, metadata, data| match event {
            DrmEvent::VBlank(crtc) => finish_frame(data, node, crtc, metadata.take()),
            DrmEvent::Error(error) => {
                tracing::error!(%error, "DRM event source failed");
                data.loop_signal.stop();
            }
        })
        .map_err(|_| anyhow::anyhow!("insert DRM event source"))?;

    let gbm = GbmDevice::new(fd).context("initialize GBM device")?;
    let egl_display =
        unsafe { EGLDisplay::new(gbm.clone()) }.context("initialize GBM EGL display")?;
    let render_node = EGLDevice::device_for_display(&egl_display)
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten());
    let context = EGLContext::new(&egl_display).context("create EGL context")?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.context("create GLES renderer")?;
    if let Err(error) = renderer.bind_wl_display(&data.display_handle) {
        tracing::warn!(%error, "EGL Wayland display binding is unavailable; wl_shm remains enabled");
    }
    data.state.shm_state.update_formats(renderer.shm_formats());
    let render_formats = renderer
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect::<FormatSet>();
    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(gbm.clone(), render_node);
    let manager = DrmOutputManager::new(
        drm,
        allocator,
        exporter,
        Some(gbm),
        COLOR_FORMATS,
        render_formats,
    );
    direct.devices.insert(
        node,
        DeviceData {
            renderer,
            manager,
            scanner: DrmScanner::new(),
            outputs: HashMap::new(),
        },
    );
    scan_connectors(data, node)?;
    Ok(())
}

fn scan_connectors(data: &mut CompositorData, node: DrmNode) -> Result<()> {
    let direct = data.direct.as_mut().context("direct backend is missing")?;
    let device = direct
        .devices
        .get_mut(&node)
        .context("DRM device is missing")?;
    let events = device
        .scanner
        .scan_connectors(device.manager.device())
        .context("scan DRM connectors")?
        .into_iter()
        .collect::<Vec<_>>();
    for event in events {
        match event {
            DrmScanEvent::Connected {
                connector,
                crtc: Some(crtc),
            } => connect_output(data, node, connector, crtc)?,
            DrmScanEvent::Connected {
                connector,
                crtc: None,
            } => {
                tracing::warn!(connector = ?connector.handle(), "connected DRM output has no CRTC");
            }
            DrmScanEvent::Disconnected { .. } => {
                bail!("DRM output disconnected; restart the compositor after reconnecting it")
            }
        }
    }
    if data.state.space.outputs().next().is_none() {
        bail!("no connected desktop DRM output was found");
    }
    Ok(())
}

fn connect_output(
    data: &mut CompositorData,
    node: DrmNode,
    connector: connector::Info,
    crtc: crtc::Handle,
) -> Result<()> {
    if data.state.space.outputs().next().is_some() {
        bail!("direct backend currently supports exactly one connected output");
    }
    let mode_index = connector
        .modes()
        .iter()
        .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .unwrap_or(0);
    let drm_mode = *connector
        .modes()
        .get(mode_index)
        .context("connected DRM output has no mode")?;
    let mode = Mode::from(drm_mode);
    let name = format!(
        "{}-{}",
        connector.interface().as_str(),
        connector.interface_id()
    );
    let physical_size = connector.size().unwrap_or((0, 0));
    let output = Output::new(
        name.clone(),
        PhysicalProperties {
            size: (physical_size.0 as i32, physical_size.1 as i32).into(),
            subpixel: Subpixel::Unknown,
            make: "SOS".into(),
            model: "Direct KMS output".into(),
        },
    );
    output.set_preferred(mode);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    let _global = output.create_global::<SosCompositor>(&data.display_handle);
    data.state.space.map_output(&output, (0, 0));
    data.state.output_size = mode.size.into();

    let direct = data.direct.as_mut().context("direct backend is missing")?;
    let device = direct
        .devices
        .get_mut(&node)
        .context("DRM device is missing")?;
    let planes = device.manager.device().planes(&crtc)?;
    let drm_output = device
        .manager
        .initialize_output::<
            _,
            SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
        >(
            crtc,
            drm_mode,
            &[connector.handle()],
            &output,
            Some(planes),
            &mut device.renderer,
            &DrmOutputRenderElements::default(),
        )
        .context("initialize direct KMS output")?;
    device.outputs.insert(
        crtc,
        OutputData {
            output,
            drm_output,
            frame_pending: false,
        },
    );
    tracing::info!(
        output = name,
        width = mode.size.w,
        height = mode.size.h,
        "initialized direct KMS output"
    );
    Ok(())
}

fn render_all(data: &mut CompositorData) {
    let targets = data
        .direct
        .as_ref()
        .map(|direct| {
            direct
                .devices
                .iter()
                .flat_map(|(node, device)| device.outputs.keys().map(|crtc| (*node, *crtc)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (node, crtc) in targets {
        if let Err(error) = render_output(data, node, crtc) {
            tracing::error!(%error, ?node, ?crtc, "direct output render failed");
            data.loop_signal.stop();
            return;
        }
    }
}

fn render_output(data: &mut CompositorData, node: DrmNode, crtc: crtc::Handle) -> Result<()> {
    let (state, direct) = (
        &mut data.state,
        data.direct.as_mut().context("direct backend is missing")?,
    );
    let device = direct
        .devices
        .get_mut(&node)
        .context("DRM device is missing")?;
    let output_data = device
        .outputs
        .get_mut(&crtc)
        .context("DRM output is missing")?;
    if output_data.frame_pending {
        return Ok(());
    }
    let elements = space_render_elements(
        &mut device.renderer,
        [&state.space],
        &output_data.output,
        1.0,
    )?;
    let result = output_data
        .drm_output
        .render_frame(
            &mut device.renderer,
            &elements,
            CLEAR_COLOR,
            FrameFlags::DEFAULT,
        )
        .map_err(|error| anyhow::anyhow!("prepare direct frame: {error}"))?;
    send_frame_callbacks(state, &output_data.output);
    if result.is_empty {
        return Ok(());
    }
    let shell_rendered = state.shell_rendered(&result.states);
    let queued_revision = state.policy.queued_revision(shell_rendered);
    let feedback = take_presentation_feedback(state, &output_data.output, &result.states);
    output_data
        .drm_output
        .queue_frame(DirectFrame {
            feedback,
            revision: queued_revision.clone(),
        })
        .map_err(|error| anyhow::anyhow!("queue direct frame: {error}"))?;
    state.policy.record_frame_queued(queued_revision.as_ref());
    output_data.frame_pending = true;
    state.space.refresh();
    state.popups.cleanup();
    let _ = data.display_handle.flush_clients();
    Ok(())
}

fn send_frame_callbacks(state: &mut SosCompositor, output: &Output) {
    state.space.elements().for_each(|window| {
        window.send_frame(output, state.clock.now(), Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    });
}

fn finish_frame(
    data: &mut CompositorData,
    node: DrmNode,
    crtc: crtc::Handle,
    metadata: Option<DrmEventMetadata>,
) {
    let Some(direct) = &mut data.direct else {
        return;
    };
    let Some(device) = direct.devices.get_mut(&node) else {
        return;
    };
    let Some(output_data) = device.outputs.get_mut(&crtc) else {
        return;
    };
    output_data.frame_pending = false;
    let submitted = match output_data.drm_output.frame_submitted() {
        Ok(frame) => frame,
        Err(error) => {
            tracing::error!(%error, "could not complete submitted DRM frame");
            data.loop_signal.stop();
            return;
        }
    };
    let Some(mut frame) = submitted else {
        return;
    };
    let Some(metadata) = metadata else {
        tracing::error!("DRM page flip omitted timing metadata; activation fence remains closed");
        return;
    };
    let (presentation_time, timestamp, clock, feedback_flags) = match metadata.time {
        DrmEventTime::Monotonic(time) => (
            Time::<Monotonic>::from(time),
            time,
            PresentationClock::Monotonic,
            wp_presentation_feedback::Kind::Vsync
                | wp_presentation_feedback::Kind::HwClock
                | wp_presentation_feedback::Kind::HwCompletion,
        ),
        DrmEventTime::Realtime(time) => {
            let now = data.state.clock.now();
            (
                now,
                time.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default(),
                PresentationClock::Realtime,
                wp_presentation_feedback::Kind::Vsync,
            )
        }
    };
    let refresh = output_data
        .output
        .current_mode()
        .map(|mode| {
            Duration::from_nanos(1_000_000_000_000u64 / u64::try_from(mode.refresh.max(1)).unwrap())
        })
        .unwrap_or(Duration::from_nanos(16_666_667));
    frame.feedback.presented(
        presentation_time,
        Refresh::fixed(refresh),
        metadata.sequence.into(),
        feedback_flags,
    );
    let revision = frame
        .revision
        .and_then(|queued| data.state.policy.record_presented(queued));
    let recovery_view = !data.state.policy.shell_mapped();
    let recovery_changed = data.last_recovery_view != Some(recovery_view);
    data.last_recovery_view = Some(recovery_view);
    if recovery_changed || revision.is_some() {
        tracing::info!(
            output = output_data.output.name(),
            output_sequence = metadata.sequence,
            presentation_clock = ?clock,
            timestamp_seconds = timestamp.as_secs(),
            timestamp_nanoseconds = timestamp.subsec_nanos(),
            recovery_view,
            "completed significant DRM page flip"
        );
    } else {
        tracing::trace!(
            output = output_data.output.name(),
            output_sequence = metadata.sequence,
            recovery_view,
            "completed unchanged DRM page flip"
        );
    }
    mark_backend_ready(data, "drm_page_flip");
    if let Some(revision) = revision {
        data.state.publish_presentation(
            revision,
            PresentationEvidence::DrmPageFlip {
                output_sequence: metadata.sequence.into(),
                timestamp_seconds: timestamp.as_secs(),
                timestamp_nanoseconds: timestamp.subsec_nanos(),
                clock,
            },
        );
    }
}

fn take_presentation_feedback(
    state: &SosCompositor,
    output: &Output,
    states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut feedback = OutputPresentationFeedback::new(output);
    state.space.elements().for_each(|window| {
        if state.space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(
                &mut feedback,
                |_: &WlSurface, _| Some(output.clone()),
                |surface, _| {
                    smithay::desktop::utils::surface_presentation_feedback_flags_from_states(
                        surface, states,
                    )
                },
            );
        }
    });
    feedback
}
