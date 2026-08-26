// The Wayland state setup follows Smithay's MIT-licensed `smallvil` example at
// tag v0.7.0, reduced to the protocols and policy SOS exercises here.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsString,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    io,
    os::unix::net::UnixDatagram,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(feature = "direct-backend")]
use std::{cell::RefCell, rc::Rc};

use compositor_control_protocol::{
    CompositorEvent, PresentationEvidence, ShellOutputSnapshot, ShellOverlayConfiguration,
    ShellStateSnapshot, ShellWindowKind, ShellWindowSnapshot, WindowControlAction,
    WindowSpaceConfiguration, MAX_SHELL_OUTPUTS, MAX_SHELL_WINDOWS,
};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use smithay::{
    backend::renderer::element::RenderElementStates,
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{keyboard::Keycode, pointer::CursorImageStatus, Seat, SeatState},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason, ObjectId},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle, Resource as _,
        },
    },
    utils::{Clock, Logical, Monotonic, Point, SERIAL_COUNTER},
    wayland::{
        compositor::{with_states, CompositorClientState, CompositorState},
        input_method::{InputMethodManagerState, PopupSurface as InputMethodPopupSurface},
        output::OutputManagerState,
        presentation::PresentationState,
        seat::WaylandFocus as _,
        selection::data_device::DataDeviceState,
        shell::xdg::{XdgShellState, XdgToplevelSurfaceData},
        shm::ShmState,
        socket::ListeningSocketSource,
        tablet_manager::TabletManagerState,
        text_input::TextInputManagerState,
        xwayland_shell::XWaylandShellState,
    },
};

#[cfg(feature = "direct-backend")]
use smithay::{
    backend::{drm::DrmNode, renderer::gles::GlesRenderer},
    wayland::dmabuf::{DmabufGlobal, DmabufState},
};

use crate::{
    control::ControlCommand,
    input::TouchLifecycle,
    policy::{
        default_shell_overlay, default_window_space, validate_shell_overlay, validate_window_space,
        window_rectangles, ClientRole, SurfacePolicy, WindowRectangle,
    },
    recovery::RecoveryUi,
    CompositorData,
};

pub struct SosCompositor {
    pub clock: Clock<Monotonic>,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub space: Space<Window>,
    pub policy: SurfacePolicy,
    pub shell_surface: Option<WlSurface>,
    pub shell_overlay_surface: Option<WlSurface>,
    pub quiesced_keyboard_focus: Option<WlSurface>,
    pub suppressed_keyboard_keys: HashSet<Keycode>,
    pub pressed_pointer_buttons: HashSet<u32>,
    pub suppressed_pointer_buttons: HashSet<u32>,
    pub touch_lifecycle: TouchLifecycle,
    pub quiesced_input_events: u64,
    pub observed_input_classes: HashSet<&'static str>,
    pub input_output_mappings: BTreeMap<String, String>,
    pub observed_input_routes: HashSet<String>,
    pub cursor_image: CursorImageStatus,
    pub surface_roles: HashMap<ObjectId, ClientRole>,
    pub xdg_windows: HashMap<ObjectId, Window>,
    pub shell_events: Option<(u32, std::sync::mpsc::Sender<CompositorEvent>)>,
    pub output_size: (i32, i32),
    pub window_space: WindowSpaceConfiguration,
    pub shell_overlay: ShellOverlayConfiguration,
    pub shell_overlay_drag: Option<ShellOverlayDrag>,
    pub application_window_drag: Option<ApplicationWindowDrag>,
    pub shell_overlay_hovered: bool,
    pub output_layout_mirrored: bool,
    pub recovery_ui: RecoveryUi,
    pub recovery_button_pressed: bool,
    pub session_exit_enabled: bool,
    pub session_exit_requested: bool,
    pub session_exit_socket: Option<PathBuf>,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xwayland_shell_state: XWaylandShellState,
    pub xwm: Option<smithay::xwayland::X11Wm>,
    pub shm_state: ShmState,
    // These values own the advertised protocol globals for the compositor lifetime.
    #[allow(dead_code)]
    pub output_manager_state: OutputManagerState,
    #[allow(dead_code)]
    pub presentation_state: PresentationState,
    pub seat_state: SeatState<SosCompositor>,
    pub data_device_state: DataDeviceState,
    #[allow(dead_code)]
    pub tablet_manager_state: TabletManagerState,
    #[allow(dead_code)]
    pub input_method_manager_state: InputMethodManagerState,
    pub input_method_popups: Vec<InputMethodPopupSurface>,
    #[allow(dead_code)]
    pub text_input_manager_state: TextInputManagerState,
    pub popups: PopupManager,
    pub seat: Seat<Self>,

    #[cfg(feature = "direct-backend")]
    pub dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
    #[cfg(feature = "direct-backend")]
    pub dmabuf_renderers: HashMap<DrmNode, Rc<RefCell<GlesRenderer>>>,
    #[cfg(feature = "direct-backend")]
    pub dmabuf_render_nodes: HashMap<DrmNode, DrmNode>,
    #[cfg(feature = "direct-backend")]
    pub dmabuf_active_devices: HashSet<DrmNode>,
    #[cfg(feature = "direct-backend")]
    pub dmabuf_primary: Option<DrmNode>,
}

fn opaque_window_id(window: &Window) -> String {
    if let Some(surface) = window.x11_surface() {
        opaque_id("window", &surface.window_id())
    } else if let Some(surface) = window.wl_surface() {
        opaque_id("window", &surface.id())
    } else {
        opaque_id("window", &format!("{:?}", window.geometry()))
    }
}

fn opaque_id(value_kind: &str, value: &impl std::hash::Hash) -> String {
    let mut hasher = DefaultHasher::new();
    value_kind.hash(&mut hasher);
    value.hash(&mut hasher);
    format!("{value_kind}-{:016x}", hasher.finish())
}

fn bounded_title(title: &str) -> String {
    let title = title.trim();
    let title = if title.is_empty() {
        "Application"
    } else {
        title
    };
    if title.len() <= 256 {
        return title.to_owned();
    }
    let mut boundary = 256;
    while !title.is_char_boundary(boundary) {
        boundary -= 1;
    }
    title[..boundary].to_owned()
}

impl SosCompositor {
    pub fn new(
        event_loop: &mut EventLoop<CompositorData>,
        display: Display<Self>,
        socket_name: &str,
    ) -> anyhow::Result<Self> {
        let clock = Clock::<Monotonic>::new();
        let display_handle = display.handle();
        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let xwayland_shell_state = XWaylandShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let presentation_state = PresentationState::new::<Self>(&display_handle, clock.id() as u32);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let tablet_manager_state = TabletManagerState::new::<Self>(&display_handle);
        let input_method_manager_state =
            InputMethodManagerState::new::<Self, _>(&display_handle, |client| {
                let Some(data) = client.get_data::<ClientState>() else {
                    return false;
                };
                std::fs::read_link(format!("/proc/{}/exe", data.pid))
                    .ok()
                    .and_then(|path| path.file_name().map(|name| name.to_owned()))
                    .is_some_and(|name| name == "sos-input-method")
            });
        let text_input_manager_state = TextInputManagerState::new::<Self>(&display_handle);
        let popups = PopupManager::default();
        let mut seat = seat_state.new_wl_seat(&display_handle, "sos-nested");
        seat.add_keyboard(Default::default(), 200, 25)?;
        seat.add_pointer();
        seat.add_touch();
        let socket_name =
            Self::init_wayland_listener(display, event_loop, socket_name, display_handle.clone())?;

        Ok(Self {
            clock,
            socket_name,
            display_handle,
            space: Space::default(),
            policy: SurfacePolicy::default(),
            shell_surface: None,
            shell_overlay_surface: None,
            quiesced_keyboard_focus: None,
            suppressed_keyboard_keys: HashSet::new(),
            pressed_pointer_buttons: HashSet::new(),
            suppressed_pointer_buttons: HashSet::new(),
            touch_lifecycle: TouchLifecycle::default(),
            quiesced_input_events: 0,
            observed_input_classes: HashSet::new(),
            input_output_mappings: BTreeMap::new(),
            observed_input_routes: HashSet::new(),
            cursor_image: CursorImageStatus::default_named(),
            surface_roles: HashMap::new(),
            xdg_windows: HashMap::new(),
            shell_events: None,
            output_size: (1280, 800),
            window_space: default_window_space((1280, 800)),
            shell_overlay: default_shell_overlay((1280, 800)),
            shell_overlay_drag: None,
            application_window_drag: None,
            shell_overlay_hovered: false,
            output_layout_mirrored: false,
            recovery_ui: RecoveryUi::from_environment(),
            recovery_button_pressed: false,
            session_exit_enabled: std::env::var("SOS_ALLOW_SESSION_EXIT").as_deref() == Ok("1"),
            session_exit_requested: false,
            session_exit_socket: std::env::var_os("SOS_SESSION_EXIT_SOCKET").map(PathBuf::from),
            compositor_state,
            xdg_shell_state,
            xwayland_shell_state,
            xwm: None,
            shm_state,
            output_manager_state,
            presentation_state,
            seat_state,
            data_device_state,
            tablet_manager_state,
            input_method_manager_state,
            input_method_popups: Vec::new(),
            text_input_manager_state,
            popups,
            seat,
            #[cfg(feature = "direct-backend")]
            dmabuf_state: None,
            #[cfg(feature = "direct-backend")]
            dmabuf_renderers: HashMap::new(),
            #[cfg(feature = "direct-backend")]
            dmabuf_render_nodes: HashMap::new(),
            #[cfg(feature = "direct-backend")]
            dmabuf_active_devices: HashSet::new(),
            #[cfg(feature = "direct-backend")]
            dmabuf_primary: None,
        })
    }

    pub fn take_session_exit_request(&mut self) -> bool {
        std::mem::take(&mut self.session_exit_requested)
    }

    pub fn handoff_session_exit_request(&mut self) -> bool {
        if !self.take_session_exit_request() {
            return false;
        }
        let Some(socket) = self.session_exit_socket.as_deref() else {
            return true;
        };
        match notify_session_owner(socket) {
            Ok(()) => {
                tracing::info!(socket = %socket.display(), "handed logout request to session owner");
                false
            }
            Err(error) => {
                tracing::warn!(%error, socket = %socket.display(), "could not hand logout request to session owner");
                true
            }
        }
    }

    fn init_wayland_listener(
        display: Display<SosCompositor>,
        event_loop: &mut EventLoop<CompositorData>,
        socket_name: &str,
        mut display_handle: DisplayHandle,
    ) -> anyhow::Result<OsString> {
        let listening_socket = ListeningSocketSource::with_name(socket_name)?;
        let socket_name = listening_socket.socket_name().to_os_string();
        let loop_handle = event_loop.handle();
        loop_handle
            .insert_source(listening_socket, move |client_stream, _, data| {
                let credentials = match getsockopt(&client_stream, PeerCredentials) {
                    Ok(credentials) => credentials,
                    Err(error) => {
                        tracing::warn!(error = %error, "could not read Wayland peer credentials");
                        return;
                    }
                };
                let Ok(pid) = u32::try_from(credentials.pid()) else {
                    tracing::warn!(pid = credentials.pid(), "Wayland peer PID is invalid");
                    return;
                };
                let role = data.state.policy.classify(pid);
                let client_data = Arc::new(ClientState {
                    compositor_state: CompositorClientState::default(),
                    pid,
                    role,
                });
                match display_handle.insert_client(client_stream, client_data) {
                    Ok(_) => tracing::info!(pid, ?role, "accepted Wayland client"),
                    Err(error) => {
                        tracing::warn!(pid, error = %error, "could not insert Wayland client")
                    }
                }
            })
            .map_err(|_| anyhow::anyhow!("insert Wayland listening socket source"))?;
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, data| {
                    // SAFETY: the display remains owned by this event source.
                    unsafe {
                        display.get_mut().dispatch_clients(&mut data.state)?;
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|_| anyhow::anyhow!("insert Wayland display source"))?;
        Ok(socket_name)
    }

    pub fn handle_control(&mut self, command: ControlCommand) {
        match command {
            ControlCommand::Register { pid, events, reply } => {
                let result = self
                    .policy
                    .register_shell(pid)
                    .map_err(|error| error.to_string());
                if result.is_ok() {
                    self.shell_events = Some((pid, events));
                }
                let _ = reply.send(result);
                self.publish_shell_state();
            }
            ControlCommand::Arm {
                pid,
                request_id,
                revision_id,
                reply,
            } => {
                let result = self
                    .policy
                    .arm(pid, request_id, revision_id.clone())
                    .map_err(|error| error.to_string());
                if let Ok(after_commit_sequence) = result {
                    tracing::info!(
                        pid,
                        request_id,
                        revision_id,
                        after_commit_sequence,
                        "armed shell presentation fence"
                    );
                    let _ = reply.send(Ok(after_commit_sequence));
                } else {
                    let _ = reply.send(result);
                }
            }
            ControlCommand::QuiesceInput {
                pid,
                request_id,
                revision_id,
                reply,
            } => {
                let result = self
                    .policy
                    .quiesce_input(pid, revision_id.clone())
                    .map_err(|error| error.to_string());
                match result {
                    Ok(changed) => {
                        if changed {
                            self.begin_input_quiesce();
                        }
                        tracing::info!(
                            pid,
                            request_id,
                            revision_id,
                            changed,
                            "quiesced compositor input for revision"
                        );
                        let _ = reply.send(Ok(changed));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            ControlCommand::ResumeInput {
                pid,
                request_id,
                revision_id,
                reply,
            } => {
                let result = self
                    .policy
                    .resume_input(pid, &revision_id)
                    .map_err(|error| error.to_string());
                match result {
                    Ok(changed) => {
                        if changed {
                            self.end_input_quiesce(true);
                        }
                        tracing::info!(
                            pid,
                            request_id,
                            revision_id,
                            changed,
                            "resumed compositor input after revision abort"
                        );
                        let _ = reply.send(Ok(changed));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            ControlCommand::ConfigureWindowSpace {
                pid,
                request_id,
                configuration,
                reply,
            } => {
                let result = if !self.policy.is_shell_owner(pid) {
                    Err("window space is not owned by the registered shell".into())
                } else {
                    validate_window_space(configuration, self.output_size)
                        .map_err(|error| error.to_string())
                };
                match result {
                    Ok(configuration) => {
                        let layout_changed = self.window_space.layout != configuration.layout;
                        self.window_space = configuration;
                        if layout_changed {
                            self.reset_application_window_layout();
                        } else {
                            self.reconfigure_application_windows();
                        }
                        tracing::info!(
                            pid,
                            request_id,
                            x = configuration.geometry.x,
                            y = configuration.geometry.y,
                            width = configuration.geometry.width,
                            height = configuration.geometry.height,
                            gap = configuration.geometry.gap,
                            layout = ?configuration.layout,
                            "configured shell window space"
                        );
                        let _ = reply.send(Ok(configuration));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            ControlCommand::ConfigureShellOverlay {
                pid,
                request_id,
                configuration,
                reply,
            } => {
                let result = if !self.policy.is_shell_owner(pid) {
                    Err("shell overlay is not owned by the registered shell".into())
                } else {
                    validate_shell_overlay(configuration, self.output_size)
                        .map_err(|error| error.to_string())
                };
                match result {
                    Ok(configuration) => {
                        self.shell_overlay = configuration;
                        if let Some(drag) = &mut self.shell_overlay_drag {
                            drag.origin = (configuration.x, configuration.y);
                            drag.pointer = self
                                .seat
                                .get_pointer()
                                .expect("seat has a pointer")
                                .current_location();
                        }
                        self.apply_shell_overlay_configuration();
                        let pointer = self
                            .seat
                            .get_pointer()
                            .expect("seat has a pointer")
                            .current_location();
                        self.synchronize_shell_overlay_hover(pointer, false);
                        tracing::info!(
                            pid,
                            request_id,
                            x = configuration.x,
                            y = configuration.y,
                            width = configuration.width,
                            height = configuration.height,
                            "configured trusted shell overlay"
                        );
                        let _ = reply.send(Ok(configuration));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            ControlCommand::ControlWindow {
                pid,
                request_id,
                window_id,
                operation,
                reply,
            } => {
                let result = if !self.policy.is_shell_owner(pid) {
                    Err("window control is not owned by the registered shell".into())
                } else {
                    self.control_window(&window_id, operation)
                };
                if result.is_ok() {
                    tracing::info!(
                        pid,
                        request_id,
                        window_id,
                        ?operation,
                        "controlled application window"
                    );
                    self.publish_shell_state();
                }
                let _ = reply.send(result);
            }
            ControlCommand::Disconnected { pid } => {
                let was_quiesced = self.policy.input_quiesced();
                self.policy.unregister_shell(pid);
                if was_quiesced {
                    self.end_input_quiesce(false);
                }
                if self.shell_events.as_ref().map(|(owner, _)| *owner) == Some(pid) {
                    self.shell_events = None;
                }
                tracing::info!(pid, "shell control connection closed");
            }
        }
    }

    pub(crate) fn publish_shell_state(&self) {
        if let Some((_, events)) = &self.shell_events {
            let _ = events.send(CompositorEvent::ShellStateChanged {
                request_id: 0,
                state: self.shell_state_snapshot(),
            });
        }
    }

    fn shell_state_snapshot(&self) -> ShellStateSnapshot {
        let mut outputs = self.space.outputs().cloned().collect::<Vec<_>>();
        outputs.sort_by_key(|output| output.name());
        let outputs = outputs
            .into_iter()
            .take(MAX_SHELL_OUTPUTS)
            .filter_map(|output| {
                let geometry = self.space.output_geometry(&output)?;
                Some(ShellOutputSnapshot {
                    id: opaque_id("output", &output.name()),
                    x: geometry.loc.x,
                    y: geometry.loc.y,
                    width: u32::try_from(geometry.size.w.max(0)).unwrap_or_default(),
                    height: u32::try_from(geometry.size.h.max(0)).unwrap_or_default(),
                    scale_milli: (output.current_scale().fractional_scale() * 1000.0)
                        .round()
                        .clamp(1.0, f64::from(u32::MAX)) as u32,
                    primary: false,
                })
            })
            .collect::<Vec<_>>();
        let mut outputs = outputs;
        if let Some(primary) = outputs.first_mut() {
            primary.primary = true;
        }
        let focused = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus());
        let windows = self
            .space
            .elements()
            .filter(|window| self.is_application_window(window))
            .take(MAX_SHELL_WINDOWS)
            .map(|window| {
                let (title, kind) = if let Some(surface) = window.x11_surface() {
                    (surface.title(), ShellWindowKind::Compatibility)
                } else {
                    let title = window
                        .wl_surface()
                        .and_then(|surface| {
                            with_states(surface.as_ref(), |states| {
                                states
                                    .data_map
                                    .get::<XdgToplevelSurfaceData>()
                                    .and_then(|state| state.lock().ok()?.title.clone())
                            })
                        })
                        .unwrap_or_else(|| "Application".into());
                    let kind = window
                        .wl_surface()
                        .and_then(|surface| Self::client_role(&surface))
                        .map_or(ShellWindowKind::Compatibility, |role| match role {
                            ClientRole::NativeApplication => ShellWindowKind::Native,
                            _ => ShellWindowKind::Compatibility,
                        });
                    (title, kind)
                };
                let active = window.wl_surface().is_some_and(|surface| {
                    focused
                        .as_ref()
                        .is_some_and(|focused| focused == surface.as_ref())
                });
                ShellWindowSnapshot {
                    id: opaque_window_id(window),
                    title: bounded_title(&title),
                    kind,
                    active,
                    can_focus: window.wl_surface().is_some(),
                    can_close: window.toplevel().is_some() || window.x11_surface().is_some(),
                }
            })
            .collect();
        ShellStateSnapshot {
            canvas_width: u32::try_from(self.output_size.0.max(0)).unwrap_or_default(),
            canvas_height: u32::try_from(self.output_size.1.max(0)).unwrap_or_default(),
            mirrored: self.output_layout_mirrored,
            outputs,
            windows,
        }
    }

    fn control_window(
        &mut self,
        window_id: &str,
        operation: WindowControlAction,
    ) -> std::result::Result<(), String> {
        let window = self
            .space
            .elements()
            .find(|window| {
                self.is_application_window(window) && opaque_window_id(window) == window_id
            })
            .cloned()
            .ok_or_else(|| "opaque window selection is stale or unknown".to_owned())?;
        match operation {
            WindowControlAction::Focus => {
                let surface = window
                    .wl_surface()
                    .map(|surface| surface.into_owned())
                    .ok_or_else(|| "window cannot receive keyboard focus".to_owned())?;
                self.space.raise_element(&window, false);
                self.space.elements().for_each(|candidate| {
                    candidate.set_activated(candidate == &window);
                    if let Some(toplevel) = candidate.toplevel() {
                        toplevel.send_pending_configure();
                    }
                });
                let keyboard = self
                    .seat
                    .get_keyboard()
                    .ok_or_else(|| "compositor keyboard is unavailable".to_owned())?;
                keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
            }
            WindowControlAction::Close => {
                if let Some(toplevel) = window.toplevel() {
                    toplevel.send_close();
                } else if let Some(surface) = window.x11_surface() {
                    surface.close().map_err(|error| error.to_string())?;
                } else {
                    return Err("window cannot be closed".into());
                }
            }
        }
        Ok(())
    }

    pub fn shell_rendered(&self, states: &RenderElementStates) -> bool {
        self.shell_surface
            .as_ref()
            .is_some_and(|surface| states.element_was_presented(surface))
    }

    pub fn publish_successful_submit(&mut self, shell_rendered: bool) {
        let Some(presented) = self.policy.record_successful_submit(shell_rendered) else {
            return;
        };
        self.publish_presentation(presented, PresentationEvidence::NestedBackendSubmit);
    }

    pub fn publish_presentation(
        &mut self,
        presented: crate::policy::QueuedRevision,
        evidence: PresentationEvidence,
    ) {
        self.end_input_quiesce(true);
        let event = CompositorEvent::Presented {
            request_id: presented.request_id,
            revision_id: presented.revision_id.clone(),
            commit_sequence: presented.commit_sequence,
            submit_sequence: presented.submit_sequence,
            evidence: evidence.clone(),
        };
        if let Some((_, events)) = &self.shell_events {
            if events.send(event).is_ok() {
                tracing::info!(
                    request_id = presented.request_id,
                    revision_id = presented.revision_id,
                    commit_sequence = presented.commit_sequence,
                    submit_sequence = presented.submit_sequence,
                    evidence = evidence.name(),
                    "presented armed shell revision"
                );
            }
        }
    }

    pub(crate) fn update_shell_overlay_drag(&mut self, pointer: Point<f64, Logical>) {
        let Some(drag) = self.shell_overlay_drag else {
            return;
        };
        let max_x = (self.output_size.0
            - i32::try_from(self.shell_overlay.width).unwrap_or(i32::MAX))
        .max(0);
        let max_y = (self.output_size.1
            - i32::try_from(self.shell_overlay.height).unwrap_or(i32::MAX))
        .max(0);
        self.shell_overlay.x =
            (drag.origin.0 + (pointer.x - drag.pointer.x).round() as i32).clamp(0, max_x);
        self.shell_overlay.y =
            (drag.origin.1 + (pointer.y - drag.pointer.y).round() as i32).clamp(0, max_y);
        let window = self
            .space
            .elements()
            .find(|window| {
                window
                    .wl_surface()
                    .and_then(|surface| Self::client_role(&surface))
                    == Some(ClientRole::ShellOverlay)
            })
            .cloned();
        if let Some(window) = window {
            self.space
                .map_element(window, (self.shell_overlay.x, self.shell_overlay.y), false);
        }
    }

    pub(crate) fn finish_shell_overlay_drag(&mut self) {
        let Some(drag) = self.shell_overlay_drag.take() else {
            return;
        };
        let moved = (self.shell_overlay.x, self.shell_overlay.y) != drag.origin;
        if let Some((_, events)) = &self.shell_events {
            let event = if moved {
                CompositorEvent::ShellOverlayMoved {
                    request_id: 0,
                    configuration: self.shell_overlay,
                }
            } else {
                CompositorEvent::ShellOverlayActivated { request_id: 0 }
            };
            let _ = events.send(event);
        }
        tracing::info!(
            x = self.shell_overlay.x,
            y = self.shell_overlay.y,
            moved,
            "completed trusted shell overlay move"
        );
        let pointer = self
            .seat
            .get_pointer()
            .expect("seat has a pointer")
            .current_location();
        self.synchronize_shell_overlay_hover(pointer, false);
    }

    pub(crate) fn update_application_window_drag(&mut self, pointer: Point<f64, Logical>) {
        let Some(drag) = self.application_window_drag.as_ref() else {
            return;
        };
        if self.window_space.layout != compositor_control_protocol::WindowLayoutMode::Floating {
            self.application_window_drag = None;
            return;
        }
        let window = drag.window.clone();
        let origin = drag.origin;
        let pointer_origin = drag.pointer;
        let Some(rectangle) = self
            .application_window_rectangles()
            .into_iter()
            .find_map(|(candidate, rectangle)| (candidate == window).then_some(rectangle))
        else {
            self.application_window_drag = None;
            return;
        };
        let geometry = self.window_space.geometry;
        let right = geometry
            .x
            .saturating_add(i32::try_from(geometry.width).unwrap_or(i32::MAX));
        let bottom = geometry
            .y
            .saturating_add(i32::try_from(geometry.height).unwrap_or(i32::MAX));
        let max_x = right.saturating_sub(rectangle.width).max(geometry.x);
        let max_y = bottom.saturating_sub(rectangle.height).max(geometry.y);
        let x = (origin.x + (pointer.x - pointer_origin.x).round() as i32).clamp(geometry.x, max_x);
        let y = (origin.y + (pointer.y - pointer_origin.y).round() as i32).clamp(geometry.y, max_y);
        self.space.map_element(window, (x, y), false);
    }

    pub(crate) fn finish_application_window_drag(&mut self) {
        let Some(drag) = self.application_window_drag.take() else {
            return;
        };
        let location = self
            .space
            .element_location(&drag.window)
            .unwrap_or(drag.origin);
        tracing::info!(
            x = location.x,
            y = location.y,
            moved = location != drag.origin,
            "completed compositor-managed application move"
        );
    }

    pub(crate) fn update_shell_overlay_hover(&mut self, pointer: Point<f64, Logical>) {
        self.synchronize_shell_overlay_hover(pointer, false);
    }

    pub(crate) fn synchronize_shell_overlay_hover(
        &mut self,
        pointer: Point<f64, Logical>,
        force: bool,
    ) {
        if self.shell_overlay_drag.is_some() && !force {
            return;
        }
        let mapped = self.space.elements().any(|window| {
            window
                .wl_surface()
                .and_then(|surface| Self::client_role(&surface))
                == Some(ClientRole::ShellOverlay)
        });
        let right = f64::from(self.shell_overlay.x) + f64::from(self.shell_overlay.width);
        let bottom = f64::from(self.shell_overlay.y) + f64::from(self.shell_overlay.height);
        let hovered = mapped
            && pointer.x >= f64::from(self.shell_overlay.x)
            && pointer.y >= f64::from(self.shell_overlay.y)
            && pointer.x < right
            && pointer.y < bottom;
        if !force && hovered == self.shell_overlay_hovered {
            return;
        }
        self.shell_overlay_hovered = hovered;
        if let Some((_, events)) = &self.shell_events {
            let _ = events.send(CompositorEvent::ShellOverlayHoverChanged {
                request_id: 0,
                hovered,
            });
        }
        tracing::info!(hovered, "changed trusted shell overlay hover state");
    }

    pub fn surface_under(
        &self,
        position: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.window_under(position).and_then(|(window, location)| {
            window
                .surface_under(position - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(surface, offset)| (surface, (offset + location).to_f64()))
        })
    }

    pub(crate) fn application_window_rectangles(&self) -> Vec<(Window, WindowRectangle)> {
        let windows = self.application_windows_in_layout_order();
        window_rectangles(self.window_space, windows.len())
            .into_iter()
            .zip(windows)
            .map(|(mut rectangle, window)| {
                if self.window_space.layout
                    == compositor_control_protocol::WindowLayoutMode::Floating
                {
                    if let Some(location) = self.space.element_location(&window) {
                        rectangle.x = location.x;
                        rectangle.y = location.y;
                    }
                }
                (window, rectangle)
            })
            .collect()
    }

    pub(crate) fn application_windows_in_layout_order(&self) -> Vec<Window> {
        let mut windows = self
            .space
            .elements()
            .filter(|window| self.is_application_window(window))
            .cloned()
            .collect::<Vec<_>>();
        if self.window_space.layout != compositor_control_protocol::WindowLayoutMode::Floating {
            // Space order is stacking order and changes whenever a window is
            // focused. Managed geometry must instead retain its spatial order;
            // otherwise raising a tile reassigns clipping rectangles before a
            // relayout and makes mapped clients appear to vanish until the next
            // shell configuration.
            windows.sort_by_key(|window| {
                application_layout_order_key(self.space.element_location(window))
            });
        }
        windows
    }

    pub(crate) fn window_under(
        &self,
        position: Point<f64, Logical>,
    ) -> Option<(&Window, Point<i32, Logical>)> {
        let application_rectangles = self.application_window_rectangles();
        let mut windows = self.space.elements().rev().collect::<Vec<_>>();
        windows.sort_by_key(|window| {
            match window
                .wl_surface()
                .and_then(|surface| Self::client_role(&surface))
            {
                Some(ClientRole::ShellOverlay) => 0,
                Some(ClientRole::NativeApplication) => 1,
                Some(ClientRole::Compatibility) => 1,
                Some(ClientRole::Shell) => 2,
                None if window.is_x11() => 1,
                None => 2,
            }
        });
        windows.into_iter().find_map(|window| {
            if self.is_application_window(window) {
                let rectangle =
                    application_rectangles
                        .iter()
                        .find_map(|(candidate, rectangle)| {
                            (candidate == window).then_some(rectangle)
                        })?;
                let left = f64::from(rectangle.x);
                let top = f64::from(rectangle.y);
                let right = left + f64::from(rectangle.width);
                let bottom = top + f64::from(rectangle.height);
                if position.x < left
                    || position.y < top
                    || position.x >= right
                    || position.y >= bottom
                {
                    return None;
                }
            }
            let mapped_location = self.space.element_location(window)?;
            // Space locations describe the client's xdg_window_geometry,
            // while surface trees and rendering start at the buffer origin.
            // Client-side decorations commonly inset window geometry (GTK
            // currently uses 20 logical pixels), so use the same render origin
            // for input that the renderer uses for pixels.
            let location = window_render_location(mapped_location, window.geometry().loc);
            window
                .surface_under(position - location.to_f64(), WindowSurfaceType::ALL)
                .is_some()
                .then_some((window, location))
        })
    }

    pub(crate) fn is_application_window(&self, window: &Window) -> bool {
        window.is_x11()
            || window
                .wl_surface()
                .and_then(|surface| Self::client_role(&surface))
                .is_some_and(|role| {
                    matches!(
                        role,
                        ClientRole::NativeApplication | ClientRole::Compatibility
                    )
                })
    }

    pub fn client_role(surface: &WlSurface) -> Option<ClientRole> {
        with_states(surface, |states| {
            states
                .data_map
                .get::<SurfaceRoleData>()
                .copied()
                .map(|role| role.0)
        })
        .or_else(|| {
            surface
                .client()
                .and_then(|client| client.get_data::<ClientState>().map(|data| data.role))
        })
    }

    pub fn client_pid(surface: &WlSurface) -> Option<u32> {
        surface
            .client()
            .and_then(|client| client.get_data::<ClientState>().map(|data| data.pid))
    }
}

fn application_layout_order_key(location: Option<Point<i32, Logical>>) -> (bool, i32, i32) {
    location
        .map(|location| (false, location.y, location.x))
        .unwrap_or((true, 0, 0))
}

fn window_render_location(
    mapped_location: Point<i32, Logical>,
    geometry_location: Point<i32, Logical>,
) -> Point<i32, Logical> {
    mapped_location - geometry_location
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceRoleData(pub ClientRole);

fn notify_session_owner(path: &Path) -> io::Result<()> {
    let socket = UnixDatagram::unbound()?;
    socket.send_to(b"logout\n", path)?;
    Ok(())
}

pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub pid: u32,
    pub role: ClientRole,
}

#[derive(Clone, Copy, Debug)]
pub struct ShellOverlayDrag {
    pub pointer: Point<f64, Logical>,
    pub origin: (i32, i32),
}

pub struct ApplicationWindowDrag {
    pub window: Window,
    pub pointer: Point<f64, Logical>,
    pub origin: Point<i32, Logical>,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, reason: DisconnectReason) {
        tracing::info!(pid = self.pid, role = ?self.role, ?reason, "Wayland client disconnected");
    }
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point};

    use super::{
        application_layout_order_key, bounded_title, notify_session_owner, opaque_id,
        window_render_location,
    };
    use std::os::unix::net::UnixDatagram;

    #[test]
    fn session_exit_notification_reaches_the_lifecycle_owner() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("logout.sock");
        let receiver = UnixDatagram::bind(&path).unwrap();

        notify_session_owner(&path).unwrap();

        let mut buffer = [0_u8; 16];
        let size = receiver.recv(&mut buffer).unwrap();
        assert_eq!(&buffer[..size], b"logout\n");
    }

    #[test]
    fn managed_application_order_follows_geometry_not_stacking_order() {
        let mut locations = [
            ("raised", Some(Point::<i32, Logical>::from((500, 60)))),
            ("master", Some(Point::<i32, Logical>::from((20, 60)))),
            ("lower", Some(Point::<i32, Logical>::from((500, 400)))),
            ("unmapped", None),
        ];

        locations.sort_by_key(|(_, location)| application_layout_order_key(*location));

        assert_eq!(
            locations.map(|(name, _)| name),
            ["master", "raised", "lower", "unmapped"]
        );
    }

    #[test]
    fn client_side_decoration_hit_testing_uses_the_render_origin() {
        let mapped = Point::<i32, Logical>::from((771, 635));
        let geometry = Point::<i32, Logical>::from((20, 20));

        assert_eq!(window_render_location(mapped, geometry), (751, 615).into());
    }

    #[test]
    fn shell_observation_strings_are_bounded_and_opaque() {
        assert_eq!(bounded_title("  "), "Application");
        let title = "界".repeat(100);
        let bounded = bounded_title(&title);
        assert!(bounded.len() <= 256);
        assert!(bounded.is_char_boundary(bounded.len()));

        let first = opaque_id("window", &42_u64);
        let second = opaque_id("window", &42_u64);
        assert_eq!(first, second);
        assert!(first.starts_with("window-"));
        assert!(!first.contains("42"));
    }
}
