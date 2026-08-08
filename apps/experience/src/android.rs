mod accessibility;
mod assets;
mod native_canvas;
mod native_input;
mod provider_client;

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

use ::jni::objects::JValue;
use experience_ir::{
    Align, AnimationKind, Canvas, ExperienceModel, HitRegion, Justify, NodeKind, StateEnvelope,
    UiEvent, UiNode,
};
use gpui::{
    div, img, prelude::*, px, rgb, Animation as GpuiAnimation, AnimationExt as _, AnyElement, App,
    Application, Context, Entity, MouseButton, Render, SharedString, Window, WindowOptions,
};
use gpui_mobile::{android::jni, packages::deeplink};
use runtime_luau::{CandidateTimings, RuntimeWorker, WorkerReady, WorkerResult};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::{
    DAILY_FLOW_AGENT_EXPERIENCE, DAILY_FLOW_EXPERIENCE, DEFAULT_EXPERIENCE, TIMEFLOW_EXPERIENCE,
};
use assets::{SosAssets, ALBUM_ASSET};
use native_input::NativeTextInput;

static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static WORKER_RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);
static CANDIDATE_PROMOTED: AtomicBool = AtomicBool::new(false);
static STRESS_REQUEST: OnceLock<Mutex<Option<StressRequest>>> = OnceLock::new();
static CANDIDATE_EVENT: OnceLock<Mutex<Option<CandidateEvent>>> = OnceLock::new();
static PROCESS_CONFIG: OnceLock<ProcessConfig> = OnceLock::new();

const ACTIVE_FILE: &str = "experience.active.luau";
const CANDIDATE_FILE: &str = "experience.candidate.luau";
const CANDIDATE_STATE_FILE: &str = "experience.candidate-state.json";
const PREVIOUS_FILE: &str = "experience.previous.luau";
const REJECTED_FILE: &str = "experience.rejected.luau";
const STATE_FILE: &str = "experience-state.json";
const MAX_STRESS_SWAPS: usize = 10_000;

#[derive(Clone, Debug, Default)]
struct ProcessConfig {
    candidate_revision: Option<String>,
    candidate_mode: Option<String>,
    stage_id: Option<u64>,
    expected_revision: Option<u64>,
}

#[derive(Clone, Debug)]
enum CandidateEvent {
    Presented(String),
    Died(String),
}

struct PendingPromotion {
    request_id: u64,
    revision: String,
    stage_id: u64,
    expected_revision: u64,
    source_sha256: String,
}

impl ProcessConfig {
    fn is_candidate(&self) -> bool {
        self.candidate_revision.is_some()
    }
}

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
    committed_at: Instant,
    worker_to_commit_us: u64,
    commit_to_render_us: u64,
    render_build_us: u64,
    callback_scheduled_at: Instant,
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
    worker_to_commit_latencies_us: Vec<u64>,
    commit_to_render_latencies_us: Vec<u64>,
    render_build_latencies_us: Vec<u64>,
    frame_callback_latencies_us: Vec<u64>,
    rss_start_kb: u64,
    rss_peak_kb: u64,
    rss_samples: Vec<(usize, u64)>,
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
    let process_config = read_process_config().unwrap_or_else(|error| {
        log::warn!("process_role_read_failed error={error}");
        ProcessConfig::default()
    });
    log::info!(
        "sos_process_role role={} revision={} mode={}",
        if process_config.is_candidate() {
            "candidate"
        } else {
            "accepted"
        },
        process_config
            .candidate_revision
            .as_deref()
            .unwrap_or("none"),
        process_config.candidate_mode.as_deref().unwrap_or("none")
    );
    let _ = PROCESS_CONFIG.set(process_config);
    let Some(shared) = jni::shared_platform() else {
        log::error!("shared Android platform is unavailable");
        return;
    };

    deeplink::set_deep_link_handler(|url| {
        if url.starts_with("sos://reload") {
            RELOAD_REQUESTED.store(true, Ordering::Release);
            log::info!("script_reload_requested");
        } else if url.starts_with("sos://worker-restart") {
            WORKER_RESTART_REQUESTED.store(true, Ordering::Release);
            log::info!("runtime_worker_restart_requested");
        } else if url.starts_with("sos://stress") {
            match parse_stress_request(url) {
                Some(request) => {
                    *stress_request_slot().lock().expect("stress request lock") = Some(request);
                    log::info!("stress_requested url={url}");
                }
                None => log::warn!("invalid stress request: {url}"),
            }
        } else if let Some(revision) = url.strip_prefix("sos://candidate-presented?revision=") {
            *candidate_event_slot().lock().expect("candidate event lock") =
                Some(CandidateEvent::Presented(revision.to_owned()));
            log::info!("candidate_presentation_observed revision={revision}");
        } else if let Some(revision) = url.strip_prefix("sos://candidate-died?revision=") {
            *candidate_event_slot().lock().expect("candidate event lock") =
                Some(CandidateEvent::Died(revision.to_owned()));
            log::info!("candidate_death_observed revision={revision}");
        }
    });

    Application::with_platform(shared.into_rc())
        .with_assets(SosAssets)
        .run(|cx: &mut App| {
            native_input::bind_keys(cx);
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
    remote_state_revision: Option<u64>,
    state_schema_version: u64,
    source: String,
    status: Option<(String, bool)>,
    next_request_id: u64,
    candidates: HashMap<u64, CandidatePurpose>,
    action_in_flight: bool,
    pending_input_event: Option<UiEvent>,
    input_state_shadow: HashMap<String, String>,
    inputs: HashMap<String, Entity<NativeTextInput>>,
    canvas_drags: HashMap<String, String>,
    pending_focus_restore: Option<String>,
    pending_frame: Option<PendingFrame>,
    stress: Option<StressRun>,
    candidate_first_frame_pending: bool,
    pending_promotion: Option<PendingPromotion>,
    pending_reconciled_state: Option<StateEnvelope>,
}

impl ExperienceHost {
    fn new(cx: &mut Context<Self>) -> Self {
        let process_config = PROCESS_CONFIG.get().cloned().unwrap_or_default();
        let model = match provider_client::snapshot() {
            Ok(model) => {
                log::info!("provider_snapshot_remote transport=tcp");
                model
            }
            Err(error) => {
                #[cfg(feature = "offline-fallback")]
                {
                    log::warn!("provider_snapshot_fallback error={error}");
                    providers_fake::snapshot()
                }
                #[cfg(not(feature = "offline-fallback"))]
                panic!("strict gate requires provider snapshot: {error}")
            }
        };
        let (mut state, remote_state_revision, mut state_schema_version) =
            match provider_client::load_state() {
                Ok(envelope) => {
                    log::info!(
                        "experience_state_remote revision={} schema_version={}",
                        envelope.revision,
                        envelope.schema_version
                    );
                    (
                        envelope.state,
                        Some(envelope.revision),
                        envelope.schema_version,
                    )
                }
                Err(error) => {
                    #[cfg(feature = "offline-fallback")]
                    {
                        log::warn!("experience_state_fallback error={error}");
                        (load_state(), None, 1)
                    }
                    #[cfg(not(feature = "offline-fallback"))]
                    panic!("strict gate requires external state service: {error}")
                }
            };
        if process_config.is_candidate() {
            if let Some(envelope) = read_state_envelope(CANDIDATE_STATE_FILE) {
                state = envelope.state;
                state_schema_version = envelope.schema_version;
                log::info!(
                    "candidate_staged_state_loaded revision={} schema_version={} source_sha256={}",
                    envelope.revision,
                    envelope.schema_version,
                    envelope.source_sha256
                );
            }
        }
        let source = if process_config.is_candidate() {
            read_file(CANDIDATE_FILE)
                .or_else(|| read_file(ACTIVE_FILE))
                .unwrap_or_else(|| DEFAULT_EXPERIENCE.to_owned())
        } else {
            read_file(ACTIVE_FILE).unwrap_or_else(|| DEFAULT_EXPERIENCE.to_owned())
        };
        let (worker, ready) = RuntimeWorker::spawn(
            source.clone(),
            model.clone(),
            state.clone(),
            state_schema_version,
        )
        .expect("runtime worker thread must start");
        let results = worker.results();
        Self::attach_worker_channels(ready, results, cx);
        log::info!(
            "runtime_worker_spawned ui_thread={:?}",
            thread::current().id()
        );

        Self {
            model,
            worker,
            tree: loading_tree(),
            state,
            remote_state_revision,
            state_schema_version,
            source,
            status: Some(("Starting Luau worker…".into(), true)),
            next_request_id: 1,
            candidates: HashMap::new(),
            action_in_flight: false,
            pending_input_event: None,
            input_state_shadow: HashMap::new(),
            inputs: HashMap::new(),
            canvas_drags: HashMap::new(),
            pending_focus_restore: None,
            pending_frame: None,
            stress: None,
            candidate_first_frame_pending: false,
            pending_promotion: None,
            pending_reconciled_state: None,
        }
    }

    fn attach_worker_channels(
        ready: async_channel::Receiver<Result<WorkerReady, String>>,
        results: async_channel::Receiver<WorkerResult>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            if let Ok(result) = ready.recv().await {
                let _ = this.update(cx, |this, cx| {
                    this.handle_worker_ready(result, cx);
                    cx.notify();
                });
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            while let Ok(result) = results.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.handle_worker_result(result, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_worker_ready(&mut self, result: Result<WorkerReady, String>, cx: &mut Context<Self>) {
        match result {
            Ok(ready) => {
                self.tree = ready.tree;
                self.state = ready.state;
                self.state_schema_version = ready.state_schema_version;
                self.status = None;
                self.publish_accessibility();
                log::info!(
                    "runtime_worker_ready ui_thread={:?} worker_thread={} initialize_us={}",
                    thread::current().id(),
                    ready.worker_thread,
                    ready.initialize_us
                );
                if PROCESS_CONFIG
                    .get()
                    .is_some_and(ProcessConfig::is_candidate)
                {
                    if PROCESS_CONFIG
                        .get()
                        .and_then(|config| config.candidate_mode.as_deref())
                        .is_some_and(|mode| mode == "crash-before" || mode == "native-crash-before")
                    {
                        log::error!(
                            "candidate_native_crash phase=before_first_frame revision={}",
                            PROCESS_CONFIG
                                .get()
                                .and_then(|config| config.candidate_revision.as_deref())
                                .unwrap_or("unknown")
                        );
                        std::process::abort();
                    }
                    self.candidate_first_frame_pending = true;
                } else if file_path(CANDIDATE_FILE).is_file() {
                    self.submit_reload();
                }
            }
            Err(error) if self.source.trim() != DEFAULT_EXPERIENCE.trim() => {
                log::error!("active source rejected at startup: {error}; using embedded source");
                self.source = DEFAULT_EXPERIENCE.to_owned();
                match RuntimeWorker::spawn(
                    self.source.clone(),
                    self.model.clone(),
                    self.state.clone(),
                    self.state_schema_version,
                ) {
                    Ok((worker, ready)) => {
                        let results = worker.results();
                        self.worker = worker;
                        Self::attach_worker_channels(ready, results, cx);
                    }
                    Err(error) => {
                        self.status = Some((format!("Runtime could not start: {error}"), false));
                    }
                }
            }
            Err(error) => {
                log::error!("embedded runtime rejected at startup: {error}");
                self.status = Some((format!("Runtime could not start: {error}"), false));
            }
        }
    }

    fn allocate_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        request_id
    }

    fn restart_worker(&mut self, cx: &mut Context<Self>) {
        if self.stress.is_some() || !self.candidates.is_empty() || self.action_in_flight {
            log::warn!("runtime_worker_restart_rejected reason=runtime_busy");
            return;
        }
        match RuntimeWorker::spawn(
            self.source.clone(),
            self.model.clone(),
            self.state.clone(),
            self.state_schema_version,
        ) {
            Ok((worker, ready)) => {
                let results = worker.results();
                self.worker = worker;
                self.status = Some(("Restarting Luau worker…".into(), true));
                Self::attach_worker_channels(ready, results, cx);
                log::info!(
                    "runtime_worker_restarting ui_thread={:?}",
                    thread::current().id()
                );
            }
            Err(error) => {
                self.status = Some((format!("Worker restart failed: {error}"), false));
                log::error!("runtime_worker_restart_failed error={error}");
            }
        }
    }

    fn dispatch(&mut self, action: String, cx: &mut Context<Self>) {
        self.dispatch_event(
            UiEvent {
                action,
                ..Default::default()
            },
            cx,
        );
    }

    fn dispatch_event(&mut self, event: UiEvent, cx: &mut Context<Self>) {
        let candidate_waiting_for_promotion = PROCESS_CONFIG
            .get()
            .is_some_and(|config| config.stage_id.is_some())
            && !CANDIDATE_PROMOTED.load(Ordering::Acquire);
        if self.pending_promotion.is_some() || candidate_waiting_for_promotion {
            self.pending_input_event = Some(event);
            return;
        }
        if self.action_in_flight || self.stress.is_some() {
            return;
        }
        let request_id = self.allocate_request_id();
        log::info!(
            "experience_action request_id={request_id} action={} target={}",
            event.action,
            event.target.as_deref().unwrap_or("none")
        );
        self.action_in_flight = true;
        if let Err(error) =
            self.worker
                .action(request_id, self.model.clone(), self.state.clone(), event)
        {
            self.action_in_flight = false;
            self.status = Some((format!("Action could not start: {error}"), false));
            cx.notify();
        }
    }

    pub(super) fn native_input_changed(
        &mut self,
        node_id: String,
        state_key: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if !self.state.is_object() {
            self.state = json!({});
        }
        if let Some(object) = self.state.as_object_mut() {
            object.insert(state_key.clone(), JsonValue::String(value.clone()));
        }
        self.input_state_shadow.insert(state_key, value.clone());
        persist_state(&self.state);
        log::info!(
            "native_text_changed node_id={} bytes={} marked_safe=true",
            node_id,
            value.len()
        );
        self.queue_input_event(
            UiEvent {
                action: "text_changed".into(),
                target: Some(node_id),
                value: Some(value),
                focused: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn native_input_focus_changed(
        &mut self,
        node_id: String,
        focused: bool,
        cx: &mut Context<Self>,
    ) {
        log::info!("native_text_focus node_id={node_id} focused={focused}");
        self.queue_input_event(
            UiEvent {
                action: "focus_changed".into(),
                target: Some(node_id),
                value: None,
                focused: Some(focused),
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn native_input_submitted(
        &mut self,
        node_id: String,
        action: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.queue_input_event(
            UiEvent {
                action,
                target: Some(node_id),
                value: Some(value),
                focused: Some(true),
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn native_canvas_down(
        &mut self,
        canvas_id: String,
        region: HitRegion,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.canvas_drags.insert(canvas_id, region.id.clone());
        if let Some(action) = region.press_action {
            self.queue_input_event(native_canvas::event(action, region.id, x, y), cx);
        }
    }

    pub(super) fn native_canvas_move(
        &mut self,
        canvas_id: String,
        specification: &Canvas,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        if !self.canvas_drags.contains_key(&canvas_id) {
            let Some(region) = specification.hit_regions.iter().rev().find(|region| {
                region.drag_action.is_some()
                    && x >= region.x
                    && x <= region.x + region.width
                    && y >= region.y
                    && y <= region.y + region.height
            }) else {
                return;
            };
            self.canvas_drags
                .insert(canvas_id.clone(), region.id.clone());
            if let Some(action) = &region.press_action {
                self.queue_input_event(
                    native_canvas::event(action.clone(), region.id.clone(), x, y),
                    cx,
                );
                return;
            }
        }
        let Some(region_id) = self.canvas_drags.get(&canvas_id) else {
            return;
        };
        let Some(region) = specification
            .hit_regions
            .iter()
            .find(|region| &region.id == region_id)
        else {
            return;
        };
        if let Some(action) = &region.drag_action {
            self.queue_input_event(
                native_canvas::event(action.clone(), region.id.clone(), x, y),
                cx,
            );
        }
    }

    pub(super) fn native_canvas_up(
        &mut self,
        canvas_id: String,
        specification: &Canvas,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(region_id) = self.canvas_drags.remove(&canvas_id) else {
            return;
        };
        let Some(region) = specification
            .hit_regions
            .iter()
            .find(|region| region.id == region_id)
        else {
            return;
        };
        if let Some(action) = &region.drop_action {
            self.queue_input_event(
                native_canvas::event(action.clone(), region.id.clone(), x, y),
                cx,
            );
        }
    }

    fn queue_input_event(&mut self, event: UiEvent, cx: &mut Context<Self>) {
        if self.action_in_flight || self.stress.is_some() {
            self.pending_input_event = Some(event);
        } else {
            self.dispatch_event(event, cx);
        }
    }

    fn dispatch_pending_input_event(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = self.pending_input_event.take() {
            self.dispatch_event(event, cx);
        }
    }

    fn merge_native_input_state(&self, state: &mut JsonValue) {
        if !state.is_object() {
            *state = json!({});
        }
        if let Some(object) = state.as_object_mut() {
            for (key, value) in &self.input_state_shadow {
                object.insert(key.clone(), JsonValue::String(value.clone()));
            }
        }
    }

    fn submit_reload(&mut self) {
        if self.stress.is_some()
            || self.action_in_flight
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
            self.state_schema_version,
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

        let alternate_source = if self.source.trim() == DAILY_FLOW_EXPERIENCE.trim() {
            DAILY_FLOW_AGENT_EXPERIENCE.to_owned()
        } else if self.source.trim() == DAILY_FLOW_AGENT_EXPERIENCE.trim() {
            DAILY_FLOW_EXPERIENCE.to_owned()
        } else if self.source.trim() == TIMEFLOW_EXPERIENCE.trim() {
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
            worker_to_commit_latencies_us: Vec::with_capacity(request.count),
            commit_to_render_latencies_us: Vec::with_capacity(request.count),
            render_build_latencies_us: Vec::with_capacity(request.count),
            frame_callback_latencies_us: Vec::with_capacity(request.count),
            rss_start_kb,
            rss_peak_kb: rss_start_kb,
            rss_samples: vec![(0, rss_start_kb)],
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
            self.state_schema_version,
            Instant::now(),
        ) {
            self.candidates.remove(&request_id);
            self.fail_stress(format!("worker unavailable: {error}"));
        }
    }

    fn handle_worker_result(&mut self, result: WorkerResult, cx: &mut Context<Self>) {
        match result {
            WorkerResult::CandidatePrepared {
                request_id,
                source,
                state,
                state_schema_version,
                timings,
                ..
            } => {
                let Some(purpose) = self.candidates.get(&request_id).copied() else {
                    let _ = self.worker.discard_candidate(request_id);
                    return;
                };
                if purpose == CandidatePurpose::Regular {
                    log::info!(
                        "candidate_validated request_id={} queue_us={} compile_us={} render_us={} worker_total_us={}",
                        request_id,
                        timings.queue_us,
                        timings.compile_us,
                        timings.render_us,
                        timings.worker_total_us
                    );
                    let Some(expected_revision) = self.remote_state_revision else {
                        let _ = self.worker.discard_candidate(request_id);
                        self.candidates.remove(&request_id);
                        self.status = Some((
                            "Candidate requires the external state service".into(),
                            false,
                        ));
                        return;
                    };
                    let hash = source_sha256(&source);
                    let stage_id = match provider_client::stage_state(
                        expected_revision,
                        state_schema_version,
                        &state,
                        &hash,
                        &[],
                    ) {
                        Ok(stage_id) => stage_id,
                        Err(error) => {
                            let _ = self.worker.discard_candidate(request_id);
                            self.candidates.remove(&request_id);
                            self.status =
                                Some((format!("Could not stage revision: {error}"), false));
                            return;
                        }
                    };
                    let revision = format!("{}-{}", expected_revision + 1, &hash[..12]);
                    let envelope = StateEnvelope {
                        revision: expected_revision + 1,
                        schema_version: state_schema_version,
                        source_sha256: hash.clone(),
                        state,
                    };
                    if let Err(error) = write_state_envelope(CANDIDATE_STATE_FILE, &envelope) {
                        let _ = provider_client::abort_state(stage_id);
                        let _ = self.worker.discard_candidate(request_id);
                        self.candidates.remove(&request_id);
                        self.status =
                            Some((format!("Could not persist staged revision: {error}"), false));
                        return;
                    }
                    self.pending_promotion = Some(PendingPromotion {
                        request_id,
                        revision: revision.clone(),
                        stage_id,
                        expected_revision,
                        source_sha256: hash,
                    });
                    self.status = Some((
                        "Candidate validated; launching isolated GPUI process…".into(),
                        true,
                    ));
                    let mode = if source.contains("sos-test-crash-before-first-frame") {
                        "crash-before"
                    } else {
                        "ready"
                    };
                    if let Err(error) =
                        launch_native_candidate(&revision, stage_id, expected_revision, mode)
                    {
                        self.reject_pending_promotion(format!("candidate launch failed: {error}"));
                    } else {
                        log::info!("candidate_process_launch revision={revision} stage_id={stage_id} expected_revision={expected_revision}");
                    }
                    return;
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
                state,
                state_schema_version,
                timings,
            } => {
                let Some(purpose) = self.candidates.remove(&request_id) else {
                    return;
                };
                self.pending_focus_restore = native_input::active_input_id();
                self.source = source;
                self.tree = tree;
                self.state = state;
                self.state_schema_version = state_schema_version;
                if let Some(envelope) = self.pending_reconciled_state.take() {
                    self.state = envelope.state;
                    self.state_schema_version = envelope.schema_version;
                    self.remote_state_revision = Some(envelope.revision);
                }
                self.publish_accessibility();
                if purpose == CandidatePurpose::Regular {
                    if read_file(CANDIDATE_FILE).is_some_and(|candidate| {
                        source_sha256(&candidate) == source_sha256(&self.source)
                    }) {
                        let _ = fs::remove_file(file_path(CANDIDATE_FILE));
                    }
                    self.status =
                        Some(("Candidate rendered; confirming presentation…".into(), true));
                }
                let committed_at = Instant::now();
                let worker_to_commit_us =
                    micros(timings.submitted_at.elapsed()).saturating_sub(timings.worker_total_us);
                self.pending_frame = Some(PendingFrame {
                    request_id,
                    purpose,
                    timings,
                    committed_at,
                    worker_to_commit_us,
                    commit_to_render_us: 0,
                    render_build_us: 0,
                    callback_scheduled_at: Instant::now(),
                });
            }
            WorkerResult::ActionCompleted {
                request_id,
                mut state,
                tree,
                effects,
                worker_us,
            } => {
                self.action_in_flight = false;
                self.merge_native_input_state(&mut state);
                if let Some(expected_revision) = self.remote_state_revision {
                    let source_sha256 = source_sha256(&self.source);
                    let mut committed = provider_client::commit_state(
                        expected_revision,
                        self.state_schema_version,
                        &state,
                        &source_sha256,
                        &effects,
                    );
                    if committed.is_err() {
                        if let Ok(current) = provider_client::load_state() {
                            if current.source_sha256 == source_sha256 {
                                self.remote_state_revision = Some(current.revision);
                                committed = provider_client::commit_state(
                                    current.revision,
                                    self.state_schema_version,
                                    &state,
                                    &source_sha256,
                                    &effects,
                                );
                            }
                        }
                    }
                    match committed {
                        Ok(envelope) => {
                            self.remote_state_revision = Some(envelope.revision);
                            state = envelope.state;
                            log::info!(
                                "experience_revision_promoted revision={} schema_version={} source_sha256={} effects={}",
                                envelope.revision,
                                envelope.schema_version,
                                envelope.source_sha256,
                                effects.len()
                            );
                        }
                        Err(error) => {
                            self.status = Some((format!("State promotion failed: {error}"), false));
                            log::warn!("experience_state_rejected error={error}");
                            self.dispatch_pending_input_event(cx);
                            return;
                        }
                    }
                }
                self.state = state;
                self.tree = tree;
                self.publish_accessibility();
                persist_state(&self.state);
                self.status = None;
                log::info!(
                    "experience_action_completed request_id={request_id} worker_us={worker_us}"
                );
                self.dispatch_pending_input_event(cx);
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
                self.dispatch_pending_input_event(cx);
            }
        }
    }

    fn publish_accessibility(&self) {
        let summary = accessibility::summary(&self.tree);
        match accessibility::publish(&summary) {
            Ok(()) => log::info!(
                "accessibility_published bytes={} semantics={}",
                summary.len(),
                count_semantics(&self.tree)
            ),
            Err(error) => log::warn!("accessibility_publish_failed error={error}"),
        }
    }

    fn reconcile_candidate_event(&mut self) {
        let event = candidate_event_slot()
            .lock()
            .expect("candidate event lock")
            .take();
        let Some(event) = event else { return };
        let Some(pending) = self.pending_promotion.take() else {
            return;
        };
        let event_revision = match &event {
            CandidateEvent::Presented(revision) | CandidateEvent::Died(revision) => revision,
        };
        if event_revision != &pending.revision {
            self.pending_promotion = Some(pending);
            return;
        }
        let promoted = provider_client::load_state().ok().filter(|current| {
            current.revision > pending.expected_revision
                && current.source_sha256 == pending.source_sha256
        });
        if let Some(current) = promoted {
            if let Some(candidate) = read_file(CANDIDATE_FILE) {
                if source_sha256(&candidate) == pending.source_sha256 {
                    if let Some(active) = read_file(ACTIVE_FILE) {
                        let _ = write_file(PREVIOUS_FILE, &active);
                    }
                    if let Err(error) = write_file(ACTIVE_FILE, &candidate) {
                        log::error!("accepted_source_reconcile_failed error={error}");
                    }
                }
            }
            if let Err(error) = self.worker.commit_candidate(pending.request_id) {
                log::error!("accepted_worker_candidate_commit_failed error={error}");
            }
            self.remote_state_revision = Some(current.revision);
            self.pending_reconciled_state = Some(current);
            log::info!(
                "accepted_revision_reconciled revision={} source_sha256={}",
                pending.revision,
                pending.source_sha256
            );
        } else {
            let _ = provider_client::abort_state(pending.stage_id);
            let _ = self.worker.discard_candidate(pending.request_id);
            self.candidates.remove(&pending.request_id);
            let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
            let _ = fs::rename(file_path(CANDIDATE_FILE), file_path(REJECTED_FILE));
            self.status = Some((
                "Candidate exited before atomic promotion; accepted revision preserved".into(),
                false,
            ));
            log::warn!(
                "candidate_revision_rolled_back revision={}",
                pending.revision
            );
        }
    }

    fn reject_pending_promotion(&mut self, reason: String) {
        if let Some(pending) = self.pending_promotion.take() {
            let _ = provider_client::abort_state(pending.stage_id);
            let _ = self.worker.discard_candidate(pending.request_id);
            self.candidates.remove(&pending.request_id);
        }
        let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
        self.status = Some((reason, false));
    }

    fn frame_presented(&mut self, frame: PendingFrame, cx: &mut Context<Self>) {
        let visible_us = micros(frame.timings.submitted_at.elapsed());
        let frame_callback_us = micros(frame.callback_scheduled_at.elapsed());
        let post_worker_us = visible_us.saturating_sub(frame.timings.worker_total_us);
        match frame.purpose {
            CandidatePurpose::Regular => {
                self.status = Some((
                    format!("Luau revision visible in {} ms", visible_us / 1_000),
                    true,
                ));
                log::info!(
                    "script_visible request_id={} source_to_visible_us={} queue_us={} compile_us={} render_us={} worker_total_us={} post_worker_us={} worker_to_commit_us={} commit_to_render_us={} gpui_tree_build_us={} frame_callback_us={}",
                    frame.request_id,
                    visible_us,
                    frame.timings.queue_us,
                    frame.timings.compile_us,
                    frame.timings.render_us,
                    frame.timings.worker_total_us,
                    post_worker_us,
                    frame.worker_to_commit_us,
                    frame.commit_to_render_us,
                    frame.render_build_us,
                    frame_callback_us
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
                stress
                    .worker_to_commit_latencies_us
                    .push(frame.worker_to_commit_us);
                stress
                    .commit_to_render_latencies_us
                    .push(frame.commit_to_render_us);
                stress.render_build_latencies_us.push(frame.render_build_us);
                stress.frame_callback_latencies_us.push(frame_callback_us);
                stress.rss_peak_kb = stress.rss_peak_kb.max(rss_kb);
                if stress.completed % 250 == 0 || stress.completed == stress.total {
                    stress.rss_samples.push((stress.completed, rss_kb));
                    log::info!(
                        "stress_sample run_id={} iteration={} rss_kb={} visible_us={} worker_us={} post_worker_us={} worker_to_commit_us={} commit_to_render_us={} gpui_tree_build_us={} frame_callback_us={}",
                        stress.run_id,
                        stress.completed,
                        rss_kb,
                        visible_us,
                        frame.timings.worker_total_us,
                        post_worker_us,
                        frame.worker_to_commit_us,
                        frame.commit_to_render_us,
                        frame.render_build_us,
                        frame_callback_us
                    );
                }

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
        let worker_to_commit_p95_us = percentile(&stress.worker_to_commit_latencies_us, 95);
        let commit_to_render_p95_us = percentile(&stress.commit_to_render_latencies_us, 95);
        let render_build_p95_us = percentile(&stress.render_build_latencies_us, 95);
        let frame_callback_p95_us = percentile(&stress.frame_callback_latencies_us, 95);
        let duration_ms = stress.started_at.elapsed().as_millis();
        log::info!(
            "stress_complete run_id={} total={} accepted={} rejected=0 duration_ms={} visible_p50_us={} visible_p95_us={} visible_p99_us={} visible_max_us={} worker_p95_us={} worker_to_commit_p95_us={} commit_to_render_p95_us={} gpui_tree_build_p95_us={} frame_callback_p95_us={} rss_start_kb={} rss_end_kb={} rss_peak_kb={} rss_delta_kb={} rss_samples={}",
            stress.run_id,
            stress.total,
            stress.completed,
            duration_ms,
            visible_p50_us,
            visible_p95_us,
            visible_p99_us,
            visible_max_us,
            worker_p95_us,
            worker_to_commit_p95_us,
            commit_to_render_p95_us,
            render_build_p95_us,
            frame_callback_p95_us,
            stress.rss_start_kb,
            rss_end_kb,
            stress.rss_peak_kb,
            rss_end_kb.saturating_sub(stress.rss_start_kb),
            stress.rss_samples.len()
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let element_id = node.id.clone().unwrap_or_else(|| path.to_string());
        let mut element = div();
        match &node.kind {
            NodeKind::Column => element = element.flex().flex_col(),
            NodeKind::Row => element = element.flex().flex_row(),
            NodeKind::Scroll => element = element.flex().flex_col().size_full(),
            NodeKind::Box
            | NodeKind::Spacer
            | NodeKind::Text(_)
            | NodeKind::TextInput(_)
            | NodeKind::Image(_)
            | NodeKind::Canvas(_) => {}
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
        if let NodeKind::Image(image) = &node.kind {
            debug_assert_eq!(image.asset, "album-orbit");
            element = element.child(img(ALBUM_ASSET).size_full());
        }
        if let NodeKind::TextInput(input) = &node.kind {
            self.input_state_shadow
                .entry(input.state_key.clone())
                .or_insert_with(|| input.value.clone());
            let mut created = false;
            let native = if let Some(native) = self.inputs.get(&element_id) {
                native.clone()
            } else {
                created = true;
                let host = cx.weak_entity();
                let native = cx.new(|input_cx| {
                    NativeTextInput::new(
                        element_id.clone(),
                        input.state_key.clone(),
                        input.value.clone(),
                        input.placeholder.clone(),
                        input.submit_action.clone(),
                        host,
                        window,
                        input_cx,
                    )
                });
                self.inputs.insert(element_id.clone(), native.clone());
                native
            };
            let should_activate = (created && input.autofocus)
                || self.pending_focus_restore.as_deref() == Some(element_id.as_str());
            native.update(cx, |native, native_cx| {
                native.sync(
                    &input.state_key,
                    &input.value,
                    &input.placeholder,
                    input.submit_action.as_deref(),
                    window,
                    native_cx,
                );
                if should_activate {
                    native.activate(window, native_cx);
                }
            });
            if should_activate {
                self.pending_focus_restore = None;
            }
            element = element.child(native);
        }
        if let NodeKind::Canvas(canvas) = &node.kind {
            element = element.child(native_canvas::render(
                element_id.clone(),
                canvas.clone(),
                cx.weak_entity(),
                cx,
            ));
        }
        for (index, child) in node.children.iter().enumerate() {
            let child_path = SharedString::from(format!("{path}-{index}"));
            element = element.child(self.render_node(child, child_path, window, cx));
        }
        let mut rendered = if let Some(action) = &node.action {
            let action = action.clone();
            element
                .id(SharedString::from(element_id.clone()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.dispatch(action.clone(), cx)),
                )
                .into_any_element()
        } else if matches!(node.kind, NodeKind::Scroll) {
            element
                .id(SharedString::from(element_id.clone()))
                .overflow_y_scroll()
                .into_any_element()
        } else {
            element.into_any_element()
        };

        if let Some(animation) = &node.animation {
            let animation_id = SharedString::from(format!("animation-{element_id}"));
            let native_animation = GpuiAnimation::new(Duration::from_millis(animation.duration_ms));
            let native_animation = if animation.repeat {
                native_animation.repeat()
            } else {
                native_animation
            };
            let kind = animation.kind;
            rendered = div()
                .child(rendered)
                .with_animation(animation_id, native_animation, move |element, delta| {
                    let opacity = match kind {
                        AnimationKind::Pulse => {
                            0.62 + 0.38 * (delta * std::f32::consts::TAU).sin().abs()
                        }
                        AnimationKind::FadeIn => delta,
                    };
                    element.opacity(opacity)
                })
                .into_any_element();
        }
        rendered
    }
}

impl Render for ExperienceHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reconcile_candidate_event();
        if RELOAD_REQUESTED.swap(false, Ordering::AcqRel) {
            let revision_busy = self.pending_promotion.is_some()
                || self
                    .candidates
                    .values()
                    .any(|purpose| *purpose == CandidatePurpose::Regular);
            if revision_busy {
                RELOAD_REQUESTED.store(true, Ordering::Release);
                cx.notify();
            } else {
                self.submit_reload();
            }
        }
        if WORKER_RESTART_REQUESTED.swap(false, Ordering::AcqRel) {
            self.restart_worker(cx);
        }
        let stress_request = stress_request_slot()
            .lock()
            .expect("stress request lock")
            .take();
        if let Some(stress_request) = stress_request {
            self.start_stress(stress_request);
        }
        if self.stress.is_none()
            && self.candidates.is_empty()
            && !self.action_in_flight
            && self.pending_input_event.is_some()
        {
            self.dispatch_pending_input_event(cx);
        }
        let mut pending_frame = self.pending_frame.take();
        let render_started_at = Instant::now();

        let tree = self.tree.clone();
        let content = self.render_node(&tree, SharedString::from("root"), window, cx);
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
        if let Some(mut frame) = pending_frame.take() {
            frame.commit_to_render_us =
                micros(render_started_at.duration_since(frame.committed_at));
            frame.render_build_us = micros(render_started_at.elapsed());
            frame.callback_scheduled_at = Instant::now();
            cx.on_next_frame(window, move |this, _, cx| {
                this.frame_presented(frame, cx);
            });
        }
        if self.candidate_first_frame_pending {
            self.candidate_first_frame_pending = false;
            cx.on_next_frame(window, move |this, _, cx| {
                if report_candidate_first_frame() {
                    this.dispatch_pending_input_event(cx);
                    cx.notify();
                }
            });
        }
        root
    }
}

fn read_process_config() -> Result<ProcessConfig, String> {
    let role = intent_string_extra("sos_process_role")?;
    if role.as_deref() != Some("candidate") {
        return Ok(ProcessConfig::default());
    }
    Ok(ProcessConfig {
        candidate_revision: intent_string_extra("revision")?.or_else(|| Some("unknown".into())),
        candidate_mode: intent_string_extra("mode")?.or_else(|| Some("ready".into())),
        stage_id: intent_string_extra("stage_id")?
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0),
        expected_revision: intent_string_extra("expected_revision")?
            .and_then(|value| value.parse().ok()),
    })
}

fn intent_string_extra(key: &str) -> Result<Option<String>, String> {
    jni::with_env(|env| {
        let activity = jni::activity(env)?;
        let intent = env
            .call_method(
                &activity,
                ::jni::jni_str!("getIntent"),
                ::jni::jni_sig!("()Landroid/content/Intent;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| error.to_string())?;
        let key = env.new_string(key).map_err(|error| error.to_string())?;
        let value = env
            .call_method(
                &intent,
                ::jni::jni_str!("getStringExtra"),
                ::jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&key)],
            )
            .and_then(|value| value.l())
            .map_err(|error| error.to_string())?;
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(jni::get_string(env, &value)))
        }
    })
}

fn report_candidate_first_frame() -> bool {
    let Some(config) = PROCESS_CONFIG.get() else {
        return false;
    };
    let Some(revision) = config.candidate_revision.clone() else {
        return false;
    };
    if let (Some(stage_id), Some(expected_revision)) = (config.stage_id, config.expected_revision) {
        let Some(staged) = read_state_envelope(CANDIDATE_STATE_FILE) else {
            log::error!("candidate_promotion_failed reason=missing_staged_state");
            std::process::abort();
        };
        let source = read_file(CANDIDATE_FILE).unwrap_or_default();
        let source_hash = source_sha256(&source);
        if source_hash != staged.source_sha256 {
            log::error!("candidate_promotion_failed reason=source_hash_mismatch");
            let _ = provider_client::abort_state(stage_id);
            std::process::abort();
        }
        match provider_client::promote_state(
            stage_id,
            expected_revision,
            staged.schema_version,
            &source_hash,
        ) {
            Ok(envelope) => {
                CANDIDATE_PROMOTED.store(true, Ordering::Release);
                if let Some(active) = read_file(ACTIVE_FILE) {
                    let _ = write_file(PREVIOUS_FILE, &active);
                }
                if let Err(error) = write_file(ACTIVE_FILE, &source) {
                    log::error!("candidate_active_source_write_failed error={error}");
                    std::process::abort();
                }
                let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                log::info!(
                    "candidate_revision_promoted revision={} state_revision={} source_sha256={}",
                    revision,
                    envelope.revision,
                    envelope.source_sha256
                );
            }
            Err(error) => {
                log::error!("candidate_promotion_failed error={error}");
                std::process::abort();
            }
        }
    }
    let result = jni::with_env(|env| {
        let activity = jni::activity(env)?;
        let revision = env
            .new_string(&revision)
            .map_err(|error| error.to_string())?;
        env.call_method(
            &activity,
            ::jni::jni_str!("onNativeCandidateFirstFrame"),
            ::jni::jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&revision)],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    });
    match result {
        Ok(()) => log::info!("candidate_gpui_first_frame revision={revision}"),
        Err(error) => log::error!("candidate_first_frame_report_failed error={error}"),
    }
    if config
        .candidate_mode
        .as_deref()
        .is_some_and(|mode| mode == "crash-after" || mode == "native-crash-after")
    {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));
            log::error!("candidate_native_crash phase=after_first_frame revision={revision}");
            std::process::abort();
        });
    }
    true
}

fn stress_request_slot() -> &'static Mutex<Option<StressRequest>> {
    STRESS_REQUEST.get_or_init(|| Mutex::new(None))
}

fn candidate_event_slot() -> &'static Mutex<Option<CandidateEvent>> {
    CANDIDATE_EVENT.get_or_init(|| Mutex::new(None))
}

fn loading_tree() -> UiNode {
    let mut root = UiNode {
        id: Some("startup-root".into()),
        kind: NodeKind::Column,
        ..Default::default()
    };
    root.style.padding = Some(24.);
    root.style.gap = Some(10.);
    root.children.push(UiNode {
        kind: NodeKind::Text("SOS is ready".into()),
        style: experience_ir::Style {
            text_size: Some(28.),
            color: Some(0x17211B),
            ..Default::default()
        },
        ..Default::default()
    });
    root.children.push(UiNode {
        kind: NodeKind::Text("Starting the experience runtime…".into()),
        style: experience_ir::Style {
            text_size: Some(14.),
            color: Some(0x637069),
            ..Default::default()
        },
        ..Default::default()
    });
    root
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

fn read_state_envelope(name: &str) -> Option<StateEnvelope> {
    read_file(name).and_then(|contents| serde_json::from_str(&contents).ok())
}

fn write_state_envelope(name: &str, envelope: &StateEnvelope) -> std::io::Result<()> {
    let contents = serde_json::to_string(envelope).map_err(std::io::Error::other)?;
    write_file(name, &contents)
}

fn launch_native_candidate(
    revision: &str,
    stage_id: u64,
    expected_revision: u64,
    mode: &str,
) -> Result<(), String> {
    jni::with_env(|env| {
        let activity = jni::activity(env)?;
        let revision = env
            .new_string(revision)
            .map_err(|error| error.to_string())?;
        let stage_id = env
            .new_string(stage_id.to_string())
            .map_err(|error| error.to_string())?;
        let expected_revision = env
            .new_string(expected_revision.to_string())
            .map_err(|error| error.to_string())?;
        let mode = env.new_string(mode).map_err(|error| error.to_string())?;
        env.call_method(
            &activity,
            ::jni::jni_str!("launchNativeCandidateMode"),
            ::jni::jni_sig!(
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V"
            ),
            &[
                JValue::Object(&revision),
                JValue::Object(&stage_id),
                JValue::Object(&expected_revision),
                JValue::Object(&mode),
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    })
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

fn count_semantics(root: &UiNode) -> usize {
    usize::from(root.accessibility.is_some())
        + root.children.iter().map(count_semantics).sum::<usize>()
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

fn source_sha256(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
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
