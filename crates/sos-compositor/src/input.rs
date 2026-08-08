// Input forwarding is adapted from Smithay's MIT-licensed `smallvil` example
// at tag v0.7.0, with SOS focus ordering and activation quiescing.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
};

use crate::{policy::ClientRole, state::SosCompositor};

impl SosCompositor {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        if self.policy.input_quiesced() {
            tracing::debug!("input quiesced for armed revision activation");
            return;
        }
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let keyboard = self.seat.get_keyboard().expect("seat has a keyboard");
                keyboard.input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, _, _| FilterResult::Forward,
                );
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(output) = self.space.outputs().next() else {
                    return;
                };
                let Some(output_geometry) = self.space.output_geometry(output) else {
                    return;
                };
                let position =
                    event.position_transformed(output_geometry.size) + output_geometry.loc.to_f64();
                let pointer = self.seat.get_pointer().expect("seat has a pointer");
                pointer.motion(
                    self,
                    self.surface_under(position),
                    &MotionEvent {
                        location: position,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().expect("seat has a pointer");
                let keyboard = self.seat.get_keyboard().expect("seat has a keyboard");
                let serial = SERIAL_COUNTER.next_serial();
                if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
                    let focused = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(window, _)| window.clone());
                    if let Some(window) = focused {
                        let surface = window
                            .toplevel()
                            .expect("mapped window is XDG")
                            .wl_surface()
                            .clone();
                        if Self::client_role(&surface) == Some(ClientRole::Compatibility) {
                            self.space.raise_element(&window, false);
                        }
                        self.space.elements().for_each(|candidate| {
                            candidate.set_activated(candidate == &window);
                            candidate
                                .toplevel()
                                .expect("mapped window is XDG")
                                .send_pending_configure();
                        });
                        keyboard.set_focus(self, Some(surface), serial);
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            window
                                .toplevel()
                                .expect("mapped window is XDG")
                                .send_pending_configure();
                        });
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                    }
                }
                pointer.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state: event.state(),
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();
                let horizontal = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
                });
                let vertical = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
                });
                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal);
                    if let Some(discrete) = event.amount_v120(Axis::Horizontal) {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical);
                    if let Some(discrete) = event.amount_v120(Axis::Vertical) {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }
                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }
                let pointer = self.seat.get_pointer().expect("seat has a pointer");
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}
