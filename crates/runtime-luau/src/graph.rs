use std::{
    collections::BTreeMap,
    io::{BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use experience_ir::{
    AppearanceProfile, Content, ExperienceEvent, ExperienceModel, ExperienceOutputEvent,
    ExperienceViewport, PaintOp, ProviderEffect, Scene, SceneEvent, SceneNode,
};
use experience_package::{
    DependencyAlias, EventId, GraphNodeId, InstanceId, PackageMetadata, ResolvedGraph, RevisionId,
    MAX_GRAPH_INSTANCES, MAX_GRAPH_SCENE_NODES,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::{LuauRuntime, RevisionAsset, RevisionAssetInput, RuntimeError};

static INSTANCE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
pub const MAX_GRAPH_WORKER_FRAME_BYTES: usize = 384 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GraphRevisionInput {
    pub source: String,
    pub sidecars: Vec<RevisionAssetInput>,
    pub model: ExperienceModel,
    pub state: JsonValue,
    pub state_schema_version: u64,
    pub package: PackageMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", content = "error", rename_all = "snake_case")]
pub enum RuntimeInstanceStatus {
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RuntimeInstanceSnapshot {
    pub instance_id: InstanceId,
    pub experience_id: experience_package::ExperienceId,
    pub revision_id: RevisionId,
    pub export_id: experience_package::ExportId,
    pub parent: Option<GraphNodeId>,
    pub dependency: Option<DependencyAlias>,
    #[serde(default)]
    pub viewport: ExperienceViewport,
    pub state: JsonValue,
    pub scene: Option<Scene>,
    pub status: RuntimeInstanceStatus,
    pub assets: Vec<RevisionAsset>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphRuntimeSnapshot {
    pub graph_id: String,
    pub root: GraphNodeId,
    pub instances: BTreeMap<GraphNodeId, RuntimeInstanceSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphEffect {
    pub node_id: GraphNodeId,
    pub instance_id: InstanceId,
    pub revision_id: RevisionId,
    pub effect: ProviderEffect,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphActionOutcome {
    pub snapshot: GraphRuntimeSnapshot,
    pub effects: Vec<GraphEffect>,
    pub external_events: Vec<ExperienceOutputEvent>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum GraphWorkerCommand {
    Action {
        request_id: u64,
        node_id: GraphNodeId,
        event: JsonValue,
    },
    RefreshModel {
        request_id: u64,
        node_id: GraphNodeId,
        model: ExperienceModel,
    },
    ApplyAppearance {
        request_id: u64,
        appearance: AppearanceProfile,
    },
    SetRootViewport {
        request_id: u64,
        viewport: ExperienceViewport,
    },
    Restore {
        request_id: u64,
        snapshot: GraphRuntimeSnapshot,
    },
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum GraphWorkerResult {
    ActionCompleted {
        request_id: u64,
        outcome: GraphActionOutcome,
    },
    Refreshed {
        request_id: u64,
        snapshot: GraphRuntimeSnapshot,
    },
    Rejected {
        request_id: u64,
        error: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "request", rename_all = "snake_case")]
enum GraphProcessRequest {
    Start {
        graph: ResolvedGraph,
        inputs: BTreeMap<RevisionId, GraphRevisionInput>,
    },
    Command {
        command: GraphWorkerCommand,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum GraphProcessEvent {
    Ready { snapshot: GraphRuntimeSnapshot },
    StartRejected { error: String },
    Result { result: GraphWorkerResult },
}

enum GraphWorkerJoin {
    Thread(std::thread::JoinHandle<()>),
    Process(std::thread::JoinHandle<()>),
}

pub struct GraphRuntimeWorker {
    commands: async_channel::Sender<GraphWorkerCommand>,
    results: async_channel::Receiver<GraphWorkerResult>,
    join: Option<GraphWorkerJoin>,
    process_id: Option<u32>,
}

impl GraphRuntimeWorker {
    pub fn start(
        graph: ResolvedGraph,
        inputs: BTreeMap<RevisionId, GraphRevisionInput>,
    ) -> Result<(Self, GraphRuntimeSnapshot), RuntimeError> {
        Self::start_with_root_viewport(graph, inputs, None)
    }

    pub fn start_with_root_viewport(
        graph: ResolvedGraph,
        inputs: BTreeMap<RevisionId, GraphRevisionInput>,
        root_viewport: Option<ExperienceViewport>,
    ) -> Result<(Self, GraphRuntimeSnapshot), RuntimeError> {
        let (commands_tx, commands_rx) = async_channel::unbounded();
        let (results_tx, results_rx) = async_channel::unbounded();
        let (ready_tx, ready_rx) = async_channel::bounded(1);
        let thread = std::thread::Builder::new()
            .name("sos-experience-graph".into())
            .spawn(move || {
                let mut runtime = match GraphRuntime::start(graph, inputs) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send_blocking(Err(error.to_string()));
                        return;
                    }
                };
                if let Some(viewport) = root_viewport {
                    if let Err(error) = runtime.set_root_viewport(viewport) {
                        let _ = ready_tx.send_blocking(Err(error.to_string()));
                        return;
                    }
                }
                if ready_tx.send_blocking(Ok(runtime.snapshot())).is_err() {
                    return;
                }
                while let Ok(command) = commands_rx.recv_blocking() {
                    let Some(result) = execute_graph_worker_command(&mut runtime, command) else {
                        break;
                    };
                    if results_tx.send_blocking(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| {
                RuntimeError::Invalid(format!("could not start graph worker: {error}"))
            })?;
        let ready = ready_rx
            .recv_blocking()
            .map_err(|_| RuntimeError::Invalid("graph worker stopped during startup".into()))?
            .map_err(RuntimeError::Invalid)?;
        Ok((
            Self {
                commands: commands_tx,
                results: results_rx,
                join: Some(GraphWorkerJoin::Thread(thread)),
                process_id: None,
            },
            ready,
        ))
    }

    pub fn start_process(
        executable: &Path,
        graph: ResolvedGraph,
        inputs: BTreeMap<RevisionId, GraphRevisionInput>,
    ) -> Result<(Self, GraphRuntimeSnapshot), RuntimeError> {
        let mut child = Command::new(executable)
            .arg("--graph-runtime-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                RuntimeError::Invalid(format!(
                    "could not start graph runtime process {}: {error}",
                    executable.display()
                ))
            })?;
        let process_id = child.id();
        let mut input = child.stdin.take().ok_or_else(|| {
            RuntimeError::Invalid("graph runtime process has no standard input".into())
        })?;
        let output = child.stdout.take().ok_or_else(|| {
            RuntimeError::Invalid("graph runtime process has no standard output".into())
        })?;
        let mut output = BufReader::new(output);
        write_graph_worker_frame(&mut input, &GraphProcessRequest::Start { graph, inputs })?;
        let ready = match read_graph_worker_frame::<_, GraphProcessEvent>(&mut output)? {
            Some(GraphProcessEvent::Ready { snapshot }) => snapshot,
            Some(GraphProcessEvent::StartRejected { error }) => {
                let _ = child.wait();
                return Err(RuntimeError::Invalid(error));
            }
            Some(GraphProcessEvent::Result { .. }) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RuntimeError::Invalid(
                    "graph runtime process returned a result before readiness".into(),
                ));
            }
            None => {
                let status = child.wait().ok();
                return Err(RuntimeError::Invalid(format!(
                    "graph runtime process stopped during startup{}",
                    status
                        .map(|status| format!(" with {status}"))
                        .unwrap_or_default()
                )));
            }
        };

        let (commands_tx, commands_rx) = async_channel::unbounded();
        let (results_tx, results_rx) = async_channel::unbounded();
        let join = std::thread::Builder::new()
            .name("sos-experience-graph-process".into())
            .spawn(move || {
                run_graph_process_io(child, input, output, commands_rx, results_tx);
            })
            .map_err(|error| {
                RuntimeError::Invalid(format!("could not monitor graph runtime process: {error}"))
            })?;
        Ok((
            Self {
                commands: commands_tx,
                results: results_rx,
                join: Some(GraphWorkerJoin::Process(join)),
                process_id: Some(process_id),
            },
            ready,
        ))
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process_id
    }

    pub fn results(&self) -> async_channel::Receiver<GraphWorkerResult> {
        self.results.clone()
    }

    pub fn action(
        &self,
        request_id: u64,
        node_id: GraphNodeId,
        event: JsonValue,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(GraphWorkerCommand::Action {
                request_id,
                node_id,
                event,
            })
            .map_err(|_| "graph runtime worker is unavailable".into())
    }

    pub fn refresh_model(
        &self,
        request_id: u64,
        node_id: GraphNodeId,
        model: ExperienceModel,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(GraphWorkerCommand::RefreshModel {
                request_id,
                node_id,
                model,
            })
            .map_err(|_| "graph runtime worker is unavailable".into())
    }

    pub fn apply_appearance(
        &self,
        request_id: u64,
        appearance: AppearanceProfile,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(GraphWorkerCommand::ApplyAppearance {
                request_id,
                appearance,
            })
            .map_err(|_| "graph runtime worker is unavailable".into())
    }

    pub fn set_root_viewport(
        &self,
        request_id: u64,
        viewport: ExperienceViewport,
    ) -> Result<(), String> {
        self.commands
            .send_blocking(GraphWorkerCommand::SetRootViewport {
                request_id,
                viewport,
            })
            .map_err(|_| "graph runtime worker is unavailable".into())
    }

    pub fn restore(&self, request_id: u64, snapshot: GraphRuntimeSnapshot) -> Result<(), String> {
        self.commands
            .send_blocking(GraphWorkerCommand::Restore {
                request_id,
                snapshot,
            })
            .map_err(|_| "graph runtime worker is unavailable".into())
    }

    pub fn shutdown(mut self) -> Result<(), String> {
        let _ = self.commands.send_blocking(GraphWorkerCommand::Shutdown);
        self.commands.close();
        if let Some(join) = self.join.take() {
            let joined = match join {
                GraphWorkerJoin::Thread(thread) | GraphWorkerJoin::Process(thread) => thread.join(),
            };
            joined.map_err(|_| "graph runtime worker panicked during shutdown".to_owned())?;
        }
        Ok(())
    }
}

impl Drop for GraphRuntimeWorker {
    fn drop(&mut self) {
        self.commands.close();
    }
}

fn execute_graph_worker_command(
    runtime: &mut GraphRuntime,
    command: GraphWorkerCommand,
) -> Option<GraphWorkerResult> {
    let result = match command {
        GraphWorkerCommand::Action {
            request_id,
            node_id,
            event,
        } => match runtime.dispatch_event(&node_id, &event) {
            Ok(outcome) => GraphWorkerResult::ActionCompleted {
                request_id,
                outcome,
            },
            Err(error) => GraphWorkerResult::Rejected {
                request_id,
                error: error.to_string(),
            },
        },
        GraphWorkerCommand::RefreshModel {
            request_id,
            node_id,
            model,
        } => match runtime.refresh_model(&node_id, model) {
            Ok(snapshot) => GraphWorkerResult::Refreshed {
                request_id,
                snapshot,
            },
            Err(error) => GraphWorkerResult::Rejected {
                request_id,
                error: error.to_string(),
            },
        },
        GraphWorkerCommand::ApplyAppearance {
            request_id,
            appearance,
        } => match runtime.apply_appearance(appearance) {
            Ok(snapshot) => GraphWorkerResult::Refreshed {
                request_id,
                snapshot,
            },
            Err(error) => GraphWorkerResult::Rejected {
                request_id,
                error: error.to_string(),
            },
        },
        GraphWorkerCommand::SetRootViewport {
            request_id,
            viewport,
        } => match runtime.set_root_viewport(viewport) {
            Ok(snapshot) => GraphWorkerResult::Refreshed {
                request_id,
                snapshot,
            },
            Err(error) => GraphWorkerResult::Rejected {
                request_id,
                error: error.to_string(),
            },
        },
        GraphWorkerCommand::Restore {
            request_id,
            snapshot,
        } => match runtime.restore(&snapshot) {
            Ok(snapshot) => GraphWorkerResult::Refreshed {
                request_id,
                snapshot,
            },
            Err(error) => GraphWorkerResult::Rejected {
                request_id,
                error: error.to_string(),
            },
        },
        GraphWorkerCommand::Shutdown => return None,
    };
    Some(result)
}

fn command_request_id(command: &GraphWorkerCommand) -> Option<u64> {
    match command {
        GraphWorkerCommand::Action { request_id, .. }
        | GraphWorkerCommand::RefreshModel { request_id, .. }
        | GraphWorkerCommand::ApplyAppearance { request_id, .. }
        | GraphWorkerCommand::SetRootViewport { request_id, .. }
        | GraphWorkerCommand::Restore { request_id, .. } => Some(*request_id),
        GraphWorkerCommand::Shutdown => None,
    }
}

fn run_graph_process_io(
    mut child: Child,
    mut input: impl Write,
    mut output: impl Read,
    commands: async_channel::Receiver<GraphWorkerCommand>,
    results: async_channel::Sender<GraphWorkerResult>,
) {
    while let Ok(command) = commands.recv_blocking() {
        let request_id = command_request_id(&command);
        let shutdown = matches!(&command, GraphWorkerCommand::Shutdown);
        if let Err(error) =
            write_graph_worker_frame(&mut input, &GraphProcessRequest::Command { command })
        {
            if let Some(request_id) = request_id {
                let _ = results.send_blocking(GraphWorkerResult::Rejected {
                    request_id,
                    error: error.to_string(),
                });
            }
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        if shutdown {
            let _ = child.wait();
            return;
        }
        match read_graph_worker_frame::<_, GraphProcessEvent>(&mut output) {
            Ok(Some(GraphProcessEvent::Result { result })) => {
                if results.send_blocking(result).is_err() {
                    break;
                }
            }
            Ok(Some(GraphProcessEvent::StartRejected { error })) => {
                if let Some(request_id) = request_id {
                    let _ =
                        results.send_blocking(GraphWorkerResult::Rejected { request_id, error });
                }
                break;
            }
            Ok(Some(GraphProcessEvent::Ready { .. })) => {
                if let Some(request_id) = request_id {
                    let _ = results.send_blocking(GraphWorkerResult::Rejected {
                        request_id,
                        error: "graph runtime process emitted duplicate readiness".into(),
                    });
                }
                break;
            }
            Ok(None) => {
                if let Some(request_id) = request_id {
                    let _ = results.send_blocking(GraphWorkerResult::Rejected {
                        request_id,
                        error: "graph runtime process stopped before replying".into(),
                    });
                }
                break;
            }
            Err(error) => {
                if let Some(request_id) = request_id {
                    let _ = results.send_blocking(GraphWorkerResult::Rejected {
                        request_id,
                        error: error.to_string(),
                    });
                }
                break;
            }
        }
    }
    let _ = write_graph_worker_frame(
        &mut input,
        &GraphProcessRequest::Command {
            command: GraphWorkerCommand::Shutdown,
        },
    );
    drop(input);
    let _ = child.wait();
}

pub fn run_graph_worker_stdio() -> Result<(), RuntimeError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run_graph_worker_protocol(stdin.lock(), stdout.lock())
}

fn run_graph_worker_protocol(
    mut input: impl Read,
    mut output: impl Write,
) -> Result<(), RuntimeError> {
    let Some(request) = read_graph_worker_frame::<_, GraphProcessRequest>(&mut input)? else {
        return Err(RuntimeError::Invalid(
            "graph runtime process received no start request".into(),
        ));
    };
    let GraphProcessRequest::Start { graph, inputs } = request else {
        return Err(RuntimeError::Invalid(
            "graph runtime process requires start as its first request".into(),
        ));
    };
    let mut runtime = match GraphRuntime::start(graph, inputs) {
        Ok(runtime) => runtime,
        Err(error) => {
            write_graph_worker_frame(
                &mut output,
                &GraphProcessEvent::StartRejected {
                    error: error.to_string(),
                },
            )?;
            return Ok(());
        }
    };
    write_graph_worker_frame(
        &mut output,
        &GraphProcessEvent::Ready {
            snapshot: runtime.snapshot(),
        },
    )?;
    while let Some(request) = read_graph_worker_frame::<_, GraphProcessRequest>(&mut input)? {
        let GraphProcessRequest::Command { command } = request else {
            return Err(RuntimeError::Invalid(
                "graph runtime process received duplicate start".into(),
            ));
        };
        let Some(result) = execute_graph_worker_command(&mut runtime, command) else {
            return Ok(());
        };
        write_graph_worker_frame(&mut output, &GraphProcessEvent::Result { result })?;
    }
    Ok(())
}

fn write_graph_worker_frame(
    output: &mut impl Write,
    value: &impl Serialize,
) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| RuntimeError::Invalid(format!("serialize graph worker frame: {error}")))?;
    if bytes.len() > MAX_GRAPH_WORKER_FRAME_BYTES {
        return Err(RuntimeError::Invalid(format!(
            "graph worker frame is {} bytes; limit is {MAX_GRAPH_WORKER_FRAME_BYTES}",
            bytes.len()
        )));
    }
    let length = u32::try_from(bytes.len())
        .map_err(|_| RuntimeError::Invalid("graph worker frame length overflow".into()))?;
    output
        .write_all(&length.to_be_bytes())
        .and_then(|_| output.write_all(&bytes))
        .and_then(|_| output.flush())
        .map_err(|error| RuntimeError::Invalid(format!("write graph worker frame: {error}")))
}

fn read_graph_worker_frame<R: Read, T: DeserializeOwned>(
    input: &mut R,
) -> Result<Option<T>, RuntimeError> {
    let mut length = [0_u8; 4];
    match input.read(&mut length[..1]) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(error) => {
            return Err(RuntimeError::Invalid(format!(
                "read graph worker frame: {error}"
            )))
        }
    }
    input
        .read_exact(&mut length[1..])
        .map_err(|error| RuntimeError::Invalid(format!("read graph worker length: {error}")))?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_GRAPH_WORKER_FRAME_BYTES {
        return Err(RuntimeError::Invalid(format!(
            "graph worker frame length {length} is outside 1..={MAX_GRAPH_WORKER_FRAME_BYTES}"
        )));
    }
    let mut bytes = vec![0_u8; length];
    input
        .read_exact(&mut bytes)
        .map_err(|error| RuntimeError::Invalid(format!("read graph worker payload: {error}")))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| RuntimeError::Invalid(format!("decode graph worker frame: {error}")))
}

struct RuntimeInstance {
    instance_id: InstanceId,
    runtime: LuauRuntime,
    model: ExperienceModel,
    state: JsonValue,
    state_schema_version: u64,
    package: PackageMetadata,
    properties: JsonValue,
    container_appearance: Option<JsonValue>,
    viewport: ExperienceViewport,
    scene: Option<Scene>,
    status: RuntimeInstanceStatus,
}

pub struct GraphRuntime {
    graph: ResolvedGraph,
    graph_id: String,
    instances: BTreeMap<GraphNodeId, RuntimeInstance>,
    children: BTreeMap<GraphNodeId, Vec<GraphNodeId>>,
}

impl GraphRuntime {
    pub fn start(
        graph: ResolvedGraph,
        inputs: BTreeMap<RevisionId, GraphRevisionInput>,
    ) -> Result<Self, RuntimeError> {
        graph
            .validate()
            .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
        let graph_id = graph
            .id()
            .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
        if graph.nodes.len() > MAX_GRAPH_INSTANCES {
            return Err(RuntimeError::Invalid("graph has too many instances".into()));
        }
        let mut instances = BTreeMap::new();
        let mut children = BTreeMap::<GraphNodeId, Vec<GraphNodeId>>::new();
        for (node_id, node) in &graph.nodes {
            let input = inputs.get(&node.revision_id).ok_or_else(|| {
                RuntimeError::Invalid(format!(
                    "graph input for revision {} is missing",
                    node.revision_id
                ))
            })?;
            if input.package.experience_id != node.experience_id
                || !input.package.contract.exports.contains_key(&node.export_id)
            {
                return Err(RuntimeError::Invalid(format!(
                    "graph input for node `{node_id}` does not match its package"
                )));
            }
            if input.state_schema_version == 0 {
                return Err(RuntimeError::Invalid(format!(
                    "node `{node_id}` has invalid state schema version"
                )));
            }
            let runtime = LuauRuntime::compile_with_assets(&input.source, input.sidecars.clone())?;
            if runtime.api_version() != experience_ir::EXPERIENCE_API_VERSION_V4 {
                return Err(RuntimeError::Invalid(format!(
                    "graph node `{node_id}` must use experience API v4"
                )));
            }
            if !runtime.export_ids()?.contains(&node.export_id.to_string()) {
                return Err(RuntimeError::Invalid(format!(
                    "source for node `{node_id}` does not implement export `{}`",
                    node.export_id
                )));
            }
            if let Some(parent) = &node.parent {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(node_id.clone());
            }
            instances.insert(
                node_id.clone(),
                RuntimeInstance {
                    instance_id: new_instance_id(&graph_id, node_id)?,
                    runtime,
                    model: input.model.clone(),
                    state: input.state.clone(),
                    state_schema_version: input.state_schema_version,
                    package: input.package.clone(),
                    properties: json!({}),
                    container_appearance: None,
                    viewport: ExperienceViewport::default(),
                    scene: None,
                    status: RuntimeInstanceStatus::Ready,
                },
            );
        }
        for children in children.values_mut() {
            children.sort();
        }
        let mut runtime = Self {
            graph,
            graph_id,
            instances,
            children,
        };
        runtime.validate_runtime_graph()?;
        let root = runtime.graph.root.clone();
        runtime.render_subtree(&root, true)?;
        runtime.validate_scene_budget()?;
        Ok(runtime)
    }

    pub fn snapshot(&self) -> GraphRuntimeSnapshot {
        GraphRuntimeSnapshot {
            graph_id: self.graph_id.clone(),
            root: self.graph.root.clone(),
            instances: self
                .graph
                .nodes
                .iter()
                .map(|(node_id, node)| {
                    let instance = self.instances.get(node_id).expect("graph instance exists");
                    (
                        node_id.clone(),
                        RuntimeInstanceSnapshot {
                            instance_id: instance.instance_id.clone(),
                            experience_id: node.experience_id.clone(),
                            revision_id: node.revision_id.clone(),
                            export_id: node.export_id.clone(),
                            parent: node.parent.clone(),
                            dependency: node.dependency.clone(),
                            viewport: instance.viewport.clone(),
                            state: instance.state.clone(),
                            scene: instance.scene.clone(),
                            status: instance.status.clone(),
                            assets: instance
                                .runtime
                                .assets()
                                .iter()
                                .cloned()
                                .map(|mut asset| {
                                    asset.path =
                                        instance_asset_path(&instance.instance_id, &asset.path);
                                    asset
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn dispatch_scene_event(
        &mut self,
        node_id: &GraphNodeId,
        event: &SceneEvent,
    ) -> Result<GraphActionOutcome, RuntimeError> {
        let event = serde_json::to_value(event)
            .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
        self.dispatch_event(node_id, &event)
    }

    pub fn dispatch_event(
        &mut self,
        node_id: &GraphNodeId,
        event: &JsonValue,
    ) -> Result<GraphActionOutcome, RuntimeError> {
        let previous = self.snapshot();
        let mut effects = Vec::new();
        let mut external_events = Vec::new();
        if let Err(error) = self
            .update_node(node_id, event, &mut effects, &mut external_events, 0)
            .and_then(|_| self.validate_scene_budget())
        {
            return match self.restore(&previous) {
                Ok(_) => Err(error),
                Err(rollback) => Err(RuntimeError::Invalid(format!(
                    "graph update failed ({error}); in-memory rollback failed ({rollback})"
                ))),
            };
        }
        Ok(GraphActionOutcome {
            snapshot: self.snapshot(),
            effects,
            external_events,
        })
    }

    pub fn refresh_model(
        &mut self,
        node_id: &GraphNodeId,
        model: ExperienceModel,
    ) -> Result<GraphRuntimeSnapshot, RuntimeError> {
        let identity = self
            .graph
            .nodes
            .get(node_id)
            .ok_or_else(|| RuntimeError::Invalid(format!("unknown graph node `{node_id}`")))?
            .experience_id
            .clone();
        let targets = self
            .graph
            .nodes
            .iter()
            .filter_map(|(candidate_id, candidate)| {
                (candidate.experience_id == identity).then_some(candidate_id.clone())
            })
            .collect::<Vec<_>>();
        let previous_snapshot = self.snapshot();
        let previous_models = targets
            .iter()
            .map(|target| (target.clone(), self.instances[target].model.clone()))
            .collect::<BTreeMap<_, _>>();
        for target in &targets {
            self.instances
                .get_mut(target)
                .expect("graph instance exists")
                .model = model.clone();
        }
        for target in targets {
            if let Err(error) = self.render_subtree(&target, target == self.graph.root) {
                for (node_id, previous) in previous_models {
                    self.instances
                        .get_mut(&node_id)
                        .expect("graph instance exists")
                        .model = previous;
                }
                self.restore(&previous_snapshot)?;
                return Err(error);
            }
        }
        if let Err(error) = self.validate_scene_budget() {
            for (node_id, previous) in previous_models {
                self.instances
                    .get_mut(&node_id)
                    .expect("graph instance exists")
                    .model = previous;
            }
            self.restore(&previous_snapshot)?;
            return Err(error);
        }
        Ok(self.snapshot())
    }

    pub fn apply_appearance(
        &mut self,
        appearance: AppearanceProfile,
    ) -> Result<GraphRuntimeSnapshot, RuntimeError> {
        let previous_snapshot = self.snapshot();
        let previous = self
            .instances
            .iter()
            .map(|(node_id, instance)| (node_id.clone(), instance.model.appearance.clone()))
            .collect::<BTreeMap<_, _>>();
        for instance in self.instances.values_mut() {
            instance.model.appearance = appearance.clone();
        }
        let root = self.graph.root.clone();
        if let Err(error) = self
            .render_subtree(&root, true)
            .and_then(|_| self.validate_scene_budget())
        {
            for (node_id, appearance) in previous {
                self.instances
                    .get_mut(&node_id)
                    .expect("graph instance exists")
                    .model
                    .appearance = appearance;
            }
            self.restore(&previous_snapshot)?;
            return Err(error);
        }
        Ok(self.snapshot())
    }

    pub fn set_root_viewport(
        &mut self,
        viewport: ExperienceViewport,
    ) -> Result<GraphRuntimeSnapshot, RuntimeError> {
        if viewport.width == 0
            || viewport.height == 0
            || !(250..=8000).contains(&viewport.scale_milli)
            || viewport
                .safe_insets
                .left
                .saturating_add(viewport.safe_insets.right)
                >= viewport.width
            || viewport
                .safe_insets
                .top
                .saturating_add(viewport.safe_insets.bottom)
                >= viewport.height
        {
            return Err(RuntimeError::Invalid(
                "root viewport or safe insets are invalid".into(),
            ));
        }
        let root = self.graph.root.clone();
        let graph_node = &self.graph.nodes[&root];
        let export = &self.instances[&root].package.contract.exports[&graph_node.export_id];
        if viewport.width < export.viewport.min_width
            || viewport.width > export.viewport.max_width
            || viewport.height < export.viewport.min_height
            || viewport.height > export.viewport.max_height
        {
            return Err(RuntimeError::Invalid(format!(
                "root viewport {}x{} is outside export `{}` bounds",
                viewport.width, viewport.height, graph_node.export_id
            )));
        }
        let previous = self.instances[&root].viewport.clone();
        self.instances
            .get_mut(&root)
            .expect("root graph instance exists")
            .viewport = viewport;
        if let Err(error) = self
            .render_subtree(&root, true)
            .and_then(|_| self.validate_scene_budget())
        {
            self.instances
                .get_mut(&root)
                .expect("root graph instance exists")
                .viewport = previous;
            self.render_subtree(&root, true)?;
            self.validate_scene_budget()?;
            return Err(error);
        }
        Ok(self.snapshot())
    }

    pub fn restore(
        &mut self,
        snapshot: &GraphRuntimeSnapshot,
    ) -> Result<GraphRuntimeSnapshot, RuntimeError> {
        if snapshot.graph_id != self.graph_id
            || snapshot.root != self.graph.root
            || snapshot.instances.len() != self.instances.len()
            || snapshot.instances.iter().any(|(node_id, saved)| {
                self.graph.nodes.get(node_id).is_none_or(|node| {
                    saved.instance_id != self.instances[node_id].instance_id
                        || saved.experience_id != node.experience_id
                        || saved.revision_id != node.revision_id
                        || saved.export_id != node.export_id
                        || saved.parent != node.parent
                        || saved.dependency != node.dependency
                })
            })
        {
            return Err(RuntimeError::Invalid(
                "rollback snapshot does not match the active graph".into(),
            ));
        }
        for (node_id, saved) in &snapshot.instances {
            let instance = self.instances.get_mut(node_id).expect("node was checked");
            instance.viewport = saved.viewport.clone();
            instance.state = saved.state.clone();
        }
        let root = self.graph.root.clone();
        self.render_subtree(&root, true)?;
        self.validate_scene_budget()?;
        Ok(self.snapshot())
    }

    pub fn state_schema_version(&self, node_id: &GraphNodeId) -> Option<u64> {
        self.instances
            .get(node_id)
            .map(|instance| instance.state_schema_version)
    }

    fn update_node(
        &mut self,
        node_id: &GraphNodeId,
        event: &JsonValue,
        effects: &mut Vec<GraphEffect>,
        external_events: &mut Vec<ExperienceOutputEvent>,
        event_depth: usize,
    ) -> Result<(), RuntimeError> {
        if event_depth > MAX_GRAPH_INSTANCES {
            return Err(RuntimeError::Invalid(
                "child event propagation exceeded the graph limit".into(),
            ));
        }
        let graph_node = self
            .graph
            .nodes
            .get(node_id)
            .cloned()
            .ok_or_else(|| RuntimeError::Invalid(format!("unknown graph node `{node_id}`")))?;
        let output = {
            let instance = self.instances.get(node_id).expect("graph instance exists");
            instance.runtime.update_export_with_effects(
                graph_node.export_id.as_str(),
                &instance.model,
                &instance.state,
                event,
                &instance.properties,
                instance.viewport.clone(),
                instance.container_appearance.clone(),
            )
        };
        let output = match output {
            Ok(output) => output,
            Err(error) if graph_node.parent.is_some() => {
                let instance = self
                    .instances
                    .get_mut(node_id)
                    .expect("graph instance exists");
                instance.scene = None;
                instance.status = RuntimeInstanceStatus::Failed(error.to_string());
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        self.validate_output_events(node_id, &output.events)?;
        let candidate_scene = {
            let instance = self.instances.get(node_id).expect("graph instance exists");
            instance.runtime.render_export(
                graph_node.export_id.as_str(),
                &instance.model,
                &output.state,
                &instance.properties,
                instance.viewport.clone(),
                instance.container_appearance.clone(),
            )
        }
        .and_then(|scene| {
            validate_scene_authority(
                &scene,
                self.instances[node_id].package.role,
                graph_node.parent.is_some(),
            )?;
            Ok(scene)
        });
        let (candidate_scene, failure) = match candidate_scene {
            Ok(mut scene) => {
                namespace_instance_scene(&mut scene.root, &self.instances[node_id].instance_id);
                (Some(scene), None)
            }
            Err(error) if graph_node.parent.is_some() => (None, Some(error.to_string())),
            Err(error) => return Err(error),
        };
        let candidate_state = output.state;
        let peers = self
            .graph
            .nodes
            .iter()
            .filter_map(|(candidate_id, candidate)| {
                (candidate_id != node_id && candidate.experience_id == graph_node.experience_id)
                    .then_some(candidate_id.clone())
            })
            .collect::<Vec<_>>();
        {
            let instance = self
                .instances
                .get_mut(node_id)
                .expect("graph instance exists");
            instance.state = candidate_state.clone();
            instance.scene = candidate_scene;
            instance.status = failure
                .as_ref()
                .map_or(RuntimeInstanceStatus::Ready, |error| {
                    RuntimeInstanceStatus::Failed(error.clone())
                });
        }
        for peer in &peers {
            self.instances
                .get_mut(peer)
                .expect("peer graph instance exists")
                .state = candidate_state.clone();
        }
        if failure.is_none() {
            for effect in output.effects {
                effects.push(GraphEffect {
                    node_id: node_id.clone(),
                    instance_id: self.instances[node_id].instance_id.clone(),
                    revision_id: graph_node.revision_id.clone(),
                    effect,
                });
            }
            self.render_children(node_id)?;
        }
        for peer in peers {
            self.render_subtree(&peer, peer == self.graph.root)?;
        }
        if failure.is_some() {
            return Ok(());
        }
        for output_event in output.events {
            let Some(parent_id) = &graph_node.parent else {
                external_events.push(output_event);
                continue;
            };
            let dependency = graph_node
                .dependency
                .as_ref()
                .expect("non-root graph node has dependency");
            let parent_event = serde_json::to_value(ExperienceEvent {
                dependency: dependency.to_string(),
                event: output_event.event,
                payload: output_event.payload,
            })
            .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
            self.update_node(
                parent_id,
                &parent_event,
                effects,
                external_events,
                event_depth + 1,
            )?;
        }
        Ok(())
    }

    fn render_subtree(&mut self, node_id: &GraphNodeId, root: bool) -> Result<(), RuntimeError> {
        let graph_node = self
            .graph
            .nodes
            .get(node_id)
            .cloned()
            .expect("graph node exists");
        let scene = {
            let instance = self.instances.get(node_id).expect("graph instance exists");
            instance.runtime.render_export(
                graph_node.export_id.as_str(),
                &instance.model,
                &instance.state,
                &instance.properties,
                instance.viewport.clone(),
                instance.container_appearance.clone(),
            )
        };
        match scene {
            Ok(mut scene) => {
                validate_scene_authority(&scene, self.instances[node_id].package.role, !root)?;
                namespace_instance_scene(&mut scene.root, &self.instances[node_id].instance_id);
                let instance = self
                    .instances
                    .get_mut(node_id)
                    .expect("graph instance exists");
                instance.scene = Some(scene);
                instance.status = RuntimeInstanceStatus::Ready;
                self.render_children(node_id)
            }
            Err(error) if !root => {
                let instance = self
                    .instances
                    .get_mut(node_id)
                    .expect("graph instance exists");
                instance.scene = None;
                instance.status = RuntimeInstanceStatus::Failed(error.to_string());
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn render_children(&mut self, parent_id: &GraphNodeId) -> Result<(), RuntimeError> {
        let children = self.children.get(parent_id).cloned().unwrap_or_default();
        for child_id in children {
            let graph_node = self
                .graph
                .nodes
                .get(&child_id)
                .cloned()
                .expect("child graph node exists");
            let alias = graph_node
                .dependency
                .as_ref()
                .expect("child has dependency alias");
            let (mount_count, mount) = {
                let parent_scene = self
                    .instances
                    .get(parent_id)
                    .and_then(|instance| instance.scene.as_ref())
                    .expect("parent scene is ready");
                let mounts = mounts_for(parent_scene, alias);
                (
                    mounts.len(),
                    mounts.first().map(|mount| {
                        (
                            mount.node.clone(),
                            mount.properties.clone(),
                            mount.container_appearance.cloned(),
                        )
                    }),
                )
            };
            if mount_count != 1 {
                let instance = self.instances.get_mut(&child_id).expect("child exists");
                instance.scene = None;
                instance.status = RuntimeInstanceStatus::Failed(format!(
                    "dependency `{alias}` requires exactly one live mount, found {}",
                    mount_count
                ));
                continue;
            }
            let (mount_node, mount_properties, container_appearance) =
                mount.expect("one mount exists");
            let binding = &self.instances[parent_id].package.dependencies[alias];
            let Some(property_fields) = mount_properties.as_object() else {
                let instance = self.instances.get_mut(&child_id).expect("child exists");
                instance.scene = None;
                instance.status = RuntimeInstanceStatus::Failed(
                    "mounted properties must be a closed record".into(),
                );
                continue;
            };
            if property_fields
                .keys()
                .any(|name| !binding.grant.properties.contains(name))
            {
                let instance = self.instances.get_mut(&child_id).expect("child exists");
                instance.scene = None;
                instance.status = RuntimeInstanceStatus::Failed(format!(
                    "dependency `{alias}` received a property outside its boundary grant"
                ));
                continue;
            }
            let child_export = self.instances[&child_id]
                .package
                .contract
                .exports
                .get(&graph_node.export_id)
                .expect("resolved child export exists");
            if let Err(error) = child_export.properties.validate_value(&mount_properties) {
                let instance = self.instances.get_mut(&child_id).expect("child exists");
                instance.scene = None;
                instance.status = RuntimeInstanceStatus::Failed(error.to_string());
                continue;
            }
            if container_appearance.is_some() && !child_export.accepts_container_appearance {
                let instance = self.instances.get_mut(&child_id).expect("child exists");
                instance.scene = None;
                instance.status = RuntimeInstanceStatus::Failed(format!(
                    "export `{}` does not accept container appearance",
                    graph_node.export_id
                ));
                continue;
            }
            let viewport = viewport_for(&mount_node, &child_export.viewport);
            {
                let instance = self.instances.get_mut(&child_id).expect("child exists");
                instance.properties = mount_properties;
                instance.container_appearance = container_appearance
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
                instance.viewport = viewport;
            }
            self.render_subtree(&child_id, false)?;
        }
        Ok(())
    }

    fn validate_runtime_graph(&self) -> Result<(), RuntimeError> {
        let mut revisions_by_experience = BTreeMap::new();
        for (node_id, node) in &self.graph.nodes {
            if let Some(existing) =
                revisions_by_experience.insert(node.experience_id.clone(), node.revision_id.clone())
            {
                if existing != node.revision_id {
                    return Err(RuntimeError::Invalid(format!(
                        "experience `{}` appears at multiple revisions in one graph",
                        node.experience_id
                    )));
                }
            }
            let mut cursor = node.parent.as_ref();
            while let Some(ancestor) = cursor {
                if self.graph.nodes[ancestor].experience_id == node.experience_id {
                    return Err(RuntimeError::Invalid(format!(
                        "experience `{}` cannot contain another instance of itself",
                        node.experience_id
                    )));
                }
                cursor = self.graph.nodes[ancestor].parent.as_ref();
            }

            let Some(parent_id) = &node.parent else {
                continue;
            };
            let alias = node
                .dependency
                .as_ref()
                .expect("validated graph child has an alias");
            let parent = &self.instances[parent_id].package;
            let binding = parent.dependencies.get(alias).ok_or_else(|| {
                RuntimeError::Invalid(format!(
                    "node `{node_id}` is not declared by parent dependency `{alias}`"
                ))
            })?;
            let child = &self.instances[node_id].package;
            if child.role == experience_package::ExperienceRole::Shell
                || binding.experience_id != node.experience_id
                || binding.export_id != node.export_id
                || (binding.policy == experience_package::DependencyPolicy::Locked
                    && binding.revision_id != node.revision_id)
            {
                return Err(RuntimeError::Invalid(format!(
                    "node `{node_id}` does not match dependency `{alias}`"
                )));
            }
            let digest = child
                .contract
                .digest()
                .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
            if digest != binding.contract_digest {
                return Err(RuntimeError::Invalid(format!(
                    "node `{node_id}` contract does not match dependency `{alias}`"
                )));
            }
            let export = &child.contract.exports[&node.export_id];
            let experience_package::ValueSchema::Record { fields } = &export.properties else {
                return Err(RuntimeError::Invalid(format!(
                    "dependency `{alias}` must expose record properties"
                )));
            };
            if fields
                .iter()
                .any(|(name, field)| field.required && !binding.grant.properties.contains(name))
                || binding
                    .grant
                    .properties
                    .iter()
                    .any(|name| !fields.contains_key(name))
                || binding
                    .grant
                    .events
                    .iter()
                    .any(|event| !export.events.contains_key(event))
            {
                return Err(RuntimeError::Invalid(format!(
                    "dependency `{alias}` has an invalid boundary grant"
                )));
            }
        }
        for node_id in self.graph.nodes.keys() {
            let declared = self.instances[node_id]
                .package
                .dependencies
                .keys()
                .collect::<std::collections::BTreeSet<_>>();
            let resolved = self
                .children
                .get(node_id)
                .into_iter()
                .flatten()
                .map(|child| {
                    self.graph.nodes[child]
                        .dependency
                        .as_ref()
                        .expect("child alias exists")
                })
                .collect::<std::collections::BTreeSet<_>>();
            if declared != resolved {
                return Err(RuntimeError::Invalid(format!(
                    "node `{node_id}` dependency set does not match the resolved graph"
                )));
            }
        }
        Ok(())
    }

    fn validate_scene_budget(&self) -> Result<(), RuntimeError> {
        fn count(node: &SceneNode) -> usize {
            1 + node.children.iter().map(count).sum::<usize>()
        }
        let nodes = self
            .instances
            .values()
            .filter_map(|instance| instance.scene.as_ref())
            .map(|scene| count(&scene.root))
            .sum::<usize>();
        if nodes > MAX_GRAPH_SCENE_NODES {
            return Err(RuntimeError::Invalid(format!(
                "experience graph produced {nodes} scene nodes, limit is {MAX_GRAPH_SCENE_NODES}"
            )));
        }
        Ok(())
    }

    fn validate_output_events(
        &self,
        node_id: &GraphNodeId,
        events: &[ExperienceOutputEvent],
    ) -> Result<(), RuntimeError> {
        let graph_node = &self.graph.nodes[node_id];
        let instance = &self.instances[node_id];
        let export = &instance.package.contract.exports[&graph_node.export_id];
        let granted = graph_node.parent.as_ref().map(|parent_id| {
            let parent = &self.instances[parent_id];
            let alias = graph_node
                .dependency
                .as_ref()
                .expect("child has dependency alias");
            &parent.package.dependencies[alias].grant.events
        });
        for event in events {
            let id = EventId::parse(&event.event)
                .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
            let schema = export.events.get(&id).ok_or_else(|| {
                RuntimeError::Invalid(format!(
                    "export `{}` emitted undeclared event `{id}`",
                    graph_node.export_id
                ))
            })?;
            schema
                .validate_value(&event.payload)
                .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
            if granted.is_some_and(|events| !events.contains(&id)) {
                return Err(RuntimeError::Invalid(format!(
                    "parent is not granted child event `{id}`"
                )));
            }
        }
        Ok(())
    }
}

fn validate_scene_authority(
    scene: &Scene,
    role: experience_package::ExperienceRole,
    mounted: bool,
) -> Result<(), RuntimeError> {
    fn visit(
        node: &SceneNode,
        role: experience_package::ExperienceRole,
        mounted: bool,
    ) -> Result<(), RuntimeError> {
        match &node.content {
            Some(Content::WindowSpace(_) | Content::ShellOverlay(_))
                if role != experience_package::ExperienceRole::Shell =>
            {
                return Err(RuntimeError::Invalid(
                    "ordinary experience emitted shell-only content".into(),
                ));
            }
            Some(Content::ApplicationSurface(_)) => {
                return Err(RuntimeError::Invalid(
                    "v4 experience emitted the v3-only application_surface primitive".into(),
                ));
            }
            _ => {}
        }
        for child in &node.children {
            visit(child, role, mounted)?;
        }
        Ok(())
    }
    visit(&scene.root, role, mounted)
}

fn new_instance_id(graph_id: &str, node_id: &GraphNodeId) -> Result<InstanceId, RuntimeError> {
    let sequence = INSTANCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let digest = Sha256::digest(
        format!("{}:{sequence}:{graph_id}:{node_id}", std::process::id()).as_bytes(),
    );
    InstanceId::parse(format!("i-{digest:x}"))
        .map_err(|error| RuntimeError::Invalid(error.to_string()))
}

fn instance_asset_path(instance_id: &InstanceId, path: &str) -> String {
    format!("instances/{instance_id}/{path}")
}

fn namespace_instance_scene(node: &mut SceneNode, instance_id: &InstanceId) {
    if let Some(Content::Image(image)) = &mut node.content {
        if image.asset != "album-orbit" {
            image.asset = instance_asset_path(instance_id, &image.asset);
        }
    }
    if let Some(Content::ProviderSurface(surface)) = &mut node.content {
        surface.surface = format!("{instance_id}::{}", surface.surface);
    }
    fn namespace_paint(operation: &mut PaintOp, instance_id: &InstanceId) {
        match operation {
            PaintOp::Shader { asset, .. } => {
                *asset = instance_asset_path(instance_id, asset);
            }
            PaintOp::Layer { operations, .. } => {
                for operation in operations {
                    namespace_paint(operation, instance_id);
                }
            }
            _ => {}
        }
    }
    for operation in &mut node.paint {
        namespace_paint(operation, instance_id);
    }
    for child in &mut node.children {
        namespace_instance_scene(child, instance_id);
    }
}

struct Mount<'a> {
    node: &'a SceneNode,
    properties: &'a JsonValue,
    container_appearance: Option<&'a experience_package::ContainerAppearance>,
}

fn mounts_for<'a>(scene: &'a Scene, alias: &DependencyAlias) -> Vec<Mount<'a>> {
    fn visit<'a>(node: &'a SceneNode, alias: &DependencyAlias, mounts: &mut Vec<Mount<'a>>) {
        if let Some(Content::ExperienceMount(mount)) = &node.content {
            if mount.dependency == alias.as_str() {
                mounts.push(Mount {
                    node,
                    properties: &mount.properties,
                    container_appearance: mount.container_appearance.as_ref(),
                });
            }
        }
        for child in &node.children {
            visit(child, alias, mounts);
        }
    }
    let mut mounts = Vec::new();
    visit(&scene.root, alias, &mut mounts);
    mounts
}

fn viewport_for(
    mount: &SceneNode,
    contract: &experience_package::ViewportContract,
) -> ExperienceViewport {
    let width = mount
        .layout
        .width
        .map(|value| value.round().max(1.0) as u32)
        .unwrap_or(contract.min_width)
        .clamp(contract.min_width, contract.max_width);
    let height = mount
        .layout
        .height
        .map(|value| value.round().max(1.0) as u32)
        .unwrap_or(contract.min_height)
        .clamp(contract.min_height, contract.max_height);
    ExperienceViewport {
        width,
        height,
        scale_milli: 1000,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use experience_package::{
        BoundaryGrant, DependencyBinding, DependencyPolicy, DerivationKind, DerivationRecord,
        ExperienceContract, ExperienceExport, ExperienceId, ExperienceRole, ExportId, FieldSchema,
        ResolvedGraphNode, ValueSchema, ViewportContract, APPEARANCE_ABI_VERSION, CONTRACT_VERSION,
        GRAPH_FORMAT_VERSION, PACKAGE_FORMAT_VERSION,
    };

    use super::*;

    fn revision(character: char) -> RevisionId {
        RevisionId::parse(character.to_string().repeat(64)).unwrap()
    }

    fn viewport() -> ViewportContract {
        ViewportContract {
            min_width: 160,
            min_height: 96,
            max_width: 1920,
            max_height: 1080,
        }
    }

    fn package(
        experience_id: &str,
        export_id: &str,
        export: ExperienceExport,
        dependencies: BTreeMap<DependencyAlias, DependencyBinding>,
    ) -> PackageMetadata {
        PackageMetadata {
            format_version: PACKAGE_FORMAT_VERSION,
            experience_id: ExperienceId::parse(experience_id).unwrap(),
            role: ExperienceRole::Ordinary,
            provider_capabilities: Default::default(),
            contract: ExperienceContract {
                contract_version: CONTRACT_VERSION,
                exports: BTreeMap::from([(ExportId::parse(export_id).unwrap(), export)]),
            },
            dependencies,
            derivation: DerivationRecord {
                kind: DerivationKind::Original,
                parents: vec![],
                request_sha256: None,
                rationale: None,
            },
            state_migration: None,
        }
    }

    #[test]
    fn graph_worker_frames_are_length_bounded_and_closed() {
        let mut encoded = Vec::new();
        write_graph_worker_frame(
            &mut encoded,
            &GraphProcessRequest::Command {
                command: GraphWorkerCommand::Shutdown,
            },
        )
        .unwrap();
        let decoded =
            read_graph_worker_frame::<_, GraphProcessRequest>(&mut encoded.as_slice()).unwrap();
        assert!(matches!(
            decoded,
            Some(GraphProcessRequest::Command {
                command: GraphWorkerCommand::Shutdown
            })
        ));

        let oversized = u32::try_from(MAX_GRAPH_WORKER_FRAME_BYTES + 1)
            .unwrap()
            .to_be_bytes();
        assert!(
            read_graph_worker_frame::<_, GraphProcessRequest>(&mut oversized.as_slice())
                .unwrap_err()
                .to_string()
                .contains("outside")
        );
    }

    #[test]
    fn root_viewport_and_safe_insets_rerender_without_changing_identity() {
        let root = GraphNodeId::parse("root").unwrap();
        let root_revision = revision('e');
        let graph = ResolvedGraph {
            format_version: GRAPH_FORMAT_VERSION,
            root: root.clone(),
            nodes: BTreeMap::from([(
                root.clone(),
                ResolvedGraphNode {
                    experience_id: ExperienceId::parse("viewport-test").unwrap(),
                    revision_id: root_revision.clone(),
                    export_id: ExportId::parse("main").unwrap(),
                    parent: None,
                    dependency: None,
                },
            )]),
        };
        let package = package(
            "viewport-test",
            "main",
            ExperienceExport {
                properties: ValueSchema::empty_record(),
                events: BTreeMap::new(),
                viewport: viewport(),
                appearance_abi: APPEARANCE_ABI_VERSION,
                accepts_container_appearance: false,
            },
            BTreeMap::new(),
        );
        let inputs = BTreeMap::from([(
            root_revision,
            GraphRevisionInput {
                source: r#"
                    return { api_version = 4, exports = { main = {
                        render = function(_, _, _, context)
                            return { id = "viewport", content = { kind = "text",
                                value = tostring(context.viewport.width) .. ":" ..
                                    tostring(context.viewport.safe_insets.top),
                                size = 16, color = 0xffffffff } }
                        end,
                    } } }
                "#
                .into(),
                sidecars: vec![],
                model: providers_fake::snapshot(),
                state: json!({}),
                state_schema_version: 1,
                package,
            },
        )]);
        let mut runtime = GraphRuntime::start(graph, inputs).unwrap();
        let initial = runtime.snapshot();
        let instance_id = initial.instances[&root].instance_id.clone();
        let viewport = ExperienceViewport {
            width: 360,
            height: 780,
            scale_milli: 2813,
            safe_insets: experience_ir::ExperienceInsets {
                left: 0,
                top: 32,
                right: 0,
                bottom: 24,
            },
        };
        let updated = runtime.set_root_viewport(viewport).unwrap();
        assert_eq!(updated.instances[&root].instance_id, instance_id);
        assert!(matches!(
            updated.instances[&root]
                .scene
                .as_ref()
                .unwrap()
                .root
                .content
                .as_ref(),
            Some(Content::Text(text)) if text.value == "360:32"
        ));

        let invalid = ExperienceViewport {
            width: 360,
            height: 780,
            scale_milli: 2813,
            safe_insets: experience_ir::ExperienceInsets {
                top: 780,
                ..Default::default()
            },
        };
        assert!(runtime.set_root_viewport(invalid).is_err());
        assert_eq!(runtime.snapshot(), updated);
    }

    #[test]
    fn child_event_updates_parent_without_merging_state_or_vms() {
        let string = || ValueSchema::String {
            max_bytes: 64,
            choices: BTreeSet::new(),
        };
        let agenda = package(
            "agenda",
            "summary",
            ExperienceExport {
                properties: ValueSchema::Record {
                    fields: BTreeMap::from([
                        (
                            "title".into(),
                            FieldSchema {
                                required: true,
                                value: string(),
                            },
                        ),
                        (
                            "detail".into(),
                            FieldSchema {
                                required: false,
                                value: string(),
                            },
                        ),
                    ]),
                },
                events: BTreeMap::from([(
                    EventId::parse("open").unwrap(),
                    ValueSchema::Record {
                        fields: BTreeMap::from([(
                            "item".into(),
                            FieldSchema {
                                required: true,
                                value: string(),
                            },
                        )]),
                    },
                )]),
                viewport: viewport(),
                appearance_abi: APPEARANCE_ABI_VERSION,
                accepts_container_appearance: false,
            },
            BTreeMap::new(),
        );
        let agenda_revision = revision('a');
        let dashboard = package(
            "dashboard",
            "main",
            ExperienceExport {
                properties: ValueSchema::empty_record(),
                events: BTreeMap::new(),
                viewport: viewport(),
                appearance_abi: APPEARANCE_ABI_VERSION,
                accepts_container_appearance: false,
            },
            BTreeMap::from([(
                DependencyAlias::parse("agenda").unwrap(),
                DependencyBinding {
                    experience_id: ExperienceId::parse("agenda").unwrap(),
                    revision_id: agenda_revision.clone(),
                    export_id: ExportId::parse("summary").unwrap(),
                    contract_digest: agenda.contract.digest().unwrap(),
                    policy: DependencyPolicy::Locked,
                    grant: BoundaryGrant {
                        properties: BTreeSet::from(["title".into()]),
                        events: BTreeSet::from([EventId::parse("open").unwrap()]),
                    },
                },
            )]),
        );
        let dashboard_revision = revision('d');
        let root = GraphNodeId::parse("root").unwrap();
        let child = GraphNodeId::parse("agenda-child").unwrap();
        let graph = ResolvedGraph {
            format_version: GRAPH_FORMAT_VERSION,
            root: root.clone(),
            nodes: BTreeMap::from([
                (
                    root.clone(),
                    ResolvedGraphNode {
                        experience_id: ExperienceId::parse("dashboard").unwrap(),
                        revision_id: dashboard_revision.clone(),
                        export_id: ExportId::parse("main").unwrap(),
                        parent: None,
                        dependency: None,
                    },
                ),
                (
                    child.clone(),
                    ResolvedGraphNode {
                        experience_id: ExperienceId::parse("agenda").unwrap(),
                        revision_id: agenda_revision.clone(),
                        export_id: ExportId::parse("summary").unwrap(),
                        parent: Some(root.clone()),
                        dependency: Some(DependencyAlias::parse("agenda").unwrap()),
                    },
                ),
            ]),
        };
        let model = providers_fake::snapshot();
        let inputs = BTreeMap::from([
            (
                dashboard_revision,
                GraphRevisionInput {
                    source: r#"
                        return { api_version = 4, exports = { main = {
                            render = function(_, state)
                                return { id = "root", children = {{
                                    id = "agenda", layout = { width = 320, height = 180 },
                                    content = { kind = "experience_mount", dependency = "agenda",
                                    properties = state.leak and
                                        { title = state.title or "Today", detail = "private" } or
                                        { title = state.title or "Today" } },
                                }} }
                            end,
                            update = function(_, state, event)
                                if event.dependency == "agenda" and event.event == "open" then
                                    if event.payload.item == "explode" then error("parent failed") end
                                    state.opened = event.payload.item
                                elseif event.action == "leak" then
                                    state.leak = true
                                end
                                return { state = state }
                            end,
                        } } }
                    "#
                    .into(),
                    sidecars: vec![],
                    model: model.clone(),
                    state: json!({}),
                    state_schema_version: 1,
                    package: dashboard,
                },
            ),
            (
                agenda_revision,
                GraphRevisionInput {
                    source: r#"
                        return { api_version = 4, exports = { summary = {
                            render = function(_, state, properties)
                                if state.fail then error("child render failed") end
                                return { id = "summary", content = { kind = "text",
                                    value = properties.title, size = 16, color = 0xffffffff } }
                            end,
                            update = function(_, state, event)
                                state.count = (state.count or 0) + 1
                                if event.action == "fail" then
                                    state.fail = true
                                    return { state = state,
                                        effects = {{ provider = "media", action = "play_pause",
                                            payload = {} }},
                                        events = {{ event = "open",
                                            payload = { item = "must-not-escape" } }} }
                                end
                                return { state = state, events = {{ event = "open",
                                    payload = { item = event.value } }} }
                            end,
                        } } }
                    "#
                    .into(),
                    sidecars: vec![],
                    model,
                    state: json!({"child_only": true}),
                    state_schema_version: 1,
                    package: agenda,
                },
            ),
        ]);

        let second = GraphRuntime::start(graph.clone(), inputs.clone())
            .unwrap()
            .snapshot();
        let mut runtime = GraphRuntime::start(graph, inputs).unwrap();
        let initial = runtime.snapshot();
        assert_ne!(
            initial.instances[&root].instance_id,
            initial.instances[&child].instance_id
        );
        assert_ne!(
            initial.instances[&root].instance_id,
            second.instances[&root].instance_id
        );
        assert_eq!(
            initial.instances[&child]
                .scene
                .as_ref()
                .unwrap()
                .root
                .content,
            Some(Content::Text(experience_ir::TextContent {
                value: "Today".into(),
                size: 16.0,
                color: 0xffffffff,
            }))
        );
        let outcome = runtime
            .dispatch_scene_event(
                &child,
                &SceneEvent {
                    action: "open".into(),
                    value: Some("meeting".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(outcome.snapshot.instances[&root].state["opened"], "meeting");
        assert_eq!(outcome.snapshot.instances[&child].state["child_only"], true);
        assert_eq!(outcome.snapshot.instances[&child].state["count"], 1);
        assert!(outcome.external_events.is_empty());

        let restored = runtime.restore(&initial).unwrap();
        assert!(restored.instances[&root].state.get("opened").is_none());
        assert_eq!(restored.instances[&child].state["child_only"], true);

        let failed_child = runtime
            .dispatch_event(&child, &json!({"action": "fail"}))
            .unwrap();
        assert!(matches!(
            &failed_child.snapshot.instances[&child].status,
            RuntimeInstanceStatus::Failed(message) if message.contains("child render failed")
        ));
        assert_eq!(
            failed_child.snapshot.instances[&root].status,
            RuntimeInstanceStatus::Ready
        );
        assert!(failed_child.snapshot.instances[&root]
            .state
            .get("opened")
            .is_none());
        assert!(failed_child.effects.is_empty());
        assert!(failed_child.external_events.is_empty());
        runtime.restore(&initial).unwrap();

        assert!(runtime
            .dispatch_scene_event(
                &child,
                &SceneEvent {
                    action: "open".into(),
                    value: Some("explode".into()),
                    ..Default::default()
                },
            )
            .is_err());
        let after_rejection = runtime.snapshot();
        assert_eq!(
            after_rejection.instances[&root].state,
            initial.instances[&root].state
        );
        assert_eq!(
            after_rejection.instances[&child].state,
            initial.instances[&child].state
        );

        let refused = runtime
            .dispatch_event(&root, &json!({"action": "leak"}))
            .unwrap();
        assert!(matches!(
            &refused.snapshot.instances[&child].status,
            RuntimeInstanceStatus::Failed(message) if message.contains("outside its boundary grant")
        ));
    }

    #[test]
    fn scene_authority_rejects_shell_content_and_v3_application_surfaces() {
        let shell_content = Scene {
            root: SceneNode {
                content: Some(Content::WindowSpace(experience_ir::WindowSpaceContent {
                    layout: experience_ir::WindowLayoutMode::Floating,
                    gap: 0.0,
                    fallback: "empty".into(),
                })),
                ..Default::default()
            },
        };
        assert!(
            validate_scene_authority(&shell_content, ExperienceRole::Ordinary, false)
                .unwrap_err()
                .to_string()
                .contains("shell-only")
        );
        validate_scene_authority(&shell_content, ExperienceRole::Shell, false).unwrap();

        let application = Scene {
            root: SceneNode {
                content: Some(Content::ApplicationSurface(
                    experience_ir::ApplicationSurfaceContent {
                        title: "Demo".into(),
                    },
                )),
                ..Default::default()
            },
        };
        for (role, mounted) in [
            (ExperienceRole::Ordinary, false),
            (ExperienceRole::Ordinary, true),
            (ExperienceRole::Shell, false),
        ] {
            assert!(validate_scene_authority(&application, role, mounted)
                .unwrap_err()
                .to_string()
                .contains("v3-only application_surface"));
        }
    }

    #[test]
    fn instance_namespace_covers_assets_shaders_and_provider_surfaces() {
        let instance_id = InstanceId::parse("i-boundary").unwrap();
        let mut root = SceneNode {
            content: Some(Content::Image(experience_ir::ImageContent {
                asset: "assets/image.png".into(),
            })),
            paint: vec![PaintOp::Layer {
                clip: None,
                transform: experience_ir::Transform2D::default(),
                opacity: 1.0,
                operations: vec![PaintOp::Shader {
                    asset: "assets/effect.wgsl".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                }],
            }],
            children: vec![SceneNode {
                content: Some(Content::ProviderSurface(
                    experience_ir::ProviderSurfaceContent {
                        surface: "camera".into(),
                    },
                )),
                ..Default::default()
            }],
            ..Default::default()
        };
        namespace_instance_scene(&mut root, &instance_id);
        let Some(Content::Image(image)) = &root.content else {
            panic!("image remains present")
        };
        assert_eq!(image.asset, "instances/i-boundary/assets/image.png");
        let PaintOp::Layer { operations, .. } = &root.paint[0] else {
            panic!("layer remains present")
        };
        let PaintOp::Shader { asset, .. } = &operations[0] else {
            panic!("shader remains present")
        };
        assert_eq!(asset, "instances/i-boundary/assets/effect.wgsl");
        let Some(Content::ProviderSurface(surface)) = &root.children[0].content else {
            panic!("provider surface remains present")
        };
        assert_eq!(surface.surface, "i-boundary::camera");
    }
}
