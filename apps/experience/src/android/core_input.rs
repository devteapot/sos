use std::{
    ffi::CStr,
    fs::{File, OpenOptions},
    io::Read,
    mem::size_of,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use gpui_mobile::android::{AndroidKeyEvent, AndroidPlatform, TouchPoint};

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const KEY_VOLUMEDOWN: u16 = 114;
const KEY_VOLUMEUP: u16 = 115;
const KEY_POWER: u16 = 116;
const ANDROID_KEYCODE_VOLUME_UP: i32 = 24;
const ANDROID_KEYCODE_VOLUME_DOWN: i32 = 25;
const ANDROID_KEYCODE_POWER: i32 = 26;
const MAX_TOUCH_SLOTS: usize = 10;
const RECOVERY_CHORD_HOLD: Duration = Duration::from_secs(2);

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

#[derive(Clone, Copy)]
struct TouchSlot {
    tracking_id: i32,
    x: f32,
    y: f32,
    down_pending: bool,
    up_pending: bool,
    dirty: bool,
}

impl Default for TouchSlot {
    fn default() -> Self {
        Self {
            tracking_id: -1,
            x: 0.0,
            y: 0.0,
            down_pending: false,
            up_pending: false,
            dirty: false,
        }
    }
}

pub fn start(platform: Arc<AndroidPlatform>) -> Result<(), String> {
    let touch = open_named_input("sec_touchscreen")
        .ok_or_else(|| "Core touchscreen sec_touchscreen is unavailable".to_owned())?;
    grab_input(&touch, "sec_touchscreen")?;
    let volume = open_named_input("gpio_keys")
        .ok_or_else(|| "Core volume device gpio_keys is unavailable".to_owned())?;
    grab_input(&volume, "gpio_keys")?;
    let power = open_named_input("sec-pmic-key")
        .ok_or_else(|| "Core power device sec-pmic-key is unavailable".to_owned())?;

    let touch_platform = Arc::clone(&platform);
    thread::Builder::new()
        .name("sos-core-touch".into())
        .spawn(move || run_touch(touch_platform, touch))
        .map_err(|error| format!("Core touchscreen thread failed to start: {error}"))?;
    thread::Builder::new()
        .name("sos-core-keys".into())
        .spawn(move || run_keys(platform, volume, power))
        .map_err(|error| format!("Core key thread failed to start: {error}"))?;
    Ok(())
}

fn run_touch(platform: Arc<AndroidPlatform>, mut input: File) {
    log::info!("core_input_ready device=sec_touchscreen mode=exclusive");
    let mut slots = [TouchSlot::default(); MAX_TOUCH_SLOTS];
    let mut current_slot = 0usize;
    loop {
        let Some(event) = read_event(&mut input) else {
            log::error!("core_input_stopped device=sec_touchscreen");
            return;
        };
        match (event.kind, event.code) {
            (EV_ABS, ABS_MT_SLOT) => {
                current_slot = (event.value.max(0) as usize).min(MAX_TOUCH_SLOTS - 1);
            }
            (EV_ABS, ABS_MT_TRACKING_ID) => {
                let slot = &mut slots[current_slot];
                if event.value < 0 {
                    slot.up_pending = slot.tracking_id >= 0;
                } else {
                    slot.tracking_id = event.value;
                    slot.down_pending = true;
                    slot.up_pending = false;
                }
                slot.dirty = true;
            }
            (EV_ABS, ABS_MT_POSITION_X) => {
                slots[current_slot].x = event.value.max(0) as f32;
                slots[current_slot].dirty = true;
            }
            (EV_ABS, ABS_MT_POSITION_Y) => {
                slots[current_slot].y = event.value.max(0) as f32;
                slots[current_slot].dirty = true;
            }
            (EV_SYN, SYN_REPORT) => flush_touch_frame(&platform, &mut slots),
            _ => {}
        }
    }
}

fn flush_touch_frame(platform: &Arc<AndroidPlatform>, slots: &mut [TouchSlot; MAX_TOUCH_SLOTS]) {
    let pointer_count = slots.iter().filter(|slot| slot.tracking_id >= 0).count();
    for slot in slots.iter_mut().filter(|slot| slot.dirty) {
        let action = if slot.down_pending {
            0
        } else if slot.up_pending {
            1
        } else if slot.tracking_id >= 0 {
            2
        } else {
            slot.dirty = false;
            continue;
        };
        let point = TouchPoint {
            id: slot.tracking_id.max(0),
            x: slot.x,
            y: slot.y,
            pressure: if slot.up_pending { 0.0 } else { 1.0 },
            action,
            pointer_count: pointer_count.max(1),
        };
        dispatch_touch(platform, point);
        if slot.up_pending {
            slot.tracking_id = -1;
        }
        slot.down_pending = false;
        slot.up_pending = false;
        slot.dirty = false;
    }
}

fn run_keys(platform: Arc<AndroidPlatform>, volume: File, power: File) {
    let mut inputs = [volume, power];
    log::info!(
        "core_input_ready device=gpio_keys mode=exclusive recovery_chord=volume-up+volume-down"
    );
    log::info!("core_input_ready device=sec-pmic-key mode=observe owner=android-power");
    let mut volume_up = false;
    let mut volume_down = false;
    let mut chord_started = None;
    let mut recovery_sent = false;
    loop {
        let mut poll_fds = [
            libc::pollfd {
                fd: inputs[0].as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: inputs[1].as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let result = unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as _, 100) };
        if result < 0 {
            log::error!(
                "core_input_stopped device=keys error={}",
                std::io::Error::last_os_error()
            );
            return;
        }
        for (index, polled) in poll_fds.iter().enumerate() {
            if polled.revents & libc::POLLIN == 0 {
                continue;
            }
            let Some(event) = read_event(&mut inputs[index]) else {
                log::error!("core_input_stopped device=keys index={index}");
                return;
            };
            if event.kind != EV_KEY || !matches!(event.value, 0 | 1) {
                continue;
            }
            let pressed = event.value == 1;
            let android_code = match event.code {
                KEY_VOLUMEUP => {
                    volume_up = pressed;
                    ANDROID_KEYCODE_VOLUME_UP
                }
                KEY_VOLUMEDOWN => {
                    volume_down = pressed;
                    ANDROID_KEYCODE_VOLUME_DOWN
                }
                KEY_POWER => ANDROID_KEYCODE_POWER,
                _ => continue,
            };
            dispatch_key(
                &platform,
                AndroidKeyEvent {
                    key_code: android_code,
                    action: if pressed { 0 } else { 1 },
                    meta_state: 0,
                    unicode_char: 0,
                },
            );
        }
        if volume_up && volume_down {
            let started = chord_started.get_or_insert_with(Instant::now);
            if !recovery_sent && started.elapsed() >= RECOVERY_CHORD_HOLD {
                recovery_sent = true;
                log::warn!("core_recovery_chord action=return-to-android");
                super::request_core_recovery();
            }
        } else {
            chord_started = None;
            recovery_sent = false;
        }
    }
}

fn dispatch_touch(platform: &Arc<AndroidPlatform>, point: TouchPoint) {
    let Some(window) = platform.primary_window() else {
        return;
    };
    platform
        .background_executor()
        .dispatch_on_main_thread(move || window.handle_touch(point));
}

fn dispatch_key(platform: &Arc<AndroidPlatform>, event: AndroidKeyEvent) {
    let Some(window) = platform.primary_window() else {
        return;
    };
    platform
        .background_executor()
        .dispatch_on_main_thread(move || window.handle_key_event(event));
}

fn open_named_input(expected: &str) -> Option<File> {
    (0..32).find_map(|index| {
        let path = format!("/dev/input/event{index}");
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(&path)
            .ok()?;
        (input_name(file.as_raw_fd()).as_deref() == Some(expected)).then_some(file)
    })
}

fn grab_input(input: &File, name: &str) -> Result<(), String> {
    let request = iow(b'E', 0x90, size_of::<libc::c_int>());
    let result = unsafe { libc::ioctl(input.as_raw_fd(), request, 1) };
    if result == 0 {
        Ok(())
    } else {
        Err(format!(
            "Core cannot exclusively own {name}: {}",
            std::io::Error::last_os_error()
        ))
    }
}

fn input_name(fd: i32) -> Option<String> {
    let mut name = [0 as libc::c_char; 256];
    let request = ior(b'E', 0x06, name.len());
    let result = unsafe { libc::ioctl(fd, request, name.as_mut_ptr()) };
    if result <= 0 {
        return None;
    }
    unsafe { CStr::from_ptr(name.as_ptr()) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn read_event(input: &mut File) -> Option<InputEvent> {
    let mut event = InputEvent::default();
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(
            (&mut event as *mut InputEvent).cast::<u8>(),
            size_of::<InputEvent>(),
        )
    };
    input.read_exact(bytes).ok()?;
    Some(event)
}

const fn ior(kind: u8, number: u8, size: usize) -> libc::c_int {
    const NR_BITS: u32 = 8;
    const TYPE_BITS: u32 = 8;
    const SIZE_BITS: u32 = 14;
    const NR_SHIFT: u32 = 0;
    const TYPE_SHIFT: u32 = NR_SHIFT + NR_BITS;
    const SIZE_SHIFT: u32 = TYPE_SHIFT + TYPE_BITS;
    const DIR_SHIFT: u32 = SIZE_SHIFT + SIZE_BITS;
    const READ: u32 = 2;
    ((READ << DIR_SHIFT)
        | ((kind as u32) << TYPE_SHIFT)
        | ((number as u32) << NR_SHIFT)
        | ((size as u32) << SIZE_SHIFT)) as libc::c_int
}

const fn iow(kind: u8, number: u8, size: usize) -> libc::c_int {
    const NR_BITS: u32 = 8;
    const TYPE_BITS: u32 = 8;
    const SIZE_BITS: u32 = 14;
    const NR_SHIFT: u32 = 0;
    const TYPE_SHIFT: u32 = NR_SHIFT + NR_BITS;
    const SIZE_SHIFT: u32 = TYPE_SHIFT + TYPE_BITS;
    const DIR_SHIFT: u32 = SIZE_SHIFT + SIZE_BITS;
    const WRITE: u32 = 1;
    ((WRITE << DIR_SHIFT)
        | ((kind as u32) << TYPE_SHIFT)
        | ((number as u32) << NR_SHIFT)
        | ((size as u32) << SIZE_SHIFT)) as libc::c_int
}
