// Input forwarding is adapted from Smithay's MIT-licensed `smallvil` example
// at tag v0.7.0, with SOS focus ordering and activation quiescing.

use std::collections::{BTreeMap, HashSet};

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Device as _, Event, InputBackend,
        InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
        PointerMotionEvent, ProximityState, TabletToolButtonEvent, TabletToolEvent,
        TabletToolProximityEvent, TabletToolTipEvent, TabletToolTipState, TouchEvent, TouchSlot,
    },
    input::{
        keyboard::{FilterResult, Keysym, ModifiersState},
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct OutputBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum AbsoluteOutputRoute {
    Output(String),
    NoOutputs,
    Ambiguous,
    ConfiguredOutputMissing(String),
}

fn resolve_absolute_output(
    device_name: &str,
    mappings: &BTreeMap<String, String>,
    output_names: &[String],
) -> AbsoluteOutputRoute {
    if output_names.is_empty() {
        return AbsoluteOutputRoute::NoOutputs;
    }
    if let Some(configured) = mappings.get(device_name) {
        return if output_names.iter().any(|output| output == configured) {
            AbsoluteOutputRoute::Output(configured.clone())
        } else {
            AbsoluteOutputRoute::ConfiguredOutputMissing(configured.clone())
        };
    }
    if output_names.len() == 1 {
        AbsoluteOutputRoute::Output(output_names[0].clone())
    } else {
        AbsoluteOutputRoute::Ambiguous
    }
}

fn clamp_to_output_layout(position: (f64, f64), outputs: &[OutputBounds]) -> Option<(f64, f64)> {
    outputs
        .iter()
        .filter(|output| output.width > 0.0 && output.height > 0.0)
        .map(|output| {
            let maximum_x = output.x + output.width - 1.0;
            let maximum_y = output.y + output.height - 1.0;
            let candidate = (
                position.0.clamp(output.x, maximum_x),
                position.1.clamp(output.y, maximum_y),
            );
            let distance = (candidate.0 - position.0).powi(2) + (candidate.1 - position.1).powi(2);
            (candidate, distance)
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(candidate, _)| candidate)
}

fn is_session_exit_shortcut(
    enabled: bool,
    state: KeyState,
    modifiers: &ModifiersState,
    keysym: Keysym,
) -> bool {
    enabled
        && state == KeyState::Pressed
        && modifiers.ctrl
        && modifiers.alt
        && keysym == Keysym::BackSpace
}

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
    #[cfg_attr(not(feature = "direct-backend"), allow(dead_code))]
    pub fn set_input_output_mappings(&mut self, mappings: BTreeMap<String, String>) {
        self.input_output_mappings = mappings;
        self.observed_input_routes.clear();
    }

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
                let state = event.state();
                let key_code = event.key_code();
                let session_exit_enabled = self.session_exit_enabled;
                let keyboard = self.seat.get_keyboard().expect("seat has a keyboard");
                let session_exit = keyboard.input::<bool, _>(
                    self,
                    key_code,
                    state,
                    serial,
                    time,
                    |_, modifiers, key| {
                        if is_session_exit_shortcut(
                            session_exit_enabled,
                            state,
                            modifiers,
                            key.modified_sym(),
                        ) {
                            FilterResult::Intercept(true)
                        } else {
                            FilterResult::Forward
                        }
                    },
                );
                if session_exit == Some(true) {
                    self.suppressed_keyboard_keys.insert(key_code);
                    self.session_exit_requested = true;
                    tracing::info!("selectable SOS login session requested logout");
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let pointer = self.seat.get_pointer().expect("seat has a pointer");
                let current = pointer.current_location();
                let candidate = current + event.delta();
                let outputs = if self.output_layout_mirrored {
                    vec![OutputBounds {
                        x: 0.0,
                        y: 0.0,
                        width: f64::from(self.output_size.0),
                        height: f64::from(self.output_size.1),
                    }]
                } else {
                    self.space
                        .outputs()
                        .filter_map(|output| self.space.output_geometry(output))
                        .map(|geometry| OutputBounds {
                            x: f64::from(geometry.loc.x),
                            y: f64::from(geometry.loc.y),
                            width: f64::from(geometry.size.w),
                            height: f64::from(geometry.size.h),
                        })
                        .collect::<Vec<_>>()
                };
                let Some(location) =
                    clamp_to_output_layout((candidate.x, candidate.y), &outputs).map(Into::into)
                else {
                    return;
                };
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
                self.update_shell_overlay_drag(location);
                self.update_application_window_drag(location);
                self.update_shell_overlay_hover(location);
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
                let Some(output_geometry) = self.absolute_output_geometry(&event.device()) else {
                    return;
                };
                let position = self.canonical_absolute_position(
                    event.position_transformed(output_geometry.size) + output_geometry.loc.to_f64(),
                    output_geometry,
                );
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
                self.update_shell_overlay_drag(position);
                self.update_application_window_drag(position);
                self.update_shell_overlay_hover(position);
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
                        if event.button_code() == 0x110 {
                            self.finish_shell_overlay_drag();
                            self.finish_application_window_drag();
                        }
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
                        .window_under(pointer.current_location())
                        .map(|(window, _)| window.clone());
                    if let Some(window) = focused {
                        let Some(surface) = window.wl_surface().map(|surface| surface.into_owned())
                        else {
                            return;
                        };
                        if window.is_x11()
                            || Self::client_role(&surface).is_some_and(|role| {
                                matches!(
                                    role,
                                    ClientRole::NativeApplication | ClientRole::Compatibility
                                )
                            })
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
        &mut self,
        event: &E,
    ) -> Option<smithay::utils::Point<f64, smithay::utils::Logical>>
    where
        I: InputBackend,
        E: AbsolutePositionEvent<I>,
    {
        let geometry = self.absolute_output_geometry(&event.device())?;
        Some(self.canonical_absolute_position(
            event.position_transformed(geometry.size) + geometry.loc.to_f64(),
            geometry,
        ))
    }

    fn canonical_absolute_position(
        &self,
        position: smithay::utils::Point<f64, smithay::utils::Logical>,
        output: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
    ) -> smithay::utils::Point<f64, smithay::utils::Logical> {
        #[cfg(feature = "direct-backend")]
        if self.output_layout_mirrored {
            let projection = crate::direct::mirror_projection(
                self.output_size,
                (output.size.w.max(0), output.size.h.max(0)),
            );
            let x = (position.x - f64::from(output.loc.x + projection.offset.0)) / projection.scale;
            let y = (position.y - f64::from(output.loc.y + projection.offset.1)) / projection.scale;
            return (
                x.clamp(0.0, f64::from((self.output_size.0 - 1).max(0))),
                y.clamp(0.0, f64::from((self.output_size.1 - 1).max(0))),
            )
                .into();
        }
        position
    }

    fn absolute_output_geometry<D>(
        &mut self,
        device: &D,
    ) -> Option<smithay::utils::Rectangle<i32, smithay::utils::Logical>>
    where
        D: smithay::backend::input::Device,
    {
        let outputs = self.space.outputs().cloned().collect::<Vec<_>>();
        let output_names = outputs
            .iter()
            .map(|output| output.name())
            .collect::<Vec<_>>();
        let device_name = device.name();
        let route =
            resolve_absolute_output(&device_name, &self.input_output_mappings, &output_names);
        let route_key = format!("{device_name}:{route:?}");
        let first_observation = self.observed_input_routes.insert(route_key);
        match route {
            AbsoluteOutputRoute::Output(selected) => {
                if first_observation {
                    tracing::info!(
                        device_name,
                        output = selected,
                        "routed absolute input device to output"
                    );
                }
                outputs
                    .iter()
                    .find(|output| output.name() == selected)
                    .and_then(|output| self.space.output_geometry(output))
            }
            AbsoluteOutputRoute::NoOutputs => {
                if first_observation {
                    tracing::warn!(device_name, "ignored absolute input without an output");
                }
                None
            }
            AbsoluteOutputRoute::Ambiguous => {
                if first_observation {
                    tracing::warn!(
                        device_name,
                        ?output_names,
                        "ignored ambiguous absolute input; configure input_outputs"
                    );
                }
                None
            }
            AbsoluteOutputRoute::ConfiguredOutputMissing(configured) => {
                if first_observation {
                    tracing::warn!(
                        device_name,
                        output = configured,
                        ?output_names,
                        "ignored absolute input because its configured output is absent"
                    );
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use smithay::{
        backend::input::{KeyState, TouchSlot},
        input::keyboard::{Keysym, ModifiersState},
    };

    use super::{
        clamp_to_output_layout, is_session_exit_shortcut, resolve_absolute_output,
        AbsoluteOutputRoute, OutputBounds, TouchLifecycle,
    };

    fn slot(id: u32) -> TouchSlot {
        Some(id).into()
    }

    #[test]
    fn configured_absolute_device_ignores_connector_discovery_order() {
        let mappings = BTreeMap::from([
            ("PiKVM PiKVM Composite Device".into(), "DP-1".into()),
            ("Integrated Touchscreen".into(), "eDP-1".into()),
        ]);
        for outputs in [
            vec!["eDP-1".into(), "DP-1".into()],
            vec!["DP-1".into(), "eDP-1".into()],
        ] {
            assert_eq!(
                resolve_absolute_output("PiKVM PiKVM Composite Device", &mappings, &outputs),
                AbsoluteOutputRoute::Output("DP-1".into())
            );
            assert_eq!(
                resolve_absolute_output("Integrated Touchscreen", &mappings, &outputs),
                AbsoluteOutputRoute::Output("eDP-1".into())
            );
        }
    }

    #[test]
    fn ambiguous_absolute_device_fails_closed_but_single_output_is_automatic() {
        let mappings = BTreeMap::from([("Mapped tablet".into(), "DP-1".into())]);
        assert_eq!(
            resolve_absolute_output("Tablet", &mappings, &[]),
            AbsoluteOutputRoute::NoOutputs
        );
        assert_eq!(
            resolve_absolute_output("Tablet", &mappings, &["eDP-1".into()]),
            AbsoluteOutputRoute::Output("eDP-1".into())
        );
        assert_eq!(
            resolve_absolute_output("Tablet", &mappings, &["DP-1".into(), "eDP-1".into()]),
            AbsoluteOutputRoute::Ambiguous
        );
        assert_eq!(
            resolve_absolute_output("Mapped tablet", &mappings, &["eDP-1".into()]),
            AbsoluteOutputRoute::ConfiguredOutputMissing("DP-1".into())
        );
    }

    #[test]
    fn relative_pointer_traverses_outputs_and_avoids_layout_gaps() {
        let outputs = [
            OutputBounds {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            OutputBounds {
                x: 1920.0,
                y: 0.0,
                width: 1920.0,
                height: 1200.0,
            },
        ];
        assert_eq!(
            clamp_to_output_layout((1929.0, 400.0), &outputs),
            Some((1929.0, 400.0))
        );
        assert_eq!(
            clamp_to_output_layout((100.0, 1150.0), &outputs),
            Some((100.0, 1079.0))
        );
        assert_eq!(
            clamp_to_output_layout((9000.0, 9000.0), &outputs),
            Some((3839.0, 1199.0))
        );
    }

    #[test]
    fn relative_pointer_stays_inside_the_mirrored_canvas() {
        let canvas = [OutputBounds {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        }];

        assert_eq!(
            clamp_to_output_layout((100.0, -60.0), &canvas),
            Some((100.0, 0.0))
        );
        assert_eq!(
            clamp_to_output_layout((100.0, 1139.0), &canvas),
            Some((100.0, 1079.0))
        );
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

    #[test]
    fn selectable_session_exit_requires_the_complete_chord() {
        let modifiers = ModifiersState {
            ctrl: true,
            alt: true,
            ..Default::default()
        };
        assert!(is_session_exit_shortcut(
            true,
            KeyState::Pressed,
            &modifiers,
            Keysym::BackSpace,
        ));
        assert!(!is_session_exit_shortcut(
            false,
            KeyState::Pressed,
            &modifiers,
            Keysym::BackSpace,
        ));
        assert!(!is_session_exit_shortcut(
            true,
            KeyState::Released,
            &modifiers,
            Keysym::BackSpace,
        ));
        assert!(!is_session_exit_shortcut(
            true,
            KeyState::Pressed,
            &ModifiersState {
                ctrl: true,
                ..Default::default()
            },
            Keysym::BackSpace,
        ));
    }
}
