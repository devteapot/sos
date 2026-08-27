use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use experience_package::{ExperienceId, ResolvedGraph};
use provider_state_service::{state_sha256, ServiceClient};
use serde::{Deserialize, Serialize};
use service_protocol::{
    GraphExperiencePromotion, GraphPromotionDraft, ResourceQuery, ResourceValue, ResponsePayload,
    ServiceRequest, TransactionStatus,
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
    pub phase: GraphActivationPhase,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RegistryPointerUpdate {
    pub experience_id: ExperienceId,
    pub previous_revision: String,
    pub candidate_revision: String,
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
    host: Option<ExperienceHost>,
    active_root: Option<ExperienceId>,
    active_graph: Option<String>,
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
    input_quiesced: bool,
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
            host: None,
            active_root: None,
            active_graph: None,
            authority: None,
            fault: None,
        }
    }

    pub fn with_authority(mut self, authority: ServiceClient) -> Self {
        self.authority = Some(authority);
        self
    }

    pub fn boot(&mut self, root_experience_id: &ExperienceId) -> Result<Option<u32>> {
        self.recover()?;
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        let Some((graph_id, graph)) = self.graphs.current(root_experience_id)? else {
            return Ok(None);
        };
        self.validate_root(root_experience_id, &graph)?;
        let mut host = ExperienceHost::launch(self.host_command.clone(), self.host_timeout)?;
        host.boot_graph(
            &graph_id,
            self.graphs.snapshot_path(&graph_id)?,
            self.revisions.root().to_path_buf(),
        )?;
        let pid = host.id();
        self.host = Some(host);
        self.active_root = Some(root_experience_id.clone());
        self.active_graph = Some(graph_id);
        Ok(Some(pid))
    }

    pub fn prepare(
        &mut self,
        root_experience_id: &ExperienceId,
        graph_id: &str,
    ) -> Result<PreparedGraphActivation> {
        let graph = self.graphs.verify(graph_id)?;
        self.validate_root(root_experience_id, &graph)?;
        let candidate_root_revision = graph.nodes[&graph.root].revision_id.to_string();
        let previous_graph_id = self
            .active_graph
            .clone()
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
        self.host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .prepare_graph(
                graph_id,
                self.graphs.snapshot_path(graph_id)?,
                self.revisions.root().to_path_buf(),
            )?;
        let authority_transaction_id = match self.stage_authority_activation(graph_id, &graph) {
            Ok(transaction_id) => transaction_id,
            Err(error) => {
                let _ = self
                    .host
                    .as_mut()
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
            input_quiesced: false,
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
        let root = self.active_root.clone().ok_or(Error::NoCurrentRevision)?;
        if *experience_id == root {
            affected.insert(root.clone());
        }
        if affected != std::collections::BTreeSet::from([root.clone()]) {
            return Err(Error::InvalidGraph(format!(
                "tracked update affects active roots {:?}; this supervisor owns only `{root}`",
                affected
            )));
        }
        let root_revision = self
            .registry
            .current(&root)?
            .ok_or(Error::NoCurrentRevision)?
            .manifest
            .revision_id;
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

    pub fn advance_experience(
        &mut self,
        experience_id: &ExperienceId,
        revision_id: &str,
    ) -> Result<Option<(String, u32)>> {
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        let mut affected = self
            .reverse_dependencies
            .affected_active_roots(experience_id, &self.graphs)?;
        if self.active_root.as_ref() == Some(experience_id) {
            affected.insert(experience_id.clone());
        }
        if affected.is_empty() {
            self.validate_registry_candidate(experience_id, revision_id)?;
            self.registry.set_current(experience_id, revision_id)?;
            self.reverse_dependencies
                .rebuild(&self.revisions, &self.registry)?;
            return Ok(None);
        }
        let prepared = self.prepare_tracked_update(experience_id, revision_id)?;
        let graph_id = prepared.graph_id().to_owned();
        let pid = self.commit(prepared)?;
        Ok(Some((graph_id, pid)))
    }

    pub fn quiesce(&mut self, prepared: &mut PreparedGraphActivation) -> Result<()> {
        if prepared.input_quiesced {
            return Ok(());
        }
        self.host
            .as_mut()
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
            phase: GraphActivationPhase::Intent,
        };
        if let Err(error) = self.quiesce(&mut prepared) {
            let _ = self
                .host
                .as_mut()
                .and_then(|host| host.discard_graph(&prepared.graph_id).ok());
            return Err(error);
        }
        if let Err(error) = self.write_journal(&journal) {
            return self.fail_activation(&journal, error);
        }
        self.inject(GraphActivationFaultPoint::AfterIntent)?;
        if let Err(error) = self
            .host
            .as_mut()
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
        self.host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .finalize_graph(&prepared.graph_id)?;
        self.active_root = Some(prepared.root_experience_id);
        self.active_graph = Some(prepared.graph_id);
        self.clear_journal()?;
        Ok(self.host.as_ref().expect("active host exists").id())
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
                self.graphs
                    .set_current(&journal.root_experience_id, &journal.previous_graph_id)?;
            }
            _ => {
                self.apply_registry_updates(&journal, true)?;
                self.reverse_dependencies
                    .rebuild(&self.revisions, &self.registry)?;
                self.graphs
                    .set_current(&journal.root_experience_id, &journal.candidate_graph_id)?;
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
        self.host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .discard_graph(&prepared.graph_id)
    }

    pub fn poll(&mut self) -> Result<Option<(String, u32, u32)>> {
        let Some(host) = self.host.as_mut() else {
            return Ok(None);
        };
        let Some(status) = host.try_wait()? else {
            return Ok(None);
        };
        let failed_pid = host.id();
        self.restart_after_exit(status, failed_pid).map(Some)
    }

    pub fn active_graph(&self) -> Option<&str> {
        self.active_graph.as_deref()
    }

    pub fn host_pid(&self) -> Option<u32> {
        self.host.as_ref().map(ExperienceHost::id)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(host) = self.host.take() {
            host.terminate()?;
        }
        self.active_graph = None;
        self.active_root = None;
        Ok(())
    }

    pub fn restart_host(&mut self) -> Result<(String, u32, u32)> {
        let host = self.host.take().ok_or(Error::NoActiveHost)?;
        let previous_pid = host.id();
        host.terminate()?;
        self.restart_current_graph(previous_pid)
    }

    fn restart_after_exit(
        &mut self,
        _status: ExitStatus,
        failed_pid: u32,
    ) -> Result<(String, u32, u32)> {
        self.host.take();
        self.restart_current_graph(failed_pid)
    }

    fn restart_current_graph(&mut self, previous_pid: u32) -> Result<(String, u32, u32)> {
        let root = self.active_root.clone().ok_or(Error::NoCurrentRevision)?;
        let (graph_id, graph) = self
            .graphs
            .current(&root)?
            .ok_or(Error::NoCurrentRevision)?;
        self.validate_root(&root, &graph)?;
        let mut host = ExperienceHost::launch(self.host_command.clone(), self.host_timeout)?;
        host.boot_graph(
            &graph_id,
            self.graphs.snapshot_path(&graph_id)?,
            self.revisions.root().to_path_buf(),
        )?;
        let pid = host.id();
        self.host = Some(host);
        self.active_graph = Some(graph_id.clone());
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

    fn stage_authority_activation(
        &self,
        graph_id: &str,
        graph: &ResolvedGraph,
    ) -> Result<Option<String>> {
        let Some(_) = &self.authority else {
            return Ok(None);
        };
        let mut promotions = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for node in graph.nodes.values() {
            if !seen.insert(node.experience_id.clone()) {
                continue;
            }
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
                continue;
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
        }
        if promotions.is_empty() {
            return Ok(None);
        }
        let transaction_id = format!(
            "graph-activate-{}-{}-{}",
            &graph_id[..32],
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
        self.host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .discard_graph(&journal.candidate_graph_id)?;
        self.apply_registry_updates(journal, false)?;
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        self.graphs
            .set_current(&journal.root_experience_id, &journal.previous_graph_id)?;
        self.active_root = Some(journal.root_experience_id.clone());
        self.active_graph = Some(journal.previous_graph_id.clone());
        self.clear_journal()
    }

    fn roll_forward_activation(&mut self, journal: &GraphActivationJournal) -> Result<()> {
        self.apply_registry_updates(journal, true)?;
        self.reverse_dependencies
            .rebuild(&self.revisions, &self.registry)?;
        self.graphs
            .set_current(&journal.root_experience_id, &journal.candidate_graph_id)?;
        self.host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .finalize_graph(&journal.candidate_graph_id)?;
        self.active_root = Some(journal.root_experience_id.clone());
        self.active_graph = Some(journal.candidate_graph_id.clone());
        self.clear_journal()
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
