use std::collections::BTreeMap;

use experience_ir::{
    AppearanceProfile, Content, ExperienceEvent, ExperienceModel, ExperienceOutputEvent,
    ExperienceViewport, ProviderEffect, Scene, SceneEvent, SceneNode,
};
use experience_package::{
    DependencyAlias, EventId, GraphNodeId, PackageMetadata, ResolvedGraph, RevisionId,
    MAX_GRAPH_INSTANCES,
};
use serde_json::{json, Value as JsonValue};

use crate::{LuauRuntime, RevisionAsset, RevisionAssetInput, RuntimeError};

#[derive(Clone, Debug)]
pub struct GraphRevisionInput {
    pub source: String,
    pub sidecars: Vec<RevisionAssetInput>,
    pub model: ExperienceModel,
    pub state: JsonValue,
    pub state_schema_version: u64,
    pub package: PackageMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeInstanceStatus {
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeInstanceSnapshot {
    pub experience_id: experience_package::ExperienceId,
    pub revision_id: RevisionId,
    pub export_id: experience_package::ExportId,
    pub parent: Option<GraphNodeId>,
    pub dependency: Option<DependencyAlias>,
    pub state: JsonValue,
    pub scene: Option<Scene>,
    pub status: RuntimeInstanceStatus,
    pub assets: Vec<RevisionAsset>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphRuntimeSnapshot {
    pub graph_id: String,
    pub root: GraphNodeId,
    pub instances: BTreeMap<GraphNodeId, RuntimeInstanceSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphEffect {
    pub node_id: GraphNodeId,
    pub revision_id: RevisionId,
    pub effect: ProviderEffect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphActionOutcome {
    pub snapshot: GraphRuntimeSnapshot,
    pub effects: Vec<GraphEffect>,
    pub external_events: Vec<ExperienceOutputEvent>,
}

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
    Restore {
        request_id: u64,
        snapshot: GraphRuntimeSnapshot,
    },
    Shutdown,
}

#[derive(Clone, Debug, PartialEq)]
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

pub struct GraphRuntimeWorker {
    commands: async_channel::Sender<GraphWorkerCommand>,
    results: async_channel::Receiver<GraphWorkerResult>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl GraphRuntimeWorker {
    pub fn start(
        graph: ResolvedGraph,
        inputs: BTreeMap<RevisionId, GraphRevisionInput>,
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
                if ready_tx.send_blocking(Ok(runtime.snapshot())).is_err() {
                    return;
                }
                while let Ok(command) = commands_rx.recv_blocking() {
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
                        GraphWorkerCommand::Shutdown => break,
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
                thread: Some(thread),
            },
            ready,
        ))
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
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| "graph runtime worker panicked during shutdown".to_owned())?;
        }
        Ok(())
    }
}

impl Drop for GraphRuntimeWorker {
    fn drop(&mut self) {
        self.commands.close();
    }
}

struct RuntimeInstance {
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
                            experience_id: node.experience_id.clone(),
                            revision_id: node.revision_id.clone(),
                            export_id: node.export_id.clone(),
                            parent: node.parent.clone(),
                            dependency: node.dependency.clone(),
                            state: instance.state.clone(),
                            scene: instance.scene.clone(),
                            status: instance.status.clone(),
                            assets: instance.runtime.assets().to_vec(),
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
        if let Err(error) = self.update_node(node_id, event, &mut effects, &mut external_events, 0)
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
        if let Err(error) = self.render_subtree(&root, true) {
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

    pub fn restore(
        &mut self,
        snapshot: &GraphRuntimeSnapshot,
    ) -> Result<GraphRuntimeSnapshot, RuntimeError> {
        if snapshot.graph_id != self.graph_id
            || snapshot.root != self.graph.root
            || snapshot.instances.len() != self.instances.len()
            || snapshot.instances.iter().any(|(node_id, saved)| {
                self.graph.nodes.get(node_id).is_none_or(|node| {
                    saved.experience_id != node.experience_id
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
            instance.state = saved.state.clone();
        }
        let root = self.graph.root.clone();
        self.render_subtree(&root, true)?;
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
            )?
        };
        self.validate_output_events(node_id, &output.events)?;
        let mut candidate_scene = {
            let instance = self.instances.get(node_id).expect("graph instance exists");
            instance.runtime.render_export(
                graph_node.export_id.as_str(),
                &instance.model,
                &output.state,
                &instance.properties,
                instance.viewport.clone(),
                instance.container_appearance.clone(),
            )?
        };
        validate_scene_authority(
            &candidate_scene,
            self.instances[node_id].package.role,
            graph_node.parent.is_some(),
        )?;
        namespace_provider_surfaces(&mut candidate_scene.root, graph_node.revision_id.as_str());
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
            instance.scene = Some(candidate_scene);
            instance.status = RuntimeInstanceStatus::Ready;
        }
        for peer in &peers {
            self.instances
                .get_mut(peer)
                .expect("peer graph instance exists")
                .state = candidate_state.clone();
        }
        for effect in output.effects {
            effects.push(GraphEffect {
                node_id: node_id.clone(),
                revision_id: graph_node.revision_id.clone(),
                effect,
            });
        }
        self.render_children(node_id)?;
        for peer in peers {
            self.render_subtree(&peer, peer == self.graph.root)?;
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
                namespace_provider_surfaces(&mut scene.root, graph_node.revision_id.as_str());
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
            Some(Content::ApplicationSurface(_)) if mounted => {
                return Err(RuntimeError::Invalid(
                    "mounted experience emitted a native application surface".into(),
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

fn namespace_provider_surfaces(node: &mut SceneNode, revision_id: &str) {
    if let Some(Content::ProviderSurface(surface)) = &mut node.content {
        surface.surface = format!("{revision_id}::{}", surface.surface);
    }
    for child in &mut node.children {
        namespace_provider_surfaces(child, revision_id);
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
        }
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
                            render = function(_, _, properties)
                                return { id = "summary", content = { kind = "text",
                                    value = properties.title, size = 16, color = 0xffffffff } }
                            end,
                            update = function(_, state, event)
                                state.count = (state.count or 0) + 1
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

        let mut runtime = GraphRuntime::start(graph, inputs).unwrap();
        let initial = runtime.snapshot();
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
    fn scene_authority_rejects_shell_content_and_mounted_application_surfaces() {
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
        assert!(
            validate_scene_authority(&application, ExperienceRole::Ordinary, true)
                .unwrap_err()
                .to_string()
                .contains("native application surface")
        );
    }
}
