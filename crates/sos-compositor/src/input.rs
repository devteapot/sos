// Input forwarding is adapted from Smithay's MIT-licensed `smallvil` example
// at tag v0.7.0, with SOS focus ordering and activation quiescing.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
};

use crate::{policy::ClientRole, state::SosCompositor};

impl SosCompositor {
    pub fn begin_input_quiesce(&mut self) {
        self.quiesced_input_events = 0;
        let keyboard = self.seat.get_keyboard().expect("seat has a keyboard");
        self.quiesced_keyboard_focus = keyboard.current_focus();
        keyboard.set_focus(
            self,
            Option::<WlSurface>::None,
            SERIAL_COUNTER.next_serial(),
        );
        let pressed_keys = keyboard.pressed_keys();
        self.suppressed_keyboard_keys
            .extend(pressed_keys.iter().copied());
        for key in &pressed_keys {
            keyboard.input_forward(
                self,
                *key,
                KeyState::Released,
                SERIAL_COUNTER.next_serial(),
                0,
                false,
            );
        }

        let pointer = self.seat.get_pointer().expect("seat has a pointer");
        let pressed_buttons = self
            .pressed_pointer_buttons
            .iter()
            .copied()
            .collect::<Vec<_>>();
        self.suppressed_pointer_buttons
            .extend(pressed_buttons.iter().copied());
        pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), 0);
        let location = pointer.current_location();
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: 0,
            },
        );
        for button in &pressed_buttons {
            pointer.button(
                self,
                &ButtonEvent {
                    button: *button,
                    state: ButtonState::Released,
                    serial: SERIAL_COUNTER.next_serial(),
                    time: 0,
                },
            );
        }
        pointer.frame(self);
        tracing::info!(
            keys = pressed_keys.len(),
            buttons = pressed_buttons.len(),
            "detached focused input for revision quiesce"
        );
    }

    pub fn end_input_quiesce(&mut self, restore_shell_focus: bool) {
        let keyboard = self.seat.get_keyboard().expect("seat has a keyboard");
        let focus = self
            .quiesced_keyboard_focus
            .take()
            .filter(|_| restore_shell_focus)
            .filter(smithay::reexports::wayland_server::Resource::is_alive);
        keyboard.set_focus(self, focus, SERIAL_COUNTER.next_serial());

        let pointer = self.seat.get_pointer().expect("seat has a pointer");
        let location = pointer.current_location();
        pointer.motion(
            self,
            self.surface_under(location),
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: 0,
            },
        );
        pointer.frame(self);
        tracing::info!(
            suppressed_events = self.quiesced_input_events,
            restore_shell_focus,
            "finished compositor input quiesce"
        );
    }

    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        if self.policy.input_quiesced() {
            self.quiesced_input_events = self.quiesced_input_events.saturating_add(1);
            match event {
                InputEvent::Keyboard { event, .. } => {
                    match event.state() {
                        KeyState::Pressed => {
                            self.suppressed_keyboard_keys.insert(event.key_code());
                        }
                        KeyState::Released => {
                            self.suppressed_keyboard_keys.remove(&event.key_code());
                        }
                    }
                    let keyboard = self.seat.get_keyboard().expect("seat has a keyboard");
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        SERIAL_COUNTER.next_serial(),
                        Event::time_msec(&event),
                        |_, _, _| FilterResult::Intercept(()),
                    );
                }
                InputEvent::PointerButton { event, .. } => match event.state() {
                    ButtonState::Pressed => {
                        self.pressed_pointer_buttons.insert(event.button_code());
                        self.suppressed_pointer_buttons.insert(event.button_code());
                    }
                    ButtonState::Released => {
                        self.pressed_pointer_buttons.remove(&event.button_code());
                        self.suppressed_pointer_buttons.remove(&event.button_code());
                    }
                },
                _ => {}
            }
            tracing::debug!("input quiesced for revision activation");
            return;
        }
        match event {
            InputEvent::Keyboard { event, .. } => {
                if self.suppressed_keyboard_keys.contains(&event.key_code()) {
                    let released = event.state() == KeyState::Released;
                    let keyboard = self.seat.get_keyboard().expect("seat has a keyboard");
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        SERIAL_COUNTER.next_serial(),
                        Event::time_msec(&event),
                        |_, _, _| FilterResult::Intercept(()),
                    );
                    if released {
                        self.suppressed_keyboard_keys.remove(&event.key_code());
                    }
                    tracing::debug!(
                        key = u32::from(event.key_code()),
                        ?released,
                        "suppressed key held across activation"
                    );
                    return;
                }
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
            InputEvent::PointerMotion { event, .. } => {
                let Some(output) = self.space.outputs().next() else {
                    return;
                };
                let Some(output_geometry) = self.space.output_geometry(output) else {
                    return;
                };
                let pointer = self.seat.get_pointer().expect("seat has a pointer");
                let current = pointer.current_location();
                let minimum = output_geometry.loc.to_f64();
                let maximum = (output_geometry.loc + output_geometry.size).to_f64();
                let location = current + event.delta();
                let location = (
                    location.x.clamp(minimum.x, maximum.x - 1.0),
                    location.y.clamp(minimum.y, maximum.y - 1.0),
                )
                    .into();
                let focus = self.surface_under(location);
                pointer.motion(
                    self,
                    focus.clone(),
                    &MotionEvent {
                        location,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
                pointer.relative_motion(
                    self,
                    focus,
                    &RelativeMotionEvent {
                        delta: event.delta(),
                        delta_unaccel: event.delta_unaccel(),
                        utime: event.time(),
                    },
                );
                pointer.frame(self);
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
                match event.state() {
                    ButtonState::Pressed => {
                        self.pressed_pointer_buttons.insert(event.button_code());
                    }
                    ButtonState::Released => {
                        self.pressed_pointer_buttons.remove(&event.button_code());
                        if self.suppressed_pointer_buttons.remove(&event.button_code()) {
                            tracing::debug!(
                                button = event.button_code(),
                                "suppressed release for button held across activation"
                            );
                            return;
                        }
                    }
                }
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
