//! Optional rootless XWayland compatibility envelope.

use std::{
    fs::OpenOptions,
    io::{ErrorKind, Write as _},
    os::unix::fs::OpenOptionsExt as _,
    path::PathBuf,
    process::Stdio,
};

use anyhow::{Context as _, Result};
use smithay::{
    desktop::Window,
    reexports::{
        calloop::EventLoop,
        wayland_server::{protocol::wl_surface::WlSurface, Resource as _},
    },
    utils::{Logical, Rectangle},
    wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    xwayland::{
        xwm::{Reorder, ResizeEdge, XwmId},
        X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler,
    },
};

use crate::{policy::compatibility_location, state::SosCompositor, CompositorData};

const LEGACY_SIZE: (i32, i32) = (720, 520);
const MAX_LEGACY_WINDOWS: usize = 8;

pub(crate) fn start(
    event_loop: &mut EventLoop<'static, CompositorData>,
    data: &mut CompositorData,
    display_file: PathBuf,
    display_number: Option<u32>,
) -> Result<()> {
    let parent = display_file
        .parent()
        .context("XWayland display file has no parent")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create XWayland runtime directory {}", parent.display()))?;
    if display_file.exists() {
        anyhow::bail!(
            "XWayland display file already exists: {}",
            display_file.display()
        );
    }
    if let Some(display_number) = display_number {
        for path in [
            PathBuf::from(format!("/tmp/.X11-unix/X{display_number}")),
            PathBuf::from(format!("/tmp/.X{display_number}-lock")),
        ] {
            match path.symlink_metadata() {
                Ok(_) => anyhow::bail!(
                    "refusing preexisting X11 display artifact for :{display_number}: {}",
                    path.display()
                ),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "inspect X11 display artifact for :{display_number}: {}",
                            path.display()
                        )
                    });
                }
            }
        }
    }
    let (xwayland, client) = XWayland::spawn(
        &data.display_handle,
        display_number,
        std::iter::empty::<(String, String)>(),
        true,
        Stdio::null(),
        Stdio::null(),
        |_| (),
    )
    .context("spawn rootless XWayland")?;
    let handle = event_loop.handle();
    let wm_handle = handle.clone();
    handle
        .insert_source(xwayland, move |event, _, data| match event {
            XWaylandEvent::Ready {
                x11_socket,
                display_number,
            } => match X11Wm::start_wm(wm_handle.clone(), x11_socket, client.clone()) {
                Ok(wm) => {
                    data.state.xwm = Some(wm);
                    let result = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&display_file)
                        .and_then(|mut file| writeln!(file, ":{display_number}"));
                    if let Err(error) = result {
                        tracing::error!(%error, path = %display_file.display(), "could not publish XWayland display");
                    } else {
                        tracing::info!(display_number, "rootless XWayland ready");
                    }
                }
                Err(error) => tracing::error!(%error, "could not start XWayland window manager"),
            },
            XWaylandEvent::Error => tracing::error!("XWayland exited during startup"),
        })
        .map_err(|_| anyhow::anyhow!("insert XWayland event source"))?;
    Ok(())
}

impl SosCompositor {
    fn map_x11(&mut self, surface: X11Surface, override_redirect: bool) {
        if !self.policy.shell_mapped() {
            tracing::warn!(
                window = surface.window_id(),
                "rejected X11 window before shell map"
            );
            return;
        }
        let count = self
            .space
            .elements()
            .filter(|window| window.is_x11())
            .count();
        if count >= MAX_LEGACY_WINDOWS {
            tracing::warn!(window = surface.window_id(), "rejected excess X11 window");
            return;
        }
        let base = compatibility_location(self.output_size, LEGACY_SIZE);
        let offset = i32::try_from(count).unwrap_or_default() * 24;
        let location = (base.0 + offset, base.1 + offset);
        let geometry = Rectangle::new(location.into(), LEGACY_SIZE.into());
        if !override_redirect {
            if let Err(error) = surface.configure(geometry) {
                tracing::warn!(%error, window = surface.window_id(), "could not configure X11 window");
                return;
            }
            if let Err(error) = surface.set_mapped(true) {
                tracing::warn!(%error, window = surface.window_id(), "could not map X11 window");
                return;
            }
        }
        self.space
            .map_element(Window::new_x11_window(surface.clone()), location, true);
        tracing::info!(
            window = surface.window_id(),
            title = surface.title(),
            x = location.0,
            y = location.1,
            override_redirect,
            "mapped bounded XWayland window"
        );
    }

    fn unmap_x11(&mut self, surface: &X11Surface) {
        let window = self
            .space
            .elements()
            .find(|window| {
                window
                    .x11_surface()
                    .is_some_and(|candidate| candidate.window_id() == surface.window_id())
            })
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window);
        }
    }

    fn configure_x11(&mut self, surface: &X11Surface, geometry: Rectangle<i32, Logical>) {
        let width = geometry.size.w.clamp(64, self.output_size.0.max(64));
        let height = geometry.size.h.clamp(64, self.output_size.1.max(64));
        let max_x = (self.output_size.0 - width).max(0);
        let max_y = (self.output_size.1 - height).max(0);
        let bounded = Rectangle::new(
            (
                geometry.loc.x.clamp(0, max_x),
                geometry.loc.y.clamp(0, max_y),
            )
                .into(),
            (width, height).into(),
        );
        if !surface.is_override_redirect() {
            let _ = surface.configure(bounded);
        }
        let mapped = self
            .space
            .elements()
            .find(|window| {
                window
                    .x11_surface()
                    .is_some_and(|candidate| candidate.window_id() == surface.window_id())
            })
            .cloned();
        if let Some(window) = mapped {
            self.space.map_element(window, bounded.loc, true);
        }
    }
}

trait AsSosState {
    fn sos_state(&mut self) -> &mut SosCompositor;
}

impl AsSosState for SosCompositor {
    fn sos_state(&mut self) -> &mut SosCompositor {
        self
    }
}

impl AsSosState for CompositorData {
    fn sos_state(&mut self) -> &mut SosCompositor {
        &mut self.state
    }
}

macro_rules! impl_xwm_handler {
    ($target:ty) => {
        impl XwmHandler for $target {
            fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
                self.sos_state().xwm.as_mut().expect("XWayland WM is ready")
            }

            fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}
            fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

            fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
                self.sos_state().map_x11(window, false);
            }

            fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
                self.sos_state().map_x11(window, true);
            }

            fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
                self.sos_state().unmap_x11(&window);
            }

            fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
                self.sos_state().unmap_x11(&window);
            }

            fn configure_request(
                &mut self,
                _xwm: XwmId,
                window: X11Surface,
                x: Option<i32>,
                y: Option<i32>,
                width: Option<u32>,
                height: Option<u32>,
                _reorder: Option<Reorder>,
            ) {
                let mut geometry = window.geometry();
                if let Some(x) = x {
                    geometry.loc.x = x;
                }
                if let Some(y) = y {
                    geometry.loc.y = y;
                }
                if let Some(width) = width {
                    geometry.size.w = i32::try_from(width).unwrap_or(i32::MAX);
                }
                if let Some(height) = height {
                    geometry.size.h = i32::try_from(height).unwrap_or(i32::MAX);
                }
                self.sos_state().configure_x11(&window, geometry);
            }

            fn configure_notify(
                &mut self,
                _xwm: XwmId,
                window: X11Surface,
                geometry: Rectangle<i32, Logical>,
                _above: Option<u32>,
            ) {
                self.sos_state().configure_x11(&window, geometry);
            }

            fn resize_request(
                &mut self,
                _xwm: XwmId,
                window: X11Surface,
                _button: u32,
                _edge: ResizeEdge,
            ) {
                self.sos_state().configure_x11(&window, window.geometry());
            }

            fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
                self.sos_state().configure_x11(&window, window.geometry());
            }
        }
    };
}

impl_xwm_handler!(SosCompositor);
impl_xwm_handler!(CompositorData);

impl XWaylandShellHandler for SosCompositor {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    fn surface_associated(&mut self, _xwm: XwmId, surface: WlSurface, window: X11Surface) {
        tracing::debug!(window = window.window_id(), surface = ?surface.id(), "associated XWayland surface");
    }
}

impl XWaylandShellHandler for CompositorData {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.state.xwayland_shell_state
    }

    fn surface_associated(&mut self, _xwm: XwmId, surface: WlSurface, window: X11Surface) {
        tracing::debug!(window = window.window_id(), surface = ?surface.id(), "associated XWayland surface");
    }
}
