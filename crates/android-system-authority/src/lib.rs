use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub use android_authority_protocol::MAX_PROVIDER_REQUEST_BYTES;
use android_authority_protocol::{
    GraphBundle, GraphEffectWire, GraphRevisionWire, GraphStateUpdateWire, RevisionAssetWire,
    RevisionRequest, RevisionResponse,
};
use experience_ir::{
    ProviderEffect, ProviderRequest, ProviderResponse, StateEnvelope, MAX_EFFECTS, MAX_STATE_BYTES,
};
use experience_package::{AppearanceProfile, ExperienceId, ExportId, ResolvedGraph};
use revision_supervisor::{
    DurableState, ExperienceRegistry, GraphResolver, GraphStore, RevisionAssetInput, RevisionInput,
    RevisionPackageInput, RevisionStore, VerifiedRevision, STOCK_SHELL_EXPERIENCE_ID,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use service_protocol::{
    AppearanceResource, DataFlowGrant, ExperienceStateResource, GrantDecisionResource,
    StateResource,
};

mod provider_registry;
mod state_service;

use provider_registry::{SystemAction, SystemProviderRegistry};
use state_service::StateService;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActivationJournal {
    revision_id: String,
    state_stage_id: u64,
}

const COMPOSITION_AUTHORITY_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CompositionAuthorityState {
    format_version: u32,
    #[serde(default)]
    states: BTreeMap<ExperienceId, StateResource>,
    #[serde(default)]
    appearance: AppearanceResource,
    #[serde(default)]
    grants: BTreeMap<ExperienceId, GrantDecisionResource>,
}

impl Default for CompositionAuthorityState {
    fn default() -> Self {
        Self {
            format_version: COMPOSITION_AUTHORITY_FORMAT_VERSION,
            states: BTreeMap::new(),
            appearance: AppearanceResource::default(),
            grants: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PendingGraphMigration {
    experience_id: ExperienceId,
    revision_id: String,
    graph_id: String,
}

pub struct AndroidSystemAuthority {
    revisions: RevisionStore,
    registry: ExperienceRegistry,
    resolver: GraphResolver,
    graphs: GraphStore,
    state: StateService,
    staged_effects: HashMap<u64, Vec<SystemAction>>,
    providers: SystemProviderRegistry,
    stock_revision_id: String,
    state_file: PathBuf,
    journal_file: PathBuf,
    composition_file: PathBuf,
    composition: CompositionAuthorityState,
    pending_graph_file: PathBuf,
    legacy_fallback_file: PathBuf,
    appearance_writer: Option<String>,
}

impl AndroidSystemAuthority {
    pub fn open(
        revision_root: impl Into<PathBuf>,
        state_file: impl Into<PathBuf>,
        bootstrap_source: &[u8],
    ) -> Result<Self, String> {
        let revision_root = revision_root.into();
        let state_file = state_file.into();
        let revisions = RevisionStore::open(&revision_root).map_err(|error| error.to_string())?;
        // The bootstrap is immutable product content (AVB/OTA signed on the
        // device) and is pinned independently of the mutable current pointer.
        let stock = revisions
            .install(RevisionInput {
                source: bootstrap_source.to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: experience_ir::EXPERIENCE_API_VERSION,
                assets: Vec::new(),
            })
            .map_err(|error| error.to_string())?;
        let current = match revisions.current().map_err(|error| error.to_string())? {
            Some(current) => current,
            None => {
                revisions
                    .set_current(&stock.manifest.revision_id)
                    .map_err(|error| error.to_string())?;
                stock.clone()
            }
        };
        Self::finish_open(revision_root, state_file, revisions, stock, current)
    }

    pub fn open_v4(
        revision_root: impl Into<PathBuf>,
        state_file: impl Into<PathBuf>,
        bootstrap: RevisionPackageInput,
    ) -> Result<Self, String> {
        let revision_root = revision_root.into();
        let state_file = state_file.into();
        let revisions = RevisionStore::open(&revision_root).map_err(|error| error.to_string())?;
        let previous_singleton = revisions.current().map_err(|error| error.to_string())?;
        let stock = revisions
            .install_package(bootstrap)
            .map_err(|error| error.to_string())?;
        let current = match previous_singleton.clone() {
            Some(current) => current,
            None => {
                revisions
                    .set_current(&stock.manifest.revision_id)
                    .map_err(|error| error.to_string())?;
                stock.clone()
            }
        };
        let mut authority =
            Self::finish_open(revision_root, state_file, revisions, stock.clone(), current)?;
        authority.initialize_v4_stock(&stock, previous_singleton.as_ref())?;
        Ok(authority)
    }

    fn finish_open(
        revision_root: PathBuf,
        state_file: PathBuf,
        revisions: RevisionStore,
        stock: VerifiedRevision,
        current: VerifiedRevision,
    ) -> Result<Self, String> {
        let initial = if state_file.exists() {
            serde_json::from_slice::<StateEnvelope>(
                &fs::read(&state_file).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?
        } else {
            StateEnvelope {
                revision: 0,
                schema_version: current.manifest.schema_version,
                source_sha256: current.manifest.source.sha256.clone(),
                state: json!({}),
            }
        };
        let journal_file = revision_root.join("activation-journal.json");
        let composition_file = state_file.with_extension("composition.json");
        let composition = match fs::read(&composition_file) {
            Ok(bytes) => {
                let composition: CompositionAuthorityState =
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                if composition.format_version != COMPOSITION_AUTHORITY_FORMAT_VERSION {
                    return Err("unsupported Android composition authority format".into());
                }
                composition
                    .appearance
                    .profile
                    .validate()
                    .map_err(|error| error.to_string())?;
                composition
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                CompositionAuthorityState::default()
            }
            Err(error) => return Err(error.to_string()),
        };
        let registry =
            ExperienceRegistry::open(revisions.clone()).map_err(|error| error.to_string())?;
        let resolver = GraphResolver::new(revisions.clone());
        let graphs = GraphStore::open(&revision_root).map_err(|error| error.to_string())?;
        let mut authority = Self {
            revisions,
            registry,
            resolver,
            graphs,
            state: StateService::new(initial),
            staged_effects: HashMap::new(),
            providers: SystemProviderRegistry::android(),
            stock_revision_id: stock.manifest.revision_id,
            state_file,
            journal_file,
            composition_file,
            composition,
            pending_graph_file: revision_root.join("pending-v4-graph.json"),
            legacy_fallback_file: revision_root.join("legacy-v3-fallback"),
            appearance_writer: None,
        };
        authority.persist_state()?;
        authority.persist_composition()?;
        authority.recover_activation()?;
        authority.ensure_consistent()?;
        Ok(authority)
    }

    fn initialize_v4_stock(
        &mut self,
        stock: &VerifiedRevision,
        previous_singleton: Option<&VerifiedRevision>,
    ) -> Result<(), String> {
        let package = stock
            .package
            .as_ref()
            .ok_or_else(|| "Android v4 bootstrap lacks package metadata".to_owned())?;
        let stock_id =
            ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID).map_err(|error| error.to_string())?;
        if package.experience_id != stock_id
            || package.role != experience_package::ExperienceRole::Shell
        {
            return Err("Android v4 bootstrap is not the reserved Stock Shell".into());
        }
        if self
            .registry
            .get(&stock_id)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            if previous_singleton.is_some_and(|revision| revision.package.is_none()) {
                self.registry
                    .migrate_legacy_current()
                    .map_err(|error| error.to_string())?;
            } else {
                self.registry
                    .create(
                        &stock_id,
                        experience_package::ExperienceRole::Shell,
                        &stock.manifest.revision_id,
                    )
                    .map_err(|error| error.to_string())?;
            }
        }

        if !self.composition.states.contains_key(&stock_id) {
            let legacy = self.state.load();
            self.composition.states.insert(
                stock_id.clone(),
                StateResource {
                    revision: legacy.revision,
                    revision_id: stock.manifest.revision_id.clone(),
                    schema_version: stock.manifest.schema_version,
                    source_sha256: stock.manifest.source.sha256.clone(),
                    state: legacy.state,
                },
            );
        }
        self.composition.grants.insert(
            stock_id.clone(),
            GrantDecisionResource {
                generation: self
                    .composition
                    .grants
                    .get(&stock_id)
                    .map_or(1, |grant| grant.generation.max(1)),
                reviewed: true,
                experience_id: stock_id.clone(),
                provider_capabilities: package.provider_capabilities.clone(),
                data_flows: package
                    .dependencies
                    .iter()
                    .filter(|(_, binding)| {
                        !binding.grant.properties.is_empty() || !binding.grant.events.is_empty()
                    })
                    .map(|(alias, binding)| {
                        (
                            alias.clone(),
                            DataFlowGrant {
                                experience_id: binding.experience_id.clone(),
                                export_id: binding.export_id.clone(),
                                grant: binding.grant.clone(),
                            },
                        )
                    })
                    .collect(),
            },
        );

        let current = self
            .registry
            .current(&stock_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Stock registry record has no current revision".to_owned())?;
        if current.package.is_none() {
            let graph = self
                .resolver
                .resolve(
                    &stock.manifest.revision_id,
                    &ExportId::parse("main").map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            let graph_id = self
                .graphs
                .install(&graph)
                .map_err(|error| error.to_string())?;
            self.write_pending_graph(&PendingGraphMigration {
                experience_id: stock_id,
                revision_id: stock.manifest.revision_id.clone(),
                graph_id,
            })?;
        } else {
            let graph = self
                .resolver
                .resolve(
                    &current.manifest.revision_id,
                    &ExportId::parse("main").map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            let graph_id = self
                .graphs
                .install(&graph)
                .map_err(|error| error.to_string())?;
            self.graphs
                .set_current(&stock_id, &graph_id)
                .map_err(|error| error.to_string())?;
        }
        self.persist_composition()
    }

    fn persist_composition(&self) -> Result<(), String> {
        let temporary = self.composition_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.composition_file,
            &serde_json::to_vec_pretty(&self.composition).map_err(|error| error.to_string())?,
        )
    }

    fn write_pending_graph(&self, pending: &PendingGraphMigration) -> Result<(), String> {
        let temporary = self.pending_graph_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.pending_graph_file,
            &serde_json::to_vec_pretty(pending).map_err(|error| error.to_string())?,
        )
    }

    fn pending_graph(&self) -> Result<Option<PendingGraphMigration>, String> {
        match fs::read(&self.pending_graph_file) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| error.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn configure_appearance_writer(&mut self, capability: &str) -> Result<(), String> {
        if capability.is_empty() || capability.len() > 256 {
            return Err("appearance-write capability must contain 1 to 256 bytes".into());
        }
        match self.appearance_writer.as_deref() {
            Some(existing) if existing != capability => {
                Err("appearance-write capability does not match authority".into())
            }
            Some(_) => Ok(()),
            None => {
                self.appearance_writer = Some(capability.to_owned());
                Ok(())
            }
        }
    }

    pub fn dispatch_provider(&mut self, request: ProviderRequest) -> ProviderResponse {
        let request_id = request.request_id();
        match request {
            ProviderRequest::Snapshot { .. } => ProviderResponse {
                model: Some(self.providers.snapshot_model()),
                ..provider_response(request_id, true)
            },
            ProviderRequest::Action {
                provider,
                action,
                payload,
                ..
            } => match self.providers.parse_and_authorize(&ProviderEffect {
                provider,
                action,
                payload,
            }) {
                Ok(action) => match self.providers.execute(&action) {
                    Ok(result) => ProviderResponse {
                        result: Some(result),
                        ..provider_response(request_id, true)
                    },
                    Err(error) => provider_failure(request_id, &error),
                },
                Err(error) => provider_failure(request_id, &error),
            },
            ProviderRequest::LoadState { .. } => ProviderResponse {
                state: Some(self.state.load()),
                ..provider_response(request_id, true)
            },
            ProviderRequest::StageState {
                expected_revision,
                schema_version,
                state,
                source_sha256,
                effects,
                ..
            } => {
                let actions = match self.authorize_effects(&effects) {
                    Ok(actions) => actions,
                    Err(error) => return provider_failure(request_id, &error),
                };
                match self
                    .state
                    .stage(expected_revision, schema_version, state, source_sha256)
                {
                    Ok(stage_id) => {
                        self.staged_effects.insert(stage_id, actions);
                        ProviderResponse {
                            stage_id: Some(stage_id),
                            ..provider_response(request_id, true)
                        }
                    }
                    Err(error) => provider_failure(request_id, &error),
                }
            }
            ProviderRequest::PromoteState { stage_id, .. } => match self.promote_state(stage_id) {
                Ok(state) => ProviderResponse {
                    state: Some(state),
                    ..provider_response(request_id, true)
                },
                Err(error) => provider_failure(request_id, &error),
            },
            ProviderRequest::AbortState { stage_id, .. } => {
                let removed = self.state.abort(stage_id);
                self.staged_effects.remove(&stage_id);
                ProviderResponse {
                    result: Some(json!({ "removed": removed })),
                    ..provider_response(request_id, true)
                }
            }
            ProviderRequest::ConfigureStateFault { point, .. } => {
                self.state.configure_fault(point);
                provider_response(request_id, true)
            }
        }
    }

    pub fn dispatch_revision(&mut self, request: RevisionRequest) -> RevisionResponse {
        let request_id = request.request_id();
        let result = match request {
            RevisionRequest::Current { .. } => self.current_response(request_id),
            RevisionRequest::CurrentGraph { .. } => self.current_graph_response(request_id),
            RevisionRequest::ConfirmGraph { graph_id, .. } => {
                self.confirm_graph_response(request_id, &graph_id)
            }
            RevisionRequest::RollbackGraph {
                failed_graph_id, ..
            } => self.rollback_graph_response(request_id, &failed_graph_id),
            RevisionRequest::CommitGraphAction {
                graph_id,
                updates,
                effects,
                ..
            } => self.commit_graph_action_response(request_id, &graph_id, updates, effects),
            RevisionRequest::CurrentAppearance { .. } => Ok(RevisionResponse {
                request_id,
                ok: true,
                appearance: Some(self.composition.appearance.clone()),
                ..RevisionResponse::default()
            }),
            RevisionRequest::UpdateAppearance {
                expected_generation,
                capability,
                profile,
                ..
            } => self.update_appearance_response(
                request_id,
                expected_generation,
                &capability,
                profile,
            ),
            RevisionRequest::Install {
                source,
                state,
                schema_version,
                experience_api_version,
                assets,
                ..
            } => self.install_response(
                request_id,
                source,
                state,
                schema_version,
                experience_api_version,
                assets,
            ),
            RevisionRequest::Activate {
                revision_id,
                state_stage_id,
                ..
            } => self.activate_response(request_id, &revision_id, state_stage_id),
            RevisionRequest::FallbackToStock {
                failed_revision_id, ..
            } => self.fallback_to_stock_response(request_id, &failed_revision_id),
        };
        result.unwrap_or_else(|error| RevisionResponse {
            request_id,
            ok: false,
            error: Some(error),
            ..RevisionResponse::default()
        })
    }

    fn current_response(&self, request_id: u64) -> Result<RevisionResponse, String> {
        let current = self
            .revisions
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "revision authority has no current revision".to_owned())?;
        revision_response(
            request_id,
            &current,
            Some(self.state.load()),
            &self.stock_revision_id,
            false,
        )
    }

    fn current_graph_response(&mut self, request_id: u64) -> Result<RevisionResponse, String> {
        if self.legacy_fallback_file.exists() {
            return self.current_response(request_id);
        }
        if let Some(pending) = self.pending_graph()? {
            let graph = self
                .graphs
                .verify(&pending.graph_id)
                .map_err(|error| error.to_string())?;
            return self.graph_response(request_id, pending.graph_id, graph, true, false);
        }
        let stock_id =
            ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID).map_err(|error| error.to_string())?;
        let Some((graph_id, graph)) = self
            .graphs
            .current(&stock_id)
            .map_err(|error| error.to_string())?
        else {
            return self.current_response(request_id);
        };
        self.graph_response(request_id, graph_id, graph, false, false)
    }

    fn confirm_graph_response(
        &mut self,
        request_id: u64,
        graph_id: &str,
    ) -> Result<RevisionResponse, String> {
        let pending = self
            .pending_graph()?
            .ok_or_else(|| "no Android v4 graph migration awaits confirmation".to_owned())?;
        if pending.graph_id != graph_id {
            return Err("graph confirmation does not name the pending graph".into());
        }
        self.graphs
            .verify(graph_id)
            .map_err(|error| error.to_string())?;
        self.registry
            .set_current(&pending.experience_id, &pending.revision_id)
            .map_err(|error| error.to_string())?;
        self.graphs
            .set_current(&pending.experience_id, graph_id)
            .map_err(|error| error.to_string())?;
        remove_synced(&self.pending_graph_file)?;
        remove_synced(&self.legacy_fallback_file)?;
        self.current_graph_response(request_id)
    }

    fn rollback_graph_response(
        &mut self,
        request_id: u64,
        failed_graph_id: &str,
    ) -> Result<RevisionResponse, String> {
        if let Some(pending) = self.pending_graph()? {
            if pending.graph_id != failed_graph_id {
                return Err("rollback does not name the pending Android graph".into());
            }
            remove_synced(&self.pending_graph_file)?;
            let current = self
                .registry
                .current(&pending.experience_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Stock registry lost its legacy rollback revision".to_owned())?;
            return revision_response(
                request_id,
                &current,
                Some(self.state.load()),
                &self.stock_revision_id,
                true,
            );
        }

        let stock_id =
            ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID).map_err(|error| error.to_string())?;
        let (current_graph_id, _) = self
            .graphs
            .current(&stock_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android authority has no active v4 graph".to_owned())?;
        if current_graph_id != failed_graph_id {
            return Err("rollback does not name the active Android graph".into());
        }
        let previous_revision = self
            .registry
            .previous(&stock_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android Stock registry has no rollback revision".to_owned())?;
        if previous_revision.package.is_some() {
            let (previous_graph_id, previous_graph) = self
                .graphs
                .previous(&stock_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Android graph store has no rollback graph".to_owned())?;
            self.registry
                .set_current(&stock_id, &previous_revision.manifest.revision_id)
                .map_err(|error| error.to_string())?;
            self.graphs
                .set_current(&stock_id, &previous_graph_id)
                .map_err(|error| error.to_string())?;
            return self.graph_response(request_id, previous_graph_id, previous_graph, false, true);
        }

        self.registry
            .set_current(&stock_id, &previous_revision.manifest.revision_id)
            .map_err(|error| error.to_string())?;
        let temporary = self.legacy_fallback_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.legacy_fallback_file,
            previous_revision.manifest.revision_id.as_bytes(),
        )?;
        revision_response(
            request_id,
            &previous_revision,
            Some(self.state.load()),
            &self.stock_revision_id,
            true,
        )
    }

    fn graph_for_action(&self, graph_id: &str) -> Result<ResolvedGraph, String> {
        if self.legacy_fallback_file.exists() {
            return Err("v4 graph actions are disabled during legacy rollback".into());
        }
        if let Some(pending) = self.pending_graph()? {
            if pending.graph_id != graph_id {
                return Err("graph action does not name the pending Android graph".into());
            }
            return self
                .graphs
                .verify(graph_id)
                .map_err(|error| error.to_string());
        }
        let stock_id =
            ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID).map_err(|error| error.to_string())?;
        let (current_id, graph) = self
            .graphs
            .current(&stock_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android authority has no active v4 graph".to_owned())?;
        if current_id != graph_id {
            return Err("graph action does not name the active Android graph".into());
        }
        Ok(graph)
    }

    fn commit_graph_action_response(
        &mut self,
        request_id: u64,
        graph_id: &str,
        updates: Vec<GraphStateUpdateWire>,
        effects: Vec<GraphEffectWire>,
    ) -> Result<RevisionResponse, String> {
        if updates.len() > experience_package::MAX_GRAPH_INSTANCES {
            return Err("graph action contains too many state updates".into());
        }
        if effects.len() > MAX_EFFECTS {
            return Err("graph action contains too many provider effects".into());
        }
        let graph = self.graph_for_action(graph_id)?;
        let mut update_by_node = BTreeMap::new();
        let mut next_states = BTreeMap::<ExperienceId, StateResource>::new();
        for update in &updates {
            let node = graph.nodes.get(&update.node_id).ok_or_else(|| {
                format!("graph state update names unknown node `{}`", update.node_id)
            })?;
            if node.experience_id != update.experience_id || node.revision_id != update.revision_id
            {
                return Err(format!(
                    "graph state identity does not match node `{}`",
                    update.node_id
                ));
            }
            if serde_json::to_vec(&update.state)
                .map_err(|error| error.to_string())?
                .len()
                > MAX_STATE_BYTES
            {
                return Err(format!(
                    "graph state for `{}` exceeds its size limit",
                    update.experience_id
                ));
            }
            let current = self
                .composition
                .states
                .get(&update.experience_id)
                .ok_or_else(|| {
                    format!(
                        "authority has no state for experience `{}`",
                        update.experience_id
                    )
                })?;
            if current.revision_id != update.revision_id.as_str()
                || current.revision != update.expected_revision
            {
                return Err(format!(
                    "graph state conflict for experience `{}`",
                    update.experience_id
                ));
            }
            let verified = self
                .revisions
                .verify(update.revision_id.as_str())
                .map_err(|error| error.to_string())?;
            let candidate = StateResource {
                revision: current.revision.saturating_add(1),
                revision_id: update.revision_id.to_string(),
                schema_version: verified.manifest.schema_version,
                source_sha256: verified.manifest.source.sha256,
                state: update.state.clone(),
            };
            if let Some(existing) = next_states.get(&update.experience_id) {
                if existing.state != candidate.state
                    || existing.revision_id != candidate.revision_id
                {
                    return Err(format!(
                        "instances of experience `{}` produced divergent state",
                        update.experience_id
                    ));
                }
            } else {
                next_states.insert(update.experience_id.clone(), candidate);
            }
            if update_by_node
                .insert(update.node_id.clone(), update)
                .is_some()
            {
                return Err(format!(
                    "graph action repeats state update for node `{}`",
                    update.node_id
                ));
            }
        }

        let mut actions = Vec::new();
        for graph_effect in &effects {
            let node = graph.nodes.get(&graph_effect.node_id).ok_or_else(|| {
                format!("graph effect names unknown node `{}`", graph_effect.node_id)
            })?;
            if node.revision_id != graph_effect.revision_id {
                return Err("graph effect revision does not match its node".into());
            }
            let update = update_by_node.get(&graph_effect.node_id).ok_or_else(|| {
                "every graph effect must carry its instance's authoritative state".to_owned()
            })?;
            if update.instance_id != graph_effect.instance_id {
                return Err("graph effect instance does not match its state update".into());
            }
            let required = provider_grant_for_effect(&graph_effect.effect)?;
            let grant = self
                .composition
                .grants
                .get(&node.experience_id)
                .filter(|grant| grant.reviewed)
                .ok_or_else(|| {
                    format!("experience `{}` has no reviewed grant", node.experience_id)
                })?;
            if !grant.provider_capabilities.contains(required) {
                return Err(format!(
                    "experience `{}` lacks provider grant `{required}`",
                    node.experience_id
                ));
            }
            let package = self
                .revisions
                .verify(graph_effect.revision_id.as_str())
                .map_err(|error| error.to_string())?
                .package
                .ok_or_else(|| "graph effect revision has no v4 package".to_owned())?;
            if !package.provider_capabilities.contains(required) {
                return Err(format!(
                    "revision `{}` did not request provider capability `{required}`",
                    graph_effect.revision_id
                ));
            }
            actions.push(self.providers.parse_and_authorize(&graph_effect.effect)?);
        }

        let mut next = self.composition.clone();
        for (experience_id, state) in &next_states {
            next.states.insert(experience_id.clone(), state.clone());
        }
        self.replace_composition(next)?;
        for action in &actions {
            match self.providers.execute(action) {
                Ok(result) => println!(
                    "android_graph_action_promoted graph_id={graph_id} action={action:?} result={result}"
                ),
                Err(error) => eprintln!(
                    "android_graph_action_effect_failed graph_id={graph_id} action={action:?} error={error}"
                ),
            }
        }
        Ok(RevisionResponse {
            request_id,
            ok: true,
            states: next_states
                .into_iter()
                .map(|(experience_id, resource)| ExperienceStateResource {
                    experience_id,
                    resource,
                })
                .collect(),
            ..RevisionResponse::default()
        })
    }

    fn update_appearance_response(
        &mut self,
        request_id: u64,
        expected_generation: u64,
        capability: &str,
        profile: AppearanceProfile,
    ) -> Result<RevisionResponse, String> {
        if self.appearance_writer.as_deref() != Some(capability) {
            return Err("appearance-write capability denied".into());
        }
        profile.validate().map_err(|error| error.to_string())?;
        if self.composition.appearance.profile.generation != expected_generation {
            return Err(format!(
                "appearance generation conflict: expected {expected_generation}, current {}",
                self.composition.appearance.profile.generation
            ));
        }
        if profile.generation != expected_generation.saturating_add(1) {
            return Err("new appearance generation must follow the expected generation".into());
        }
        let appearance = AppearanceResource { profile };
        let mut next = self.composition.clone();
        next.appearance = appearance.clone();
        self.replace_composition(next)?;
        Ok(RevisionResponse {
            request_id,
            ok: true,
            appearance: Some(appearance),
            ..RevisionResponse::default()
        })
    }

    fn replace_composition(&mut self, next: CompositionAuthorityState) -> Result<(), String> {
        let temporary = self.composition_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.composition_file,
            &serde_json::to_vec_pretty(&next).map_err(|error| error.to_string())?,
        )?;
        self.composition = next;
        Ok(())
    }

    fn graph_response(
        &mut self,
        request_id: u64,
        graph_id: String,
        graph: ResolvedGraph,
        migration_pending: bool,
        fallback_performed: bool,
    ) -> Result<RevisionResponse, String> {
        self.validate_graph_grants(&graph)?;
        let mut revisions = Vec::new();
        let mut seeded_state = false;
        let mut seen = BTreeMap::<String, ExperienceId>::new();
        for node in graph.nodes.values() {
            seen.entry(node.revision_id.to_string())
                .or_insert_with(|| node.experience_id.clone());
        }
        for (revision_id, experience_id) in seen {
            let revision = self
                .revisions
                .verify(&revision_id)
                .map_err(|error| error.to_string())?;
            let mut package = revision
                .package
                .clone()
                .ok_or_else(|| "v4 graph contains a legacy revision".to_owned())?;
            let record = self
                .registry
                .get(&experience_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!("v4 graph experience `{experience_id}` is not registered")
                })?;
            package.role = record.role;
            let state = match self.composition.states.get(&experience_id) {
                Some(state) if state.revision_id == revision_id => state.clone(),
                _ => {
                    let durable: DurableState = serde_json::from_slice(
                        &fs::read(revision.directory.join(&revision.manifest.state.path))
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                    let state = StateResource {
                        revision: 0,
                        revision_id: revision_id.clone(),
                        schema_version: durable.schema_version,
                        source_sha256: durable.source_sha256,
                        state: durable.state,
                    };
                    self.composition
                        .states
                        .insert(experience_id.clone(), state.clone());
                    seeded_state = true;
                    state
                }
            };
            revisions.push(GraphRevisionWire {
                revision_id: revision_id.clone(),
                source: fs::read_to_string(revision.directory.join(&revision.manifest.source.path))
                    .map_err(|error| error.to_string())?,
                assets: revision_assets(&revision)?,
                package,
                state: ExperienceStateResource {
                    experience_id,
                    resource: state,
                },
            });
        }
        if seeded_state {
            self.persist_composition()?;
        }
        let mut grants = BTreeMap::new();
        for node in graph.nodes.values() {
            if let Some(grant) = self.composition.grants.get(&node.experience_id) {
                grants.insert(node.experience_id.clone(), grant.clone());
            }
        }
        Ok(RevisionResponse {
            request_id,
            ok: true,
            stock_revision_id: Some(self.stock_revision_id.clone()),
            stock_trusted: true,
            fallback_performed,
            graph: Some(GraphBundle {
                graph_id,
                graph,
                revisions,
                appearance: self.composition.appearance.clone(),
                grants: grants.into_values().collect(),
                migration_pending,
            }),
            ..RevisionResponse::default()
        })
    }

    fn validate_graph_grants(&self, graph: &ResolvedGraph) -> Result<(), String> {
        for node in graph.nodes.values() {
            let revision = self
                .revisions
                .verify(node.revision_id.as_str())
                .map_err(|error| error.to_string())?;
            let package = revision
                .package
                .as_ref()
                .ok_or_else(|| "v4 graph contains a legacy revision".to_owned())?;
            let requested_flows = package
                .dependencies
                .iter()
                .filter(|(_, binding)| {
                    !binding.grant.properties.is_empty() || !binding.grant.events.is_empty()
                })
                .collect::<BTreeMap<_, _>>();
            if package.provider_capabilities.is_empty() && requested_flows.is_empty() {
                continue;
            }
            let decision = self
                .composition
                .grants
                .get(&node.experience_id)
                .filter(|decision| decision.reviewed)
                .ok_or_else(|| {
                    format!(
                        "experience `{}` lacks a reviewed authority grant",
                        node.experience_id
                    )
                })?;
            if !package
                .provider_capabilities
                .is_subset(&decision.provider_capabilities)
                || requested_flows.iter().any(|(alias, requested)| {
                    decision.data_flows.get(alias).is_none_or(|approved| {
                        approved.experience_id != requested.experience_id
                            || approved.export_id != requested.export_id
                            || !requested
                                .grant
                                .properties
                                .is_subset(&approved.grant.properties)
                            || !requested.grant.events.is_subset(&approved.grant.events)
                    })
                })
            {
                return Err(format!(
                    "revision `{}` lacks an exact Android authority grant decision",
                    node.revision_id
                ));
            }
        }
        Ok(())
    }

    fn install_response(
        &self,
        request_id: u64,
        source: String,
        state: serde_json::Value,
        schema_version: u64,
        experience_api_version: u32,
        assets: Vec<RevisionAssetWire>,
    ) -> Result<RevisionResponse, String> {
        let revision = self
            .revisions
            .install(RevisionInput {
                source: source.into_bytes(),
                state,
                schema_version,
                experience_api_version,
                assets: assets
                    .into_iter()
                    .map(|asset| RevisionAssetInput {
                        id: asset.id,
                        kind: asset.kind,
                        bytes: asset.bytes,
                    })
                    .collect(),
            })
            .map_err(|error| error.to_string())?;
        revision_response(request_id, &revision, None, &self.stock_revision_id, false)
    }

    fn activate_response(
        &mut self,
        request_id: u64,
        revision_id: &str,
        state_stage_id: u64,
    ) -> Result<RevisionResponse, String> {
        let revision = self
            .revisions
            .verify(revision_id)
            .map_err(|error| error.to_string())?;
        // Logical conflicts must be rejected before the durable intent is
        // written. Once the journal exists, every subsequent error is an
        // integrity boundary that recovery must finish after process restart.
        let staged = self.state.validate_promotion(state_stage_id)?;
        if staged.source_sha256 != revision.manifest.source.sha256
            || staged.schema_version != revision.manifest.schema_version
        {
            return Err("staged state does not match the immutable revision".into());
        }
        self.write_journal(&ActivationJournal {
            revision_id: revision_id.into(),
            state_stage_id,
        })?;
        let state = self
            .promote_state(state_stage_id)
            .unwrap_or_else(|error| fatal_activation(error));
        self.revisions
            .set_current(revision_id)
            .map_err(|error| error.to_string())
            .unwrap_or_else(|error| fatal_activation(error));
        self.remove_journal()
            .unwrap_or_else(|error| fatal_activation(error));
        revision_response(
            request_id,
            &revision,
            Some(state),
            &self.stock_revision_id,
            false,
        )
    }

    fn fallback_to_stock_response(
        &mut self,
        request_id: u64,
        failed_revision_id: &str,
    ) -> Result<RevisionResponse, String> {
        let current = self
            .revisions
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "revision authority has no current revision".to_owned())?;
        if current.manifest.revision_id != failed_revision_id {
            return Err("fallback request does not name the active revision".into());
        }
        if failed_revision_id == self.stock_revision_id {
            return Err("stock experience failed; fixed Recovery is required".into());
        }
        let stock = self
            .revisions
            .verify(&self.stock_revision_id)
            .map_err(|error| error.to_string())?;
        let stage_id = self.state.stage(
            self.state.load().revision,
            stock.manifest.schema_version,
            json!({}),
            stock.manifest.source.sha256.clone(),
        )?;
        self.write_journal(&ActivationJournal {
            revision_id: self.stock_revision_id.clone(),
            state_stage_id: stage_id,
        })?;
        let state = self
            .promote_state(stage_id)
            .unwrap_or_else(|error| fatal_activation(error));
        self.revisions
            .set_current(&self.stock_revision_id)
            .map_err(|error| error.to_string())
            .unwrap_or_else(|error| fatal_activation(error));
        self.remove_journal()
            .unwrap_or_else(|error| fatal_activation(error));
        println!(
            "android_authority_stock_fallback failed_revision={} stock_revision={}",
            failed_revision_id, self.stock_revision_id
        );
        revision_response(
            request_id,
            &stock,
            Some(state),
            &self.stock_revision_id,
            true,
        )
    }

    fn promote_state(&mut self, stage_id: u64) -> Result<StateEnvelope, String> {
        let before_revision = self.state.load().revision;
        let promoted = self.state.promote(stage_id);
        let current = self.state.load();
        if current.revision > before_revision {
            if let Some(actions) = self.staged_effects.remove(&stage_id) {
                for action in actions {
                    match self.providers.execute(&action) {
                        Ok(result) => println!(
                            "provider_action_promoted revision={} action={action:?} result={result}",
                            current.revision
                        ),
                        Err(error) => eprintln!(
                            "provider_action_failed_after_promotion revision={} action={action:?} error={error}",
                            current.revision
                        ),
                    }
                }
            }
            self.persist_state()?;
        }
        promoted
    }

    fn authorize_effects(&self, effects: &[ProviderEffect]) -> Result<Vec<SystemAction>, String> {
        if effects.len() > experience_ir::MAX_EFFECTS {
            return Err("too many staged provider effects".into());
        }
        effects
            .iter()
            .map(|effect| self.providers.parse_and_authorize(effect))
            .collect()
    }

    fn recover_activation(&mut self) -> Result<(), String> {
        let Ok(bytes) = fs::read(&self.journal_file) else {
            return Ok(());
        };
        let journal: ActivationJournal =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let revision = self
            .revisions
            .verify(&journal.revision_id)
            .map_err(|error| error.to_string())?;
        if self.state.load().source_sha256 == revision.manifest.source.sha256 {
            self.revisions
                .set_current(&journal.revision_id)
                .map_err(|error| error.to_string())?;
            println!(
                "android_authority_recovered revision_id={} stage_id={}",
                journal.revision_id, journal.state_stage_id
            );
        }
        self.remove_journal()
    }

    fn ensure_consistent(&self) -> Result<(), String> {
        let current = self
            .revisions
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "revision authority has no current revision".to_owned())?;
        if current.manifest.source.sha256 != self.state.load().source_sha256 {
            return Err("revision pointer and provider/state authority disagree".into());
        }
        Ok(())
    }

    fn persist_state(&self) -> Result<(), String> {
        let parent = self
            .state_file
            .parent()
            .ok_or_else(|| "state file must have a parent".to_owned())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = self.state_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.state_file,
            &serde_json::to_vec_pretty(&self.state.load()).map_err(|error| error.to_string())?,
        )
    }

    fn write_journal(&self, journal: &ActivationJournal) -> Result<(), String> {
        let temporary = self.journal_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.journal_file,
            &serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?,
        )
    }

    fn remove_journal(&self) -> Result<(), String> {
        match fs::remove_file(&self.journal_file) {
            Ok(()) => sync_parent(&self.journal_file),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn provider_grant_for_effect(effect: &ProviderEffect) -> Result<&'static str, String> {
    match effect.provider.as_str() {
        "audio" => Ok("audio_control"),
        "media" => Ok("music_control"),
        "network" => Ok("network_control"),
        "apps" => Ok("application_launch"),
        "attention" | "power" => Ok("system_control"),
        provider => Err(format!(
            "provider `{provider}` is not available through the Android graph authority"
        )),
    }
}

fn fatal_activation(error: String) -> ! {
    eprintln!("android_authority_fatal_activation error={error}");
    std::process::abort()
}

fn write_synced_atomic(temporary: &Path, destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::rename(temporary, destination).map_err(|error| error.to_string())?;
    sync_parent(destination)
}

fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "durable file must have a parent".to_owned())?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

fn remove_synced(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn revision_assets(revision: &VerifiedRevision) -> Result<Vec<RevisionAssetWire>, String> {
    revision
        .manifest
        .assets
        .iter()
        .map(|asset| {
            Ok(RevisionAssetWire {
                id: asset.id.clone(),
                kind: asset.kind.clone(),
                bytes: fs::read(revision.directory.join(&asset.file.path))
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

fn revision_response(
    request_id: u64,
    revision: &VerifiedRevision,
    state: Option<StateEnvelope>,
    stock_revision_id: &str,
    fallback_performed: bool,
) -> Result<RevisionResponse, String> {
    let source = fs::read_to_string(revision.directory.join(&revision.manifest.source.path))
        .map_err(|error| error.to_string())?;
    let assets = revision_assets(revision)?;
    Ok(RevisionResponse {
        request_id,
        ok: true,
        revision_id: Some(revision.manifest.revision_id.clone()),
        source: Some(source),
        state,
        assets,
        stock_revision_id: Some(stock_revision_id.into()),
        stock_trusted: true,
        fallback_performed,
        graph: None,
        appearance: None,
        states: Vec::new(),
        error: None,
    })
}

fn provider_response(request_id: u64, ok: bool) -> ProviderResponse {
    ProviderResponse {
        request_id,
        ok,
        model: None,
        result: None,
        state: None,
        stage_id: None,
        error: None,
    }
}

fn provider_failure(request_id: u64, error: &str) -> ProviderResponse {
    ProviderResponse {
        error: Some(error.into()),
        ..provider_response(request_id, false)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use android_authority_protocol::RevisionRequest;
    use experience_package::{
        DerivationKind, DerivationRecord, ExperienceContract, ExperienceExport, ExperienceRole,
        PackageMetadata, ValueSchema, ViewportContract, APPEARANCE_ABI_VERSION, CONTRACT_VERSION,
        PACKAGE_FORMAT_VERSION,
    };

    fn stock_v4(source: &str) -> RevisionPackageInput {
        RevisionPackageInput {
            revision: RevisionInput {
                source: source.as_bytes().to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: experience_ir::EXPERIENCE_API_VERSION_V4,
                assets: vec![],
            },
            package: PackageMetadata {
                format_version: PACKAGE_FORMAT_VERSION,
                experience_id: ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID).unwrap(),
                role: ExperienceRole::Shell,
                provider_capabilities: Default::default(),
                contract: ExperienceContract {
                    contract_version: CONTRACT_VERSION,
                    exports: BTreeMap::from([(
                        ExportId::parse("main").unwrap(),
                        ExperienceExport {
                            properties: ValueSchema::empty_record(),
                            events: BTreeMap::new(),
                            viewport: ViewportContract {
                                min_width: 160,
                                min_height: 96,
                                max_width: 1920,
                                max_height: 1080,
                            },
                            appearance_abi: APPEARANCE_ABI_VERSION,
                            accepts_container_appearance: false,
                        },
                    )]),
                },
                dependencies: BTreeMap::new(),
                derivation: DerivationRecord {
                    kind: DerivationKind::Original,
                    parents: vec![],
                    request_sha256: None,
                    rationale: None,
                },
                state_migration: None,
            },
        }
    }

    fn install_and_stage(authority: &mut AndroidSystemAuthority, source: &str) -> (String, u64) {
        let installed = authority.dispatch_revision(RevisionRequest::Install {
            request_id: 1,
            source: source.to_owned(),
            state: json!({ "candidate": true }),
            schema_version: 1,
            experience_api_version: 3,
            assets: Vec::new(),
        });
        assert!(installed.ok);
        let revision_id = installed.revision_id.unwrap();
        let source_sha256 = authority
            .revisions
            .verify(&revision_id)
            .unwrap()
            .manifest
            .source
            .sha256;
        let staged = authority.dispatch_provider(ProviderRequest::StageState {
            request_id: 2,
            expected_revision: 0,
            schema_version: 1,
            state: json!({ "candidate": true }),
            source_sha256,
            effects: Vec::new(),
        });
        assert!(staged.ok);
        (revision_id, staged.stage_id.unwrap())
    }

    #[test]
    fn v4_bootstrap_migration_confirms_then_rolls_back_to_untouched_v3() {
        let temporary = tempfile::tempdir().unwrap();
        let revision_root = temporary.path().join("revisions");
        let state_file = temporary.path().join("provider.json");
        let legacy_source = "return { api_version = 3, legacy = true }";
        AndroidSystemAuthority::open(&revision_root, &state_file, legacy_source.as_bytes())
            .unwrap();

        let v4_source = r#"
            return { api_version = 4, exports = { main = {
                render = function() return { id = "stock" } end,
                update = function(_, state) return { state = state } end,
            } } }
        "#;
        let mut authority =
            AndroidSystemAuthority::open_v4(&revision_root, &state_file, stock_v4(v4_source))
                .unwrap();
        let candidate =
            authority.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 40 });
        let candidate_graph = candidate.graph.unwrap();
        assert!(candidate_graph.migration_pending);
        assert_eq!(candidate_graph.revisions[0].source, v4_source);
        let graph_id = candidate_graph.graph_id;

        drop(authority);
        let mut restarted =
            AndroidSystemAuthority::open_v4(&revision_root, &state_file, stock_v4(v4_source))
                .unwrap();
        assert!(
            restarted
                .dispatch_revision(RevisionRequest::CurrentGraph { request_id: 41 })
                .graph
                .unwrap()
                .migration_pending
        );
        let confirmed = restarted.dispatch_revision(RevisionRequest::ConfirmGraph {
            request_id: 42,
            graph_id: graph_id.clone(),
        });
        assert!(confirmed.ok, "{:?}", confirmed.error);
        assert!(!confirmed.graph.unwrap().migration_pending);

        drop(restarted);
        let mut accepted =
            AndroidSystemAuthority::open_v4(&revision_root, &state_file, stock_v4(v4_source))
                .unwrap();
        let current = accepted.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 43 });
        assert_eq!(current.graph.as_ref().unwrap().graph_id, graph_id);
        let rolled_back = accepted.dispatch_revision(RevisionRequest::RollbackGraph {
            request_id: 44,
            failed_graph_id: graph_id,
        });
        assert!(rolled_back.ok, "{:?}", rolled_back.error);
        assert!(rolled_back.fallback_performed);
        assert!(rolled_back.graph.is_none());
        assert_eq!(rolled_back.source.as_deref(), Some(legacy_source));

        drop(accepted);
        let mut legacy =
            AndroidSystemAuthority::open_v4(&revision_root, &state_file, stock_v4(v4_source))
                .unwrap();
        let response = legacy.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 45 });
        assert!(response.graph.is_none());
        assert_eq!(response.source.as_deref(), Some(legacy_source));
    }

    #[test]
    fn v4_graph_state_and_appearance_are_authoritative_across_restart() {
        let temporary = tempfile::tempdir().unwrap();
        let revision_root = temporary.path().join("revisions");
        let state_file = temporary.path().join("provider.json");
        let source = r#"
            return { api_version = 4, exports = { main = {
                render = function() return { id = "stock" } end,
                update = function(_, state) return { state = state } end,
            } } }
        "#;
        let mut authority =
            AndroidSystemAuthority::open_v4(&revision_root, &state_file, stock_v4(source)).unwrap();
        let current = authority.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 50 });
        let bundle = current.graph.unwrap();
        let root = bundle.graph.nodes.get(&bundle.graph.root).unwrap();
        let revision = bundle
            .revisions
            .iter()
            .find(|revision| revision.revision_id == root.revision_id.as_str())
            .unwrap();
        let instance_id = experience_package::InstanceId::parse("android-test-instance").unwrap();
        let committed = authority.dispatch_revision(RevisionRequest::CommitGraphAction {
            request_id: 51,
            graph_id: bundle.graph_id.clone(),
            updates: vec![GraphStateUpdateWire {
                node_id: bundle.graph.root.clone(),
                instance_id,
                experience_id: root.experience_id.clone(),
                revision_id: root.revision_id.clone(),
                expected_revision: revision.state.resource.revision,
                state: json!({"counter": 1}),
            }],
            effects: vec![],
        });
        assert!(committed.ok, "{:?}", committed.error);
        assert_eq!(committed.states[0].resource.revision, 1);

        authority
            .configure_appearance_writer("android-appearance-test")
            .unwrap();
        let mut profile = bundle.appearance.profile;
        profile.generation = 1;
        profile.reduce_motion = true;
        let appearance = authority.dispatch_revision(RevisionRequest::UpdateAppearance {
            request_id: 52,
            expected_generation: 0,
            capability: "android-appearance-test".into(),
            profile,
        });
        assert!(appearance.ok, "{:?}", appearance.error);

        drop(authority);
        let mut restarted =
            AndroidSystemAuthority::open_v4(&revision_root, &state_file, stock_v4(source)).unwrap();
        let current = restarted.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 53 });
        let bundle = current.graph.unwrap();
        assert_eq!(
            bundle.revisions[0].state.resource.state,
            json!({"counter": 1})
        );
        assert_eq!(bundle.appearance.profile.generation, 1);
        assert!(bundle.appearance.profile.reduce_motion);
    }

    #[test]
    fn presentation_activation_commits_state_and_revision_together() {
        let temporary = tempfile::tempdir().unwrap();
        let mut authority = AndroidSystemAuthority::open(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            b"return { api_version = 3, revision = 1 }",
        )
        .unwrap();
        let source = "return { api_version = 3, revision = 2 }".to_owned();
        let (revision_id, stage_id) = install_and_stage(&mut authority, &source);
        let activated = authority.dispatch_revision(RevisionRequest::Activate {
            request_id: 3,
            revision_id,
            state_stage_id: stage_id,
        });
        assert!(activated.ok, "{:?}", activated.error);
        assert_eq!(activated.state.unwrap().revision, 1);
        assert_eq!(authority.state.load().state, json!({ "candidate": true }));
    }

    #[test]
    fn provider_snapshot_is_live_system_abi_not_seeded_home_content() {
        let temporary = tempfile::tempdir().unwrap();
        let mut authority = AndroidSystemAuthority::open(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            b"return { api_version = 3 }",
        )
        .unwrap();
        let response = authority.dispatch_provider(ProviderRequest::Snapshot { request_id: 9 });
        let model = response.model.unwrap();
        assert_eq!(
            model.providers.abi_version,
            experience_ir::SYSTEM_PROVIDER_ABI_VERSION
        );
        assert_eq!(model.greeting, "SOS");
        assert!(model.calendar.is_empty());
        assert!(model.notes.is_empty());
        assert_ne!(model.date, "Saturday, 8 August");
        assert_ne!(model.music.artist, "Tycho");
        assert!(!model.system.timezone.is_empty());
    }

    #[test]
    fn restart_recovers_state_first_activation_gap() {
        let temporary = tempfile::tempdir().unwrap();
        let revision_root = temporary.path().join("revisions");
        let state_file = temporary.path().join("provider.json");
        let bootstrap = b"return { api_version = 3, revision = 1 }";
        let revision_id = {
            let mut authority =
                AndroidSystemAuthority::open(&revision_root, &state_file, bootstrap).unwrap();
            let (revision_id, stage_id) =
                install_and_stage(&mut authority, "return { api_version = 3, revision = 2 }");
            authority
                .write_journal(&ActivationJournal {
                    revision_id: revision_id.clone(),
                    state_stage_id: stage_id,
                })
                .unwrap();
            authority.promote_state(stage_id).unwrap();
            revision_id
        };

        let recovered =
            AndroidSystemAuthority::open(&revision_root, &state_file, bootstrap).unwrap();
        assert_eq!(
            recovered
                .revisions
                .current()
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            revision_id
        );
        assert_eq!(recovered.state.load().state, json!({ "candidate": true }));
        assert!(!revision_root.join("activation-journal.json").exists());
    }

    #[test]
    fn activation_rejects_state_for_another_source() {
        let temporary = tempfile::tempdir().unwrap();
        let mut authority = AndroidSystemAuthority::open(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            b"return { api_version = 3, revision = 1 }",
        )
        .unwrap();
        let installed = authority.dispatch_revision(RevisionRequest::Install {
            request_id: 1,
            source: "return { api_version = 3, revision = 2 }".into(),
            state: json!({}),
            schema_version: 1,
            experience_api_version: 3,
            assets: Vec::new(),
        });
        let staged = authority.dispatch_provider(ProviderRequest::StageState {
            request_id: 2,
            expected_revision: 0,
            schema_version: 1,
            state: json!({}),
            source_sha256: "0".repeat(64),
            effects: Vec::new(),
        });
        let activated = authority.dispatch_revision(RevisionRequest::Activate {
            request_id: 3,
            revision_id: installed.revision_id.unwrap(),
            state_stage_id: staged.stage_id.unwrap(),
        });
        assert!(!activated.ok);
        assert_eq!(authority.state.load().revision, 0);
    }

    #[test]
    fn stale_stage_is_rejected_before_journal_and_next_activation_succeeds() {
        let temporary = tempfile::tempdir().unwrap();
        let revision_root = temporary.path().join("revisions");
        let mut authority = AndroidSystemAuthority::open(
            &revision_root,
            temporary.path().join("provider.json"),
            b"return { api_version = 3, revision = 1 }",
        )
        .unwrap();
        let stale_source = "return { api_version = 3, revision = 2 }";
        let (stale_revision_id, stale_stage_id) = install_and_stage(&mut authority, stale_source);

        let stock_sha256 = authority
            .revisions
            .current()
            .unwrap()
            .unwrap()
            .manifest
            .source
            .sha256;
        let competing_stage = authority
            .state
            .stage(0, 1, json!({ "focus": false }), stock_sha256)
            .unwrap();
        authority.promote_state(competing_stage).unwrap();

        let rejected = authority.dispatch_revision(RevisionRequest::Activate {
            request_id: 3,
            revision_id: stale_revision_id,
            state_stage_id: stale_stage_id,
        });
        assert!(!rejected.ok);
        assert_eq!(rejected.error.as_deref(), Some("staged state is stale"));
        assert!(!revision_root.join("activation-journal.json").exists());

        let source = "return { api_version = 3, revision = 3 }";
        let installed = authority.dispatch_revision(RevisionRequest::Install {
            request_id: 4,
            source: source.into(),
            state: json!({ "candidate": "retry" }),
            schema_version: 1,
            experience_api_version: 3,
            assets: Vec::new(),
        });
        assert!(installed.ok);
        let revision_id = installed.revision_id.unwrap();
        let source_sha256 = authority
            .revisions
            .verify(&revision_id)
            .unwrap()
            .manifest
            .source
            .sha256;
        let staged = authority.dispatch_provider(ProviderRequest::StageState {
            request_id: 5,
            expected_revision: 1,
            schema_version: 1,
            state: json!({ "candidate": "retry" }),
            source_sha256,
            effects: Vec::new(),
        });
        assert!(staged.ok);
        let activated = authority.dispatch_revision(RevisionRequest::Activate {
            request_id: 6,
            revision_id,
            state_stage_id: staged.stage_id.unwrap(),
        });
        assert!(activated.ok, "{:?}", activated.error);
        assert_eq!(activated.state.unwrap().revision, 2);
    }

    #[test]
    fn rejected_generated_revision_falls_back_to_pinned_stock() {
        let temporary = tempfile::tempdir().unwrap();
        let bootstrap = b"return { api_version = 3, stock = true }";
        let mut authority = AndroidSystemAuthority::open(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            bootstrap,
        )
        .unwrap();
        let stock_revision_id = authority.stock_revision_id.clone();
        let (generated_revision_id, stage_id) = install_and_stage(
            &mut authority,
            "return { api_version = 3, generated = true }",
        );
        assert!(
            authority
                .dispatch_revision(RevisionRequest::Activate {
                    request_id: 3,
                    revision_id: generated_revision_id.clone(),
                    state_stage_id: stage_id,
                })
                .ok
        );

        let fallback = authority.dispatch_revision(RevisionRequest::FallbackToStock {
            request_id: 4,
            failed_revision_id: generated_revision_id,
        });
        assert!(fallback.ok, "{:?}", fallback.error);
        assert!(fallback.fallback_performed);
        assert!(fallback.stock_trusted);
        assert_eq!(
            fallback.revision_id.as_deref(),
            Some(stock_revision_id.as_str())
        );
        assert_eq!(
            fallback.source.as_deref(),
            Some(std::str::from_utf8(bootstrap).unwrap())
        );
        assert_eq!(fallback.state.unwrap().state, json!({}));

        let second = authority.dispatch_revision(RevisionRequest::FallbackToStock {
            request_id: 5,
            failed_revision_id: stock_revision_id,
        });
        assert!(!second.ok);
        assert!(second.error.unwrap().contains("fixed Recovery"));
    }
}
