#![cfg_attr(not(feature = "direct-backend"), allow(dead_code))]

use std::{fs, os::unix::net::UnixDatagram, path::PathBuf};

use font8x8::{UnicodeFonts, BASIC_FONTS};
use serde::Deserialize;
use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::Transform,
};

pub const WIDTH: i32 = 720;
pub const HEIGHT: i32 = 420;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RecoveryStatus {
    current_revision: String,
    previous_revision: String,
    failure_reason: String,
    progress: String,
    safe_mode: bool,
    providers_disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RecoveryAction {
    Restart,
    Rollback,
    SafeMode,
    DisableProviders,
}

impl RecoveryAction {
    fn name(&self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::Rollback => "rollback",
            Self::SafeMode => "safe_mode",
            Self::DisableProviders => "disable_providers",
        }
    }
}

pub struct RecoveryUi {
    status_file: Option<PathBuf>,
    command_socket: Option<PathBuf>,
}

impl RecoveryUi {
    pub fn from_environment() -> Self {
        Self {
            status_file: std::env::var_os("SOS_RECOVERY_STATE_FILE").map(PathBuf::from),
            command_socket: std::env::var_os("SOS_RECOVERY_COMMAND_SOCKET").map(PathBuf::from),
        }
    }

    pub fn buffer(&self) -> MemoryRenderBuffer {
        let status = self.read_status();
        let mut pixels = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
        fill(&mut pixels, 0, 0, WIDTH, HEIGHT, [17, 21, 29, 255]);
        fill(
            &mut pixels,
            24,
            24,
            WIDTH - 48,
            HEIGHT - 48,
            [35, 42, 55, 255],
        );
        draw_text(&mut pixels, 48, 48, "SOS RECOVERY", [240, 243, 250, 255], 3);
        draw_text(
            &mut pixels,
            48,
            94,
            &format!("CURRENT: {}", short(&status.current_revision, "UNKNOWN")),
            [203, 211, 225, 255],
            2,
        );
        draw_text(
            &mut pixels,
            48,
            118,
            &format!("PREVIOUS: {}", short(&status.previous_revision, "NONE")),
            [203, 211, 225, 255],
            2,
        );
        draw_text(
            &mut pixels,
            48,
            154,
            &format!(
                "FAILURE: {}",
                short(&status.failure_reason, "SHELL NOT RUNNING")
            ),
            [255, 174, 164, 255],
            2,
        );
        draw_text(
            &mut pixels,
            48,
            190,
            &format!("PROGRESS: {}", short(&status.progress, "IDLE")),
            [165, 215, 255, 255],
            2,
        );
        button(&mut pixels, 40, 140, "RESTART", false);
        button(&mut pixels, 200, 140, "ROLLBACK", false);
        button(&mut pixels, 360, 160, "SAFE MODE", status.safe_mode);
        button(
            &mut pixels,
            540,
            140,
            "NO PROVIDERS",
            status.providers_disabled,
        );
        draw_text(
            &mut pixels,
            48,
            386,
            "SELECT A RECOVERY ACTION",
            [160, 170, 190, 255],
            1,
        );
        MemoryRenderBuffer::from_slice(
            &pixels,
            Fourcc::Argb8888,
            (WIDTH, HEIGHT),
            1,
            Transform::Normal,
            None,
        )
    }

    pub fn click(&self, location: (f64, f64), output: (i32, i32)) -> bool {
        let origin = ((output.0 - WIDTH) / 2, (output.1 - HEIGHT) / 2);
        let local = (
            location.0.round() as i32 - origin.0,
            location.1.round() as i32 - origin.1,
        );
        let Some(action) = hit_action(local) else {
            return false;
        };
        self.send(&action);
        true
    }

    fn read_status(&self) -> RecoveryStatus {
        self.status_file
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .filter(|bytes| bytes.len() <= 64 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn send(&self, action: &RecoveryAction) {
        let Some(path) = self.command_socket.as_deref() else {
            tracing::warn!(
                action = action.name(),
                "recovery action has no lifecycle socket"
            );
            return;
        };
        let payload = format!("{{\"action\":\"{}\"}}\n", action.name());
        match UnixDatagram::unbound().and_then(|socket| socket.send_to(payload.as_bytes(), path)) {
            Ok(_) => tracing::info!(action = action.name(), "sent recovery action"),
            Err(error) => tracing::warn!(%error, action = action.name(), "recovery action failed"),
        }
    }
}

fn hit_action((x, y): (i32, i32)) -> Option<RecoveryAction> {
    if !(310..=368).contains(&y) {
        return None;
    }
    match x {
        40..=180 => Some(RecoveryAction::Restart),
        200..=340 => Some(RecoveryAction::Rollback),
        360..=520 => Some(RecoveryAction::SafeMode),
        540..=680 => Some(RecoveryAction::DisableProviders),
        _ => None,
    }
}

fn short<'a>(value: &'a str, fallback: &'a str) -> String {
    let value = if value.is_empty() { fallback } else { value };
    value.chars().take(42).collect::<String>().to_uppercase()
}

fn button(pixels: &mut [u8], x: i32, width: i32, label: &str, active: bool) {
    fill(
        pixels,
        x,
        310,
        width,
        58,
        if active {
            [55, 125, 91, 255]
        } else {
            [60, 76, 102, 255]
        },
    );
    draw_text(pixels, x + 10, 329, label, [255, 255, 255, 255], 1);
}

fn draw_text(pixels: &mut [u8], mut x: i32, y: i32, value: &str, color: [u8; 4], scale: i32) {
    for character in value.chars() {
        if let Some(glyph) = BASIC_FONTS.get(character) {
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..8 {
                    if bits & (1 << column) != 0 {
                        fill(
                            pixels,
                            x + column * scale,
                            y + row as i32 * scale,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
        }
        x += 9 * scale;
        if x >= WIDTH - 20 {
            break;
        }
    }
}

fn fill(pixels: &mut [u8], x: i32, y: i32, width: i32, height: i32, color: [u8; 4]) {
    for row in y.max(0)..(y + height).min(HEIGHT) {
        for column in x.max(0)..(x + width).min(WIDTH) {
            let offset = ((row * WIDTH + column) * 4) as usize;
            pixels[offset..offset + 4].copy_from_slice(&color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_buttons_are_bounded_and_named() {
        assert_eq!(hit_action((50, 330)), Some(RecoveryAction::Restart));
        assert_eq!(hit_action((250, 330)), Some(RecoveryAction::Rollback));
        assert_eq!(hit_action((400, 330)), Some(RecoveryAction::SafeMode));
        assert_eq!(
            hit_action((600, 330)),
            Some(RecoveryAction::DisableProviders)
        );
        assert_eq!(hit_action((10, 10)), None);
    }
}
