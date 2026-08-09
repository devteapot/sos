#![cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux;

pub use linux::current_platform;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawTouchPhase {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug)]
pub struct RawTouchEvent {
    pub id: i32,
    pub phase: RawTouchPhase,
    pub x: f32,
    pub y: f32,
    pub pointer_count: usize,
    pub pressure: Option<f32>,
    pub time_millis: u32,
}

type RawTouchCallback = Box<dyn FnMut(RawTouchEvent) + Send + 'static>;
static RAW_TOUCH_CALLBACK: OnceLock<Mutex<Option<RawTouchCallback>>> = OnceLock::new();

pub fn set_raw_touch_callback(callback: Option<RawTouchCallback>) {
    *RAW_TOUCH_CALLBACK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("raw touch callback lock") = callback;
}

pub(crate) fn dispatch_raw_touch(event: RawTouchEvent) {
    if let Some(callback) = RAW_TOUCH_CALLBACK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("raw touch callback lock")
        .as_mut()
    {
        callback(event);
    }
}
