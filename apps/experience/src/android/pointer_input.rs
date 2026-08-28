use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, OnceLock},
    time::Instant,
};

use experience_ir::{Interaction, PointerCapture, SceneEvent};
use gpui::{Bounds, Pixels};

const MAX_POINTER_SAMPLES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub id: u64,
    pub phase: Phase,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub pointer_count: usize,
    pub event_time_nanos: u64,
}

#[derive(Clone)]
struct Surface {
    id: String,
    bounds: [f32; 4],
    interaction: Interaction,
    epoch: u64,
    order: u64,
}

#[derive(Clone)]
struct NativeInput {
    id: String,
    bounds: [f32; 4],
    epoch: u64,
    order: u64,
}

#[derive(Clone)]
struct Capture {
    surface: Surface,
    target: String,
    start_x: f32,
    start_y: f32,
    last_x: f32,
    last_y: f32,
    last_time_nanos: u64,
}

#[derive(Default)]
struct Router {
    epoch: u64,
    order: u64,
    surfaces: HashMap<String, Surface>,
    native_inputs: HashMap<String, NativeInput>,
    captures: HashMap<u64, Capture>,
}

static SAMPLES: OnceLock<Mutex<VecDeque<Sample>>> = OnceLock::new();
static ROUTER: OnceLock<Mutex<Router>> = OnceLock::new();
static CLOCK: OnceLock<Instant> = OnceLock::new();

#[cfg(target_os = "android")]
pub fn install() {
    let start = *CLOCK.get_or_init(Instant::now);
    gpui_mobile::set_raw_touch_callback(Some(Box::new(move |event| {
        let phase = match event.action {
            0 => Phase::Down,
            2 => Phase::Move,
            1 => Phase::Up,
            _ => Phase::Cancel,
        };
        if phase != Phase::Move {
            log::info!(
                "scene_pointer phase={} id={} count={} pressure={:.3}",
                phase_name(phase),
                event.id,
                event.pointer_count,
                event.pressure
            );
        }
        push_sample(Sample {
            id: event.id.max(0) as u64,
            phase,
            x: event.x,
            y: event.y,
            pressure: event.pressure.clamp(0.0, 1.0),
            pointer_count: event.pointer_count.min(32),
            event_time_nanos: start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        });
    })));
}

#[cfg(target_os = "linux")]
pub fn install() -> async_channel::Receiver<()> {
    let start = *CLOCK.get_or_init(Instant::now);
    let (wake_tx, wake_rx) = async_channel::bounded(1);
    gpui_linux::set_raw_touch_callback(Some(Box::new(move |event| {
        let phase = match event.phase {
            gpui_linux::RawTouchPhase::Down => Phase::Down,
            gpui_linux::RawTouchPhase::Move => Phase::Move,
            gpui_linux::RawTouchPhase::Up => Phase::Up,
            gpui_linux::RawTouchPhase::Cancel => Phase::Cancel,
        };
        if phase != Phase::Move {
            log::info!(
                "scene_pointer platform=linux phase={} id={} count={} pressure={}",
                phase_name(phase),
                event.id,
                event.pointer_count,
                event
                    .pressure
                    .map(|pressure| format!("{pressure:.3}"))
                    .unwrap_or_else(|| "unavailable".into()),
            );
        }
        push_sample_and_wake(
            Sample {
                id: event.id.max(0) as u64,
                phase,
                x: event.x,
                y: event.y,
                pressure: event.pressure.unwrap_or(1.0).clamp(0.0, 1.0),
                pointer_count: event.pointer_count.min(32),
                event_time_nanos: start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            },
            &wake_tx,
        );
    })));
    wake_rx
}

pub fn begin_frame() {
    let mut router = ROUTER
        .get_or_init(|| Mutex::new(Router::default()))
        .lock()
        .expect("pointer router lock");
    begin_router_frame(&mut router);
}

fn begin_router_frame(router: &mut Router) {
    router.epoch = router.epoch.wrapping_add(1).max(1);
    let epoch = router.epoch;
    router
        .surfaces
        .retain(|_, surface| epoch.saturating_sub(surface.epoch) <= 2);
    router
        .native_inputs
        .retain(|_, input| epoch.saturating_sub(input.epoch) <= 2);
}

pub fn record_surface(id: &str, bounds: Bounds<Pixels>, interaction: &Interaction) {
    if interaction.pointer_action.is_none() && interaction.multi_pointer_action.is_none() {
        return;
    }
    let mut router = ROUTER
        .get_or_init(|| Mutex::new(Router::default()))
        .lock()
        .expect("pointer router lock");
    record_router_surface(&mut router, id, bounds, interaction);
}

fn record_router_surface(
    router: &mut Router,
    id: &str,
    bounds: Bounds<Pixels>,
    interaction: &Interaction,
) {
    router.order = router.order.wrapping_add(1).max(1);
    let mut interaction = interaction.clone();
    if let Some((namespace, _)) = id.split_once("::") {
        for region in &mut interaction.hit_regions {
            if !region.id.starts_with(&format!("{namespace}::")) {
                region.id = format!("{namespace}::{}", region.id);
            }
        }
    }
    let surface = Surface {
        id: id.to_owned(),
        bounds: [
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ],
        interaction,
        epoch: router.epoch,
        order: router.order,
    };
    router.surfaces.insert(id.to_owned(), surface);
}

pub fn record_native_input(id: &str, bounds: Bounds<Pixels>) {
    let mut router = ROUTER
        .get_or_init(|| Mutex::new(Router::default()))
        .lock()
        .expect("pointer router lock");
    router.order = router.order.wrapping_add(1).max(1);
    let input = NativeInput {
        id: id.to_owned(),
        bounds: [
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ],
        epoch: router.epoch,
        order: router.order,
    };
    router.native_inputs.insert(id.to_owned(), input);
}

pub fn native_input_at(x: f32, y: f32) -> Option<String> {
    let router = ROUTER
        .get_or_init(|| Mutex::new(Router::default()))
        .lock()
        .expect("pointer router lock");
    hit_native_input(&router, x, y).map(|input| input.id.clone())
}

pub fn take_samples() -> Vec<Sample> {
    SAMPLES
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .expect("pointer sample lock")
        .drain(..)
        .collect()
}

pub fn route(sample: Sample) -> Vec<SceneEvent> {
    let mut router = ROUTER
        .get_or_init(|| Mutex::new(Router::default()))
        .lock()
        .expect("pointer router lock");
    route_with_router(&mut router, sample)
}

fn route_with_router(router: &mut Router, sample: Sample) -> Vec<SceneEvent> {
    if sample.phase == Phase::Down {
        let surface_capture = router
            .captures
            .values()
            .find(|capture| capture.surface.interaction.capture == PointerCapture::Surface)
            .cloned();
        let capture = surface_capture
            .map(|capture| {
                let x = sample.x - capture.surface.bounds[0];
                let y = sample.y - capture.surface.bounds[1];
                Capture {
                    surface: capture.surface,
                    target: capture.target,
                    start_x: x,
                    start_y: y,
                    last_x: x,
                    last_y: y,
                    last_time_nanos: sample.event_time_nanos,
                }
            })
            .or_else(|| hit_surface(&router, sample.x, sample.y, sample.event_time_nanos));
        if let Some(capture) = capture {
            router.captures.insert(sample.id, capture);
        }
    }

    let Some(mut capture) = router.captures.get(&sample.id).cloned() else {
        return Vec::new();
    };
    let inside = contains(capture.surface.bounds, sample.x, sample.y);
    let deliver = capture.surface.interaction.capture != PointerCapture::None || inside;
    let local_x = sample.x - capture.surface.bounds[0];
    let local_y = sample.y - capture.surface.bounds[1];
    let delta_x = local_x - capture.last_x;
    let delta_y = local_y - capture.last_y;
    let seconds = sample
        .event_time_nanos
        .saturating_sub(capture.last_time_nanos) as f32
        / 1_000_000_000.0;
    let seconds = seconds.max(0.001);
    capture.last_x = local_x;
    capture.last_y = local_y;
    capture.last_time_nanos = sample.event_time_nanos;
    router.captures.insert(sample.id, capture.clone());

    let mut events = Vec::new();
    if deliver {
        if let Some(action) = &capture.surface.interaction.pointer_action {
            events.push(SceneEvent {
                action: action.clone(),
                target: Some(capture.target.clone()),
                x: Some(local_x),
                y: Some(local_y),
                delta_x: Some(delta_x),
                delta_y: Some(delta_y),
                velocity_x: Some(delta_x / seconds),
                velocity_y: Some(delta_y / seconds),
                phase: Some(phase_name(sample.phase).into()),
                pointer_id: Some(sample.id),
                pointer_count: Some(sample.pointer_count),
                pressure: Some(sample.pressure),
                ..Default::default()
            });
        }
        if let Some(action) = &capture.surface.interaction.multi_pointer_action {
            let mut active = router
                .captures
                .iter()
                .filter(|(_, value)| value.surface.id == capture.surface.id)
                .collect::<Vec<_>>();
            active.sort_by_key(|(id, _)| **id);
            if active.len() >= 2 {
                let first = active[0].1;
                let second = active[1].1;
                let start_dx = second.start_x - first.start_x;
                let start_dy = second.start_y - first.start_y;
                let current_dx = second.last_x - first.last_x;
                let current_dy = second.last_y - first.last_y;
                let start_distance = start_dx.hypot(start_dy).max(0.001);
                let phase = match sample.phase {
                    Phase::Down if active.len() == 2 => "start",
                    Phase::Up | Phase::Cancel => "end",
                    _ => "update",
                };
                events.push(SceneEvent {
                    action: action.clone(),
                    target: Some(capture.surface.id.clone()),
                    x: Some((first.last_x + second.last_x) * 0.5),
                    y: Some((first.last_y + second.last_y) * 0.5),
                    phase: Some(phase.into()),
                    pointer_id: Some(sample.id),
                    pointer_count: Some(active.len()),
                    pressure: Some(sample.pressure),
                    scale: Some(current_dx.hypot(current_dy) / start_distance),
                    rotation_degrees: Some(
                        (current_dy.atan2(current_dx) - start_dy.atan2(start_dx)).to_degrees(),
                    ),
                    ..Default::default()
                });
            }
        }
    }

    if matches!(sample.phase, Phase::Up | Phase::Cancel) {
        router.captures.remove(&sample.id);
    }
    events
}

fn hit_surface(router: &Router, x: f32, y: f32, event_time_nanos: u64) -> Option<Capture> {
    let surface = router
        .surfaces
        .values()
        .filter(|surface| contains(surface.bounds, x, y))
        .max_by_key(|surface| surface.order)?
        .clone();
    let local_x = x - surface.bounds[0];
    let local_y = y - surface.bounds[1];
    let target = surface
        .interaction
        .hit_regions
        .iter()
        .rev()
        .find(|region| {
            local_x >= region.x
                && local_x <= region.x + region.width
                && local_y >= region.y
                && local_y <= region.y + region.height
        })
        .map(|region| region.id.clone())
        .unwrap_or_else(|| surface.id.clone());
    Some(Capture {
        surface,
        target,
        start_x: local_x,
        start_y: local_y,
        last_x: local_x,
        last_y: local_y,
        last_time_nanos: event_time_nanos,
    })
}

fn hit_native_input(router: &Router, x: f32, y: f32) -> Option<&NativeInput> {
    router
        .native_inputs
        .values()
        .filter(|input| contains(input.bounds, x, y))
        .max_by_key(|input| input.order)
}

fn contains(bounds: [f32; 4], x: f32, y: f32) -> bool {
    x >= bounds[0] && y >= bounds[1] && x <= bounds[0] + bounds[2] && y <= bounds[1] + bounds[3]
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Down => "down",
        Phase::Move => "move",
        Phase::Up => "up",
        Phase::Cancel => "cancel",
    }
}

fn push_sample(sample: Sample) {
    let mut samples = SAMPLES
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .expect("pointer sample lock");
    if samples.len() >= MAX_POINTER_SAMPLES {
        samples.pop_front();
    }
    samples.push_back(sample);
}

#[cfg(target_os = "linux")]
fn push_sample_and_wake(sample: Sample, wakes: &async_channel::Sender<()>) {
    push_sample(sample);
    // Raw Wayland touch bypasses GPUI's element input path. Mark the host
    // dirty through its foreground task instead of waiting for an unrelated
    // mouse/key event to trigger the next entity render.
    let _ = wakes.try_send(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    #[test]
    fn phase_names_are_stable() {
        assert_eq!(phase_name(Phase::Down), "down");
        assert_eq!(phase_name(Phase::Cancel), "cancel");
    }

    #[test]
    fn native_input_hit_uses_the_topmost_recorded_bounds() {
        let mut router = Router::default();
        router.native_inputs.insert(
            "note-draft".into(),
            NativeInput {
                id: "note-draft".into(),
                bounds: [10.0, 10.0, 80.0, 40.0],
                epoch: 1,
                order: 1,
            },
        );
        router.native_inputs.insert(
            "agent-prompt".into(),
            NativeInput {
                id: "agent-prompt".into(),
                bounds: [20.0, 20.0, 80.0, 40.0],
                epoch: 1,
                order: 2,
            },
        );

        assert_eq!(
            hit_native_input(&router, 25.0, 25.0).map(|input| input.id.as_str()),
            Some("agent-prompt")
        );
        assert_eq!(
            hit_native_input(&router, 15.0, 15.0).map(|input| input.id.as_str()),
            Some("note-draft")
        );
        assert!(hit_native_input(&router, 200.0, 200.0).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn raw_touch_queues_one_coalesced_host_wake() {
        take_samples();
        let (wake_tx, wake_rx) = async_channel::bounded(1);
        let sample = Sample {
            id: 1,
            phase: Phase::Down,
            x: 4.0,
            y: 8.0,
            pressure: 1.0,
            pointer_count: 1,
            event_time_nanos: 10,
        };
        push_sample_and_wake(sample, &wake_tx);
        push_sample_and_wake(sample, &wake_tx);

        assert!(wake_rx.try_recv().is_ok());
        assert!(wake_rx.try_recv().is_err());
        assert_eq!(take_samples().len(), 2);
    }

    #[test]
    fn surface_capture_routes_two_pointer_transform_until_release() {
        let mut router = Router::default();
        begin_router_frame(&mut router);
        record_router_surface(
            &mut router,
            "surface",
            Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(100.0), px(100.0)),
            },
            &Interaction {
                pointer_action: Some("pointer".into()),
                multi_pointer_action: Some("transform".into()),
                capture: PointerCapture::Surface,
                ..Default::default()
            },
        );
        let sample = |id, phase, x, y, count, time| Sample {
            id,
            phase,
            x,
            y,
            pressure: 0.5,
            pointer_count: count,
            event_time_nanos: time,
        };

        let first = route_with_router(&mut router, sample(1, Phase::Down, 10.0, 10.0, 1, 1));
        assert_eq!(first[0].pointer_id, Some(1));
        let second = route_with_router(&mut router, sample(2, Phase::Down, 30.0, 10.0, 2, 2));
        let start = second
            .iter()
            .find(|event| event.action == "transform")
            .unwrap();
        assert_eq!(start.phase.as_deref(), Some("start"));
        assert_eq!(start.pointer_count, Some(2));

        let moved = route_with_router(&mut router, sample(2, Phase::Move, 10.0, 30.0, 2, 3));
        let update = moved
            .iter()
            .find(|event| event.action == "transform")
            .unwrap();
        assert_eq!(update.phase.as_deref(), Some("update"));
        assert!((update.scale.unwrap() - 1.0).abs() < 0.001);
        assert!((update.rotation_degrees.unwrap() - 90.0).abs() < 0.001);

        let released = route_with_router(&mut router, sample(2, Phase::Up, 10.0, 30.0, 2, 4));
        assert!(released
            .iter()
            .any(|event| event.action == "transform" && event.phase.as_deref() == Some("end")));
        assert!(
            route_with_router(&mut router, sample(2, Phase::Move, 20.0, 20.0, 1, 5)).is_empty()
        );
    }

    #[test]
    fn instance_scoped_surface_namespaces_hit_region_targets() {
        let mut router = Router::default();
        begin_router_frame(&mut router);
        record_router_surface(
            &mut router,
            "i-child::surface",
            Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(100.0), px(100.0)),
            },
            &Interaction {
                pointer_action: Some("pointer".into()),
                hit_regions: vec![experience_ir::HitRegion {
                    id: "button".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 50.0,
                    height: 50.0,
                    press_action: None,
                    drag_action: None,
                    drop_action: None,
                    tap_action: None,
                    double_tap_action: None,
                    long_press_action: None,
                    swipe_action: None,
                }],
                ..Default::default()
            },
        );
        let events = route_with_router(
            &mut router,
            Sample {
                id: 91,
                phase: Phase::Down,
                x: 10.0,
                y: 10.0,
                pressure: 1.0,
                pointer_count: 1,
                event_time_nanos: 91,
            },
        );
        assert_eq!(events[0].target.as_deref(), Some("i-child::button"));
        let _ = route_with_router(
            &mut router,
            Sample {
                id: 91,
                phase: Phase::Up,
                x: 10.0,
                y: 10.0,
                pressure: 0.0,
                pointer_count: 1,
                event_time_nanos: 92,
            },
        );
    }
}
