// The Wayland state setup follows Smithay's MIT-licensed `smallvil` example at
// tag v0.7.0, reduced to the protocols and policy SOS exercises here.

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    io,
    os::unix::net::UnixDatagram,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(feature = "direct-backend")]
use std::{cell::RefCell, rc::Rc};

use compositor_control_protocol::{CompositorEvent, PresentationEvidence};
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
    utils::{Clock, Logical, Monotonic, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        input_method::{InputMethodManagerState, PopupSurface as InputMethodPopupSurface},
        output::OutputManagerState,
        presentation::PresentationState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
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
    policy::{ClientRole, SurfacePolicy},
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
    pub quiesced_keyboard_focus: Option<WlSurface>,
    pub suppressed_keyboard_keys: HashSet<Keycode>,
    pub pressed_pointer_buttons: HashSet<u32>,
    pub suppressed_pointer_buttons: HashSet<u32>,
    pub touch_lifecycle: TouchLifecycle,
    pub quiesced_input_events: u64,
    pub observed_input_classes: HashSet<&'static str>,
    pub cursor_image: CursorImageStatus,
    pub surface_roles: HashMap<ObjectId, ClientRole>,
    pub shell_events: Option<(u32, std::sync::mpsc::Sender<CompositorEvent>)>,
    pub output_size: (i32, i32),
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
            quiesced_keyboard_focus: None,
            suppressed_keyboard_keys: HashSet::new(),
            pressed_pointer_buttons: HashSet::new(),
            suppressed_pointer_buttons: HashSet::new(),
            touch_lifecycle: TouchLifecycle::default(),
            quiesced_input_events: 0,
            observed_input_classes: HashSet::new(),
            cursor_image: CursorImageStatus::default_named(),
            surface_roles: HashMap::new(),
            shell_events: None,
            output_size: (1280, 800),
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

    pub fn surface_under(
        &self,
        position: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(position)
            .and_then(|(window, location)| {
                window
                    .surface_under(position - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, offset)| (surface, (offset + location).to_f64()))
            })
    }

    pub fn client_role(surface: &WlSurface) -> Option<ClientRole> {
        surface
            .client()
            .and_then(|client| client.get_data::<ClientState>().map(|data| data.role))
    }
}

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

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, reason: DisconnectReason) {
        tracing::info!(pid = self.pid, role = ?self.role, ?reason, "Wayland client disconnected");
    }
}

#[cfg(test)]
mod tests {
    use super::notify_session_owner;
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
}
