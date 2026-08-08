use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use experience_ir::{Align, ExperienceModel, Justify, NodeKind, UiEvent, UiNode};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, Application, Context, MouseButton, Render,
    SharedString, Window, WindowOptions,
};
use gpui_mobile::{android::jni, packages::deeplink};
use runtime_luau::{CandidateTimings, RuntimeWorker, WorkerResult};
use serde_json::{json, Value as JsonValue};

use crate::{DEFAULT_EXPERIENCE, TIMEFLOW_EXPERIENCE};

static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static STRESS_REQUEST: OnceLock<Mutex<Option<StressRequest>>> = OnceLock::new();

const ACTIVE_FILE: &str = "experience.active.luau";
const CANDIDATE_FILE: &str = "experience.candidate.luau";
const PREVIOUS_FILE: &str = "experience.previous.luau";
const REJECTED_FILE: &str = "experience.rejected.luau";
const STATE_FILE: &str = "experience-state.json";
const MAX_STRESS_SWAPS: usize = 10_000;

#[derive(Clone, Debug)]
struct StressRequest {
    run_id: String,
    count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidatePurpose {
    Regular,
    Stress,
}

struct PendingFrame {
    request_id: u64,
    purpose: CandidatePurpose,
    timings: CandidateTimings,
}

struct StressRun {
    run_id: String,
    total: usize,
    completed: usize,
    started_at: Instant,
    original_source: String,
    alternate_source: String,
    visible_latencies_us: Vec<u64>,
    worker_latencies_us: Vec<u64>,
    rss_start_kb: u64,
    rss_peak_kb: u64,
}

#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("sos-experience"),
    );
    jni::install_panic_hook();

    if let Some(path) = app.internal_data_path() {
        let _ = FILES_DIR.set(path);
    } else {
        log::error!("Android did not expose an internal data path");
        return;
    }

    let _platform = jni::init_platform(&app);
    let Some(shared) = jni::shared_platform() else {
        log::error!("shared Android platform is unavailable");
        return;
    };

    deeplink::set_deep_link_handler(|url| {
        if url.starts_with("sos://reload") {
            RELOAD_REQUESTED.store(true, Ordering::Release);
            log::info!("script_reload_requested");
        } else if url.starts_with("sos://stress") {
            match parse_stress_request(url) {
                Some(request) => {
                    *stress_request_slot().lock().expect("stress request lock") = Some(request);
                    log::info!("stress_requested url={url}");
                }
                None => log::warn!("invalid stress request: {url}"),
            }
        }
    });

    Application::with_platform(shared.into_rc()).run(|cx: &mut App| {
        match cx.open_window(
            WindowOptions {
                window_bounds: None,
                ..Default::default()
            },
            |_, cx| cx.new(ExperienceHost::new),
        ) {
            Ok(_) => log::info!("SOS experience window is live"),
            Err(error) => log::error!("failed to open experience window: {error}"),
        }
    });
}

struct ExperienceHost {
    model: ExperienceModel,
    worker: RuntimeWorker,
    tree: UiNode,
    state: JsonValue,
    source: String,
    status: Option<(String, bool)>,
    next_request_id: u64,
    candidates: HashMap<u64, CandidatePurpose>,
    action_in_flight: bool,
    pending_frame: Option<PendingFrame>,
    stress: Option<StressRun>,
}

impl ExperienceHost {
    fn new(cx: &mut Context<Self>) -> Self {
        let model = providers_fake::snapshot();
        let state = load_state();
        let preferred_source =
            read_file(ACTIVE_FILE).unwrap_or_else(|| DEFAULT_EXPERIENCE.to_owned());
        let (worker, ready, source) =
            match RuntimeWorker::start(preferred_source.clone(), model.clone(), state.clone()) {
                Ok((worker, ready)) => (worker, ready, preferred_source),
                Err(error) => {
                    log::error!(
                        "active source rejected at startup: {error}; using embedded source"
                    );
                    let (worker, ready) = RuntimeWorker::start(
                        DEFAULT_EXPERIENCE.to_owned(),
                        model.clone(),
                        state.clone(),
                    )
                    .expect("embedded experience must be valid");
                    (worker, ready, DEFAULT_EXPERIENCE.to_owned())
                }
            };

        let results = worker.results();
        cx.spawn(async move |this, cx| {
            while let Ok(result) = results.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.handle_worker_result(result);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        log::info!(
            "runtime_worker_ready ui_thread={:?} worker_thread={} initialize_us={}",
            thread::current().id(),
            ready.worker_thread,
            ready.initialize_us
        );

        let mut host = Self {
            model,
            worker,
            tree: ready.tree,
            state,
            source,
            status: None,
            next_request_id: 1,
            candidates: HashMap::new(),
            action_in_flight: false,
            pending_frame: None,
            stress: None,
        };
        if file_path(CANDIDATE_FILE).is_file() {
            host.submit_reload();
        }
        host
    }

    fn allocate_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        request_id
    }

    fn dispatch(&mut self, action: String, cx: &mut Context<Self>) {
        if self.action_in_flight || self.stress.is_some() {
            return;
        }
        let request_id = self.allocate_request_id();
        log::info!("experience_action request_id={request_id} action={action}");
        self.action_in_flight = true;
        if let Err(error) = self.worker.action(
            request_id,
            self.model.clone(),
            self.state.clone(),
            UiEvent { action },
        ) {
            self.action_in_flight = false;
            self.status = Some((format!("Action could not start: {error}"), false));
            cx.notify();
        }
    }

    fn submit_reload(&mut self) {
        if self.stress.is_some()
            || self
                .candidates
                .values()
                .any(|purpose| *purpose == CandidatePurpose::Regular)
        {
            self.status = Some(("A candidate or stress run is already active".into(), false));
            return;
        }
        let Some(candidate_source) = read_file(CANDIDATE_FILE) else {
            self.status = Some(("No candidate script found".into(), false));
            return;
        };
        let request_id = self.allocate_request_id();
        let submitted_at = Instant::now();
        self.candidates
            .insert(request_id, CandidatePurpose::Regular);
        self.status = Some(("Compiling candidate on Luau worker…".into(), true));
        log::info!(
            "script_submitted request_id={request_id} ui_thread={:?} source_bytes={}",
            thread::current().id(),
            candidate_source.len()
        );
        if let Err(error) = self.worker.prepare_candidate(
            request_id,
            candidate_source,
            self.model.clone(),
            self.state.clone(),
            submitted_at,
        ) {
            self.candidates.remove(&request_id);
            self.status = Some((format!("Candidate could not start: {error}"), false));
        }
    }

    fn start_stress(&mut self, request: StressRequest) {
        if self.stress.is_some() || !self.candidates.is_empty() || self.action_in_flight {
            log::warn!(
                "stress_failed run_id={} reason=runtime_busy",
                request.run_id
            );
            self.status = Some((
                "Cannot start stress run while runtime is busy".into(),
                false,
            ));
            return;
        }

        let alternate_source = if self.source.trim() == TIMEFLOW_EXPERIENCE.trim() {
            DEFAULT_EXPERIENCE.to_owned()
        } else {
            TIMEFLOW_EXPERIENCE.to_owned()
        };
        let rss_start_kb = current_rss_kb().unwrap_or_default();
        log::info!(
            "stress_started run_id={} total={} rss_start_kb={}",
            request.run_id,
            request.count,
            rss_start_kb
        );
        self.status = Some((format!("Stress 0 / {}", request.count), true));
        self.stress = Some(StressRun {
            run_id: request.run_id,
            total: request.count,
            completed: 0,
            started_at: Instant::now(),
            original_source: self.source.clone(),
            alternate_source,
            visible_latencies_us: Vec::with_capacity(request.count),
            worker_latencies_us: Vec::with_capacity(request.count),
            rss_start_kb,
            rss_peak_kb: rss_start_kb,
        });
        self.submit_next_stress_candidate();
    }

    fn submit_next_stress_candidate(&mut self) {
        let Some(stress) = &self.stress else {
            return;
        };
        let iteration = stress.completed + 1;
        let source = if iteration == stress.total || iteration % 2 == 0 {
            stress.original_source.clone()
        } else {
            stress.alternate_source.clone()
        };
        let request_id = self.allocate_request_id();
        self.candidates.insert(request_id, CandidatePurpose::Stress);
        if let Err(error) = self.worker.prepare_candidate(
            request_id,
            source,
            self.model.clone(),
            self.state.clone(),
            Instant::now(),
        ) {
            self.candidates.remove(&request_id);
            self.fail_stress(format!("worker unavailable: {error}"));
        }
    }

    fn handle_worker_result(&mut self, result: WorkerResult) {
        match result {
            WorkerResult::CandidatePrepared {
                request_id, source, ..
            } => {
                let Some(purpose) = self.candidates.get(&request_id).copied() else {
                    let _ = self.worker.discard_candidate(request_id);
                    return;
                };
                if purpose == CandidatePurpose::Regular {
                    if let Err(error) = write_file(PREVIOUS_FILE, &self.source)
                        .and_then(|_| write_file(ACTIVE_FILE, &source))
                    {
                        let _ = self.worker.discard_candidate(request_id);
                        self.candidates.remove(&request_id);
                        self.status = Some((format!("Could not persist revision: {error}"), false));
                        return;
                    }
                }
                if let Err(error) = self.worker.commit_candidate(request_id) {
                    self.candidates.remove(&request_id);
                    if purpose == CandidatePurpose::Stress {
                        self.fail_stress(format!("commit failed: {error}"));
                    } else {
                        self.status = Some((format!("Candidate commit failed: {error}"), false));
                    }
                }
            }
            WorkerResult::CandidateRejected {
                request_id,
                error,
                timings,
                ..
            } => {
                let purpose = self.candidates.remove(&request_id);
                log::warn!(
                    "script_rejected request_id={} error={} queue_us={} compile_us={} render_us={} worker_total_us={}",
                    request_id,
                    error,
                    timings.queue_us,
                    timings.compile_us,
                    timings.render_us,
                    timings.worker_total_us
                );
                match purpose {
                    Some(CandidatePurpose::Regular) => {
                        self.status = Some((format!("Candidate rejected: {error}"), false));
                        let _ = fs::rename(file_path(CANDIDATE_FILE), file_path(REJECTED_FILE));
                    }
                    Some(CandidatePurpose::Stress) => self.fail_stress(error),
                    None => {}
                }
            }
            WorkerResult::CandidateCommitted {
                request_id,
                source,
                tree,
                timings,
            } => {
                let Some(purpose) = self.candidates.remove(&request_id) else {
                    return;
                };
                self.source = source;
                self.tree = tree;
                if purpose == CandidatePurpose::Regular {
                    let _ = fs::remove_file(file_path(CANDIDATE_FILE));
                    self.status =
                        Some(("Candidate rendered; confirming presentation…".into(), true));
                }
                self.pending_frame = Some(PendingFrame {
                    request_id,
                    purpose,
                    timings,
                });
            }
            WorkerResult::ActionCompleted {
                request_id,
                state,
                tree,
                worker_us,
            } => {
                self.action_in_flight = false;
                self.state = state;
                self.tree = tree;
                persist_state(&self.state);
                self.status = None;
                log::info!(
                    "experience_action_completed request_id={request_id} worker_us={worker_us}"
                );
            }
            WorkerResult::ActionRejected {
                request_id,
                error,
                worker_us,
            } => {
                self.action_in_flight = false;
                self.status = Some((format!("Action rejected: {error}"), false));
                log::warn!(
                    "experience_action_rejected request_id={request_id} worker_us={worker_us} error={error}"
                );
            }
        }
    }

    fn frame_presented(&mut self, frame: PendingFrame, cx: &mut Context<Self>) {
        let visible_us = micros(frame.timings.submitted_at.elapsed());
        match frame.purpose {
            CandidatePurpose::Regular => {
                self.status = Some((
                    format!("Luau revision visible in {} ms", visible_us / 1_000),
                    true,
                ));
                log::info!(
                    "script_visible request_id={} source_to_visible_us={} queue_us={} compile_us={} render_us={} worker_total_us={}",
                    frame.request_id,
                    visible_us,
                    frame.timings.queue_us,
                    frame.timings.compile_us,
                    frame.timings.render_us,
                    frame.timings.worker_total_us
                );
            }
            CandidatePurpose::Stress => {
                let rss_kb = current_rss_kb().unwrap_or_default();
                let Some(stress) = &mut self.stress else {
                    return;
                };
                stress.completed += 1;
                stress.visible_latencies_us.push(visible_us);
                stress
                    .worker_latencies_us
                    .push(frame.timings.worker_total_us);
                stress.rss_peak_kb = stress.rss_peak_kb.max(rss_kb);

                if stress.completed == stress.total {
                    self.complete_stress();
                } else {
                    if stress.completed % 25 == 0 {
                        self.status = Some((
                            format!("Stress {} / {}", stress.completed, stress.total),
                            true,
                        ));
                    }
                    self.submit_next_stress_candidate();
                }
            }
        }
        cx.notify();
    }

    fn complete_stress(&mut self) {
        let Some(mut stress) = self.stress.take() else {
            return;
        };
        let rss_end_kb = current_rss_kb().unwrap_or_default();
        stress.rss_peak_kb = stress.rss_peak_kb.max(rss_end_kb);
        let visible_p50_us = percentile(&stress.visible_latencies_us, 50);
        let visible_p95_us = percentile(&stress.visible_latencies_us, 95);
        let visible_p99_us = percentile(&stress.visible_latencies_us, 99);
        let visible_max_us = stress
            .visible_latencies_us
            .iter()
            .copied()
            .max()
            .unwrap_or_default();
        let worker_p95_us = percentile(&stress.worker_latencies_us, 95);
        let duration_ms = stress.started_at.elapsed().as_millis();
        log::info!(
            "stress_complete run_id={} total={} accepted={} rejected=0 duration_ms={} visible_p50_us={} visible_p95_us={} visible_p99_us={} visible_max_us={} worker_p95_us={} rss_start_kb={} rss_end_kb={} rss_peak_kb={} rss_delta_kb={}",
            stress.run_id,
            stress.total,
            stress.completed,
            duration_ms,
            visible_p50_us,
            visible_p95_us,
            visible_p99_us,
            visible_max_us,
            worker_p95_us,
            stress.rss_start_kb,
            rss_end_kb,
            stress.rss_peak_kb,
            rss_end_kb.saturating_sub(stress.rss_start_kb)
        );
        self.status = Some((
            format!(
                "{} swaps · p95 {} ms · ΔRSS {} KB",
                stress.completed,
                visible_p95_us / 1_000,
                rss_end_kb.saturating_sub(stress.rss_start_kb)
            ),
            true,
        ));
    }

    fn fail_stress(&mut self, reason: String) {
        let run_id = self
            .stress
            .as_ref()
            .map(|stress| stress.run_id.as_str())
            .unwrap_or("unknown");
        let completed = self
            .stress
            .as_ref()
            .map(|stress| stress.completed)
            .unwrap_or_default();
        log::error!(
            "stress_failed run_id={} completed={} reason={}",
            run_id,
            completed,
            reason
        );
        self.status = Some((format!("Stress failed after {completed}: {reason}"), false));
        self.stress = None;
    }

    fn render_node(
        &mut self,
        node: &UiNode,
        path: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let element_id = node.id.clone().unwrap_or_else(|| path.to_string());
        let mut element = div();
        match node.kind {
            NodeKind::Column => element = element.flex().flex_col(),
            NodeKind::Row => element = element.flex().flex_row(),
            NodeKind::Scroll => element = element.flex().flex_col().size_full(),
            NodeKind::Box | NodeKind::Spacer | NodeKind::Text(_) => {}
        }

        if let Some(background) = node.style.background {
            element = element.bg(rgb(background));
        }
        if let Some(color) = node.style.color {
            element = element.text_color(rgb(color));
        }
        if let Some(padding) = node.style.padding {
            element = element.p(px(padding));
        }
        if let Some(gap) = node.style.gap {
            element = element.gap(px(gap));
        }
        if let Some(radius) = node.style.radius {
            element = element.rounded(px(radius));
        }
        if let Some(text_size) = node.style.text_size {
            element = element.text_size(px(text_size));
        }
        if let Some(width) = node.style.width {
            element = element.w(px(width));
        }
        if let Some(height) = node.style.height {
            element = element.h(px(height));
        }
        if node.style.grow {
            element = element.flex_1();
        }
        element = match node.style.align {
            Some(Align::Start) => element.items_start(),
            Some(Align::Center) => element.items_center(),
            Some(Align::End) => element.items_end(),
            None => element,
        };
        element = match node.style.justify {
            Some(Justify::Start) => element.justify_start(),
            Some(Justify::Center) => element.justify_center(),
            Some(Justify::End) => element.justify_end(),
            Some(Justify::Between) => element.justify_between(),
            None => element,
        };
        if let NodeKind::Text(text) = &node.kind {
            element = element.child(SharedString::from(text.clone()));
        }
        for (index, child) in node.children.iter().enumerate() {
            let child_path = SharedString::from(format!("{path}-{index}"));
            element = element.child(self.render_node(child, child_path, cx));
        }
        if let Some(action) = &node.action {
            let action = action.clone();
            return element
                .id(SharedString::from(element_id))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.dispatch(action.clone(), cx)),
                )
                .into_any_element();
        }
        if matches!(node.kind, NodeKind::Scroll) {
            return element
                .id(SharedString::from(element_id))
                .overflow_y_scroll()
                .into_any_element();
        }
        element.into_any_element()
    }
}

impl Render for ExperienceHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if RELOAD_REQUESTED.swap(false, Ordering::AcqRel) {
            self.submit_reload();
        }
        let stress_request = stress_request_slot()
            .lock()
            .expect("stress request lock")
            .take();
        if let Some(stress_request) = stress_request {
            self.start_stress(stress_request);
        }
        if let Some(frame) = self.pending_frame.take() {
            cx.on_next_frame(window, move |this, _, cx| {
                this.frame_presented(frame, cx);
            });
        }

        let tree = self.tree.clone();
        let content = self.render_node(&tree, SharedString::from("root"), cx);
        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .pt(px(34.0))
            .pb(px(18.0))
            .bg(rgb(0xF3F1E8))
            .child(content);
        if let Some((message, accepted)) = &self.status {
            root = root.child(
                div()
                    .absolute()
                    .left(px(14.0))
                    .right(px(14.0))
                    .bottom(px(12.0))
                    .p(px(10.0))
                    .rounded(px(12.0))
                    .bg(rgb(if *accepted { 0x2F684B } else { 0x8C3A36 }))
                    .text_color(rgb(0xFFFFFF))
                    .text_size(px(12.0))
                    .child(SharedString::from(message.clone())),
            );
        }
        root
    }
}

fn stress_request_slot() -> &'static Mutex<Option<StressRequest>> {
    STRESS_REQUEST.get_or_init(|| Mutex::new(None))
}

fn parse_stress_request(url: &str) -> Option<StressRequest> {
    let query = url.split_once('?')?.1;
    let mut count = None;
    let mut run_id = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "count" => count = value.parse::<usize>().ok(),
            "run_id" if !value.is_empty() && value.len() <= 96 => run_id = Some(value.to_owned()),
            _ => {}
        }
    }
    let count = count?.clamp(1, MAX_STRESS_SWAPS);
    Some(StressRequest {
        run_id: run_id.unwrap_or_else(|| "manual".into()),
        count,
    })
}

fn file_path(name: &str) -> PathBuf {
    FILES_DIR
        .get()
        .expect("files directory initialized")
        .join(name)
}

fn read_file(name: &str) -> Option<String> {
    fs::read_to_string(file_path(name)).ok()
}

fn write_file(name: &str, contents: &str) -> std::io::Result<()> {
    let destination = file_path(name);
    let temporary = destination.with_extension("tmp");
    fs::write(&temporary, contents)?;
    fs::rename(temporary, destination)
}

fn load_state() -> JsonValue {
    read_file(STATE_FILE)
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_else(|| json!({}))
}

fn persist_state(state: &JsonValue) {
    match serde_json::to_string(state) {
        Ok(contents) => {
            if let Err(error) = write_file(STATE_FILE, &contents) {
                log::warn!("could not persist experience state: {error}");
            }
        }
        Err(error) => log::warn!("could not serialize experience state: {error}"),
    }
}

fn current_rss_kb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn percentile(values: &[u64], percent: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) * percent.min(100)) / 100;
    sorted[index]
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_stress_requests() {
        let request = parse_stress_request("sos://stress?count=1000&run_id=test-1").unwrap();
        assert_eq!(request.count, 1_000);
        assert_eq!(request.run_id, "test-1");
        assert_eq!(
            parse_stress_request("sos://stress?count=999999&run_id=max")
                .unwrap()
                .count,
            MAX_STRESS_SWAPS
        );
        assert!(parse_stress_request("sos://stress?run_id=missing").is_none());
    }

    #[test]
    fn calculates_percentiles() {
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 95), 4);
        assert_eq!(percentile(&[], 95), 0);
    }
}
