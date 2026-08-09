use std::{fs::File, io::Write, os::fd::AsFd};

use linux_input_method::{ComposeEngine, Edit};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_keyboard_grab_v2, zwp_input_method_manager_v2, zwp_input_method_v2,
    zwp_input_popup_surface_v2,
};

const POPUP_WIDTH: i32 = 360;
const POPUP_HEIGHT: i32 = 48;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = State {
        running: true,
        ..State::default()
    };
    while state.running {
        queue.blocking_dispatch(&mut state)?;
    }
    Ok(())
}

#[derive(Default)]
struct State {
    running: bool,
    seat: Option<wl_seat::WlSeat>,
    manager: Option<zwp_input_method_manager_v2::ZwpInputMethodManagerV2>,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    input_method: Option<zwp_input_method_v2::ZwpInputMethodV2>,
    keyboard: Option<zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2>,
    popup: Option<zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2>,
    popup_surface: Option<wl_surface::WlSurface>,
    popup_buffer: Option<wl_buffer::WlBuffer>,
    engine: ComposeEngine,
    active: bool,
    serial: u32,
    cursor_rectangle: Option<(i32, i32, i32, i32)>,
}

impl State {
    fn try_initialize(&mut self, qh: &QueueHandle<Self>) {
        if self.input_method.is_none() {
            if let (Some(manager), Some(seat)) = (&self.manager, &self.seat) {
                let input_method = manager.get_input_method(seat, qh, ());
                self.keyboard = Some(input_method.grab_keyboard(qh, ()));
                self.input_method = Some(input_method);
                eprintln!("sos-ime: attached input-method-v2");
            }
        }
        if self.popup.is_none() {
            if let (Some(input_method), Some(compositor), Some(shm)) =
                (&self.input_method, &self.compositor, &self.shm)
            {
                let surface = compositor.create_surface(qh, ());
                let (buffer, file) = candidate_buffer(shm, qh, &self.engine);
                surface.attach(Some(&buffer), 0, 0);
                surface.damage_buffer(0, 0, POPUP_WIDTH, POPUP_HEIGHT);
                surface.commit();
                let popup = input_method.get_input_popup_surface(&surface, qh, ());
                drop(file);
                self.popup = Some(popup);
                self.popup_surface = Some(surface);
                self.popup_buffer = Some(buffer);
            }
        }
    }

    fn refresh_popup(&mut self, qh: &QueueHandle<Self>) {
        let (Some(shm), Some(surface)) = (&self.shm, &self.popup_surface) else {
            return;
        };
        let (buffer, file) = candidate_buffer(shm, qh, &self.engine);
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, POPUP_WIDTH, POPUP_HEIGHT);
        surface.commit();
        drop(file);
        self.popup_buffer = Some(buffer);
        if !self.engine.candidates().is_empty() {
            eprintln!(
                "sos-ime: candidates={:?} selected={:?}",
                self.engine.candidates(),
                self.engine.selected_candidate()
            );
        }
    }

    fn apply_edit(&mut self, edit: Edit, qh: &QueueHandle<Self>) {
        if !self.active {
            return;
        }
        let Some(input_method) = self.input_method.as_ref() else {
            return;
        };
        match edit {
            Edit::Preedit(text) => {
                let cursor = i32::try_from(text.len()).unwrap_or(i32::MAX);
                input_method.set_preedit_string(text, cursor, cursor);
            }
            Edit::Commit(text) => {
                input_method.set_preedit_string(String::new(), 0, 0);
                input_method.commit_string(text);
            }
            Edit::Clear => input_method.set_preedit_string(String::new(), 0, 0),
            Edit::None => return,
        }
        input_method.commit(self.serial);
        self.refresh_popup(qh);
    }

    fn key_pressed(&mut self, key: u32, qh: &QueueHandle<Self>) {
        let edit = match key {
            1 => self.engine.cancel(),
            14 => self.engine.backspace(),
            28 | 57 => self.engine.accept(),
            40 => self.engine.acute(),
            105 => self.engine.cursor_left(),
            106 => self.engine.cursor_right(),
            _ => evdev_letter(key)
                .map(|letter| self.engine.letter(letter))
                .unwrap_or(Edit::None),
        };
        self.apply_edit(edit, qh);
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(9), qh, ()))
                }
                "wl_compositor" if state.compositor.is_none() => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "wl_shm" if state.shm.is_none() => state.shm = Some(registry.bind(name, 1, qh, ())),
                "zwp_input_method_manager_v2" if state.manager.is_none() => {
                    state.manager = Some(registry.bind(name, 1, qh, ()))
                }
                _ => {}
            }
            state.try_initialize(qh);
        }
    }
}

impl Dispatch<zwp_input_method_v2::ZwpInputMethodV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &zwp_input_method_v2::ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v2::Event::Activate => {
                state.active = true;
                state.engine.cancel();
                eprintln!("sos-ime: activated");
            }
            zwp_input_method_v2::Event::Deactivate => {
                state.active = false;
                state.engine.cancel();
                state.refresh_popup(qh);
                eprintln!("sos-ime: deactivated");
            }
            zwp_input_method_v2::Event::SurroundingText {
                text,
                cursor,
                anchor,
            } => eprintln!(
                "sos-ime: surrounding bytes={} cursor={cursor} anchor={anchor}",
                text.len()
            ),
            zwp_input_method_v2::Event::Done => state.serial = state.serial.wrapping_add(1),
            zwp_input_method_v2::Event::Unavailable => {
                eprintln!("sos-ime: unavailable");
                state.running = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
        event: zwp_input_popup_surface_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let zwp_input_popup_surface_v2::Event::TextInputRectangle {
            x,
            y,
            width,
            height,
        } = event
        else {
            return;
        };
        state.cursor_rectangle = Some((x, y, width, height));
        eprintln!("sos-ime: cursor-rectangle={x},{y} {width}x{height}");
    }
}

impl Dispatch<zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2,
        event: zwp_input_method_keyboard_grab_v2::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let zwp_input_method_keyboard_grab_v2::Event::Key {
            key,
            state: WEnum::Value(wl_keyboard::KeyState::Pressed),
            ..
        } = event
        {
            state.key_pressed(key, qh);
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore zwp_input_method_manager_v2::ZwpInputMethodManagerV2);

fn evdev_letter(key: u32) -> Option<char> {
    Some(match key {
        16 => 'q',
        17 => 'w',
        18 => 'e',
        19 => 'r',
        20 => 't',
        21 => 'y',
        22 => 'u',
        23 => 'i',
        24 => 'o',
        25 => 'p',
        30 => 'a',
        31 => 's',
        32 => 'd',
        33 => 'f',
        34 => 'g',
        35 => 'h',
        36 => 'j',
        37 => 'k',
        38 => 'l',
        44 => 'z',
        45 => 'x',
        46 => 'c',
        47 => 'v',
        48 => 'b',
        49 => 'n',
        50 => 'm',
        _ => return None,
    })
}

fn candidate_buffer(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<State>,
    engine: &ComposeEngine,
) -> (wl_buffer::WlBuffer, File) {
    let size = usize::try_from(POPUP_WIDTH * POPUP_HEIGHT * 4).unwrap();
    let mut file = tempfile::tempfile().expect("create IME shared-memory buffer");
    let selected = engine
        .selected_candidate()
        .and_then(|candidate| {
            engine
                .candidates()
                .iter()
                .position(|item| item == &candidate)
        })
        .unwrap_or(0);
    let count = engine.candidates().len().max(1);
    let mut pixels = vec![0u8; size];
    for y in 0..POPUP_HEIGHT as usize {
        for x in 0..POPUP_WIDTH as usize {
            let candidate = (x * count / POPUP_WIDTH as usize).min(count - 1);
            let highlighted = !engine.candidates().is_empty() && candidate == selected;
            let color = if highlighted {
                [0x52, 0x91, 0xff, 0xff]
            } else {
                [0x28, 0x2d, 0x35, 0xf2]
            };
            let offset = (y * POPUP_WIDTH as usize + x) * 4;
            pixels[offset..offset + 4].copy_from_slice(&color);
        }
    }
    file.write_all(&pixels).expect("draw IME candidate buffer");
    file.flush().expect("flush IME candidate buffer");
    let pool = shm.create_pool(file.as_fd(), i32::try_from(size).unwrap(), qh, ());
    let buffer = pool.create_buffer(
        0,
        POPUP_WIDTH,
        POPUP_HEIGHT,
        POPUP_WIDTH * 4,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    pool.destroy();
    (buffer, file)
}
