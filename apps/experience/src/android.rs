mod accessibility;
mod agent;
mod native_input;
mod network;
mod provider_client;
#[cfg(feature = "aosp-system")]
mod revision_client;

use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use experience_ir::{
    AgentMessage, AgentMessageRole, Align, AnimationKind, Content, ExperienceModel, Flow,
    HitRegion, Interaction, Justify, PaintOp, ProviderEffect, Scene, SceneEvent, SceneNode,
    StateEnvelope, TextContent, WifiSecurity, MAX_AGENT_MESSAGES, MAX_AGENT_MESSAGE_BYTES,
};
use gpui::{
    canvas, div, img, prelude::*, px, relative, rgb, Animation as GpuiAnimation, AnimationExt as _,
    AnyElement, App, Application, Context, Entity, MouseButton, Render, ScrollHandle, SharedString,
    Window, WindowOptions,
};
use gpui_mobile::{android::jni, packages::deeplink};
use runtime_luau::{CandidateTimings, RuntimeWorker, WorkerReady, WorkerResult};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::assets::{self, SosAssets, ALBUM_ASSET};
use crate::pointer_input;
use crate::scene_surface;
use crate::{
    DAILY_FLOW_AGENT_EXPERIENCE, DAILY_FLOW_EXPERIENCE, DEFAULT_EXPERIENCE, TIMEFLOW_EXPERIENCE,
};
use native_input::NativeTextInput;

static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static WORKER_RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);
static STRESS_REQUEST: OnceLock<Mutex<Option<StressRequest>>> = OnceLock::new();

fn request_host_frame() {
    gpui_mobile::TEXT_INPUT_DIRTY.store(true, Ordering::Release);
    if let Some(platform) = jni::platform() {
        platform.background_executor().dispatch_on_main_thread(|| {
            if let Some(window) = jni::platform().and_then(|platform| platform.primary_window()) {
                window.request_frame();
            }
        });
    }
}

const ACTIVE_FILE: &str = "experience.active.luau";
const CANDIDATE_FILE: &str = "experience.candidate.luau";
const CANDIDATE_STATE_FILE: &str = "experience.candidate-state.json";
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
    committed_at: Instant,
    worker_to_commit_us: u64,
    commit_to_render_us: u64,
    render_build_us: u64,
    callback_scheduled_at: Instant,
    authority_activation: Option<PendingAuthorityActivation>,
}

#[cfg_attr(not(feature = "aosp-system"), allow(dead_code))]
struct PendingAuthorityActivation {
    revision_id: String,
    state_stage_id: u64,
    previous_source: String,
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

struct GestureSession {
    region: HitRegion,
    started_at: Instant,
    start_x: f32,
    start_y: f32,
    last_x: f32,
    last_y: f32,
    last_at: Instant,
    click_count: usize,
    moved: bool,
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
    pointer_input::install();
    log::info!("sos_experience_host role=permanent");
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
    scene: Scene,
    state: JsonValue,
    remote_state_revision: Option<u64>,
    state_schema_version: u64,
    source: String,
    status: Option<(String, bool)>,
    next_request_id: u64,
    candidates: HashMap<u64, CandidatePurpose>,
    pending_authority_activations: HashMap<u64, PendingAuthorityActivation>,
    action_in_flight: bool,
    pending_input_events: VecDeque<SceneEvent>,
    input_state_shadow: HashMap<String, String>,
    inputs: HashMap<String, Entity<NativeTextInput>>,
    scroll_handles: HashMap<String, ScrollHandle>,
    surface_gestures: HashMap<String, GestureSession>,
    surface_taps: HashMap<String, (String, Instant)>,
    pending_focus_restore: Option<String>,
    pending_frame: Option<PendingFrame>,
    stress: Option<StressRun>,
    pending_reconciled_state: Option<StateEnvelope>,
    agent_updates: async_channel::Sender<agent::AgentUpdate>,
    #[cfg(feature = "aosp-system")]
    revision_assets: Vec<runtime_luau::RevisionAssetInput>,
    accessibility_dirty: bool,
}

impl ExperienceHost {
    fn new(cx: &mut Context<Self>) -> Self {
        #[cfg(feature = "aosp-system")]
        let authority_current = revision_client::current_with_retry()
            .unwrap_or_else(|error| panic!("system revision authority is required: {error}"));
        let mut model = match provider_client::snapshot() {
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
        match network::snapshot() {
            Ok(snapshot) => {
                log::info!(
                    "android_network_snapshot enabled={} connected={} validated={} networks={}",
                    snapshot.wifi_enabled,
                    snapshot.connected,
                    snapshot.validated,
                    snapshot.networks.len()
                );
                model.network = snapshot;
            }
            Err(error) => {
                log::warn!("android_network_snapshot_failed error={error}");
                model.network.error = Some("Wi-Fi status unavailable".into());
            }
        }
        match agent::status() {
            Ok(status) => {
                log::info!(
                    "android_agent_status provider={} configured={}",
                    status.provider,
                    status.configured
                );
                agent::apply_status(&mut model.agent, &status);
            }
            Err(error) => {
                log::warn!("android_agent_status_failed error={error}");
                model.agent.available = true;
                model.agent.activity = "Deterministic fake provider ready".into();
            }
        }
        #[cfg(not(feature = "aosp-system"))]
        let (state, remote_state_revision, state_schema_version, remote_source_sha256) =
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
                        Some(envelope.source_sha256),
                    )
                }
                Err(error) => {
                    #[cfg(feature = "offline-fallback")]
                    {
                        log::warn!("experience_state_fallback error={error}");
                        (load_state(), None, 1, None)
                    }
                    #[cfg(not(feature = "offline-fallback"))]
                    panic!("strict gate requires external state service: {error}")
                }
            };
        #[cfg(feature = "aosp-system")]
        let (state, remote_state_revision, state_schema_version) = {
            let provider_state = provider_client::load_state().unwrap_or_else(|error| {
                panic!("system provider/state authority is required: {error}")
            });
            let revision_state = authority_current
                .state
                .clone()
                .unwrap_or_else(|| panic!("system revision authority omitted current state"));
            if provider_state != revision_state {
                panic!("revision and provider/state authority disagree at startup");
            }
            (
                revision_state.state,
                Some(revision_state.revision),
                revision_state.schema_version,
            )
        };
        #[cfg(not(feature = "aosp-system"))]
        let source = recover_committed_source(remote_source_sha256.as_deref());
        #[cfg(feature = "aosp-system")]
        let source = authority_current
            .source
            .clone()
            .unwrap_or_else(|| panic!("system revision authority omitted current source"));
        #[cfg(feature = "aosp-system")]
        let revision_assets = revision_client::inputs(authority_current.assets);
        #[cfg(not(feature = "aosp-system"))]
        let (worker, ready) = RuntimeWorker::spawn(
            source.clone(),
            model.clone(),
            state.clone(),
            state_schema_version,
        )
        .expect("runtime worker thread must start");
        #[cfg(feature = "aosp-system")]
        let (worker, ready) = RuntimeWorker::spawn_with_assets(
            source.clone(),
            model.clone(),
            state.clone(),
            state_schema_version,
            revision_assets.clone(),
        )
        .expect("runtime worker thread must start");
        let results = worker.results();
        let (agent_updates, agent_results) = async_channel::unbounded();
        Self::attach_worker_channels(ready, results, cx);
        Self::attach_network_poll(cx);
        Self::attach_agent_updates(agent_results, cx);
        Self::attach_agent_poll(cx);
        log::info!(
            "runtime_worker_spawned ui_thread={:?}",
            thread::current().id()
        );

        Self {
            model,
            worker,
            scene: loading_scene(),
            state,
            remote_state_revision,
            state_schema_version,
            source,
            status: Some(("Starting Luau worker…".into(), true)),
            next_request_id: 1,
            candidates: HashMap::new(),
            pending_authority_activations: HashMap::new(),
            action_in_flight: false,
            pending_input_events: VecDeque::new(),
            input_state_shadow: HashMap::new(),
            inputs: HashMap::new(),
            scroll_handles: HashMap::new(),
            surface_gestures: HashMap::new(),
            surface_taps: HashMap::new(),
            pending_focus_restore: None,
            pending_frame: None,
            stress: None,
            pending_reconciled_state: None,
            agent_updates,
            #[cfg(feature = "aosp-system")]
            revision_assets,
            accessibility_dirty: true,
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

    fn attach_network_poll(cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| loop {
            executor.timer(Duration::from_secs(2)).await;
            let snapshot = network::snapshot();
            if this
                .update(cx, |this, cx| match snapshot {
                    Ok(snapshot) if snapshot != this.model.network => {
                        this.model.network = snapshot;
                        this.refresh_model_from_authority();
                        cx.notify();
                    }
                    Ok(_) => {}
                    Err(error) => log::warn!("android_network_poll_failed error={error}"),
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
    }

    fn attach_agent_updates(
        updates: async_channel::Receiver<agent::AgentUpdate>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Ok(update) = updates.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.handle_agent_update(update);
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

    fn attach_agent_poll(cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| loop {
            executor.timer(Duration::from_secs(2)).await;
            let status = agent::status();
            if this
                .update(cx, |this, cx| match status {
                    Ok(status) => {
                        let before = this.model.agent.clone();
                        agent::apply_status(&mut this.model.agent, &status);
                        if this.model.agent != before {
                            this.refresh_model_from_authority();
                            cx.notify();
                        }
                    }
                    Err(error) => log::warn!("android_agent_poll_failed error={error}"),
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
    }

    fn refresh_model_from_authority(&mut self) {
        let request_id = self.allocate_request_id();
        if let Err(error) =
            self.worker
                .refresh_model(request_id, self.model.clone(), self.state.clone())
        {
            log::warn!("android_model_refresh_start_failed error={error}");
        }
    }

    fn handle_worker_ready(&mut self, result: Result<WorkerReady, String>, cx: &mut Context<Self>) {
        #[cfg(feature = "aosp-system")]
        let _ = &cx;
        match result {
            Ok(ready) => {
                assets::install(&ready.assets);
                #[cfg(feature = "aosp-system")]
                {
                    self.revision_assets = runtime_asset_inputs(&ready.assets);
                }
                self.scene = ready.scene;
                self.state = ready.state;
                self.state_schema_version = ready.state_schema_version;
                self.status = None;
                self.accessibility_dirty = true;
                log::info!(
                    "runtime_worker_ready ui_thread={:?} worker_thread={} initialize_us={}",
                    thread::current().id(),
                    ready.worker_thread,
                    ready.initialize_us
                );
                if file_path(CANDIDATE_FILE).is_file() {
                    self.submit_reload();
                }
            }
            Err(error) if self.source.trim() != DEFAULT_EXPERIENCE.trim() => {
                #[cfg(feature = "aosp-system")]
                {
                    log::error!("system revision rejected at startup: {error}");
                    std::process::abort();
                }
                #[cfg(not(feature = "aosp-system"))]
                {
                    log::error!(
                        "active source rejected at startup: {error}; using embedded source"
                    );
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
                            self.status =
                                Some((format!("Runtime could not start: {error}"), false));
                        }
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
        #[cfg(not(feature = "aosp-system"))]
        let spawned = RuntimeWorker::spawn(
            self.source.clone(),
            self.model.clone(),
            self.state.clone(),
            self.state_schema_version,
        );
        #[cfg(feature = "aosp-system")]
        let spawned = RuntimeWorker::spawn_with_assets(
            self.source.clone(),
            self.model.clone(),
            self.state.clone(),
            self.state_schema_version,
            self.revision_assets.clone(),
        );
        match spawned {
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
            SceneEvent {
                action,
                ..Default::default()
            },
            cx,
        );
    }

    fn dispatch_event(&mut self, event: SceneEvent, cx: &mut Context<Self>) {
        let revision_activation_pending = self
            .candidates
            .values()
            .any(|purpose| *purpose == CandidatePurpose::Regular)
            || self
                .pending_frame
                .as_ref()
                .is_some_and(|frame| frame.purpose == CandidatePurpose::Regular);
        if revision_activation_pending {
            self.enqueue_pending_event(event);
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
            SceneEvent {
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
        if focused {
            let scroll_target = node_id.clone();
            let timer = cx.background_executor().timer(Duration::from_millis(250));
            cx.spawn(async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    this.scroll_node_into_view(&scroll_target);
                    cx.notify();
                });
            })
            .detach();
        }
        self.queue_input_event(
            SceneEvent {
                action: "focus_changed".into(),
                target: Some(node_id),
                value: None,
                focused: Some(focused),
                ..Default::default()
            },
            cx,
        );
    }

    fn scroll_node_into_view(&mut self, node_id: &str) {
        fn scroll_parent(node: &SceneNode, target: &str, parent: Option<&str>) -> Option<String> {
            let parent = if node.layout.scroll_y {
                node.id.as_deref().or(parent)
            } else {
                parent
            };
            if node.id.as_deref() == Some(target) {
                return parent.map(str::to_owned);
            }
            node.children
                .iter()
                .find_map(|child| scroll_parent(child, target, parent))
        }

        let Some(scroll_id) = scroll_parent(&self.scene.root, node_id, None) else {
            return;
        };
        let Some(target) = accessibility::bounds(node_id) else {
            return;
        };
        let Some(handle) = self.scroll_handles.get(&scroll_id) else {
            return;
        };
        let viewport = handle.bounds();
        let viewport_top = f32::from(viewport.origin.y) + 12.0;
        let viewport_bottom =
            viewport_top + f32::from(viewport.size.height) - 24.0 - native_input::ime_inset();
        let target_top = target[1];
        let target_bottom = target[1] + target[3];
        let current = handle.offset();
        let y = if target_bottom > viewport_bottom {
            current.y - px(target_bottom - viewport_bottom)
        } else if target_top < viewport_top {
            current.y + px(viewport_top - target_top)
        } else {
            return;
        }
        .clamp(-handle.max_offset().y, px(0.0));
        handle.set_offset(gpui::point(current.x, y));
        accessibility::mark_state_changed();
        log::info!(
            "native_text_scrolled_into_view node_id={} scroll_id={} offset_y={:.1}",
            node_id,
            scroll_id,
            f32::from(y)
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
            SceneEvent {
                action,
                target: Some(node_id),
                value: Some(value),
                focused: Some(true),
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn scene_surface_down(
        &mut self,
        surface_id: String,
        region: HitRegion,
        x: f32,
        y: f32,
        platform_click_count: usize,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let press = region.press_action.clone();
        let target = region.id.clone();
        let click_count =
            if self
                .surface_taps
                .get(&surface_id)
                .is_some_and(|(last_target, last_at)| {
                    last_target == &target
                        && now.duration_since(*last_at) <= Duration::from_millis(400)
                })
            {
                2
            } else {
                platform_click_count.max(1)
            };
        self.surface_gestures.insert(
            surface_id,
            GestureSession {
                region,
                started_at: now,
                start_x: x,
                start_y: y,
                last_x: x,
                last_y: y,
                last_at: now,
                click_count,
                moved: false,
            },
        );
        if let Some(action) = press {
            self.queue_input_event(
                scene_surface::event(action, target, x, y, "start", 0.0, 0.0, 0.0, 0.0),
                cx,
            );
        }
    }

    pub(super) fn scene_surface_move(
        &mut self,
        surface_id: String,
        specification: &Interaction,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        if !self.surface_gestures.contains_key(&surface_id) {
            let Some(region) = specification.hit_regions.iter().rev().find(|region| {
                region.drag_action.is_some()
                    && x >= region.x
                    && x <= region.x + region.width
                    && y >= region.y
                    && y <= region.y + region.height
            }) else {
                return;
            };
            let now = Instant::now();
            self.surface_gestures.insert(
                surface_id.clone(),
                GestureSession {
                    region: region.clone(),
                    started_at: now,
                    start_x: x,
                    start_y: y,
                    last_x: x,
                    last_y: y,
                    last_at: now,
                    click_count: 1,
                    moved: false,
                },
            );
            if let Some(action) = &region.press_action {
                self.queue_input_event(
                    scene_surface::event(
                        action.clone(),
                        region.id.clone(),
                        x,
                        y,
                        "start",
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                    ),
                    cx,
                );
                return;
            }
        }
        let now = Instant::now();
        let Some(session) = self.surface_gestures.get_mut(&surface_id) else {
            return;
        };
        let delta_x = x - session.start_x;
        let delta_y = y - session.start_y;
        let sample_seconds = now.duration_since(session.last_at).as_secs_f32().max(0.001);
        let velocity_x = (x - session.last_x) / sample_seconds;
        let velocity_y = (y - session.last_y) / sample_seconds;
        session.moved |= delta_x.hypot(delta_y) >= 8.0;
        session.last_x = x;
        session.last_y = y;
        session.last_at = now;
        let action = session.region.drag_action.clone();
        let target = session.region.id.clone();
        if let Some(action) = action {
            self.queue_input_event(
                scene_surface::event(
                    action, target, x, y, "update", delta_x, delta_y, velocity_x, velocity_y,
                ),
                cx,
            );
        }
    }

    pub(super) fn scene_surface_up(
        &mut self,
        surface_id: String,
        _specification: &Interaction,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.surface_gestures.remove(&surface_id) else {
            return;
        };
        let elapsed = session.started_at.elapsed();
        let seconds = elapsed.as_secs_f32().max(0.001);
        let delta_x = x - session.start_x;
        let delta_y = y - session.start_y;
        let velocity_x = delta_x / seconds;
        let velocity_y = delta_y / seconds;
        let region = session.region;
        let action = if let Some(action) = &region.drop_action {
            Some(action.clone())
        } else if session.click_count >= 2 {
            region
                .double_tap_action
                .clone()
                .or_else(|| region.tap_action.clone())
        } else if delta_x.hypot(delta_y) >= 56.0 && elapsed <= Duration::from_millis(900) {
            region.swipe_action.clone()
        } else if !session.moved && elapsed >= Duration::from_millis(500) {
            region
                .long_press_action
                .clone()
                .or_else(|| region.tap_action.clone())
        } else if !session.moved {
            region.tap_action.clone()
        } else {
            None
        };
        if region.drop_action.is_none() && !session.moved && elapsed < Duration::from_millis(500) {
            if session.click_count >= 2 {
                self.surface_taps.remove(&surface_id);
            } else {
                self.surface_taps
                    .insert(surface_id, (region.id.clone(), Instant::now()));
            }
        } else {
            self.surface_taps.remove(&surface_id);
        }
        if let Some(action) = action {
            self.queue_input_event(
                scene_surface::event(
                    action, region.id, x, y, "end", delta_x, delta_y, velocity_x, velocity_y,
                ),
                cx,
            );
        }
    }

    fn queue_input_event(&mut self, event: SceneEvent, cx: &mut Context<Self>) {
        if self.action_in_flight || self.stress.is_some() {
            self.enqueue_pending_event(event);
        } else {
            self.dispatch_event(event, cx);
        }
    }

    fn enqueue_pending_event(&mut self, event: SceneEvent) {
        let coalescible = matches!(event.phase.as_deref(), Some("move" | "update"));
        if coalescible {
            if let Some(existing) = self.pending_input_events.iter_mut().rev().find(|queued| {
                queued.action == event.action
                    && queued.target == event.target
                    && queued.pointer_id == event.pointer_id
                    && queued.phase == event.phase
            }) {
                *existing = event;
                return;
            }
        }
        if self.pending_input_events.len() >= 64 {
            if let Some(position) = self
                .pending_input_events
                .iter()
                .position(|queued| matches!(queued.phase.as_deref(), Some("move" | "update")))
            {
                self.pending_input_events.remove(position);
            } else {
                self.pending_input_events.pop_front();
            }
        }
        self.pending_input_events.push_back(event);
    }

    fn dispatch_pending_input_event(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = self.pending_input_events.pop_front() {
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
        let Some(candidate_source) = read_file(CANDIDATE_FILE) else {
            self.status = Some(("No candidate script found".into(), false));
            return;
        };
        self.submit_candidate_source(candidate_source);
    }

    fn submit_candidate_source(&mut self, candidate_source: String) {
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
                assets,
                ..
            } => {
                let Some(purpose) = self.candidates.get(&request_id).copied() else {
                    let _ = self.worker.discard_candidate(request_id);
                    return;
                };
                #[cfg(not(feature = "aosp-system"))]
                let _ = &assets;
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
                    #[cfg(not(feature = "aosp-system"))]
                    let revision = format!("{}-{}", expected_revision + 1, &hash[..12]);
                    let envelope = StateEnvelope {
                        revision: expected_revision + 1,
                        schema_version: state_schema_version,
                        source_sha256: hash.clone(),
                        state,
                    };
                    if let Err(error) = write_state_envelope(CANDIDATE_STATE_FILE, &envelope) {
                        let _ = self.worker.discard_candidate(request_id);
                        self.candidates.remove(&request_id);
                        self.status =
                            Some((format!("Could not persist staged revision: {error}"), false));
                        return;
                    }
                    #[cfg(feature = "aosp-system")]
                    {
                        self.status =
                            Some(("Candidate validated; staging system revision…".into(), true));
                        let revision_id = match revision_client::install(
                            source.clone(),
                            envelope.state.clone(),
                            state_schema_version,
                            &assets,
                        ) {
                            Ok(revision_id) => revision_id,
                            Err(error) => {
                                let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                                let _ = self.worker.discard_candidate(request_id);
                                self.candidates.remove(&request_id);
                                self.status =
                                    Some((format!("Could not install revision: {error}"), false));
                                return;
                            }
                        };
                        let stage_id = match provider_client::stage_state(
                            expected_revision,
                            state_schema_version,
                            &envelope.state,
                            &hash,
                            &[],
                        ) {
                            Ok(stage_id) => stage_id,
                            Err(error) => {
                                let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                                let _ = self.worker.discard_candidate(request_id);
                                self.candidates.remove(&request_id);
                                self.status =
                                    Some((format!("Could not stage revision: {error}"), false));
                                return;
                            }
                        };
                        self.pending_authority_activations.insert(
                            request_id,
                            PendingAuthorityActivation {
                                revision_id,
                                state_stage_id: stage_id,
                                previous_source: self.source.clone(),
                            },
                        );
                        self.status = Some((
                            "Candidate staged; activating scene before system commit…".into(),
                            true,
                        ));
                        if let Err(error) = self.worker.commit_candidate(request_id) {
                            self.candidates.remove(&request_id);
                            self.pending_authority_activations.remove(&request_id);
                            let _ = provider_client::abort_state(stage_id);
                            let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                            self.status =
                                Some((format!("Candidate could not activate: {error}"), false));
                        }
                        return;
                    }
                    #[cfg(not(feature = "aosp-system"))]
                    {
                        self.status = Some((
                            "Candidate validated; committing state and activating scene…".into(),
                            true,
                        ));
                        let stage_id = match provider_client::stage_state(
                            expected_revision,
                            state_schema_version,
                            &envelope.state,
                            &hash,
                            &[],
                        ) {
                            Ok(stage_id) => stage_id,
                            Err(error) => {
                                let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                                let _ = self.worker.discard_candidate(request_id);
                                self.candidates.remove(&request_id);
                                self.status =
                                    Some((format!("Could not stage revision: {error}"), false));
                                return;
                            }
                        };
                        let committed = match provider_client::commit_staged_state(
                            stage_id,
                            expected_revision,
                            state_schema_version,
                            &hash,
                        ) {
                            Ok(envelope) => envelope,
                            Err(error) => {
                                let _ = provider_client::abort_state(stage_id);
                                let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                                let _ = self.worker.discard_candidate(request_id);
                                self.candidates.remove(&request_id);
                                self.status =
                                    Some((format!("Could not commit revision: {error}"), false));
                                return;
                            }
                        };
                        if let Some(active) = read_file(ACTIVE_FILE) {
                            let _ = write_file(PREVIOUS_FILE, &active);
                        }
                        if let Err(error) = write_file(ACTIVE_FILE, &source) {
                            log::error!("active_source_write_failed error={error}");
                        }
                        self.remote_state_revision = Some(committed.revision);
                        self.pending_reconciled_state = Some(committed.clone());
                        let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                        log::info!(
                        "experience_revision_committed revision={} state_revision={} source_sha256={}",
                        revision,
                        committed.revision,
                        committed.source_sha256
                    );
                        if let Err(error) = self.worker.commit_candidate(request_id) {
                            self.candidates.remove(&request_id);
                            self.source = source;
                            self.state = committed.state;
                            self.state_schema_version = committed.schema_version;
                            self.pending_reconciled_state = None;
                            self.status = Some((
                                format!("Committed revision could not activate: {error}"),
                                false,
                            ));
                            self.restart_worker(cx);
                        }
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
                scene,
                state,
                state_schema_version,
                timings,
                assets,
            } => {
                let Some(purpose) = self.candidates.remove(&request_id) else {
                    return;
                };
                let authority_activation = self.pending_authority_activations.remove(&request_id);
                self.pending_focus_restore = native_input::active_input_id();
                assets::install(&assets);
                #[cfg(feature = "aosp-system")]
                {
                    self.revision_assets = runtime_asset_inputs(&assets);
                }
                self.source = source;
                self.scene = scene;
                self.state = state;
                self.state_schema_version = state_schema_version;
                if let Some(envelope) = self.pending_reconciled_state.take() {
                    self.state = envelope.state;
                    self.state_schema_version = envelope.schema_version;
                    self.remote_state_revision = Some(envelope.revision);
                }
                self.accessibility_dirty = true;
                if purpose == CandidatePurpose::Regular {
                    #[cfg(not(feature = "aosp-system"))]
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
                    authority_activation,
                });
            }
            WorkerResult::ActionCompleted {
                request_id,
                mut state,
                scene,
                effects,
                worker_us,
            } => {
                self.action_in_flight = false;
                self.merge_native_input_state(&mut state);
                let effect_count = effects.len();
                let (host_effects, provider_effects): (Vec<_>, Vec<_>) = effects
                    .into_iter()
                    .partition(|effect| matches!(effect.provider.as_str(), "network" | "agent"));
                if let Some(expected_revision) = self.remote_state_revision {
                    let source_sha256 = source_sha256(&self.source);
                    let mut committed = provider_client::commit_state(
                        expected_revision,
                        self.state_schema_version,
                        &state,
                        &source_sha256,
                        &provider_effects,
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
                                    &provider_effects,
                                );
                            }
                        }
                    }
                    match committed {
                        Ok(envelope) => {
                            self.remote_state_revision = Some(envelope.revision);
                            state = envelope.state;
                            log::info!(
                                "experience_state_committed revision={} schema_version={} source_sha256={} effects={}",
                                envelope.revision,
                                envelope.schema_version,
                                envelope.source_sha256,
                                effect_count
                            );
                        }
                        Err(error) => {
                            self.status = Some((format!("State commit failed: {error}"), false));
                            log::warn!("experience_state_rejected error={error}");
                            self.dispatch_pending_input_event(cx);
                            return;
                        }
                    }
                }
                self.state = state;
                self.scene = scene;
                self.accessibility_dirty = true;
                persist_state(&self.state);
                self.status = None;
                let (network_effects, agent_effects): (Vec<_>, Vec<_>) = host_effects
                    .into_iter()
                    .partition(|effect| effect.provider == "network");
                self.execute_network_effects(network_effects);
                self.execute_agent_effects(agent_effects);
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
            WorkerResult::ModelRefreshed {
                request_id,
                scene,
                worker_us,
            } => {
                self.scene = scene;
                self.accessibility_dirty = true;
                log::info!(
                    "experience_model_refreshed request_id={request_id} worker_us={worker_us}"
                );
            }
            WorkerResult::ModelRefreshRejected {
                request_id,
                error,
                worker_us,
            } => {
                self.status = Some((format!("Model refresh rejected: {error}"), false));
                log::warn!(
                    "experience_model_refresh_rejected request_id={request_id} worker_us={worker_us} error={error}"
                );
            }
        }
    }

    fn execute_network_effects(&mut self, effects: Vec<ProviderEffect>) {
        for effect in effects {
            let result = match effect.action.as_str() {
                "refresh" => network::refresh(),
                "disconnect" => network::disconnect(),
                "connect" => {
                    let ssid = effect.payload.get("ssid").and_then(JsonValue::as_str);
                    let security = effect.payload.get("security").and_then(JsonValue::as_str);
                    match (ssid, security) {
                        (Some(ssid), Some(security)) => self
                            .model
                            .network
                            .networks
                            .iter()
                            .find(|network| {
                                let trusted_security = match network.security {
                                    WifiSecurity::Open => "open",
                                    WifiSecurity::Personal => "personal",
                                    WifiSecurity::Enterprise => "enterprise",
                                };
                                network.ssid == ssid && trusted_security == security
                            })
                            .map(|selected| network::connect(&selected.ssid, selected.security))
                            .unwrap_or_else(|| {
                                Err("network is not present in the trusted scan snapshot".into())
                            }),
                        _ => Err("network.connect omitted its bounded selection".into()),
                    }
                }
                _ => Err(format!(
                    "unsupported trusted network action: {}",
                    effect.action
                )),
            };
            if let Err(error) = result {
                self.status = Some((format!("Network action failed: {error}"), false));
                log::warn!("android_network_action_failed action={}", effect.action);
            }
        }
        if let Ok(snapshot) = network::snapshot() {
            self.model.network = snapshot;
            self.refresh_model_from_authority();
        }
    }

    fn execute_agent_effects(&mut self, effects: Vec<ProviderEffect>) {
        for effect in effects {
            let result = match effect.action.as_str() {
                "configure_openai" => agent::configure_openai(),
                "configure_openrouter" => agent::configure_openrouter(),
                "configure_codex" => agent::configure_codex(),
                "use_fake" => agent::use_fake(),
                "clear_credential" => agent::clear_credential(),
                "prompt" => effect
                    .payload
                    .get("prompt")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|prompt| !prompt.is_empty() && prompt.len() <= MAX_AGENT_MESSAGE_BYTES)
                    .map(|prompt| self.start_agent_prompt(prompt.to_owned()))
                    .unwrap_or_else(|| {
                        Err("agent.prompt omitted a bounded non-empty prompt".into())
                    }),
                _ => Err(format!(
                    "unsupported trusted agent action: {}",
                    effect.action
                )),
            };
            if let Err(error) = result {
                self.status = Some((format!("Agent action failed: {error}"), false));
                log::warn!("android_agent_action_failed action={}", effect.action);
            }
        }
        if let Ok(status) = agent::status() {
            agent::apply_status(&mut self.model.agent, &status);
            self.refresh_model_from_authority();
        }
    }

    fn start_agent_prompt(&mut self, prompt: String) -> Result<(), String> {
        if self.model.agent.busy {
            return Err("the resident agent is already handling a prompt".into());
        }
        let status = agent::status()?;
        if status.provider != "fake" && !status.configured {
            return Err("The selected Pi provider has no configured credential".into());
        }
        agent::spawn_prompt(
            status,
            prompt,
            self.source.clone(),
            self.model.clone(),
            self.agent_updates.clone(),
        );
        Ok(())
    }

    fn handle_agent_update(&mut self, update: agent::AgentUpdate) {
        match update {
            agent::AgentUpdate::Started { prompt } => {
                self.model.agent.busy = true;
                self.model.agent.error = None;
                self.model.agent.activity = "Understanding the request".into();
                push_agent_message(&mut self.model, AgentMessageRole::User, prompt);
            }
            agent::AgentUpdate::ToolStarted(name) => {
                self.model.agent.activity = format!("Using {}", display_agent_tool(&name));
            }
            agent::AgentUpdate::ToolFinished { name, ok } => {
                self.model.agent.activity = if ok {
                    format!("{} complete", display_agent_tool(&name))
                } else {
                    format!("{} failed", display_agent_tool(&name))
                };
            }
            agent::AgentUpdate::Candidate { source, summary } => {
                push_agent_message(&mut self.model, AgentMessageRole::Assistant, summary);
                self.model.agent.activity = "Validating the proposed experience".into();
                self.submit_candidate_source(source);
            }
            agent::AgentUpdate::Completed => {
                self.model.agent.busy = false;
                self.model.agent.activity = agent::status()
                    .map(|status| status.activity)
                    .unwrap_or_else(|_| "Agent ready".into());
            }
            agent::AgentUpdate::Failed(error) => {
                self.model.agent.busy = false;
                self.model.agent.activity = "Agent request failed".into();
                self.model.agent.error = Some(error);
            }
        }
        self.refresh_model_from_authority();
    }

    fn publish_accessibility(&self, cx: &App) {
        let text = self
            .inputs
            .iter()
            .map(|(id, input)| (id.clone(), input.read(cx).accessibility_state()))
            .collect::<HashMap<_, _>>();
        let scroll = self
            .scroll_handles
            .iter()
            .map(|(id, handle)| {
                (
                    id.clone(),
                    accessibility::ScrollState {
                        offset_y: -f32::from(handle.offset().y),
                        max_offset_y: f32::from(handle.max_offset().y),
                        bounds: {
                            let bounds = handle.bounds();
                            [
                                f32::from(bounds.origin.x),
                                f32::from(bounds.origin.y),
                                f32::from(bounds.size.width),
                                f32::from(bounds.size.height),
                            ]
                        },
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        match accessibility::publish(&self.scene, &text, &scroll) {
            Ok(bytes) => log::info!(
                "accessibility_published bytes={} semantics={}",
                bytes,
                count_semantics(&self.scene)
            ),
            Err(error) => log::warn!("accessibility_publish_failed error={error}"),
        }
    }

    fn frame_presented(&mut self, frame: PendingFrame, cx: &mut Context<Self>) {
        let visible_us = micros(frame.timings.submitted_at.elapsed());
        let frame_callback_us = micros(frame.callback_scheduled_at.elapsed());
        let post_worker_us = visible_us.saturating_sub(frame.timings.worker_total_us);
        match frame.purpose {
            CandidatePurpose::Regular => {
                #[cfg(feature = "aosp-system")]
                {
                    let activation = frame.authority_activation.unwrap_or_else(|| {
                        log::error!(
                            "system candidate reached presentation without activation metadata"
                        );
                        std::process::abort();
                    });
                    let activated = revision_client::activate(
                        activation.revision_id.clone(),
                        activation.state_stage_id,
                    )
                    .unwrap_or_else(|error| {
                        log::error!(
                            "system_revision_activation_failed revision={} stage_id={} error={error}",
                            activation.revision_id,
                            activation.state_stage_id
                        );
                        std::process::abort();
                    });
                    if activated.revision_id.as_deref() != Some(&activation.revision_id)
                        || activated.source.as_deref() != Some(self.source.as_str())
                    {
                        log::error!(
                            "system_revision_activation_mismatch expected_revision={}",
                            activation.revision_id
                        );
                        std::process::abort();
                    }
                    let envelope = activated.state.unwrap_or_else(|| {
                        log::error!("system revision activation omitted committed state");
                        std::process::abort();
                    });
                    if let Err(error) = write_file(PREVIOUS_FILE, &activation.previous_source) {
                        log::warn!("previous_source_cache_write_failed error={error}");
                    }
                    if let Err(error) = write_file(ACTIVE_FILE, &self.source) {
                        log::warn!("active_source_cache_write_failed error={error}");
                    }
                    self.remote_state_revision = Some(envelope.revision);
                    self.state_schema_version = envelope.schema_version;
                    self.state = envelope.state;
                    let _ = fs::remove_file(file_path(CANDIDATE_FILE));
                    let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
                    log::info!(
                        "android_authority_revision_activated revision={} state_revision={} source_sha256={}",
                        activation.revision_id,
                        envelope.revision,
                        envelope.source_sha256
                    );
                }
                #[cfg(not(feature = "aosp-system"))]
                let _ = frame.authority_activation;
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
        node: &SceneNode,
        path: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let element_id = node.id.clone().unwrap_or_else(|| path.to_string());
        let mut element = div();
        match node.layout.flow {
            Flow::Overlay => {}
            Flow::Column => element = element.flex().flex_col(),
            Flow::Row => element = element.flex().flex_row(),
        }
        if node.layout.scroll_y {
            element = element.size_full();
        }

        for operation in &node.paint {
            if let PaintOp::FillBounds { color, radius } = operation {
                element = element.bg(rgb(*color)).rounded(px(*radius));
            }
        }
        if let Some(padding) = node.layout.padding {
            element = element.p(px(padding));
        }
        if let Some(gap) = node.layout.gap {
            element = element.gap(px(gap));
        }
        if let Some(width) = node.layout.width {
            element = element.w(px(width));
        }
        if let Some(height) = node.layout.height {
            element = element.h(px(height));
        }
        if let Some(width) = node.layout.min_width {
            element = element.min_w(px(width));
        }
        if let Some(height) = node.layout.min_height {
            element = element.min_h(px(height));
        }
        if let Some(width) = node.layout.max_width {
            element = element.max_w(px(width));
        }
        if let Some(height) = node.layout.max_height {
            element = element.max_h(px(height));
        }
        if let Some(ratio) = node.layout.aspect_ratio {
            element = element.aspect_ratio(ratio);
        }
        if let Some(position) = node.layout.position {
            element = element.absolute().left(px(position.x)).top(px(position.y));
        }
        if let Some(program) = node.layout.program {
            if let Some(width) = program.measure_width {
                element = element.w(relative(width));
            }
            if let Some(height) = program.measure_height {
                element = element.h(relative(height));
            }
            if program.arrange_x.is_some() || program.arrange_y.is_some() {
                element = element.absolute();
            }
            if let Some(x) = program.arrange_x {
                element = element.left(relative(x));
            }
            if let Some(y) = program.arrange_y {
                element = element.top(relative(y));
            }
        }
        if node.layout.clip_bounds {
            element = element.overflow_hidden();
        }
        if node.layout.grow {
            element = element.flex_1();
        }
        element = match node.layout.align {
            Some(Align::Start) => element.items_start(),
            Some(Align::Center) => element.items_center(),
            Some(Align::End) => element.items_end(),
            None => element,
        };
        element = match node.layout.justify {
            Some(Justify::Start) => element.justify_start(),
            Some(Justify::Center) => element.justify_center(),
            Some(Justify::End) => element.justify_end(),
            Some(Justify::Between) => element.justify_between(),
            None => element,
        };
        if let Some(Content::Text(text)) = &node.content {
            element = element
                .text_color(rgb(text.color))
                .text_size(px(text.size))
                .child(SharedString::from(text.value.clone()));
        }
        if let Some(Content::Image(image)) = &node.content {
            let path = if image.asset == "album-orbit" {
                ALBUM_ASSET.to_owned()
            } else {
                image.asset.clone()
            };
            element = element.child(img(path).size_full());
        }
        if matches!(&node.content, Some(Content::ProviderSurface(_))) {
            element = element.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Provider surface unavailable on this host"),
            );
        }
        if let Some(Content::TextSession(input)) = &node.content {
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
        if node.semantics.is_some() || node.layout.scroll_y {
            let semantic_id = element_id.clone();
            element = element.child(
                canvas(
                    move |bounds, _, _| accessibility::record_bounds(&semantic_id, bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        }
        let uses_surface = node
            .paint
            .iter()
            .any(|operation| !matches!(operation, PaintOp::FillBounds { .. }))
            || !node.interaction.hit_regions.is_empty()
            || node.interaction.double_tap_action.is_some()
            || node.interaction.long_press_action.is_some()
            || node.interaction.swipe_action.is_some()
            || node.interaction.pointer_action.is_some()
            || node.interaction.multi_pointer_action.is_some();
        if uses_surface {
            element = element.child(scene_surface::render(
                element_id.clone(),
                node.paint.clone(),
                node.interaction.clone(),
                cx.weak_entity(),
                cx,
            ));
        }
        for (index, child) in node.children.iter().enumerate() {
            let child_path = SharedString::from(format!("{path}-{index}"));
            element = element.child(self.render_node(child, child_path, window, cx));
        }
        if node.layout.scroll_y {
            let ime_inset = native_input::ime_inset();
            if ime_inset > 0.0 && native_input::active_input_id().is_some() {
                element = element.child(div().flex_none().h(px(ime_inset)));
            }
        }
        let surface_owns_tap = uses_surface && node.interaction.tap_action.is_some();
        let tap_action = node
            .interaction
            .tap_action
            .as_ref()
            .filter(|_| !surface_owns_tap)
            .cloned();
        let mut rendered = if node.layout.scroll_y {
            let handle = self
                .scroll_handles
                .entry(element_id.clone())
                .or_default()
                .clone();
            let scroll = element
                .id(SharedString::from(element_id.clone()))
                .track_scroll(&handle)
                .overflow_y_scroll();
            if let Some(action) = tap_action {
                scroll
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| this.dispatch(action.clone(), cx)),
                    )
                    .into_any_element()
            } else {
                scroll.into_any_element()
            }
        } else if let Some(action) = tap_action {
            let action = action.clone();
            element
                .id(SharedString::from(element_id.clone()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.dispatch(action.clone(), cx)),
                )
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

impl scene_surface::SceneSurfaceHost for ExperienceHost {
    fn record_scene_surface(
        surface_id: &str,
        bounds: gpui::Bounds<gpui::Pixels>,
        interaction: &Interaction,
    ) {
        pointer_input::record_surface(surface_id, bounds, interaction);
    }

    fn scene_surface_down(
        &mut self,
        surface_id: String,
        region: HitRegion,
        _specification: &Interaction,
        position: (f32, f32),
        platform_click_count: usize,
        cx: &mut Context<Self>,
    ) {
        let (x, y) = position;
        ExperienceHost::scene_surface_down(
            self,
            surface_id,
            region,
            x,
            y,
            platform_click_count,
            cx,
        );
    }

    fn scene_surface_move(
        &mut self,
        surface_id: String,
        specification: &Interaction,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        ExperienceHost::scene_surface_move(self, surface_id, specification, x, y, cx);
    }

    fn scene_surface_up(
        &mut self,
        surface_id: String,
        specification: &Interaction,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        ExperienceHost::scene_surface_up(self, surface_id, specification, x, y, cx);
    }
}

impl Render for ExperienceHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if native_input::take_ime_inset_changed() && native_input::ime_inset() > 0.0 {
            if let Some(scroll_target) = native_input::active_input_id() {
                let timer = cx.background_executor().timer(Duration::from_millis(50));
                cx.spawn(async move |this, cx| {
                    timer.await;
                    let _ = this.update(cx, |this, cx| {
                        this.scroll_node_into_view(&scroll_target);
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        for state in native_input::take_ime_states() {
            let node_id = state.node_id.clone();
            if let Some(input) = self.inputs.get(&node_id).cloned() {
                let outcome =
                    input.update(cx, |input, input_cx| input.apply_ime_state(state, input_cx));
                if outcome.changed {
                    self.native_input_changed(
                        node_id.clone(),
                        outcome.state_key,
                        outcome.value.clone(),
                        cx,
                    );
                }
                if let Some(action) = outcome.submit_action {
                    self.native_input_submitted(node_id, action, outcome.value, cx);
                }
            }
        }
        for sample in pointer_input::take_samples() {
            for event in pointer_input::route(sample) {
                self.queue_input_event(event, cx);
            }
        }
        while let Some(action) = accessibility::take_action() {
            match action.kind.as_str() {
                "click" if !action.value.is_empty() => self.queue_input_event(
                    SceneEvent {
                        action: action.value,
                        target: Some(action.target),
                        ..Default::default()
                    },
                    cx,
                ),
                "focus" => {
                    if let Some(input) = self.inputs.get(&action.target).cloned() {
                        input.update(cx, |input, input_cx| input.activate(window, input_cx));
                    }
                }
                "set_text" => {
                    let state_key = find_text_session(&self.scene.root, &action.target)
                        .map(|input| input.state_key.clone());
                    if let Some(state_key) = state_key {
                        if let Some(input) = self.inputs.get(&action.target).cloned() {
                            let replacement = action.value.clone();
                            input.update(cx, |input, input_cx| {
                                input.replace_from_accessibility(replacement, input_cx)
                            });
                        }
                        self.native_input_changed(action.target, state_key, action.value, cx);
                    }
                }
                "set_selection" => {
                    let selection = action.value.split_once(':').and_then(|(start, end)| {
                        Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                    });
                    if let (Some((start, end)), Some(input)) =
                        (selection, self.inputs.get(&action.target).cloned())
                    {
                        input.update(cx, |input, input_cx| {
                            input.set_selection_from_accessibility(start, end, input_cx)
                        });
                    }
                }
                "copy" | "cut" | "paste" => {
                    if let Some(input) = self.inputs.get(&action.target).cloned() {
                        let command = action.kind.clone();
                        input.update(cx, |input, input_cx| {
                            input.accessibility_clipboard_action(&command, window, input_cx)
                        });
                    }
                }
                "scroll_forward" | "scroll_backward" => {
                    if let Some(handle) = self.scroll_handles.get(&action.target) {
                        let current = handle.offset();
                        let page = handle.bounds().size.height * 0.8;
                        let y = if action.kind == "scroll_forward" {
                            current.y - page
                        } else {
                            current.y + page
                        }
                        .clamp(-handle.max_offset().y, px(0.));
                        handle.set_offset(gpui::point(current.x, y));
                        accessibility::mark_state_changed();
                        cx.notify();
                    }
                }
                _ => log::warn!("unsupported accessibility action kind={}", action.kind),
            }
        }
        if RELOAD_REQUESTED.swap(false, Ordering::AcqRel) {
            let revision_busy = self
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
            && self.pending_frame.is_none()
            && !self.pending_input_events.is_empty()
        {
            self.dispatch_pending_input_event(cx);
        }
        let mut pending_frame = self.pending_frame.take();
        let render_started_at = Instant::now();

        pointer_input::begin_frame();
        assets::install_fonts(window);
        let scene = self.scene.clone();
        let content = self.render_node(&scene.root, SharedString::from("root"), window, cx);
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
        if self.accessibility_dirty
            || accessibility::take_bounds_changed()
            || accessibility::take_state_changed()
        {
            self.accessibility_dirty = false;
            cx.on_next_frame(window, |this, _, cx| this.publish_accessibility(cx));
        }
        root
    }
}

fn find_text_session<'a>(node: &'a SceneNode, id: &str) -> Option<&'a experience_ir::TextSession> {
    if node.id.as_deref() == Some(id) {
        if let Some(Content::TextSession(input)) = &node.content {
            return Some(input);
        }
    }
    node.children
        .iter()
        .find_map(|child| find_text_session(child, id))
}

fn stress_request_slot() -> &'static Mutex<Option<StressRequest>> {
    STRESS_REQUEST.get_or_init(|| Mutex::new(None))
}

fn loading_scene() -> Scene {
    Scene {
        root: SceneNode {
            id: Some("startup-root".into()),
            layout: experience_ir::Layout {
                flow: Flow::Column,
                padding: Some(24.),
                gap: Some(10.),
                ..Default::default()
            },
            children: vec![
                SceneNode {
                    content: Some(Content::Text(TextContent {
                        value: "SOS is ready".into(),
                        size: 28.,
                        color: 0x17211B,
                    })),
                    ..Default::default()
                },
                SceneNode {
                    content: Some(Content::Text(TextContent {
                        value: "Starting the experience runtime…".into(),
                        size: 14.,
                        color: 0x637069,
                    })),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    }
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

#[cfg(feature = "aosp-system")]
fn runtime_asset_inputs(
    assets: &[runtime_luau::RevisionAsset],
) -> Vec<runtime_luau::RevisionAssetInput> {
    assets
        .iter()
        .map(|asset| runtime_luau::RevisionAssetInput {
            id: asset.id.clone(),
            kind: asset.kind.clone(),
            bytes: asset.bytes.clone(),
        })
        .collect()
}

#[cfg(not(feature = "aosp-system"))]
fn recover_committed_source(remote_source_sha256: Option<&str>) -> String {
    let active = read_file(ACTIVE_FILE).unwrap_or_else(|| DEFAULT_EXPERIENCE.to_owned());
    let Some(remote_source_sha256) = remote_source_sha256.filter(|hash| !hash.is_empty()) else {
        return active;
    };
    if source_sha256(&active) == remote_source_sha256 {
        if read_file(CANDIDATE_FILE)
            .is_some_and(|candidate| source_sha256(&candidate) == remote_source_sha256)
        {
            let _ = fs::remove_file(file_path(CANDIDATE_FILE));
            let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
            log::info!("committed_source_cache_reconciled source_sha256={remote_source_sha256}");
        }
        return active;
    }
    let Some(candidate) = read_file(CANDIDATE_FILE)
        .filter(|candidate| source_sha256(candidate) == remote_source_sha256)
    else {
        log::error!("committed_source_recovery_failed source_sha256={remote_source_sha256}");
        return active;
    };
    if let Err(error) = write_file(PREVIOUS_FILE, &active) {
        log::warn!("previous_source_recovery_write_failed error={error}");
    }
    if let Err(error) = write_file(ACTIVE_FILE, &candidate) {
        log::error!("committed_source_recovery_write_failed error={error}");
    } else {
        let _ = fs::remove_file(file_path(CANDIDATE_FILE));
        let _ = fs::remove_file(file_path(CANDIDATE_STATE_FILE));
        log::info!("committed_source_recovered source_sha256={remote_source_sha256}");
    }
    candidate
}

fn write_state_envelope(name: &str, envelope: &StateEnvelope) -> std::io::Result<()> {
    let contents = serde_json::to_string(envelope).map_err(std::io::Error::other)?;
    write_file(name, &contents)
}

#[cfg(feature = "offline-fallback")]
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

fn push_agent_message(model: &mut ExperienceModel, role: AgentMessageRole, mut text: String) {
    if text.len() > MAX_AGENT_MESSAGE_BYTES {
        let mut boundary = MAX_AGENT_MESSAGE_BYTES;
        while !text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        text.truncate(boundary);
    }
    if text.is_empty() {
        return;
    }
    model.agent.messages.push(AgentMessage { role, text });
    if model.agent.messages.len() > MAX_AGENT_MESSAGES {
        let excess = model.agent.messages.len() - MAX_AGENT_MESSAGES;
        model.agent.messages.drain(..excess);
    }
}

fn display_agent_tool(name: &str) -> &str {
    match name {
        "get_experience_context" => "experience context",
        "propose_experience" => "experience author",
        _ => "agent tool",
    }
}

fn current_rss_kb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn count_semantics(scene: &Scene) -> usize {
    fn visit(root: &SceneNode) -> usize {
        usize::from(root.semantics.is_some()) + root.children.iter().map(visit).sum::<usize>()
    }
    visit(&scene.root)
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
