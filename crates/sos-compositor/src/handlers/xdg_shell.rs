use smithay::{
    delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, PopupKind, PopupManager, Space, Window,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            backend::DisconnectReason,
            protocol::{wl_seat, wl_surface::WlSurface},
            Resource as _,
        },
    },
    utils::{Logical, Serial, Size},
    wayland::{
        compositor::with_states,
        seat::WaylandFocus as _,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    policy::{compatibility_location, ClientRole},
    state::SosCompositor,
};

const COMPATIBILITY_SIZE: (i32, i32) = (720, 520);

impl XdgShellHandler for SosCompositor {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();
        let role = Self::client_role(&wl_surface).unwrap_or(ClientRole::Compatibility);
        if role == ClientRole::Shell {
            let stale_shell = self
                .shell_surface
                .as_ref()
                .filter(|current| *current != &wl_surface)
                .cloned();
            if let Some(stale_shell) = stale_shell {
                let stale_window = self
                    .space
                    .elements()
                    .find(|window| window.wl_surface().as_deref() == Some(&stale_shell))
                    .cloned();
                if let Some(stale_window) = stale_window {
                    self.space.unmap_elem(&stale_window);
                }
                self.surface_roles.remove(&stale_shell.id());
                self.shell_surface = None;
                self.policy.unmap(ClientRole::Shell);
                tracing::info!("replaced stale SOS shell surface");
            }
        }
        if let Err(error) = self.policy.map(role) {
            tracing::warn!(?role, error = %error, "rejected toplevel by SOS surface policy");
            if let Some(client) = wl_surface.client() {
                self.display_handle
                    .backend_handle()
                    .kill_client(client.id(), DisconnectReason::ConnectionClosed);
            }
            return;
        }
        self.surface_roles.insert(wl_surface.id(), role);

        let output_size: Size<i32, Logical> = self.output_size.into();
        let location = match role {
            ClientRole::Shell => {
                surface.with_pending_state(|state| {
                    state.size = Some(output_size);
                    state.states.set(xdg_toplevel::State::Fullscreen);
                });
                self.shell_surface = Some(wl_surface.clone());
                (0, 0)
            }
            ClientRole::Compatibility => {
                let size: Size<i32, Logical> = COMPATIBILITY_SIZE.into();
                surface.with_pending_state(|state| state.size = Some(size));
                compatibility_location(self.output_size, COMPATIBILITY_SIZE)
            }
        };

        let window = Window::new_wayland_window(surface);
        if role == ClientRole::Shell {
            let compatibility = self
                .space
                .elements()
                .filter(|window| {
                    window
                        .wl_surface()
                        .and_then(|surface| Self::client_role(&surface))
                        == Some(ClientRole::Compatibility)
                })
                .filter_map(|window| {
                    self.space
                        .element_location(window)
                        .map(|location| (window.clone(), location))
                })
                .collect::<Vec<_>>();
            for (compatibility, _) in &compatibility {
                self.space.unmap_elem(compatibility);
            }
            self.space.map_element(window, location, false);
            for (compatibility, location) in compatibility {
                self.space.map_element(compatibility, location, false);
            }
        } else {
            self.space.map_element(window, location, false);
        }
        tracing::info!(
            ?role,
            x = location.0,
            y = location.1,
            "mapped fixed-policy XDG toplevel"
        );
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let Some(role) = self.surface_roles.remove(&surface.wl_surface().id()) else {
            tracing::debug!("ignored destruction of an unmapped XDG toplevel");
            return;
        };
        let window = self
            .space
            .elements()
            .find(|window| {
                window
                    .toplevel()
                    .is_some_and(|toplevel| toplevel == &surface)
            })
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window);
        }
        if role == ClientRole::Shell && self.shell_surface.as_ref() == Some(surface.wl_surface()) {
            self.shell_surface = None;
        }
        self.policy.unmap(role);
        tracing::info!(?role, "unmapped XDG toplevel");
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        self.apply_fixed_size(&surface);
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        self.apply_fixed_size(&surface);
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        self.apply_fixed_size(&surface);
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        self.apply_fixed_size(&surface);
    }
}

delegate_xdg_shell!(SosCompositor);

pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    if let Some(window) = space.elements().find(|window| {
        window
            .toplevel()
            .is_some_and(|top| top.wl_surface() == surface)
    }) {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .expect("XDG toplevel state exists")
                .lock()
                .expect("XDG toplevel state is not poisoned")
                .initial_configure_sent
        });
        if !initial_configure_sent {
            window
                .toplevel()
                .expect("mapped window is XDG")
                .send_configure();
        }
    }

    popups.commit(surface);
    if let Some(PopupKind::Xdg(popup)) = popups.find_popup(surface) {
        if !popup.is_initial_configure_sent() {
            let _ = popup.send_configure();
        }
    }
}

impl SosCompositor {
    #[cfg(feature = "direct-backend")]
    pub(crate) fn reconfigure_for_output_layout(&mut self) {
        let windows = self.space.elements().cloned().collect::<Vec<_>>();
        for window in windows {
            let Some(toplevel) = window.toplevel().cloned() else {
                continue;
            };
            let role =
                Self::client_role(toplevel.wl_surface()).unwrap_or(ClientRole::Compatibility);
            self.apply_fixed_size(&toplevel);
            if role == ClientRole::Compatibility {
                self.space.map_element(
                    window,
                    compatibility_location(self.output_size, COMPATIBILITY_SIZE),
                    false,
                );
            }
        }
    }

    fn apply_fixed_size(&mut self, surface: &ToplevelSurface) {
        let role = Self::client_role(surface.wl_surface()).unwrap_or(ClientRole::Compatibility);
        let size: Size<i32, Logical> = match role {
            ClientRole::Shell => self.output_size.into(),
            ClientRole::Compatibility => COMPATIBILITY_SIZE.into(),
        };
        surface.with_pending_state(|state| {
            state.size = Some(size);
            if role == ClientRole::Shell {
                state.states.set(xdg_toplevel::State::Fullscreen);
            }
        });
        surface.send_pending_configure();
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.space.elements().find(|window| {
            window
                .toplevel()
                .is_some_and(|top| top.wl_surface() == &root)
        }) else {
            return;
        };
        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let Some(mut target) = self.space.output_geometry(output) else {
            return;
        };
        let Some(window_geometry) = self.space.element_geometry(window) else {
            return;
        };
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geometry.loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
