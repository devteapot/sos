use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::{
    ShellOverlayConfiguration, WindowLayoutMode as CompositorWindowLayoutMode,
    WindowSpaceConfiguration, WindowSpaceGeometry,
};
use experience_host_protocol::{HostEvent, HostRequest};
use experience_ir::{
    AgentMessage, AgentMessageRole, Align, AnimationKind, Content, ExperienceModel, Flow,
    HitRegion, Interaction, Justify, PaintOp, Scene, SceneEvent, SceneNode, WindowLayoutMode,
    WindowSpaceContent, EXPERIENCE_API_VERSION, MAX_AGENT_MESSAGES, MAX_AGENT_MESSAGE_BYTES,
};
use gpui::{
    div, img, point, prelude::*, px, relative, rgb, size, Animation as GpuiAnimation,
    AnimationExt as _, AnyElement, App, Bounds, Context, Entity, MouseButton, Render, SharedString,
    Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
};
use provider_state_service::ServiceClient;
use providers_linux::{load_grants, ProviderContext, ProviderFrame, ProviderHub, ProviderSnapshot};
use runtime_luau::{
    load_revision_assets, RevisionAssetInput, RuntimeWorker, WorkerReady, WorkerResult,
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use service_protocol::{
    NotesAction, PromotionDraft, ProviderAction, ResourceQuery, ResourceValue, ResponsePayload,
    ServiceError, ServiceRequest, StateResource, TransactionStatus,
};
use sha2::{Digest, Sha256};

use crate::agent_bridge::{self, AgentUpdate};
use crate::assets::{self, SosAssets, ALBUM_ASSET};
use crate::compositor_fence::{CompositorFence, FenceEvent};
use crate::linux_accessibility::{self, Action as AccessibilityAction};
use crate::linux_input::{self, NativeTextInput};
use crate::pointer_input;
use crate::scene_surface;
use crate::window_space;

#[derive(Clone, Debug)]
struct Options {
    service_socket: Option<PathBuf>,
    agent_socket: Option<PathBuf>,
    windowed: bool,
}

#[derive(Clone, Debug)]
struct ProviderUpdate {
    generation: String,
    revision_id: String,
    model: ExperienceModel,
    frames: Vec<ProviderFrame>,
}

#[derive(Clone, Debug)]
struct LinuxProviderAccess {
    hub: ProviderHub,
    grant_path: PathBuf,
    allow_development_wildcard: bool,
    active_revision: Arc<Mutex<String>>,
}

impl LinuxProviderAccess {
    fn context(&self, revision_id: &str) -> Result<ProviderContext> {
        load_grants(
            &self.grant_path,
            revision_id,
            self.allow_development_wildcard,
        )
        .with_context(|| format!("load provider grants {}", self.grant_path.display()))
    }

    fn snapshot(&self, revision_id: &str) -> Result<ProviderSnapshot> {
        self.hub
            .snapshot_with_frames(&self.context(revision_id)?)
            .with_context(|| format!("read Linux provider snapshot for revision {revision_id}"))
    }

    fn execute_effects(
        &self,
        revision_id: &str,
        effects: &[experience_ir::ProviderEffect],
    ) -> std::result::Result<(), String> {
        let context = self
            .context(revision_id)
            .map_err(|error| error.to_string())?;
        for effect in effects {
            self.hub
                .execute_effect(&context, effect)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn activate(&self, revision_id: &str) {
        *self.active_revision.lock().expect("provider revision lock") = revision_id.into();
    }

    fn active_revision(&self) -> String {
        self.active_revision
            .lock()
            .expect("provider revision lock")
            .clone()
    }
}

#[derive(Debug)]
enum ProtocolInput {
    Request(HostRequest),
    Failed(String),
    Closed,
}

#[derive(Clone, Debug)]
struct LoadedRevision {
    revision_id: String,
    source: String,
    source_sha256: String,
    state: JsonValue,
    schema_version: u64,
    assets: Vec<RevisionAssetInput>,
}

#[derive(Clone, Debug)]
struct PreparingRevision {
    prepare_request_id: u64,
    revision: LoadedRevision,
    model: ExperienceModel,
    provider_frames: Vec<ProviderFrame>,
}

#[derive(Clone, Debug)]
struct PreparedRevision {
    prepare_request_id: u64,
    revision: LoadedRevision,
    model: ExperienceModel,
    provider_frames: Vec<ProviderFrame>,
}

#[derive(Clone, Debug)]
struct PendingCommit {
    present_request_id: u64,
    revision: PreparedRevision,
}

#[derive(Clone, Debug)]
struct PendingPresentation {
    request_id: u64,
    revision_id: String,
}

#[derive(Debug)]
struct ActionCommitResult {
    request_id: u64,
    state: JsonValue,
    scene: Scene,
    result: std::result::Result<ActionCommitOutcome, String>,
}

#[derive(Debug)]
struct ActionCommitOutcome {
    authoritative: StateResource,
    agent_prompt: Option<String>,
}

#[derive(Clone, Debug)]
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

#[derive(Debug, Deserialize)]
struct RevisionManifest {
    format_version: u32,
    revision_id: String,
    schema_version: u64,
    experience_api_version: u32,
    source: FileIdentity,
    state: FileIdentity,
}

#[derive(Debug, Deserialize)]
struct FileIdentity {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct DurableState {
    schema_version: u64,
    source_sha256: String,
    state: JsonValue,
}

pub fn run() -> Result<()> {
    let touch_wakes = pointer_input::install();
    let options = parse_options(std::env::args().skip(1))?;
    let accessibility =
        linux_accessibility::start_from_environment().map_err(|error| anyhow::anyhow!(error))?;
    let (first_request, reader) = read_first_request()?;
    let HostRequest::Boot {
        request_id,
        revision_id,
        revision_path,
        experience_api_version,
    } = first_request
    else {
        bail!("the first host request must be boot");
    };
    let revision = load_revision(&revision_id, &revision_path, experience_api_version)?;
    let compositor_fence = CompositorFence::from_environment()?;
    if let Some(fence) = &compositor_fence {
        fence
            .quiesce_input(request_id, &revision.revision_id)
            .context("quiesce input for compositor boot presentation")?;
        let after_commit_sequence = fence
            .arm(request_id, &revision.revision_id)
            .context("arm boot presentation with SOS compositor")?;
        eprintln!(
            "sos_compositor_armed request_id={request_id} revision_id={} after_commit_sequence={after_commit_sequence}",
            revision.revision_id
        );
    }
    let (mut model, provider_updates, provider_access) =
        start_provider_updates(&revision.revision_id)?;
    model.agent.available = options
        .agent_socket
        .as_ref()
        .is_some_and(|socket| socket.exists());
    let (worker, ready) = RuntimeWorker::start_with_assets(
        revision.source.clone(),
        model.clone(),
        revision.state.clone(),
        revision.schema_version,
        revision.assets.clone(),
    )
    .map_err(|error| anyhow::anyhow!("initialize Luau runtime worker: {error}"))?;
    let results = worker.results();
    let (protocol_tx, protocol_rx) = async_channel::unbounded();
    spawn_protocol_reader(reader, protocol_tx)?;

    eprintln!(
        "sos_experience_host_start revision_id={} source_sha256={} worker_thread={} initialize_us={} service_socket={}",
        revision.revision_id,
        revision.source_sha256,
        ready.worker_thread,
        ready.initialize_us,
        options
            .service_socket
            .as_deref()
            .map_or("none".into(), |path| path.display().to_string())
    );

    let windowed = options.windowed;
    gpui_platform::application()
        .with_assets(SosAssets)
        .run(move |cx: &mut App| {
            linux_input::bind_keys(cx);
            let restore_bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
            let window_bounds = if windowed {
                WindowBounds::Windowed(restore_bounds)
            } else {
                WindowBounds::Fullscreen(restore_bounds)
            };
            let host_entity = Rc::new(RefCell::new(None::<Entity<LinuxExperienceHost>>));
            let host_entity_for_window = host_entity.clone();
            let host = cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    titlebar: None,
                    app_id: Some("dev.sos.experience".into()),
                    ..Default::default()
                },
                move |_, cx| {
                    let entity = cx.new(|cx| {
                        LinuxExperienceHost::new(
                            model,
                            worker,
                            ready,
                            revision,
                            request_id,
                            protocol_rx,
                            results,
                            provider_updates,
                            provider_access,
                            options.agent_socket,
                            accessibility,
                            options.service_socket,
                            compositor_fence,
                            touch_wakes,
                            cx,
                        )
                    });
                    *host_entity_for_window.borrow_mut() = Some(entity.clone());
                    entity
                },
            );
            match host {
                Ok(_) => {
                    let Some(host_entity) = host_entity.borrow().clone() else {
                        eprintln!("sos_experience_overlay_failed error=host entity unavailable");
                        cx.quit();
                        return;
                    };
                    let overlay_host = host_entity.clone();
                    let overlay_bounds = Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(72.), px(72.)),
                    };
                    let overlay = cx.open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(overlay_bounds)),
                            titlebar: None,
                            focus: false,
                            kind: WindowKind::Normal,
                            is_movable: true,
                            is_resizable: false,
                            window_background: WindowBackgroundAppearance::Transparent,
                            window_decorations: Some(WindowDecorations::Client),
                            app_id: Some("dev.sos.experience.overlay".into()),
                            ..Default::default()
                        },
                        move |_, cx| cx.new(|cx| ShellOverlayView::new(overlay_host, cx)),
                    );
                    if let Err(error) = overlay {
                        eprintln!("sos_experience_overlay_failed error={error}");
                        cx.quit();
                        return;
                    }
                    let application_bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
                    let application = cx.open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(application_bounds)),
                            titlebar: None,
                            app_id: Some("dev.sos.experience.application".into()),
                            ..Default::default()
                        },
                        move |_, cx| cx.new(|cx| ApplicationSurfaceView::new(host_entity, cx)),
                    );
                    if let Err(error) = application {
                        eprintln!("sos_experience_application_surface_failed error={error}");
                        cx.quit();
                        return;
                    }
                    cx.activate(true)
                }
                Err(error) => {
                    eprintln!("sos_experience_window_failed error={error}");
                    cx.quit();
                }
            }
        });
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderTarget {
    Base,
    Overlay,
    Application,
}

struct ShellOverlayView {
    host: Entity<LinuxExperienceHost>,
}

impl ShellOverlayView {
    fn new(host: Entity<LinuxExperienceHost>, cx: &mut Context<Self>) -> Self {
        cx.observe(&host, |_, _, cx| cx.notify()).detach();
        Self { host }
    }
}

impl Render for ShellOverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        assets::install_fonts(window);
        let host = self.host.clone();
        let content = host.update(cx, |host, host_cx| {
            let overlay = shell_overlay_node(&host.scene.root).cloned();
            overlay.map(|node| {
                host.render_node(
                    &node,
                    SharedString::from("shell-overlay"),
                    RenderTarget::Overlay,
                    window,
                    host_cx,
                )
            })
        });
        div()
            .id("shell-overlay-window")
            .size_full()
            .when_some(content, |root, content| root.child(content))
    }
}

struct ApplicationSurfaceView {
    host: Entity<LinuxExperienceHost>,
}

impl ApplicationSurfaceView {
    fn new(host: Entity<LinuxExperienceHost>, cx: &mut Context<Self>) -> Self {
        cx.observe(&host, |_, _, cx| cx.notify()).detach();
        Self { host }
    }
}

impl Render for ApplicationSurfaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        assets::install_fonts(window);
        let host = self.host.clone();
        let content = host.update(cx, |host, host_cx| {
            let application = application_surface_node(&host.scene.root).cloned();
            application.map(|node| {
                if let Some(Content::ApplicationSurface(application)) = &node.content {
                    window.set_window_title(&application.title);
                }
                host.render_node(
                    &node,
                    SharedString::from("application-surface"),
                    RenderTarget::Application,
                    window,
                    host_cx,
                )
            })
        });
        div()
            .id("sos-application-window")
            .size_full()
            .when_some(content, |root, content| root.child(content))
    }
}

pub(super) struct LinuxExperienceHost {
    model: ExperienceModel,
    worker: RuntimeWorker,
    scene: Scene,
    state: JsonValue,
    state_schema_version: u64,
    active_revision_id: String,
    active_source_sha256: String,
    service_socket: Option<PathBuf>,
    compositor_fence: Option<CompositorFence>,
    last_window_space: Option<WindowSpaceConfiguration>,
    last_shell_overlay: Option<ShellOverlayConfiguration>,
    preparing: Option<PreparingRevision>,
    prepared: Option<PreparedRevision>,
    pending_commit: Option<PendingCommit>,
    pending_presentation: Option<PendingPresentation>,
    last_presented_revision: Option<String>,
    input_quiesced_revision: Option<String>,
    action_in_flight: bool,
    pending_input_events: VecDeque<SceneEvent>,
    inputs: HashMap<String, Entity<NativeTextInput>>,
    input_state_shadow: HashMap<String, String>,
    active_input_id: Option<String>,
    pending_focus_restore: Option<String>,
    action_commits: async_channel::Sender<ActionCommitResult>,
    next_action_request_id: u64,
    surface_gestures: HashMap<String, GestureSession>,
    surface_taps: HashMap<String, (String, Instant)>,
    status: Option<(String, bool)>,
    provider_refresh: Option<(u64, ExperienceModel)>,
    queued_provider_model: Option<ExperienceModel>,
    next_provider_request_id: u64,
    provider_access: Option<LinuxProviderAccess>,
    agent_socket: Option<PathBuf>,
    agent_updates: async_channel::Sender<AgentUpdate>,
    accessibility: Option<linux_accessibility::Service>,
    accessibility_actions: VecDeque<AccessibilityAction>,
    semantic_focus: Option<String>,
}

impl LinuxExperienceHost {
    #[allow(clippy::too_many_arguments)]
    fn new(
        model: ExperienceModel,
        worker: RuntimeWorker,
        ready: WorkerReady,
        revision: LoadedRevision,
        boot_request_id: u64,
        protocol: async_channel::Receiver<ProtocolInput>,
        results: async_channel::Receiver<WorkerResult>,
        provider_updates: async_channel::Receiver<ProviderUpdate>,
        provider_access: Option<LinuxProviderAccess>,
        agent_socket: Option<PathBuf>,
        accessibility: Option<linux_accessibility::Service>,
        service_socket: Option<PathBuf>,
        compositor_fence: Option<CompositorFence>,
        touch_wakes: async_channel::Receiver<()>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (action_commits, action_results) = async_channel::unbounded();
        let (agent_updates, agent_results) = async_channel::unbounded();
        Self::attach_protocol(protocol, cx);
        Self::attach_worker_results(results, cx);
        Self::attach_provider_updates(provider_updates, cx);
        Self::attach_action_results(action_results, cx);
        Self::attach_agent_updates(agent_results, cx);
        Self::attach_touch_wakes(touch_wakes, cx);
        if let Some(accessibility) = &accessibility {
            Self::attach_accessibility_actions(accessibility.actions(), cx);
        }
        if let Some(fence) = &compositor_fence {
            Self::attach_compositor_events(fence.events(), cx);
        }
        let boot_quiesced_revision = compositor_fence
            .as_ref()
            .map(|_| revision.revision_id.clone());
        assets::install(&ready.assets);
        Self {
            model,
            worker,
            scene: ready.scene,
            state: ready.state,
            state_schema_version: ready.state_schema_version,
            active_revision_id: revision.revision_id.clone(),
            active_source_sha256: revision.source_sha256,
            service_socket,
            compositor_fence,
            last_window_space: None,
            last_shell_overlay: None,
            preparing: None,
            prepared: None,
            pending_commit: None,
            pending_presentation: Some(PendingPresentation {
                request_id: boot_request_id,
                revision_id: revision.revision_id,
            }),
            last_presented_revision: None,
            input_quiesced_revision: boot_quiesced_revision,
            action_in_flight: false,
            pending_input_events: VecDeque::new(),
            inputs: HashMap::new(),
            input_state_shadow: HashMap::new(),
            active_input_id: None,
            pending_focus_restore: None,
            action_commits,
            next_action_request_id: 1,
            surface_gestures: HashMap::new(),
            surface_taps: HashMap::new(),
            status: Some(("Booting committed SOS revision…".into(), true)),
            provider_refresh: None,
            queued_provider_model: None,
            next_provider_request_id: 1,
            provider_access,
            agent_socket,
            agent_updates,
            accessibility,
            accessibility_actions: VecDeque::new(),
            semantic_focus: None,
        }
    }

    fn attach_protocol(protocol: async_channel::Receiver<ProtocolInput>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while let Ok(input) = protocol.recv().await {
                let should_stop = matches!(input, ProtocolInput::Closed);
                if this
                    .update(cx, |this, cx| this.handle_protocol_input(input, cx))
                    .is_err()
                    || should_stop
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn attach_worker_results(
        results: async_channel::Receiver<WorkerResult>,
        cx: &mut Context<Self>,
    ) {
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

    fn attach_provider_updates(
        updates: async_channel::Receiver<ProviderUpdate>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Ok(update) = updates.recv().await {
                if this
                    .update(cx, |this, cx| {
                        if update.revision_id != this.active_revision_id {
                            eprintln!(
                                "sos_provider_event_dropped event_revision={} active_revision={}",
                                update.revision_id, this.active_revision_id
                            );
                            return;
                        }
                        eprintln!(
                            "sos_provider_event generation={} revision_id={}",
                            update.generation, this.active_revision_id
                        );
                        assets::install_provider_frames(&update.frames);
                        let mut model = update.model;
                        model.agent = this.model.agent.clone();
                        this.request_model_refresh(model, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn request_model_refresh(&mut self, model: ExperienceModel, cx: &mut Context<Self>) {
        if self.provider_refresh.is_some()
            || self.action_in_flight
            || self.preparing.is_some()
            || self.prepared.is_some()
            || self.pending_commit.is_some()
            || self.pending_presentation.is_some()
        {
            self.queued_provider_model = Some(model);
            return;
        }
        let request_id = self.next_provider_request_id;
        self.next_provider_request_id = self.next_provider_request_id.wrapping_add(1).max(1);
        match self
            .worker
            .refresh_model(request_id, model.clone(), self.state.clone())
        {
            Ok(()) => self.provider_refresh = Some((request_id, model)),
            Err(error) => {
                self.status = Some((format!("Provider refresh could not start: {error}"), false));
                cx.notify();
            }
        }
    }

    fn dispatch_queued_provider_model(&mut self, cx: &mut Context<Self>) {
        if let Some(model) = self.queued_provider_model.take() {
            self.request_model_refresh(model, cx);
        }
    }

    fn attach_action_results(
        results: async_channel::Receiver<ActionCommitResult>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Ok(result) = results.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.handle_action_commit(result, cx);
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

    fn attach_agent_updates(updates: async_channel::Receiver<AgentUpdate>, cx: &mut Context<Self>) {
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

    fn attach_accessibility_actions(
        actions: async_channel::Receiver<AccessibilityAction>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Ok(action) = actions.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.accessibility_actions.push_back(action);
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

    fn attach_touch_wakes(wakes: async_channel::Receiver<()>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while wakes.recv().await.is_ok() {
                if this
                    .update(cx, |_, cx| {
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

    fn handle_agent_update(&mut self, update: AgentUpdate, cx: &mut Context<Self>) {
        match update {
            AgentUpdate::Started { prompt } => {
                eprintln!("sos_agent_prompt_started bytes={}", prompt.len());
                self.model.agent.available = true;
                self.model.agent.busy = true;
                self.model.agent.activity = "Thinking…".into();
                self.model.agent.error = None;
                push_agent_message(&mut self.model, AgentMessageRole::User, prompt);
            }
            AgentUpdate::TextDelta(delta) => {
                eprintln!("sos_agent_text_delta bytes={}", delta.len());
                self.model.agent.activity = "Writing…".into();
                append_assistant_delta(&mut self.model, &delta);
            }
            AgentUpdate::ToolStarted(name) => {
                eprintln!("sos_agent_tool_started name={name}");
                self.model.agent.activity = format!("Using {}…", display_agent_tool(&name));
            }
            AgentUpdate::ToolFinished { name, ok } => {
                eprintln!("sos_agent_tool_finished name={name} ok={ok}");
                self.model.agent.activity = if ok {
                    format!("Finished {}", display_agent_tool(&name))
                } else {
                    format!("{} failed", display_agent_tool(&name))
                };
            }
            AgentUpdate::Completed => {
                eprintln!("sos_agent_prompt_completed");
                self.model.agent.busy = false;
                self.model.agent.activity = "Ready".into();
            }
            AgentUpdate::Failed(error) => {
                eprintln!("sos_agent_prompt_failed error={error}");
                self.model.agent.available = self.agent_socket.is_some();
                self.model.agent.busy = false;
                self.model.agent.activity = "Could not complete request".into();
                self.model.agent.error = Some(truncate_agent_text(error));
            }
        }
        self.request_model_refresh(self.model.clone(), cx);
    }

    fn attach_compositor_events(
        events: async_channel::Receiver<FenceEvent>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let failed = matches!(event, FenceEvent::Failed(_));
                if this
                    .update(cx, |this, cx| {
                        this.handle_compositor_event(event, cx);
                        cx.notify();
                    })
                    .is_err()
                    || failed
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_compositor_event(&mut self, event: FenceEvent, cx: &mut Context<Self>) {
        match event {
            FenceEvent::Presented(presented) => {
                let Some(pending) = self.pending_presentation.take() else {
                    eprintln!(
                        "sos_compositor_evidence_unexpected request_id={} revision_id={}",
                        presented.request_id, presented.revision_id
                    );
                    cx.quit();
                    return;
                };
                if pending.request_id != presented.request_id
                    || pending.revision_id != presented.revision_id
                {
                    eprintln!(
                        "sos_compositor_evidence_mismatch expected_request_id={} expected_revision_id={} request_id={} revision_id={}",
                        pending.request_id,
                        pending.revision_id,
                        presented.request_id,
                        presented.revision_id
                    );
                    cx.quit();
                    return;
                }
                self.last_presented_revision = Some(presented.revision_id.clone());
                self.input_quiesced_revision = None;
                self.status = None;
                eprintln!(
                    "sos_revision_frame revision_id={} evidence={} commit_sequence={} submit_sequence={}",
                    presented.revision_id,
                    presented.evidence.name(),
                    presented.commit_sequence,
                    presented.submit_sequence
                );
                emit(&HostEvent::Presented {
                    request_id: presented.request_id,
                    revision_id: presented.revision_id,
                });
                self.dispatch_pending_input_event(cx);
                self.dispatch_queued_provider_model(cx);
            }
            FenceEvent::WindowSpaceRejected(error) => {
                eprintln!("sos_window_space_rejected error={error}");
            }
            FenceEvent::ShellOverlayMoved(configuration) => {
                self.last_shell_overlay = Some(configuration);
                let event = SceneEvent {
                    action: "shell_overlay_moved".into(),
                    target: Some("agent-overlay".into()),
                    x: Some(configuration.x as f32),
                    y: Some(configuration.y as f32),
                    ..Default::default()
                };
                if self.pending_presentation.is_some() {
                    self.enqueue_pending_event(event);
                } else {
                    self.queue_input_event(event, cx);
                }
            }
            FenceEvent::ShellOverlayActivated => {
                let event = SceneEvent {
                    action: "shell_overlay_activated".into(),
                    target: Some("agent-overlay".into()),
                    ..Default::default()
                };
                if self.pending_presentation.is_some() {
                    self.enqueue_pending_event(event);
                } else {
                    self.queue_input_event(event, cx);
                }
            }
            FenceEvent::ShellOverlayHoverChanged(hovered) => {
                let action = shell_overlay_node(&self.scene.root)
                    .and_then(|node| node.interaction.hover_action.clone());
                if let Some(action) = action {
                    let event = SceneEvent {
                        action,
                        target: Some("agent-overlay".into()),
                        focused: Some(hovered),
                        ..Default::default()
                    };
                    if self.pending_presentation.is_some() {
                        self.enqueue_pending_event(event);
                    } else {
                        self.queue_input_event(event, cx);
                    }
                }
            }
            FenceEvent::ShellOverlayRejected(error) => {
                eprintln!("sos_shell_overlay_rejected error={error}");
            }
            FenceEvent::Failed(error) => {
                eprintln!("sos_compositor_fence_failed error={error}");
                cx.quit();
            }
        }
    }

    fn handle_protocol_input(&mut self, input: ProtocolInput, cx: &mut Context<Self>) {
        match input {
            ProtocolInput::Request(request) => self.handle_request(request, cx),
            ProtocolInput::Failed(error) => {
                eprintln!("sos_host_protocol_input_failed error={error}");
                cx.quit();
            }
            ProtocolInput::Closed => {
                eprintln!("sos_host_protocol_input_closed");
                cx.quit();
            }
        }
    }

    fn handle_request(&mut self, request: HostRequest, cx: &mut Context<Self>) {
        match request {
            HostRequest::Boot {
                request_id,
                revision_id,
                ..
            } => reject(request_id, revision_id, "host is already booted"),
            HostRequest::Prepare {
                request_id,
                revision_id,
                revision_path,
                experience_api_version,
            } => {
                if self.preparing.is_some()
                    || self.prepared.is_some()
                    || self.pending_commit.is_some()
                    || self.pending_presentation.is_some()
                    || self.action_in_flight
                {
                    reject(
                        request_id,
                        revision_id,
                        "another revision operation is active",
                    );
                    return;
                }
                let revision =
                    match load_revision(&revision_id, &revision_path, experience_api_version) {
                        Ok(revision) => revision,
                        Err(error) => {
                            reject(request_id, revision_id, error.to_string());
                            return;
                        }
                    };
                let (mut candidate_model, provider_frames) = match &self.provider_access {
                    Some(access) => match access.snapshot(&revision.revision_id) {
                        Ok(snapshot) => (snapshot.model, snapshot.frames),
                        Err(error) => {
                            reject(request_id, revision_id, error.to_string());
                            return;
                        }
                    },
                    None => (self.model.clone(), Vec::new()),
                };
                candidate_model.agent = self.model.agent.clone();
                if let Err(error) = self.worker.prepare_candidate_with_assets(
                    request_id,
                    revision.source.clone(),
                    revision.assets.clone(),
                    candidate_model.clone(),
                    revision.state.clone(),
                    revision.schema_version,
                    Instant::now(),
                ) {
                    reject(request_id, revision_id, error);
                    return;
                }
                self.preparing = Some(PreparingRevision {
                    prepare_request_id: request_id,
                    revision,
                    model: candidate_model,
                    provider_frames,
                });
                self.status = Some(("Preparing Luau revision…".into(), true));
                cx.notify();
            }
            HostRequest::QuiesceInput {
                request_id,
                revision_id,
            } => {
                if self
                    .prepared
                    .as_ref()
                    .map(|prepared| prepared.revision.revision_id.as_str())
                    != Some(revision_id.as_str())
                {
                    reject(request_id, revision_id, "prepared revision does not match");
                    return;
                }
                if self.input_quiesced_revision.as_deref() == Some(&revision_id) {
                    emit(&HostEvent::InputQuiesced {
                        request_id,
                        revision_id,
                    });
                    return;
                }
                if self.input_quiesced_revision.is_some() {
                    reject(
                        request_id,
                        revision_id,
                        "input is quiesced for another revision",
                    );
                    return;
                }
                if let Some(fence) = &self.compositor_fence {
                    if let Err(error) = fence.quiesce_input(request_id, &revision_id) {
                        reject(request_id, revision_id, error.to_string());
                        return;
                    }
                    eprintln!(
                        "sos_compositor_input_quiesced request_id={request_id} revision_id={revision_id}"
                    );
                } else {
                    eprintln!(
                        "sos_host_input_quiesced request_id={request_id} revision_id={revision_id} evidence=host_dispatch_only"
                    );
                }
                let dropped_events = self.pending_input_events.len();
                self.pending_input_events.clear();
                self.surface_gestures.clear();
                self.input_quiesced_revision = Some(revision_id.clone());
                eprintln!(
                    "sos_input_epoch_closed request_id={request_id} revision_id={revision_id} dropped_events={dropped_events}"
                );
                emit(&HostEvent::InputQuiesced {
                    request_id,
                    revision_id,
                });
            }
            HostRequest::Present {
                request_id,
                revision_id,
            } => {
                let Some(prepared) = self.prepared.take() else {
                    reject(request_id, revision_id, "no prepared revision");
                    return;
                };
                if prepared.revision.revision_id != revision_id {
                    self.prepared = Some(prepared);
                    reject(request_id, revision_id, "prepared revision does not match");
                    return;
                }
                if self.input_quiesced_revision.as_deref() != Some(&revision_id) {
                    self.prepared = Some(prepared);
                    reject(
                        request_id,
                        revision_id,
                        "input is not quiesced for revision",
                    );
                    return;
                }
                if let Err(error) = self.worker.commit_candidate(prepared.prepare_request_id) {
                    reject(request_id, revision_id, error);
                    self.prepared = Some(prepared);
                    return;
                }
                self.pending_commit = Some(PendingCommit {
                    present_request_id: request_id,
                    revision: prepared,
                });
                self.status = Some(("Committing prepared Luau VM…".into(), true));
            }
            HostRequest::Confirm {
                request_id,
                revision_id,
            } => {
                if self.last_presented_revision.as_deref() == Some(&revision_id) {
                    emit(&HostEvent::Confirmed {
                        request_id,
                        revision_id,
                    });
                } else {
                    reject(request_id, revision_id, "revision has not been presented");
                }
            }
            HostRequest::Discard {
                request_id,
                revision_id,
            } => {
                let Some(prepared) = self.prepared.take() else {
                    reject(request_id, revision_id, "no prepared revision");
                    return;
                };
                if prepared.revision.revision_id != revision_id {
                    self.prepared = Some(prepared);
                    reject(request_id, revision_id, "prepared revision does not match");
                    return;
                }
                let was_quiesced = self.input_quiesced_revision.as_deref() == Some(&revision_id);
                if was_quiesced {
                    if let Some(fence) = &self.compositor_fence {
                        if let Err(error) = fence.resume_input(request_id, &revision_id) {
                            self.prepared = Some(prepared);
                            reject(request_id, revision_id, error.to_string());
                            return;
                        }
                    }
                    self.input_quiesced_revision = None;
                }
                match self.worker.discard_candidate(prepared.prepare_request_id) {
                    Ok(()) => {
                        self.status = None;
                        emit(&HostEvent::Discarded {
                            request_id,
                            revision_id,
                        });
                    }
                    Err(error) => {
                        self.prepared = Some(prepared);
                        reject(request_id, revision_id, error);
                    }
                }
                cx.notify();
            }
            HostRequest::Shutdown { request_id } => {
                emit(&HostEvent::Shutdown { request_id });
                cx.quit();
            }
        }
    }

    fn handle_worker_result(&mut self, result: WorkerResult, cx: &mut Context<Self>) {
        match result {
            WorkerResult::CandidatePrepared {
                request_id,
                timings,
                ..
            } => {
                let Some(preparing) = self.preparing.take() else {
                    let _ = self.worker.discard_candidate(request_id);
                    return;
                };
                if preparing.prepare_request_id != request_id {
                    self.preparing = Some(preparing);
                    let _ = self.worker.discard_candidate(request_id);
                    return;
                }
                eprintln!(
                    "sos_revision_prepared revision_id={} queue_us={} compile_us={} render_us={} worker_total_us={}",
                    preparing.revision.revision_id,
                    timings.queue_us,
                    timings.compile_us,
                    timings.render_us,
                    timings.worker_total_us
                );
                let revision_id = preparing.revision.revision_id.clone();
                self.prepared = Some(PreparedRevision {
                    prepare_request_id: request_id,
                    revision: preparing.revision,
                    model: preparing.model,
                    provider_frames: preparing.provider_frames,
                });
                self.status = Some((
                    "Revision prepared; accepted scene remains active".into(),
                    true,
                ));
                emit(&HostEvent::Prepared {
                    request_id,
                    revision_id,
                });
            }
            WorkerResult::CandidateRejected {
                request_id, error, ..
            } => {
                let revision_id = self
                    .preparing
                    .take()
                    .filter(|candidate| candidate.prepare_request_id == request_id)
                    .map_or_else(
                        || "unknown".into(),
                        |candidate| candidate.revision.revision_id,
                    );
                self.status = Some((format!("Revision rejected: {error}"), false));
                reject(request_id, revision_id, error);
            }
            WorkerResult::CandidateCommitted {
                request_id,
                scene,
                state,
                state_schema_version,
                timings,
                assets: revision_assets,
                ..
            } => {
                let Some(commit) = self.pending_commit.take() else {
                    return;
                };
                if commit.revision.prepare_request_id != request_id {
                    self.pending_commit = Some(commit);
                    return;
                }
                let revision_id = commit.revision.revision.revision_id.clone();
                self.pending_focus_restore = self.active_input_id.clone();
                if let Some(fence) = &self.compositor_fence {
                    let after_commit_sequence = match fence
                        .arm(commit.present_request_id, &revision_id)
                    {
                        Ok(sequence) => sequence,
                        Err(error) => {
                            eprintln!(
                                "sos_compositor_arm_failed request_id={} revision_id={} error={error:#}",
                                commit.present_request_id, revision_id
                            );
                            cx.quit();
                            return;
                        }
                    };
                    eprintln!(
                        "sos_compositor_armed request_id={} revision_id={} after_commit_sequence={after_commit_sequence}",
                        commit.present_request_id, revision_id
                    );
                }
                assets::install(&revision_assets);
                assets::install_provider_frames(&commit.revision.provider_frames);
                self.scene = scene;
                self.state = state;
                self.state_schema_version = state_schema_version;
                self.active_revision_id = revision_id;
                self.model = commit.revision.model;
                if let Some(access) = &self.provider_access {
                    access.activate(&self.active_revision_id);
                }
                self.active_source_sha256 = commit.revision.revision.source_sha256;
                self.pending_presentation = Some(PendingPresentation {
                    request_id: commit.present_request_id,
                    revision_id: self.active_revision_id.clone(),
                });
                self.status = Some((
                    if self.compositor_fence.is_some() {
                        "Scene switched; waiting for compositor submit…"
                    } else {
                        "Scene switched; waiting for GPUI frame…"
                    }
                    .into(),
                    true,
                ));
                eprintln!(
                    "sos_revision_committed revision_id={} worker_total_us={}",
                    self.active_revision_id, timings.worker_total_us
                );
                cx.notify();
            }
            WorkerResult::ModelRefreshed {
                request_id,
                scene,
                worker_us,
            } => {
                let Some((expected, model)) = self.provider_refresh.take() else {
                    return;
                };
                if expected != request_id {
                    self.provider_refresh = Some((expected, model));
                    return;
                }
                self.model = model;
                self.scene = scene;
                self.status = None;
                eprintln!(
                    "sos_provider_model_refreshed request_id={request_id} worker_us={worker_us} revision_id={}",
                    self.active_revision_id
                );
                self.dispatch_queued_provider_model(cx);
                cx.notify();
            }
            WorkerResult::ModelRefreshRejected {
                request_id,
                error,
                worker_us,
            } => {
                if self
                    .provider_refresh
                    .as_ref()
                    .is_some_and(|(expected, _)| *expected == request_id)
                {
                    self.provider_refresh = None;
                }
                self.status = Some((format!("Provider update rejected: {error}"), false));
                eprintln!(
                    "sos_provider_model_rejected request_id={request_id} worker_us={worker_us} error={error}"
                );
                self.dispatch_queued_provider_model(cx);
                cx.notify();
            }
            WorkerResult::ActionCompleted {
                request_id,
                state,
                scene,
                effects,
                worker_us,
            } => {
                eprintln!(
                    "sos_action_completed request_id={request_id} worker_us={worker_us} effects={} service_socket={}",
                    effects.len(),
                    self.service_socket
                        .as_deref()
                        .map_or("none".into(), |path| path.display().to_string())
                );
                let Some(service_socket) = self.service_socket.clone() else {
                    self.action_in_flight = false;
                    if effects.is_empty() {
                        reconcile_input_state_shadow(&mut self.input_state_shadow, &state);
                        self.state = state;
                        merge_input_state_shadow(&mut self.state, &self.input_state_shadow);
                        self.scene = scene;
                        self.status = None;
                    } else {
                        self.status =
                            Some(("Provider effects require --service-socket".into(), false));
                    }
                    self.dispatch_pending_input_event(cx);
                    cx.notify();
                    return;
                };
                self.status = Some(("Committing state and provider effects…".into(), true));
                let revision_id = self.active_revision_id.clone();
                let source_sha256 = self.active_source_sha256.clone();
                let schema_version = self.state_schema_version;
                let provider_access = self.provider_access.clone();
                let sender = self.action_commits.clone();
                thread::Builder::new()
                    .name("sos-action-commit".into())
                    .spawn(move || {
                        let result = commit_action(
                            &service_socket,
                            request_id,
                            &revision_id,
                            &source_sha256,
                            schema_version,
                            &state,
                            &effects,
                            provider_access.as_ref(),
                        );
                        let _ = sender.send_blocking(ActionCommitResult {
                            request_id,
                            state,
                            scene,
                            result,
                        });
                    })
                    .expect("action commit thread must start");
            }
            WorkerResult::ActionRejected {
                request_id,
                error,
                worker_us,
            } => {
                self.action_in_flight = false;
                self.status = Some((format!("Action rejected: {error}"), false));
                eprintln!(
                    "sos_action_rejected request_id={request_id} worker_us={worker_us} error={error}"
                );
                self.dispatch_pending_input_event(cx);
                cx.notify();
            }
        }
    }

    fn handle_action_commit(&mut self, result: ActionCommitResult, cx: &mut Context<Self>) {
        self.action_in_flight = false;
        match result.result {
            Ok(outcome) => {
                let authoritative = outcome.authoritative;
                if authoritative.state != result.state {
                    self.status = Some((
                        "Authority returned unexpected interaction state".into(),
                        false,
                    ));
                    eprintln!(
                        "sos_action_commit_rejected request_id={} error=state_mismatch",
                        result.request_id
                    );
                } else {
                    reconcile_input_state_shadow(
                        &mut self.input_state_shadow,
                        &authoritative.state,
                    );
                    self.state = authoritative.state;
                    merge_input_state_shadow(&mut self.state, &self.input_state_shadow);
                    self.scene = result.scene;
                    self.status = None;
                    eprintln!(
                        "sos_action_committed request_id={} authority_revision={}",
                        result.request_id, authoritative.revision
                    );
                    if let Some(prompt) = outcome.agent_prompt {
                        self.start_agent_prompt(prompt, cx);
                    }
                }
            }
            Err(error) => {
                self.status = Some((format!("State/effect commit failed: {error}"), false));
                eprintln!(
                    "sos_action_commit_rejected request_id={} error={error}",
                    result.request_id
                );
            }
        }
        self.dispatch_pending_input_event(cx);
        self.dispatch_queued_provider_model(cx);
    }

    fn start_agent_prompt(&mut self, prompt: String, cx: &mut Context<Self>) {
        if self.model.agent.busy {
            self.status = Some(("The agent is already handling a request".into(), false));
            return;
        }
        let Some(socket) = self.agent_socket.clone() else {
            self.handle_agent_update(
                AgentUpdate::Failed("resident Pi agent is not configured".into()),
                cx,
            );
            return;
        };
        self.handle_agent_update(
            AgentUpdate::Started {
                prompt: prompt.clone(),
            },
            cx,
        );
        agent_bridge::spawn_prompt(socket, prompt, self.agent_updates.clone());
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

    pub(super) fn native_input_changed(
        &mut self,
        node_id: String,
        state_key: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if !self.state.is_object() {
            self.state = serde_json::json!({});
        }
        if let Some(object) = self.state.as_object_mut() {
            object.insert(state_key.clone(), JsonValue::String(value.clone()));
        }
        self.input_state_shadow.insert(state_key, value.clone());
        eprintln!(
            "sos_linux_text_changed node_id={} bytes={}",
            node_id,
            value.len()
        );
        self.queue_input_event(
            SceneEvent {
                action: "text_changed".into(),
                target: Some(node_id),
                value: Some(value),
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
        if focused {
            self.active_input_id = Some(node_id.clone());
        } else if self.active_input_id.as_deref() == Some(&node_id) {
            self.active_input_id = None;
        }
        eprintln!("sos_linux_text_focus node_id={node_id} focused={focused}");
        self.queue_input_event(
            SceneEvent {
                action: "focus_changed".into(),
                target: Some(node_id),
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
        eprintln!(
            "sos_linux_text_submitted node_id={} action={} bytes={}",
            node_id,
            action,
            value.len()
        );
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

    fn dispatch_event(&mut self, event: SceneEvent, cx: &mut Context<Self>) {
        if self.action_in_flight
            || self.preparing.is_some()
            || self.prepared.is_some()
            || self.pending_commit.is_some()
            || self.pending_presentation.is_some()
        {
            return;
        }
        let request_id = self.next_action_request_id;
        self.next_action_request_id = self.next_action_request_id.wrapping_add(1).max(1);
        self.action_in_flight = true;
        merge_input_state_shadow(&mut self.state, &self.input_state_shadow);
        eprintln!(
            "sos_action_dispatched request_id={request_id} action={} target={}",
            event.action,
            event.target.as_deref().unwrap_or("none")
        );
        if let Err(error) =
            self.worker
                .action(request_id, self.model.clone(), self.state.clone(), event)
        {
            self.action_in_flight = false;
            self.status = Some((format!("Action could not start: {error}"), false));
            cx.notify();
        }
    }

    fn queue_input_event(&mut self, event: SceneEvent, cx: &mut Context<Self>) {
        if self.action_in_flight {
            self.enqueue_pending_event(event);
        } else if self.preparing.is_none()
            && self.prepared.is_none()
            && self.pending_commit.is_none()
            && self.pending_presentation.is_none()
        {
            self.dispatch_event(event, cx);
        } else {
            eprintln!(
                "sos_input_event_blocked action={} preparing={} prepared={} pending_commit={} pending_presentation={}",
                event.action,
                self.preparing.is_some(),
                self.prepared.is_some(),
                self.pending_commit.is_some(),
                self.pending_presentation.is_some()
            );
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

    fn handle_accessibility_action(
        &mut self,
        action: AccessibilityAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.kind.as_str() {
            "next" | "previous" => {
                let ids = semantic_ids(&self.scene);
                if ids.is_empty() {
                    return;
                }
                let current = self
                    .semantic_focus
                    .as_ref()
                    .and_then(|focused| ids.iter().position(|id| id == focused));
                let index = if action.kind == "next" {
                    current.map_or(0, |index| (index + 1) % ids.len())
                } else {
                    current
                        .and_then(|index| index.checked_sub(1))
                        .unwrap_or(ids.len() - 1)
                };
                self.semantic_focus = Some(ids[index].clone());
                self.status = Some((format!("Accessibility focus: {}", ids[index]), true));
            }
            "focus" => {
                self.semantic_focus = Some(action.target.clone());
                if let Some(input) = self.inputs.get(&action.target).cloned() {
                    window.defer(cx, move |window, cx| {
                        input.update(cx, |input, input_cx| input.activate(window, input_cx));
                    });
                }
            }
            "activate" => {
                if let Some(scene_action) = node_by_id(&self.scene.root, &action.target)
                    .and_then(|node| node.interaction.tap_action.clone())
                {
                    self.queue_input_event(
                        SceneEvent {
                            action: scene_action,
                            target: Some(action.target.clone()),
                            ..Default::default()
                        },
                        cx,
                    );
                }
            }
            "scroll_forward" | "scroll_backward" => self.queue_input_event(
                SceneEvent {
                    action: "accessibility_scroll".into(),
                    target: Some(action.target.clone()),
                    delta_y: Some(if action.kind == "scroll_forward" {
                        1.0
                    } else {
                        -1.0
                    }),
                    phase: Some("update".into()),
                    ..Default::default()
                },
                cx,
            ),
            "set_value" => {
                if let Some(input) = self.inputs.get(&action.target).cloned() {
                    let value = action.value.clone();
                    window.defer(cx, move |window, cx| {
                        input.update(cx, |input, input_cx| {
                            input.accessibility_set_value(&value, window, input_cx)
                        });
                    });
                }
            }
            "set_selection" => {
                let selection = action
                    .value
                    .split_once(':')
                    .and_then(|(start, end)| Some((start.parse().ok()?, end.parse().ok()?)));
                if let (Some((start, end)), Some(input)) =
                    (selection, self.inputs.get(&action.target).cloned())
                {
                    window.defer(cx, move |_, cx| {
                        input.update(cx, |input, input_cx| {
                            input.accessibility_set_selection(start, end, input_cx)
                        });
                    });
                }
            }
            "submit" => {
                if let Some(input) = self.inputs.get(&action.target).cloned() {
                    window.defer(cx, move |_, cx| {
                        input.update(cx, |input, input_cx| input.accessibility_submit(input_cx));
                    });
                }
            }
            "copy" | "cut" | "paste" => {
                if let Some(input) = self.inputs.get(&action.target).cloned() {
                    let kind = action.kind.clone();
                    window.defer(cx, move |window, cx| {
                        input.update(cx, |input, input_cx| {
                            input.accessibility_clipboard_action(&kind, window, input_cx)
                        });
                    });
                }
            }
            _ => return,
        }
        eprintln!(
            "sos_accessibility_action kind={} target={}",
            action.kind, action.target
        );
        cx.notify();
    }

    fn dispatch_pending_input_event(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = self.pending_input_events.pop_front() {
            self.dispatch_event(event, cx);
        }
    }

    fn surface_down(
        &mut self,
        surface_id: String,
        region: HitRegion,
        specification: &Interaction,
        position: (f32, f32),
        platform_click_count: usize,
        cx: &mut Context<Self>,
    ) {
        let (x, y) = position;
        let now = Instant::now();
        let press = region.press_action.clone();
        let pointer = specification.pointer_action.clone();
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
        if let Some(action) = pointer {
            self.queue_input_event(
                pointer_event(
                    action,
                    target.clone(),
                    x,
                    y,
                    "down",
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ),
                cx,
            );
        }
        if let Some(action) = press {
            self.queue_input_event(
                scene_surface::event(action, target, x, y, "start", 0.0, 0.0, 0.0, 0.0),
                cx,
            );
        }
    }

    fn surface_move(
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
        let pointer = specification.pointer_action.clone();
        if let Some(action) = pointer {
            self.queue_input_event(
                pointer_event(
                    action,
                    target.clone(),
                    x,
                    y,
                    "move",
                    delta_x,
                    delta_y,
                    velocity_x,
                    velocity_y,
                    1.0,
                ),
                cx,
            );
        }
        if let Some(action) = action {
            self.queue_input_event(
                scene_surface::event(
                    action, target, x, y, "update", delta_x, delta_y, velocity_x, velocity_y,
                ),
                cx,
            );
        }
    }

    fn surface_up(
        &mut self,
        surface_id: String,
        specification: &Interaction,
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
        if let Some(action) = specification.pointer_action.clone() {
            self.queue_input_event(
                pointer_event(
                    action,
                    region.id.clone(),
                    x,
                    y,
                    "up",
                    delta_x,
                    delta_y,
                    velocity_x,
                    velocity_y,
                    0.0,
                ),
                cx,
            );
        }
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

    fn sync_shell_overlay(&mut self, window: &Window) {
        let Some(Content::ShellOverlay(overlay)) =
            shell_overlay_node(&self.scene.root).and_then(|node| node.content.as_ref())
        else {
            return;
        };
        let viewport = window.viewport_size();
        let output_width = f32::from(viewport.width).floor().max(1.0) as i32;
        let output_height = f32::from(viewport.height).floor().max(1.0) as i32;
        let width = overlay.width.round().clamp(48.0, 720.0) as u32;
        let height = overlay.height.round().clamp(48.0, 360.0) as u32;
        let max_x = (output_width - i32::try_from(width).unwrap_or(i32::MAX)).max(0);
        let max_y = (output_height - i32::try_from(height).unwrap_or(i32::MAX)).max(0);
        let configuration = ShellOverlayConfiguration {
            x: overlay.x.round() as i32,
            y: overlay.y.round() as i32,
            width,
            height,
        };
        let configuration = ShellOverlayConfiguration {
            x: configuration.x.clamp(0, max_x),
            y: configuration.y.clamp(0, max_y),
            ..configuration
        };
        if self.last_shell_overlay == Some(configuration) {
            return;
        }
        let Some(fence) = &self.compositor_fence else {
            self.last_shell_overlay = Some(configuration);
            return;
        };
        match fence.configure_shell_overlay(configuration) {
            Ok(()) => self.last_shell_overlay = Some(configuration),
            Err(error) => eprintln!("sos_shell_overlay_configure_failed error={error}"),
        }
    }

    fn render_node(
        &mut self,
        node: &SceneNode,
        path: SharedString,
        target: RenderTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if target == RenderTarget::Base
            && matches!(
                node.content,
                Some(Content::ShellOverlay(_) | Content::ApplicationSurface(_))
            )
        {
            return div().into_any_element();
        }
        let element_id = node.id.clone().unwrap_or_else(|| path.to_string());
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
            // A growing scene node represents the flexible remainder of its
            // parent. Let it shrink below its contents' intrinsic size so a
            // scrollable child cannot expand the shell past the viewport.
            element = element.flex_1().min_w_0().min_h_0();
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
        if let Some(Content::TextSession(input)) = &node.content {
            let displayed_value = self
                .input_state_shadow
                .get(&input.state_key)
                .cloned()
                .unwrap_or_else(|| input.value.clone());
            let created = !self.inputs.contains_key(&element_id);
            let native = if let Some(native) = self.inputs.get(&element_id) {
                native.clone()
            } else {
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
            if self.pending_focus_restore.as_deref() == Some(element_id.as_str()) {
                self.pending_focus_restore = None;
            }
            element = element
                .border_1()
                .border_color(rgb(0x98A29B))
                .p(px(8.))
                .child(native);
        }
        if let Some(Content::Image(image)) = &node.content {
            let path = if image.asset == "album-orbit" {
                ALBUM_ASSET.to_owned()
            } else {
                image.asset.clone()
            };
            element = element.child(img(path).size_full());
        }
        if let Some(Content::ProviderSurface(surface)) = &node.content {
            if let Some(path) = assets::provider_surface_path(&surface.surface) {
                element = element.child(img(path).size_full());
            } else {
                let protected = self
                    .model
                    .surfaces
                    .iter()
                    .find(|candidate| candidate.id == surface.surface)
                    .is_some_and(|candidate| candidate.protected);
                element = element.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x171A20))
                        .text_color(rgb(0xD5D9E0))
                        .child(if protected {
                            "Protected surface unavailable: no secure Linux scanout path"
                        } else {
                            "Provider surface unavailable"
                        }),
                );
            }
        }
        if let Some(Content::WindowSpace(space)) = &node.content {
            element = element.relative();
            if !space.fallback.is_empty() {
                element = element.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0x737A75))
                        .text_size(px(14.))
                        .child(SharedString::from(space.fallback.clone())),
                );
            }
            element = element.child(window_space::render(
                element_id.clone(),
                space.clone(),
                cx.weak_entity(),
            ));
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
            element = element.child(self.render_node(child, child_path, target, window, cx));
        }
        let surface_owns_tap = uses_surface && node.interaction.tap_action.is_some();
        let mut rendered = if let Some(action) = node
            .interaction
            .tap_action
            .as_ref()
            .filter(|_| !surface_owns_tap && !node.interaction.surface_drag)
        {
            let action = action.clone();
            element
                .id(SharedString::from(element_id.clone()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.dispatch(action.clone(), cx)),
                )
                .into_any_element()
        } else if node.layout.scroll_y {
            element
                .id(SharedString::from(element_id.clone()))
                .overflow_y_scroll()
                .into_any_element()
        } else {
            element.into_any_element()
        };
        if node.interaction.surface_drag {
            rendered = div()
                .id(SharedString::from(format!("surface-drag-{element_id}")))
                .size_full()
                .child(rendered)
                .on_mouse_down(MouseButton::Left, move |_, window, app| {
                    window.prevent_default();
                    app.stop_propagation();
                    window.start_window_move();
                })
                .into_any_element();
        }
        if target == RenderTarget::Base {
            if let Some(action) = &node.interaction.hover_action {
                let action = action.clone();
                let event_target = element_id.clone();
                rendered = div()
                    .id(SharedString::from(format!("hover-{element_id}")))
                    .size_full()
                    .child(rendered)
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        this.queue_input_event(
                            SceneEvent {
                                action: action.clone(),
                                target: Some(event_target.clone()),
                                focused: Some(*hovered),
                                ..Default::default()
                            },
                            cx,
                        );
                    }))
                    .into_any_element();
            }
        }
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

impl window_space::WindowSpaceHost for LinuxExperienceHost {
    fn record_window_space(
        &mut self,
        node_id: String,
        bounds: Bounds<gpui::Pixels>,
        specification: WindowSpaceContent,
        _cx: &mut Context<Self>,
    ) {
        let x = f32::from(bounds.origin.x);
        let y = f32::from(bounds.origin.y);
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        if [x, y, width, height]
            .into_iter()
            .any(|value| !value.is_finite())
            || width < 160.0
            || height < 120.0
        {
            return;
        }
        let left = x.ceil().max(0.0);
        let top = y.ceil().max(0.0);
        let right = (x + width).floor().max(left);
        let bottom = (y + height).floor().max(top);
        let configuration = WindowSpaceConfiguration {
            geometry: WindowSpaceGeometry {
                x: left as i32,
                y: top as i32,
                width: (right - left) as u32,
                height: (bottom - top) as u32,
                gap: specification.gap.round().clamp(0.0, 128.0) as u32,
            },
            layout: match specification.layout {
                WindowLayoutMode::Floating => CompositorWindowLayoutMode::Floating,
                WindowLayoutMode::Tiling => CompositorWindowLayoutMode::Tiling,
                WindowLayoutMode::Scrolling => CompositorWindowLayoutMode::Scrolling,
            },
        };
        if configuration.geometry.width < 160 || configuration.geometry.height < 120 {
            return;
        }
        if self.last_window_space == Some(configuration) {
            return;
        }
        let Some(fence) = &self.compositor_fence else {
            return;
        };
        match fence.configure_window_space(configuration) {
            Ok(()) => {
                self.last_window_space = Some(configuration);
                eprintln!(
                    "sos_window_space node_id={node_id} x={} y={} width={} height={} gap={} layout={:?}",
                    configuration.geometry.x,
                    configuration.geometry.y,
                    configuration.geometry.width,
                    configuration.geometry.height,
                    configuration.geometry.gap,
                    configuration.layout
                );
            }
            Err(error) => {
                eprintln!("sos_window_space_failed node_id={node_id} error={error}");
            }
        }
    }
}

impl scene_surface::SceneSurfaceHost for LinuxExperienceHost {
    fn enables_pointer_fallback() -> bool {
        true
    }

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
        specification: &Interaction,
        position: (f32, f32),
        platform_click_count: usize,
        cx: &mut Context<Self>,
    ) {
        self.surface_down(
            surface_id,
            region,
            specification,
            position,
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
        self.surface_move(surface_id, specification, x, y, cx);
    }

    fn scene_surface_up(
        &mut self,
        surface_id: String,
        specification: &Interaction,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.surface_up(surface_id, specification, x, y, cx);
    }
}

impl Render for LinuxExperienceHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(accessibility) = self.accessibility.clone() {
            while let Some(action) = self.accessibility_actions.pop_front() {
                self.handle_accessibility_action(action, window, cx);
            }
            accessibility.publish(
                &self.scene,
                self.semantic_focus.clone(),
                self.status.as_ref().map(|status| status.0.clone()),
            );
        }
        for sample in pointer_input::take_samples() {
            if sample.phase == pointer_input::Phase::Down {
                if let Some(node_id) = pointer_input::native_input_at(sample.x, sample.y) {
                    if let Some(input) = self.inputs.get(&node_id).cloned() {
                        let position = point(px(sample.x), px(sample.y));
                        eprintln!(
                            "sos_linux_touch_focus node_id={node_id} x={:.1} y={:.1}",
                            sample.x, sample.y
                        );
                        window.defer(cx, move |window, cx| {
                            input.update(cx, |input, input_cx| {
                                input.focus_at(position, window, input_cx)
                            });
                        });
                    }
                }
            }
            for event in pointer_input::route(sample) {
                self.queue_input_event(event, cx);
            }
        }
        pointer_input::begin_frame();
        assets::install_fonts(window);
        let scene = self.scene.clone();
        self.sync_shell_overlay(window);
        let content = self.render_node(
            &scene.root,
            SharedString::from("root"),
            RenderTarget::Base,
            window,
            cx,
        );
        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0xF3F1E8))
            .child(content);
        if let Some((message, accepted)) = &self.status {
            root = root.child(
                div()
                    .absolute()
                    .left(px(14.))
                    .right(px(14.))
                    .bottom(px(12.))
                    .p(px(10.))
                    .rounded(px(12.))
                    .bg(rgb(if *accepted { 0x2F684B } else { 0x8C3A36 }))
                    .text_color(rgb(0xFFFFFF))
                    .text_size(px(12.))
                    .child(SharedString::from(message.clone())),
            );
        }
        if self.compositor_fence.is_none() {
            if let Some(presentation) = self.pending_presentation.take() {
                cx.on_next_frame(window, move |this, _, cx| {
                    this.last_presented_revision = Some(presentation.revision_id.clone());
                    this.status = None;
                    eprintln!(
                        "sos_revision_frame revision_id={} evidence=gpui_next_frame",
                        presentation.revision_id
                    );
                    emit(&HostEvent::Presented {
                        request_id: presentation.request_id,
                        revision_id: presentation.revision_id,
                    });
                    this.dispatch_queued_provider_model(cx);
                    cx.notify();
                });
            }
        }
        root
    }
}

fn node_by_id<'a>(node: &'a SceneNode, id: &str) -> Option<&'a SceneNode> {
    if node.id.as_deref() == Some(id) {
        return Some(node);
    }
    node.children.iter().find_map(|child| node_by_id(child, id))
}

fn shell_overlay_node(node: &SceneNode) -> Option<&SceneNode> {
    if matches!(node.content, Some(Content::ShellOverlay(_))) {
        return Some(node);
    }
    node.children.iter().find_map(shell_overlay_node)
}

fn application_surface_node(node: &SceneNode) -> Option<&SceneNode> {
    if matches!(node.content, Some(Content::ApplicationSurface(_))) {
        return Some(node);
    }
    node.children.iter().find_map(application_surface_node)
}

fn merge_input_state_shadow(state: &mut JsonValue, shadow: &HashMap<String, String>) {
    if !state.is_object() {
        *state = serde_json::json!({});
    }
    if let Some(object) = state.as_object_mut() {
        for (key, value) in shadow {
            object.insert(key.clone(), JsonValue::String(value.clone()));
        }
    }
}

fn reconcile_input_state_shadow(shadow: &mut HashMap<String, String>, authoritative: &JsonValue) {
    shadow.retain(|key, value| {
        authoritative.get(key).and_then(JsonValue::as_str) != Some(value.as_str())
    });
}

fn semantic_ids(scene: &Scene) -> Vec<String> {
    fn visit(node: &SceneNode, output: &mut Vec<String>) {
        if let Some(id) = node
            .id
            .as_ref()
            .filter(|_| node.semantics.is_some() || node.layout.scroll_y)
        {
            output.push(id.clone());
        }
        for child in &node.children {
            visit(child, output);
        }
    }
    let mut ids = Vec::new();
    visit(&scene.root, &mut ids);
    ids
}

fn start_provider_updates(
    revision_id: &str,
) -> Result<(
    ExperienceModel,
    async_channel::Receiver<ProviderUpdate>,
    Option<LinuxProviderAccess>,
)> {
    let (sender, receiver) = async_channel::bounded(1);
    let providers_disabled = ["SOS_SAFE_MODE_FILE", "SOS_PROVIDER_DISABLE_FILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .any(|path| path.exists());
    if providers_disabled {
        eprintln!("sos_provider_safe_mode synthetic_model=true");
        drop(sender);
        return Ok((providers_fake::snapshot(), receiver, None));
    }
    let Some(root) = std::env::var_os("SOS_LINUX_PROVIDER_ROOT").map(PathBuf::from) else {
        drop(sender);
        return Ok((providers_fake::snapshot(), receiver, None));
    };
    let hub = ProviderHub::open(&root)
        .with_context(|| format!("open Linux provider root {}", root.display()))?;
    let grant_path = std::env::var_os("SOS_PROVIDER_GRANTS")
        .map(PathBuf::from)
        .context("SOS_LINUX_PROVIDER_ROOT requires SOS_PROVIDER_GRANTS")?;
    let access = LinuxProviderAccess {
        hub,
        grant_path,
        allow_development_wildcard: std::env::var_os("SOS_PROVIDER_DEVELOPMENT_GRANTS").as_deref()
            == Some(std::ffi::OsStr::new("1")),
        active_revision: Arc::new(Mutex::new(revision_id.into())),
    };
    // Fingerprint before taking the initial snapshot. Providers such as PipeWire
    // can become ready while the first model is being collected; recording the
    // generation afterwards would make that newer state the watcher baseline
    // while leaving the UI stuck with the older snapshot.
    let hub = access.hub.clone();
    let generation = hub.generation().context("fingerprint Linux providers")?;
    let snapshot = access.snapshot(revision_id)?;
    assets::install_provider_frames(&snapshot.frames);
    let model = snapshot.model;
    let watcher_access = access.clone();
    thread::Builder::new()
        .name("sos-provider-events".into())
        .spawn(move || {
            let mut generation = generation;
            let mut revision_id = watcher_access.active_revision();
            while !sender.is_closed() {
                thread::sleep(Duration::from_secs(1));
                let next_revision_id = watcher_access.active_revision();
                let next = match hub.generation() {
                    Ok(next) if next != generation || next_revision_id != revision_id => next,
                    Ok(_) => continue,
                    Err(error) => {
                        eprintln!(
                            "sos_provider_unavailable revision_id={next_revision_id} error={error}"
                        );
                        continue;
                    }
                };
                match watcher_access.snapshot(&next_revision_id) {
                    Ok(snapshot) => {
                        generation = next.clone();
                        revision_id = next_revision_id.clone();
                        if sender
                            .send_blocking(ProviderUpdate {
                                generation: format!("{revision_id}:{next}"),
                                revision_id: revision_id.clone(),
                                model: snapshot.model,
                                frames: snapshot.frames,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => eprintln!(
                        "sos_provider_unavailable revision_id={next_revision_id} error={error}"
                    ),
                }
            }
        })
        .context("start Linux provider subscription thread")?;
    Ok((model, receiver, Some(access)))
}

fn parse_options(args: impl Iterator<Item = String>) -> Result<Options> {
    let mut service_socket = None;
    let mut agent_socket = None;
    let mut windowed = false;
    let mut args = args.peekable();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--service-socket" => {
                service_socket = Some(PathBuf::from(
                    args.next().context("--service-socket requires a path")?,
                ));
            }
            "--agent-socket" => {
                agent_socket = Some(PathBuf::from(
                    args.next().context("--agent-socket requires a path")?,
                ));
            }
            "--windowed" => windowed = true,
            other => bail!("unknown option: {other}"),
        }
    }
    Ok(Options {
        service_socket,
        agent_socket,
        windowed,
    })
}

fn read_first_request() -> Result<(HostRequest, BufReader<io::Stdin>)> {
    let mut reader = BufReader::new(io::stdin());
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        bail!("supervisor closed stdin before boot");
    }
    let request = serde_json::from_str(&line).context("decode initial host request")?;
    Ok((request, reader))
}

fn spawn_protocol_reader(
    reader: BufReader<io::Stdin>,
    sender: async_channel::Sender<ProtocolInput>,
) -> Result<()> {
    thread::Builder::new()
        .name("sos-host-protocol".into())
        .spawn(move || {
            for line in reader.lines() {
                let input = match line {
                    Ok(line) => match serde_json::from_str(&line) {
                        Ok(request) => ProtocolInput::Request(request),
                        Err(error) => ProtocolInput::Failed(error.to_string()),
                    },
                    Err(error) => ProtocolInput::Failed(error.to_string()),
                };
                let failed = matches!(input, ProtocolInput::Failed(_));
                if sender.send_blocking(input).is_err() || failed {
                    return;
                }
            }
            let _ = sender.send_blocking(ProtocolInput::Closed);
        })?;
    Ok(())
}

fn load_revision(
    expected_revision_id: &str,
    directory: &Path,
    requested_api_version: u32,
) -> Result<LoadedRevision> {
    if requested_api_version != EXPERIENCE_API_VERSION {
        bail!("unsupported experience API version {requested_api_version}");
    }
    let manifest: RevisionManifest = serde_json::from_slice(
        &fs::read(directory.join("manifest.json")).context("read revision manifest")?,
    )
    .context("decode revision manifest")?;
    if manifest.revision_id != expected_revision_id {
        bail!("revision manifest identity does not match request");
    }
    if manifest.format_version != 3 {
        bail!(
            "unsupported revision manifest format {}",
            manifest.format_version
        );
    }
    if manifest.experience_api_version != requested_api_version {
        bail!("revision manifest experience API does not match request");
    }
    let source = read_verified_file(directory, &manifest.source)?;
    let state_bytes = read_verified_file(directory, &manifest.state)?;
    let state: DurableState =
        serde_json::from_slice(&state_bytes).context("decode durable revision state")?;
    if state.schema_version != manifest.schema_version
        || state.source_sha256 != manifest.source.sha256
    {
        bail!("source, state, and schema do not describe one revision");
    }
    let source = String::from_utf8(source).context("revision source is not UTF-8")?;
    let assets = load_revision_assets(directory)
        .map_err(|error| anyhow::anyhow!("load revision sidecars: {error}"))?;
    Ok(LoadedRevision {
        revision_id: expected_revision_id.into(),
        source,
        source_sha256: manifest.source.sha256,
        state: state.state,
        schema_version: state.schema_version,
        assets,
    })
}

fn read_verified_file(directory: &Path, identity: &FileIdentity) -> Result<Vec<u8>> {
    let relative = Path::new(&identity.path);
    if relative.components().count() != 1 {
        bail!("invalid revision file path: {}", identity.path);
    }
    let bytes = fs::read(directory.join(relative))
        .with_context(|| format!("read revision file {}", identity.path))?;
    if bytes.len() as u64 != identity.size
        || format!("{:x}", Sha256::digest(&bytes)) != identity.sha256
    {
        bail!("revision file identity mismatch: {}", identity.path);
    }
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn commit_action(
    socket: &Path,
    request_id: u64,
    revision_id: &str,
    source_sha256: &str,
    schema_version: u64,
    state: &JsonValue,
    effects: &[experience_ir::ProviderEffect],
    provider_access: Option<&LinuxProviderAccess>,
) -> std::result::Result<ActionCommitOutcome, String> {
    let client = ServiceClient::new(socket, Duration::from_secs(2));
    let current = get_service_state(&client, 1)?;
    if current.revision_id != revision_id
        || current.source_sha256 != source_sha256
        || current.schema_version != schema_version
    {
        return Err("authority does not match the active experience revision".into());
    }
    let mut actions = Vec::new();
    let mut linux_effects = Vec::new();
    let mut agent_prompt = None;
    for effect in effects {
        if effect.provider == "agent" && effect.action == "prompt" {
            if agent_prompt.is_some() {
                return Err("one interaction may emit only one agent.prompt effect".into());
            }
            let prompt = effect
                .payload
                .get("prompt")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|prompt| !prompt.is_empty())
                .ok_or_else(|| "agent.prompt omitted a non-empty prompt".to_owned())?;
            if prompt.len() > MAX_AGENT_MESSAGE_BYTES {
                return Err(format!(
                    "agent.prompt exceeds the {MAX_AGENT_MESSAGE_BYTES}-byte limit"
                ));
            }
            agent_prompt = Some(prompt.to_owned());
            continue;
        }
        match provider_action(effect)? {
            Some(action) => actions.push(action),
            None => linux_effects.push(effect.clone()),
        }
    }
    if !linux_effects.is_empty() && provider_access.is_none() {
        return Err("Linux provider effect requires configured provider access".into());
    }
    let transaction_id = format!("host-{}-{request_id}", std::process::id());
    let draft = PromotionDraft {
        transaction_id: transaction_id.clone(),
        expected_revision: current.revision,
        revision_id: revision_id.into(),
        schema_version,
        source_sha256: source_sha256.into(),
        state: state.clone(),
        migration: None,
        actions,
    };
    expect_transaction(call_service(
        &client,
        &ServiceRequest::StagePromotion {
            request_id: 2,
            draft,
        },
    )?)?;

    if let Some(access) = provider_access {
        if let Err(error) = access.execute_effects(revision_id, &linux_effects) {
            let _ = call_service(
                &client,
                &ServiceRequest::Abort {
                    request_id: 6,
                    transaction_id: transaction_id.clone(),
                },
            );
            return Err(format!(
                "Linux provider effect failed before promotion: {error}"
            ));
        }
    }

    let promoted = call_service(
        &client,
        &ServiceRequest::Promote {
            request_id: 3,
            transaction_id: transaction_id.clone(),
        },
    )
    .and_then(expect_transaction);
    if let Err(promote_error) = promoted {
        let record = call_service(
            &client,
            &ServiceRequest::GetTransaction {
                request_id: 4,
                transaction_id,
            },
        )
        .and_then(expect_transaction)
        .map_err(|reconcile_error| {
            format!("promotion failed ({promote_error}); reconciliation failed ({reconcile_error})")
        })?;
        if record.status != TransactionStatus::Committed {
            return Err(format!(
                "promotion was not committed after ambiguous response: {:?}",
                record.status
            ));
        }
    }
    Ok(ActionCommitOutcome {
        authoritative: get_service_state(&client, 5)?,
        agent_prompt,
    })
}

fn truncate_agent_text(mut text: String) -> String {
    if text.len() <= MAX_AGENT_MESSAGE_BYTES {
        return text;
    }
    let mut boundary = MAX_AGENT_MESSAGE_BYTES;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text
}

fn push_agent_message(model: &mut ExperienceModel, role: AgentMessageRole, text: String) {
    let text = truncate_agent_text(text);
    if text.is_empty() {
        return;
    }
    model.agent.messages.push(AgentMessage { role, text });
    if model.agent.messages.len() > MAX_AGENT_MESSAGES {
        let excess = model.agent.messages.len() - MAX_AGENT_MESSAGES;
        model.agent.messages.drain(..excess);
    }
}

fn append_assistant_delta(model: &mut ExperienceModel, delta: &str) {
    if delta.is_empty() {
        return;
    }
    if let Some(message) = model
        .agent
        .messages
        .last_mut()
        .filter(|message| message.role == AgentMessageRole::Assistant)
    {
        message.text.push_str(delta);
        message.text = truncate_agent_text(std::mem::take(&mut message.text));
    } else {
        push_agent_message(model, AgentMessageRole::Assistant, delta.to_owned());
    }
}

fn display_agent_tool(name: &str) -> &str {
    match name {
        "get_experience_context" => "experience context",
        "validate_experience" => "experience validator",
        "submit_experience" => "experience installer",
        _ => "agent tool",
    }
}

fn get_service_state(
    client: &ServiceClient,
    request_id: u64,
) -> std::result::Result<StateResource, String> {
    match call_service(
        client,
        &ServiceRequest::GetResource {
            request_id,
            query: ResourceQuery::ExperienceState,
        },
    )? {
        ResponsePayload::Resource {
            value: ResourceValue::ExperienceState(state),
        } => Ok(state),
        _ => Err("provider service returned the wrong resource payload".into()),
    }
}

fn call_service(
    client: &ServiceClient,
    request: &ServiceRequest,
) -> std::result::Result<ResponsePayload, String> {
    let response = client.call(request).map_err(|error| error.to_string())?;
    if !response.ok {
        return Err(service_error(response.error));
    }
    response
        .payload
        .ok_or_else(|| "provider service response omitted its payload".into())
}

fn service_error(error: Option<ServiceError>) -> String {
    error.map_or_else(
        || "provider service rejected request".into(),
        |error| format!("{error:?}"),
    )
}

fn expect_transaction(
    payload: ResponsePayload,
) -> std::result::Result<service_protocol::TransactionRecord, String> {
    match payload {
        ResponsePayload::Transaction { record } => Ok(record),
        _ => Err("provider service returned the wrong transaction payload".into()),
    }
}

fn provider_action(
    effect: &experience_ir::ProviderEffect,
) -> std::result::Result<Option<ProviderAction>, String> {
    match (effect.provider.as_str(), effect.action.as_str()) {
        ("notes", "attach_to_event") => {
            let note_id = effect
                .payload
                .get("note_id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "notes.attach_to_event omitted note_id".to_owned())?;
            let event_title = effect
                .payload
                .get("event_title")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "notes.attach_to_event omitted event_title".to_owned())?;
            Ok(Some(ProviderAction::Notes(NotesAction::AttachToEvent {
                note_id: note_id.into(),
                event_title: event_title.into(),
            })))
        }
        ("notes", "write")
        | ("calendar", "append")
        | ("music", "command")
        | ("audio", "set_volume")
        | ("audio", "adjust_volume")
        | ("audio", "set_muted")
        | ("media", "play_pause")
        | ("media", "next")
        | ("media", "previous")
        | ("network", "connect")
        | ("network", "disconnect")
        | ("apps", "launch")
        | ("attention", "acknowledge") => Ok(None),
        (provider, action) => Err(format!("unsupported provider effect: {provider}.{action}")),
    }
}

#[allow(clippy::too_many_arguments)]
fn pointer_event(
    action: String,
    target: String,
    x: f32,
    y: f32,
    phase: &str,
    delta_x: f32,
    delta_y: f32,
    velocity_x: f32,
    velocity_y: f32,
    pressure: f32,
) -> SceneEvent {
    SceneEvent {
        pointer_id: Some(0),
        pointer_count: Some(1),
        pressure: Some(pressure),
        ..scene_surface::event(
            action, target, x, y, phase, delta_x, delta_y, velocity_x, velocity_y,
        )
    }
}

fn reject(request_id: u64, revision_id: String, error: impl Into<String>) {
    emit(&HostEvent::Rejected {
        request_id,
        revision_id,
        error: error.into(),
    });
}

fn emit(event: &HostEvent) {
    let mut output = io::stdout().lock();
    if serde_json::to_writer(&mut output, event)
        .and_then(|()| {
            output.write_all(b"\n").map_err(serde_json::Error::io)?;
            output.flush().map_err(serde_json::Error::io)
        })
        .is_err()
    {
        eprintln!("sos_host_protocol_output_failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_service(
        socket: PathBuf,
        state_file: PathBuf,
    ) -> thread::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let handle = thread::spawn({
            let socket = socket.clone();
            move || provider_state_service::serve(&socket, &state_file)
        });
        for _ in 0..200 {
            if socket.exists() {
                return handle;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("provider service did not create its socket");
    }

    #[test]
    fn host_options_are_explicit() {
        let options = parse_options(
            [
                "--windowed",
                "--service-socket",
                "/run/sos/provider.sock",
                "--agent-socket",
                "/run/sos-agent/agent.sock",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert!(options.windowed);
        assert_eq!(
            options.service_socket,
            Some(PathBuf::from("/run/sos/provider.sock"))
        );
        assert_eq!(
            options.agent_socket,
            Some(PathBuf::from("/run/sos-agent/agent.sock"))
        );
        assert!(parse_options(["--unknown".into()].into_iter()).is_err());
    }

    #[test]
    fn newer_native_text_survives_an_older_authority_result_until_caught_up() {
        let mut shadow = HashMap::from([("draft".into(), "newest".into())]);
        let mut local = serde_json::json!({"draft": "older"});

        reconcile_input_state_shadow(&mut shadow, &local);
        assert_eq!(shadow.get("draft").map(String::as_str), Some("newest"));
        merge_input_state_shadow(&mut local, &shadow);
        assert_eq!(local["draft"], "newest");

        reconcile_input_state_shadow(&mut shadow, &serde_json::json!({"draft": "newest"}));
        assert!(shadow.is_empty());
    }

    #[test]
    fn provider_effects_remain_typed_at_the_linux_boundary() {
        let effect = experience_ir::ProviderEffect {
            provider: "notes".into(),
            action: "attach_to_event".into(),
            payload: serde_json::json!({
                "note_id": "note-1",
                "event_title": "Design review"
            }),
        };
        assert_eq!(
            provider_action(&effect).unwrap(),
            Some(ProviderAction::Notes(NotesAction::AttachToEvent {
                note_id: "note-1".into(),
                event_title: "Design review".into(),
            }))
        );
        assert_eq!(
            provider_action(&experience_ir::ProviderEffect {
                provider: "notes".into(),
                action: "write".into(),
                payload: JsonValue::Null,
            })
            .unwrap(),
            None
        );
        for (provider, action) in [
            ("audio", "set_volume"),
            ("audio", "adjust_volume"),
            ("audio", "set_muted"),
            ("media", "play_pause"),
            ("media", "next"),
            ("media", "previous"),
            ("network", "connect"),
            ("network", "disconnect"),
            ("apps", "launch"),
            ("attention", "acknowledge"),
        ] {
            assert_eq!(
                provider_action(&experience_ir::ProviderEffect {
                    provider: provider.into(),
                    action: action.into(),
                    payload: JsonValue::Null,
                })
                .unwrap(),
                None,
                "{provider}.{action} must stay inside the Linux provider boundary"
            );
        }
        assert!(provider_action(&experience_ir::ProviderEffect {
            provider: "shell".into(),
            action: "exec".into(),
            payload: JsonValue::Null,
        })
        .is_err());
    }

    #[test]
    fn mouse_fallback_emits_the_v3_single_pointer_shape() {
        let event = pointer_event(
            "point".into(),
            "surface".into(),
            12.0,
            18.0,
            "move",
            2.0,
            3.0,
            20.0,
            30.0,
            1.0,
        );
        assert_eq!(event.pointer_id, Some(0));
        assert_eq!(event.pointer_count, Some(1));
        assert_eq!(event.pressure, Some(1.0));
        assert_eq!(event.phase.as_deref(), Some("move"));
    }

    #[test]
    fn revision_loader_carries_verified_v3_sidecars_into_the_worker_boundary() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path();
        let revision_id = "c".repeat(64);
        let source = br#"return {
            api_version = 3,
            render = function()
                return { id = "root", content = { kind = "image", asset = "hero" } }
            end,
        }"#;
        let source_sha256 = format!("{:x}", Sha256::digest(source));
        let state = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "source_sha256": source_sha256,
            "state": {},
        }))
        .unwrap();
        let image = b"\x89PNG\r\n\x1a\nlinux-host-fixture";
        fs::create_dir(directory.join("assets")).unwrap();
        fs::write(directory.join("source.luau"), source).unwrap();
        fs::write(directory.join("state.json"), &state).unwrap();
        fs::write(directory.join("assets/hero.png"), image).unwrap();
        let manifest = serde_json::json!({
            "format_version": 3,
            "revision_id": revision_id,
            "schema_version": 1,
            "experience_api_version": 3,
            "source": {
                "path": "source.luau",
                "size": source.len(),
                "sha256": format!("{:x}", Sha256::digest(source)),
            },
            "state": {
                "path": "state.json",
                "size": state.len(),
                "sha256": format!("{:x}", Sha256::digest(&state)),
            },
            "assets": [{
                "id": "hero",
                "kind": "png",
                "file": {
                    "path": "assets/hero.png",
                    "size": image.len(),
                    "sha256": format!("{:x}", Sha256::digest(image)),
                },
            }],
        });
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        let loaded = load_revision(&revision_id, directory, 3).unwrap();
        assert_eq!(loaded.assets.len(), 1);
        assert_eq!(loaded.assets[0].id, "hero");
        let (worker, ready) = RuntimeWorker::start_with_assets(
            loaded.source,
            providers_fake::snapshot(),
            loaded.state,
            loaded.schema_version,
            loaded.assets,
        )
        .unwrap();
        assert_eq!(ready.assets[0].id, "hero");
        worker.shutdown().unwrap();
    }

    #[test]
    fn provider_effect_commits_over_socket_and_survives_service_restart() {
        use std::os::unix::fs::PermissionsExt as _;

        let temporary = tempfile::tempdir().unwrap();
        let state_file = temporary.path().join("authority.json");
        let socket = temporary.path().join("provider.sock");
        let revision_id = "b".repeat(64);
        let source_sha256 = "a".repeat(64);
        let grant_path = temporary.path().join("grants.json");
        fs::write(
            &grant_path,
            serde_json::to_vec(&serde_json::json!({
                revision_id.clone(): ["notes_write"]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::set_permissions(&grant_path, fs::Permissions::from_mode(0o600)).unwrap();
        let provider_access = LinuxProviderAccess {
            hub: ProviderHub::open(temporary.path().join("linux-providers")).unwrap(),
            grant_path,
            allow_development_wildcard: false,
            active_revision: Arc::new(Mutex::new(revision_id.clone())),
        };
        let mut authority = provider_state_service::Authority::open(&state_file).unwrap();
        authority
            .stage(PromotionDraft {
                transaction_id: "linux-host-bootstrap".into(),
                expected_revision: 0,
                revision_id: revision_id.clone(),
                schema_version: 1,
                source_sha256: source_sha256.clone(),
                state: serde_json::json!({}),
                migration: None,
                actions: Vec::new(),
            })
            .unwrap();
        authority.promote("linux-host-bootstrap").unwrap();
        drop(authority);

        let service = start_service(socket.clone(), state_file.clone());
        let state = serde_json::json!({"attached": true});
        let committed = commit_action(
            &socket,
            41,
            &revision_id,
            &source_sha256,
            1,
            &state,
            &[
                experience_ir::ProviderEffect {
                    provider: "notes".into(),
                    action: "attach_to_event".into(),
                    payload: serde_json::json!({
                        "note_id": "note-1",
                        "event_title": "Design review"
                    }),
                },
                experience_ir::ProviderEffect {
                    provider: "notes".into(),
                    action: "write".into(),
                    payload: serde_json::json!({
                        "name": "from-generated-action",
                        "content": "# Generated action\nCapability checked."
                    }),
                },
                experience_ir::ProviderEffect {
                    provider: "agent".into(),
                    action: "prompt".into(),
                    payload: serde_json::json!({"prompt": "Make the day quieter"}),
                },
            ],
            Some(&provider_access),
        )
        .unwrap();
        assert_eq!(committed.authoritative.revision, 2);
        assert_eq!(committed.authoritative.state, state);
        assert_eq!(
            committed.agent_prompt.as_deref(),
            Some("Make the day quieter")
        );
        assert!(temporary
            .path()
            .join("linux-providers/notes/from-generated-action.md")
            .exists());
        let client = ServiceClient::new(&socket, Duration::from_secs(1));
        client
            .call(&ServiceRequest::Shutdown { request_id: 99 })
            .unwrap();
        service.join().unwrap().unwrap();

        let restarted = start_service(socket.clone(), state_file.clone());
        let client = ServiceClient::new(&socket, Duration::from_secs(1));
        let notes = call_service(
            &client,
            &ServiceRequest::GetResource {
                request_id: 100,
                query: ResourceQuery::Notes,
            },
        )
        .unwrap();
        assert!(matches!(
            notes,
            ResponsePayload::Resource {
                value: ResourceValue::Notes(resource)
            } if resource.attachments.get("note-1").map(String::as_str) == Some("Design review")
        ));
        client
            .call(&ServiceRequest::Shutdown { request_id: 101 })
            .unwrap();
        restarted.join().unwrap().unwrap();
    }
}
