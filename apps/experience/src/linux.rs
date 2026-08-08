use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context as _, Result};
use experience_host_protocol::{HostEvent, HostRequest};
use experience_ir::{
    Align, AnimationKind, Content, ExperienceModel, Flow, HitRegion, Interaction, Justify, PaintOp,
    Scene, SceneEvent, SceneNode, EXPERIENCE_API_VERSION,
};
use gpui::{
    div, img, prelude::*, px, relative, rgb, size, Animation as GpuiAnimation, AnimationExt as _,
    AnyElement, App, Bounds, Context, MouseButton, Render, SharedString, Window, WindowBounds,
    WindowOptions,
};
use provider_state_service::ServiceClient;
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

use crate::assets::{self, SosAssets, ALBUM_ASSET};
use crate::compositor_fence::{CompositorFence, FenceEvent};
use crate::scene_surface;

#[derive(Clone, Debug)]
struct Options {
    service_socket: Option<PathBuf>,
    windowed: bool,
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
}

#[derive(Clone, Debug)]
struct PreparedRevision {
    prepare_request_id: u64,
    revision: LoadedRevision,
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
    result: std::result::Result<StateResource, String>,
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
    let options = parse_options(std::env::args().skip(1))?;
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
        let after_commit_sequence = fence
            .arm(request_id, &revision.revision_id)
            .context("arm boot presentation with SOS compositor")?;
        eprintln!(
            "sos_compositor_armed request_id={request_id} revision_id={} after_commit_sequence={after_commit_sequence}",
            revision.revision_id
        );
    }
    let model = providers_fake::snapshot();
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
            let restore_bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
            let window_bounds = if windowed {
                WindowBounds::Windowed(restore_bounds)
            } else {
                WindowBounds::Fullscreen(restore_bounds)
            };
            let host = cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    titlebar: None,
                    app_id: Some("dev.sos.experience".into()),
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|cx| {
                        LinuxExperienceHost::new(
                            model,
                            worker,
                            ready,
                            revision,
                            request_id,
                            protocol_rx,
                            results,
                            options.service_socket,
                            compositor_fence,
                            cx,
                        )
                    })
                },
            );
            match host {
                Ok(_) => cx.activate(true),
                Err(error) => {
                    eprintln!("sos_experience_window_failed error={error}");
                    cx.quit();
                }
            }
        });
    Ok(())
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
    preparing: Option<PreparingRevision>,
    prepared: Option<PreparedRevision>,
    pending_commit: Option<PendingCommit>,
    pending_presentation: Option<PendingPresentation>,
    last_presented_revision: Option<String>,
    action_in_flight: bool,
    pending_input_events: VecDeque<SceneEvent>,
    action_commits: async_channel::Sender<ActionCommitResult>,
    next_action_request_id: u64,
    surface_gestures: HashMap<String, GestureSession>,
    surface_taps: HashMap<String, (String, Instant)>,
    status: Option<(String, bool)>,
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
        service_socket: Option<PathBuf>,
        compositor_fence: Option<CompositorFence>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (action_commits, action_results) = async_channel::unbounded();
        Self::attach_protocol(protocol, cx);
        Self::attach_worker_results(results, cx);
        Self::attach_action_results(action_results, cx);
        if let Some(fence) = &compositor_fence {
            Self::attach_compositor_events(fence.events(), cx);
        }
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
            preparing: None,
            prepared: None,
            pending_commit: None,
            pending_presentation: Some(PendingPresentation {
                request_id: boot_request_id,
                revision_id: revision.revision_id,
            }),
            last_presented_revision: None,
            action_in_flight: false,
            pending_input_events: VecDeque::new(),
            action_commits,
            next_action_request_id: 1,
            surface_gestures: HashMap::new(),
            surface_taps: HashMap::new(),
            status: Some(("Booting committed SOS revision…".into(), true)),
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
                self.status = None;
                eprintln!(
                    "sos_revision_frame revision_id={} evidence=nested_backend_submit commit_sequence={} submit_sequence={}",
                    presented.revision_id,
                    presented.commit_sequence,
                    presented.submit_sequence
                );
                emit(&HostEvent::Presented {
                    request_id: presented.request_id,
                    revision_id: presented.revision_id,
                });
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
                if let Err(error) = self.worker.prepare_candidate_with_assets(
                    request_id,
                    revision.source.clone(),
                    revision.assets.clone(),
                    self.model.clone(),
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
                });
                self.status = Some(("Preparing Luau revision…".into(), true));
                cx.notify();
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
                self.scene = scene;
                self.state = state;
                self.state_schema_version = state_schema_version;
                self.active_revision_id = revision_id;
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
                        self.state = state;
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
            Ok(authoritative) => {
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
                    self.state = authoritative.state;
                    self.scene = result.scene;
                    self.status = None;
                    eprintln!(
                        "sos_action_committed request_id={} authority_revision={}",
                        result.request_id, authoritative.revision
                    );
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

    fn render_node(
        &mut self,
        node: &SceneNode,
        path: SharedString,
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
        if let Some(Content::TextSession(input)) = &node.content {
            let text = if input.value.is_empty() {
                input.placeholder.clone()
            } else {
                input.value.clone()
            };
            element = element
                .border_1()
                .border_color(rgb(0x98A29B))
                .p(px(8.))
                .child(SharedString::from(text));
        }
        if let Some(Content::Image(image)) = &node.content {
            let path = if image.asset == "album-orbit" {
                ALBUM_ASSET.to_owned()
            } else {
                image.asset.clone()
            };
            element = element.child(img(path).size_full());
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
            element = element.child(self.render_node(child, child_path, cx));
        }
        let surface_owns_tap = uses_surface && node.interaction.tap_action.is_some();
        let mut rendered = if let Some(action) = node
            .interaction
            .tap_action
            .as_ref()
            .filter(|_| !surface_owns_tap)
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

impl scene_surface::SceneSurfaceHost for LinuxExperienceHost {
    fn enables_pointer_fallback() -> bool {
        true
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
        assets::install_fonts(window);
        let scene = self.scene.clone();
        let content = self.render_node(&scene.root, SharedString::from("root"), cx);
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
                    cx.notify();
                });
            }
        }
        root
    }
}

fn parse_options(args: impl Iterator<Item = String>) -> Result<Options> {
    let mut service_socket = None;
    let mut windowed = false;
    let mut args = args.peekable();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--service-socket" => {
                service_socket = Some(PathBuf::from(
                    args.next().context("--service-socket requires a path")?,
                ));
            }
            "--windowed" => windowed = true,
            other => bail!("unknown option: {other}"),
        }
    }
    Ok(Options {
        service_socket,
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

fn commit_action(
    socket: &Path,
    request_id: u64,
    revision_id: &str,
    source_sha256: &str,
    schema_version: u64,
    state: &JsonValue,
    effects: &[experience_ir::ProviderEffect],
) -> std::result::Result<StateResource, String> {
    let client = ServiceClient::new(socket, Duration::from_secs(2));
    let current = get_service_state(&client, 1)?;
    if current.revision_id != revision_id
        || current.source_sha256 != source_sha256
        || current.schema_version != schema_version
    {
        return Err("authority does not match the active experience revision".into());
    }
    let actions = effects
        .iter()
        .map(provider_action)
        .collect::<std::result::Result<Vec<_>, _>>()?;
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
    get_service_state(&client, 5)
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
) -> std::result::Result<ProviderAction, String> {
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
            Ok(ProviderAction::Notes(NotesAction::AttachToEvent {
                note_id: note_id.into(),
                event_title: event_title.into(),
            }))
        }
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
            ["--windowed", "--service-socket", "/run/sos/provider.sock"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert!(options.windowed);
        assert_eq!(
            options.service_socket,
            Some(PathBuf::from("/run/sos/provider.sock"))
        );
        assert!(parse_options(["--unknown".into()].into_iter()).is_err());
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
            ProviderAction::Notes(NotesAction::AttachToEvent {
                note_id: "note-1".into(),
                event_title: "Design review".into(),
            })
        );
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
        let temporary = tempfile::tempdir().unwrap();
        let state_file = temporary.path().join("authority.json");
        let socket = temporary.path().join("provider.sock");
        let revision_id = "b".repeat(64);
        let source_sha256 = "a".repeat(64);
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
            &[experience_ir::ProviderEffect {
                provider: "notes".into(),
                action: "attach_to_event".into(),
                payload: serde_json::json!({
                    "note_id": "note-1",
                    "event_title": "Design review"
                }),
            }],
        )
        .unwrap();
        assert_eq!(committed.revision, 2);
        assert_eq!(committed.state, state);
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
