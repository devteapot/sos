use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use experience_package::{ExperienceId, ResolvedGraph, MAX_GRAPH_INSTANCES};
use provider_state_service::{state_sha256, ServiceClient};
use serde::{Deserialize, Serialize};
use service_protocol::{
    DataFlowGrant, GraphExperiencePromotion, GraphPromotionDraft, ResourceQuery, ResourceValue,
    ResponsePayload, ServiceRequest, TransactionStatus,
};

use crate::{
    DurableState, Error, ExperienceHost, ExperienceRegistry, GraphStore, HostCommand, Result,
    ReverseDependencyIndex, RevisionStore,
};

const GRAPH_ACTIVATION_JOURNAL_VERSION: u32 = 1;
static JOURNAL_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphActivationPhase {
    Intent,
    Presented,
    AuthorityCommitted,
    RegistryCommitted,
    GraphCommitted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GraphActivationJournal {
    pub format_version: u32,
    pub root_experience_id: ExperienceId,
    pub previous_graph_id: String,
    pub previous_root_revision: String,
    pub candidate_graph_id: String,
    pub candidate_root_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_transaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub registry_updates: Vec<RegistryPointerUpdate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph_updates: Vec<GraphPointerUpdate>,
    pub phase: GraphActivationPhase,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RegistryPointerUpdate {
    pub experience_id: ExperienceId,
    pub previous_revision: String,
    pub candidate_revision: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GraphPointerUpdate {
    pub root_experience_id: ExperienceId,
    pub previous_graph_id: String,
    pub candidate_graph_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphActivationFaultPoint {
    AfterIntent,
    AfterPresented,
    AfterAuthorityCommit,
    AfterRegistryCommit,
    AfterGraphCommit,
}

pub struct ExperienceGraphSupervisor {
    revisions: RevisionStore,
    registry: ExperienceRegistry,
    graphs: GraphStore,
    reverse_dependencies: ReverseDependencyIndex,
    host_command: HostCommand,
    host_timeout: Duration,
    hosts: BTreeMap<ExperienceId, ExperienceHost>,
    active_graphs: BTreeMap<ExperienceId, String>,
    primary_root: Option<ExperienceId>,
    authority: Option<ServiceClient>,
    journal_file: PathBuf,
    fault: Option<GraphActivationFaultPoint>,
}

pub struct PreparedGraphActivation {
    root_experience_id: ExperienceId,
    graph_id: String,
    previous_graph_id: String,
    previous_root_revision: String,
    graph: ResolvedGraph,
    authority_transaction_id: Option<String>,
    registry_updates: Vec<RegistryPointerUpdate>,
    host_prepared: bool,
    input_quiesced: bool,
}

pub struct PreparedGraphSetActivation {
    updates: Vec<PreparedGraphActivation>,
    authority_transaction_id: Option<String>,
    registry_updates: Vec<RegistryPointerUpdate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperienceAdvance {
    pub graph_updates: Vec<ExperienceGraphAdvance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperienceGraphAdvance {
    pub root_experience_id: ExperienceId,
    pub graph_id: String,
    pub host_pid: Option<u32>,
}

impl PreparedGraphActivation {
    pub fn graph_id(&self) -> &str {
        &self.graph_id
    }

    pub fn previous_graph_id(&self) -> &str {
        &self.previous_graph_id
    }
}

impl ExperienceGraphSupervisor {
    pub fn new(
        revisions: RevisionStore,
        registry: ExperienceRegistry,
        graphs: GraphStore,
        host_command: HostCommand,
        host_timeout: Duration,
    ) -> Self {
        let reverse_dependencies = ReverseDependencyIndex::open(revisions.root());
        Self {
            journal_file: revisions.root().join("graph-activation-journal.json"),
            revisions,
            registry,
            graphs,
            reverse_dependencies,
            host_command,
            host_timeout,
            hosts: BTreeMap::new(),
            active_graphs: BTreeMap::new(),
            primary_root: None,
            authority: None,
            fault: None,
        }
    }

    pub fn with_authority(mut self, authority: ServiceClient) -> Self {
        self.authority = Some(authority);
        self
    }

    pub fn boot(&mut self, root_experience_id: &ExperienceId) -> Result<Option<u32>> {
        if self.hosts.contains_key(root_experience_id) {
            return Err(Error::InvalidGraph(format!(
                "experience `{root_experience_id}` is already presented"
            )));
        }
        self.recover()?;
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        let Some((graph_id, graph)) = self.graphs.current(root_experience_id)? else {
            return Ok(None);
        };
        self.validate_root(root_experience_id, &graph)?;
        self.validate_graph_grants(&graph)?;
        self.validate_live_instance_budget(std::iter::once((root_experience_id, &graph)))?;
        let mut host = ExperienceHost::launch(self.host_command.clone(), self.host_timeout)?;
        host.boot_graph(
            &graph_id,
            self.graphs.snapshot_path(&graph_id)?,
            self.revisions.root().to_path_buf(),
        )?;
        let pid = host.id();
        self.hosts.insert(root_experience_id.clone(), host);
        self.active_graphs
            .insert(root_experience_id.clone(), graph_id);
        self.primary_root
            .get_or_insert_with(|| root_experience_id.clone());
        Ok(Some(pid))
    }

    pub fn prepare(
        &mut self,
        root_experience_id: &ExperienceId,
        graph_id: &str,
    ) -> Result<PreparedGraphActivation> {
        self.prepare_internal(root_experience_id, graph_id, true, false)
    }

    fn prepare_internal(
        &mut self,
        root_experience_id: &ExperienceId,
        graph_id: &str,
        stage_authority: bool,
        allow_unpresented: bool,
    ) -> Result<PreparedGraphActivation> {
        let graph = self.graphs.verify(graph_id)?;
        self.validate_root(root_experience_id, &graph)?;
        self.validate_graph_grants(&graph)?;
        if self.hosts.contains_key(root_experience_id) {
            self.validate_live_instance_budget(std::iter::once((root_experience_id, &graph)))?;
        }
        let candidate_root_revision = graph.nodes[&graph.root].revision_id.to_string();
        let previous_graph_id = self
            .active_graphs
            .get(root_experience_id)
            .cloned()
            .or_else(|| {
                self.graphs
                    .current(root_experience_id)
                    .ok()
                    .flatten()
                    .map(|(graph_id, _)| graph_id)
            })
            .ok_or(Error::NoCurrentRevision)?;
        let previous_graph = self.graphs.verify(&previous_graph_id)?;
        self.validate_root(root_experience_id, &previous_graph)?;
        let previous_root_revision = previous_graph.nodes[&previous_graph.root]
            .revision_id
            .to_string();
        let registry_revision = self
            .registry
            .current(root_experience_id)?
            .ok_or(Error::NoCurrentRevision)?
            .manifest
            .revision_id;
        if registry_revision != previous_root_revision {
            return Err(Error::InvalidGraph(
                "experience registry and active graph root disagree".into(),
            ));
        }
        let host_prepared = if let Some(host) = self.hosts.get_mut(root_experience_id) {
            host.prepare_graph(
                graph_id,
                self.graphs.snapshot_path(graph_id)?,
                self.revisions.root().to_path_buf(),
            )?;
            true
        } else if allow_unpresented {
            false
        } else {
            return Err(Error::NoActiveHost);
        };
        let authority_transaction_id = match stage_authority
            .then(|| self.stage_authority_activation(graph_id, &graph))
            .transpose()
            .map(|transaction| transaction.flatten())
        {
            Ok(transaction_id) => transaction_id,
            Err(error) => {
                let _ = self
                    .hosts
                    .get_mut(root_experience_id)
                    .and_then(|host| host.discard_graph(graph_id).ok());
                return Err(error);
            }
        };
        Ok(PreparedGraphActivation {
            root_experience_id: root_experience_id.clone(),
            graph_id: graph_id.into(),
            previous_graph_id,
            previous_root_revision: previous_root_revision.clone(),
            graph,
            authority_transaction_id,
            registry_updates: vec![RegistryPointerUpdate {
                experience_id: root_experience_id.clone(),
                previous_revision: previous_root_revision.clone(),
                candidate_revision: candidate_root_revision,
            }],
            host_prepared,
            input_quiesced: !host_prepared,
        })
    }

    pub fn prepare_tracked_update(
        &mut self,
        experience_id: &ExperienceId,
        revision_id: &str,
    ) -> Result<PreparedGraphActivation> {
        let candidate = self.revisions.verify(revision_id)?;
        let candidate_package = candidate.package.as_ref().ok_or_else(|| {
            Error::InvalidGraph("tracked updates require a v4 package revision".into())
        })?;
        let record = self.registry.get(experience_id)?.ok_or_else(|| {
            Error::InvalidGraph(format!("unknown tracked experience `{experience_id}`"))
        })?;
        if candidate_package.experience_id != *experience_id
            || candidate_package.role != record.role
        {
            return Err(Error::InvalidGraph(
                "tracked update revision has the wrong experience identity or role".into(),
            ));
        }
        let previous_revision = self
            .registry
            .current(experience_id)?
            .ok_or(Error::NoCurrentRevision)?
            .manifest
            .revision_id;
        if previous_revision == revision_id {
            return Err(Error::InvalidGraph(
                "tracked update revision is already current".into(),
            ));
        }
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        let mut affected = self
            .reverse_dependencies
            .affected_active_roots(experience_id, &self.graphs)?;
        if self.active_graphs.contains_key(experience_id) {
            affected.insert(experience_id.clone());
        }
        let Some(root) = affected
            .iter()
            .next()
            .cloned()
            .filter(|_| affected.len() == 1)
        else {
            return Err(Error::InvalidGraph(format!(
                "tracked update affects presented roots {:?}; use a graph-set activation",
                affected,
            )));
        };
        if !self.active_graphs.contains_key(&root) {
            return Err(Error::InvalidGraph(format!(
                "tracked update affects root `{root}`, but this supervisor does not present it"
            )));
        }
        let root_revision = if root == *experience_id {
            revision_id.to_owned()
        } else {
            self.registry
                .current(&root)?
                .ok_or(Error::NoCurrentRevision)?
                .manifest
                .revision_id
        };
        let override_revision = experience_package::RevisionId::parse(revision_id)
            .map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let graph = crate::GraphResolver::new(self.revisions.clone())
            .resolve_tracked_with_overrides(
                &root_revision,
                &experience_package::ExportId::parse("main")
                    .map_err(|error| Error::InvalidGraph(error.to_string()))?,
                &self.registry,
                &std::collections::BTreeMap::from([(experience_id.clone(), override_revision)]),
            )?;
        let graph_id = self.graphs.install(&graph)?;
        let mut prepared = self.prepare(&root, &graph_id)?;
        prepared.registry_updates = vec![RegistryPointerUpdate {
            experience_id: experience_id.clone(),
            previous_revision,
            candidate_revision: revision_id.into(),
        }];
        Ok(prepared)
    }

    pub fn prepare_tracked_update_set(
        &mut self,
        experience_id: &ExperienceId,
        revision_id: &str,
    ) -> Result<PreparedGraphSetActivation> {
        self.validate_registry_candidate(experience_id, revision_id)?;
        let previous_revision = self
            .registry
            .current(experience_id)?
            .ok_or(Error::NoCurrentRevision)?
            .manifest
            .revision_id;
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        let mut affected = self
            .reverse_dependencies
            .affected_active_roots(experience_id, &self.graphs)?;
        if self.active_graphs.contains_key(experience_id) {
            affected.insert(experience_id.clone());
        }
        if affected.is_empty() {
            return Err(Error::InvalidGraph(
                "graph-set activation requires at least one affected root".into(),
            ));
        }
        let override_revision = experience_package::RevisionId::parse(revision_id)
            .map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let overrides = BTreeMap::from([(experience_id.clone(), override_revision)]);
        let main = experience_package::ExportId::parse("main")
            .map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let mut candidates = Vec::new();
        for root in affected {
            let root_revision = if root == *experience_id {
                revision_id.to_owned()
            } else {
                self.registry
                    .current(&root)?
                    .ok_or(Error::NoCurrentRevision)?
                    .manifest
                    .revision_id
            };
            let graph = crate::GraphResolver::new(self.revisions.clone())
                .resolve_tracked_with_overrides(
                    &root_revision,
                    &main,
                    &self.registry,
                    &overrides,
                )?;
            let graph_id = self.graphs.install(&graph)?;
            candidates.push((root, graph_id, graph));
        }
        self.validate_live_instance_budget(
            candidates
                .iter()
                .filter(|(root, _, _)| self.hosts.contains_key(root))
                .map(|(root, _, graph)| (root, graph)),
        )?;
        let mut updates = Vec::new();
        for (root, graph_id, _) in candidates {
            match self.prepare_internal(&root, &graph_id, false, true) {
                Ok(prepared) => updates.push(prepared),
                Err(error) => {
                    for prepared in updates {
                        let _ = self
                            .hosts
                            .get_mut(&prepared.root_experience_id)
                            .and_then(|host| host.discard_graph(&prepared.graph_id).ok());
                    }
                    return Err(error);
                }
            }
        }
        let activation_identity = updates
            .iter()
            .map(|update| update.graph_id.as_str())
            .collect::<Vec<_>>()
            .join(":");
        let activation_id = experience_package::hex_sha256(activation_identity.as_bytes());
        let graphs = updates
            .iter()
            .map(|update| update.graph.clone())
            .collect::<Vec<_>>();
        let authority_transaction_id = match self.stage_authority_graphs(&activation_id, &graphs) {
            Ok(transaction_id) => transaction_id,
            Err(error) => {
                for prepared in updates {
                    let _ = self
                        .hosts
                        .get_mut(&prepared.root_experience_id)
                        .and_then(|host| host.discard_graph(&prepared.graph_id).ok());
                }
                return Err(error);
            }
        };
        Ok(PreparedGraphSetActivation {
            updates,
            authority_transaction_id,
            registry_updates: vec![RegistryPointerUpdate {
                experience_id: experience_id.clone(),
                previous_revision,
                candidate_revision: revision_id.into(),
            }],
        })
    }

    pub fn advance_experience(
        &mut self,
        experience_id: &ExperienceId,
        revision_id: &str,
    ) -> Result<ExperienceAdvance> {
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        let mut affected = self
            .reverse_dependencies
            .affected_active_roots(experience_id, &self.graphs)?;
        if self.active_graphs.contains_key(experience_id) {
            affected.insert(experience_id.clone());
        }
        if affected.is_empty() {
            self.validate_registry_candidate(experience_id, revision_id)?;
            self.registry.set_current(experience_id, revision_id)?;
            self.reverse_dependencies
                .rebuild(&self.revisions, &self.registry)?;
            return Ok(ExperienceAdvance {
                graph_updates: Vec::new(),
            });
        }
        if affected.len() > 1
            || affected
                .iter()
                .any(|root| !self.active_graphs.contains_key(root))
        {
            let prepared = self.prepare_tracked_update_set(experience_id, revision_id)?;
            return self.commit_set(prepared);
        }
        let prepared = self.prepare_tracked_update(experience_id, revision_id)?;
        let root = prepared.root_experience_id.clone();
        let graph_id = prepared.graph_id().to_owned();
        let pid = self.commit(prepared)?;
        Ok(ExperienceAdvance {
            graph_updates: vec![ExperienceGraphAdvance {
                root_experience_id: root,
                graph_id,
                host_pid: Some(pid),
            }],
        })
    }

    pub fn quiesce(&mut self, prepared: &mut PreparedGraphActivation) -> Result<()> {
        if prepared.input_quiesced {
            return Ok(());
        }
        self.hosts
            .get_mut(&prepared.root_experience_id)
            .ok_or(Error::NoActiveHost)?
            .quiesce_graph_input(&prepared.graph_id)?;
        prepared.input_quiesced = true;
        Ok(())
    }

    pub fn commit(&mut self, mut prepared: PreparedGraphActivation) -> Result<u32> {
        let candidate_root_revision = prepared.graph.nodes[&prepared.graph.root]
            .revision_id
            .to_string();
        let mut journal = GraphActivationJournal {
            format_version: GRAPH_ACTIVATION_JOURNAL_VERSION,
            root_experience_id: prepared.root_experience_id.clone(),
            previous_graph_id: prepared.previous_graph_id.clone(),
            previous_root_revision: prepared.previous_root_revision.clone(),
            candidate_graph_id: prepared.graph_id.clone(),
            candidate_root_revision: candidate_root_revision.clone(),
            authority_transaction_id: prepared.authority_transaction_id.clone(),
            registry_updates: prepared.registry_updates.clone(),
            graph_updates: vec![GraphPointerUpdate {
                root_experience_id: prepared.root_experience_id.clone(),
                previous_graph_id: prepared.previous_graph_id.clone(),
                candidate_graph_id: prepared.graph_id.clone(),
            }],
            phase: GraphActivationPhase::Intent,
        };
        if let Err(error) = self.quiesce(&mut prepared) {
            let _ = self
                .hosts
                .get_mut(&prepared.root_experience_id)
                .and_then(|host| host.discard_graph(&prepared.graph_id).ok());
            return Err(error);
        }
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterIntent)?;
        if let Err(error) = self
            .hosts
            .get_mut(&prepared.root_experience_id)
            .ok_or(Error::NoActiveHost)?
            .present_graph(&prepared.graph_id)
        {
            return self.fail_activation(&journal, error);
        }
        journal.phase = GraphActivationPhase::Presented;
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterPresented)?;
        if let Err(error) = self.promote_authority(journal.authority_transaction_id.as_deref()) {
            return self.fail_activation(&journal, error);
        }
        journal.phase = GraphActivationPhase::AuthorityCommitted;
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterAuthorityCommit)?;
        if let Err(error) = self.apply_registry_updates(&journal, true) {
            return self.fail_activation(&journal, error);
        }
        if let Err(error) = self
            .reverse_dependencies
            .rebuild(&self.revisions, &self.registry)
        {
            return self.fail_activation(&journal, error);
        }
        journal.phase = GraphActivationPhase::RegistryCommitted;
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterRegistryCommit)?;
        if let Err(error) = self
            .graphs
            .set_current(&prepared.root_experience_id, &prepared.graph_id)
        {
            return self.fail_activation(&journal, error);
        }
        journal.phase = GraphActivationPhase::GraphCommitted;
        self.write_journal(&journal)?;
        self.inject(GraphActivationFaultPoint::AfterGraphCommit)?;
        self.hosts
            .get_mut(&prepared.root_experience_id)
            .ok_or(Error::NoActiveHost)?
            .finalize_graph(&prepared.graph_id)?;
        let root_experience_id = prepared.root_experience_id;
        self.active_graphs
            .insert(root_experience_id.clone(), prepared.graph_id);
        self.clear_journal()?;
        Ok(self.hosts[&root_experience_id].id())
    }

    pub fn commit_set(
        &mut self,
        mut prepared: PreparedGraphSetActivation,
    ) -> Result<ExperienceAdvance> {
        let primary = prepared
            .updates
            .first()
            .ok_or_else(|| Error::InvalidGraph("graph activation set is empty".into()))?;
        let candidate_root_revision = primary.graph.nodes[&primary.graph.root]
            .revision_id
            .to_string();
        let mut journal = GraphActivationJournal {
            format_version: GRAPH_ACTIVATION_JOURNAL_VERSION,
            root_experience_id: primary.root_experience_id.clone(),
            previous_graph_id: primary.previous_graph_id.clone(),
            previous_root_revision: primary.previous_root_revision.clone(),
            candidate_graph_id: primary.graph_id.clone(),
            candidate_root_revision,
            authority_transaction_id: prepared.authority_transaction_id.clone(),
            registry_updates: prepared.registry_updates.clone(),
            graph_updates: prepared
                .updates
                .iter()
                .map(|update| GraphPointerUpdate {
                    root_experience_id: update.root_experience_id.clone(),
                    previous_graph_id: update.previous_graph_id.clone(),
                    candidate_graph_id: update.graph_id.clone(),
                })
                .collect(),
            phase: GraphActivationPhase::Intent,
        };
        for update in &mut prepared.updates {
            if let Err(error) = self.quiesce(update) {
                self.abort_authority(prepared.authority_transaction_id.as_deref())?;
                for candidate in prepared.updates {
                    let _ = self
                        .hosts
                        .get_mut(&candidate.root_experience_id)
                        .and_then(|host| host.discard_graph(&candidate.graph_id).ok());
                }
                return Err(error);
            }
        }
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterIntent)?;
        for update in &prepared.updates {
            if !update.host_prepared {
                continue;
            }
            if let Err(error) = self
                .hosts
                .get_mut(&update.root_experience_id)
                .ok_or(Error::NoActiveHost)?
                .present_graph(&update.graph_id)
            {
                return self.fail_activation(&journal, error);
            }
        }
        journal.phase = GraphActivationPhase::Presented;
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterPresented)?;
        if let Err(error) = self.promote_authority(journal.authority_transaction_id.as_deref()) {
            return self.fail_activation(&journal, error);
        }
        journal.phase = GraphActivationPhase::AuthorityCommitted;
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterAuthorityCommit)?;
        if let Err(error) = self.apply_registry_updates(&journal, true) {
            return self.fail_activation(&journal, error);
        }
        if let Err(error) = self
            .reverse_dependencies
            .rebuild(&self.revisions, &self.registry)
        {
            return self.fail_activation(&journal, error);
        }
        journal.phase = GraphActivationPhase::RegistryCommitted;
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterRegistryCommit)?;
        if let Err(error) = self.apply_graph_updates(&journal, true) {
            return self.fail_activation(&journal, error);
        }
        journal.phase = GraphActivationPhase::GraphCommitted;
        self.write_journal(&journal)?;
        self.inject(GraphActivationFaultPoint::AfterGraphCommit)?;
        let mut activated = Vec::new();
        for update in prepared.updates {
            let host_pid = if update.host_prepared {
                let host = self
                    .hosts
                    .get_mut(&update.root_experience_id)
                    .ok_or(Error::NoActiveHost)?;
                host.finalize_graph(&update.graph_id)?;
                self.active_graphs
                    .insert(update.root_experience_id.clone(), update.graph_id.clone());
                Some(host.id())
            } else {
                None
            };
            activated.push(ExperienceGraphAdvance {
                root_experience_id: update.root_experience_id,
                graph_id: update.graph_id,
                host_pid,
            });
        }
        self.clear_journal()?;
        Ok(ExperienceAdvance {
            graph_updates: activated,
        })
    }

    pub fn configure_fault(&mut self, point: Option<GraphActivationFaultPoint>) {
        self.fault = point;
    }

    pub fn journal(&self) -> Result<Option<GraphActivationJournal>> {
        self.load_journal()
    }

    pub fn recover(&mut self) -> Result<Option<String>> {
        let Some(journal) = self.load_journal()? else {
            return Ok(None);
        };
        self.validate_journal(&journal)?;
        let authority_committed = self.authority_transaction_committed(&journal)?;
        match (journal.phase, authority_committed) {
            (GraphActivationPhase::Intent | GraphActivationPhase::Presented, false) => {
                self.abort_authority(journal.authority_transaction_id.as_deref())?;
                self.apply_registry_updates(&journal, false)?;
                self.reverse_dependencies
                    .rebuild(&self.revisions, &self.registry)?;
                self.apply_graph_updates(&journal, false)?;
            }
            _ => {
                self.apply_registry_updates(&journal, true)?;
                self.reverse_dependencies
                    .rebuild(&self.revisions, &self.registry)?;
                self.apply_graph_updates(&journal, true)?;
            }
        }
        self.clear_journal()?;
        Ok(Some(match (journal.phase, authority_committed) {
            (GraphActivationPhase::Intent | GraphActivationPhase::Presented, false) => {
                journal.previous_graph_id
            }
            _ => journal.candidate_graph_id,
        }))
    }

    pub fn discard(&mut self, prepared: PreparedGraphActivation) -> Result<()> {
        self.abort_authority(prepared.authority_transaction_id.as_deref())?;
        self.hosts
            .get_mut(&prepared.root_experience_id)
            .ok_or(Error::NoActiveHost)?
            .discard_graph(&prepared.graph_id)
    }

    pub fn poll(&mut self) -> Result<Option<(String, u32, u32)>> {
        let roots = self.hosts.keys().cloned().collect::<Vec<_>>();
        for root in roots {
            let host = self.hosts.get_mut(&root).expect("presented host exists");
            let Some(status) = host.try_wait()? else {
                continue;
            };
            let failed_pid = host.id();
            return self.restart_after_exit(&root, status, failed_pid).map(Some);
        }
        Ok(None)
    }

    pub fn active_graph(&self) -> Option<&str> {
        self.primary_root
            .as_ref()
            .and_then(|root| self.active_graphs.get(root).map(String::as_str))
    }

    pub fn active_graph_for(&self, root: &ExperienceId) -> Option<&str> {
        self.active_graphs.get(root).map(String::as_str)
    }

    pub fn host_pid(&self) -> Option<u32> {
        self.primary_root
            .as_ref()
            .and_then(|root| self.hosts.get(root).map(ExperienceHost::id))
    }

    pub fn presented_graphs(&self) -> BTreeMap<ExperienceId, (String, u32)> {
        self.hosts
            .iter()
            .filter_map(|(root, host)| {
                self.active_graphs
                    .get(root)
                    .map(|graph_id| (root.clone(), (graph_id.clone(), host.id())))
            })
            .collect()
    }

    pub fn dismiss(&mut self, root: &ExperienceId) -> Result<Option<u32>> {
        let Some(host) = self.hosts.remove(root) else {
            return Ok(None);
        };
        let pid = host.id();
        host.terminate()?;
        self.active_graphs.remove(root);
        if self.primary_root.as_ref() == Some(root) {
            self.primary_root = self.hosts.keys().next().cloned();
        }
        Ok(Some(pid))
    }

    pub fn shutdown(&mut self) -> Result<()> {
        for (_, host) in std::mem::take(&mut self.hosts) {
            host.terminate()?;
        }
        self.active_graphs.clear();
        self.primary_root = None;
        Ok(())
    }

    pub fn restart_host(&mut self) -> Result<(String, u32, u32)> {
        let root = self.primary_root.clone().ok_or(Error::NoActiveHost)?;
        let host = self.hosts.remove(&root).ok_or(Error::NoActiveHost)?;
        let previous_pid = host.id();
        host.terminate()?;
        self.restart_current_graph(&root, previous_pid)
    }

    fn restart_after_exit(
        &mut self,
        root: &ExperienceId,
        _status: ExitStatus,
        failed_pid: u32,
    ) -> Result<(String, u32, u32)> {
        self.hosts.remove(root);
        self.restart_current_graph(root, failed_pid)
    }

    fn restart_current_graph(
        &mut self,
        root: &ExperienceId,
        previous_pid: u32,
    ) -> Result<(String, u32, u32)> {
        let (graph_id, graph) = self.graphs.current(root)?.ok_or(Error::NoCurrentRevision)?;
        self.validate_root(root, &graph)?;
        self.validate_live_instance_budget(std::iter::once((root, &graph)))?;
        let mut host = ExperienceHost::launch(self.host_command.clone(), self.host_timeout)?;
        host.boot_graph(
            &graph_id,
            self.graphs.snapshot_path(&graph_id)?,
            self.revisions.root().to_path_buf(),
        )?;
        let pid = host.id();
        self.hosts.insert(root.clone(), host);
        self.active_graphs.insert(root.clone(), graph_id.clone());
        Ok((graph_id, previous_pid, pid))
    }

    fn validate_registry_candidate(
        &self,
        experience_id: &ExperienceId,
        revision_id: &str,
    ) -> Result<()> {
        let candidate = self.revisions.verify(revision_id)?;
        let candidate_package = candidate.package.as_ref().ok_or_else(|| {
            Error::InvalidGraph("experience updates require a v4 package revision".into())
        })?;
        let record = self
            .registry
            .get(experience_id)?
            .ok_or_else(|| Error::InvalidGraph(format!("unknown experience `{experience_id}`")))?;
        if candidate_package.experience_id != *experience_id
            || candidate_package.role != record.role
        {
            return Err(Error::InvalidGraph(
                "candidate revision has the wrong experience identity or role".into(),
            ));
        }
        let current = self
            .registry
            .current(experience_id)?
            .ok_or(Error::NoCurrentRevision)?;
        if current.manifest.revision_id == revision_id {
            return Err(Error::InvalidGraph(
                "candidate revision is already current".into(),
            ));
        }
        Ok(())
    }

    fn validate_root(&self, root: &ExperienceId, graph: &ResolvedGraph) -> Result<()> {
        let node = graph
            .nodes
            .get(&graph.root)
            .ok_or_else(|| Error::InvalidGraph("graph root is missing".into()))?;
        if &node.experience_id != root {
            return Err(Error::InvalidGraph(
                "graph root does not match the activated experience".into(),
            ));
        }
        self.revisions.verify(node.revision_id.as_str())?;
        Ok(())
    }

    fn validate_graph_grants(&self, graph: &ResolvedGraph) -> Result<()> {
        if self.authority.is_none() {
            return Ok(());
        }
        for node in graph.nodes.values() {
            let revision = self.revisions.verify(node.revision_id.as_str())?;
            let package = revision.package.as_ref().ok_or_else(|| {
                Error::InvalidGraph("v4 graph node is missing package metadata".into())
            })?;
            let data_flows = package
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
                .collect::<BTreeMap<_, _>>();
            if package.provider_capabilities.is_empty() && data_flows.is_empty() {
                continue;
            }
            let decision = match self.call_authority(&ServiceRequest::GetResource {
                request_id: 36,
                query: ResourceQuery::GrantDecisionFor {
                    experience_id: node.experience_id.clone(),
                },
            })? {
                ResponsePayload::Resource {
                    value: ResourceValue::GrantDecision(decision),
                } => decision,
                _ => {
                    return Err(Error::InvalidGraph(
                        "authority returned the wrong grant decision payload".into(),
                    ))
                }
            };
            if !decision.reviewed
                || decision.experience_id != node.experience_id
                || !package
                    .provider_capabilities
                    .is_subset(&decision.provider_capabilities)
                || data_flows.iter().any(|(alias, requested)| {
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
                return Err(Error::InvalidGraph(format!(
                    "revision `{}` lacks an exact authority grant decision",
                    node.revision_id
                )));
            }
        }
        Ok(())
    }

    fn validate_live_instance_budget<'a>(
        &self,
        replacements: impl IntoIterator<Item = (&'a ExperienceId, &'a ResolvedGraph)>,
    ) -> Result<()> {
        let mut replacement_sizes = BTreeMap::new();
        for (root, graph) in replacements {
            if replacement_sizes
                .insert(root.clone(), graph.nodes.len())
                .is_some()
            {
                return Err(Error::InvalidGraph(format!(
                    "live graph budget repeats root `{root}`"
                )));
            }
        }
        let mut instances = 0usize;
        for root in self.hosts.keys() {
            instances += if let Some(size) = replacement_sizes.remove(root) {
                size
            } else {
                let graph_id = self.active_graphs.get(root).ok_or_else(|| {
                    Error::InvalidGraph(format!(
                        "presented root `{root}` has no active graph identity"
                    ))
                })?;
                self.graphs.verify(graph_id)?.nodes.len()
            };
        }
        instances += replacement_sizes.values().sum::<usize>();
        if instances > MAX_GRAPH_INSTANCES {
            return Err(Error::InvalidGraph(format!(
                "presented graphs require {instances} live instances; limit is {MAX_GRAPH_INSTANCES}"
            )));
        }
        Ok(())
    }

    fn apply_registry_updates(
        &self,
        journal: &GraphActivationJournal,
        candidate: bool,
    ) -> Result<()> {
        if journal.registry_updates.is_empty() {
            return self.registry.set_current(
                &journal.root_experience_id,
                if candidate {
                    &journal.candidate_root_revision
                } else {
                    &journal.previous_root_revision
                },
            );
        }
        for update in &journal.registry_updates {
            self.registry.set_current(
                &update.experience_id,
                if candidate {
                    &update.candidate_revision
                } else {
                    &update.previous_revision
                },
            )?;
        }
        Ok(())
    }

    fn apply_graph_updates(&self, journal: &GraphActivationJournal, candidate: bool) -> Result<()> {
        if journal.graph_updates.is_empty() {
            return self.graphs.set_current(
                &journal.root_experience_id,
                if candidate {
                    &journal.candidate_graph_id
                } else {
                    &journal.previous_graph_id
                },
            );
        }
        for update in &journal.graph_updates {
            self.graphs.set_current(
                &update.root_experience_id,
                if candidate {
                    &update.candidate_graph_id
                } else {
                    &update.previous_graph_id
                },
            )?;
        }
        Ok(())
    }

    fn stage_authority_activation(
        &self,
        graph_id: &str,
        graph: &ResolvedGraph,
    ) -> Result<Option<String>> {
        self.stage_authority_graphs(graph_id, std::slice::from_ref(graph))
    }

    fn stage_authority_graphs(
        &self,
        activation_id: &str,
        graphs: &[ResolvedGraph],
    ) -> Result<Option<String>> {
        let Some(_) = &self.authority else {
            return Ok(None);
        };
        let mut promotions = Vec::new();
        let mut seen = BTreeMap::new();
        for graph in graphs {
            for node in graph.nodes.values() {
                if let Some(revision_id) = seen.get(&node.experience_id) {
                    if revision_id != &node.revision_id {
                        return Err(Error::InvalidGraph(format!(
                            "experience `{}` resolves to multiple revisions across the activation set",
                            node.experience_id
                        )));
                    }
                    continue;
                }
                seen.insert(node.experience_id.clone(), node.revision_id.clone());
                self.append_authority_promotion(node, &mut promotions)?;
            }
        }
        if promotions.is_empty() {
            return Ok(None);
        }
        let transaction_id = format!(
            "graph-activate-{}-{}-{}",
            &activation_id[..32],
            std::process::id(),
            JOURNAL_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let draft = GraphPromotionDraft {
            transaction_id: transaction_id.clone(),
            activate: true,
            promotions,
        };
        match self.call_authority(&ServiceRequest::StageGraphPromotion {
            request_id: 30,
            draft: draft.clone(),
        })? {
            ResponsePayload::GraphTransaction { record }
                if record.draft == draft && record.status == TransactionStatus::Staged => {}
            _ => {
                return Err(Error::InvalidGraph(
                    "authority did not retain the staged graph activation".into(),
                ))
            }
        }
        Ok(Some(transaction_id))
    }

    fn append_authority_promotion(
        &self,
        node: &experience_package::ResolvedGraphNode,
        promotions: &mut Vec<GraphExperiencePromotion>,
    ) -> Result<()> {
        let revision = self.revisions.verify(node.revision_id.as_str())?;
        let durable: DurableState = serde_json::from_slice(&fs::read(
            revision.directory.join(&revision.manifest.state.path),
        )?)?;
        let current = self.authority_state_for(&node.experience_id)?;
        let exact = self.authority_state_at(&node.experience_id, node.revision_id.as_str())?;
        if current.revision_id == node.revision_id.as_str()
            && exact.revision_id == node.revision_id.as_str()
            && exact.schema_version == durable.schema_version
            && exact.source_sha256 == durable.source_sha256
        {
            return Ok(());
        }
        if durable.schema_version < current.schema_version {
            return Err(Error::InvalidGraph(format!(
                "candidate schema {} cannot replace experience `{}` schema {}",
                durable.schema_version, node.experience_id, current.schema_version
            )));
        }
        let (state, schema_version, source_sha256) = if exact.revision_id
            == node.revision_id.as_str()
            && exact.schema_version == durable.schema_version
            && exact.source_sha256 == durable.source_sha256
        {
            (exact.state, exact.schema_version, exact.source_sha256)
        } else {
            (durable.state, durable.schema_version, durable.source_sha256)
        };
        let migration = if schema_version > current.schema_version {
            Some(service_protocol::MigrationProof {
                from_schema_version: current.schema_version,
                to_schema_version: schema_version,
                from_state_sha256: state_sha256(&current.state)
                    .map_err(|error| Error::InvalidGraph(error.to_string()))?,
            })
        } else {
            None
        };
        promotions.push(GraphExperiencePromotion {
            experience_id: node.experience_id.clone(),
            expected_revision: current.revision,
            revision_id: node.revision_id.to_string(),
            schema_version,
            source_sha256,
            state,
            migration,
            actions: Vec::new(),
        });
        Ok(())
    }

    fn authority_state_for(
        &self,
        experience_id: &ExperienceId,
    ) -> Result<service_protocol::StateResource> {
        match self.call_authority(&ServiceRequest::GetResource {
            request_id: 31,
            query: ResourceQuery::ExperienceStateFor {
                experience_id: experience_id.clone(),
            },
        })? {
            ResponsePayload::Resource {
                value: ResourceValue::ExperienceStateFor(state),
            } => Ok(state.resource),
            _ => Err(Error::InvalidGraph(
                "authority returned the wrong experience state payload".into(),
            )),
        }
    }

    fn authority_state_at(
        &self,
        experience_id: &ExperienceId,
        revision_id: &str,
    ) -> Result<service_protocol::StateResource> {
        match self.call_authority(&ServiceRequest::GetResource {
            request_id: 32,
            query: ResourceQuery::ExperienceStateAt {
                experience_id: experience_id.clone(),
                revision_id: revision_id.into(),
            },
        })? {
            ResponsePayload::Resource {
                value: ResourceValue::ExperienceStateAt(state),
            } => Ok(state.resource),
            _ => Err(Error::InvalidGraph(
                "authority returned the wrong revision state payload".into(),
            )),
        }
    }

    fn call_authority(&self, request: &ServiceRequest) -> Result<ResponsePayload> {
        let client = self.authority.as_ref().ok_or_else(|| {
            Error::InvalidGraph("graph activation authority is unavailable".into())
        })?;
        let response = client.call(request)?;
        if !response.ok {
            return Err(Error::InvalidGraph(format!(
                "graph activation authority rejected the request: {:?}",
                response.error
            )));
        }
        response.payload.ok_or_else(|| {
            Error::InvalidGraph("graph activation authority omitted its response".into())
        })
    }

    fn promote_authority(&self, transaction_id: Option<&str>) -> Result<()> {
        let Some(transaction_id) = transaction_id else {
            return Ok(());
        };
        match self.call_authority(&ServiceRequest::PromoteGraph {
            request_id: 33,
            transaction_id: transaction_id.into(),
        })? {
            ResponsePayload::GraphTransaction { record }
                if record.status == TransactionStatus::Committed =>
            {
                Ok(())
            }
            _ => Err(Error::InvalidGraph(
                "authority did not commit the graph activation".into(),
            )),
        }
    }

    fn abort_authority(&self, transaction_id: Option<&str>) -> Result<()> {
        let Some(transaction_id) = transaction_id else {
            return Ok(());
        };
        match self.call_authority(&ServiceRequest::AbortGraph {
            request_id: 34,
            transaction_id: transaction_id.into(),
        })? {
            ResponsePayload::GraphTransaction { record }
                if record.status == TransactionStatus::Aborted =>
            {
                Ok(())
            }
            _ => Err(Error::InvalidGraph(
                "authority did not abort the graph activation".into(),
            )),
        }
    }

    fn authority_transaction_committed(&self, journal: &GraphActivationJournal) -> Result<bool> {
        let Some(transaction_id) = &journal.authority_transaction_id else {
            return Ok(matches!(
                journal.phase,
                GraphActivationPhase::AuthorityCommitted
                    | GraphActivationPhase::RegistryCommitted
                    | GraphActivationPhase::GraphCommitted
            ));
        };
        match self.call_authority(&ServiceRequest::GetGraphTransaction {
            request_id: 35,
            transaction_id: transaction_id.clone(),
        })? {
            ResponsePayload::GraphTransaction { record } => {
                Ok(record.status == TransactionStatus::Committed)
            }
            _ => Err(Error::InvalidGraph(
                "authority returned the wrong graph transaction payload".into(),
            )),
        }
    }

    fn inject(&mut self, point: GraphActivationFaultPoint) -> Result<()> {
        if self.fault == Some(point) {
            self.fault = None;
            Err(Error::InjectedGraphActivationFault(format!("{point:?}")))
        } else {
            Ok(())
        }
    }

    fn fail_activation<T>(&mut self, journal: &GraphActivationJournal, error: Error) -> Result<T> {
        let authority_committed =
            self.authority_transaction_committed(journal)
                .unwrap_or(matches!(
                    journal.phase,
                    GraphActivationPhase::AuthorityCommitted
                        | GraphActivationPhase::RegistryCommitted
                        | GraphActivationPhase::GraphCommitted
                ));
        let recovery = if authority_committed {
            self.roll_forward_activation(journal)
        } else {
            self.rollback_activation(journal)
        };
        match recovery {
            Ok(()) => Err(error),
            Err(recovery) => Err(Error::InvalidGraph(format!(
                "graph activation failed ({error}); recovery also failed ({recovery})"
            ))),
        }
    }

    fn rollback_activation(&mut self, journal: &GraphActivationJournal) -> Result<()> {
        self.abort_authority(journal.authority_transaction_id.as_deref())?;
        for update in self.journal_graph_updates(journal) {
            if let Some(host) = self.hosts.get_mut(&update.root_experience_id) {
                host.discard_graph(&update.candidate_graph_id)?;
            }
        }
        self.apply_registry_updates(journal, false)?;
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        self.apply_graph_updates(journal, false)?;
        for update in self.journal_graph_updates(journal) {
            if self.hosts.contains_key(&update.root_experience_id) {
                self.active_graphs
                    .insert(update.root_experience_id, update.previous_graph_id);
            }
        }
        self.clear_journal()
    }

    fn roll_forward_activation(&mut self, journal: &GraphActivationJournal) -> Result<()> {
        self.apply_registry_updates(journal, true)?;
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        self.apply_graph_updates(journal, true)?;
        for update in self.journal_graph_updates(journal) {
            if let Some(host) = self.hosts.get_mut(&update.root_experience_id) {
                host.finalize_graph(&update.candidate_graph_id)?;
                self.active_graphs
                    .insert(update.root_experience_id, update.candidate_graph_id);
            }
        }
        self.clear_journal()
    }

    fn journal_graph_updates(&self, journal: &GraphActivationJournal) -> Vec<GraphPointerUpdate> {
        if journal.graph_updates.is_empty() {
            vec![GraphPointerUpdate {
                root_experience_id: journal.root_experience_id.clone(),
                previous_graph_id: journal.previous_graph_id.clone(),
                candidate_graph_id: journal.candidate_graph_id.clone(),
            }]
        } else {
            journal.graph_updates.clone()
        }
    }

    fn validate_journal(&self, journal: &GraphActivationJournal) -> Result<()> {
        if journal.format_version != GRAPH_ACTIVATION_JOURNAL_VERSION {
            return Err(Error::InvalidGraph(
                "invalid graph activation journal version".into(),
            ));
        }
        let previous = self.graphs.verify(&journal.previous_graph_id)?;
        let candidate = self.graphs.verify(&journal.candidate_graph_id)?;
        self.validate_root(&journal.root_experience_id, &previous)?;
        self.validate_root(&journal.root_experience_id, &candidate)?;
        if previous.nodes[&previous.root].revision_id.as_str() != journal.previous_root_revision
            || candidate.nodes[&candidate.root].revision_id.as_str()
                != journal.candidate_root_revision
        {
            return Err(Error::InvalidGraph(
                "graph activation journal root binding mismatch".into(),
            ));
        }
        let mut graph_roots = std::collections::BTreeSet::new();
        for update in self.journal_graph_updates(journal) {
            if !graph_roots.insert(update.root_experience_id.clone()) {
                return Err(Error::InvalidGraph(
                    "graph activation journal repeats a graph root".into(),
                ));
            }
            let previous = self.graphs.verify(&update.previous_graph_id)?;
            let candidate = self.graphs.verify(&update.candidate_graph_id)?;
            self.validate_root(&update.root_experience_id, &previous)?;
            self.validate_root(&update.root_experience_id, &candidate)?;
        }
        let mut experiences = std::collections::BTreeSet::new();
        for update in &journal.registry_updates {
            if !experiences.insert(update.experience_id.clone()) {
                return Err(Error::InvalidGraph(
                    "graph activation journal repeats a registry experience".into(),
                ));
            }
            let record = self.registry.get(&update.experience_id)?.ok_or_else(|| {
                Error::InvalidGraph(format!(
                    "graph activation journal names unknown experience `{}`",
                    update.experience_id
                ))
            })?;
            for revision_id in [&update.previous_revision, &update.candidate_revision] {
                let revision = self.revisions.verify(revision_id)?;
                if let Some(package) = revision.package {
                    if package.experience_id != update.experience_id || package.role != record.role
                    {
                        return Err(Error::InvalidGraph(
                            "graph activation journal registry binding mismatch".into(),
                        ));
                    }
                } else if !record.accepts_legacy_revisions {
                    return Err(Error::InvalidGraph(
                        "graph activation journal names an unauthorized legacy revision".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn load_journal(&self) -> Result<Option<GraphActivationJournal>> {
        match fs::read(&self.journal_file) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_journal(&self, journal: &GraphActivationJournal) -> Result<()> {
        let temporary = self.revisions.root().join(format!(
            ".graph-activation-journal-{}-{}.tmp",
            std::process::id(),
            JOURNAL_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> Result<()> {
            file.write_all(&serde_json::to_vec_pretty(journal)?)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.journal_file)?;
            sync_directory(self.revisions.root())
        })();
        if result.is_err() {
            fs::remove_file(&temporary).ok();
        }
        result
    }

    fn clear_journal(&self) -> Result<()> {
        match fs::remove_file(&self.journal_file) {
            Ok(()) => sync_directory(self.revisions.root()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

impl Drop for ExperienceGraphSupervisor {
    fn drop(&mut self) {
        self.shutdown().ok();
    }
}
