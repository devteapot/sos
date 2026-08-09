// Input forwarding is adapted from Smithay's MIT-licensed `smallvil` example
// at tag v0.7.0, with SOS focus ordering and activation quiescing.

use std::collections::HashSet;

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Device as _, Event, InputBackend,
        InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
        PointerMotionEvent, ProximityState, TabletToolButtonEvent, TabletToolEvent,
        TabletToolProximityEvent, TabletToolTipEvent, TabletToolTipState, TouchEvent, TouchSlot,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
        touch::{DownEvent, MotionEvent as TouchMotion, UpEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
    wayland::{
        seat::WaylandFocus as _,
        tablet_manager::{TabletDescriptor, TabletHandle, TabletSeatTrait, TabletToolHandle},
    },
};

use crate::{policy::ClientRole, state::SosCompositor};

type LogicalPoint = smithay::utils::Point<f64, smithay::utils::Logical>;
type TabletTarget = (
    LogicalPoint,
    Option<(WlSurface, LogicalPoint)>,
    TabletHandle,
    TabletToolHandle,
);

#[derive(Default)]
pub(crate) struct TouchLifecycle {
    active: HashSet<TouchSlot>,
    suppressed: HashSet<TouchSlot>,
}

impl TouchLifecycle {
    fn begin_quiesce(&mut self) -> usize {
        let active = self.active.len();
        self.suppressed.extend(self.active.drain());
        active
    }

    fn observe_quiesced_down(&mut self, slot: TouchSlot) {
        self.suppressed.insert(slot);
    }

    fn observe_quiesced_up(&mut self, slot: TouchSlot) {
        self.active.remove(&slot);
        self.suppressed.remove(&slot);
    }

    fn begin_contact(&mut self, slot: TouchSlot) {
        self.suppressed.remove(&slot);
        self.active.insert(slot);
    }

    fn can_forward_motion(&self, slot: TouchSlot) -> bool {
        self.active.contains(&slot) && !self.suppressed.contains(&slot)
    }

    fn end_contact(&mut self, slot: TouchSlot) -> bool {
        if self.suppressed.remove(&slot) {
            self.active.remove(&slot);
            false
        } else {
            self.active.remove(&slot)
        }
    }

    fn cancel(&mut self) -> bool {
        let had_active = !self.active.is_empty();
        self.active.clear();
        self.suppressed.clear();
        had_active
    }
}

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

        let touch = self.seat.get_touch().expect("seat has touch");
        let active_touches = self.touch_lifecycle.begin_quiesce();
        if active_touches != 0 {
            touch.cancel(self);
        }
        tracing::info!(
            keys = pressed_keys.len(),
            buttons = pressed_buttons.len(),
            touches = active_touches,
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
        match &event {
            InputEvent::DeviceAdded { device }
                if device.has_capability(smithay::backend::input::DeviceCapability::TabletTool) =>
            {
                self.seat
                    .tablet_seat()
                    .add_tablet::<Self>(&self.display_handle, &TabletDescriptor::from(device));
            }
            InputEvent::DeviceRemoved { device }
                if device.has_capability(smithay::backend::input::DeviceCapability::TabletTool) =>
            {
                self.seat
                    .tablet_seat()
                    .remove_tablet(&TabletDescriptor::from(device));
            }
            _ => {}
        }
        let input_class = match &event {
            InputEvent::Keyboard { .. } => Some("keyboard"),
            InputEvent::PointerMotion { .. } => Some("relative_pointer"),
            InputEvent::PointerMotionAbsolute { .. } => Some("absolute_pointer"),
            InputEvent::PointerButton { .. } => Some("pointer_button"),
            InputEvent::PointerAxis { .. } => Some("pointer_axis"),
            InputEvent::TouchDown { .. }
            | InputEvent::TouchMotion { .. }
            | InputEvent::TouchUp { .. }
            | InputEvent::TouchCancel { .. }
            | InputEvent::TouchFrame { .. } => Some("touch"),
            InputEvent::TabletToolAxis { .. }
            | InputEvent::TabletToolProximity { .. }
            | InputEvent::TabletToolTip { .. }
            | InputEvent::TabletToolButton { .. } => Some("tablet_pressure"),
            _ => None,
        };
        if let Some(input_class) =
            input_class.filter(|value| self.observed_input_classes.insert(*value))
        {
            tracing::info!(input_class, "observed native compositor input");
        }
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
                InputEvent::TouchDown { event, .. } => {
                    self.touch_lifecycle.observe_quiesced_down(event.slot());
                }
                InputEvent::TouchUp { event, .. } => {
                    self.touch_lifecycle.observe_quiesced_up(event.slot());
                }
                InputEvent::TouchCancel { .. } => {
                    self.touch_lifecycle.cancel();
                }
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
                    tracing::info!(
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
                if event.button_code() == 0x110 && !self.policy.shell_mapped() {
                    if event.state() == ButtonState::Pressed {
                        let location = self
                            .seat
                            .get_pointer()
                            .expect("seat has a pointer")
                            .current_location();
                        if self
                            .recovery_ui
                            .click((location.x, location.y), self.output_size)
                        {
                            self.recovery_button_pressed = true;
                            return;
                        }
                    } else if self.recovery_button_pressed {
                        self.recovery_button_pressed = false;
                        return;
                    }
                }
                match event.state() {
                    ButtonState::Pressed => {
                        self.pressed_pointer_buttons.insert(event.button_code());
                    }
                    ButtonState::Released => {
                        self.pressed_pointer_buttons.remove(&event.button_code());
                        if self.suppressed_pointer_buttons.remove(&event.button_code()) {
                            tracing::info!(
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
                        let Some(surface) = window.wl_surface().map(|surface| surface.into_owned())
                        else {
                            return;
                        };
                        if window.is_x11()
                            || Self::client_role(&surface) == Some(ClientRole::Compatibility)
                        {
                            self.space.raise_element(&window, false);
                        }
                        self.space.elements().for_each(|candidate| {
                            candidate.set_activated(candidate == &window);
                            if let Some(toplevel) = candidate.toplevel() {
                                toplevel.send_pending_configure();
                            }
                        });
                        keyboard.set_focus(self, Some(surface), serial);
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            if let Some(toplevel) = window.toplevel() {
                                toplevel.send_pending_configure();
                            }
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
            InputEvent::TouchDown { event, .. } => {
                let Some(position) = self.touch_position(&event) else {
                    return;
                };
                let slot = event.slot();
                // A fresh down means libinput has reused this slot for a new
                // contact; an older activation-boundary suppression no longer
                // applies to it.
                self.touch_lifecycle.begin_contact(slot);
                tracing::info!(slot = i32::from(slot), "began native touch contact");
                let touch = self.seat.get_touch().expect("seat has touch");
                touch.down(
                    self,
                    self.surface_under(position),
                    &DownEvent {
                        slot,
                        location: position,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
            }
            InputEvent::TouchMotion { event, .. } => {
                let slot = event.slot();
                if !self.touch_lifecycle.can_forward_motion(slot) {
                    return;
                }
                let Some(position) = self.touch_position(&event) else {
                    return;
                };
                let touch = self.seat.get_touch().expect("seat has touch");
                touch.motion(
                    self,
                    self.surface_under(position),
                    &TouchMotion {
                        slot,
                        location: position,
                        time: event.time_msec(),
                    },
                );
            }
            InputEvent::TouchUp { event, .. } => {
                let slot = event.slot();
                tracing::info!(slot = i32::from(slot), "ended native touch contact");
                if !self.touch_lifecycle.end_contact(slot) {
                    tracing::info!(
                        slot = i32::from(slot),
                        "suppressed touch release held across activation"
                    );
                    return;
                }
                let touch = self.seat.get_touch().expect("seat has touch");
                touch.up(
                    self,
                    &UpEvent {
                        slot,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
            }
            InputEvent::TouchCancel { .. } => {
                tracing::info!("cancelled native touch contacts");
                let had_active_touches = self.touch_lifecycle.cancel();
                if had_active_touches {
                    let touch = self.seat.get_touch().expect("seat has touch");
                    touch.cancel(self);
                }
            }
            InputEvent::TouchFrame { .. } => {
                let touch = self.seat.get_touch().expect("seat has touch");
                touch.frame(self);
            }
            InputEvent::TabletToolProximity { event } => {
                let Some((position, focus, tablet, tool)) = self.tablet_target(&event) else {
                    return;
                };
                match event.state() {
                    ProximityState::In => {
                        if let Some(focus) = focus {
                            tool.proximity_in(
                                position,
                                focus,
                                &tablet,
                                SERIAL_COUNTER.next_serial(),
                                event.time_msec(),
                            );
                        }
                    }
                    ProximityState::Out => tool.proximity_out(event.time_msec()),
                }
            }
            InputEvent::TabletToolAxis { event } => {
                let Some((position, focus, tablet, tool)) = self.tablet_target(&event) else {
                    return;
                };
                Self::queue_tablet_axes(&tool, &event);
                tool.motion(
                    position,
                    focus,
                    &tablet,
                    SERIAL_COUNTER.next_serial(),
                    event.time_msec(),
                );
            }
            InputEvent::TabletToolTip { event } => {
                let Some((position, focus, tablet, tool)) = self.tablet_target(&event) else {
                    return;
                };
                Self::queue_tablet_axes(&tool, &event);
                tool.motion(
                    position,
                    focus,
                    &tablet,
                    SERIAL_COUNTER.next_serial(),
                    event.time_msec(),
                );
                match event.tip_state() {
                    TabletToolTipState::Down => {
                        tool.tip_down(SERIAL_COUNTER.next_serial(), event.time_msec())
                    }
                    TabletToolTipState::Up => tool.tip_up(event.time_msec()),
                }
            }
            InputEvent::TabletToolButton { event } => {
                let Some((_position, _focus, _tablet, tool)) = self.tablet_target(&event) else {
                    return;
                };
                tool.button(
                    event.button(),
                    event.button_state(),
                    SERIAL_COUNTER.next_serial(),
                    event.time_msec(),
                );
            }
            _ => {}
        }
    }

    fn tablet_target<I, E>(&mut self, event: &E) -> Option<TabletTarget>
    where
        I: InputBackend,
        E: TabletToolEvent<I>,
    {
        let position = self.touch_position(event)?;
        let focus = self.surface_under(position);
        let tablet_seat = self.seat.tablet_seat();
        let tablet = tablet_seat.add_tablet::<Self>(
            &self.display_handle,
            &TabletDescriptor::from(&event.device()),
        );
        let display_handle = self.display_handle.clone();
        let tool = tablet_seat.add_tool(self, &display_handle, &event.tool());
        Some((position, focus, tablet, tool))
    }

    fn queue_tablet_axes<I, E>(tool: &TabletToolHandle, event: &E)
    where
        I: InputBackend,
        E: TabletToolEvent<I>,
    {
        if event.pressure_has_changed() {
            let pressure = event.pressure().clamp(0.0, 1.0);
            tool.pressure(pressure);
            tracing::info!(pressure, "forwarded native tablet pressure");
        }
        if event.distance_has_changed() {
            tool.distance(event.distance().clamp(0.0, 1.0));
        }
        if event.tilt_has_changed() {
            tool.tilt(event.tilt());
        }
        if event.rotation_has_changed() {
            tool.rotation(event.rotation());
        }
        if event.slider_has_changed() {
            tool.slider_position(event.slider_position().clamp(-1.0, 1.0));
        }
        if event.wheel_has_changed() {
            tool.wheel(event.wheel_delta(), event.wheel_delta_discrete());
        }
    }

    fn touch_position<I, E>(
        &self,
        event: &E,
    ) -> Option<smithay::utils::Point<f64, smithay::utils::Logical>>
    where
        I: InputBackend,
        E: AbsolutePositionEvent<I>,
    {
        let output = self.space.outputs().next()?;
        let geometry = self.space.output_geometry(output)?;
        Some(event.position_transformed(geometry.size) + geometry.loc.to_f64())
    }
}

#[cfg(test)]
mod tests {
    use smithay::backend::input::TouchSlot;

    use super::TouchLifecycle;

    fn slot(id: u32) -> TouchSlot {
        Some(id).into()
    }

    #[test]
    fn held_touch_slots_are_cancelled_and_suppressed_until_release() {
        let mut lifecycle = TouchLifecycle::default();
        lifecycle.begin_contact(slot(2));
        lifecycle.begin_contact(slot(7));

        assert_eq!(lifecycle.begin_quiesce(), 2);
        assert!(!lifecycle.can_forward_motion(slot(2)));
        assert!(!lifecycle.end_contact(slot(2)));
        assert!(!lifecycle.can_forward_motion(slot(7)));
        assert!(!lifecycle.end_contact(slot(7)));
    }

    #[test]
    fn fresh_down_reuses_a_suppressed_slot_as_a_new_contact() {
        let mut lifecycle = TouchLifecycle::default();
        lifecycle.begin_contact(slot(4));
        lifecycle.begin_quiesce();
        lifecycle.begin_contact(slot(4));

        assert!(lifecycle.can_forward_motion(slot(4)));
        assert!(lifecycle.end_contact(slot(4)));
    }

    #[test]
    fn contact_started_and_released_while_quiesced_never_leaks() {
        let mut lifecycle = TouchLifecycle::default();
        lifecycle.observe_quiesced_down(slot(9));
        assert!(!lifecycle.can_forward_motion(slot(9)));

        lifecycle.observe_quiesced_up(slot(9));
        assert!(!lifecycle.can_forward_motion(slot(9)));
    }
}
