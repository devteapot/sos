mod compositor;
mod xdg_shell;

use smithay::{
    delegate_data_device, delegate_output, delegate_presentation, delegate_seat,
    delegate_text_input_manager,
    input::{Seat, SeatHandler, SeatState},
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource as _},
    wayland::{
        output::OutputHandler,
        selection::{
            data_device::{
                set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
                ServerDndGrabHandler,
            },
            SelectionHandler,
        },
    },
};

use crate::state::SosCompositor;

impl SeatHandler for SosCompositor {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client);
    }
}

delegate_seat!(SosCompositor);
delegate_text_input_manager!(SosCompositor);

impl SelectionHandler for SosCompositor {
    type SelectionUserData = ();
}

impl DataDeviceHandler for SosCompositor {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for SosCompositor {}
impl ServerDndGrabHandler for SosCompositor {}

delegate_data_device!(SosCompositor);

impl OutputHandler for SosCompositor {}
delegate_output!(SosCompositor);
delegate_presentation!(SosCompositor);
