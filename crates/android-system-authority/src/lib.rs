use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub use android_authority_protocol::MAX_PROVIDER_REQUEST_BYTES;
use android_authority_protocol::{
    AuthorityAuditSnapshot, GraphBundle, GraphEffectWire, GraphRevisionWire, GraphStateUpdateWire,
    RevisionAssetWire, RevisionRequest, RevisionResponse,
};
use experience_ir::{
    ProviderEffect, ProviderRequest, ProviderResponse, ShellExperience, StateEnvelope, MAX_EFFECTS,
    MAX_STATE_BYTES, SHELL_MODEL_ABI_VERSION,
};
use experience_package::{AppearanceProfile, ExperienceId, ExportId, ResolvedGraph};
use revision_supervisor::{
    DurableState, ExperienceRegistry, GraphResolver, GraphStore, RevisionAssetInput, RevisionInput,
    RevisionPackageInput, RevisionStore, VerifiedRevision, STOCK_MOBILE_EXPERIENCE_ID,
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

const COMPOSITION_AUTHORITY_FORMAT_VERSION: u32 = 1;
const MAX_ANDROID_LAUNCHABLE_EXPERIENCES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CompositionAuthorityState {
    format_version: u32,
    #[serde(default)]
    presented_experience: Option<ExperienceId>,
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
            presented_experience: None,
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
    #[serde(default)]
    candidate: bool,
    #[serde(default)]
    presentation_only: bool,
    #[serde(default)]
    staged_states: BTreeMap<ExperienceId, StateResource>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GraphActivationJournal {
    experience_id: ExperienceId,
    revision_id: String,
    graph_id: String,
    #[serde(default = "default_true")]
    update_pointers: bool,
    target_composition: CompositionAuthorityState,
    previous_composition: CompositionAuthorityState,
}

fn default_true() -> bool {
    true
}

pub struct AndroidSystemAuthority {
    revisions: RevisionStore,
    registry: ExperienceRegistry,
    resolver: GraphResolver,
    graphs: GraphStore,
    state: StateService,
    staged_effects: HashMap<u64, Vec<SystemAction>>,
    providers: SystemProviderRegistry,
    stock_experience_id: ExperienceId,
    state_file: PathBuf,
    composition_file: PathBuf,
    previous_composition_file: PathBuf,
    graph_activation_journal_file: PathBuf,
    composition: CompositionAuthorityState,
    pending_graph_file: PathBuf,
    appearance_writer: Option<String>,
}

impl AndroidSystemAuthority {
    pub fn open_v4(
        revision_root: impl Into<PathBuf>,
        state_file: impl Into<PathBuf>,
        bootstrap: RevisionPackageInput,
    ) -> Result<Self, String> {
        let revision_root = revision_root.into();
        let state_file = state_file.into();
        let revisions = RevisionStore::open(&revision_root).map_err(|error| error.to_string())?;
        let stock = revisions
            .install_package(bootstrap)
            .map_err(|error| error.to_string())?;
        let mut authority = Self::finish_open(revision_root, state_file, revisions, stock.clone())?;
        authority.initialize_v4_stock(&stock)?;
        Ok(authority)
    }

    pub fn install_reference_composition(&mut self) -> Result<(), String> {
        let ids = [
            "sos.example.agenda",
            "sos.example.media",
            "sos.example.dashboard",
            "sos.example.agenda-media-remix",
        ]
        .map(|id| ExperienceId::parse(id).map_err(|error| error.to_string()))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        let present = ids
            .iter()
            .map(|id| self.registry.get(id).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        if present.iter().all(Option::is_none) {
            revision_supervisor::install_reference_composition(&self.revisions)
                .map_err(|error| error.to_string())?;
        } else if present.iter().any(Option::is_none) {
            return Err("Android reference composition registry is incomplete".into());
        }

        let main = ExportId::parse("main").map_err(|error| error.to_string())?;
        for id in ids {
            let revision = self
                .registry
                .current(&id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("reference Experience `{id}` has no current revision"))?;
            if self
                .graphs
                .current(&id)
                .map_err(|error| error.to_string())?
                .is_none()
            {
                let graph = self
                    .resolver
                    .resolve(&revision.manifest.revision_id, &main)
                    .map_err(|error| error.to_string())?;
                let graph_id = self
                    .graphs
                    .install(&graph)
                    .map_err(|error| error.to_string())?;
                self.graphs
                    .set_current(&id, &graph_id)
                    .map_err(|error| error.to_string())?;
            }
            self.seed_product_revision(&revision)?;
        }
        self.persist_composition()
    }

    fn seed_product_revision(&mut self, revision: &VerifiedRevision) -> Result<(), String> {
        let package = &revision.package;
        if !self.composition.states.contains_key(&package.experience_id) {
            let durable: DurableState = serde_json::from_slice(
                &fs::read(revision.directory.join(&revision.manifest.state.path))
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            self.composition.states.insert(
                package.experience_id.clone(),
                StateResource {
                    revision: 0,
                    revision_id: revision.manifest.revision_id.clone(),
                    schema_version: durable.schema_version,
                    source_sha256: durable.source_sha256,
                    state: durable.state,
                },
            );
        }
        self.composition
            .grants
            .entry(package.experience_id.clone())
            .or_insert_with(|| GrantDecisionResource {
                generation: 1,
                reviewed: true,
                experience_id: package.experience_id.clone(),
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
            });
        Ok(())
    }

    fn finish_open(
        revision_root: PathBuf,
        state_file: PathBuf,
        revisions: RevisionStore,
        stock: VerifiedRevision,
    ) -> Result<Self, String> {
        let initial = if state_file.exists() {
            serde_json::from_slice::<StateEnvelope>(
                &fs::read(&state_file).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?
        } else {
            StateEnvelope {
                revision: 0,
                schema_version: stock.manifest.schema_version,
                source_sha256: stock.manifest.source.sha256.clone(),
                state: json!({}),
            }
        };
        let composition_file = state_file.with_extension("composition.json");
        let previous_composition_file = state_file.with_extension("composition.previous.json");
        let graph_activation_journal_file = revision_root.join("graph-activation-journal.json");
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
        let stock_experience_id = stock.package.experience_id.clone();
        let mut authority = Self {
            revisions,
            registry,
            resolver,
            graphs,
            state: StateService::new(initial),
            staged_effects: HashMap::new(),
            providers: SystemProviderRegistry::android(),
            stock_experience_id,
            state_file,
            composition_file,
            previous_composition_file,
            graph_activation_journal_file,
            composition,
            pending_graph_file: revision_root.join("pending-v4-graph.json"),
            appearance_writer: None,
        };
        authority.persist_state()?;
        authority.persist_composition()?;
        authority.recover_graph_activation()?;
        Ok(authority)
    }

    fn initialize_v4_stock(&mut self, stock: &VerifiedRevision) -> Result<(), String> {
        let package = &stock.package;
        let stock_id =
            ExperienceId::parse(STOCK_MOBILE_EXPERIENCE_ID).map_err(|error| error.to_string())?;
        if package.experience_id != stock_id
            || self.stock_experience_id != stock_id
            || package.role != experience_package::ExperienceRole::Shell
        {
            return Err("Android v4 bootstrap is not the reserved Stock Mobile experience".into());
        }
        if self
            .registry
            .get(&stock_id)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            self.registry
                .create(
                    &stock_id,
                    experience_package::ExperienceRole::Shell,
                    &stock.manifest.revision_id,
                )
                .map_err(|error| error.to_string())?;
        }

        if !self.composition.states.contains_key(&stock_id) {
            let initial = self.state.load();
            self.composition.states.insert(
                stock_id.clone(),
                StateResource {
                    revision: initial.revision,
                    revision_id: stock.manifest.revision_id.clone(),
                    schema_version: stock.manifest.schema_version,
                    source_sha256: stock.manifest.source.sha256.clone(),
                    state: initial.state,
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

    fn activate_graph_durably(&mut self, journal: GraphActivationJournal) -> Result<(), String> {
        self.revisions
            .verify(&journal.revision_id)
            .map_err(|error| error.to_string())?;
        self.graphs
            .verify(&journal.graph_id)
            .map_err(|error| error.to_string())?;
        let temporary = self.graph_activation_journal_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.graph_activation_journal_file,
            &serde_json::to_vec_pretty(&journal).map_err(|error| error.to_string())?,
        )?;
        self.complete_graph_activation(journal)
    }

    fn complete_graph_activation(&mut self, journal: GraphActivationJournal) -> Result<(), String> {
        let temporary = self.previous_composition_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.previous_composition_file,
            &serde_json::to_vec_pretty(&journal.previous_composition)
                .map_err(|error| error.to_string())?,
        )?;
        self.replace_composition(journal.target_composition)?;
        if journal.update_pointers {
            self.registry
                .set_current(&journal.experience_id, &journal.revision_id)
                .map_err(|error| error.to_string())?;
            self.graphs
                .set_current(&journal.experience_id, &journal.graph_id)
                .map_err(|error| error.to_string())?;
        }
        remove_synced(&self.pending_graph_file)?;
        remove_synced(&self.graph_activation_journal_file)
    }

    fn recover_graph_activation(&mut self) -> Result<(), String> {
        let bytes = match fs::read(&self.graph_activation_journal_file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        let journal: GraphActivationJournal =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        println!(
            "android_graph_activation_recovering revision_id={} graph_id={}",
            journal.revision_id, journal.graph_id
        );
        self.complete_graph_activation(journal)
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
            ProviderRequest::Snapshot { .. } => match self.snapshot_model() {
                Ok(model) => ProviderResponse {
                    model: Some(model),
                    ..provider_response(request_id, true)
                },
                Err(error) => provider_failure(request_id, &error),
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

    fn snapshot_model(&self) -> Result<experience_ir::ExperienceModel, String> {
        let mut model = self.providers.snapshot_model();
        let mut experiences = Vec::new();
        for record in self.registry.list().map_err(|error| error.to_string())? {
            if record.role == experience_package::ExperienceRole::Shell
                || self
                    .graphs
                    .current(&record.experience_id)
                    .map_err(|error| error.to_string())?
                    .is_none()
            {
                continue;
            }
            let title = record
                .experience_id
                .as_str()
                .rsplit('.')
                .next()
                .unwrap_or(record.experience_id.as_str())
                .replace(['-', '_'], " ");
            experiences.push(ShellExperience {
                experience_id: record.experience_id.to_string(),
                title,
            });
        }
        if experiences.len() > MAX_ANDROID_LAUNCHABLE_EXPERIENCES {
            return Err(format!(
                "Android registry exposes {} launchable Experiences; limit is {MAX_ANDROID_LAUNCHABLE_EXPERIENCES}",
                experiences.len()
            ));
        }
        model.shell.abi_version = SHELL_MODEL_ABI_VERSION;
        model.shell.experiences = experiences;
        Ok(model)
    }

    pub fn dispatch_revision(&mut self, request: RevisionRequest) -> RevisionResponse {
        let request_id = request.request_id();
        let result = match request {
            RevisionRequest::CurrentGraph { .. } => self.current_graph_response(request_id),
            RevisionRequest::AuditSnapshot { .. } => Ok(RevisionResponse {
                request_id,
                ok: true,
                audit_snapshot: Some(AuthorityAuditSnapshot {
                    format_version: self.composition.format_version,
                    presented_experience: self.composition.presented_experience.clone(),
                    states: self.composition.states.clone(),
                    appearance: self.composition.appearance.clone(),
                    grants: self.composition.grants.clone(),
                }),
                ..RevisionResponse::default()
            }),
            RevisionRequest::PresentExperience {
                expected_graph_id,
                experience_id,
                ..
            } => self.present_experience_response(
                request_id,
                &expected_graph_id,
                &experience_id,
                false,
            ),
            RevisionRequest::DismissExperience {
                expected_graph_id,
                experience_id,
                ..
            } => self.present_experience_response(
                request_id,
                &expected_graph_id,
                &experience_id,
                true,
            ),
            RevisionRequest::ConfirmGraph { graph_id, .. } => {
                self.confirm_graph_response(request_id, &graph_id)
            }
            RevisionRequest::RollbackGraph {
                failed_graph_id, ..
            } => self.rollback_graph_response(request_id, &failed_graph_id),
            RevisionRequest::StageGraphRevision {
                expected_graph_id,
                package,
                source,
                state,
                schema_version,
                assets,
                ..
            } => self.stage_graph_revision_response(
                request_id,
                &expected_graph_id,
                package,
                source,
                state,
                schema_version,
                assets,
            ),
            RevisionRequest::DiscardGraph { graph_id, .. } => {
                self.discard_graph_response(request_id, &graph_id)
            }
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
            RevisionRequest::SetExperienceAppearance {
                expected_graph_id,
                writer_experience_id,
                expected_generation,
                profile,
                ..
            } => self.set_experience_appearance_response(
                request_id,
                &expected_graph_id,
                &writer_experience_id,
                expected_generation,
                profile,
            ),
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
        };
        result.unwrap_or_else(|error| RevisionResponse {
            request_id,
            ok: false,
            error: Some(error),
            ..RevisionResponse::default()
        })
    }

    fn active_experience_id(&self) -> Result<ExperienceId, String> {
        Ok(self
            .composition
            .presented_experience
            .clone()
            .unwrap_or_else(|| self.stock_experience_id.clone()))
    }

    fn current_graph_response(&mut self, request_id: u64) -> Result<RevisionResponse, String> {
        if let Some(pending) = self.pending_graph()? {
            let graph = self
                .graphs
                .verify(&pending.graph_id)
                .map_err(|error| error.to_string())?;
            return self.graph_response(request_id, pending.graph_id, graph, true);
        }
        let active_id = self.active_experience_id()?;
        let Some((graph_id, graph)) = self
            .graphs
            .current(&active_id)
            .map_err(|error| error.to_string())?
        else {
            return Err("Android authority has no active v4 graph".into());
        };
        self.graph_response(request_id, graph_id, graph, false)
    }

    fn present_experience_response(
        &mut self,
        request_id: u64,
        expected_graph_id: &str,
        experience_id: &ExperienceId,
        dismiss: bool,
    ) -> Result<RevisionResponse, String> {
        if self.pending_graph()?.is_some() {
            return Err("another Android graph already awaits presentation".into());
        }
        let active_id = self.active_experience_id()?;
        let (active_graph_id, active_graph) = self
            .graphs
            .current(&active_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android authority has no active v4 graph".to_owned())?;
        if active_graph_id != expected_graph_id {
            return Err("presented Android graph changed before the lifecycle request".into());
        }
        let active_root = active_graph
            .nodes
            .get(&active_graph.root)
            .ok_or_else(|| "active Android graph has no root".to_owned())?;
        if active_root.experience_id != active_id {
            return Err("active Android graph root does not match its presented Experience".into());
        }

        let target_id = if dismiss {
            if experience_id != &active_id {
                return Err("dismiss request does not name the presented Experience".into());
            }
            self.stock_experience_id.clone()
        } else {
            let active_record = self
                .registry
                .get(&active_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "presented Experience is not registered".to_owned())?;
            if active_record.role != experience_package::ExperienceRole::Shell {
                return Err("only the registry-authorized shell may present an Experience".into());
            }
            experience_id.clone()
        };
        if target_id == active_id {
            return Err("requested Experience is already presented".into());
        }
        let target_record = self
            .registry
            .get(&target_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("unknown Experience `{target_id}`"))?;
        if !dismiss && target_record.role == experience_package::ExperienceRole::Shell {
            return Err("the shell cannot present another shell Experience".into());
        }
        let target_revision = self
            .registry
            .current(&target_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Experience `{target_id}` has no current revision"))?;
        let (graph_id, graph) = self
            .graphs
            .current(&target_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Experience `{target_id}` has no resolved graph"))?;
        let root = graph
            .nodes
            .get(&graph.root)
            .ok_or_else(|| "target Android graph has no root".to_owned())?;
        if root.experience_id != target_id
            || root.revision_id.as_str() != target_revision.manifest.revision_id
        {
            return Err("target Android graph does not match its registry pointer".into());
        }
        self.validate_graph_grants(&graph)?;
        self.write_pending_graph(&PendingGraphMigration {
            experience_id: target_id,
            revision_id: target_revision.manifest.revision_id,
            graph_id: graph_id.clone(),
            candidate: false,
            presentation_only: true,
            staged_states: BTreeMap::new(),
        })?;
        self.graph_response(request_id, graph_id, graph, true)
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
        let mut target_composition = self.composition.clone();
        for (experience_id, state) in &pending.staged_states {
            target_composition
                .states
                .insert(experience_id.clone(), state.clone());
        }
        if pending.presentation_only {
            target_composition.presented_experience = (pending.experience_id
                != self.stock_experience_id)
                .then(|| pending.experience_id.clone());
        }
        self.activate_graph_durably(GraphActivationJournal {
            experience_id: pending.experience_id,
            revision_id: pending.revision_id,
            graph_id: graph_id.to_owned(),
            update_pointers: !pending.presentation_only,
            target_composition,
            previous_composition: self.composition.clone(),
        })?;
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
            if pending.candidate || pending.presentation_only {
                return self.current_graph_response(request_id);
            }
            return Err("bootstrap graph rollback has no confirmed v4 predecessor".into());
        }

        let active_id = self.active_experience_id()?;
        let (current_graph_id, _) = self
            .graphs
            .current(&active_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android authority has no active v4 graph".to_owned())?;
        if current_graph_id != failed_graph_id {
            return Err("rollback does not name the active Android graph".into());
        }
        let previous_revision = self
            .registry
            .previous(&active_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "presented Android Experience has no rollback revision".to_owned())?;
        let (previous_graph_id, previous_graph) = self
            .graphs
            .previous(&active_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android graph store has no rollback graph".to_owned())?;
        let previous_composition: CompositionAuthorityState = serde_json::from_slice(
            &fs::read(&self.previous_composition_file)
                .map_err(|_| "Android graph rollback state is unavailable".to_owned())?,
        )
        .map_err(|error| error.to_string())?;
        self.activate_graph_durably(GraphActivationJournal {
            experience_id: active_id,
            revision_id: previous_revision.manifest.revision_id,
            graph_id: previous_graph_id.clone(),
            update_pointers: true,
            target_composition: previous_composition,
            previous_composition: self.composition.clone(),
        })?;
        self.graph_response(request_id, previous_graph_id, previous_graph, false)
    }

    fn stage_graph_revision_response(
        &mut self,
        request_id: u64,
        expected_graph_id: &str,
        package: experience_package::PackageMetadata,
        source: String,
        state: serde_json::Value,
        schema_version: u64,
        assets: Vec<RevisionAssetWire>,
    ) -> Result<RevisionResponse, String> {
        if self.pending_graph()?.is_some() {
            return Err("another Android graph already awaits presentation".into());
        }
        let active_id = self.active_experience_id()?;
        let (current_graph_id, current_graph) = self
            .graphs
            .current(&active_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android v4 authoring requires an active graph".to_owned())?;
        if current_graph_id != expected_graph_id {
            return Err("authoring target graph changed before staging".into());
        }
        let current_root = current_graph
            .nodes
            .get(&current_graph.root)
            .ok_or_else(|| "active Android graph has no root".to_owned())?;
        if package.experience_id != current_root.experience_id {
            return Err("candidate package does not target the active Experience".into());
        }
        let record = self
            .registry
            .get(&package.experience_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "candidate Experience is not registered".to_owned())?;
        if package.role != record.role {
            return Err("candidate role does not match the registry-owned role".into());
        }
        let current_state = self
            .composition
            .states
            .get(&package.experience_id)
            .ok_or_else(|| "active authoring target has no authority state".to_owned())?;
        let migration = package
            .state_migration
            .as_ref()
            .ok_or_else(|| "v4 replacement requires an explicit state migration".to_owned())?;
        match &migration.source {
            experience_package::StateMigrationSource::ExperienceRevision {
                experience_id,
                revision_id,
                schema_version: source_schema_version,
                state_sha256,
            } if experience_id == &package.experience_id
                && revision_id == &current_root.revision_id
                && *source_schema_version == current_state.schema_version
                && state_sha256
                    == &experience_package::canonical_sha256(&current_state.state)
                        .map_err(|error| error.to_string())? => {}
            _ => {
                return Err(
                    "candidate state migration does not bind the active authority state".into(),
                )
            }
        }
        let revision = self
            .revisions
            .install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: source.into_bytes(),
                    state: state.clone(),
                    schema_version,
                    experience_api_version: experience_ir::EXPERIENCE_API_VERSION,
                    assets: assets
                        .into_iter()
                        .map(|asset| RevisionAssetInput {
                            id: asset.id,
                            kind: asset.kind,
                            bytes: asset.bytes,
                        })
                        .collect(),
                },
                package,
            })
            .map_err(|error| error.to_string())?;
        let graph = self
            .resolver
            .resolve(
                &revision.manifest.revision_id,
                &ExportId::parse("main").map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        self.validate_graph_grants(&graph)?;
        let graph_id = self
            .graphs
            .install(&graph)
            .map_err(|error| error.to_string())?;
        let mut staged_states = BTreeMap::new();
        for node in graph.nodes.values() {
            if staged_states.contains_key(&node.experience_id) {
                continue;
            }
            let binding = self
                .revisions
                .verify(node.revision_id.as_str())
                .map_err(|error| error.to_string())?;
            let staged = if node.experience_id == revision.package.experience_id {
                let current = self
                    .composition
                    .states
                    .get(&node.experience_id)
                    .ok_or_else(|| "active authoring target has no authority state".to_owned())?;
                StateResource {
                    revision: current.revision.saturating_add(1),
                    revision_id: node.revision_id.to_string(),
                    schema_version,
                    source_sha256: binding.manifest.source.sha256,
                    state: state.clone(),
                }
            } else if let Some(current) = self
                .composition
                .states
                .get(&node.experience_id)
                .filter(|current| current.revision_id == node.revision_id.as_str())
            {
                current.clone()
            } else {
                let durable: DurableState = serde_json::from_slice(
                    &fs::read(binding.directory.join(&binding.manifest.state.path))
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                StateResource {
                    revision: 0,
                    revision_id: node.revision_id.to_string(),
                    schema_version: durable.schema_version,
                    source_sha256: durable.source_sha256,
                    state: durable.state,
                }
            };
            staged_states.insert(node.experience_id.clone(), staged);
        }
        self.write_pending_graph(&PendingGraphMigration {
            experience_id: current_root.experience_id.clone(),
            revision_id: revision.manifest.revision_id,
            graph_id: graph_id.clone(),
            candidate: true,
            presentation_only: false,
            staged_states,
        })?;
        self.graph_response(request_id, graph_id, graph, true)
    }

    fn discard_graph_response(
        &mut self,
        request_id: u64,
        graph_id: &str,
    ) -> Result<RevisionResponse, String> {
        let pending = self
            .pending_graph()?
            .ok_or_else(|| "no Android graph awaits discard".to_owned())?;
        if pending.graph_id != graph_id {
            return Err("discard does not name the pending Android graph".into());
        }
        remove_synced(&self.pending_graph_file)?;
        self.current_graph_response(request_id)
    }

    fn graph_for_action(&self, graph_id: &str) -> Result<ResolvedGraph, String> {
        if let Some(pending) = self.pending_graph()? {
            if pending.graph_id != graph_id {
                return Err("graph action does not name the pending Android graph".into());
            }
            return self
                .graphs
                .verify(graph_id)
                .map_err(|error| error.to_string());
        }
        let active_id = self.active_experience_id()?;
        let (current_id, graph) = self
            .graphs
            .current(&active_id)
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
                .package;
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
        self.commit_appearance_response(request_id, expected_generation, profile)
    }

    fn set_experience_appearance_response(
        &mut self,
        request_id: u64,
        expected_graph_id: &str,
        writer_experience_id: &ExperienceId,
        expected_generation: u64,
        profile: AppearanceProfile,
    ) -> Result<RevisionResponse, String> {
        if self.pending_graph()?.is_some() {
            return Err("appearance cannot change while a graph awaits presentation".into());
        }
        let active_id = self.active_experience_id()?;
        let (graph_id, _graph) = self
            .graphs
            .current(&active_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Android authority has no active v4 graph".to_owned())?;
        if graph_id != expected_graph_id {
            return Err("presented Android graph changed before appearance write".into());
        }
        let record = self
            .registry
            .get(writer_experience_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "appearance writer is not registered".to_owned())?;
        if writer_experience_id != &self.stock_experience_id
            || record.role != experience_package::ExperienceRole::Shell
        {
            return Err("only the platform's pinned Stock experience may write appearance".into());
        }
        let package = self
            .registry
            .current(writer_experience_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "appearance writer has no current revision".to_owned())?
            .package;
        let grant = self
            .composition
            .grants
            .get(writer_experience_id)
            .filter(|grant| grant.reviewed)
            .ok_or_else(|| "appearance writer has no reviewed grant".to_owned())?;
        if !package.provider_capabilities.contains("appearance_write")
            || !grant.provider_capabilities.contains("appearance_write")
        {
            return Err("appearance-write capability denied".into());
        }
        self.commit_appearance_response(request_id, expected_generation, profile)
    }

    fn commit_appearance_response(
        &mut self,
        request_id: u64,
        expected_generation: u64,
        profile: AppearanceProfile,
    ) -> Result<RevisionResponse, String> {
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
    ) -> Result<RevisionResponse, String> {
        self.validate_graph_grants(&graph)?;
        let staged_states = self
            .pending_graph()?
            .filter(|pending| pending.graph_id == graph_id && pending.candidate)
            .map(|pending| pending.staged_states)
            .unwrap_or_default();
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
            let mut package = revision.package.clone();
            let record = self
                .registry
                .get(&experience_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!("v4 graph experience `{experience_id}` is not registered")
                })?;
            package.role = record.role;
            let state = match staged_states.get(&experience_id) {
                Some(state) if state.revision_id == revision_id => state.clone(),
                _ => match self.composition.states.get(&experience_id) {
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
                },
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
            let package = &revision.package;
            let record = self
                .registry
                .get(&node.experience_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!(
                        "graph experience `{}` is not registered",
                        node.experience_id
                    )
                })?;
            if package.experience_id != node.experience_id || package.role != record.role {
                return Err(format!(
                    "graph revision `{}` conflicts with its registry identity",
                    node.revision_id
                ));
            }
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
    use super::*;
    use experience_package::{ExperienceRole, PackageMetadata, PACKAGE_FORMAT_VERSION};

    fn stock_mobile(source: &str) -> RevisionPackageInput {
        RevisionPackageInput {
            revision: RevisionInput {
                source: source.as_bytes().to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: experience_package::EXPERIENCE_API_VERSION,
                assets: Vec::new(),
            },
            package: serde_json::from_str(include_str!("../../../experiences/mobile.package.json"))
                .unwrap(),
        }
    }

    fn source() -> &'static str {
        "return { api_version = 4, exports = { main = { render = function() return { id = 'root' } end } } }"
    }

    #[test]
    fn checked_in_stock_mobile_package_is_the_reserved_v4_shell() {
        let package: PackageMetadata =
            serde_json::from_str(include_str!("../../../experiences/mobile.package.json")).unwrap();
        package.validate().unwrap();
        assert_eq!(package.format_version, PACKAGE_FORMAT_VERSION);
        assert_eq!(package.experience_id.as_str(), STOCK_MOBILE_EXPERIENCE_ID);
        assert_eq!(package.role, ExperienceRole::Shell);
    }

    #[test]
    fn startup_and_restart_expose_only_the_stock_v4_graph() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("revisions");
        let state = temporary.path().join("provider.json");
        let mut authority =
            AndroidSystemAuthority::open_v4(&root, &state, stock_mobile(source())).unwrap();
        let first = authority.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 1 });
        assert!(first.ok, "{:?}", first.error);
        let first = first.graph.unwrap();
        assert!(!first.migration_pending);
        assert_eq!(first.revisions.len(), 1);
        assert_eq!(
            first.revisions[0].package.experience_id.as_str(),
            STOCK_MOBILE_EXPERIENCE_ID
        );
        assert_eq!(
            first.revisions[0].package.format_version,
            PACKAGE_FORMAT_VERSION
        );
        drop(authority);

        let mut restarted =
            AndroidSystemAuthority::open_v4(&root, &state, stock_mobile(source())).unwrap();
        let second = restarted.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 2 });
        assert!(second.ok, "{:?}", second.error);
        assert_eq!(second.graph.unwrap().graph_id, first.graph_id);
    }

    #[test]
    fn reference_experiences_are_registry_launchables_without_replacing_stock() {
        let temporary = tempfile::tempdir().unwrap();
        let mut authority = AndroidSystemAuthority::open_v4(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            stock_mobile(source()),
        )
        .unwrap();
        let stock = authority
            .dispatch_revision(RevisionRequest::CurrentGraph { request_id: 1 })
            .graph
            .unwrap()
            .graph_id;
        authority.install_reference_composition().unwrap();
        let snapshot = authority.dispatch_provider(ProviderRequest::Snapshot { request_id: 2 });
        let launchables = snapshot.model.unwrap().shell.experiences;
        assert!(launchables
            .iter()
            .any(|experience| experience.experience_id == "sos.example.dashboard"));
        let current = authority.dispatch_revision(RevisionRequest::CurrentGraph { request_id: 3 });
        assert_eq!(current.graph.unwrap().graph_id, stock);
    }
}
