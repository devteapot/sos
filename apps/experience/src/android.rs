mod accessibility;
mod agent;
#[cfg(feature = "core-native")]
mod core_input;
mod native_input;
#[cfg(not(feature = "aosp-system"))]
mod network;
mod provider_client;
#[cfg(feature = "aosp-system")]
mod revision_client;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(feature = "core-native")]
use std::{
    ffi::{c_char, c_void, CStr},
    ptr::NonNull,
    sync::{atomic::AtomicI32, Arc},
};

#[cfg(not(feature = "aosp-system"))]
use experience_ir::WifiSecurity;
use experience_ir::{
    AgentMessage, AgentMessageRole, Align, AnimationKind, Content, ExperienceModel,
    ExperienceViewport, Flow, HitRegion, Interaction, Justify, PaintOp, ProviderEffect, Scene,
    SceneEvent, SceneNode, StateEnvelope, TextContent, MAX_AGENT_MESSAGES, MAX_AGENT_MESSAGE_BYTES,
};
#[cfg(feature = "aosp-system")]
use experience_ir::{SemanticRole, Semantics};
use experience_package::{
    canonical_sha256, ExperienceId, ExperienceRole, GraphNodeId, InstanceId, PackageMetadata,
    RevisionId, StateMigrationRecord, StateMigrationSource,
};
use gpui::{
    canvas, div, img, prelude::*, px, relative, rgb, Animation as GpuiAnimation, AnimationExt as _,
    AnyElement, App, Application, Context, Entity, MouseButton, Render, ScrollHandle, SharedString,
    Window, WindowOptions,
};
#[cfg(feature = "core-native")]
use gpui_mobile::android::AndroidPlatform;
use gpui_mobile::android::{jni, SharedPlatform};
#[cfg(not(feature = "core-native"))]
use gpui_mobile::packages::deeplink;
use runtime_luau::{CandidateTimings, RuntimeWorker};
#[cfg(feature = "aosp-system")]
use runtime_luau::{
    GraphRevisionInput, GraphRuntimeSnapshot, GraphRuntimeWorker, GraphWorkerResult,
    RuntimeInstanceStatus,
};
#[cfg(not(feature = "aosp-system"))]
use runtime_luau::{WorkerReady, WorkerResult};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
#[cfg(feature = "core-native")]
use zeroize::Zeroize;

use crate::android_agent_contract::{AgentActivationEvidence, AgentActivationPhase};
use crate::android_interaction_contract::semantic_tracker_offset;
#[cfg(not(feature = "core-native"))]
use crate::android_interaction_contract::{text_tap_outcome, TextTapOutcome};
use crate::assets::{self, SosAssets, ALBUM_ASSET};
use crate::deterministic_mobile_agent_candidate;
#[cfg(feature = "aosp-system")]
use crate::graph_scene::composed_graph_scene;
use crate::pointer_input;
use crate::scene_surface;
#[cfg(not(feature = "aosp-system"))]
use crate::MOBILE_EXPERIENCE;
use native_input::NativeTextInput;

static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static ROLLBACK_REQUESTED: AtomicBool = AtomicBool::new(false);
static WORKER_RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "aosp-system")]
static APPEARANCE_TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);
static STRESS_REQUEST: OnceLock<Mutex<Option<StressRequest>>> = OnceLock::new();
#[cfg(feature = "aosp-system")]
static EXPERIENCE_LIFECYCLE_REQUEST: OnceLock<Mutex<Option<AndroidExperienceLifecycle>>> =
    OnceLock::new();
#[cfg(feature = "aosp-system")]
static REFERENCE_GRAPH_EVENT_REQUEST: OnceLock<Mutex<Option<AndroidReferenceGraphEvent>>> =
    OnceLock::new();
#[cfg(not(feature = "core-native"))]
static MOBILE_NAVIGATION_REQUEST: OnceLock<Mutex<Option<String>>> = OnceLock::new();
#[cfg(feature = "aosp-system")]
const ANDROID_SYSTEM_THEME_ID: &str = "android-system-theme";
#[cfg(feature = "aosp-system")]
const ANDROID_SYSTEM_ROLLBACK_ID: &str = "android-system-rollback";
#[cfg(feature = "aosp-system")]
const ANDROID_SYSTEM_HOME_ID: &str = "android-system-home";
#[cfg(feature = "aosp-system")]
const STOCK_MOBILE_EXPERIENCE_ID: &str = "sos.stock.mobile";
#[cfg(feature = "aosp-system")]
const STOCK_MOBILE_THEME_ACTION: &str = "stock_system_theme";
#[cfg(feature = "aosp-system")]
const STOCK_MOBILE_ROLLBACK_ACTION: &str = "stock_system_rollback";
#[cfg(feature = "core-native")]
static CORE_PLATFORM: OnceLock<Mutex<Option<Arc<AndroidPlatform>>>> = OnceLock::new();
#[cfg(feature = "core-native")]
static CORE_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "core-native")]
static CORE_EXIT_REASON: AtomicI32 = AtomicI32::new(0);

fn request_host_frame() {
    #[cfg(not(feature = "core-native"))]
    {
        gpui_mobile::TEXT_INPUT_DIRTY.store(true, Ordering::Release);
        if let Some(platform) = jni::platform() {
            platform.background_executor().dispatch_on_main_thread(|| {
                if let Some(window) = jni::platform().and_then(|platform| platform.primary_window())
                {
                    window.request_frame();
                }
            });
        }
    }
    #[cfg(feature = "core-native")]
    if let Some(platform) = core_platform_slot()
        .lock()
        .expect("Core platform lock")
        .clone()
    {
        let dispatch_platform = Arc::clone(&platform);
        platform
            .background_executor()
            .dispatch_on_main_thread(move || {
                if let Some(window) = dispatch_platform.primary_window() {
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
#[cfg(not(feature = "core-native"))]
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
    agent_activation: Option<AgentActivationEvidence>,
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

#[cfg(feature = "aosp-system")]
#[derive(Clone, Debug)]
struct AndroidGraphRevision {
    package: PackageMetadata,
    allowed_capabilities: BTreeSet<String>,
    state_revision: u64,
    schema_version: u64,
    sidecars: Vec<runtime_luau::RevisionAssetInput>,
}

#[cfg(feature = "aosp-system")]
struct ActiveAndroidGraph {
    graph_id: String,
    worker: GraphRuntimeWorker,
    snapshot: GraphRuntimeSnapshot,
    revisions: BTreeMap<RevisionId, AndroidGraphRevision>,
    appearance_generation: u64,
}

#[cfg(feature = "aosp-system")]
#[derive(Clone, Debug, PartialEq, Eq)]
enum AndroidExperienceLifecycle {
    Present(ExperienceId),
    Dismiss(ExperienceId),
}

#[cfg(feature = "aosp-system")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AndroidSystemControl {
    Theme,
    Rollback,
    Home,
}

#[cfg(feature = "aosp-system")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct AndroidReferenceGraphEvent {
    experience_id: ExperienceId,
    action: String,
}

#[derive(Clone, Debug)]
struct GraphOwner {
    node_id: GraphNodeId,
    instance_id: InstanceId,
}

#[cfg(not(feature = "core-native"))]
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
        } else if url.starts_with("sos://rollback") {
            ROLLBACK_REQUESTED.store(true, Ordering::Release);
            log::info!("graph_rollback_requested");
        } else if url.starts_with("sos://worker-restart") {
            WORKER_RESTART_REQUESTED.store(true, Ordering::Release);
            log::info!("runtime_worker_restart_requested");
        } else if url.starts_with("sos://appearance/toggle") {
            queue_android_appearance_toggle();
        } else if let Some(experience_id) = url.strip_prefix("sos://experience/present/") {
            queue_android_experience_lifecycle(experience_id, false);
        } else if let Some(experience_id) = url.strip_prefix("sos://experience/dismiss/") {
            queue_android_experience_lifecycle(experience_id, true);
        } else if let Some(path) = url.strip_prefix("sos://experience/event/") {
            queue_android_reference_graph_event(path);
        } else if let Some(screen) = url.strip_prefix("sos://mobile/navigate/") {
            if matches!(screen, "home" | "apps" | "agent" | "controls") {
                *MOBILE_NAVIGATION_REQUEST
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .expect("mobile navigation request lock") = Some(screen.into());
                log::info!("stock_mobile_navigation_requested screen={screen}");
                request_host_frame();
            } else {
                log::warn!("stock_mobile_navigation_rejected screen={screen}");
            }
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

    run_experience(shared);

    // `android-activity` may create a fresh native thread when HOME is
    // relaunched while keeping the Linux process alive. GPUI's Android
    // dispatcher and its process-global platform are bound to the original
    // native thread, so re-entering `android_main` in that process would reuse
    // a stale dispatcher and fail GPUI's main-thread invariant. NativeActivity
    // owns no durable state in-process; the SOS model and journal live on disk.
    // End the spent process after the Activity lifecycle exits so Android
    // starts the next HOME instance with a clean platform and dispatcher.
    log::info!("native_activity_lifecycle_ended; recycling process");
    unsafe { libc::_exit(0) }
}

// `android-activity`'s NativeActivity glue has a load-time reference to this
// symbol even in the standalone Core process, where no Activity can invoke it.
// Keep the symbol inert so `dlopen` can resolve the shared object without
// reintroducing an Activity lifecycle into Core.
#[cfg(feature = "core-native")]
#[no_mangle]
fn android_main(_app: android_activity::AndroidApp) {
    log::error!("Core runtime received an impossible NativeActivity entry");
}

fn run_experience(shared: SharedPlatform) {
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
                Ok(_) => log::info!("SOS experience window is live frame_padding=none"),
                Err(error) => log::error!("failed to open experience window: {error}"),
            }
        });
}

#[cfg(feature = "core-native")]
fn core_platform_slot() -> &'static Mutex<Option<Arc<AndroidPlatform>>> {
    CORE_PLATFORM.get_or_init(|| Mutex::new(None))
}

#[cfg(feature = "core-native")]
extern "C" fn core_signal_handler(signal: i32) {
    CORE_EXIT_REASON.store(signal, Ordering::Release);
    CORE_STOP_REQUESTED.store(true, Ordering::Release);
}

#[cfg(feature = "core-native")]
fn install_core_signal_handlers() {
    unsafe {
        let handler = core_signal_handler as *const () as libc::sighandler_t;
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGHUP, handler);
    }
}

#[cfg(feature = "core-native")]
fn request_core_recovery() {
    CORE_EXIT_REASON.store(100, Ordering::Release);
    CORE_STOP_REQUESTED.store(true, Ordering::Release);
}

/// Run the permanent GPUI experience on a SurfaceComposer-owned native
/// window. This entry point has no Activity, JNI lifecycle, or APK data path.
#[cfg(feature = "core-native")]
#[no_mangle]
pub unsafe extern "C" fn sos_core_main(
    native_window: *mut c_void,
    density_dpi: i32,
    data_dir: *const c_char,
) -> i32 {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("sos-core-experience"),
    );
    jni::install_panic_hook();
    CORE_STOP_REQUESTED.store(false, Ordering::Release);
    CORE_EXIT_REASON.store(0, Ordering::Release);
    install_core_signal_handlers();

    let Some(native_window) = NonNull::new(native_window.cast()) else {
        log::error!("Core host received a null ANativeWindow");
        return 64;
    };
    if data_dir.is_null() {
        log::error!("Core host received a null data directory");
        return 64;
    }
    let data_dir = match CStr::from_ptr(data_dir).to_str() {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => {
            log::error!("Core host received an invalid data directory");
            return 64;
        }
    };
    if FILES_DIR.get().is_none() {
        let _ = FILES_DIR.set(data_dir);
    }

    let native_window = ndk::native_window::NativeWindow::clone_from_ptr(native_window);
    let platform =
        match AndroidPlatform::new_standalone(native_window, density_dpi, &CORE_STOP_REQUESTED) {
            Ok(platform) => platform,
            Err(error) => {
                log::error!("Core GPUI platform failed: {error:#}");
                return 70;
            }
        };
    *core_platform_slot().lock().expect("Core platform lock") = Some(Arc::clone(&platform));
    pointer_input::install();
    if let Err(error) = core_input::start(Arc::clone(&platform)) {
        log::error!("Core input ownership failed: {error}");
        *core_platform_slot().lock().expect("Core platform lock") = None;
        return 70;
    }
    log::info!("sos_experience_host role=core-native density_dpi={density_dpi}");

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_experience(SharedPlatform::new(platform));
    }));
    agent::zeroize_credential_on_exit();
    *core_platform_slot().lock().expect("Core platform lock") = None;
    if outcome.is_err() {
        log::error!("Core GPUI host unwound into its fixed native boundary");
        return 70;
    }
    let reason = CORE_EXIT_REASON.load(Ordering::Acquire);
    log::info!("sos_experience_host_stopped role=core-native reason={reason}");
    if reason == 100 {
        100
    } else {
        0
    }
}

/// Run the non-shipping Core 1 provider acceptance probe from the existing
/// init-owned Core host domain. The export exists only in the explicitly
/// selected `core-provider-acceptance` test build.
#[cfg(feature = "core-provider-acceptance")]
#[no_mangle]
pub unsafe extern "C" fn sos_core_provider_acceptance_probe(mode: *const c_char) -> i32 {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("sos-core-provider-probe"),
    );
    if mode.is_null() {
        log::error!("core_provider_probe case=mode status=FAIL reason=null_mode");
        return 1;
    }
    let mode = match CStr::from_ptr(mode).to_str() {
        Ok(mode) => mode,
        Err(_) => {
            log::error!("core_provider_probe case=mode status=FAIL reason=invalid_mode");
            return 1;
        }
    };
    log::info!(
        "core_provider_probe contract=authority-json-line timeout_ms={}",
        provider_client::PROVIDER_PROBE_TIMEOUT.as_millis()
    );
    let report = android_provider_acceptance::run_probe(mode, provider_client::request_probe);
    for line in &report.lines {
        match report.status {
            android_provider_acceptance::ProbeStatus::Pass => log::info!("{line}"),
            android_provider_acceptance::ProbeStatus::Skip => log::warn!("{line}"),
            android_provider_acceptance::ProbeStatus::Fail => log::error!("{line}"),
        }
    }
    log::info!(
        "core_provider_probe result={} exit_code={}",
        report.status.label(),
        report.status.exit_code()
    );
    report.status.exit_code()
}

struct ExperienceHost {
    model: ExperienceModel,
    worker: Option<RuntimeWorker>,
    #[cfg(feature = "aosp-system")]
    active_graph: Option<ActiveAndroidGraph>,
    #[cfg(feature = "aosp-system")]
    pending_graph_previous: Option<(u64, GraphRuntimeSnapshot)>,
    #[cfg(feature = "aosp-system")]
    pending_graph_viewport: Option<(u64, ExperienceViewport)>,
    #[cfg(feature = "aosp-system")]
    pending_graph_confirmation: Option<String>,
    #[cfg(feature = "aosp-system")]
    pending_graph_agent_activation: Option<AgentActivationEvidence>,
    #[cfg(feature = "aosp-system")]
    pending_graph_rollback_presentation: Option<String>,
    scene: Scene,
    state: JsonValue,
    remote_state_revision: Option<u64>,
    state_schema_version: u64,
    source: String,
    #[cfg(feature = "aosp-system")]
    system_revision_id: String,
    status: Option<(String, bool)>,
    next_request_id: u64,
    candidates: HashMap<u64, CandidatePurpose>,
    pending_authority_activations: HashMap<u64, PendingAuthorityActivation>,
    pending_agent_activations: HashMap<u64, AgentActivationEvidence>,
    action_in_flight: bool,
    pending_input_events: VecDeque<SceneEvent>,
    input_state_shadow: HashMap<String, String>,
    inputs: HashMap<String, Entity<NativeTextInput>>,
    scroll_handles: HashMap<String, ScrollHandle>,
    surface_gestures: HashMap<String, GestureSession>,
    surface_taps: HashMap<String, (String, Instant)>,
    pending_focus_restore: Option<String>,
    pending_frame: Option<PendingFrame>,
    revision_activation_pending: bool,
    revision_recovery_status: Option<String>,
    stress: Option<StressRun>,
    pending_reconciled_state: Option<StateEnvelope>,
    agent_updates: async_channel::Sender<agent::AgentUpdate>,
    #[cfg(feature = "aosp-system")]
    revision_assets: Vec<runtime_luau::RevisionAssetInput>,
    accessibility_dirty: bool,
    #[cfg(feature = "aosp-system")]
    node_owners: HashMap<String, GraphOwner>,
}

impl ExperienceHost {
    #[cfg(not(feature = "core-native"))]
    fn blur_compat_input_on_outside_tap(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let active = native_input::active_input_id();
        #[cfg(feature = "aosp-system")]
        let semantic_scene = self
            .active_graph
            .as_ref()
            .map(|graph| composed_graph_scene(&graph.snapshot))
            .unwrap_or_else(|| self.scene.clone());
        #[cfg(not(feature = "aosp-system"))]
        let semantic_scene = self.scene.clone();
        let bounds = self
            .inputs
            .iter()
            .filter(|(id, _)| find_text_session(&semantic_scene.root, id).is_some())
            .filter_map(|(id, input)| input.read(cx).bounds().map(|bounds| (id.as_str(), bounds)));
        if text_tap_outcome(
            active.as_deref(),
            bounds,
            f32::from(event.position.x),
            f32::from(event.position.y),
        ) == TextTapOutcome::OutsideInputs
        {
            log::info!("compat_ime_outside_tap outcome=blur");
            window.blur();
        }
    }

    fn new(cx: &mut Context<Self>) -> Self {
        #[cfg(feature = "aosp-system")]
        let authority_current = revision_client::current_graph_with_retry()
            .unwrap_or_else(|error| panic!("system revision authority is required: {error}"));
        #[cfg(feature = "aosp-system")]
        let authority_graph = authority_current
            .graph
            .clone()
            .unwrap_or_else(|| panic!("system revision authority omitted its v4 graph"));
        #[cfg(feature = "aosp-system")]
        let system_revision_id = authority_graph.graph.nodes[&authority_graph.graph.root]
            .revision_id
            .to_string();
        let mut model = match provider_client::snapshot() {
            Ok(model) => {
                #[cfg(feature = "core-native")]
                log::info!("provider_snapshot_remote transport=unix");
                #[cfg(not(feature = "core-native"))]
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
        #[cfg(not(feature = "aosp-system"))]
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
        #[cfg(feature = "aosp-system")]
        {
            model.appearance = authority_graph.appearance.profile.clone();
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
            let root = &authority_graph.graph.nodes[&authority_graph.graph.root];
            let revision = authority_graph
                .revisions
                .iter()
                .find(|revision| revision.revision_id == root.revision_id.as_str())
                .unwrap_or_else(|| panic!("system graph omitted its root revision"));
            (
                revision.state.resource.state.clone(),
                Some(revision.state.resource.revision),
                revision.state.resource.schema_version,
            )
        };
        #[cfg(not(feature = "aosp-system"))]
        let source = recover_committed_source(remote_source_sha256.as_deref());
        #[cfg(feature = "aosp-system")]
        let source = {
            let root = &authority_graph.graph.nodes[&authority_graph.graph.root];
            authority_graph
                .revisions
                .iter()
                .find(|revision| revision.revision_id == root.revision_id.as_str())
                .map(|revision| revision.source.clone())
                .unwrap_or_else(|| panic!("system graph omitted root source"))
        };
        #[cfg(feature = "aosp-system")]
        let revision_assets = {
            let root = &authority_graph.graph.nodes[&authority_graph.graph.root];
            authority_graph
                .revisions
                .iter()
                .find(|revision| revision.revision_id == root.revision_id.as_str())
                .map(|revision| revision_client::inputs(revision.assets.clone()))
                .unwrap_or_default()
        };
        #[cfg(not(feature = "aosp-system"))]
        let (worker, ready) = RuntimeWorker::spawn(
            source.clone(),
            model.clone(),
            state.clone(),
            state_schema_version,
        )
        .expect("runtime worker thread must start");
        #[cfg(not(feature = "aosp-system"))]
        let results = worker.results();
        #[cfg(feature = "aosp-system")]
        let (active_graph, pending_graph_confirmation) = {
            let migration_pending = authority_graph.migration_pending;
            let graph_id = authority_graph.graph_id.clone();
            let graph = match start_android_graph_runtime(authority_graph, &model) {
                Ok(graph) => graph,
                Err(error) => {
                    let rollback = revision_client::rollback_graph(graph_id.clone());
                    log::error!(
                            "system_graph_runtime_rejected graph_id={graph_id} error={error} rollback_ok={}",
                            rollback.is_ok()
                        );
                    std::process::abort();
                }
            };
            let results = graph.worker.results();
            Self::attach_graph_channels(results, cx);
            (Some(graph), migration_pending.then_some(graph_id))
        };
        let (agent_updates, agent_results) = async_channel::unbounded();
        #[cfg(not(feature = "aosp-system"))]
        Self::attach_worker_channels(ready, results, cx);
        #[cfg(feature = "aosp-system")]
        Self::attach_provider_poll(cx);
        #[cfg(not(feature = "aosp-system"))]
        Self::attach_network_poll(cx);
        Self::attach_agent_updates(agent_results, cx);
        Self::attach_agent_poll(cx);
        log::info!(
            "runtime_worker_spawned ui_thread={:?}",
            thread::current().id()
        );
        #[cfg(feature = "aosp-system")]
        let initial_scene = active_graph
            .as_ref()
            .and_then(|graph| {
                graph
                    .snapshot
                    .instances
                    .get(&graph.snapshot.root)
                    .and_then(|instance| instance.scene.clone())
            })
            .unwrap_or_else(loading_scene);

        Self {
            model,
            worker: {
                #[cfg(feature = "aosp-system")]
                {
                    None
                }
                #[cfg(not(feature = "aosp-system"))]
                {
                    Some(worker)
                }
            },
            #[cfg(feature = "aosp-system")]
            active_graph,
            #[cfg(feature = "aosp-system")]
            pending_graph_previous: None,
            #[cfg(feature = "aosp-system")]
            pending_graph_viewport: None,
            #[cfg(feature = "aosp-system")]
            pending_graph_confirmation,
            #[cfg(feature = "aosp-system")]
            pending_graph_agent_activation: None,
            #[cfg(feature = "aosp-system")]
            pending_graph_rollback_presentation: None,
            scene: {
                #[cfg(feature = "aosp-system")]
                {
                    initial_scene
                }
                #[cfg(not(feature = "aosp-system"))]
                {
                    loading_scene()
                }
            },
            state,
            remote_state_revision,
            state_schema_version,
            source,
            #[cfg(feature = "aosp-system")]
            system_revision_id,
            status: Some(("Starting Luau worker…".into(), true)),
            next_request_id: 1,
            candidates: HashMap::new(),
            pending_authority_activations: HashMap::new(),
            pending_agent_activations: HashMap::new(),
            action_in_flight: false,
            pending_input_events: VecDeque::new(),
            input_state_shadow: HashMap::new(),
            inputs: HashMap::new(),
            scroll_handles: HashMap::new(),
            surface_gestures: HashMap::new(),
            surface_taps: HashMap::new(),
            pending_focus_restore: None,
            pending_frame: None,
            revision_activation_pending: false,
            revision_recovery_status: None,
            stress: None,
            pending_reconciled_state: None,
            agent_updates,
            #[cfg(feature = "aosp-system")]
            revision_assets,
            accessibility_dirty: true,
            #[cfg(feature = "aosp-system")]
            node_owners: HashMap::new(),
        }
    }

    #[cfg(not(feature = "aosp-system"))]
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

    #[cfg(feature = "aosp-system")]
    fn attach_graph_channels(
        results: async_channel::Receiver<GraphWorkerResult>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Ok(result) = results.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.handle_graph_result(result, cx);
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

    #[cfg(feature = "aosp-system")]
    fn handle_graph_result(&mut self, result: GraphWorkerResult, cx: &mut Context<Self>) {
        match result {
            GraphWorkerResult::ActionCompleted {
                request_id,
                outcome,
            } => {
                let Some((pending_id, previous)) = self.pending_graph_previous.take() else {
                    self.action_in_flight = false;
                    self.status = Some(("Graph action lost its rollback snapshot".into(), false));
                    return;
                };
                if pending_id != request_id {
                    self.action_in_flight = false;
                    self.status = Some(("Graph action response was out of order".into(), false));
                    return;
                }
                let Some(graph) = self.active_graph.as_ref() else {
                    self.action_in_flight = false;
                    self.status =
                        Some(("Graph action completed after graph removal".into(), false));
                    return;
                };
                let (updates, effects, agent_effects, lifecycle) =
                    match android_graph_action_wire(graph, &previous, &outcome) {
                        Ok(wire) => wire,
                        Err(error) => {
                            graph.worker.restore(request_id, previous.clone()).ok();
                            self.install_graph_snapshot(previous);
                            self.action_in_flight = false;
                            self.status = Some((format!("Graph result rejected: {error}"), false));
                            self.dispatch_pending_input_event(cx);
                            return;
                        }
                    };
                let expected_graph_id = graph.graph_id.clone();
                match revision_client::commit_graph_action(
                    expected_graph_id.clone(),
                    updates,
                    effects,
                ) {
                    Ok(states) => {
                        if let Some(active) = self.active_graph.as_mut() {
                            for state in states {
                                for (revision_id, revision) in &mut active.revisions {
                                    if revision.package.experience_id == state.experience_id
                                        && revision_id.as_str() == state.resource.revision_id
                                    {
                                        revision.state_revision = state.resource.revision;
                                    }
                                }
                            }
                        }
                        log_android_graph_status_transitions(&previous, &outcome.snapshot);
                        self.install_graph_snapshot(outcome.snapshot);
                        self.execute_agent_effects(agent_effects);
                        log::info!("android_graph_action_committed request_id={request_id}");
                        self.status = lifecycle
                            .as_ref()
                            .map(|_| ("Opening Experience…".into(), true));
                        if let Some(lifecycle) = lifecycle {
                            if let Err(error) =
                                self.stage_lifecycle_graph(expected_graph_id, lifecycle, cx)
                            {
                                self.status =
                                    Some((format!("Experience lifecycle failed: {error}"), false));
                                log::warn!(
                                    "android_experience_lifecycle_rejected request_id={request_id} error={error}"
                                );
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(active) = self.active_graph.as_ref() {
                            active.worker.restore(request_id, previous.clone()).ok();
                        }
                        self.install_graph_snapshot(previous);
                        self.status = Some((format!("Graph state commit failed: {error}"), false));
                        log::warn!(
                            "android_graph_action_commit_rejected request_id={request_id} error={error}"
                        );
                    }
                }
                self.action_in_flight = false;
                if self.pending_graph_confirmation.is_none() {
                    self.dispatch_pending_input_event(cx);
                }
            }
            GraphWorkerResult::Refreshed {
                request_id,
                snapshot,
            } => {
                let failed_instances = snapshot
                    .instances
                    .values()
                    .filter(|instance| matches!(instance.status, RuntimeInstanceStatus::Failed(_)))
                    .count();
                if let Some(graph) = self.active_graph.as_ref() {
                    log_android_graph_status_transitions(&graph.snapshot, &snapshot);
                }
                self.install_graph_snapshot(snapshot);
                if self
                    .pending_graph_viewport
                    .as_ref()
                    .is_some_and(|(pending_id, _)| *pending_id == request_id)
                {
                    let (_, viewport) = self
                        .pending_graph_viewport
                        .take()
                        .expect("viewport request was checked");
                    log::info!(
                        "android_graph_viewport_applied request_id={} width={} height={} scale_milli={} safe_left={} safe_top={} safe_right={} safe_bottom={}",
                        request_id,
                        viewport.width,
                        viewport.height,
                        viewport.scale_milli,
                        viewport.safe_insets.left,
                        viewport.safe_insets.top,
                        viewport.safe_insets.right,
                        viewport.safe_insets.bottom
                    );
                } else {
                    self.status = None;
                    log::info!(
                        "android_graph_refreshed request_id={} appearance_generation={} failed_instances={}",
                        request_id,
                        self.model.appearance.generation,
                        failed_instances
                    );
                }
            }
            GraphWorkerResult::Rejected { request_id, error } => {
                if self
                    .pending_graph_viewport
                    .as_ref()
                    .is_some_and(|(pending_id, _)| *pending_id == request_id)
                {
                    self.pending_graph_viewport = None;
                    self.status = Some((format!("Viewport update rejected: {error}"), false));
                    log::warn!(
                        "android_graph_viewport_rejected request_id={request_id} error={error}"
                    );
                    return;
                }
                self.pending_graph_previous = None;
                self.action_in_flight = false;
                self.status = Some((format!("Graph operation rejected: {error}"), false));
                log::warn!(
                    "android_graph_operation_rejected request_id={request_id} error={error}"
                );
                self.dispatch_pending_input_event(cx);
            }
        }
    }

    #[cfg(feature = "aosp-system")]
    fn install_graph_snapshot(&mut self, snapshot: GraphRuntimeSnapshot) {
        assets::install_graph(
            snapshot
                .instances
                .values()
                .map(|instance| instance.assets.as_slice()),
        );
        let root = snapshot.instances.get(&snapshot.root);
        if let Some(scene) = root.and_then(|instance| instance.scene.clone()) {
            self.scene = scene;
        }
        if let Some(state) = root.map(|instance| instance.state.clone()) {
            self.state = state;
        }
        self.input_state_shadow.retain(|key, value| {
            let Some((instance_id, state_key)) = key.split_once("::") else {
                return true;
            };
            snapshot
                .instances
                .values()
                .find(|instance| instance.instance_id.as_str() == instance_id)
                .and_then(|instance| instance.state.get(state_key))
                .and_then(JsonValue::as_str)
                != Some(value.as_str())
        });
        self.node_owners.clear();
        self.accessibility_dirty = true;
        if let Some(graph) = self.active_graph.as_mut() {
            graph.snapshot = snapshot;
        }
    }

    #[cfg(feature = "aosp-system")]
    fn sync_root_viewport(&mut self) {
        if self.pending_graph_viewport.is_some()
            || self.action_in_flight
            || self.pending_graph_confirmation.is_some()
        {
            return;
        }
        let Some(viewport) = native_input::viewport_context() else {
            return;
        };
        let Some(active) = self.active_graph.as_ref() else {
            return;
        };
        if active.snapshot.instances[&active.snapshot.root].viewport == viewport {
            return;
        }
        let request_id = self.allocate_request_id();
        let Some(active) = self.active_graph.as_ref() else {
            return;
        };
        if let Err(error) = active
            .worker
            .set_root_viewport(request_id, viewport.clone())
        {
            self.status = Some((format!("Viewport update could not start: {error}"), false));
            return;
        }
        self.pending_graph_viewport = Some((request_id, viewport));
    }

    #[cfg(feature = "aosp-system")]
    fn attach_provider_poll(cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| loop {
            executor.timer(Duration::from_secs(2)).await;
            let snapshot = provider_client::snapshot();
            let appearance = revision_client::current_appearance();
            if this
                .update(cx, |this, cx| {
                    if let Ok(appearance) = appearance {
                        if appearance.profile.generation != this.model.appearance.generation {
                            this.model.appearance = appearance.profile.clone();
                            let request_id = this.allocate_request_id();
                            if let Some(graph) = this.active_graph.as_mut() {
                                graph.appearance_generation = appearance.profile.generation;
                                if let Err(error) = graph
                                    .worker
                                    .apply_appearance(request_id, appearance.profile)
                                {
                                    log::warn!("android_appearance_apply_failed error={error}");
                                }
                            }
                        }
                    }
                    match snapshot {
                        Ok(mut snapshot) => {
                            // Agent credentials and conversation remain in the
                            // resident app adapter; the system provider authority
                            // remains canonical for every system fact.
                            snapshot.agent = this.model.agent.clone();
                            snapshot.appearance = this.model.appearance.clone();
                            if snapshot == this.model {
                                return;
                            }
                            this.model = snapshot;
                            this.refresh_model_from_authority();
                            cx.notify();
                        }
                        Err(error) => log::warn!("android_provider_poll_failed error={error}"),
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
    }

    #[cfg(not(feature = "aosp-system"))]
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
                        this.handle_agent_update(update, cx);
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
        #[cfg(feature = "aosp-system")]
        if let Some(graph) = self.active_graph.as_ref() {
            for (offset, (node_id, instance)) in graph.snapshot.instances.iter().enumerate() {
                let revision = &graph.revisions[&instance.revision_id];
                let model = filter_android_graph_model(
                    &self.model,
                    revision.package.role,
                    &revision.allowed_capabilities,
                );
                let refresh_id = request_id.wrapping_add(offset as u64);
                if let Err(error) = graph
                    .worker
                    .refresh_model(refresh_id, node_id.clone(), model)
                {
                    log::warn!(
                        "android_graph_model_refresh_start_failed node_id={} error={error}",
                        node_id
                    );
                }
            }
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            log::warn!("android_model_refresh_start_failed error=runtime_unavailable");
            return;
        };
        if let Err(error) = worker.refresh_model(request_id, self.model.clone(), self.state.clone())
        {
            log::warn!("android_model_refresh_start_failed error={error}");
        }
    }

    #[cfg(not(feature = "aosp-system"))]
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
                self.status = self
                    .revision_recovery_status
                    .take()
                    .map(|message| (message, false));
                self.revision_activation_pending = false;
                self.accessibility_dirty = true;
                log::info!(
                    "runtime_worker_ready ui_thread={:?} worker_thread={} initialize_us={}",
                    thread::current().id(),
                    ready.worker_thread,
                    ready.initialize_us
                );
                if file_path(CANDIDATE_FILE).is_file() {
                    self.submit_reload(cx);
                }
            }
            Err(error) if self.source.trim() != MOBILE_EXPERIENCE.trim() => {
                #[cfg(feature = "aosp-system")]
                {
                    log::error!(
                        "system graph runtime rejected at startup: {error}; fixed Recovery is required"
                    );
                    std::process::abort();
                }
                #[cfg(not(feature = "aosp-system"))]
                {
                    log::error!(
                        "active source rejected at startup: {error}; using embedded source"
                    );
                    self.source = MOBILE_EXPERIENCE.to_owned();
                    match RuntimeWorker::spawn(
                        self.source.clone(),
                        self.model.clone(),
                        self.state.clone(),
                        self.state_schema_version,
                    ) {
                        Ok((worker, ready)) => {
                            let results = worker.results();
                            self.worker = Some(worker);
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
                #[cfg(feature = "aosp-system")]
                {
                    log::error!(
                        "trusted stock runtime rejected at startup: {error}; fixed Recovery is required"
                    );
                    std::process::abort();
                }
                #[cfg(not(feature = "aosp-system"))]
                {
                    log::error!("embedded runtime rejected at startup: {error}");
                    self.status = Some((format!("Runtime could not start: {error}"), false));
                }
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
        #[cfg(feature = "aosp-system")]
        {
            let response = match revision_client::current_graph_with_retry() {
                Ok(response) => response,
                Err(error) => {
                    self.status = Some((format!("Graph restart failed: {error}"), false));
                    return;
                }
            };
            let Some(bundle) = response.graph else {
                self.status = Some(("Graph restart response omitted its v4 graph".into(), false));
                return;
            };
            match start_android_graph_runtime(bundle, &self.model) {
                Ok(graph) => {
                    let results = graph.worker.results();
                    let snapshot = graph.snapshot.clone();
                    self.pending_graph_viewport = None;
                    self.active_graph = Some(graph);
                    self.install_graph_snapshot(snapshot);
                    self.status = Some(("Restarted v4 experience graph".into(), true));
                    Self::attach_graph_channels(results, cx);
                }
                Err(error) => {
                    self.status = Some((format!("Graph restart failed: {error}"), false));
                }
            }
            return;
        }
        #[cfg(not(feature = "aosp-system"))]
        let spawned = RuntimeWorker::spawn(
            self.source.clone(),
            self.model.clone(),
            self.state.clone(),
            self.state_schema_version,
        );
        #[cfg(not(feature = "aosp-system"))]
        match spawned {
            Ok((worker, ready)) => {
                let results = worker.results();
                self.worker = Some(worker);
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

    fn dispatch_event(&mut self, mut event: SceneEvent, cx: &mut Context<Self>) {
        let revision_activation_pending = self.revision_activation_pending
            || self
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
        #[cfg(feature = "aosp-system")]
        if self.presented_stock_mobile() {
            let control = match event.action.as_str() {
                STOCK_MOBILE_THEME_ACTION => Some(AndroidSystemControl::Theme),
                STOCK_MOBILE_ROLLBACK_ACTION => Some(AndroidSystemControl::Rollback),
                _ => None,
            };
            if let Some(control) = control {
                let root_instance = self
                    .active_graph
                    .as_ref()
                    .map(|graph| {
                        graph.snapshot.instances[&graph.snapshot.root]
                            .instance_id
                            .clone()
                    })
                    .expect("Stock Mobile requires an active graph");
                let root_target = event
                    .target
                    .as_deref()
                    .is_some_and(|target| target.starts_with(&format!("{root_instance}::")));
                if root_target {
                    self.activate_android_system_control(control, cx);
                    return;
                }
                log::warn!(
                    "android_stock_control_rejected action={} reason=non_root_target",
                    event.action
                );
            }
        }
        let request_id = self.allocate_request_id();
        log::info!(
            "experience_action request_id={request_id} action={} target={}",
            event.action,
            event.target.as_deref().unwrap_or("none")
        );
        self.action_in_flight = true;
        #[cfg(feature = "aosp-system")]
        if let Some(graph) = self.active_graph.as_ref() {
            let owner = event
                .target
                .as_ref()
                .and_then(|target| {
                    self.node_owners
                        .get(target)
                        .cloned()
                        .or_else(|| graph_owner_for_target(&graph.snapshot, target))
                })
                .unwrap_or_else(|| graph_owner(&graph.snapshot, &graph.snapshot.root));
            if let Some(target) = &mut event.target {
                let prefix = format!("{}::", owner.instance_id);
                if let Some(local) = target.strip_prefix(&prefix) {
                    *target = local.to_owned();
                }
            }
            self.pending_graph_previous = Some((request_id, graph.snapshot.clone()));
            let event = match serde_json::to_value(event) {
                Ok(event) => event,
                Err(error) => {
                    self.pending_graph_previous = None;
                    self.action_in_flight = false;
                    self.status = Some((format!("Action could not encode: {error}"), false));
                    return;
                }
            };
            if let Err(error) = graph.worker.action(request_id, owner.node_id, event) {
                self.pending_graph_previous = None;
                self.action_in_flight = false;
                self.status = Some((format!("Graph action could not start: {error}"), false));
                cx.notify();
            }
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            self.action_in_flight = false;
            self.status = Some(("Action could not start: runtime unavailable".into(), false));
            cx.notify();
            return;
        };
        if let Err(error) = worker.action(request_id, self.model.clone(), self.state.clone(), event)
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
        #[cfg(feature = "aosp-system")]
        let graph_active = self.active_graph.is_some();
        #[cfg(not(feature = "aosp-system"))]
        let graph_active = false;
        if !graph_active {
            if !self.state.is_object() {
                self.state = json!({});
            }
            if let Some(object) = self.state.as_object_mut() {
                object.insert(state_key.clone(), JsonValue::String(value.clone()));
            }
            persist_state(&self.state);
        }
        self.input_state_shadow.insert(state_key, value.clone());
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

        #[cfg(feature = "aosp-system")]
        let scene = self
            .active_graph
            .as_ref()
            .map(|graph| composed_graph_scene(&graph.snapshot))
            .unwrap_or_else(|| self.scene.clone());
        #[cfg(not(feature = "aosp-system"))]
        let scene = self.scene.clone();
        let Some(scroll_id) = scroll_parent(&scene.root, node_id, None) else {
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
        mut region: HitRegion,
        x: f32,
        y: f32,
        platform_click_count: usize,
        cx: &mut Context<Self>,
    ) {
        region.id = self.scoped_graph_target(&surface_id, &region.id);
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
            let mut region = region.clone();
            region.id = self.scoped_graph_target(&surface_id, &region.id);
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

    fn scoped_graph_target(&self, surface_id: &str, local_target: &str) -> String {
        #[cfg(feature = "aosp-system")]
        if let Some(owner) = self.node_owners.get(surface_id) {
            return format!("{}::{local_target}", owner.instance_id);
        }
        local_target.to_owned()
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

    fn submit_reload(&mut self, cx: &mut Context<Self>) {
        let Some(candidate_source) = read_file(CANDIDATE_FILE) else {
            self.status = Some(("No candidate script found".into(), false));
            return;
        };
        self.submit_candidate_source(candidate_source, cx);
    }

    fn submit_candidate_source(&mut self, candidate_source: String, cx: &mut Context<Self>) {
        self.submit_candidate_source_with_origin(candidate_source, false, cx);
    }

    fn submit_agent_candidate_source(&mut self, candidate_source: String, cx: &mut Context<Self>) {
        self.submit_candidate_source_with_origin(candidate_source, true, cx);
    }

    fn submit_candidate_source_with_origin(
        &mut self,
        candidate_source: String,
        from_verified_agent: bool,
        cx: &mut Context<Self>,
    ) {
        if self.worker.is_none() {
            #[cfg(feature = "aosp-system")]
            self.submit_v4_candidate(candidate_source, from_verified_agent, cx);
            #[cfg(not(feature = "aosp-system"))]
            {
                let _ = (candidate_source, from_verified_agent);
                self.status = Some(("Runtime worker is unavailable".into(), false));
            }
            return;
        }
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
        if from_verified_agent {
            self.pending_agent_activations
                .insert(request_id, AgentActivationEvidence::submitted(request_id));
        }
        self.status = Some(("Compiling candidate on Luau worker…".into(), true));
        log::info!(
            "script_submitted request_id={request_id} ui_thread={:?} source_bytes={}",
            thread::current().id(),
            candidate_source.len()
        );
        if let Err(error) = self
            .worker
            .as_ref()
            .expect("standalone worker")
            .prepare_candidate(
                request_id,
                candidate_source,
                self.model.clone(),
                self.state.clone(),
                self.state_schema_version,
                submitted_at,
            )
        {
            self.candidates.remove(&request_id);
            self.pending_agent_activations.remove(&request_id);
            self.status = Some((format!("Candidate could not start: {error}"), false));
        }
    }

    #[cfg(feature = "aosp-system")]
    fn reject_v4_candidate(&mut self, message: String) {
        log::warn!("android_v4_candidate_rejected error={message}");
        self.status = Some((message, false));
    }

    #[cfg(feature = "aosp-system")]
    fn submit_v4_candidate(
        &mut self,
        candidate_source: String,
        from_verified_agent: bool,
        cx: &mut Context<Self>,
    ) {
        if self.action_in_flight || self.pending_graph_confirmation.is_some() {
            self.reject_v4_candidate("A graph action or activation is already active".into());
            return;
        }
        let mut agent_activation = from_verified_agent.then(|| {
            let request_id = self.allocate_request_id();
            AgentActivationEvidence::submitted(request_id)
        });
        let Some(active) = self.active_graph.as_ref() else {
            self.reject_v4_candidate("No v4 authoring target is active".into());
            return;
        };
        let root = &active.snapshot.root;
        let root_instance = &active.snapshot.instances[root];
        let current = &active.revisions[&root_instance.revision_id];
        let runtime = match runtime_luau::LuauRuntime::compile_with_assets(
            &candidate_source,
            current.sidecars.clone(),
        ) {
            Ok(runtime) => runtime,
            Err(error) => {
                self.reject_v4_candidate(format!("Candidate compile failed: {error}"));
                return;
            }
        };
        if runtime.api_version() != experience_ir::EXPERIENCE_API_VERSION {
            self.reject_v4_candidate("New authoring submissions must use Experience API v4".into());
            return;
        }
        let implemented = match runtime.export_ids() {
            Ok(exports) => exports.into_iter().collect::<BTreeSet<_>>(),
            Err(error) => {
                self.reject_v4_candidate(format!("Candidate exports failed: {error}"));
                return;
            }
        };
        let declared = current
            .package
            .contract
            .exports
            .keys()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        if implemented != declared {
            self.reject_v4_candidate(
                "Candidate exports do not match the active v4 contract".into(),
            );
            return;
        }
        let state = match runtime.migrate_state(current.schema_version, &root_instance.state) {
            Ok(state) => state,
            Err(error) => {
                self.reject_v4_candidate(format!("Candidate state migration failed: {error}"));
                return;
            }
        };
        let schema_version = match runtime.state_schema_version() {
            Ok(version) => version,
            Err(error) => {
                self.reject_v4_candidate(format!("Candidate state schema failed: {error}"));
                return;
            }
        };
        let mut package = current.package.clone();
        package.state_migration = Some(StateMigrationRecord {
            source: StateMigrationSource::ExperienceRevision {
                experience_id: package.experience_id.clone(),
                revision_id: root_instance.revision_id.clone(),
                schema_version: current.schema_version,
                state_sha256: match canonical_sha256(&root_instance.state) {
                    Ok(digest) => digest,
                    Err(error) => {
                        self.reject_v4_candidate(format!("Candidate state hash failed: {error}"));
                        return;
                    }
                },
            },
            target_schema_version: schema_version,
            result_state_sha256: match canonical_sha256(&state) {
                Ok(digest) => digest,
                Err(error) => {
                    self.reject_v4_candidate(format!("Migrated state hash failed: {error}"));
                    return;
                }
            },
        });
        if let Err(error) = package.validate() {
            self.reject_v4_candidate(format!("Candidate package failed: {error}"));
            return;
        }
        let model =
            filter_android_graph_model(&self.model, package.role, &current.allowed_capabilities);
        if let Err(error) = validate_android_candidate_exports(&runtime, &package, &model, &state) {
            self.reject_v4_candidate(format!("Candidate validation failed: {error}"));
            return;
        }
        if let Some(evidence) = agent_activation.as_mut() {
            evidence
                .advance(AgentActivationPhase::Validated)
                .expect("v4 agent validation must follow submission");
            log::info!(
                "android_agent_candidate_validation_ack request_id={} phase=validated authority_committed=false",
                evidence.request_id()
            );
        }
        let assets = current
            .sidecars
            .iter()
            .map(|asset| android_authority_protocol::RevisionAssetWire {
                id: asset.id.clone(),
                kind: asset.kind.clone(),
                bytes: asset.bytes.clone(),
            })
            .collect();
        self.status = Some(("Candidate validated; staging v4 graph…".into(), true));
        let response = match revision_client::stage_graph_revision(
            active.graph_id.clone(),
            package,
            candidate_source.clone(),
            state,
            schema_version,
            assets,
        ) {
            Ok(response) => response,
            Err(error) => {
                self.reject_v4_candidate(format!("Candidate staging failed: {error}"));
                return;
            }
        };
        let Some(bundle) = response.graph else {
            self.reject_v4_candidate("Candidate staging omitted its resolved graph".into());
            return;
        };
        if let Some(evidence) = agent_activation.as_mut() {
            evidence
                .advance(AgentActivationPhase::Staged)
                .expect("v4 graph staging must follow validation");
            log::info!(
                "android_agent_activation_stage_ack request_id={} graph_id={} phase=staged authority_committed=false",
                evidence.request_id(),
                bundle.graph_id
            );
        }
        let graph_id = bundle.graph_id.clone();
        let graph = match start_android_graph_runtime(bundle, &self.model) {
            Ok(graph) => graph,
            Err(error) => {
                let _ = revision_client::discard_graph(graph_id);
                self.reject_v4_candidate(format!("Candidate graph rejected: {error}"));
                return;
            }
        };
        let results = graph.worker.results();
        let snapshot = graph.snapshot.clone();
        self.pending_graph_viewport = None;
        self.active_graph = Some(graph);
        self.install_graph_snapshot(snapshot);
        self.source = candidate_source;
        self.state_schema_version = schema_version;
        self.system_revision_id = self.active_graph.as_ref().unwrap().snapshot.instances
            [&self.active_graph.as_ref().unwrap().snapshot.root]
            .revision_id
            .to_string();
        self.pending_graph_confirmation = Some(graph_id.clone());
        self.pending_graph_agent_activation = agent_activation;
        self.status = Some(("Candidate rendered; confirming v4 graph…".into(), true));
        log::info!(
            "android_v4_candidate_staged graph_id={} source_sha256={}",
            graph_id,
            source_sha256(&self.source)
        );
        Self::attach_graph_channels(results, cx);
    }

    #[cfg(feature = "aosp-system")]
    fn stage_lifecycle_graph(
        &mut self,
        expected_graph_id: String,
        lifecycle: AndroidExperienceLifecycle,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let response = match lifecycle.clone() {
            AndroidExperienceLifecycle::Present(experience_id) => {
                revision_client::present_experience(expected_graph_id, experience_id)
            }
            AndroidExperienceLifecycle::Dismiss(experience_id) => {
                revision_client::dismiss_experience(expected_graph_id, experience_id)
            }
        }?;
        let bundle = response
            .graph
            .ok_or_else(|| "lifecycle response omitted its resolved graph".to_owned())?;
        if !bundle.migration_pending {
            return Err("lifecycle graph did not require presentation confirmation".into());
        }
        let graph_id = bundle.graph_id.clone();
        let root_revision_id = bundle.graph.nodes[&bundle.graph.root].revision_id.clone();
        let root_revision = bundle
            .revisions
            .iter()
            .find(|revision| revision.revision_id == root_revision_id.as_str())
            .ok_or_else(|| "lifecycle graph omitted its root revision".to_owned())?;
        let source = root_revision.source.clone();
        let schema_version = root_revision.state.resource.schema_version;
        let revision_id = root_revision.revision_id.clone();
        let graph = match start_android_graph_runtime(bundle, &self.model) {
            Ok(graph) => graph,
            Err(error) => {
                let _ = revision_client::discard_graph(graph_id);
                return Err(format!("lifecycle graph runtime rejected: {error}"));
            }
        };
        let results = graph.worker.results();
        let snapshot = graph.snapshot.clone();
        self.pending_graph_viewport = None;
        self.active_graph = Some(graph);
        self.install_graph_snapshot(snapshot);
        self.source = source;
        self.state_schema_version = schema_version;
        self.system_revision_id = revision_id;
        self.pending_graph_confirmation = Some(graph_id.clone());
        self.pending_graph_agent_activation = None;
        self.status = Some(("Experience rendered; confirming graph…".into(), true));
        log::info!("android_experience_lifecycle_staged action={lifecycle:?} graph_id={graph_id}");
        Self::attach_graph_channels(results, cx);
        cx.notify();
        Ok(())
    }

    #[cfg(feature = "aosp-system")]
    fn presented_ordinary_experience(&self) -> Option<ExperienceId> {
        let graph = self.active_graph.as_ref()?;
        let root = graph.snapshot.instances.get(&graph.snapshot.root)?;
        let revision = graph.revisions.get(&root.revision_id)?;
        (revision.package.role != ExperienceRole::Shell).then(|| root.experience_id.clone())
    }

    #[cfg(feature = "aosp-system")]
    fn presented_stock_mobile(&self) -> bool {
        let Some(graph) = self.active_graph.as_ref() else {
            return false;
        };
        let root = &graph.snapshot.instances[&graph.snapshot.root];
        let revision = &graph.revisions[&root.revision_id];
        root.experience_id.as_str() == STOCK_MOBILE_EXPERIENCE_ID
            && revision.package.role == ExperienceRole::Shell
    }

    #[cfg(feature = "aosp-system")]
    fn activate_android_system_control(
        &mut self,
        control: AndroidSystemControl,
        cx: &mut Context<Self>,
    ) {
        match control {
            AndroidSystemControl::Theme => {
                if self.action_in_flight || self.pending_graph_confirmation.is_some() {
                    self.status = Some(("A graph action is already active".into(), false));
                } else if let Err(error) = self.toggle_authority_appearance() {
                    self.status = Some((format!("Appearance write failed: {error}"), false));
                } else {
                    log::info!("android_system_control action=appearance_toggle");
                }
            }
            AndroidSystemControl::Rollback => {
                log::info!("android_system_control action=rollback_graph");
                self.rollback_active_graph(cx);
            }
            AndroidSystemControl::Home => {
                let Some(experience_id) = self.presented_ordinary_experience() else {
                    self.status = Some(("No ordinary v4 Experience is presented".into(), false));
                    cx.notify();
                    return;
                };
                let Some(graph_id) = self
                    .active_graph
                    .as_ref()
                    .map(|graph| graph.graph_id.clone())
                else {
                    self.status = Some(("No v4 graph is active".into(), false));
                    cx.notify();
                    return;
                };
                log::info!(
                    "android_system_control action=dismiss_experience experience_id={experience_id}"
                );
                if let Err(error) = self.stage_lifecycle_graph(
                    graph_id,
                    AndroidExperienceLifecycle::Dismiss(experience_id),
                    cx,
                ) {
                    self.status = Some((format!("Experience dismissal failed: {error}"), false));
                }
            }
        }
        cx.notify();
    }

    #[cfg(feature = "aosp-system")]
    fn toggle_authority_appearance(&mut self) -> Result<(), String> {
        let graph_id = self
            .active_graph
            .as_ref()
            .map(|graph| graph.graph_id.clone())
            .ok_or_else(|| "No v4 graph is active".to_owned())?;
        let expected_generation = self.model.appearance.generation;
        let mut profile = self.model.appearance.clone();
        profile.generation = expected_generation.saturating_add(1);
        profile.scheme = match profile.scheme {
            experience_package::ColorScheme::Light => experience_package::ColorScheme::Dark,
            experience_package::ColorScheme::Dark => experience_package::ColorScheme::Light,
        };
        let appearance = revision_client::set_experience_appearance(
            graph_id.clone(),
            expected_generation,
            profile,
        )?;
        self.model.appearance = appearance.profile.clone();
        let request_id = self.allocate_request_id();
        let graph = self
            .active_graph
            .as_mut()
            .ok_or_else(|| "v4 graph disappeared during appearance write".to_owned())?;
        graph.appearance_generation = appearance.profile.generation;
        graph
            .worker
            .apply_appearance(request_id, appearance.profile.clone())
            .map_err(|error| error.to_string())?;
        self.status = Some((
            format!(
                "Appearance generation {} is active",
                appearance.profile.generation
            ),
            true,
        ));
        log::info!(
            "android_appearance_write_committed graph_id={} generation={} scheme={:?}",
            graph_id,
            appearance.profile.generation,
            appearance.profile.scheme
        );
        Ok(())
    }

    #[cfg(feature = "aosp-system")]
    fn rollback_active_graph(&mut self, cx: &mut Context<Self>) {
        if self.action_in_flight || self.pending_graph_confirmation.is_some() {
            self.status = Some((
                "A graph action or activation is already active".into(),
                false,
            ));
            return;
        }
        let Some(active) = self.active_graph.as_ref() else {
            self.status = Some(("No active v4 graph can be rolled back".into(), false));
            return;
        };
        let failed_graph_id = active.graph_id.clone();
        let response = match revision_client::rollback_graph(failed_graph_id.clone()) {
            Ok(response) => response,
            Err(error) => {
                self.status = Some((format!("Graph rollback failed: {error}"), false));
                log::warn!(
                    "android_graph_rollback_rejected graph_id={} error={error}",
                    failed_graph_id
                );
                return;
            }
        };
        let Some(bundle) = response.graph else {
            self.status = Some(("Graph rollback response omitted its v4 graph".into(), false));
            log::error!("android_graph_rollback_omitted_graph failed_graph_id={failed_graph_id}");
            return;
        };
        let graph_id = bundle.graph_id.clone();
        let root_revision_id = bundle.graph.nodes[&bundle.graph.root].revision_id.clone();
        let Some(root_revision) = bundle
            .revisions
            .iter()
            .find(|revision| revision.revision_id == root_revision_id.as_str())
        else {
            log::error!("android_graph_rollback_omitted_root graph_id={graph_id}");
            std::process::abort();
        };
        let source = root_revision.source.clone();
        let schema_version = root_revision.state.resource.schema_version;
        let revision_id = root_revision.revision_id.clone();
        let graph = match start_android_graph_runtime(bundle, &self.model) {
            Ok(graph) => graph,
            Err(error) => {
                let restore = revision_client::rollback_graph(graph_id.clone());
                log::error!(
                    "android_graph_rollback_runtime_rejected graph_id={graph_id} error={error} restore_ok={}",
                    restore.is_ok()
                );
                std::process::abort();
            }
        };
        let results = graph.worker.results();
        let snapshot = graph.snapshot.clone();
        self.pending_graph_viewport = None;
        self.active_graph = Some(graph);
        self.install_graph_snapshot(snapshot);
        self.source = source;
        self.state_schema_version = schema_version;
        self.system_revision_id = revision_id;
        self.pending_graph_rollback_presentation = Some(graph_id.clone());
        self.status = Some((
            "Graph rollback rendered; awaiting presentation…".into(),
            true,
        ));
        Self::attach_graph_channels(results, cx);
        cx.notify();
    }

    fn advance_agent_activation(&mut self, request_id: u64, phase: AgentActivationPhase) -> bool {
        let Some(evidence) = self.pending_agent_activations.get_mut(&request_id) else {
            return false;
        };
        evidence
            .advance(phase)
            .expect("agent activation evidence must advance in authoritative order");
        true
    }

    fn start_stress(&mut self, request: StressRequest) {
        if self.worker.is_none() {
            log::warn!(
                "stress_failed run_id={} reason=v4_graph_active",
                request.run_id
            );
            self.status = Some((
                "Legacy source-swap stress is disabled for v4 graphs".into(),
                false,
            ));
            return;
        }
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

        let alternate_source = deterministic_mobile_agent_candidate(&self.source);
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
        if let Err(error) = self
            .worker
            .as_ref()
            .expect("standalone worker")
            .prepare_candidate(
                request_id,
                source,
                self.model.clone(),
                self.state.clone(),
                self.state_schema_version,
                Instant::now(),
            )
        {
            self.candidates.remove(&request_id);
            self.fail_stress(format!("worker unavailable: {error}"));
        }
    }

    #[cfg(not(feature = "aosp-system"))]
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
                    let _ = self
                        .worker
                        .as_ref()
                        .expect("standalone worker")
                        .discard_candidate(request_id);
                    return;
                };
                #[cfg(not(feature = "aosp-system"))]
                let _ = &assets;
                if purpose == CandidatePurpose::Regular {
                    if self.advance_agent_activation(request_id, AgentActivationPhase::Validated) {
                        log::info!(
                            "android_agent_candidate_validation_ack request_id={request_id} phase=validated authority_committed=false"
                        );
                    }
                    log::info!(
                        "candidate_validated request_id={} queue_us={} compile_us={} render_us={} worker_total_us={}",
                        request_id,
                        timings.queue_us,
                        timings.compile_us,
                        timings.render_us,
                        timings.worker_total_us
                    );
                    let Some(expected_revision) = self.remote_state_revision else {
                        let _ = self
                            .worker
                            .as_ref()
                            .expect("standalone worker")
                            .discard_candidate(request_id);
                        self.candidates.remove(&request_id);
                        self.pending_agent_activations.remove(&request_id);
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
                        let _ = self
                            .worker
                            .as_ref()
                            .expect("standalone worker")
                            .discard_candidate(request_id);
                        self.candidates.remove(&request_id);
                        self.pending_agent_activations.remove(&request_id);
                        self.status =
                            Some((format!("Could not persist staged revision: {error}"), false));
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
                                let _ = self
                                    .worker
                                    .as_ref()
                                    .expect("standalone worker")
                                    .discard_candidate(request_id);
                                self.candidates.remove(&request_id);
                                self.pending_agent_activations.remove(&request_id);
                                self.status =
                                    Some((format!("Could not stage revision: {error}"), false));
                                return;
                            }
                        };
                        if self.advance_agent_activation(request_id, AgentActivationPhase::Staged) {
                            log::info!(
                                "android_agent_activation_stage_ack request_id={request_id} revision={revision} state_stage_id={stage_id} phase=staged authority_committed=false"
                            );
                        }
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
                                let _ = self
                                    .worker
                                    .as_ref()
                                    .expect("standalone worker")
                                    .discard_candidate(request_id);
                                self.candidates.remove(&request_id);
                                self.pending_agent_activations.remove(&request_id);
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
                        if let Err(error) = self
                            .worker
                            .as_ref()
                            .expect("standalone worker")
                            .commit_candidate(request_id)
                        {
                            self.candidates.remove(&request_id);
                            self.pending_agent_activations.remove(&request_id);
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
                if let Err(error) = self
                    .worker
                    .as_ref()
                    .expect("standalone worker")
                    .commit_candidate(request_id)
                {
                    self.candidates.remove(&request_id);
                    self.pending_agent_activations.remove(&request_id);
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
                self.pending_agent_activations.remove(&request_id);
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
                let agent_activation = self.pending_agent_activations.remove(&request_id);
                if purpose == CandidatePurpose::Regular && authority_activation.is_some() {
                    self.revision_activation_pending = true;
                }
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
                    agent_activation,
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
                #[cfg(feature = "aosp-system")]
                let (host_effects, provider_effects): (Vec<_>, Vec<_>) = effects
                    .into_iter()
                    .partition(|effect| effect.provider == "agent");
                #[cfg(not(feature = "aosp-system"))]
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
                #[cfg(feature = "aosp-system")]
                self.execute_agent_effects(host_effects);
                #[cfg(not(feature = "aosp-system"))]
                {
                    let (network_effects, agent_effects): (Vec<_>, Vec<_>) = host_effects
                        .into_iter()
                        .partition(|effect| effect.provider == "network");
                    self.execute_network_effects(network_effects);
                    self.execute_agent_effects(agent_effects);
                }
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

    #[cfg(not(feature = "aosp-system"))]
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
            log::info!("android_agent_effect_dispatch action={}", effect.action);
            if matches!(
                effect.action.as_str(),
                "configure_openai"
                    | "configure_openrouter"
                    | "configure_codex"
                    | "use_fake"
                    | "clear_credential"
                    | "prompt"
            ) {
                self.model.agent.error = None;
            }
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
            if let Err(error) = &result {
                self.status = Some((format!("Agent action failed: {error}"), false));
                log::warn!("android_agent_action_failed action={}", effect.action);
            }
            #[cfg(feature = "core-native")]
            if result.is_ok() {
                match effect.action.as_str() {
                    "configure_openrouter" => native_input::activate_core_credential_keyboard(),
                    "use_fake" | "clear_credential" => {
                        native_input::deactivate_core_credential_keyboard()
                    }
                    _ => {}
                }
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

    fn handle_agent_update(&mut self, update: agent::AgentUpdate, cx: &mut Context<Self>) {
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
                self.submit_agent_candidate_source(source, cx);
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
        #[cfg(feature = "aosp-system")]
        let semantic_scene = {
            let mut scene = self
                .active_graph
                .as_ref()
                .map(|graph| composed_graph_scene(&graph.snapshot))
                .unwrap_or_else(|| self.scene.clone());
            if self.active_graph.is_some() {
                append_android_system_control_semantics(
                    &mut scene,
                    self.presented_ordinary_experience().is_some(),
                );
            }
            scene
        };
        #[cfg(not(feature = "aosp-system"))]
        let semantic_scene = self.scene.clone();
        match accessibility::publish(&semantic_scene, &text, &scroll) {
            Ok(bytes) => log::info!(
                "accessibility_published bytes={} semantics={}",
                bytes,
                count_semantics(&semantic_scene)
            ),
            Err(error) => log::warn!("accessibility_publish_failed error={error}"),
        }
    }

    #[cfg(not(feature = "aosp-system"))]
    fn frame_presented(&mut self, mut frame: PendingFrame, cx: &mut Context<Self>) {
        let visible_us = micros(frame.timings.submitted_at.elapsed());
        let frame_callback_us = micros(frame.callback_scheduled_at.elapsed());
        let post_worker_us = visible_us.saturating_sub(frame.timings.worker_total_us);
        match frame.purpose {
            CandidatePurpose::Regular => {
                let _ = frame.authority_activation.take();
                if let Some(mut evidence) = frame.agent_activation.take() {
                    evidence
                        .advance(AgentActivationPhase::Committed)
                        .expect("agent activation commit requires staged host evidence");
                    log::info!(
                            "android_agent_activation_commit request_id={} phase=committed authority=host-presented",
                            evidence.request_id()
                        );
                }
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

    #[cfg(feature = "aosp-system")]
    fn confirm_presented_graph(&mut self, graph_id: String) {
        let confirmed = revision_client::confirm_graph(graph_id.clone()).or_else(|error| {
            log::warn!("android_graph_confirmation_ambiguous graph_id={graph_id} error={error}");
            let current = revision_client::current_graph_with_retry()?;
            match current.graph {
                Some(ref bundle) if bundle.graph_id == graph_id && !bundle.migration_pending => {
                    Ok(current)
                }
                _ => Err(error),
            }
        });
        match confirmed {
            Ok(response)
                if response.graph.as_ref().is_some_and(|bundle| {
                    bundle.graph_id == graph_id && !bundle.migration_pending
                }) =>
            {
                log::info!("android_graph_presented graph_id={graph_id} confirmed=true");
                if let Some(mut evidence) = self.pending_graph_agent_activation.take() {
                    evidence
                        .advance(AgentActivationPhase::Committed)
                        .expect("v4 agent commit must follow graph staging");
                    log::info!(
                        "android_agent_activation_commit request_id={} graph_id={} phase=committed authority=system-graph",
                        evidence.request_id(),
                        graph_id
                    );
                }
            }
            Ok(_) | Err(_) => {
                let rollback = revision_client::rollback_graph(graph_id.clone());
                log::error!(
                    "android_graph_confirmation_failed graph_id={graph_id} rollback_ok={}",
                    rollback.is_ok()
                );
                std::process::abort();
            }
        }
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
        owner: Option<GraphOwner>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let local_id = node.id.clone().unwrap_or_else(|| path.to_string());
        let element_id = owner.as_ref().map_or_else(
            || local_id.clone(),
            |owner| format!("{}::{local_id}", owner.instance_id),
        );
        #[cfg(feature = "aosp-system")]
        if let Some(owner) = &owner {
            self.node_owners.insert(element_id.clone(), owner.clone());
        }
        let mut element = div();
        match node.layout.flow {
            Flow::Overlay => {}
            Flow::Column => element = element.flex().flex_col(),
            Flow::Row => element = element.flex().flex_row(),
        }
        if node.layout.wrap {
            element = element.flex_wrap();
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
        if let Some(Content::WindowSpace(space)) = &node.content {
            element = element.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if space.fallback.is_empty() {
                        "Application windows are unavailable on this host".into()
                    } else {
                        space.fallback.clone()
                    }),
            );
        }
        #[cfg(feature = "aosp-system")]
        if let Some(Content::ExperienceMount(mount)) = &node.content {
            element = element.overflow_hidden();
            let mounted = self.active_graph.as_ref().and_then(|graph| {
                let parent = owner
                    .as_ref()
                    .map(|owner| &owner.node_id)
                    .unwrap_or(&graph.snapshot.root);
                graph
                    .snapshot
                    .instances
                    .iter()
                    .find(|(_, instance)| {
                        instance.parent.as_ref() == Some(parent)
                            && instance.dependency.as_ref().map(|alias| alias.as_str())
                                == Some(mount.dependency.as_str())
                    })
                    .map(|(node_id, instance)| {
                        (
                            GraphOwner {
                                node_id: node_id.clone(),
                                instance_id: instance.instance_id.clone(),
                            },
                            instance.scene.clone(),
                            instance.status.clone(),
                        )
                    })
            });
            match mounted {
                Some((child_owner, Some(scene), RuntimeInstanceStatus::Ready)) => {
                    element = element.child(self.render_node(
                        &scene.root,
                        SharedString::from(format!("mount-{}", child_owner.node_id)),
                        Some(child_owner),
                        window,
                        cx,
                    ));
                }
                Some((_, _, RuntimeInstanceStatus::Failed(error))) => {
                    element = element.child(android_mount_fallback(&format!(
                        "Experience unavailable: {error}"
                    )));
                }
                _ => element = element.child(android_mount_fallback("Experience unavailable")),
            }
        }
        if let Some(Content::TextSession(input)) = &node.content {
            let state_key = owner.as_ref().map_or_else(
                || input.state_key.clone(),
                |owner| format!("{}::{}", owner.instance_id, input.state_key),
            );
            let displayed_value = self
                .input_state_shadow
                .get(&state_key)
                .cloned()
                .unwrap_or_else(|| input.value.clone());
            self.input_state_shadow
                .entry(state_key.clone())
                .or_insert_with(|| displayed_value.clone());
            let mut created = false;
            let native = if let Some(native) = self.inputs.get(&element_id) {
                native.clone()
            } else {
                created = true;
                let host = cx.weak_entity();
                let native = cx.new(|input_cx| {
                    NativeTextInput::new(
                        element_id.clone(),
                        state_key.clone(),
                        displayed_value.clone(),
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
                    &state_key,
                    &displayed_value,
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
            let mut tracker = canvas(
                move |bounds, _, _| accessibility::record_bounds(&semantic_id, bounds),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full();
            let tracker_offset = semantic_tracker_offset(node.layout.flow, node.layout.padding);
            if tracker_offset != 0.0 {
                tracker = tracker.left(px(tracker_offset)).top(px(tracker_offset));
            }
            element = element.child(tracker);
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
            element = element.child(self.render_node(child, child_path, owner.clone(), window, cx));
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
                let target = element_id.clone();
                scroll
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.queue_input_event(
                                SceneEvent {
                                    action: action.clone(),
                                    target: Some(target.clone()),
                                    ..Default::default()
                                },
                                cx,
                            )
                        }),
                    )
                    .into_any_element()
            } else {
                scroll.into_any_element()
            }
        } else if let Some(action) = tap_action {
            let action = action.clone();
            let target = element_id.clone();
            element
                .id(SharedString::from(element_id.clone()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.queue_input_event(
                            SceneEvent {
                                action: action.clone(),
                                target: Some(target.clone()),
                                ..Default::default()
                            },
                            cx,
                        )
                    }),
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

#[cfg(feature = "core-native")]
fn core_credential_overlay() -> impl IntoElement {
    let snapshot = agent::credential_snapshot();
    let error = snapshot.error.map(SharedString::from);
    let cancel = div()
        .flex_1()
        .h(px(48.0))
        .rounded(px(12.0))
        .bg(rgb(0xD7D4C9))
        .text_color(rgb(0x17211B))
        .flex()
        .items_center()
        .justify_center()
        .child("Cancel")
        .on_mouse_down(MouseButton::Left, |_, window, app| {
            window.prevent_default();
            app.stop_propagation();
            agent::cancel_credential();
            native_input::deactivate_core_credential_keyboard();
            request_host_frame();
        });
    let save = div()
        .flex_1()
        .h(px(48.0))
        .rounded(px(12.0))
        .bg(rgb(0x2F684B))
        .text_color(rgb(0xFFFFFF))
        .flex()
        .items_center()
        .justify_center()
        .child("Save")
        .on_mouse_down(MouseButton::Left, |_, window, app| {
            window.prevent_default();
            app.stop_propagation();
            if agent::save_credential() {
                native_input::deactivate_core_credential_keyboard();
            }
            request_host_frame();
        });
    let mut card =
        div()
            .w_full()
            .max_w(px(620.0))
            .p(px(22.0))
            .rounded(px(18.0))
            .bg(rgb(0xF3F1E8))
            .text_color(rgb(0x17211B))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(div().text_size(px(22.0)).child("Configure OpenRouter"))
            .child(div().text_size(px(13.0)).child(
                "Memory-only until this Core process exits · deepseek/deepseek-v4-flash-0731",
            ))
            .child(
                div()
                    .h(px(44.0))
                    .px(px(12.0))
                    .rounded(px(10.0))
                    .bg(rgb(0xFFFFFF))
                    .text_size(px(18.0))
                    .flex()
                    .items_center()
                    .child(SharedString::from(snapshot.masked))
                    .on_mouse_down(MouseButton::Left, |_, window, app| {
                        window.prevent_default();
                        app.stop_propagation();
                        native_input::activate_core_credential_keyboard();
                        request_host_frame();
                    }),
            );
    if let Some(error) = error {
        card = card.child(div().text_color(rgb(0x8C3A36)).child(error));
    }
    card = card.child(div().flex().gap(px(10.0)).child(cancel).child(save));
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom(px(220.0))
        .p(px(18.0))
        .bg(gpui::rgba(0x17211BDD))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, |_, window, app| {
            window.prevent_default();
            app.stop_propagation();
        })
        .child(card)
}

impl Render for ExperienceHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "aosp-system")]
        self.sync_root_viewport();
        #[cfg(feature = "core-native")]
        for mut text in native_input::take_core_credential_input() {
            agent::apply_credential_input(&text);
            text.zeroize();
            if !agent::credential_snapshot().visible {
                native_input::deactivate_core_credential_keyboard();
            }
        }
        #[cfg(feature = "core-native")]
        if agent::take_credential_changed() {
            self.model.agent.error = None;
            if let Ok(status) = agent::status() {
                agent::apply_status(&mut self.model.agent, &status);
                self.refresh_model_from_authority();
            }
        }
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
        #[cfg(feature = "aosp-system")]
        let accessibility_scene = self
            .active_graph
            .as_ref()
            .map(|graph| composed_graph_scene(&graph.snapshot))
            .unwrap_or_else(|| self.scene.clone());
        #[cfg(not(feature = "aosp-system"))]
        let accessibility_scene = self.scene.clone();
        while let Some(action) = accessibility::take_action() {
            #[cfg(feature = "aosp-system")]
            if action.kind == "click" {
                let control = match (action.target.as_str(), action.value.as_str()) {
                    (ANDROID_SYSTEM_THEME_ID, "android_system_theme") => {
                        Some(AndroidSystemControl::Theme)
                    }
                    (ANDROID_SYSTEM_ROLLBACK_ID, "android_system_rollback") => {
                        Some(AndroidSystemControl::Rollback)
                    }
                    (ANDROID_SYSTEM_HOME_ID, "android_system_home") => {
                        Some(AndroidSystemControl::Home)
                    }
                    _ => None,
                };
                if let Some(control) = control {
                    self.activate_android_system_control(control, cx);
                    continue;
                }
            }
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
                    let state_key = find_text_session(&accessibility_scene.root, &action.target)
                        .map(|input| {
                            action.target.split_once("::").map_or_else(
                                || input.state_key.clone(),
                                |(instance, _)| format!("{instance}::{}", input.state_key),
                            )
                        });
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
        #[cfg(not(feature = "core-native"))]
        let mobile_navigation = MOBILE_NAVIGATION_REQUEST
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("mobile navigation request lock")
            .take();
        #[cfg(not(feature = "core-native"))]
        if let Some(screen) = mobile_navigation {
            #[cfg(feature = "aosp-system")]
            let mobile_navigation_busy =
                self.action_in_flight || self.pending_graph_confirmation.is_some();
            #[cfg(not(feature = "aosp-system"))]
            let mobile_navigation_busy = self.action_in_flight;
            if mobile_navigation_busy {
                *MOBILE_NAVIGATION_REQUEST
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .expect("mobile navigation request lock") = Some(screen);
                cx.notify();
            } else {
                #[cfg(feature = "aosp-system")]
                if self.presented_ordinary_experience().is_some() {
                    *MOBILE_NAVIGATION_REQUEST
                        .get_or_init(|| Mutex::new(None))
                        .lock()
                        .expect("mobile navigation request lock") = Some(screen);
                    self.activate_android_system_control(AndroidSystemControl::Home, cx);
                } else {
                    self.queue_input_event(
                        SceneEvent {
                            action: format!("navigate_{screen}"),
                            target: Some("stock-mobile-root".into()),
                            ..Default::default()
                        },
                        cx,
                    );
                }
                #[cfg(not(feature = "aosp-system"))]
                self.queue_input_event(
                    SceneEvent {
                        action: format!("navigate_{screen}"),
                        target: Some("stock-mobile-root".into()),
                        ..Default::default()
                    },
                    cx,
                );
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
                self.submit_reload(cx);
            }
        }
        if WORKER_RESTART_REQUESTED.swap(false, Ordering::AcqRel) {
            self.restart_worker(cx);
        }
        if ROLLBACK_REQUESTED.swap(false, Ordering::AcqRel) {
            #[cfg(feature = "aosp-system")]
            self.rollback_active_graph(cx);
            #[cfg(not(feature = "aosp-system"))]
            {
                self.status = Some((
                    "Graph rollback requires the v4 system authority".into(),
                    false,
                ));
            }
        }
        #[cfg(feature = "aosp-system")]
        if APPEARANCE_TOGGLE_REQUESTED.swap(false, Ordering::AcqRel) {
            if self.action_in_flight || self.pending_graph_confirmation.is_some() {
                APPEARANCE_TOGGLE_REQUESTED.store(true, Ordering::Release);
            } else if let Err(error) = self.toggle_authority_appearance() {
                self.status = Some((format!("Appearance write failed: {error}"), false));
                log::warn!("android_appearance_write_rejected error={error}");
            }
        }
        #[cfg(feature = "aosp-system")]
        let lifecycle_request = {
            EXPERIENCE_LIFECYCLE_REQUEST
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("experience lifecycle request lock")
                .take()
        };
        #[cfg(feature = "aosp-system")]
        if let Some(lifecycle) = lifecycle_request {
            if self.action_in_flight || self.pending_graph_confirmation.is_some() {
                *EXPERIENCE_LIFECYCLE_REQUEST
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .expect("experience lifecycle request lock") = Some(lifecycle);
            } else if let Some(graph_id) = self
                .active_graph
                .as_ref()
                .map(|graph| graph.graph_id.clone())
            {
                if let Err(error) = self.stage_lifecycle_graph(graph_id, lifecycle, cx) {
                    self.status = Some((format!("Experience lifecycle failed: {error}"), false));
                }
            } else {
                self.status = Some(("No v4 graph is active".into(), false));
            }
        }
        #[cfg(feature = "aosp-system")]
        let reference_event_request = {
            REFERENCE_GRAPH_EVENT_REQUEST
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("reference graph event request lock")
                .take()
        };
        #[cfg(feature = "aosp-system")]
        if let Some(request) = reference_event_request {
            if self.action_in_flight || self.pending_graph_confirmation.is_some() {
                *REFERENCE_GRAPH_EVENT_REQUEST
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .expect("reference graph event request lock") = Some(request);
            } else if let Some(instance_id) = self.active_graph.as_ref().and_then(|graph| {
                graph
                    .snapshot
                    .instances
                    .values()
                    .find(|instance| instance.experience_id == request.experience_id)
                    .map(|instance| instance.instance_id.clone())
            }) {
                let action = request.action.clone();
                self.dispatch_event(
                    SceneEvent {
                        action: request.action,
                        target: Some(format!("{instance_id}::acceptance-control")),
                        ..SceneEvent::default()
                    },
                    cx,
                );
                log::info!(
                    "android_reference_graph_event_dispatched experience_id={} action={action}",
                    request.experience_id
                );
            } else {
                self.status = Some((
                    format!(
                        "Reference Experience `{}` is not active",
                        request.experience_id
                    ),
                    false,
                ));
            }
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
            && !self.revision_activation_pending
            && !self.pending_input_events.is_empty()
        {
            self.dispatch_pending_input_event(cx);
        }
        #[cfg(not(feature = "aosp-system"))]
        let mut pending_frame = self.pending_frame.take();
        #[cfg(not(feature = "aosp-system"))]
        let render_started_at = Instant::now();

        pointer_input::begin_frame();
        assets::install_fonts(window);
        #[cfg(feature = "aosp-system")]
        self.node_owners.clear();
        let scene = self.scene.clone();
        #[cfg(feature = "aosp-system")]
        let root_owner = self
            .active_graph
            .as_ref()
            .map(|graph| graph_owner(&graph.snapshot, &graph.snapshot.root));
        #[cfg(not(feature = "aosp-system"))]
        let root_owner = None;
        let content = self.render_node(
            &scene.root,
            SharedString::from("root"),
            root_owner,
            window,
            cx,
        );
        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0xF3F1E8))
            .child(content);
        #[cfg(not(feature = "core-native"))]
        {
            root = root.capture_any_mouse_down(cx.listener(|this, event, window, cx| {
                this.blur_compat_input_on_outside_tap(event, window, cx)
            }));
        }
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
        #[cfg(feature = "aosp-system")]
        if self.presented_ordinary_experience().is_some() {
            let control = |id: &'static str, label: &'static str| {
                div()
                    .id(SharedString::from(id))
                    .h(px(34.0))
                    .px(px(12.0))
                    .rounded(px(10.0))
                    .bg(rgb(0x17211B))
                    .text_color(rgb(0xFFFFFF))
                    .text_size(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(label)
                    .child(
                        canvas(
                            move |bounds, _, _| accessibility::record_bounds(id, bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
            };
            let theme = control(ANDROID_SYSTEM_THEME_ID, "Theme").on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.activate_android_system_control(AndroidSystemControl::Theme, cx);
                }),
            );
            let rollback = control(ANDROID_SYSTEM_ROLLBACK_ID, "Rollback").on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.activate_android_system_control(AndroidSystemControl::Rollback, cx);
                }),
            );
            let controls = div()
                .absolute()
                .top(px(12.0))
                .right(px(12.0))
                .flex()
                .gap(px(8.0))
                .child(theme)
                .child(rollback)
                .child(control(ANDROID_SYSTEM_HOME_ID, "Home").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.activate_android_system_control(AndroidSystemControl::Home, cx);
                    }),
                ));
            root = root.child(controls);
        }
        #[cfg(feature = "core-native")]
        if agent::credential_snapshot().visible {
            root = root.child(core_credential_overlay());
        }
        #[cfg(feature = "core-native")]
        if native_input::software_keyboard_visible() {
            root = root.child(native_input::software_keyboard_overlay());
        }
        #[cfg(not(feature = "aosp-system"))]
        if let Some(mut frame) = pending_frame.take() {
            frame.commit_to_render_us =
                micros(render_started_at.duration_since(frame.committed_at));
            frame.render_build_us = micros(render_started_at.elapsed());
            frame.callback_scheduled_at = Instant::now();
            cx.on_next_frame(window, move |this, _, cx| {
                this.frame_presented(frame, cx);
            });
        }
        #[cfg(feature = "aosp-system")]
        if let Some(graph_id) = self.pending_graph_confirmation.take() {
            cx.on_next_frame(window, move |this, _, _| {
                this.confirm_presented_graph(graph_id)
            });
        }
        #[cfg(feature = "aosp-system")]
        if let Some(graph_id) = self.pending_graph_rollback_presentation.take() {
            cx.on_next_frame(window, move |this, _, _| {
                this.status = Some(("Previous v4 graph is active".into(), true));
                log::info!("android_graph_rollback_presented graph_id={graph_id}");
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

#[cfg(feature = "aosp-system")]
fn start_android_graph_runtime(
    bundle: android_authority_protocol::GraphBundle,
    root_model: &ExperienceModel,
) -> Result<ActiveAndroidGraph, String> {
    let grants = bundle
        .grants
        .iter()
        .map(|grant| (grant.experience_id.clone(), grant))
        .collect::<BTreeMap<_, _>>();
    let mut inputs = BTreeMap::new();
    let mut revisions = BTreeMap::new();
    for revision in &bundle.revisions {
        let revision_id =
            RevisionId::parse(revision.revision_id.clone()).map_err(|error| error.to_string())?;
        if revision.package.experience_id != revision.state.experience_id
            || revision.state.resource.revision_id != revision.revision_id
        {
            return Err(format!(
                "graph revision `{revision_id}` has inconsistent experience state"
            ));
        }
        if !revision.package.provider_capabilities.is_empty() {
            let grant = grants
                .get(&revision.package.experience_id)
                .filter(|grant| grant.reviewed)
                .ok_or_else(|| {
                    format!(
                        "experience `{}` has no reviewed Android grant",
                        revision.package.experience_id
                    )
                })?;
            if !revision
                .package
                .provider_capabilities
                .is_subset(&grant.provider_capabilities)
            {
                return Err(format!(
                    "experience `{}` requests ungranted provider capabilities",
                    revision.package.experience_id
                ));
            }
        }
        let allowed_capabilities = revision.package.provider_capabilities.clone();
        let mut model =
            filter_android_graph_model(root_model, revision.package.role, &allowed_capabilities);
        model.appearance = bundle.appearance.profile.clone();
        let sidecars = revision_client::inputs(revision.assets.clone());
        inputs.insert(
            revision_id.clone(),
            GraphRevisionInput {
                source: revision.source.clone(),
                sidecars: sidecars.clone(),
                model,
                state: revision.state.resource.state.clone(),
                state_schema_version: revision.state.resource.schema_version,
                package: revision.package.clone(),
            },
        );
        revisions.insert(
            revision_id,
            AndroidGraphRevision {
                package: revision.package.clone(),
                allowed_capabilities,
                state_revision: revision.state.resource.revision,
                schema_version: revision.state.resource.schema_version,
                sidecars,
            },
        );
    }
    for (node_id, node) in &bundle.graph.nodes {
        let revision = revisions
            .get(&node.revision_id)
            .ok_or_else(|| format!("graph omitted revision `{}`", node.revision_id))?;
        if revision.package.experience_id != node.experience_id
            || !revision
                .package
                .contract
                .exports
                .contains_key(&node.export_id)
        {
            return Err(format!(
                "graph node `{}` does not match its package",
                node_id
            ));
        }
    }
    let (worker, snapshot) = GraphRuntimeWorker::start_with_root_viewport(
        bundle.graph,
        inputs,
        native_input::viewport_context(),
    )
    .map_err(|error| error.to_string())?;
    assets::install_graph(
        snapshot
            .instances
            .values()
            .map(|instance| instance.assets.as_slice()),
    );
    let root_experience = snapshot.instances[&snapshot.root].experience_id.clone();
    log::info!(
        "android_graph_runtime_ready graph_id={} root_experience={} instances={}",
        bundle.graph_id,
        root_experience,
        snapshot.instances.len()
    );
    for (node_id, instance) in &snapshot.instances {
        log::info!(
            "android_graph_instance_ready node_id={} instance_id={} experience_id={} export_id={} status={:?}",
            node_id,
            instance.instance_id,
            instance.experience_id,
            instance.export_id,
            instance.status
        );
    }
    Ok(ActiveAndroidGraph {
        graph_id: bundle.graph_id,
        worker,
        snapshot,
        revisions,
        appearance_generation: bundle.appearance.profile.generation,
    })
}

#[cfg(feature = "aosp-system")]
fn filter_android_graph_model(
    source: &ExperienceModel,
    role: ExperienceRole,
    capabilities: &BTreeSet<String>,
) -> ExperienceModel {
    let has = |capability: &str| capabilities.contains(capability);
    let mut model = source.clone();
    if !has("system_read") {
        model.date.clear();
        model.weather = experience_ir::Weather {
            summary: String::new(),
            temperature_c: 0,
            high_c: 0,
            low_c: 0,
        };
        model.system = experience_ir::SystemState::default();
        model.providers.clock = experience_ir::ClockProviderState::default();
        model.providers.power = experience_ir::PowerProviderState::default();
        model.providers.attention = experience_ir::AttentionProviderState::default();
    }
    if !has("calendar_read") && !has("calendar_write") {
        model.calendar.clear();
    }
    if !has("notes_read") && !has("notes_write") {
        model.notes.clear();
    }
    if !has("music_read") && !has("music_control") {
        model.music = experience_ir::Music {
            title: String::new(),
            artist: String::new(),
            playing: false,
        };
    }
    if !has("network_control") {
        model.network = experience_ir::NetworkState::default();
        model.providers.connectivity = experience_ir::ConnectivityProviderState::default();
    }
    if !has("audio_control") {
        model.providers.audio = experience_ir::AudioProviderState::default();
    }
    if !has("application_launch") {
        model.providers.apps = experience_ir::AppsProviderState::default();
    }
    model.surfaces.retain(|surface| {
        let kind_allowed = match surface.kind {
            experience_ir::ProviderSurfaceKind::Video => has("video_read"),
            experience_ir::ProviderSurfaceKind::Camera => has("camera_read"),
        };
        kind_allowed && (!surface.protected || has("protected_surface"))
    });
    model
        .providers
        .capabilities
        .retain(|capability| match capability {
            experience_ir::SystemCapability::AudioSetVolume
            | experience_ir::SystemCapability::AudioSetMuted => has("audio_control"),
            experience_ir::SystemCapability::MediaPlayPause
            | experience_ir::SystemCapability::MediaNext
            | experience_ir::SystemCapability::MediaPrevious => has("music_control"),
            experience_ir::SystemCapability::WifiConnect
            | experience_ir::SystemCapability::WifiDisconnect => has("network_control"),
            experience_ir::SystemCapability::AppLaunch => has("application_launch"),
            experience_ir::SystemCapability::AttentionAcknowledge
            | experience_ir::SystemCapability::RequestLock
            | experience_ir::SystemCapability::RequestRestart
            | experience_ir::SystemCapability::RequestShutdown => has("system_control"),
        });
    if role != ExperienceRole::Shell {
        model.shell = experience_ir::ShellModel::default();
        model.agent = experience_ir::AgentConversation::default();
    }
    model
}

#[cfg(feature = "aosp-system")]
fn validate_android_candidate_exports(
    runtime: &runtime_luau::LuauRuntime,
    package: &PackageMetadata,
    model: &ExperienceModel,
    state: &JsonValue,
) -> Result<(), String> {
    let report = runtime
        .validate_all(model, state)
        .map_err(|error| error.to_string())?;
    if !report.valid {
        let failures = report
            .scenarios
            .iter()
            .filter_map(|scenario| {
                scenario
                    .diagnostic
                    .as_ref()
                    .map(|diagnostic| diagnostic.message.clone())
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("candidate scenarios failed: {failures}"));
    }
    if package.role == ExperienceRole::Shell
        && !android_shell_has_agent_composer(runtime, model, state)?
    {
        return Err(
            "Stock Mobile candidates must retain a text_session that submits agent_submit".into(),
        );
    }
    let mut appearance_model = model.clone();
    for (export_id, export) in &package.contract.exports {
        let properties = export.properties.example_value();
        for (width, height) in [
            (export.viewport.min_width, export.viewport.min_height),
            (export.viewport.max_width, export.viewport.max_height),
        ] {
            runtime
                .render_export(
                    export_id.as_str(),
                    &appearance_model,
                    state,
                    &properties,
                    experience_ir::ExperienceViewport {
                        width,
                        height,
                        scale_milli: 1000,
                        ..Default::default()
                    },
                    None,
                )
                .map_err(|error| error.to_string())?;
        }
        appearance_model.appearance.contrast = experience_package::Contrast::High;
        appearance_model.appearance.reduce_motion = true;
        runtime
            .render_export(
                export_id.as_str(),
                &appearance_model,
                state,
                &properties,
                experience_ir::ExperienceViewport {
                    width: export.viewport.min_width,
                    height: export.viewport.min_height,
                    scale_milli: 1000,
                    ..Default::default()
                },
                None,
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(feature = "aosp-system")]
fn android_shell_has_agent_composer(
    runtime: &runtime_luau::LuauRuntime,
    model: &ExperienceModel,
    state: &JsonValue,
) -> Result<bool, String> {
    fn contains_composer(node: &SceneNode) -> bool {
        matches!(
            &node.content,
            Some(Content::TextSession(session))
                if session.submit_action.as_deref() == Some("agent_submit")
        ) || node.children.iter().any(contains_composer)
    }

    for (key, value) in [("active_workspace", "agent"), ("shell_panel", "agent")] {
        let mut branch = state.clone();
        let object = branch
            .as_object_mut()
            .ok_or_else(|| "Stock Mobile state must be a record".to_owned())?;
        object.insert(key.into(), JsonValue::String(value.into()));
        let scene = runtime
            .render(model, &branch)
            .map_err(|error| format!("render Stock agent branch: {error}"))?;
        if contains_composer(&scene.root) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(feature = "aosp-system")]
fn android_graph_action_wire(
    graph: &ActiveAndroidGraph,
    previous: &GraphRuntimeSnapshot,
    outcome: &runtime_luau::GraphActionOutcome,
) -> Result<
    (
        Vec<android_authority_protocol::GraphStateUpdateWire>,
        Vec<android_authority_protocol::GraphEffectWire>,
        Vec<ProviderEffect>,
        Option<AndroidExperienceLifecycle>,
    ),
    String,
> {
    let mut affected_nodes = BTreeSet::new();
    for (node_id, instance) in &outcome.snapshot.instances {
        let prior = previous
            .instances
            .get(node_id)
            .ok_or_else(|| format!("graph result introduced unknown node `{node_id}`"))?;
        if prior.instance_id != instance.instance_id
            || prior.experience_id != instance.experience_id
            || prior.revision_id != instance.revision_id
        {
            return Err(format!(
                "graph result changed identity for node `{node_id}`"
            ));
        }
        if prior.state != instance.state {
            affected_nodes.insert(node_id.clone());
        }
    }
    for effect in &outcome.effects {
        let instance = outcome
            .snapshot
            .instances
            .get(&effect.node_id)
            .ok_or_else(|| format!("graph effect names unknown node `{}`", effect.node_id))?;
        if instance.instance_id != effect.instance_id || instance.revision_id != effect.revision_id
        {
            return Err("graph effect identity does not match its instance".into());
        }
        affected_nodes.insert(effect.node_id.clone());
    }
    let updates = affected_nodes
        .iter()
        .map(|node_id| {
            let instance = &outcome.snapshot.instances[node_id];
            let revision = graph
                .revisions
                .get(&instance.revision_id)
                .ok_or_else(|| "graph revision metadata is missing".to_owned())?;
            Ok(android_authority_protocol::GraphStateUpdateWire {
                node_id: node_id.clone(),
                instance_id: instance.instance_id.clone(),
                experience_id: instance.experience_id.clone(),
                revision_id: instance.revision_id.clone(),
                expected_revision: revision.state_revision,
                state: instance.state.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut effects = Vec::new();
    let mut agent_effects = Vec::new();
    let mut lifecycle = None;
    for effect in &outcome.effects {
        if effect.effect.provider == "agent" {
            agent_effects.push(effect.effect.clone());
        } else if effect.effect.provider == "shell" {
            if effect.node_id != outcome.snapshot.root {
                return Err("only the presented graph root may request shell lifecycle".into());
            }
            let instance = &outcome.snapshot.instances[&effect.node_id];
            let revision = graph
                .revisions
                .get(&instance.revision_id)
                .ok_or_else(|| "shell lifecycle revision metadata is missing".to_owned())?;
            if revision.package.role != ExperienceRole::Shell {
                return Err("only the registry-authorized shell may request lifecycle".into());
            }
            let experience_id = effect
                .effect
                .payload
                .get("experience_id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "shell lifecycle requires experience_id".to_owned())?;
            let experience_id =
                ExperienceId::parse(experience_id).map_err(|error| error.to_string())?;
            let requested = match effect.effect.action.as_str() {
                "present_experience" => AndroidExperienceLifecycle::Present(experience_id),
                "dismiss_experience" => AndroidExperienceLifecycle::Dismiss(experience_id),
                action => return Err(format!("unsupported shell lifecycle action `{action}`")),
            };
            if lifecycle.replace(requested).is_some() {
                return Err(
                    "one graph action may request at most one shell lifecycle change".into(),
                );
            }
        } else {
            effects.push(android_authority_protocol::GraphEffectWire {
                node_id: effect.node_id.clone(),
                instance_id: effect.instance_id.clone(),
                revision_id: effect.revision_id.clone(),
                effect: effect.effect.clone(),
            });
        }
    }
    Ok((updates, effects, agent_effects, lifecycle))
}

#[cfg(feature = "aosp-system")]
fn graph_owner(snapshot: &GraphRuntimeSnapshot, node_id: &GraphNodeId) -> GraphOwner {
    GraphOwner {
        node_id: node_id.clone(),
        instance_id: snapshot.instances[node_id].instance_id.clone(),
    }
}

#[cfg(feature = "aosp-system")]
fn graph_owner_for_target(snapshot: &GraphRuntimeSnapshot, target: &str) -> Option<GraphOwner> {
    let (instance_id, _) = target.split_once("::")?;
    snapshot
        .instances
        .iter()
        .find(|(_, instance)| instance.instance_id.as_str() == instance_id)
        .map(|(node_id, instance)| GraphOwner {
            node_id: node_id.clone(),
            instance_id: instance.instance_id.clone(),
        })
}

#[cfg(feature = "aosp-system")]
fn log_android_graph_status_transitions(
    previous: &GraphRuntimeSnapshot,
    next: &GraphRuntimeSnapshot,
) {
    let root_ready = next
        .instances
        .get(&next.root)
        .is_some_and(|instance| instance.status == RuntimeInstanceStatus::Ready);
    for (node_id, instance) in &next.instances {
        let prior = previous.instances.get(node_id).map(|prior| &prior.status);
        match (&instance.status, prior) {
            (RuntimeInstanceStatus::Failed(error), Some(RuntimeInstanceStatus::Ready)) => {
                log::warn!(
                    "android_graph_instance_failed node_id={} instance_id={} experience_id={} root_ready={} error={}",
                    node_id,
                    instance.instance_id,
                    instance.experience_id,
                    root_ready,
                    error
                );
            }
            (RuntimeInstanceStatus::Ready, Some(RuntimeInstanceStatus::Failed(_))) => {
                log::info!(
                    "android_graph_instance_recovered node_id={} instance_id={} experience_id={} root_ready={}",
                    node_id,
                    instance.instance_id,
                    instance.experience_id,
                    root_ready
                );
            }
            _ => {}
        }
    }
}

fn android_mount_fallback(message: &str) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .bg(rgb(0x20252B))
        .text_color(rgb(0xD5D9E0))
        .text_size(px(13.0))
        .child(SharedString::from(message.to_owned()))
        .into_any_element()
}

#[cfg(all(feature = "aosp-system", not(feature = "core-native")))]
fn queue_android_appearance_toggle() {
    APPEARANCE_TOGGLE_REQUESTED.store(true, Ordering::Release);
    log::info!("android_appearance_toggle_requested");
}

#[cfg(all(not(feature = "aosp-system"), not(feature = "core-native")))]
fn queue_android_appearance_toggle() {
    log::warn!("android_appearance_toggle_unsupported_without_aosp_system");
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

#[cfg(all(feature = "aosp-system", not(feature = "core-native")))]
fn queue_android_experience_lifecycle(experience_id: &str, dismiss: bool) {
    match ExperienceId::parse(experience_id) {
        Ok(experience_id) => {
            let request = if dismiss {
                AndroidExperienceLifecycle::Dismiss(experience_id)
            } else {
                AndroidExperienceLifecycle::Present(experience_id)
            };
            *EXPERIENCE_LIFECYCLE_REQUEST
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("experience lifecycle request lock") = Some(request.clone());
            log::info!("android_experience_lifecycle_requested action={request:?}");
            request_host_frame();
        }
        Err(error) => log::warn!("android_experience_lifecycle_url_rejected error={error}"),
    }
}

#[cfg(all(not(feature = "aosp-system"), not(feature = "core-native")))]
fn queue_android_experience_lifecycle(_experience_id: &str, _dismiss: bool) {
    log::warn!("android_experience_lifecycle_unsupported_without_aosp_system");
}

#[cfg(all(feature = "aosp-system", not(feature = "core-native")))]
fn queue_android_reference_graph_event(path: &str) {
    let request = path.split_once('/').and_then(|(experience_id, action)| {
        if action.is_empty()
            || action.len() > 64
            || !action
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return None;
        }
        Some(AndroidReferenceGraphEvent {
            experience_id: ExperienceId::parse(experience_id).ok()?,
            action: action.to_owned(),
        })
    });
    match request {
        Some(request) => {
            *REFERENCE_GRAPH_EVENT_REQUEST
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("reference graph event request lock") = Some(request.clone());
            log::info!("android_reference_graph_event_requested event={request:?}");
            request_host_frame();
        }
        None => log::warn!("android_reference_graph_event_url_rejected path={path}"),
    }
}

#[cfg(all(not(feature = "aosp-system"), not(feature = "core-native")))]
fn queue_android_reference_graph_event(_path: &str) {
    log::warn!("android_reference_graph_event_unsupported_without_aosp_system");
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

#[cfg(not(feature = "core-native"))]
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
    let active = read_file(ACTIVE_FILE).unwrap_or_else(|| MOBILE_EXPERIENCE.to_owned());
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

#[cfg(all(feature = "offline-fallback", not(feature = "aosp-system")))]
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

#[cfg(feature = "aosp-system")]
fn append_android_system_control_semantics(scene: &mut Scene, ordinary_root: bool) {
    if !ordinary_root {
        return;
    }
    let control = |id: &str, label: &str, action: &str| SceneNode {
        id: Some(id.into()),
        interaction: Interaction {
            tap_action: Some(action.into()),
            ..Default::default()
        },
        semantics: Some(Semantics {
            role: SemanticRole::Button,
            label: label.into(),
            value: None,
            hint: Some("SOS system control".into()),
        }),
        ..Default::default()
    };
    scene.root.children.extend([
        control(
            ANDROID_SYSTEM_THEME_ID,
            "Change system theme",
            "android_system_theme",
        ),
        control(
            ANDROID_SYSTEM_ROLLBACK_ID,
            "Roll back Experience graph",
            "android_system_rollback",
        ),
    ]);
    if ordinary_root {
        scene.root.children.push(control(
            ANDROID_SYSTEM_HOME_ID,
            "Return to Stock",
            "android_system_home",
        ));
    }
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
