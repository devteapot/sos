// The direct backend is intentionally a single-seat implementation
// adapted from Smithay's MIT-licensed Anvil udev backend at tag v0.7.0. It keeps
// SOS policy independent from KMS and releases an activation fence only from the
// VBlank event corresponding to the queued shell buffer.

use std::{cell::RefCell, collections::HashMap, fs, path::Path, rc::Rc, time::Duration};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::{PresentationClock, PresentationEvidence};
use serde::Deserialize;
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
            DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata, DrmEventTime, DrmNode, NodeType,
        },
        egl::{EGLContext, EGLDevice, EGLDisplay},
        input::{Device as _, InputEvent},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
                Kind, RenderElementStates,
            },
            gles::GlesRenderer,
            ImportDma, ImportEgl, ImportMemWl,
        },
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
        udev::{UdevBackend, UdevEvent},
    },
    desktop::{
        space::{space_render_elements, SpaceRenderElements},
        utils::OutputPresentationFeedback,
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop, LoopHandle, RegistrationToken,
        },
        drm::control::{connector, crtc, ModeTypeFlags},
        input::Libinput,
        rustix::fs::OFlags,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::{protocol::wl_surface::WlSurface, Resource as _},
    },
    utils::{DeviceFd, Monotonic, Time, Transform},
    wayland::{
        compositor,
        dmabuf::{DmabufFeedbackBuilder, DmabufState},
        presentation::Refresh,
    },
};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};

use crate::{mark_backend_ready, policy::QueuedRevision, state::SosCompositor, CompositorData};

const FRAME_INTERVAL: Duration = Duration::from_millis(8);
const CLEAR_COLOR: [f32; 4] = [0.025, 0.03, 0.035, 1.0];
const COLOR_FORMATS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Abgr8888];
const CURSOR_WIDTH: i32 = 18;
const CURSOR_HEIGHT: i32 = 24;

smithay::backend::renderer::element::render_elements! {
    DirectRenderElement<=GlesRenderer>;
    Space=SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
    CursorSurface=WaylandSurfaceRenderElement<GlesRenderer>,
    Cursor=MemoryRenderBufferRenderElement<GlesRenderer>,
}

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
    needs_initial_damage: bool,
}

struct DeviceData {
    event_token: RegistrationToken,
    renderer: Rc<RefCell<GlesRenderer>>,
    manager: DrmOutputManager<
        GbmAllocator<DrmDeviceFd>,
        GbmFramebufferExporter<DrmDeviceFd>,
        DirectFrame,
        DrmDeviceFd,
    >,
    scanner: DrmScanner,
    outputs: HashMap<crtc::Handle, OutputData>,
}

#[derive(Clone, Debug, PartialEq)]
struct OutputConfig {
    requested_size: Option<(i32, i32)>,
    rotation: u16,
    scale: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct OutputConfigFile {
    mode: Option<String>,
    rotation: Option<u16>,
    scale: Option<f64>,
}

pub struct DirectBackend {
    session: LibSeatSession,
    devices: HashMap<DrmNode, DeviceData>,
    cursor_buffer: MemoryRenderBuffer,
    initial_damage_buffer: MemoryRenderBuffer,
    session_paused: bool,
    output_config: OutputConfig,
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
            if data.state.take_session_exit_request() {
                data.loop_signal.stop();
            }
        })
        .map_err(|_| anyhow::anyhow!("insert libinput event source"))?;

    event_loop
        .handle()
        .insert_source(session_notifier, move |event, _, data| match event {
            SessionEvent::PauseSession => {
                libinput.suspend();
                if let Some(direct) = &mut data.direct {
                    direct.session_paused = true;
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
                    direct.session_paused = false;
                }
                tracing::info!("direct session activated");
            }
        })
        .map_err(|_| anyhow::anyhow!("insert libseat event source"))?;

    let loop_handle = event_loop.handle();
    let output_config = load_output_config()?;
    data.direct = Some(DirectBackend {
        session,
        devices: HashMap::new(),
        cursor_buffer: default_cursor_buffer(),
        initial_damage_buffer: initial_damage_buffer(),
        session_paused: false,
        output_config,
    });
    for (device_id, path) in devices {
        let node = DrmNode::from_dev_id(device_id).context("identify DRM node")?;
        add_device(&loop_handle, data, node, &path)?;
    }
    refresh_dmabuf_global(data)?;
    if data.state.space.outputs().next().is_none() {
        bail!("no connected desktop DRM output was found");
    }

    let udev_loop_handle = event_loop.handle();
    event_loop
        .handle()
        .insert_source(udev, move |event, _, data| match event {
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
                if let Err(error) = add_device(&udev_loop_handle, data, node, &path) {
                    tracing::error!(%error, ?node, path = %path.display(), "could not hot-add DRM device");
                } else if let Err(error) = refresh_dmabuf_global(data) {
                    tracing::error!(%error, ?node, "could not update dmabuf feedback after hot-add");
                } else {
                    tracing::info!(?node, path = %path.display(), "hot-added DRM device");
                }
            }
            UdevEvent::Changed { device_id } => {
                let Ok(node) = DrmNode::from_dev_id(device_id) else {
                    return;
                };
                let result = refresh_output_config(data)
                    .and_then(|changed| (!changed).then(|| scan_connectors(data, node)).transpose())
                    .and_then(|_| refresh_dmabuf_global(data))
                    .map(|_| ());
                if let Err(error) = result {
                    tracing::error!(%error, ?node, "could not apply DRM connector hotplug");
                }
            }
            UdevEvent::Removed { device_id } => {
                let Ok(node) = DrmNode::from_dev_id(device_id) else {
                    return;
                };
                remove_device(&udev_loop_handle, data, node);
                tracing::info!(?node, "hot-removed DRM device; waiting for a replacement");
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
    loop_handle: &LoopHandle<'_, CompositorData>,
    data: &mut CompositorData,
    node: DrmNode,
    path: &Path,
) -> Result<()> {
    let direct = data.direct.as_mut().context("direct backend is missing")?;
    let fd = direct
        .session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .with_context(|| format!("open DRM device through libseat: {}", path.display()))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, notifier) = DrmDevice::new(fd.clone(), true).context("initialize DRM device")?;
    let event_token = loop_handle
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
        .and_then(|device| device.try_get_render_node().ok().flatten())
        .or_else(|| node.node_with_type(NodeType::Render).and_then(Result::ok));
    let feedback_node = render_node.unwrap_or(node);
    let context = EGLContext::new(&egl_display).context("create EGL context")?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.context("create GLES renderer")?;
    if let Err(error) = renderer.bind_wl_display(&data.display_handle) {
        tracing::warn!(%error, "EGL Wayland display binding is unavailable; continuing with explicit Linux dmabuf and wl_shm paths");
    }
    data.state.shm_state.update_formats(renderer.shm_formats());
    let render_formats = renderer
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect::<FormatSet>();
    let renderer = Rc::new(RefCell::new(renderer));
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
            event_token,
            renderer: Rc::clone(&renderer),
            manager,
            scanner: DrmScanner::new(),
            outputs: HashMap::new(),
        },
    );
    data.state.dmabuf_renderers.insert(node, renderer);
    data.state.dmabuf_render_nodes.insert(node, feedback_node);
    scan_connectors(data, node)?;
    Ok(())
}

fn common_dmabuf_formats(mut sets: impl Iterator<Item = FormatSet>) -> FormatSet {
    let Some(mut common) = sets.next() else {
        return FormatSet::default();
    };
    for formats in sets {
        common = common.intersection(&formats).copied().collect();
    }
    common
}

fn refresh_dmabuf_global(data: &mut CompositorData) -> Result<()> {
    let active_devices = data
        .direct
        .as_ref()
        .context("direct backend is missing")?
        .devices
        .iter()
        .filter_map(|(node, device)| (!device.outputs.is_empty()).then_some(*node))
        .collect();
    data.state.dmabuf_active_devices = active_devices;
    let state = &mut data.state;
    let mut devices = state
        .dmabuf_active_devices
        .iter()
        .copied()
        .collect::<Vec<_>>();
    devices.sort_by_key(|node| (node.major(), node.minor()));
    let Some(primary_device) = devices.first().copied() else {
        state.dmabuf_primary = None;
        tracing::warn!("no connected direct renderer is available for Linux dmabuf");
        return Ok(());
    };
    let primary_render_node = *state
        .dmabuf_render_nodes
        .get(&primary_device)
        .context("direct renderer has no dmabuf render node")?;
    let format_sets = devices
        .iter()
        .map(|node| {
            state
                .dmabuf_renderers
                .get(node)
                .expect("listed dmabuf renderer exists")
                .borrow()
                .dmabuf_formats()
        })
        .collect::<Vec<_>>();
    let formats = common_dmabuf_formats(format_sets.into_iter());
    if formats.iter().next().is_none() {
        bail!("direct renderers share no importable dmabuf format");
    }
    let format_count = formats.iter().count();
    let feedback = DmabufFeedbackBuilder::new(primary_render_node.dev_id(), formats)
        .build()
        .context("build direct renderer dmabuf feedback")?;
    if let Some((dmabuf_state, global)) = &state.dmabuf_state {
        dmabuf_state.set_default_feedback(global, &feedback);
    } else {
        let mut dmabuf_state = DmabufState::new();
        let global = dmabuf_state
            .create_global_with_default_feedback::<SosCompositor>(&state.display_handle, &feedback);
        state.dmabuf_state = Some((dmabuf_state, global));
    }
    state.dmabuf_primary = Some(primary_render_node);
    tracing::info!(
        ?primary_render_node,
        format_count,
        renderers = devices.len(),
        "advertised Linux dmabuf feedback"
    );
    Ok(())
}

fn remove_device(
    loop_handle: &LoopHandle<'_, CompositorData>,
    data: &mut CompositorData,
    node: DrmNode,
) {
    let Some(mut device) = data
        .direct
        .as_mut()
        .and_then(|direct| direct.devices.remove(&node))
    else {
        return;
    };
    for output in device.outputs.drain().map(|(_, output)| output.output) {
        data.state.space.unmap_output(&output);
    }
    data.state.dmabuf_renderers.remove(&node);
    data.state.dmabuf_render_nodes.remove(&node);
    data.state.dmabuf_active_devices.remove(&node);
    if let Err(error) = refresh_dmabuf_global(data) {
        tracing::error!(%error, ?node, "could not update dmabuf feedback after device removal");
    }
    loop_handle.remove(device.event_token);
    update_output_layout(&mut data.state);
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
            DrmScanEvent::Disconnected {
                connector,
                crtc: Some(crtc),
            } => {
                disconnect_output(data, node, crtc);
                tracing::info!(connector = ?connector.handle(), ?crtc, "disconnected DRM output");
            }
            DrmScanEvent::Disconnected {
                connector,
                crtc: None,
            } => tracing::info!(
                connector = ?connector.handle(),
                "disconnected unassigned DRM connector"
            ),
        }
    }
    Ok(())
}

fn refresh_output_config(data: &mut CompositorData) -> Result<bool> {
    let next = load_output_config()?;
    let direct = data.direct.as_mut().context("direct backend is missing")?;
    if direct.output_config == next {
        return Ok(false);
    }
    tracing::info!(?next, "applying changed direct output configuration");
    direct.output_config = next;
    let nodes = direct.devices.keys().copied().collect::<Vec<_>>();
    for device in direct.devices.values_mut() {
        for output in device.outputs.drain().map(|(_, output)| output.output) {
            data.state.space.unmap_output(&output);
        }
        device.scanner = DrmScanner::new();
    }
    update_output_layout(&mut data.state);
    for node in nodes {
        scan_connectors(data, node)?;
    }
    Ok(true)
}

fn disconnect_output(data: &mut CompositorData, node: DrmNode, crtc: crtc::Handle) {
    let output = data
        .direct
        .as_mut()
        .and_then(|direct| direct.devices.get_mut(&node))
        .and_then(|device| device.outputs.remove(&crtc))
        .map(|output| output.output);
    if let Some(output) = output {
        data.state.space.unmap_output(&output);
        update_output_layout(&mut data.state);
    }
}

fn connect_output(
    data: &mut CompositorData,
    node: DrmNode,
    connector: connector::Info,
    crtc: crtc::Handle,
) -> Result<()> {
    let config = data
        .direct
        .as_ref()
        .context("direct backend is missing")?
        .output_config
        .clone();
    let requested_size = config.requested_size;
    let mode_index = requested_size
        .and_then(|size| {
            connector
                .modes()
                .iter()
                .position(|mode| Mode::from(*mode).size == size.into())
        })
        .or_else(|| {
            connector
                .modes()
                .iter()
                .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        })
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
    let transform = match config.rotation {
        90 => Transform::_90,
        180 => Transform::_180,
        270 => Transform::_270,
        _ => Transform::Normal,
    };
    let scale = config.scale;
    output.change_current_state(
        Some(mode),
        Some(transform),
        Some(Scale::Fractional(scale)),
        Some((0, 0).into()),
    );
    let _global = output.create_global::<SosCompositor>(&data.display_handle);
    data.state.space.map_output(&output, (0, 0));

    let direct = data.direct.as_mut().context("direct backend is missing")?;
    let device = direct
        .devices
        .get_mut(&node)
        .context("DRM device is missing")?;
    let planes = device.manager.device().planes(&crtc)?;
    let mut renderer = device.renderer.borrow_mut();
    let drm_output = device
        .manager
        .initialize_output::<_, DirectRenderElement>(
            crtc,
            drm_mode,
            &[connector.handle()],
            &output,
            Some(planes),
            &mut *renderer,
            &DrmOutputRenderElements::default(),
        )
        .context("initialize direct KMS output")?;
    // `initialize_output` performs a validation commit. Reset its swapchain so
    // the first compositor frame has age zero and damages the complete CRTC,
    // even when a newly hot-plugged output is temporarily outside the shell's
    // last acknowledged size.
    drm_output.reset_buffers();
    device.outputs.insert(
        crtc,
        OutputData {
            output,
            drm_output,
            frame_pending: false,
            needs_initial_damage: true,
        },
    );
    update_output_layout(&mut data.state);
    tracing::info!(
        output = name,
        width = mode.size.w,
        height = mode.size.h,
        scale,
        ?transform,
        "initialized direct KMS output"
    );
    Ok(())
}

fn load_output_config() -> Result<OutputConfig> {
    let mut file = OutputConfigFile::default();
    if let Some(path) = std::env::var_os("SOS_OUTPUT_CONFIG_FILE") {
        let bytes = fs::read(&path)
            .with_context(|| format!("read output configuration {}", Path::new(&path).display()))?;
        if bytes.len() > 4096 {
            bail!("output configuration exceeds 4096 bytes");
        }
        file = serde_json::from_slice(&bytes).context("parse output configuration")?;
    }
    let mode = std::env::var("SOS_OUTPUT_MODE").ok().or(file.mode);
    let requested_size = mode.as_deref().map(parse_output_mode).transpose()?;
    let rotation = std::env::var("SOS_OUTPUT_ROTATION")
        .ok()
        .map(|value| value.parse::<u16>().context("parse SOS_OUTPUT_ROTATION"))
        .transpose()?
        .or(file.rotation)
        .unwrap_or(0);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        bail!("output rotation must be 0, 90, 180, or 270");
    }
    let scale = std::env::var("SOS_OUTPUT_SCALE")
        .ok()
        .map(|value| value.parse::<f64>().context("parse SOS_OUTPUT_SCALE"))
        .transpose()?
        .or(file.scale)
        .unwrap_or(1.0);
    if !scale.is_finite() || !(1.0..=4.0).contains(&scale) {
        bail!("output scale must be finite and between 1.0 and 4.0");
    }
    Ok(OutputConfig {
        requested_size,
        rotation,
        scale,
    })
}

fn parse_output_mode(value: &str) -> Result<(i32, i32)> {
    let (width, height) = value
        .split_once('x')
        .context("output mode must be WIDTHxHEIGHT")?;
    let size = (
        width.parse::<i32>().context("parse output mode width")?,
        height.parse::<i32>().context("parse output mode height")?,
    );
    if size.0 <= 0 || size.1 <= 0 {
        bail!("output mode dimensions must be positive");
    }
    Ok(size)
}

fn update_output_layout(state: &mut SosCompositor) {
    let mut outputs = state.space.outputs().cloned().collect::<Vec<_>>();
    outputs.sort_by_key(Output::name);
    let mut x = 0;
    let mut height = 0;
    for output in outputs {
        state.space.map_output(&output, (x, 0));
        if let Some(geometry) = state.space.output_geometry(&output) {
            tracing::info!(
                output = output.name(),
                x,
                width = geometry.size.w,
                height = geometry.size.h,
                "positioned direct output"
            );
            x += geometry.size.w;
            height = height.max(geometry.size.h);
        }
    }
    state.output_size = (x, height);
    state.reconfigure_for_output_layout();
}

fn render_all(data: &mut CompositorData) {
    if data
        .direct
        .as_ref()
        .is_none_or(|direct| direct.session_paused)
    {
        return;
    }
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
    let renderer = Rc::clone(&device.renderer);
    let mut renderer = renderer.borrow_mut();
    let mut elements = if output_data.needs_initial_damage {
        let marker = MemoryRenderBufferRenderElement::from_buffer(
            &mut *renderer,
            (0.0, 0.0),
            &direct.initial_damage_buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .context("upload initial output damage marker")?;
        vec![DirectRenderElement::Cursor(marker)]
    } else {
        Vec::new()
    };
    elements.extend(cursor_render_elements(
        &mut renderer,
        state,
        &output_data.output,
        &direct.cursor_buffer,
    ));
    if state.policy.shell_mapped() {
        elements.extend(input_method_render_elements(
            &mut renderer,
            state,
            &output_data.output,
        ));
        elements.extend(
            space_render_elements(&mut *renderer, [&state.space], &output_data.output, 1.0)?
                .into_iter()
                .map(DirectRenderElement::Space),
        );
    } else if let Some(element) = recovery_render_element(&mut renderer, state, &output_data.output)
    {
        elements.push(DirectRenderElement::Cursor(element));
    }
    let result = output_data
        .drm_output
        .render_frame(&mut *renderer, &elements, CLEAR_COLOR, FrameFlags::DEFAULT)
        .map_err(|error| anyhow::anyhow!("prepare direct frame: {error}"))?;
    if output_data.needs_initial_damage {
        tracing::info!(
            output = output_data.output.name(),
            empty = result.is_empty,
            elements = elements.len(),
            "prepared initial direct output frame"
        );
    }
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
    output_data.needs_initial_damage = false;
    state.space.refresh();
    state.popups.cleanup();
    let _ = data.display_handle.flush_clients();
    Ok(())
}

fn recovery_render_element(
    renderer: &mut GlesRenderer,
    state: &SosCompositor,
    output: &Output,
) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
    let geometry = state.space.output_geometry(output)?;
    let buffer = state.recovery_ui.buffer();
    let location = (
        f64::from(geometry.loc.x + (geometry.size.w - crate::recovery::WIDTH) / 2),
        f64::from(geometry.loc.y + (geometry.size.h - crate::recovery::HEIGHT) / 2),
    );
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        location,
        &buffer,
        None,
        None,
        None,
        Kind::Unspecified,
    )
    .map_err(|error| tracing::warn!(%error, "could not upload recovery interface"))
    .ok()
}

fn input_method_render_elements(
    renderer: &mut GlesRenderer,
    state: &SosCompositor,
    output: &Output,
) -> Vec<DirectRenderElement> {
    let Some(output_geometry) = state.space.output_geometry(output) else {
        return Vec::new();
    };
    state
        .input_method_popups
        .iter()
        .filter(|popup| popup.alive())
        .flat_map(|popup| {
            let parent_location = popup
                .get_parent()
                .map(|parent| parent.location.loc)
                .unwrap_or_default();
            let location = parent_location + popup.location() - output_geometry.loc;
            render_elements_from_surface_tree(
                renderer,
                popup.wl_surface(),
                location.to_physical_precise_round(1.0),
                1.0,
                1.0,
                Kind::Unspecified,
            )
            .into_iter()
            .map(DirectRenderElement::CursorSurface)
        })
        .collect()
}

fn cursor_render_elements(
    renderer: &mut GlesRenderer,
    state: &SosCompositor,
    output: &Output,
    fallback: &MemoryRenderBuffer,
) -> Vec<DirectRenderElement> {
    let Some(output_geometry) = state.space.output_geometry(output) else {
        return Vec::new();
    };
    let pointer_location = state
        .seat
        .get_pointer()
        .expect("seat has a pointer")
        .current_location()
        - output_geometry.loc.to_f64();

    match &state.cursor_image {
        smithay::input::pointer::CursorImageStatus::Hidden => Vec::new(),
        smithay::input::pointer::CursorImageStatus::Surface(surface) if surface.is_alive() => {
            let hotspot = compositor::with_states(surface, |states| {
                states
                    .data_map
                    .get::<smithay::input::pointer::CursorImageSurfaceData>()
                    .map(|attributes| attributes.lock().unwrap().hotspot)
                    .unwrap_or_default()
            });
            let location = (
                pointer_location.x.round() as i32 - hotspot.x,
                pointer_location.y.round() as i32 - hotspot.y,
            );
            render_elements_from_surface_tree(renderer, surface, location, 1.0, 1.0, Kind::Cursor)
                .into_iter()
                .map(DirectRenderElement::CursorSurface)
                .collect()
        }
        _ => MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (pointer_location.x.round(), pointer_location.y.round()),
            fallback,
            None,
            None,
            None,
            Kind::Cursor,
        )
        .map(|element| vec![DirectRenderElement::Cursor(element)])
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not upload compositor cursor");
            Vec::new()
        }),
    }
}

fn default_cursor_buffer() -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        &default_cursor_pixels(),
        Fourcc::Argb8888,
        (CURSOR_WIDTH, CURSOR_HEIGHT),
        1,
        Transform::Normal,
        None,
    )
}

fn initial_damage_buffer() -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        &[8, 8, 8, 255],
        Fourcc::Argb8888,
        (1, 1),
        1,
        Transform::Normal,
        None,
    )
}

fn default_cursor_pixels() -> Vec<u8> {
    let mut pixels = vec![0_u8; (CURSOR_WIDTH * CURSOR_HEIGHT * 4) as usize];
    for y in 0..CURSOR_HEIGHT {
        for x in 0..CURSOR_WIDTH {
            let arrow_edge = y.min(17) / 2;
            let in_head = y <= 17 && x <= arrow_edge;
            let in_tail = (10..=22).contains(&y) && (5..=8).contains(&x);
            if !in_head && !in_tail {
                continue;
            }
            let border = x == 0
                || y == 0
                || (in_head && x == arrow_edge)
                || (in_tail && (x == 5 || x == 8 || y == 22));
            let color = if border { 0 } else { 255 };
            let offset = ((y * CURSOR_WIDTH + x) * 4) as usize;
            pixels[offset..offset + 4].copy_from_slice(&[color, color, color, 255]);
        }
    }
    pixels
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

#[cfg(test)]
mod tests {
    use smithay::backend::allocator::{format::FormatSet, Format, Fourcc, Modifier};

    use super::{common_dmabuf_formats, default_cursor_pixels, CURSOR_HEIGHT, CURSOR_WIDTH};

    #[test]
    fn fallback_cursor_has_a_stable_nonempty_extent() {
        let pixels = default_cursor_pixels();
        assert_eq!(pixels.len(), (CURSOR_WIDTH * CURSOR_HEIGHT * 4) as usize);
        let opaque = pixels.chunks_exact(4).filter(|pixel| pixel[3] != 0).count();
        assert!(opaque > 40);
        assert!(opaque < (CURSOR_WIDTH * CURSOR_HEIGHT) as usize);
    }

    #[test]
    fn dmabuf_feedback_advertises_only_formats_shared_by_every_renderer() {
        let linear_argb = Format {
            code: Fourcc::Argb8888,
            modifier: Modifier::Linear,
        };
        let implicit_argb = Format {
            code: Fourcc::Argb8888,
            modifier: Modifier::Invalid,
        };
        let linear_abgr = Format {
            code: Fourcc::Abgr8888,
            modifier: Modifier::Linear,
        };
        let first = [linear_argb, implicit_argb, linear_abgr]
            .into_iter()
            .collect::<FormatSet>();
        let second = [linear_argb, linear_abgr]
            .into_iter()
            .collect::<FormatSet>();
        let third = [linear_argb].into_iter().collect::<FormatSet>();

        let common = common_dmabuf_formats([first, second, third].into_iter());

        assert_eq!(common.iter().copied().collect::<Vec<_>>(), [linear_argb]);
    }
}
