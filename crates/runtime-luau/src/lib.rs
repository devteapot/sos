use std::{
    cell::Cell,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use experience_ir::{
    validate_tree, Accessibility, AccessibilityRole, Align, Animation, AnimationKind, Canvas,
    CanvasCommand, CanvasPoint, ExperienceModel, HitRegion, Image, Justify, NodeKind,
    ProviderEffect, Style, TextInput, UiEvent, UiNode, MAX_CANVAS_COMMANDS, MAX_CANVAS_POINTS,
    MAX_CHILDREN, MAX_EFFECTS, MAX_EFFECT_PAYLOAD_BYTES, MAX_HIT_REGIONS, MAX_STATE_BYTES,
    MAX_TEXT_BYTES, MAX_TREE_DEPTH, MAX_TREE_NODES,
};
use mlua::{
    chunk::{ChunkMode, Compiler},
    Error as LuaError, Function, Lua, LuaSerdeExt, RegistryKey, Table, Value, VmState,
};
use serde_json::{json, Value as JsonValue};
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
    pub tree: UiNode,
    pub state: JsonValue,
    pub state_schema_version: u64,
    pub worker_thread: String,
    pub initialize_us: u64,
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
        tree: UiNode,
        state: JsonValue,
        state_schema_version: u64,
        timings: CandidateTimings,
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
        tree: UiNode,
        state: JsonValue,
        state_schema_version: u64,
        timings: CandidateTimings,
    },
    ActionCompleted {
        request_id: u64,
        state: JsonValue,
        tree: UiNode,
        effects: Vec<ProviderEffect>,
        worker_us: u64,
    },
    ActionRejected {
        request_id: u64,
        error: String,
        worker_us: u64,
    },
}

enum WorkerCommand {
    PrepareCandidate {
        request_id: u64,
        source: String,
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
        event: UiEvent,
    },
    Shutdown,
}

struct PreparedCandidate {
    request_id: u64,
    source: String,
    runtime: LuauRuntime,
    tree: UiNode,
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
        let (commands_tx, commands_rx) = async_channel::unbounded();
        let (results_tx, results_rx) = async_channel::unbounded();
        let (ready_tx, ready_rx) = async_channel::bounded::<Result<WorkerReady, String>>(1);

        let thread = thread::Builder::new()
            .name("sos-luau-runtime".into())
            .spawn(move || {
                let started = Instant::now();
                let initialized = LuauRuntime::compile(&source).and_then(|runtime| {
                    let state = runtime.migrate_state(state_schema_version, &state)?;
                    let state_schema_version = runtime.state_schema_version()?;
                    let tree = runtime.render(&model, &state)?;
                    Ok((runtime, tree, state, state_schema_version))
                });
                let (mut active_runtime, tree, active_state, active_state_schema_version) =
                    match initialized {
                        Ok(initialized) => initialized,
                        Err(error) => {
                            let _ = ready_tx.send_blocking(Err(error.to_string()));
                            return;
                        }
                    };
                let ready = WorkerReady {
                    tree,
                    state: active_state,
                    state_schema_version: active_state_schema_version,
                    worker_thread: format!("{:?}", thread::current().id()),
                    initialize_us: micros(started.elapsed()),
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
                            let candidate_runtime = match LuauRuntime::compile(&source) {
                                Ok(runtime) => runtime,
                                Err(error) => {
                                    let timings = CandidateTimings {
                                        submitted_at,
                                        queue_us,
                                        compile_us: micros(compile_started.elapsed()),
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
                            let tree = match candidate_runtime.render(&model, &state) {
                                Ok(tree) => tree,
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
                            prepared = Some(PreparedCandidate {
                                request_id,
                                source: source.clone(),
                                runtime: candidate_runtime,
                                tree: tree.clone(),
                                state: state.clone(),
                                state_schema_version,
                                timings: timings.clone(),
                            });
                            let _ = results_tx.send_blocking(WorkerResult::CandidatePrepared {
                                request_id,
                                source,
                                tree,
                                state,
                                state_schema_version,
                                timings,
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
                            let _ = results_tx.send_blocking(WorkerResult::CandidateCommitted {
                                request_id,
                                source: candidate.source,
                                tree: candidate.tree,
                                state: candidate.state,
                                state_schema_version: candidate.state_schema_version,
                                timings: candidate.timings,
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
                                    let tree = active_runtime.render(&model, &outcome.state)?;
                                    Ok((outcome, tree))
                                });
                            let worker_us = micros(started.elapsed());
                            let result = match result {
                                Ok((outcome, tree)) => WorkerResult::ActionCompleted {
                                    request_id,
                                    state: outcome.state,
                                    tree,
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
        let (worker, ready_rx) = Self::spawn(source, model, state, state_schema_version)?;
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
        self.commands
            .send_blocking(WorkerCommand::PrepareCandidate {
                request_id,
                source,
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
        event: UiEvent,
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

        let module = {
            let result = run_bounded(&deadline, RENDER_BUDGET, || {
                lua.load(source)
                    .set_name("experience")
                    .set_mode(ChunkMode::Text)
                    .eval::<Table>()
            })?;
            let _: Function = result.get("render").map_err(|_| {
                RuntimeError::Invalid("module must export render(model, state)".into())
            })?;
            lua.create_registry_value(result)?
        };

        Ok(Self {
            lua,
            module,
            deadline,
        })
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
    ) -> Result<UiNode, RuntimeError> {
        let module: Table = self.lua.registry_value(&self.module)?;
        let render: Function = module.get("render")?;
        let model = self.lua.to_value(model)?;
        let state = self.lua.to_value(state)?;
        let value = run_bounded(&self.deadline, RENDER_BUDGET, || {
            render.call::<Value>((model, state))
        })?;
        let root = Decoder::default().node(value, 1)?;
        validate_tree(&root).map_err(|error| RuntimeError::Invalid(error.to_string()))?;
        Ok(root)
    }

    pub fn update(
        &self,
        model: &ExperienceModel,
        state: &JsonValue,
        event: &UiEvent,
    ) -> Result<JsonValue, RuntimeError> {
        Ok(self.update_with_effects(model, state, event)?.state)
    }

    pub fn update_with_effects(
        &self,
        model: &ExperienceModel,
        state: &JsonValue,
        event: &UiEvent,
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
        if (provider.as_str(), action.as_str()) != ("notes", "attach_to_event") {
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

#[derive(Default)]
struct Decoder {
    nodes: usize,
}

impl Decoder {
    fn node(&mut self, value: Value, depth: usize) -> Result<UiNode, RuntimeError> {
        if depth > MAX_TREE_DEPTH {
            return Err(RuntimeError::Invalid("tree is too deep".into()));
        }
        self.nodes += 1;
        if self.nodes > MAX_TREE_NODES {
            return Err(RuntimeError::Invalid("tree has too many nodes".into()));
        }

        let table = value
            .as_table()
            .ok_or_else(|| RuntimeError::Invalid("each UI node must be a table".into()))?;
        let kind_name: String = table.get("type")?;
        let kind = match kind_name.as_str() {
            "box" => NodeKind::Box,
            "column" => NodeKind::Column,
            "row" => NodeKind::Row,
            "scroll" => NodeKind::Scroll,
            "spacer" => NodeKind::Spacer,
            "text" => {
                let text: String = table.get("text")?;
                if text.len() > MAX_TEXT_BYTES {
                    return Err(RuntimeError::Invalid("text is too long".into()));
                }
                NodeKind::Text(text)
            }
            "text_input" => NodeKind::TextInput(TextInput {
                state_key: required_bounded_string(table, "state_key", 256)?,
                value: required_bounded_string(table, "value", MAX_TEXT_BYTES)?,
                placeholder: bounded_optional_string(table, "placeholder", MAX_TEXT_BYTES)?
                    .unwrap_or_default(),
                submit_action: bounded_optional_string(table, "submit_action", 256)?,
                autofocus: table.get::<Option<bool>>("autofocus")?.unwrap_or(false),
            }),
            "image" => {
                let asset = required_bounded_string(table, "asset", 256)?;
                if asset != "album-orbit" {
                    return Err(RuntimeError::Invalid(format!(
                        "image asset is not allowed: {asset}"
                    )));
                }
                NodeKind::Image(Image { asset })
            }
            "canvas" => NodeKind::Canvas(decode_canvas(table)?),
            other => return Err(RuntimeError::Invalid(format!("unknown node type: {other}"))),
        };

        let style = match table.get::<Option<Table>>("style")? {
            Some(style) => decode_style(&style)?,
            None => Style::default(),
        };
        let mut children = Vec::new();
        if let Some(child_table) = table.get::<Option<Table>>("children")? {
            for child in child_table.sequence_values::<Value>() {
                if children.len() >= MAX_CHILDREN {
                    return Err(RuntimeError::Invalid("node has too many children".into()));
                }
                children.push(self.node(child?, depth + 1)?);
            }
        }

        Ok(UiNode {
            id: bounded_optional_string(table, "id", 256)?,
            kind,
            style,
            action: bounded_optional_string(table, "action", 256)?,
            animation: decode_animation(table.get::<Option<Table>>("animation")?)?,
            accessibility: decode_accessibility(table.get::<Option<Table>>("accessibility")?)?,
            children,
        })
    }
}

fn decode_canvas(table: &Table) -> Result<Canvas, RuntimeError> {
    let mut commands = Vec::new();
    let mut point_count = 0usize;
    if let Some(command_table) = table.get::<Option<Table>>("commands")? {
        for command in command_table.sequence_values::<Table>() {
            if commands.len() >= MAX_CANVAS_COMMANDS {
                return Err(RuntimeError::Invalid("canvas has too many commands".into()));
            }
            let command = command?;
            let kind = required_bounded_string(&command, "kind", 32)?;
            commands.push(match kind.as_str() {
                "path" => {
                    let points_table: Table = command.get("points")?;
                    let mut points = Vec::new();
                    for point in points_table.sequence_values::<Table>() {
                        point_count += 1;
                        if point_count > MAX_CANVAS_POINTS {
                            return Err(RuntimeError::Invalid("canvas has too many points".into()));
                        }
                        let point = point?;
                        points.push(CanvasPoint {
                            x: canvas_number(&point, "x")?,
                            y: canvas_number(&point, "y")?,
                        });
                    }
                    CanvasCommand::Path {
                        points,
                        color: command.get("color")?,
                        width: optional_canvas_number(&command, "width")?,
                        closed: command.get::<Option<bool>>("closed")?.unwrap_or(false),
                    }
                }
                "quad" => CanvasCommand::Quad {
                    x: canvas_number(&command, "x")?,
                    y: canvas_number(&command, "y")?,
                    width: canvas_number(&command, "width")?,
                    height: canvas_number(&command, "height")?,
                    radius: optional_canvas_number(&command, "radius")?.unwrap_or_default(),
                    color: command.get("color")?,
                },
                other => {
                    return Err(RuntimeError::Invalid(format!(
                        "unknown canvas command: {other}"
                    )))
                }
            });
        }
    }

    let mut hit_regions = Vec::new();
    if let Some(region_table) = table.get::<Option<Table>>("hit_regions")? {
        for region in region_table.sequence_values::<Table>() {
            if hit_regions.len() >= MAX_HIT_REGIONS {
                return Err(RuntimeError::Invalid(
                    "canvas has too many hit regions".into(),
                ));
            }
            let region = region?;
            hit_regions.push(HitRegion {
                id: required_bounded_string(&region, "id", 256)?,
                x: canvas_number(&region, "x")?,
                y: canvas_number(&region, "y")?,
                width: canvas_number(&region, "width")?,
                height: canvas_number(&region, "height")?,
                press_action: bounded_optional_string(&region, "press_action", 256)?,
                drag_action: bounded_optional_string(&region, "drag_action", 256)?,
                drop_action: bounded_optional_string(&region, "drop_action", 256)?,
            });
        }
    }

    Ok(Canvas {
        commands,
        hit_regions,
    })
}

fn canvas_number(table: &Table, key: &'static str) -> Result<f32, RuntimeError> {
    let value: f32 = table.get(key)?;
    if !value.is_finite() || !(-10_000.0..=10_000.0).contains(&value) {
        return Err(RuntimeError::Invalid(format!("invalid canvas {key}")));
    }
    Ok(value)
}

fn optional_canvas_number(table: &Table, key: &'static str) -> Result<Option<f32>, RuntimeError> {
    let value = table.get::<Option<f32>>(key)?;
    if value.is_some_and(|value| !value.is_finite() || !(-10_000.0..=10_000.0).contains(&value)) {
        return Err(RuntimeError::Invalid(format!("invalid canvas {key}")));
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

fn decode_accessibility(table: Option<Table>) -> Result<Option<Accessibility>, RuntimeError> {
    let Some(table) = table else {
        return Ok(None);
    };
    let role = match required_bounded_string(&table, "role", 32)?.as_str() {
        "button" => AccessibilityRole::Button,
        "image" => AccessibilityRole::Image,
        "text_field" => AccessibilityRole::TextField,
        "header" => AccessibilityRole::Header,
        "status" => AccessibilityRole::Status,
        value => {
            return Err(RuntimeError::Invalid(format!(
                "invalid accessibility role: {value}"
            )))
        }
    };
    Ok(Some(Accessibility {
        role,
        label: required_bounded_string(&table, "label", MAX_TEXT_BYTES)?,
        value: bounded_optional_string(&table, "value", MAX_TEXT_BYTES)?,
        hint: bounded_optional_string(&table, "hint", MAX_TEXT_BYTES)?,
    }))
}

fn decode_style(table: &Table) -> Result<Style, RuntimeError> {
    Ok(Style {
        background: table.get("background")?,
        color: table.get("color")?,
        padding: finite_dimension(table, "padding")?,
        gap: finite_dimension(table, "gap")?,
        radius: finite_dimension(table, "radius")?,
        text_size: finite_dimension(table, "text_size")?,
        width: finite_dimension(table, "width")?,
        height: finite_dimension(table, "height")?,
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

fn finite_dimension(table: &Table, key: &'static str) -> Result<Option<f32>, RuntimeError> {
    let value = table.get::<Option<f32>>(key)?;
    if value.is_some_and(|value| !(0.0..=10_000.0).contains(&value)) {
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
            render = function(model, state)
                return {
                    type = "column", id = "root", style = { gap = 8 }, children = {
                        { type = "text", text = model.weather.summary },
                        { type = "text", text = state.on and "on" or "off", action = "toggle" },
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
    fn renders_and_updates_a_typed_tree() {
        let runtime = LuauRuntime::compile(SCRIPT).unwrap();
        let model = providers_fake_for_test();
        let mut state = runtime.initial_state();
        let tree = runtime.render(&model, &state).unwrap();
        assert_eq!(tree.children.len(), 2);
        state = runtime
            .update(
                &model,
                &state,
                &UiEvent {
                    action: "toggle".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(state["on"], true);
    }

    #[test]
    fn interrupts_an_infinite_render() {
        let runtime =
            LuauRuntime::compile("return { render = function() while true do end end }").unwrap();
        let error = runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap_err();
        assert!(error.to_string().contains("time budget"));
    }

    #[test]
    fn rejects_unknown_nodes() {
        let runtime = LuauRuntime::compile(
            "return { render = function() return { type = 'native_surface' } end }",
        )
        .unwrap();
        assert!(runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap_err()
            .to_string()
            .contains("unknown node type"));
    }

    #[test]
    fn decodes_bounded_native_primitives_and_semantics() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    render = function()
                        return {
                            type = "column", id = "root", children = {
                                {
                                    type = "image", id = "art", asset = "album-orbit",
                                    animation = { kind = "pulse", duration_ms = 1200, loop = true },
                                    accessibility = { role = "image", label = "Album art" },
                                },
                                {
                                    type = "text_input", id = "draft", state_key = "draft",
                                    value = "Caffè ☕️ – 明日のデザイン", autofocus = true,
                                    submit_action = "save_note",
                                    accessibility = {
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
        let tree = runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap();
        assert_eq!(tree.children.len(), 2);
        assert!(matches!(tree.children[0].kind, NodeKind::Image(_)));
        assert!(tree.children[0].animation.is_some());
        assert!(matches!(tree.children[1].kind, NodeKind::TextInput(_)));
        assert!(tree.children[1].accessibility.is_some());
    }

    #[test]
    fn rejects_non_allowlisted_image_assets() {
        let runtime = LuauRuntime::compile(
            "return { render = function() return { type = 'image', id = 'x', asset = 'https://example.com/x.png' } end }",
        )
        .unwrap();
        assert!(runtime
            .render(&providers_fake_for_test(), &json!({}))
            .unwrap_err()
            .to_string()
            .contains("image asset is not allowed"));
    }

    #[test]
    fn decodes_canvas_geometry_hit_regions_and_pointer_events() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    render = function(_, state)
                        return {
                            type = "canvas", id = "time-space",
                            style = { width = 320, height = 480 },
                            commands = {
                                { kind = "path", color = 0x77AAFF, width = 4,
                                  points = {{x = 24, y = 20}, {x = 92, y = 180}, {x = 40, y = 420}} },
                                { kind = "quad", x = state.x or 40, y = 300,
                                  width = 100, height = 48, radius = 12, color = 0x223355 },
                            },
                            hit_regions = {{
                                id = "note-1", x = state.x or 40, y = 300, width = 100, height = 48,
                                press_action = "note_press", drag_action = "note_drag", drop_action = "note_drop",
                            }},
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
        let tree = runtime.render(&model, &json!({})).unwrap();
        let NodeKind::Canvas(canvas) = tree.kind else {
            panic!("expected canvas")
        };
        assert_eq!(canvas.commands.len(), 2);
        assert_eq!(
            canvas.hit_regions[0].drop_action.as_deref(),
            Some("note_drop")
        );
        let state = runtime
            .update(
                &model,
                &json!({}),
                &UiEvent {
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
                    render = function() return { type = "box", id = "root" } end,
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
                &UiEvent {
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
    fn runs_an_explicit_bounded_state_schema_migration() {
        let runtime = LuauRuntime::compile(
            r#"
                return {
                    state_version = 2,
                    migrate = function(from_version, state)
                        if from_version ~= 1 then error("unexpected source schema") end
                        return { playing = state.playing, migrated_from = from_version }
                    end,
                    render = function() return { type = "box" } end,
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
            r#"return { state_version = 2, render = function() return { type = "box" } end }"#,
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
                    render = function()
                        if io ~= nil or package ~= nil or (os ~= nil and os.execute ~= nil) then
                            error("privileged library exposed")
                        end
                        return { type = "box" }
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
        assert_eq!(ready.tree.children.len(), 2);

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
                "return { render = function() while true do end end }".into(),
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
                UiEvent {
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
    fn worker_shuts_down_and_recreates_cleanly() {
        let model = providers_fake_for_test();
        for _ in 0..25 {
            let (worker, ready) =
                RuntimeWorker::start(SCRIPT.into(), model.clone(), json!({}), 1).unwrap();
            assert_eq!(ready.tree.children.len(), 2);
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
        }
    }
}
