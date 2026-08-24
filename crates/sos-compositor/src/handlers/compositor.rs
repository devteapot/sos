use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler,
            CompositorState,
        },
        seat::WaylandFocus as _,
        shm::{ShmHandler, ShmState},
    },
};

use crate::{handlers::xdg_shell, state::ClientState, state::SosCompositor};

#[cfg(feature = "direct-backend")]
use smithay::{
    backend::{allocator::dmabuf::Dmabuf, renderer::ImportDma},
    delegate_dmabuf,
    wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
};

impl CompositorHandler for SosCompositor {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        if let Some(data) = client.get_data::<ClientState>() {
            &data.compositor_state
        } else {
            &client
                .get_data::<smithay::xwayland::XWaylandClientData>()
                .expect("accepted clients carry compositor or XWayland state")
                .compositor_state
        }
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if self.shell_surface.as_ref() == Some(&root) {
                let sequence = self.policy.record_shell_commit();
                tracing::debug!(sequence, "observed shell surface commit");
            }
            if let Some(window) = self.space.elements().find(|window| {
                window
                    .wl_surface()
                    .is_some_and(|surface| surface.as_ref() == &root)
            }) {
                window.on_commit();
            }
        }
        xdg_shell::handle_commit(&mut self.popups, &self.space, surface);
    }
}

impl BufferHandler for SosCompositor {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for SosCompositor {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(SosCompositor);
delegate_shm!(SosCompositor);

#[cfg(feature = "direct-backend")]
impl DmabufHandler for SosCompositor {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self
            .dmabuf_state
            .as_mut()
            .expect("direct backend initializes dmabuf state before accepting clients")
            .0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let Some(primary) = self.dmabuf_primary else {
            tracing::warn!("rejected dmabuf because the direct renderer is unavailable");
            notifier.failed();
            return;
        };

        for (node, renderer) in &self.dmabuf_renderers {
            if !self.dmabuf_active_devices.contains(node) {
                continue;
            }
            let Ok(mut renderer) = renderer.try_borrow_mut() else {
                tracing::warn!(?node, "rejected dmabuf while its direct renderer was busy");
                notifier.failed();
                return;
            };
            if let Err(error) = renderer.import_dmabuf(&dmabuf, None) {
                tracing::warn!(?node, %error, "direct renderer rejected client dmabuf");
                notifier.failed();
                return;
            }
        }

        dmabuf.set_node(primary);
        if notifier.successful::<Self>().is_err() {
            tracing::warn!("dmabuf client disappeared before buffer creation completed");
        }
    }
}

#[cfg(feature = "direct-backend")]
delegate_dmabuf!(SosCompositor);
