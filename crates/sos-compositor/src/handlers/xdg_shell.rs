use smithay::{
    backend::renderer::utils::with_renderer_surface_state,
    delegate_xdg_shell,
    desktop::{find_popup_root_surface, get_popup_toplevel_coords, PopupKind, Window},
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            backend::DisconnectReason,
            protocol::{wl_seat, wl_surface::WlSurface},
            Resource as _,
        },
    },
    utils::{Logical, Rectangle, Serial, Size},
    wayland::{
        compositor::with_states,
        seat::WaylandFocus as _,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelStateSet, ToplevelSurface, XdgShellHandler,
            XdgShellState, XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    policy::{
        default_shell_overlay, default_window_space, validate_shell_overlay, validate_window_space,
        window_rectangles, ClientRole, WindowRectangle, MAX_COMPATIBILITY_TOPLEVELS,
    },
    state::{ApplicationWindowDrag, ShellOverlayDrag, SosCompositor, SurfaceRoleData},
};

impl XdgShellHandler for SosCompositor {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();
        let client_role = Self::client_role(&wl_surface).unwrap_or(ClientRole::Compatibility);
        let incoming_pid = Self::client_pid(&wl_surface);
        let existing_shell_pid = self.shell_surface.as_ref().and_then(Self::client_pid);
        let role = if client_role == ClientRole::Shell {
            match (existing_shell_pid, incoming_pid) {
                (None, _) => ClientRole::Shell,
                (Some(existing), Some(incoming)) if existing != incoming => ClientRole::Shell,
                _ if self.shell_overlay_surface.is_none() => ClientRole::ShellOverlay,
                _ => ClientRole::NativeApplication,
            }
        } else {
            ClientRole::Compatibility
        };
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
                    self.policy.unmap(ClientRole::Shell);
                }
                self.surface_roles.remove(&stale_shell.id());
                self.xdg_windows.remove(&stale_shell.id());
                self.shell_surface = None;
                tracing::info!("replaced stale SOS shell surface");
            }
            let stale_overlay = self.shell_overlay_surface.take();
            if let Some(stale_overlay) = stale_overlay {
                if let Some(stale_window) = self.xdg_windows.remove(&stale_overlay.id()) {
                    if self
                        .space
                        .elements()
                        .any(|candidate| candidate == &stale_window)
                    {
                        self.space.unmap_elem(&stale_window);
                        self.policy.unmap(ClientRole::ShellOverlay);
                    }
                }
                self.surface_roles.remove(&stale_overlay.id());
                tracing::info!("removed stale SOS shell overlay surface");
            }
        } else if matches!(
            role,
            ClientRole::NativeApplication | ClientRole::Compatibility
        ) {
            let application_count = self
                .space
                .elements()
                .filter(|window| {
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
                })
                .count();
            if application_count >= MAX_COMPATIBILITY_TOPLEVELS {
                tracing::warn!(
                    limit = MAX_COMPATIBILITY_TOPLEVELS,
                    "rejected excess compatibility toplevel"
                );
                if let Some(client) = wl_surface.client() {
                    self.display_handle
                        .backend_handle()
                        .kill_client(client.id(), DisconnectReason::ConnectionClosed);
                }
                return;
            }
        }
        self.surface_roles.insert(wl_surface.id(), role);
        with_states(&wl_surface, |states| {
            states
                .data_map
                .insert_if_missing_threadsafe(|| SurfaceRoleData(role));
        });

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
            ClientRole::ShellOverlay => {
                let configuration = self.shell_overlay;
                let size: Size<i32, Logical> = (
                    i32::try_from(configuration.width).unwrap_or(i32::MAX),
                    i32::try_from(configuration.height).unwrap_or(i32::MAX),
                )
                    .into();
                surface.with_pending_state(|state| state.size = Some(size));
                self.shell_overlay_surface = Some(wl_surface.clone());
                (configuration.x, configuration.y)
            }
            ClientRole::NativeApplication | ClientRole::Compatibility => {
                let count = self
                    .space
                    .elements()
                    .filter(|window| {
                        window
                            .wl_surface()
                            .and_then(|surface| Self::client_role(&surface))
                            .is_some_and(|role| {
                                matches!(
                                    role,
                                    ClientRole::NativeApplication | ClientRole::Compatibility
                                )
                            })
                    })
                    .count()
                    + 1;
                let rectangle = window_rectangles(self.window_space, count)
                    .last()
                    .copied()
                    .expect("new compatibility window has a layout rectangle");
                let size: Size<i32, Logical> = (rectangle.width, rectangle.height).into();
                surface.with_pending_state(|state| {
                    state.size = Some(size);
                    configure_application_toplevel_states(
                        &mut state.states,
                        self.window_space.layout,
                    );
                });
                (rectangle.x, rectangle.y)
            }
        };

        self.xdg_windows
            .insert(wl_surface.id(), Window::new_wayland_window(surface));
        tracing::info!(
            ?role,
            x = location.0,
            y = location.1,
            "registered compositor-managed XDG toplevel"
        );
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let Some(role) = self.surface_roles.remove(&surface.wl_surface().id()) else {
            tracing::debug!("ignored destruction of an unmapped XDG toplevel");
            return;
        };
        let window = self.xdg_windows.remove(&surface.wl_surface().id());
        let was_mapped = window
            .as_ref()
            .is_some_and(|window| self.space.elements().any(|candidate| candidate == window));
        if window.as_ref().is_some_and(|window| {
            self.application_window_drag
                .as_ref()
                .is_some_and(|drag| &drag.window == window)
        }) {
            self.application_window_drag = None;
        }
        if let Some(window) = window.filter(|_| was_mapped) {
            self.space.unmap_elem(&window);
        }
        if role == ClientRole::Shell && self.shell_surface.as_ref() == Some(surface.wl_surface()) {
            self.shell_surface = None;
        }
        if role == ClientRole::ShellOverlay
            && self.shell_overlay_surface.as_ref() == Some(surface.wl_surface())
        {
            self.shell_overlay_surface = None;
        }
        if was_mapped {
            self.policy.unmap(role);
        }
        if was_mapped
            && matches!(
                role,
                ClientRole::NativeApplication | ClientRole::Compatibility
            )
        {
            self.reconfigure_application_windows();
        }
        tracing::info!(
            ?role,
            was_mapped,
            "destroyed compositor-managed XDG toplevel"
        );
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        if !self.pressed_pointer_buttons.contains(&0x110) {
            return;
        }
        let pointer = self.seat.get_pointer().expect("seat has a pointer");
        let pointer_location = pointer.current_location();
        match Self::client_role(surface.wl_surface()) {
            Some(ClientRole::ShellOverlay) => {
                if self.shell_overlay_hovered {
                    self.shell_overlay_hovered = false;
                    if let Some((_, events)) = &self.shell_events {
                        let _ = events.send(
                            compositor_control_protocol::CompositorEvent::ShellOverlayHoverChanged {
                                request_id: 0,
                                hovered: false,
                            },
                        );
                    }
                    tracing::info!("collapsed trusted shell overlay for move");
                }
                self.shell_overlay_drag = Some(ShellOverlayDrag {
                    pointer: pointer_location,
                    origin: (self.shell_overlay.x, self.shell_overlay.y),
                });
                tracing::info!("began trusted shell overlay move");
            }
            Some(ClientRole::NativeApplication | ClientRole::Compatibility)
                if self.window_space.layout
                    == compositor_control_protocol::WindowLayoutMode::Floating =>
            {
                let Some(window) = self.xdg_windows.get(&surface.wl_surface().id()).cloned() else {
                    return;
                };
                let Some(origin) = self.space.element_location(&window) else {
                    return;
                };
                self.space.raise_element(&window, false);
                self.application_window_drag = Some(ApplicationWindowDrag {
                    window,
                    pointer: pointer_location,
                    origin,
                });
                tracing::info!("began compositor-managed application move");
            }
            _ => {}
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        if matches!(
            Self::client_role(surface.wl_surface()),
            Some(ClientRole::NativeApplication | ClientRole::Compatibility)
        ) {
            if self.window_space.layout != compositor_control_protocol::WindowLayoutMode::Floating {
                self.reconfigure_application_windows();
            }
            tracing::debug!(
                ?edges,
                ?self.window_space.layout,
                "ignored unsupported application resize request"
            );
        }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XdgMappingTransition {
    Map,
    Unmap,
    Unchanged,
}

fn xdg_mapping_transition(has_buffer: bool, is_mapped: bool) -> XdgMappingTransition {
    match (has_buffer, is_mapped) {
        (true, false) => XdgMappingTransition::Map,
        (false, true) => XdgMappingTransition::Unmap,
        _ => XdgMappingTransition::Unchanged,
    }
}

fn configure_application_toplevel_states(
    states: &mut ToplevelStateSet,
    layout: compositor_control_protocol::WindowLayoutMode,
) {
    let compositor_managed = layout != compositor_control_protocol::WindowLayoutMode::Floating;
    for edge in [
        xdg_toplevel::State::TiledTop,
        xdg_toplevel::State::TiledRight,
        xdg_toplevel::State::TiledBottom,
        xdg_toplevel::State::TiledLeft,
    ] {
        if compositor_managed {
            states.set(edge);
        } else {
            states.unset(edge);
        }
    }
}

impl SosCompositor {
    pub(crate) fn handle_xdg_commit(&mut self, surface: &WlSurface) {
        let mut root = surface.clone();
        while let Some(parent) = smithay::wayland::compositor::get_parent(&root) {
            root = parent;
        }

        if let Some(window) = self.xdg_windows.get(&root.id()).cloned() {
            let initial_configure_sent = with_states(&root, |states| {
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
                    .expect("registered window is XDG")
                    .send_configure();
            }

            let has_buffer = with_renderer_surface_state(&root, |state| state.buffer().is_some())
                .unwrap_or(false);
            let is_mapped = self.space.elements().any(|candidate| candidate == &window);
            match xdg_mapping_transition(has_buffer, is_mapped) {
                XdgMappingTransition::Map => self.map_xdg_window(window),
                XdgMappingTransition::Unmap => self.unmap_xdg_window(window),
                XdgMappingTransition::Unchanged => {}
            }
        }

        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface) {
            if !popup.is_initial_configure_sent() {
                let _ = popup.send_configure();
            }
        }
    }

    fn map_xdg_window(&mut self, window: Window) {
        let wl_surface = window
            .wl_surface()
            .expect("registered XDG window has a Wayland surface");
        let role = Self::client_role(&wl_surface).unwrap_or(ClientRole::Compatibility);
        if let Err(error) = self.policy.map(role) {
            tracing::warn!(?role, error = %error, "rejected XDG buffer map by SOS surface policy");
            if let Some(client) = wl_surface.client() {
                self.display_handle
                    .backend_handle()
                    .kill_client(client.id(), DisconnectReason::ConnectionClosed);
            }
            return;
        }

        if role == ClientRole::Shell {
            let compatibility = self
                .space
                .elements()
                .filter(|window| {
                    window
                        .wl_surface()
                        .and_then(|surface| Self::client_role(&surface))
                        .is_some_and(|role| {
                            matches!(
                                role,
                                ClientRole::NativeApplication | ClientRole::Compatibility
                            )
                        })
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
            self.space.map_element(window, (0, 0), false);
            for (compatibility, location) in compatibility {
                self.space.map_element(compatibility, location, false);
            }
        } else if role == ClientRole::ShellOverlay {
            let configuration = self.shell_overlay;
            self.space
                .map_element(window, (configuration.x, configuration.y), false);
            let pointer = self
                .seat
                .get_pointer()
                .expect("seat has a pointer")
                .current_location();
            self.synchronize_shell_overlay_hover(pointer, true);
        } else {
            let application_count = self
                .space
                .elements()
                .filter(|candidate| self.is_application_window(candidate))
                .count()
                + 1;
            let location = window_rectangles(self.window_space, application_count)
                .last()
                .map(|rectangle| (rectangle.x, rectangle.y))
                .unwrap_or((self.window_space.geometry.x, self.window_space.geometry.y));
            self.space.map_element(window, location, false);
            self.reconfigure_application_windows();
        }
        tracing::info!(?role, "mapped compositor-managed XDG toplevel buffer");
    }

    fn unmap_xdg_window(&mut self, window: Window) {
        let role = window
            .wl_surface()
            .and_then(|surface| Self::client_role(&surface))
            .unwrap_or(ClientRole::Compatibility);
        self.space.unmap_elem(&window);
        if self
            .application_window_drag
            .as_ref()
            .is_some_and(|drag| drag.window == window)
        {
            self.application_window_drag = None;
        }
        self.policy.unmap(role);
        if matches!(
            role,
            ClientRole::NativeApplication | ClientRole::Compatibility
        ) {
            self.reconfigure_application_windows();
        }
        tracing::info!(?role, "unmapped compositor-managed XDG toplevel buffer");
    }

    pub(crate) fn reconfigure_for_output_layout(&mut self) {
        if validate_window_space(self.window_space, self.output_size).is_err() {
            self.window_space = default_window_space(self.output_size);
        }
        if validate_shell_overlay(self.shell_overlay, self.output_size).is_err() {
            self.shell_overlay = default_shell_overlay(self.output_size);
        }
        self.apply_shell_size();
        self.apply_shell_overlay_configuration();
        self.reconfigure_application_windows();
    }

    pub(crate) fn reconfigure_application_windows(&mut self) {
        let windows = self
            .space
            .elements()
            .filter(|window| {
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
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut rectangles = window_rectangles(self.window_space, windows.len());
        if self.window_space.layout == compositor_control_protocol::WindowLayoutMode::Floating {
            for (window, rectangle) in windows.iter().zip(&mut rectangles) {
                if let Some(location) = self.space.element_location(window) {
                    let geometry = self.window_space.geometry;
                    let right = geometry
                        .x
                        .saturating_add(i32::try_from(geometry.width).unwrap_or(i32::MAX));
                    let bottom = geometry
                        .y
                        .saturating_add(i32::try_from(geometry.height).unwrap_or(i32::MAX));
                    rectangle.x = location.x.clamp(
                        geometry.x,
                        right.saturating_sub(rectangle.width).max(geometry.x),
                    );
                    rectangle.y = location.y.clamp(
                        geometry.y,
                        bottom.saturating_sub(rectangle.height).max(geometry.y),
                    );
                }
            }
        }
        for (window, rectangle) in windows.into_iter().zip(rectangles) {
            self.configure_application_window(window, rectangle);
        }
    }

    pub(crate) fn reset_application_window_layout(&mut self) {
        self.application_window_drag = None;
        let windows = self
            .space
            .elements()
            .filter(|window| self.is_application_window(window))
            .cloned()
            .collect::<Vec<_>>();
        let rectangles = window_rectangles(self.window_space, windows.len());
        for (window, rectangle) in windows.into_iter().zip(rectangles) {
            self.configure_application_window(window, rectangle);
        }
    }

    fn configure_application_window(&mut self, window: Window, rectangle: WindowRectangle) {
        if let Some(toplevel) = window.toplevel().cloned() {
            let size: Size<i32, Logical> = (rectangle.width, rectangle.height).into();
            toplevel.with_pending_state(|state| {
                state.size = Some(size);
                configure_application_toplevel_states(&mut state.states, self.window_space.layout);
            });
            toplevel.send_pending_configure();
            self.space
                .map_element(window, (rectangle.x, rectangle.y), false);
        } else if let Some(surface) = window.x11_surface().cloned() {
            self.configure_x11(
                &surface,
                Rectangle::new(
                    (rectangle.x, rectangle.y).into(),
                    (rectangle.width, rectangle.height).into(),
                ),
            );
        }
    }

    fn apply_shell_size(&mut self) {
        let shell = self.space.elements().find_map(|window| {
            let toplevel = window.toplevel()?.clone();
            (Self::client_role(toplevel.wl_surface()) == Some(ClientRole::Shell))
                .then_some(toplevel)
        });
        if let Some(shell) = shell {
            self.apply_fixed_size(&shell);
        }
    }

    pub(crate) fn apply_shell_overlay_configuration(&mut self) {
        let Some(window) = self
            .space
            .elements()
            .find(|window| {
                window
                    .wl_surface()
                    .and_then(|surface| Self::client_role(&surface))
                    == Some(ClientRole::ShellOverlay)
            })
            .cloned()
        else {
            return;
        };
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        let configuration = self.shell_overlay;
        let size: Size<i32, Logical> = (
            i32::try_from(configuration.width).unwrap_or(i32::MAX),
            i32::try_from(configuration.height).unwrap_or(i32::MAX),
        )
            .into();
        toplevel.with_pending_state(|state| state.size = Some(size));
        toplevel.send_pending_configure();
        self.space
            .map_element(window, (configuration.x, configuration.y), false);
    }

    fn apply_fixed_size(&mut self, surface: &ToplevelSurface) {
        let role = Self::client_role(surface.wl_surface()).unwrap_or(ClientRole::Compatibility);
        let size: Size<i32, Logical> = match role {
            ClientRole::Shell => self.output_size.into(),
            ClientRole::ShellOverlay => (
                i32::try_from(self.shell_overlay.width).unwrap_or(i32::MAX),
                i32::try_from(self.shell_overlay.height).unwrap_or(i32::MAX),
            )
                .into(),
            ClientRole::NativeApplication | ClientRole::Compatibility => {
                let rectangle = window_rectangles(self.window_space, 1)
                    .into_iter()
                    .next()
                    .expect("window-space layout returns one rectangle");
                (rectangle.width, rectangle.height).into()
            }
        };
        surface.with_pending_state(|state| {
            state.size = Some(size);
            if role == ClientRole::Shell {
                state.states.set(xdg_toplevel::State::Fullscreen);
            } else if matches!(
                role,
                ClientRole::NativeApplication | ClientRole::Compatibility
            ) {
                configure_application_toplevel_states(&mut state.states, self.window_space.layout);
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

#[cfg(test)]
mod tests {
    use smithay::{
        reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland::shell::xdg::ToplevelStateSet,
    };

    use super::{
        configure_application_toplevel_states, xdg_mapping_transition, XdgMappingTransition,
    };

    #[test]
    fn maps_only_when_a_buffer_first_appears() {
        assert_eq!(
            xdg_mapping_transition(true, false),
            XdgMappingTransition::Map
        );
        assert_eq!(
            xdg_mapping_transition(true, true),
            XdgMappingTransition::Unchanged
        );
    }

    #[test]
    fn null_buffer_unmaps_without_destroying_the_role() {
        assert_eq!(
            xdg_mapping_transition(false, true),
            XdgMappingTransition::Unmap
        );
        assert_eq!(
            xdg_mapping_transition(false, false),
            XdgMappingTransition::Unchanged
        );
    }

    #[test]
    fn managed_layouts_advertise_every_tiled_edge() {
        for layout in [
            compositor_control_protocol::WindowLayoutMode::Tiling,
            compositor_control_protocol::WindowLayoutMode::Scrolling,
        ] {
            let mut states = ToplevelStateSet::default();
            states.set(xdg_toplevel::State::Activated);
            configure_application_toplevel_states(&mut states, layout);
            assert!(states.contains(xdg_toplevel::State::Activated));
            for edge in [
                xdg_toplevel::State::TiledTop,
                xdg_toplevel::State::TiledRight,
                xdg_toplevel::State::TiledBottom,
                xdg_toplevel::State::TiledLeft,
            ] {
                assert!(states.contains(edge));
            }
        }
    }

    #[test]
    fn floating_layout_clears_tiled_edges_without_losing_focus() {
        let mut states = ToplevelStateSet::default();
        states.set(xdg_toplevel::State::Activated);
        for edge in [
            xdg_toplevel::State::TiledTop,
            xdg_toplevel::State::TiledRight,
            xdg_toplevel::State::TiledBottom,
            xdg_toplevel::State::TiledLeft,
        ] {
            states.set(edge);
        }

        configure_application_toplevel_states(
            &mut states,
            compositor_control_protocol::WindowLayoutMode::Floating,
        );

        assert!(states.contains(xdg_toplevel::State::Activated));
        for edge in [
            xdg_toplevel::State::TiledTop,
            xdg_toplevel::State::TiledRight,
            xdg_toplevel::State::TiledBottom,
            xdg_toplevel::State::TiledLeft,
        ] {
            assert!(!states.contains(edge));
        }
    }
}
