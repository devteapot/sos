mod compositor;
mod xdg_shell;

use smithay::{
    delegate_data_device, delegate_input_method_manager, delegate_output, delegate_presentation,
    delegate_seat, delegate_tablet_manager, delegate_text_input_manager, delegate_xwayland_shell,
    input::{Seat, SeatHandler, SeatState},
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource as _},
    wayland::{
        input_method::{InputMethodHandler, PopupSurface},
        output::OutputHandler,
        selection::{
            data_device::{
                set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
                ServerDndGrabHandler,
            },
            SelectionHandler,
        },
        tablet_manager::TabletSeatHandler,
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
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        self.cursor_image = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client);
    }
}

delegate_seat!(SosCompositor);
delegate_text_input_manager!(SosCompositor);
delegate_input_method_manager!(SosCompositor);

impl TabletSeatHandler for SosCompositor {
    fn tablet_tool_image(
        &mut self,
        _tool: &smithay::backend::input::TabletToolDescriptor,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        self.cursor_image = image;
    }
}

delegate_tablet_manager!(SosCompositor);

impl InputMethodHandler for SosCompositor {
    fn new_popup(&mut self, surface: PopupSurface) {
        self.input_method_popups.retain(PopupSurface::alive);
        self.input_method_popups.push(surface);
    }

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        self.input_method_popups
            .retain(|candidate| candidate != &surface);
    }

    fn popup_repositioned(&mut self, _surface: PopupSurface) {}

    fn parent_geometry(
        &self,
        parent: &WlSurface,
    ) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
        self.space
            .elements()
            .find(|window| {
                window
                    .toplevel()
                    .is_some_and(|toplevel| toplevel.wl_surface() == parent)
            })
            .and_then(|window| self.space.element_geometry(window))
            .unwrap_or_default()
    }
}

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
delegate_xwayland_shell!(SosCompositor);
