use std::{
    cell::Cell,
    collections::HashMap,
    fs,
    path::{Component, Path},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use experience_ir::{
    validate_scene, Align, Animation, AnimationKind, ClipRect, Content, ExperienceModel, Flow,
    GlyphRun, HitRegion, ImageContent, Interaction, Justify, Layout, LayoutPosition, LayoutProgram,
    PaintOp, PaintPoint, PointerCapture, ProviderEffect, ProviderSurfaceContent, Scene, SceneEvent,
    SceneNode, SemanticRole, Semantics, TextContent, TextSession, Transform2D,
    EXPERIENCE_API_VERSION, MAX_CHILDREN, MAX_EFFECTS, MAX_EFFECT_PAYLOAD_BYTES, MAX_GLYPH_RUNS,
    MAX_HIT_REGIONS, MAX_PAINT_DEPTH, MAX_PAINT_OPS, MAX_PAINT_POINTS, MAX_REVISION_ASSETS,
    MAX_REVISION_ASSET_BYTES, MAX_REVISION_ASSET_TOTAL_BYTES, MAX_SCENE_DEPTH, MAX_SCENE_NODES,
    MAX_STATE_BYTES, MAX_TEXT_BYTES,
};
use mlua::{
    chunk::{ChunkMode, Compiler},
    Error as LuaError, Function, Lua, LuaSerdeExt, RegistryKey, Table, Value, VmState,
};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
pub const MEMORY_LIMIT_BYTES: usize = 16 * 1024 * 1024;
pub const RENDER_BUDGET: Duration = Duration::from_millis(20);
pub const UPDATE_BUDGET: Duration = Duration::from_millis(5);
pub const MIGRATION_BUDGET: Duration = Duration::from_millis(20);

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("source is larger than {MAX_SOURCE_BYTES} bytes")]
    SourceTooLarge,
    #[error("Luau error: {0}")]
    Lua(#[from] LuaError),
    #[error("invalid experience: {0}")]
    Invalid(String),
}

pub struct LuauRuntime {
    lua: Lua,
    module: RegistryKey,
    deadline: Rc<Cell<Option<Instant>>>,
    assets: Vec<RevisionAsset>,
    asset_paths: HashMap<String, String>,
    shader_paths: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionAsset {
    pub id: String,
    pub path: String,
    pub kind: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionAssetInput {
    pub id: String,
    pub kind: String,
    pub bytes: Vec<u8>,
}

#[derive(Deserialize)]
struct SidecarManifest {
    format_version: u32,
    assets: Vec<SidecarIdentity>,
}

#[derive(Deserialize)]
struct SidecarIdentity {
    id: String,
    kind: String,
    file: SidecarFileIdentity,
}

#[derive(Deserialize)]
struct SidecarFileIdentity {
    path: String,
    size: u64,
    sha256: String,
}

/// Loads and re-verifies the immutable sidecars from a supervisor revision.
/// The returned bytes are still untrusted input and are validated again by
/// `compile_with_assets` before they become visible to a scene.
pub fn load_revision_assets(directory: &Path) -> Result<Vec<RevisionAssetInput>, RuntimeError> {
    let manifest = fs::read(directory.join("manifest.json")).map_err(|error| {
        RuntimeError::Invalid(format!("could not read revision manifest: {error}"))
    })?;
    let manifest: SidecarManifest = serde_json::from_slice(&manifest)
        .map_err(|error| RuntimeError::Invalid(format!("invalid revision manifest: {error}")))?;
    if manifest.format_version != 3 || manifest.assets.len() > MAX_REVISION_ASSETS {
        return Err(RuntimeError::Invalid(
            "unsupported revision sidecar manifest".into(),
        ));
    }
    let mut total = 0usize;
    let mut assets = Vec::with_capacity(manifest.assets.len());
    for asset in manifest.assets {
        let relative = Path::new(&asset.file.path);
        if !asset.file.path.starts_with("assets/")
            || relative.components().count() != 2
            || !relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(RuntimeError::Invalid(format!(
                "invalid revision asset path: {}",
                asset.file.path
            )));
        }
        if asset.file.size == 0 || asset.file.size > MAX_REVISION_ASSET_BYTES as u64 {
            return Err(RuntimeError::Invalid(format!(
                "invalid revision asset size: {}",
                asset.id
            )));
        }
        total = total.saturating_add(asset.file.size as usize);
        if total > MAX_REVISION_ASSET_TOTAL_BYTES {
            return Err(RuntimeError::Invalid(
                "revision asset package is too large".into(),
            ));
        }
        let path = directory.join(relative);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            RuntimeError::Invalid(format!(
                "could not inspect revision asset {}: {error}",
                asset.id
            ))
        })?;
        if !metadata.file_type().is_file() || metadata.len() != asset.file.size {
            return Err(RuntimeError::Invalid(format!(
                "invalid revision asset type or size: {}",
                asset.id
            )));
        }
        let bytes = fs::read(path).map_err(|error| {
            RuntimeError::Invalid(format!(
                "could not read revision asset {}: {error}",
                asset.id
            ))
        })?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        if bytes.len() as u64 != asset.file.size || sha256 != asset.file.sha256 {
            return Err(RuntimeError::Invalid(format!(
                "revision asset identity mismatch: {}",
                asset.id
            )));
        }
        assets.push(RevisionAssetInput {
            id: asset.id,
            kind: asset.kind,
            bytes,
        });
    }
    Ok(assets)
}

#[derive(Clone, Debug)]
pub struct CandidateTimings {
    pub submitted_at: Instant,
    pub queue_us: u64,
    pub compile_us: u64,
    pub render_us: u64,
    pub worker_total_us: u64,
}

#[derive(Clone, Debug)]
pub struct WorkerReady {
    pub scene: Scene,
    pub state: JsonValue,
    pub state_schema_version: u64,
    pub worker_thread: String,
    pub initialize_us: u64,
    pub assets: Vec<RevisionAsset>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateOutcome {
    pub state: JsonValue,
    pub effects: Vec<ProviderEffect>,
}

#[derive(Clone, Debug)]
pub enum WorkerResult {
    CandidatePrepared {
        request_id: u64,
        source: String,
        scene: Scene,
        state: JsonValue,
        state_schema_version: u64,
        timings: CandidateTimings,
        assets: Vec<RevisionAsset>,
    },
    CandidateRejected {
        request_id: u64,
        source: String,
        error: String,
        timings: CandidateTimings,
    },
    CandidateCommitted {
        request_id: u64,
        source: String,
        scene: Scene,
        state: JsonValue,
        state_schema_version: u64,
        timings: CandidateTimings,
        assets: Vec<RevisionAsset>,
    },
    ActionCompleted {
        request_id: u64,
        state: JsonValue,
        scene: Scene,
        effects: Vec<ProviderEffect>,
        worker_us: u64,
    },
    ActionRejected {
        request_id: u64,
        error: String,
        worker_us: u64,
    },
    ModelRefreshed {
        request_id: u64,
        scene: Scene,
        worker_us: u64,
    },
    ModelRefreshRejected {
        request_id: u64,
        error: String,
        worker_us: u64,
    },
}

enum WorkerCommand {
    PrepareCandidate {
        request_id: u64,
        source: String,
        sidecars: Vec<RevisionAssetInput>,
        model: ExperienceModel,
        state: JsonValue,
        state_schema_version: u64,
        submitted_at: Instant,
    },
    CommitCandidate {
        request_id: u64,
    },
    DiscardCandidate {
        request_id: u64,
    },
    Action {
        request_id: u64,
        model: ExperienceModel,
        state: JsonValue,
        event: SceneEvent,
    },
    RefreshModel {
        request_id: u64,
        model: ExperienceModel,
        state: JsonValue,
    },
    Shutdown,
}

struct PreparedCandidate {
    request_id: u64,
    source: String,
    runtime: LuauRuntime,
    scene: Scene,
    state: JsonValue,
    state_schema_version: u64,
    timings: CandidateTimings,
}

pub struct RuntimeWorker {
    commands: async_channel::Sender<WorkerCommand>,
    results: async_channel::Receiver<WorkerResult>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RuntimeWorker {
    pub fn spawn(
        source: String,
        model: ExperienceModel,
        state: JsonValue,
        state_schema_version: u64,
    ) -> Result<(Self, async_channel::Receiver<Result<WorkerReady, String>>), RuntimeError> {
        Self::spawn_with_assets(source, model, state, state_schema_version, Vec::new())
    }

    pub fn spawn_with_assets(
        source: String,
        model: ExperienceModel,
        state: JsonValue,
        state_schema_version: u64,
        sidecars: Vec<RevisionAssetInput>,
    ) -> Result<(Self, async_channel::Receiver<Result<WorkerReady, String>>), RuntimeError> {
        let (commands_tx, commands_rx) = async_channel::unbounded();
        let (results_tx, results_rx) = async_channel::unbounded();
        let (ready_tx, ready_rx) = async_channel::bounded::<Result<WorkerReady, String>>(1);

        let thread = thread::Builder::new()
            .name("sos-luau-runtime".into())
            .spawn(move || {
                let started = Instant::now();
                let initialized =
                    LuauRuntime::compile_with_assets(&source, sidecars).and_then(|runtime| {
                        let state = runtime.migrate_state(state_schema_version, &state)?;
                        let state_schema_version = runtime.state_schema_version()?;
                        let scene = runtime.render(&model, &state)?;
                        Ok((runtime, scene, state, state_schema_version))
                    });
                let (mut active_runtime, scene, active_state, active_state_schema_version) =
                    match initialized {
                        Ok(initialized) => initialized,
                        Err(error) => {
                            let _ = ready_tx.send_blocking(Err(error.to_string()));
                            return;
                        }
                    };
                let ready = WorkerReady {
                    scene,
                    state: active_state,
                    state_schema_version: active_state_schema_version,
                    worker_thread: format!("{:?}", thread::current().id()),
                    initialize_us: micros(started.elapsed()),
                    assets: active_runtime.assets().to_vec(),
                };
                if ready_tx.send_blocking(Ok(ready)).is_err() {
                    return;
                }

                let mut prepared: Option<PreparedCandidate> = None;
                while let Ok(command) = commands_rx.recv_blocking() {
                    match command {
                        WorkerCommand::PrepareCandidate {
                            request_id,
                            source,
                            sidecars,
                            model,
                            state,
                            state_schema_version,
                            submitted_at,
                        } => {
                            if prepared.is_some() {
                                let timings = CandidateTimings {
                                    submitted_at,
                                    queue_us: micros(submitted_at.elapsed()),
                                    compile_us: 0,
                                    render_us: 0,
                                    worker_total_us: micros(submitted_at.elapsed()),
                                };
                                let _ = results_tx.send_blocking(WorkerResult::CandidateRejected {
                                    request_id,
                                    source,
                                    error: "another candidate is awaiting commit".into(),
                                    timings,
                                });
                                continue;
                            }

                            let worker_started = Instant::now();
                            let queue_us = micros(worker_started.duration_since(submitted_at));
                            let compile_started = Instant::now();
                            let candidate_runtime =
                                match LuauRuntime::compile_with_assets(&source, sidecars) {
                                    Ok(runtime) => runtime,
                                    Err(error) => {
                                        let timings = CandidateTimings {
                                            submitted_at,
                                            queue_us,
                                            compile_us: micros(compile_started.elapsed()),
                                            render_us: 0,
                                            worker_total_us: micros(worker_started.elapsed()),
                                        };
                                        let _ = results_tx.send_blocking(
                                            WorkerResult::CandidateRejected {
                                                request_id,
                                                source,
                                                error: error.to_string(),
                                                timings,
                                            },
                                        );
                                        continue;
                                    }
                                };
                            let compile_us = micros(compile_started.elapsed());
                            let state = match candidate_runtime
                                .migrate_state(state_schema_version, &state)
                            {
                                Ok(state) => state,
                                Err(error) => {
                                    let timings = CandidateTimings {
                                        submitted_at,
                                        queue_us,
                                        compile_us,
                                        render_us: 0,
                                        worker_total_us: micros(worker_started.elapsed()),
                                    };
                                    let _ =
                                        results_tx.send_blocking(WorkerResult::CandidateRejected {
                                            request_id,
                                            source,
                                            error: error.to_string(),
                                            timings,
                                        });
                                    continue;
                                }
                            };
                            let state_schema_version = match candidate_runtime
                                .state_schema_version()
                            {
                                Ok(version) => version,
                                Err(error) => {
                                    let timings = CandidateTimings {
                                        submitted_at,
                                        queue_us,
                                        compile_us,
                                        render_us: 0,
                                        worker_total_us: micros(worker_started.elapsed()),
                                    };
                                    let _ =
                                        results_tx.send_blocking(WorkerResult::CandidateRejected {
                                            request_id,
                                            source,
                                            error: error.to_string(),
                                            timings,
                                        });
                                    continue;
                                }
                            };
                            let render_started = Instant::now();
                            let scene = match candidate_runtime.render(&model, &state) {
                                Ok(scene) => scene,
                                Err(error) => {
                                    let timings = CandidateTimings {
                                        submitted_at,
                                        queue_us,
                                        compile_us,
                                        render_us: micros(render_started.elapsed()),
                                        worker_total_us: micros(worker_started.elapsed()),
                                    };
                                    let _ =
                                        results_tx.send_blocking(WorkerResult::CandidateRejected {
                                            request_id,
                                            source,
                                            error: error.to_string(),
                                            timings,
                                        });
                                    continue;
                                }
                            };
                            let timings = CandidateTimings {
                                submitted_at,
                                queue_us,
                                compile_us,
                                render_us: micros(render_started.elapsed()),
                                worker_total_us: micros(worker_started.elapsed()),
                            };
                            let assets = candidate_runtime.assets().to_vec();
                            prepared = Some(PreparedCandidate {
                                request_id,
                                source: source.clone(),
                                runtime: candidate_runtime,
                                scene: scene.clone(),
                                state: state.clone(),
                                state_schema_version,
                                timings: timings.clone(),
                            });
                            let _ = results_tx.send_blocking(WorkerResult::CandidatePrepared {
                                request_id,
                                source,
                                scene,
                                state,
                                state_schema_version,
                                timings,
                                assets,
                            });
                        }
                        WorkerCommand::CommitCandidate { request_id } => {
                            let Some(candidate) = prepared.take() else {
                                continue;
                            };
                            if candidate.request_id != request_id {
                                prepared = Some(candidate);
                                continue;
                            }
                            active_runtime = candidate.runtime;
                            let assets = active_runtime.assets().to_vec();
                            let _ = results_tx.send_blocking(WorkerResult::CandidateCommitted {
                                request_id,
                                source: candidate.source,
                                scene: candidate.scene,
                                state: candidate.state,
                                state_schema_version: candidate.state_schema_version,
                                timings: candidate.timings,
                                assets,
                            });
                        }
                        WorkerCommand::DiscardCandidate { request_id } => {
                            if prepared.as_ref().map(|candidate| candidate.request_id)
                                == Some(request_id)
                            {
                                prepared = None;
                            }
                        }
                        WorkerCommand::Action {
                            request_id,
                            model,
                            state,
                            event,
                        } => {
                            let started = Instant::now();
                            let result = active_runtime
                                .update_with_effects(&model, &state, &event)
                                .and_then(|outcome| {
                                    let scene = active_runtime.render(&model, &outcome.state)?;
                                    Ok((outcome, scene))
                                });
                            let worker_us = micros(started.elapsed());
                            let result = match result {
                                Ok((outcome, scene)) => WorkerResult::ActionCompleted {
                                    request_id,
                                    state: outcome.state,
                                    scene,
                                    effects: outcome.effects,
                                    worker_us,
                                },
                                Err(error) => WorkerResult::ActionRejected {
                                    request_id,
                                    error: error.to_string(),
                                    worker_us,
                                },
                            };
                            let _ = results_tx.send_blocking(result);
                        }
                        WorkerCommand::RefreshModel {
                            request_id,
                            model,
                            state,
                        } => {
                            let started = Instant::now();
                            let result = active_runtime.render(&model, &state);
                            let worker_us = micros(started.elapsed());
                            let result = match result {
                                Ok(scene) => WorkerResult::ModelRefreshed {
                                    request_id,
                                    scene,
                                    worker_us,
                                },
                                Err(error) => WorkerResult::ModelRefreshRejected {
                                    request_id,
                                    error: error.to_string(),
                                    worker_us,
                                },
                            };
                            let _ = results_tx.send_blocking(result);
                        }
                        WorkerCommand::Shutdown => break,
                    }
                }
            })
            .map_err(|error| RuntimeError::Invalid(format!("could not start worker: {error}")))?;

        Ok((
            Self {
                commands: commands_tx,
                results: results_rx,
                thread: Some(thread),
            },
            ready_rx,
        ))
    }

    pub fn start(
        source: String,
        model: ExperienceModel,
        state: JsonValue,
        state_schema_version: u64,
    ) -> Result<(Self, WorkerReady), RuntimeError> {
        Self::start_with_assets(source, model, state, state_schema_version, Vec::new())
    }

    pub fn start_with_assets(
        source: String,
        model: ExperienceModel,
        state: JsonValue,
        state_schema_version: u64,
        sidecars: Vec<RevisionAssetInput>,
    ) -> Result<(Self, WorkerReady), RuntimeError> {
        let (worker, ready_rx) =
            Self::spawn_with_assets(source, model, state, state_schema_version, sidecars)?;
        let ready = ready_rx
            .recv_blocking()
            .map_err(|_| RuntimeError::Invalid("runtime worker stopped during startup".into()))?
            .map_err(RuntimeError::Invalid)?;
        Ok((worker, ready))
    }

    pub fn results(&self) -> async_channel::Receiver<WorkerResult> {
        self.results.clone()
    }

    pub fn prepare_candidate(
        &self,
        request_id: u64,
        source: String,
        model: ExperienceModel,
        state: JsonValue,
        state_schema_version: u64,
        submitted_at: Instant,
    ) -> Result<(), String> {
        self.prepare_candidate_with_assets(
            request_id,
            source,
            Vec::new(),
            model,
            state,
            state_schema_version,
            submitted_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_candidate_with_assets(
        &self,
        request_id: u64,
        source: String,
        sidecars: Vec<RevisionAssetInput>,
        model: ExperienceModel,
        state: JsonValue,
        state_schema_version: u64,
        submitted_at: Instant,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(WorkerCommand::PrepareCandidate {
                request_id,
                source,
                sidecars,
                model,
                state,
                state_schema_version,
                submitted_at,
            })
            .map_err(|_| "runtime worker is unavailable".into())
    }

    pub fn commit_candidate(&self, request_id: u64) -> Result<(), String> {
        self.commands
            .send_blocking(WorkerCommand::CommitCandidate { request_id })
            .map_err(|_| "runtime worker is unavailable".into())
    }

    pub fn discard_candidate(&self, request_id: u64) -> Result<(), String> {
        self.commands
            .send_blocking(WorkerCommand::DiscardCandidate { request_id })
            .map_err(|_| "runtime worker is unavailable".into())
    }

    pub fn action(
        &self,
        request_id: u64,
        model: ExperienceModel,
        state: JsonValue,
        event: SceneEvent,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(WorkerCommand::Action {
                request_id,
                model,
                state,
                event,
            })
            .map_err(|_| "runtime worker is unavailable".into())
    }

    pub fn refresh_model(
        &self,
        request_id: u64,
        model: ExperienceModel,
        state: JsonValue,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(WorkerCommand::RefreshModel {
                request_id,
                model,
                state,
            })
            .map_err(|_| "runtime worker is unavailable".into())
    }

    pub fn shutdown(mut self) -> Result<(), String> {
        let _ = self.commands.send_blocking(WorkerCommand::Shutdown);
        self.commands.close();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| "runtime worker panicked during shutdown".to_owned())?;
        }
        Ok(())
    }
}

impl Drop for RuntimeWorker {
    fn drop(&mut self) {
        self.commands.close();
    }
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

impl LuauRuntime {
    pub fn compile(source: &str) -> Result<Self, RuntimeError> {
        Self::compile_with_assets(source, Vec::new())
    }

    pub fn compile_with_assets(
        source: &str,
        sidecars: Vec<RevisionAssetInput>,
    ) -> Result<Self, RuntimeError> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(RuntimeError::SourceTooLarge);
        }

        let lua = Lua::new();
        lua.set_memory_limit(MEMORY_LIMIT_BYTES)?;
        lua.set_compiler(Compiler::new().set_optimization_level(1).set_debug_level(1));
        lua.sandbox(true)?;

        let deadline = Rc::new(Cell::new(None::<Instant>));
        let interrupt_deadline = deadline.clone();
        lua.set_interrupt(move |_| {
            if interrupt_deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                return Err(LuaError::RuntimeError(
                    "experience exceeded its time budget".into(),
                ));
            }
            Ok(VmState::Continue)
        });

        let (module, mut assets, mut asset_paths) = {
            let result = run_bounded(&deadline, RENDER_BUDGET, || {
                lua.load(source)
                    .set_name("experience")
                    .set_mode(ChunkMode::Text)
                    .eval::<Table>()
            })?;
            let _: Function = result.get("render").map_err(|_| {
                RuntimeError::Invalid("module must export render(model, state)".into())
            })?;
            let api_version = result
                .get::<Option<u32>>("api_version")?
                .ok_or_else(|| RuntimeError::Invalid("module must export api_version".into()))?;
            if api_version != EXPERIENCE_API_VERSION {
                return Err(RuntimeError::Invalid(format!(
                    "experience API {api_version} is unsupported; host requires {EXPERIENCE_API_VERSION}"
                )));
            }
            let (assets, asset_paths) =
                decode_revision_assets(result.get::<Option<Table>>("assets")?)?;
            (lua.create_registry_value(result)?, assets, asset_paths)
        };
        merge_revision_assets(&mut assets, &mut asset_paths, sidecars)?;
        let shader_paths = assets
            .iter()
            .filter(|asset| asset.kind == "shader")
            .map(|asset| (asset.id.clone(), asset.path.clone()))
            .collect();

        Ok(Self {
            lua,
            module,
            deadline,
            assets,
            asset_paths,
            shader_paths,
        })
    }

    pub fn assets(&self) -> &[RevisionAsset] {
        &self.assets
    }

    pub fn initial_state(&self) -> JsonValue {
        json!({})
    }

    pub fn state_schema_version(&self) -> Result<u64, RuntimeError> {
        let module: Table = self.lua.registry_value(&self.module)?;
        let version = module.get::<Option<u64>>("state_version")?.unwrap_or(1);
        if version == 0 {
            return Err(RuntimeError::Invalid(
                "state_version must be a positive integer".into(),
            ));
        }
        Ok(version)
    }

    pub fn migrate_state(
        &self,
        from_version: u64,
        state: &JsonValue,
    ) -> Result<JsonValue, RuntimeError> {
        let target_version = self.state_schema_version()?;
        if from_version == target_version {
            return Ok(state.clone());
        }
        if from_version > target_version {
            return Err(RuntimeError::Invalid(format!(
                "cannot migrate state backward from schema {from_version} to {target_version}"
            )));
        }
        let module: Table = self.lua.registry_value(&self.module)?;
        let migrate: Option<Function> = module.get("migrate")?;
        let migrate = migrate.ok_or_else(|| {
            RuntimeError::Invalid(format!(
                "schema changed from {from_version} to {target_version} but migrate(from_version, state) is missing"
            ))
        })?;
        let state = self.lua.to_value(state)?;
        let migrated = run_bounded(&self.deadline, MIGRATION_BUDGET, || {
            migrate.call::<Value>((from_version, state))
        })?;
        let migrated: JsonValue = self.lua.from_value(migrated)?;
        if serde_json::to_vec(&migrated)
            .map_err(|error| RuntimeError::Invalid(error.to_string()))?
            .len()
            > MAX_STATE_BYTES
        {
            return Err(RuntimeError::Invalid("migrated state is too large".into()));
        }
        Ok(migrated)
    }

    pub fn render(
        &self,
        model: &ExperienceModel,
        state: &JsonValue,
    ) -> Result<Scene, RuntimeError> {
        let module: Table = self.lua.registry_value(&self.module)?;
        let render: Function = module.get("render")?;
        let model = self.lua.to_value(model)?;
        let state = self.lua.to_value(state)?;
        let value = run_bounded(&self.deadline, RENDER_BUDGET, || {
            render.call::<Value>((model, state))
        })?;
        let scene = Scene {
            root: Decoder {
                nodes: 0,
                asset_paths: &self.asset_paths,
                shader_paths: &self.shader_paths,
            }
            .node(value, 1)?,
        };
        validate_scene(&scene).map_err(|error| RuntimeError::Invalid(error.to_string()))?;
        Ok(scene)
    }

    pub fn update(
        &self,
        model: &ExperienceModel,
        state: &JsonValue,
        event: &SceneEvent,
    ) -> Result<JsonValue, RuntimeError> {
        Ok(self.update_with_effects(model, state, event)?.state)
    }

    pub fn update_with_effects(
        &self,
        model: &ExperienceModel,
        state: &JsonValue,
        event: &SceneEvent,
    ) -> Result<UpdateOutcome, RuntimeError> {
        let module: Table = self.lua.registry_value(&self.module)?;
        let update: Option<Function> = module.get("update")?;
        let Some(update) = update else {
            return Ok(UpdateOutcome {
                state: state.clone(),
                effects: Vec::new(),
            });
        };
        let model = self.lua.to_value(model)?;
        let state = self.lua.to_value(state)?;
        let event = self.lua.to_value(event)?;
        let value = run_bounded(&self.deadline, UPDATE_BUDGET, || {
            update.call::<Value>((model, state, event))
        })?;
        if let Value::Table(table) = &value {
            let envelope_state = table.get::<Option<Value>>("state")?;
            if let Some(envelope_state) = envelope_state {
                let state = self.lua.from_value(envelope_state)?;
                let effects = decode_effects(table.get::<Option<Table>>("effects")?, &self.lua)?;
                return Ok(UpdateOutcome { state, effects });
            }
        }
        Ok(UpdateOutcome {
            state: self.lua.from_value(value)?,
            effects: Vec::new(),
        })
    }
}

fn decode_revision_assets(
    table: Option<Table>,
) -> Result<(Vec<RevisionAsset>, HashMap<String, String>), RuntimeError> {
    let Some(table) = table else {
        return Ok((Vec::new(), HashMap::new()));
    };
    let mut assets = Vec::new();
    let mut paths = HashMap::new();
    for pair in table.pairs::<String, Table>() {
        if assets.len() >= MAX_REVISION_ASSETS {
            return Err(RuntimeError::Invalid(
                "revision declares too many assets".into(),
            ));
        }
        let (id, asset) = pair?;
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || id == "album-orbit"
            || paths.contains_key(&id)
        {
            return Err(RuntimeError::Invalid(format!(
                "invalid or duplicate revision asset id: {id}"
            )));
        }
        let kind = required_bounded_string(&asset, "kind", 32)?;
        if kind != "svg" {
            return Err(RuntimeError::Invalid(format!(
                "unsupported revision asset kind: {kind}"
            )));
        }
        let data = required_bounded_string(&asset, "data", MAX_REVISION_ASSET_BYTES)?;
        validate_svg_asset(&data)?;
        let bytes = data.into_bytes();
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let path = format!("sos/revisions/{sha256}.svg");
        paths.insert(id.clone(), path.clone());
        assets.push(RevisionAsset {
            id,
            path,
            kind,
            bytes,
            sha256,
        });
    }
    Ok((assets, paths))
}

fn merge_revision_assets(
    assets: &mut Vec<RevisionAsset>,
    paths: &mut HashMap<String, String>,
    mut sidecars: Vec<RevisionAssetInput>,
) -> Result<(), RuntimeError> {
    if assets.len().saturating_add(sidecars.len()) > MAX_REVISION_ASSETS {
        return Err(RuntimeError::Invalid(
            "revision declares too many assets".into(),
        ));
    }
    sidecars.sort_by(|left, right| left.id.cmp(&right.id));
    for sidecar in sidecars {
        if sidecar.id.is_empty()
            || sidecar.id.len() > 128
            || !sidecar
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || sidecar.id == "album-orbit"
            || assets.iter().any(|asset| asset.id == sidecar.id)
        {
            return Err(RuntimeError::Invalid(format!(
                "invalid or duplicate revision asset id: {}",
                sidecar.id
            )));
        }
        validate_revision_asset_bytes(&sidecar.kind, &sidecar.bytes)?;
        let extension = revision_asset_extension(&sidecar.kind).ok_or_else(|| {
            RuntimeError::Invalid(format!("unsupported revision asset kind: {}", sidecar.kind))
        })?;
        let sha256 = format!("{:x}", Sha256::digest(&sidecar.bytes));
        let path = format!("sos/revisions/{sha256}.{extension}");
        if matches!(sidecar.kind.as_str(), "svg" | "png" | "jpeg" | "webp") {
            paths.insert(sidecar.id.clone(), path.clone());
        }
        assets.push(RevisionAsset {
            id: sidecar.id,
            path,
            kind: sidecar.kind,
            bytes: sidecar.bytes,
            sha256,
        });
    }
    let total = assets.iter().map(|asset| asset.bytes.len()).sum::<usize>();
    if total > MAX_REVISION_ASSET_TOTAL_BYTES {
        return Err(RuntimeError::Invalid(
            "revision asset package is too large".into(),
        ));
    }
    Ok(())
}

fn revision_asset_extension(kind: &str) -> Option<&'static str> {
    match kind {
        "svg" => Some("svg"),
        "png" => Some("png"),
        "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        "font" => Some("font"),
        "shader" => Some("wgsl"),
        _ => None,
    }
}

fn validate_revision_asset_bytes(kind: &str, bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.is_empty() || bytes.len() > MAX_REVISION_ASSET_BYTES {
        return Err(RuntimeError::Invalid(format!("invalid {kind} asset size")));
    }
    let valid = match kind {
        "svg" => std::str::from_utf8(bytes)
            .ok()
            .is_some_and(|data| validate_svg_asset(data).is_ok()),
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        "font" => {
            bytes.starts_with(&[0, 1, 0, 0])
                || bytes.starts_with(b"OTTO")
                || bytes.starts_with(b"ttcf")
                || bytes.starts_with(b"wOFF")
                || bytes.starts_with(b"wOF2")
        }
        "shader" => std::str::from_utf8(bytes)
            .ok()
            .is_some_and(validate_shader_asset),
        _ => false,
    };
    if !valid {
        return Err(RuntimeError::Invalid(format!(
            "invalid or unsupported {kind} revision asset"
        )));
    }
    Ok(())
}

/// A shader paint has no bindings and no compute entry point. The stable host
/// supplies a three-vertex fullscreen draw into a bounded RGBA target, which
/// keeps revision WGSL from acquiring buffers, textures, or storage authority.
fn validate_shader_asset(source: &str) -> bool {
    let Ok(module) = naga::front::wgsl::parse_str(source) else {
        return false;
    };
    if module
        .global_variables
        .iter()
        .any(|(_, global)| global.binding.is_some())
    {
        return false;
    }
    let has_vertex = module
        .entry_points
        .iter()
        .any(|entry| entry.name == "vs_main" && entry.stage == naga::ShaderStage::Vertex);
    let has_fragment = module
        .entry_points
        .iter()
        .any(|entry| entry.name == "fs_main" && entry.stage == naga::ShaderStage::Fragment);
    let has_compute = module
        .entry_points
        .iter()
        .any(|entry| entry.stage == naga::ShaderStage::Compute);
    has_vertex
        && has_fragment
        && !has_compute
        && naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .is_ok()
}

fn validate_svg_asset(data: &str) -> Result<(), RuntimeError> {
    let normalized = data.to_ascii_lowercase();
    if data.len() > MAX_REVISION_ASSET_BYTES
        || !normalized.contains("<svg")
        || !normalized.contains("</svg>")
        || [
            "<script",
            "javascript:",
            "<!doctype",
            "<!entity",
            "<foreignobject",
            "xlink:href",
            "href=\"http",
            "href='http",
            "url(http",
        ]
        .iter()
        .any(|needle| normalized.contains(needle))
    {
        return Err(RuntimeError::Invalid(
            "revision SVG is malformed or uses an external/active feature".into(),
        ));
    }
    Ok(())
}

fn decode_effects(table: Option<Table>, lua: &Lua) -> Result<Vec<ProviderEffect>, RuntimeError> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let mut effects = Vec::new();
    for effect in table.sequence_values::<Table>() {
        if effects.len() >= MAX_EFFECTS {
            return Err(RuntimeError::Invalid("too many provider effects".into()));
        }
        let effect = effect?;
        let provider = required_bounded_string(&effect, "provider", 128)?;
        let action = required_bounded_string(&effect, "action", 128)?;
        if !matches!(
            (provider.as_str(), action.as_str()),
            ("notes", "attach_to_event")
                | ("notes", "write")
                | ("calendar", "append")
                | ("music", "command")
                | ("agent", "prompt")
                | ("agent", "configure_openai")
                | ("agent", "configure_openrouter")
                | ("agent", "configure_codex")
                | ("agent", "use_fake")
                | ("agent", "clear_credential")
                | ("audio", "set_volume")
                | ("audio", "adjust_volume")
                | ("audio", "set_muted")
                | ("media", "play_pause")
                | ("media", "next")
                | ("media", "previous")
                | ("network", "refresh")
                | ("network", "connect")
                | ("network", "disconnect")
                | ("apps", "launch")
                | ("attention", "acknowledge")
        ) {
            return Err(RuntimeError::Invalid(format!(
                "provider action is not allowed: {provider}.{action}"
            )));
        }
        let payload = match effect.get::<Option<Value>>("payload")? {
            Some(value) => lua.from_value(value)?,
            None => JsonValue::Null,
        };
        if serde_json::to_vec(&payload)
            .map_err(|error| RuntimeError::Invalid(error.to_string()))?
            .len()
            > MAX_EFFECT_PAYLOAD_BYTES
        {
            return Err(RuntimeError::Invalid(
                "provider effect payload is too large".into(),
            ));
        }
        effects.push(ProviderEffect {
            provider,
            action,
            payload,
        });
    }
    Ok(effects)
}

fn run_bounded<T>(
    deadline: &Cell<Option<Instant>>,
    budget: Duration,
    operation: impl FnOnce() -> Result<T, LuaError>,
) -> Result<T, RuntimeError> {
    deadline.set(Some(Instant::now() + budget));
    let result = operation();
    deadline.set(None);
    result.map_err(RuntimeError::from)
}

struct Decoder<'a> {
    nodes: usize,
    asset_paths: &'a HashMap<String, String>,
    shader_paths: &'a HashMap<String, String>,
}

impl Decoder<'_> {
    fn node(&mut self, value: Value, depth: usize) -> Result<SceneNode, RuntimeError> {
        if depth > MAX_SCENE_DEPTH {
            return Err(RuntimeError::Invalid("scene is too deep".into()));
        }
        self.nodes += 1;
        if self.nodes > MAX_SCENE_NODES {
            return Err(RuntimeError::Invalid("scene has too many nodes".into()));
        }

        let table = value
            .as_table()
            .ok_or_else(|| RuntimeError::Invalid("each scene node must be a table".into()))?;
        let mut children = Vec::new();
        if let Some(child_table) = table.get::<Option<Table>>("children")? {
            for child in child_table.sequence_values::<Value>() {
                if children.len() >= MAX_CHILDREN {
                    return Err(RuntimeError::Invalid("node has too many children".into()));
                }
                children.push(self.node(child?, depth + 1)?);
            }
        }

        Ok(SceneNode {
            id: bounded_optional_string(table, "id", 256)?,
            layout: decode_layout(table.get::<Option<Table>>("layout")?)?,
            content: decode_content(table.get::<Option<Table>>("content")?, self.asset_paths)?,
            paint: decode_paint(table.get::<Option<Table>>("paint")?, self.shader_paths)?,
            interaction: decode_interaction(table.get::<Option<Table>>("interaction")?)?,
            animation: decode_animation(table.get::<Option<Table>>("animation")?)?,
            semantics: decode_semantics(table.get::<Option<Table>>("semantics")?)?,
            children,
        })
    }
}

fn decode_layout(table: Option<Table>) -> Result<Layout, RuntimeError> {
    let Some(table) = table else {
        return Ok(Layout::default());
    };
    Ok(Layout {
        flow: match table.get::<Option<String>>("flow")?.as_deref() {
            None | Some("overlay") => Flow::Overlay,
            Some("column") => Flow::Column,
            Some("row") => Flow::Row,
            Some(value) => return Err(RuntimeError::Invalid(format!("invalid flow: {value}"))),
        },
        scroll_y: table.get::<Option<bool>>("scroll_y")?.unwrap_or(false),
        padding: finite_dimension(&table, "padding")?,
        gap: finite_dimension(&table, "gap")?,
        width: finite_dimension(&table, "width")?,
        height: finite_dimension(&table, "height")?,
        min_width: finite_dimension(&table, "min_width")?,
        min_height: finite_dimension(&table, "min_height")?,
        max_width: finite_dimension(&table, "max_width")?,
        max_height: finite_dimension(&table, "max_height")?,
        aspect_ratio: finite_dimension(&table, "aspect_ratio")?,
        position: table
            .get::<Option<Table>>("position")?
            .map(|position| -> Result<LayoutPosition, RuntimeError> {
                Ok(LayoutPosition {
                    x: scene_number(&position, "x")?,
                    y: scene_number(&position, "y")?,
                })
            })
            .transpose()?,
        program: table
            .get::<Option<Table>>("program")?
            .map(|program| -> Result<LayoutProgram, RuntimeError> {
                Ok(LayoutProgram {
                    measure_width: layout_fraction(&program, "measure_width")?,
                    measure_height: layout_fraction(&program, "measure_height")?,
                    arrange_x: layout_fraction(&program, "arrange_x")?,
                    arrange_y: layout_fraction(&program, "arrange_y")?,
                })
            })
            .transpose()?,
        clip_bounds: table.get::<Option<bool>>("clip_bounds")?.unwrap_or(false),
        grow: table.get::<Option<bool>>("grow")?.unwrap_or(false),
        align: match table.get::<Option<String>>("align")?.as_deref() {
            None => None,
            Some("start") => Some(Align::Start),
            Some("center") => Some(Align::Center),
            Some("end") => Some(Align::End),
            Some(value) => return Err(RuntimeError::Invalid(format!("invalid align: {value}"))),
        },
        justify: match table.get::<Option<String>>("justify")?.as_deref() {
            None => None,
            Some("start") => Some(Justify::Start),
            Some("center") => Some(Justify::Center),
            Some("end") => Some(Justify::End),
            Some("between") => Some(Justify::Between),
            Some(value) => return Err(RuntimeError::Invalid(format!("invalid justify: {value}"))),
        },
    })
}

fn decode_content(
    table: Option<Table>,
    asset_paths: &HashMap<String, String>,
) -> Result<Option<Content>, RuntimeError> {
    let Some(table) = table else {
        return Ok(None);
    };
    let kind = required_bounded_string(&table, "kind", 32)?;
    let content = match kind.as_str() {
        "text" => Content::Text(TextContent {
            value: required_bounded_string(&table, "value", MAX_TEXT_BYTES)?,
            size: required_dimension(&table, "size")?,
            color: table.get("color")?,
        }),
        "text_session" => Content::TextSession(TextSession {
            state_key: required_bounded_string(&table, "state_key", 256)?,
            value: required_bounded_string(&table, "value", MAX_TEXT_BYTES)?,
            placeholder: bounded_optional_string(&table, "placeholder", MAX_TEXT_BYTES)?
                .unwrap_or_default(),
            submit_action: bounded_optional_string(&table, "submit_action", 256)?,
            autofocus: table.get::<Option<bool>>("autofocus")?.unwrap_or(false),
        }),
        "image" => {
            let asset_id = required_bounded_string(&table, "asset", 256)?;
            let asset = if asset_id == "album-orbit" {
                asset_id
            } else if let Some(path) = asset_paths.get(&asset_id) {
                path.clone()
            } else {
                return Err(RuntimeError::Invalid(format!(
                    "image asset is not declared by this revision: {asset_id}"
                )));
            };
            Content::Image(ImageContent { asset })
        }
        "provider_surface" => Content::ProviderSurface(ProviderSurfaceContent {
            surface: required_bounded_string(&table, "surface", 128)?,
        }),
        other => {
            return Err(RuntimeError::Invalid(format!(
                "unknown content kind: {other}"
            )))
        }
    };
    Ok(Some(content))
}

fn decode_paint(
    table: Option<Table>,
    shader_paths: &HashMap<String, String>,
) -> Result<Vec<PaintOp>, RuntimeError> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    PaintDecoder {
        operations: 0,
        points: 0,
        glyph_runs: 0,
        shader_paths,
    }
    .operations(table, 1)
}

struct PaintDecoder<'a> {
    operations: usize,
    points: usize,
    glyph_runs: usize,
    shader_paths: &'a HashMap<String, String>,
}

impl PaintDecoder<'_> {
    fn operations(&mut self, table: Table, depth: usize) -> Result<Vec<PaintOp>, RuntimeError> {
        if depth > MAX_PAINT_DEPTH {
            return Err(RuntimeError::Invalid("paint list is too deep".into()));
        }
        let mut decoded = Vec::new();
        for operation in table.sequence_values::<Table>() {
            self.operations += 1;
            if self.operations > MAX_PAINT_OPS {
                return Err(RuntimeError::Invalid(
                    "scene node has too many paint operations".into(),
                ));
            }
            decoded.push(self.operation(operation?, depth)?);
        }
        Ok(decoded)
    }

    fn operation(&mut self, operation: Table, depth: usize) -> Result<PaintOp, RuntimeError> {
        let kind = required_bounded_string(&operation, "kind", 32)?;
        Ok(match kind.as_str() {
            "fill_bounds" => PaintOp::FillBounds {
                color: operation.get("color")?,
                radius: optional_scene_number(&operation, "radius")?.unwrap_or_default(),
            },
            "path" => {
                let points_table: Table = operation.get("points")?;
                let mut points = Vec::new();
                for point in points_table.sequence_values::<Table>() {
                    self.points += 1;
                    if self.points > MAX_PAINT_POINTS {
                        return Err(RuntimeError::Invalid(
                            "scene node has too many paint points".into(),
                        ));
                    }
                    let point = point?;
                    points.push(PaintPoint {
                        x: scene_number(&point, "x")?,
                        y: scene_number(&point, "y")?,
                    });
                }
                PaintOp::Path {
                    points,
                    color: operation.get("color")?,
                    width: optional_scene_number(&operation, "width")?,
                    closed: operation.get::<Option<bool>>("closed")?.unwrap_or(false),
                }
            }
            "quad" => PaintOp::Quad {
                x: scene_number(&operation, "x")?,
                y: scene_number(&operation, "y")?,
                width: scene_number(&operation, "width")?,
                height: scene_number(&operation, "height")?,
                radius: optional_scene_number(&operation, "radius")?.unwrap_or_default(),
                color: operation.get("color")?,
            },
            "glyphs" => {
                let run_table: Table = operation.get("runs")?;
                let mut runs = Vec::new();
                for run in run_table.sequence_values::<Table>() {
                    self.glyph_runs += 1;
                    if self.glyph_runs > MAX_GLYPH_RUNS {
                        return Err(RuntimeError::Invalid(
                            "scene node has too many glyph runs".into(),
                        ));
                    }
                    let run = run?;
                    runs.push(GlyphRun {
                        text: required_bounded_string(&run, "text", MAX_TEXT_BYTES)?,
                        color: run.get("color")?,
                        font_family: bounded_optional_string(&run, "font_family", 256)?,
                        weight: run.get::<Option<u16>>("weight")?.unwrap_or(400),
                        italic: run.get::<Option<bool>>("italic")?.unwrap_or(false),
                    });
                }
                PaintOp::Glyphs {
                    x: scene_number(&operation, "x")?,
                    y: scene_number(&operation, "y")?,
                    size: required_dimension(&operation, "size")?,
                    line_height: optional_scene_number(&operation, "line_height")?,
                    max_width: optional_scene_number(&operation, "max_width")?,
                    runs,
                }
            }
            "shader" => {
                let asset_id = required_bounded_string(&operation, "asset", 128)?;
                let asset = self.shader_paths.get(&asset_id).cloned().ok_or_else(|| {
                    RuntimeError::Invalid(format!(
                        "shader asset is not declared by this revision: {asset_id}"
                    ))
                })?;
                PaintOp::Shader {
                    asset,
                    x: scene_number(&operation, "x")?,
                    y: scene_number(&operation, "y")?,
                    width: required_dimension(&operation, "width")?,
                    height: required_dimension(&operation, "height")?,
                }
            }
            "layer" => {
                let transform = operation
                    .get::<Option<Table>>("transform")?
                    .map(|transform| decode_transform(&transform))
                    .transpose()?
                    .unwrap_or_default();
                let clip = operation
                    .get::<Option<Table>>("clip")?
                    .map(|clip| -> Result<ClipRect, RuntimeError> {
                        Ok(ClipRect {
                            x: scene_number(&clip, "x")?,
                            y: scene_number(&clip, "y")?,
                            width: scene_number(&clip, "width")?,
                            height: scene_number(&clip, "height")?,
                        })
                    })
                    .transpose()?;
                PaintOp::Layer {
                    clip,
                    transform,
                    opacity: operation.get::<Option<f32>>("opacity")?.unwrap_or(1.0),
                    operations: self.operations(operation.get("paint")?, depth + 1)?,
                }
            }
            other => {
                return Err(RuntimeError::Invalid(format!(
                    "unknown paint operation: {other}"
                )))
            }
        })
    }
}

fn decode_transform(table: &Table) -> Result<Transform2D, RuntimeError> {
    Ok(Transform2D {
        translate_x: optional_scene_number(table, "translate_x")?.unwrap_or(0.0),
        translate_y: optional_scene_number(table, "translate_y")?.unwrap_or(0.0),
        scale_x: optional_scene_number(table, "scale_x")?.unwrap_or(1.0),
        scale_y: optional_scene_number(table, "scale_y")?.unwrap_or(1.0),
        rotation_degrees: optional_scene_number(table, "rotation_degrees")?.unwrap_or(0.0),
    })
}

fn decode_interaction(table: Option<Table>) -> Result<Interaction, RuntimeError> {
    let Some(table) = table else {
        return Ok(Interaction::default());
    };
    let mut hit_regions = Vec::new();
    if let Some(region_table) = table.get::<Option<Table>>("hit_regions")? {
        for region in region_table.sequence_values::<Table>() {
            if hit_regions.len() >= MAX_HIT_REGIONS {
                return Err(RuntimeError::Invalid(
                    "scene node has too many hit regions".into(),
                ));
            }
            let region = region?;
            hit_regions.push(HitRegion {
                id: required_bounded_string(&region, "id", 256)?,
                x: scene_number(&region, "x")?,
                y: scene_number(&region, "y")?,
                width: scene_number(&region, "width")?,
                height: scene_number(&region, "height")?,
                press_action: bounded_optional_string(&region, "press_action", 256)?,
                drag_action: bounded_optional_string(&region, "drag_action", 256)?,
                drop_action: bounded_optional_string(&region, "drop_action", 256)?,
                tap_action: bounded_optional_string(&region, "tap_action", 256)?,
                double_tap_action: bounded_optional_string(&region, "double_tap_action", 256)?,
                long_press_action: bounded_optional_string(&region, "long_press_action", 256)?,
                swipe_action: bounded_optional_string(&region, "swipe_action", 256)?,
            });
        }
    }
    Ok(Interaction {
        tap_action: bounded_optional_string(&table, "tap_action", 256)?,
        double_tap_action: bounded_optional_string(&table, "double_tap_action", 256)?,
        long_press_action: bounded_optional_string(&table, "long_press_action", 256)?,
        swipe_action: bounded_optional_string(&table, "swipe_action", 256)?,
        pointer_action: bounded_optional_string(&table, "pointer_action", 256)?,
        multi_pointer_action: bounded_optional_string(&table, "multi_pointer_action", 256)?,
        capture: match table.get::<Option<String>>("capture")?.as_deref() {
            None | Some("none") => PointerCapture::None,
            Some("pointer") => PointerCapture::Pointer,
            Some("surface") => PointerCapture::Surface,
            Some(value) => {
                return Err(RuntimeError::Invalid(format!(
                    "invalid pointer capture policy: {value}"
                )))
            }
        },
        hit_regions,
    })
}

fn scene_number(table: &Table, key: &'static str) -> Result<f32, RuntimeError> {
    let value: f32 = table.get(key)?;
    if !value.is_finite() || !(-10_000.0..=10_000.0).contains(&value) {
        return Err(RuntimeError::Invalid(format!("invalid scene {key}")));
    }
    Ok(value)
}

fn optional_scene_number(table: &Table, key: &'static str) -> Result<Option<f32>, RuntimeError> {
    let value = table.get::<Option<f32>>(key)?;
    if value.is_some_and(|value| !value.is_finite() || !(-10_000.0..=10_000.0).contains(&value)) {
        return Err(RuntimeError::Invalid(format!("invalid scene {key}")));
    }
    Ok(value)
}

fn decode_animation(table: Option<Table>) -> Result<Option<Animation>, RuntimeError> {
    let Some(table) = table else {
        return Ok(None);
    };
    let kind = match required_bounded_string(&table, "kind", 32)?.as_str() {
        "pulse" => AnimationKind::Pulse,
        "fade_in" => AnimationKind::FadeIn,
        value => {
            return Err(RuntimeError::Invalid(format!(
                "invalid animation kind: {value}"
            )))
        }
    };
    let duration_ms = table.get::<u64>("duration_ms")?;
    if !(16..=60_000).contains(&duration_ms) {
        return Err(RuntimeError::Invalid("invalid animation duration".into()));
    }
    Ok(Some(Animation {
        kind,
        duration_ms,
        repeat: table.get::<Option<bool>>("loop")?.unwrap_or(false),
    }))
}

fn decode_semantics(table: Option<Table>) -> Result<Option<Semantics>, RuntimeError> {
    let Some(table) = table else {
        return Ok(None);
    };
    let role = match required_bounded_string(&table, "role", 32)?.as_str() {
        "button" => SemanticRole::Button,
        "image" => SemanticRole::Image,
        "text_field" => SemanticRole::TextField,
        "header" => SemanticRole::Header,
        "status" => SemanticRole::Status,
        "scroll_area" => SemanticRole::ScrollArea,
        value => {
            return Err(RuntimeError::Invalid(format!(
                "invalid semantic role: {value}"
            )))
        }
    };
    Ok(Some(Semantics {
        role,
        label: required_bounded_string(&table, "label", MAX_TEXT_BYTES)?,
        value: bounded_optional_string(&table, "value", MAX_TEXT_BYTES)?,
        hint: bounded_optional_string(&table, "hint", MAX_TEXT_BYTES)?,
    }))
}

fn finite_dimension(table: &Table, key: &'static str) -> Result<Option<f32>, RuntimeError> {
    let value = table.get::<Option<f32>>(key)?;
    if value.is_some_and(|value| !(0.0..=10_000.0).contains(&value)) {
        return Err(RuntimeError::Invalid(format!("invalid {key}")));
    }
    Ok(value)
}

fn layout_fraction(table: &Table, key: &'static str) -> Result<Option<f32>, RuntimeError> {
    let value = table.get::<Option<f32>>(key)?;
    if value.is_some_and(|value| !value.is_finite() || !(-4.0..=4.0).contains(&value)) {
        return Err(RuntimeError::Invalid(format!(
            "invalid layout program fraction: {key}"
        )));
    }
    Ok(value)
}

fn required_dimension(table: &Table, key: &'static str) -> Result<f32, RuntimeError> {
    let value = table.get::<f32>(key)?;
    if !value.is_finite() || !(0.0..=10_000.0).contains(&value) || value == 0.0 {
        return Err(RuntimeError::Invalid(format!("invalid {key}")));
    }
    Ok(value)
}

fn bounded_optional_string(
    table: &Table,
    key: &'static str,
    max: usize,
) -> Result<Option<String>, RuntimeError> {
    let value = table.get::<Option<String>>(key)?;
    if value.as_ref().is_some_and(|value| value.len() > max) {
        return Err(RuntimeError::Invalid(format!("{key} is too long")));
    }
    Ok(value)
}

fn required_bounded_string(
    table: &Table,
    key: &'static str,
    max: usize,
) -> Result<String, RuntimeError> {
    bounded_optional_string(table, key, max)?
        .ok_or_else(|| RuntimeError::Invalid(format!("{key} is required")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"
        return {
            api_version = 3,
            render = function(model, state)
                return {
                    id = "root", layout = { flow = "column", gap = 8 }, children = {
                        { content = { kind = "text", value = model.weather.summary, size = 16, color = 0xffffff } },
                        {
                            id = "toggle",
                            content = { kind = "text", value = state.on and "on" or "off", size = 16, color = 0xffffff },
                            interaction = { tap_action = "toggle" },
                        },
                    }
                }
            end,
            update = function(_, state, event)
                if event.action == "toggle" then state.on = not state.on end
                return state
            end,
        }
    "#;

    #[test]
    fn renders_and_updates_a_typed_scene() {
        let runtime = LuauRuntime::compile(SCRIPT).unwrap();
        let model = providers_fake_for_test();
        let mut state = runtime.initial_state();
        let scene = runtime.render(&model, &state).unwrap();
        assert_eq!(scene.root.children.len(), 2);
        state = runtime
            .update(
                &model,
                &state,
                &SceneEvent {
                    action: "toggle".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(state["on"], true);
    }

    #[test]
    fn rejects_catalog_abi_sources_instead_of_adapting_them() {
        let missing = LuauRuntime::compile("return { render = function() return {} end }")
            .err()
            .unwrap()
            .to_string();
        assert!(missing.contains("module must export api_version"));

        let version_one = LuauRuntime::compile(
            "return { api_version = 1, render = function() return { type = 'box' } end }",
        )
        .err()
        .unwrap()
        .to_string();
        assert!(version_one.contains("host requires 3"));
    }

    #[test]
    fn interrupts_an_infinite_render() {
        let runtime = LuauRuntime::compile(
            "return { api_version = 3, render = function() while true do end end }",
        )
        .unwrap();
        let error = runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap_err();
        assert!(error.to_string().contains("time budget"));
    }

    #[test]
    fn rejects_unknown_content() {
        let runtime = LuauRuntime::compile(
            "return { api_version = 3, render = function() return { content = { kind = 'native_surface' } } end }",
        )
        .unwrap();
        assert!(runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap_err()
            .to_string()
            .contains("unknown content kind"));
    }

    #[test]
    fn decodes_bounded_native_primitives_and_semantics() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function()
                        return {
                            id = "root", layout = { flow = "column" }, children = {
                                {
                                    id = "art", content = { kind = "image", asset = "album-orbit" },
                                    animation = { kind = "pulse", duration_ms = 1200, loop = true },
                                    semantics = { role = "image", label = "Album art" },
                                },
                                {
                                    id = "draft",
                                    content = {
                                        kind = "text_session", state_key = "draft",
                                        value = "Caffè ☕️ – 明日のデザイン", autofocus = true,
                                        submit_action = "save_note",
                                    },
                                    semantics = {
                                        role = "text_field", label = "Note draft",
                                        value = "Caffè ☕️ – 明日のデザイン",
                                    },
                                },
                            },
                        }
                    end,
                }
            "#,
        )
        .unwrap();
        let scene = runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap();
        assert_eq!(scene.root.children.len(), 2);
        assert!(matches!(
            scene.root.children[0].content,
            Some(Content::Image(_))
        ));
        assert!(scene.root.children[0].animation.is_some());
        assert!(matches!(
            scene.root.children[1].content,
            Some(Content::TextSession(_))
        ));
        assert!(scene.root.children[1].semantics.is_some());
    }

    #[test]
    fn rejects_undeclared_image_assets() {
        let runtime = LuauRuntime::compile(
            "return { api_version = 3, render = function() return { id = 'x', content = { kind = 'image', asset = 'https://example.com/x.png' } } end }",
        )
        .unwrap();
        assert!(runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap_err()
            .to_string()
            .contains("image asset is not declared by this revision"));
    }

    #[test]
    fn decodes_retained_layout_layers_glyphs_gestures_and_revision_assets() {
        let runtime = LuauRuntime::compile(
            r##"
                return {
                    api_version = 3,
                    assets = {
                        mark = { kind = "svg", data = [[<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" fill="#fff"/></svg>]] },
                    },
                    render = function()
                        return {
                            id = "surface",
                            layout = {
                                width = 320, height = 480, min_width = 280, max_width = 360,
                                aspect_ratio = 0.6666667, clip_bounds = true,
                                position = { x = 4, y = 8 },
                                program = { measure_width = 0.75, arrange_x = 0.125 },
                            },
                            paint = {{
                                kind = "layer", opacity = 0.8,
                                clip = { x = 0, y = 0, width = 200, height = 100 },
                                transform = { translate_x = 2, scale_x = 1.1, scale_y = 1.1, rotation_degrees = 3 },
                                paint = {{ kind = "glyphs", x = 8, y = 12, size = 16, runs = {
                                    { text = "SOS", color = 0xffffff, weight = 700, italic = true },
                                } }},
                            }},
                            interaction = {
                                tap_action = "tap", double_tap_action = "zoom",
                                long_press_action = "pin", swipe_action = "swipe",
                                pointer_action = "pointer", multi_pointer_action = "pinch",
                                capture = "surface",
                            },
                            children = {{ id = "mark", content = { kind = "image", asset = "mark" } }},
                        }
                    end,
                }
            "##,
        )
        .unwrap();
        assert_eq!(runtime.assets().len(), 1);
        assert_eq!(runtime.assets()[0].id, "mark");
        assert!(runtime.assets()[0].path.starts_with("sos/revisions/"));
        let scene = runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap();
        assert_eq!(scene.root.layout.position.unwrap().x, 4.0);
        assert_eq!(scene.root.layout.program.unwrap().measure_width, Some(0.75));
        assert!(scene.root.layout.clip_bounds);
        assert!(matches!(scene.root.paint[0], PaintOp::Layer { .. }));
        assert_eq!(
            scene.root.interaction.double_tap_action.as_deref(),
            Some("zoom")
        );
        assert_eq!(scene.root.interaction.capture, PointerCapture::Surface);
        let Some(Content::Image(image)) = &scene.root.children[0].content else {
            panic!("expected revision image")
        };
        assert_eq!(image.asset, runtime.assets()[0].path);
    }

    #[test]
    fn sidecar_images_fonts_and_shaders_enter_one_runtime_asset_set() {
        let runtime = LuauRuntime::compile_with_assets(
            r#"return {
                api_version = 3,
                render = function()
                    return {
                        id = "hero",
                        content = { kind = "image", asset = "hero" },
                        paint = {{ kind = "shader", asset = "glow", x = 0, y = 0, width = 32, height = 16 }},
                    }
                end,
            }"#,
            vec![
                RevisionAssetInput {
                    id: "hero".into(),
                    kind: "png".into(),
                    bytes: b"\x89PNG\r\n\x1a\nfixture".to_vec(),
                },
                RevisionAssetInput {
                    id: "display".into(),
                    kind: "font".into(),
                    bytes: b"OTTOfixture".to_vec(),
                },
                RevisionAssetInput {
                    id: "glow".into(),
                    kind: "shader".into(),
                    bytes: test_shader().as_bytes().to_vec(),
                },
            ],
        )
        .unwrap();
        assert_eq!(runtime.assets().len(), 3);
        let scene = runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap();
        let Some(Content::Image(image)) = scene.root.content else {
            panic!("expected sidecar image")
        };
        assert!(image.asset.ends_with(".png"));
        assert!(matches!(
            &scene.root.paint[0],
            PaintOp::Shader { asset, width, height, .. }
                if asset.ends_with(".wgsl") && *width == 32.0 && *height == 16.0
        ));
    }

    fn test_shader() -> &'static str {
        r#"
            @vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
                let x = f32((index << 1u) & 2u);
                let y = f32(index & 2u);
                return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
            }
            @fragment fn fs_main() -> @location(0) vec4<f32> {
                return vec4<f32>(0.2, 0.5, 0.9, 1.0);
            }
        "#
    }

    #[test]
    fn rejects_active_content_in_revision_svg_assets() {
        let error = match LuauRuntime::compile(
            r#"return { api_version = 3, assets = { bad = { kind = "svg", data = "<svg><script>bad()</script></svg>" } }, render = function() return { id = "root" } end }"#,
        ) {
            Ok(_) => panic!("active SVG content should be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("external/active feature"));
    }

    #[test]
    fn decodes_paint_geometry_hit_regions_and_pointer_events() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function(_, state)
                        return {
                            id = "time-space",
                            layout = { width = 320, height = 480 },
                            paint = {
                                { kind = "path", color = 0x77AAFF, width = 4,
                                  points = {{x = 24, y = 20}, {x = 92, y = 180}, {x = 40, y = 420}} },
                                { kind = "quad", x = state.x or 40, y = 300,
                                  width = 100, height = 48, radius = 12, color = 0x223355 },
                            },
                            interaction = { hit_regions = {{
                                    id = "note-1", x = state.x or 40, y = 300, width = 100, height = 48,
                                    press_action = "note_press", drag_action = "note_drag", drop_action = "note_drop",
                                }}
                            },
                        }
                    end,
                    update = function(_, state, event)
                        if event.action == "note_drag" then state.x = event.x - 50 end
                        return state
                    end,
                }
            "#,
        )
        .unwrap();
        let model = providers_fake_for_test();
        let scene = runtime.render(&model, &json!({})).unwrap();
        assert_eq!(scene.root.paint.len(), 2);
        assert_eq!(
            scene.root.interaction.hit_regions[0].drop_action.as_deref(),
            Some("note_drop")
        );
        let state = runtime
            .update(
                &model,
                &json!({}),
                &SceneEvent {
                    action: "note_drag".into(),
                    x: Some(180.0),
                    y: Some(300.0),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(state["x"], 130.0);
    }

    #[test]
    fn returns_a_bounded_typed_provider_effect() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function() return { id = "root" } end,
                    update = function(_, state)
                        state.attached = true
                        return {
                            state = state,
                            effects = {{
                                provider = "notes", action = "attach_to_event",
                                payload = { note_id = "note-1", event_title = "Design review" },
                            }},
                        }
                    end,
                }
            "#,
        )
        .unwrap();
        let outcome = runtime
            .update_with_effects(
                &providers_fake_for_test(),
                &json!({}),
                &SceneEvent {
                    action: "drop".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(outcome.state["attached"], true);
        assert_eq!(outcome.effects.len(), 1);
        assert_eq!(outcome.effects[0].provider, "notes");
    }

    #[test]
    fn returns_the_bounded_agent_prompt_capability() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function() return { id = "root" } end,
                    update = function(_, state, event)
                        return {
                            state = state,
                            effects = {{
                                provider = "agent", action = "prompt",
                                payload = { prompt = event.value },
                            }},
                        }
                    end,
                }
            "#,
        )
        .unwrap();
        let outcome = runtime
            .update_with_effects(
                &providers_fake_for_test(),
                &json!({}),
                &SceneEvent {
                    action: "agent_submit".into(),
                    value: Some("Make this calmer".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(outcome.effects.len(), 1);
        assert_eq!(outcome.effects[0].provider, "agent");
        assert_eq!(outcome.effects[0].action, "prompt");
        assert_eq!(outcome.effects[0].payload["prompt"], "Make this calmer");
    }

    #[test]
    fn returns_the_bounded_network_selection_capability() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function() return { id = "root" } end,
                    update = function(_, state)
                        return {
                            state = state,
                            effects = {{
                                provider = "network", action = "connect",
                                payload = { ssid = "SOS Lab", security = "personal" },
                            }},
                        }
                    end,
                }
            "#,
        )
        .unwrap();
        let outcome = runtime
            .update_with_effects(
                &providers_fake_for_test(),
                &json!({}),
                &SceneEvent {
                    action: "wifi_connect_1".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(outcome.effects.len(), 1);
        assert_eq!(outcome.effects[0].provider, "network");
        assert_eq!(outcome.effects[0].action, "connect");
        assert_eq!(outcome.effects[0].payload["ssid"], "SOS Lab");
        assert_eq!(outcome.effects[0].payload["security"], "personal");
    }

    #[test]
    fn returns_only_the_bounded_agent_credential_controls() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function() return { id = "root" } end,
                    update = function(_, state, event)
                        return { state = state, effects = {{
                            provider = "agent", action = event.action,
                        }} }
                    end,
                }
            "#,
        )
        .unwrap();
        for action in [
            "configure_openai",
            "configure_openrouter",
            "configure_codex",
            "use_fake",
            "clear_credential",
        ] {
            let outcome = runtime
                .update_with_effects(
                    &providers_fake_for_test(),
                    &json!({}),
                    &SceneEvent {
                        action: action.into(),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(outcome.effects[0].provider, "agent");
            assert_eq!(outcome.effects[0].action, action);
        }
    }

    #[test]
    fn runs_an_explicit_bounded_state_schema_migration() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    state_version = 2,
                    migrate = function(from_version, state)
                        if from_version ~= 1 then error("unexpected source schema") end
                        return { playing = state.playing, migrated_from = from_version }
                    end,
                    render = function() return {} end,
                }
            "#,
        )
        .unwrap();
        assert_eq!(runtime.state_schema_version().unwrap(), 2);
        assert_eq!(
            runtime
                .migrate_state(1, &json!({ "playing": true }))
                .unwrap(),
            json!({ "playing": true, "migrated_from": 1 })
        );
        assert!(runtime.migrate_state(3, &json!({})).is_err());
    }

    #[test]
    fn rejects_a_schema_change_without_a_migration() {
        let runtime = LuauRuntime::compile(
            r#"return { api_version = 3, state_version = 2, render = function() return {} end }"#,
        )
        .unwrap();
        assert!(runtime.migrate_state(1, &json!({})).is_err());
    }

    #[test]
    fn rejects_invalid_source() {
        assert!(LuauRuntime::compile("return {").is_err());
    }

    #[test]
    fn standard_io_and_package_loading_are_not_exposed() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function()
                        if io ~= nil or package ~= nil or (os ~= nil and os.execute ~= nil) then
                            error("privileged library exposed")
                        end
                        return {}
                    end,
                }
            "#,
        )
        .unwrap();
        runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap();
    }

    #[test]
    fn rejects_a_resource_bomb() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    api_version = 3,
                    render = function()
                        local values = {}
                        while true do
                            table.insert(values, string.rep("x", 1024 * 1024))
                        end
                    end,
                }
            "#,
        )
        .unwrap();
        let error = runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("memory") || error.contains("time budget"));
    }

    #[test]
    fn worker_owns_the_vm_and_commits_transactionally() {
        let caller_thread = format!("{:?}", thread::current().id());
        let model = providers_fake_for_test();
        let (worker, ready) =
            RuntimeWorker::start(SCRIPT.into(), model.clone(), json!({}), 1).unwrap();
        assert_ne!(ready.worker_thread, caller_thread);
        assert_eq!(ready.scene.root.children.len(), 2);

        let results = worker.results();
        worker
            .prepare_candidate(
                1,
                format!("{SCRIPT}\n-- revision 1"),
                model.clone(),
                json!({}),
                1,
                Instant::now(),
            )
            .unwrap();
        assert!(matches!(
            results.recv_blocking().unwrap(),
            WorkerResult::CandidatePrepared { request_id: 1, .. }
        ));
        worker.commit_candidate(1).unwrap();
        assert!(matches!(
            results.recv_blocking().unwrap(),
            WorkerResult::CandidateCommitted { request_id: 1, .. }
        ));

        worker
            .prepare_candidate(
                2,
                "return { api_version = 3, render = function() while true do end end }".into(),
                model.clone(),
                json!({}),
                1,
                Instant::now(),
            )
            .unwrap();
        assert!(matches!(
            results.recv_blocking().unwrap(),
            WorkerResult::CandidateRejected { request_id: 2, .. }
        ));

        worker
            .action(
                3,
                model,
                json!({}),
                SceneEvent {
                    action: "toggle".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        match results.recv_blocking().unwrap() {
            WorkerResult::ActionCompleted {
                request_id, state, ..
            } => {
                assert_eq!(request_id, 3);
                assert_eq!(state["on"], true);
            }
            result => panic!("unexpected result: {result:?}"),
        }
    }

    #[test]
    fn worker_prepares_each_candidate_with_its_own_sidecars() {
        fn source(asset: &str) -> String {
            format!(
                r#"return {{
                    api_version = 3,
                    render = function()
                        return {{ id = "root", content = {{ kind = "image", asset = "{asset}" }} }}
                    end,
                }}"#
            )
        }

        fn image(id: &str) -> RevisionAssetInput {
            RevisionAssetInput {
                id: id.into(),
                kind: "png".into(),
                bytes: b"\x89PNG\r\n\x1a\nfixture".to_vec(),
            }
        }

        let model = providers_fake_for_test();
        let (worker, ready) = RuntimeWorker::start_with_assets(
            source("boot-image"),
            model.clone(),
            json!({}),
            1,
            vec![image("boot-image")],
        )
        .unwrap();
        assert_eq!(ready.assets[0].id, "boot-image");

        let results = worker.results();
        worker
            .prepare_candidate_with_assets(
                1,
                source("candidate-image"),
                vec![image("candidate-image")],
                model.clone(),
                json!({}),
                1,
                Instant::now(),
            )
            .unwrap();
        match results.recv_blocking().unwrap() {
            WorkerResult::CandidatePrepared { assets, .. } => {
                assert_eq!(assets.len(), 1);
                assert_eq!(assets[0].id, "candidate-image");
            }
            result => panic!("unexpected result: {result:?}"),
        }
        worker.discard_candidate(1).unwrap();

        worker
            .prepare_candidate(2, source("boot-image"), model, json!({}), 1, Instant::now())
            .unwrap();
        assert!(matches!(
            results.recv_blocking().unwrap(),
            WorkerResult::CandidateRejected { request_id: 2, .. }
        ));
    }

    #[test]
    fn worker_shuts_down_and_recreates_cleanly() {
        let model = providers_fake_for_test();
        for _ in 0..25 {
            let (worker, ready) =
                RuntimeWorker::start(SCRIPT.into(), model.clone(), json!({}), 1).unwrap();
            assert_eq!(ready.scene.root.children.len(), 2);
            worker.shutdown().unwrap();
        }
    }

    fn providers_fake_for_test() -> ExperienceModel {
        ExperienceModel {
            greeting: "Hello".into(),
            date: "Today".into(),
            weather: experience_ir::Weather {
                summary: "Clear".into(),
                temperature_c: 20,
                high_c: 22,
                low_c: 10,
            },
            calendar: vec![],
            notes: vec![],
            music: experience_ir::Music {
                title: "Track".into(),
                artist: "Artist".into(),
                playing: true,
            },
            system: experience_ir::SystemState::default(),
            surfaces: Vec::new(),
            agent: Default::default(),
            network: Default::default(),
            providers: Default::default(),
        }
    }
}
