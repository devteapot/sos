use std::{
    cell::Cell,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use experience_ir::{
    validate_tree, Align, ExperienceModel, Justify, NodeKind, Style, UiEvent, UiNode, MAX_CHILDREN,
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
    pub worker_thread: String,
    pub initialize_us: u64,
}

#[derive(Clone, Debug)]
pub enum WorkerResult {
    CandidatePrepared {
        request_id: u64,
        source: String,
        tree: UiNode,
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
        timings: CandidateTimings,
    },
    ActionCompleted {
        request_id: u64,
        state: JsonValue,
        tree: UiNode,
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
}

struct PreparedCandidate {
    request_id: u64,
    source: String,
    runtime: LuauRuntime,
    tree: UiNode,
    timings: CandidateTimings,
}

pub struct RuntimeWorker {
    commands: async_channel::Sender<WorkerCommand>,
    results: async_channel::Receiver<WorkerResult>,
}

impl RuntimeWorker {
    pub fn start(
        source: String,
        model: ExperienceModel,
        state: JsonValue,
    ) -> Result<(Self, WorkerReady), RuntimeError> {
        let (commands_tx, commands_rx) = async_channel::unbounded();
        let (results_tx, results_rx) = async_channel::unbounded();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<WorkerReady, String>>(1);

        thread::Builder::new()
            .name("sos-luau-runtime".into())
            .spawn(move || {
                let started = Instant::now();
                let initialized = LuauRuntime::compile(&source).and_then(|runtime| {
                    let tree = runtime.render(&model, &state)?;
                    Ok((runtime, tree))
                });
                let (mut active_runtime, tree) = match initialized {
                    Ok(initialized) => initialized,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                let ready = WorkerReady {
                    tree,
                    worker_thread: format!("{:?}", thread::current().id()),
                    initialize_us: micros(started.elapsed()),
                };
                if ready_tx.send(Ok(ready)).is_err() {
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
                                timings: timings.clone(),
                            });
                            let _ = results_tx.send_blocking(WorkerResult::CandidatePrepared {
                                request_id,
                                source,
                                tree,
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
                            let result =
                                active_runtime
                                    .update(&model, &state, &event)
                                    .and_then(|state| {
                                        let tree = active_runtime.render(&model, &state)?;
                                        Ok((state, tree))
                                    });
                            let worker_us = micros(started.elapsed());
                            let result = match result {
                                Ok((state, tree)) => WorkerResult::ActionCompleted {
                                    request_id,
                                    state,
                                    tree,
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
                    }
                }
            })
            .map_err(|error| RuntimeError::Invalid(format!("could not start worker: {error}")))?;

        let ready = ready_rx
            .recv()
            .map_err(|_| RuntimeError::Invalid("runtime worker stopped during startup".into()))?
            .map_err(RuntimeError::Invalid)?;
        Ok((
            Self {
                commands: commands_tx,
                results: results_rx,
            },
            ready,
        ))
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
        submitted_at: Instant,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(WorkerCommand::PrepareCandidate {
                request_id,
                source,
                model,
                state,
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
        let module: Table = self.lua.registry_value(&self.module)?;
        let update: Option<Function> = module.get("update")?;
        let Some(update) = update else {
            return Ok(state.clone());
        };
        let model = self.lua.to_value(model)?;
        let state = self.lua.to_value(state)?;
        let event = self.lua.to_value(event)?;
        let value = run_bounded(&self.deadline, UPDATE_BUDGET, || {
            update.call::<Value>((model, state, event))
        })?;
        self.lua.from_value(value).map_err(RuntimeError::from)
    }
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
            children,
        })
    }
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
            RuntimeWorker::start(SCRIPT.into(), model.clone(), json!({})).unwrap();
        assert_ne!(ready.worker_thread, caller_thread);
        assert_eq!(ready.tree.children.len(), 2);

        let results = worker.results();
        worker
            .prepare_candidate(
                1,
                format!("{SCRIPT}\n-- revision 1"),
                model.clone(),
                json!({}),
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
