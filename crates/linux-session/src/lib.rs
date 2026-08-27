mod authoring;
mod system_session;

use std::{fs, path::Path, time::Duration};

use anyhow::{bail, Context as _, Result};
use experience_package::ExperienceId;
use provider_state_service::{state_sha256, ServiceClient};
use revision_supervisor::{
    ActivationJournal, DurableState, GraphStore, RevisionStore, VerifiedRevision,
};
use service_protocol::{
    GraphExperiencePromotion, GraphPromotionDraft, MigrationProof, PromotionDraft, ResourceQuery,
    ResourceValue, ResponsePayload, ServiceRequest, StateResource, TransactionRecord,
    TransactionStatus,
};

pub use authoring::{run_authoring_broker, AuthoringBrokerOptions};
pub use system_session::{
    run_host_proxy, run_system_session, ServiceIdentity, SessionIdentityMode, SystemSessionOptions,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapOutcome {
    Initialized {
        transaction_id: String,
        revision_id: String,
    },
    AlreadyBound {
        revision_id: String,
    },
    RecoveryRequired {
        pointer_revision: String,
        authority_revision: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphBootstrapOutcome {
    Initialized {
        transaction_id: String,
        graph_id: String,
        experience_count: usize,
    },
    AlreadyBound {
        graph_id: String,
    },
}

struct RevisionState {
    revision_id: String,
    durable: DurableState,
}

pub fn bootstrap_authority(
    revision_root: &Path,
    service_socket: &Path,
    timeout: Duration,
) -> Result<BootstrapOutcome> {
    let store = RevisionStore::open(revision_root)?;
    let revision = store
        .current()?
        .context("cannot bootstrap authority before the revision pointer is initialized")?;
    let target = load_revision_state(&revision)?;
    let client = ServiceClient::new(service_socket, timeout);
    let current = get_state(&client)?;
    if state_binding_matches(&current, &target) {
        return Ok(BootstrapOutcome::AlreadyBound {
            revision_id: target.revision_id,
        });
    }
    if current != StateResource::default() {
        if journal_binds_mismatch(revision_root, &target.revision_id, &current.revision_id)? {
            return Ok(BootstrapOutcome::RecoveryRequired {
                pointer_revision: target.revision_id,
                authority_revision: current.revision_id,
            });
        }
        bail!(
            "provider authority is already initialized at revision {}; refusing to replace it with {}",
            display_revision(&current.revision_id),
            target.revision_id
        );
    }

    let transaction_id = format!("linux-bootstrap-{}", target.revision_id);
    let draft = draft(&transaction_id, &current, &target)?;
    stage(&client, draft)?;
    promote(&client, &transaction_id)?;
    let committed = get_state(&client)?;
    if !state_matches(&committed, &target) {
        bail!("provider authority did not commit the boot revision binding");
    }
    Ok(BootstrapOutcome::Initialized {
        transaction_id,
        revision_id: target.revision_id,
    })
}

pub fn bootstrap_graph_authority(
    revision_root: &Path,
    root_experience_id: &ExperienceId,
    service_socket: &Path,
    timeout: Duration,
) -> Result<GraphBootstrapOutcome> {
    let store = RevisionStore::open(revision_root)?;
    let graphs = GraphStore::open(revision_root)?;
    let (graph_id, graph) = graphs
        .current(root_experience_id)?
        .with_context(|| format!("experience `{root_experience_id}` has no active graph"))?;
    let client = ServiceClient::new(service_socket, timeout);
    let mut promotions = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for node in graph.nodes.values() {
        if !seen.insert(node.experience_id.clone()) {
            let first = graph
                .nodes
                .values()
                .find(|candidate| candidate.experience_id == node.experience_id)
                .expect("seen experience has a graph node");
            if first.revision_id != node.revision_id {
                bail!(
                    "graph uses experience `{}` at more than one revision",
                    node.experience_id
                );
            }
            continue;
        }
        let revision = store.verify(node.revision_id.as_str())?;
        let durable = load_revision_state(&revision)?.durable;
        let exact =
            get_experience_state_at(&client, &node.experience_id, node.revision_id.as_str())?;
        if exact.revision_id == node.revision_id.as_str()
            && exact.schema_version == durable.schema_version
            && exact.source_sha256 == durable.source_sha256
        {
            continue;
        }
        let current = get_experience_state_for(&client, &node.experience_id)?;
        if durable.schema_version < current.schema_version {
            bail!(
                "graph revision schema {} cannot replace experience `{}` schema {}",
                durable.schema_version,
                node.experience_id,
                current.schema_version
            );
        }
        let migration = if durable.schema_version > current.schema_version {
            Some(MigrationProof {
                from_schema_version: current.schema_version,
                to_schema_version: durable.schema_version,
                from_state_sha256: state_sha256(&current.state)?,
            })
        } else {
            None
        };
        promotions.push(GraphExperiencePromotion {
            experience_id: node.experience_id.clone(),
            expected_revision: current.revision,
            revision_id: node.revision_id.to_string(),
            schema_version: durable.schema_version,
            source_sha256: durable.source_sha256,
            state: durable.state,
            migration,
            actions: Vec::new(),
        });
    }
    if promotions.is_empty() {
        return Ok(GraphBootstrapOutcome::AlreadyBound { graph_id });
    }
    let transaction_id = format!("linux-graph-bootstrap-{graph_id}");
    let draft = GraphPromotionDraft {
        transaction_id: transaction_id.clone(),
        activate: true,
        promotions,
    };
    let staged = call(
        &client,
        &ServiceRequest::StageGraphPromotion {
            request_id: 20,
            draft: draft.clone(),
        },
    )?;
    match staged {
        ResponsePayload::GraphTransaction { record }
            if record.draft == draft && record.status != TransactionStatus::Aborted => {}
        _ => bail!("provider authority did not retain graph bootstrap transaction"),
    }
    let promoted = call(
        &client,
        &ServiceRequest::PromoteGraph {
            request_id: 21,
            transaction_id: transaction_id.clone(),
        },
    )?;
    let experience_count = match promoted {
        ResponsePayload::GraphTransaction { record }
            if record.draft == draft && record.status == TransactionStatus::Committed =>
        {
            record.draft.promotions.len()
        }
        _ => bail!("provider authority did not commit graph bootstrap transaction"),
    };
    for promotion in &draft.promotions {
        let exact =
            get_experience_state_at(&client, &promotion.experience_id, &promotion.revision_id)?;
        if exact.revision_id != promotion.revision_id
            || exact.schema_version != promotion.schema_version
            || exact.source_sha256 != promotion.source_sha256
            || exact.state != promotion.state
        {
            bail!(
                "provider authority did not bind graph state for experience `{}`",
                promotion.experience_id
            );
        }
    }
    Ok(GraphBootstrapOutcome::Initialized {
        transaction_id,
        graph_id,
        experience_count,
    })
}

fn get_experience_state_for(
    client: &ServiceClient,
    experience_id: &ExperienceId,
) -> Result<StateResource> {
    match call(
        client,
        &ServiceRequest::GetResource {
            request_id: 22,
            query: ResourceQuery::ExperienceStateFor {
                experience_id: experience_id.clone(),
            },
        },
    )? {
        ResponsePayload::Resource {
            value: ResourceValue::ExperienceStateFor(state),
        } => Ok(state.resource),
        _ => bail!("provider authority returned the wrong experience state payload"),
    }
}

fn get_experience_state_at(
    client: &ServiceClient,
    experience_id: &ExperienceId,
    revision_id: &str,
) -> Result<StateResource> {
    match call(
        client,
        &ServiceRequest::GetResource {
            request_id: 23,
            query: ResourceQuery::ExperienceStateAt {
                experience_id: experience_id.clone(),
                revision_id: revision_id.into(),
            },
        },
    )? {
        ResponsePayload::Resource {
            value: ResourceValue::ExperienceStateAt(state),
        } => Ok(state.resource),
        _ => bail!("provider authority returned the wrong revision state payload"),
    }
}

fn journal_binds_mismatch(
    revision_root: &Path,
    pointer_revision: &str,
    authority_revision: &str,
) -> Result<bool> {
    let path = revision_root.join("activation-journal.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let journal: ActivationJournal =
        serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))?;
    Ok(journal.previous_revision == pointer_revision
        && journal.candidate_revision == authority_revision)
}

pub fn stage_revision(
    revision_root: &Path,
    revision_id: &str,
    service_socket: &Path,
    timeout: Duration,
) -> Result<String> {
    let store = RevisionStore::open(revision_root)?;
    let current_revision = store
        .current()?
        .context("cannot stage activation before the revision pointer is initialized")?;
    let accepted = load_revision_state(&current_revision)?;
    let candidate = load_revision_state(&store.verify(revision_id)?)?;
    let client = ServiceClient::new(service_socket, timeout);
    let current = get_state(&client)?;
    if !state_binding_matches(&current, &accepted) {
        bail!(
            "provider authority revision {} does not match the accepted pointer {}",
            display_revision(&current.revision_id),
            accepted.revision_id
        );
    }
    if candidate.revision_id == accepted.revision_id {
        bail!("candidate revision is already active");
    }

    let transaction_id = format!(
        "linux-activate-{}-{}",
        current.revision, candidate.revision_id
    );
    stage(&client, draft(&transaction_id, &current, &candidate)?)?;
    Ok(transaction_id)
}

pub fn shutdown_authority(service_socket: &Path, timeout: Duration) -> Result<()> {
    let client = ServiceClient::new(service_socket, timeout);
    match call(&client, &ServiceRequest::Shutdown { request_id: 90 })? {
        ResponsePayload::Shutdown => Ok(()),
        _ => bail!("provider authority returned the wrong shutdown payload"),
    }
}

fn load_revision_state(revision: &VerifiedRevision) -> Result<RevisionState> {
    let path = revision.directory.join(&revision.manifest.state.path);
    let durable: DurableState = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("read revision state {}", path.display()))?,
    )
    .with_context(|| format!("decode revision state {}", path.display()))?;
    Ok(RevisionState {
        revision_id: revision.manifest.revision_id.clone(),
        durable,
    })
}

fn draft(
    transaction_id: &str,
    current: &StateResource,
    target: &RevisionState,
) -> Result<PromotionDraft> {
    if target.durable.schema_version < current.schema_version {
        bail!(
            "candidate schema {} cannot replace authority schema {}",
            target.durable.schema_version,
            current.schema_version
        );
    }
    let migration = if target.durable.schema_version > current.schema_version {
        Some(MigrationProof {
            from_schema_version: current.schema_version,
            to_schema_version: target.durable.schema_version,
            from_state_sha256: state_sha256(&current.state)?,
        })
    } else {
        None
    };
    Ok(PromotionDraft {
        transaction_id: transaction_id.into(),
        expected_revision: current.revision,
        revision_id: target.revision_id.clone(),
        schema_version: target.durable.schema_version,
        source_sha256: target.durable.source_sha256.clone(),
        state: target.durable.state.clone(),
        migration,
        actions: Vec::new(),
    })
}

fn get_state(client: &ServiceClient) -> Result<StateResource> {
    match call(
        client,
        &ServiceRequest::GetResource {
            request_id: 1,
            query: ResourceQuery::ExperienceState,
        },
    )? {
        ResponsePayload::Resource {
            value: ResourceValue::ExperienceState(state),
        } => Ok(state),
        _ => bail!("provider authority returned the wrong state payload"),
    }
}

fn stage(client: &ServiceClient, draft: PromotionDraft) -> Result<TransactionRecord> {
    let transaction_id = draft.transaction_id.clone();
    let response = client.call(&ServiceRequest::StagePromotion {
        request_id: 2,
        draft: draft.clone(),
    });
    match response {
        Ok(response) if response.ok => {
            let record = transaction_payload(response.payload)?;
            if record.draft != draft {
                bail!("provider authority returned a different staged transaction");
            }
            Ok(record)
        }
        response => {
            let first_error = describe_response_error(response);
            let record = get_transaction(client, &transaction_id).with_context(|| first_error)?;
            if record.draft != draft || record.status == TransactionStatus::Aborted {
                bail!("provider authority did not retain the requested staged transaction");
            }
            Ok(record)
        }
    }
}

fn promote(client: &ServiceClient, transaction_id: &str) -> Result<TransactionRecord> {
    let response = client.call(&ServiceRequest::Promote {
        request_id: 3,
        transaction_id: transaction_id.into(),
    });
    let record = match response {
        Ok(response) if response.ok => transaction_payload(response.payload)?,
        response => {
            let first_error = describe_response_error(response);
            get_transaction(client, transaction_id).with_context(|| first_error)?
        }
    };
    if record.status != TransactionStatus::Committed {
        bail!(
            "provider authority did not commit transaction {transaction_id}: {:?}",
            record.status
        );
    }
    Ok(record)
}

fn get_transaction(client: &ServiceClient, transaction_id: &str) -> Result<TransactionRecord> {
    match call(
        client,
        &ServiceRequest::GetTransaction {
            request_id: 4,
            transaction_id: transaction_id.into(),
        },
    )? {
        ResponsePayload::Transaction { record } => Ok(record),
        _ => bail!("provider authority returned the wrong transaction payload"),
    }
}

fn call(client: &ServiceClient, request: &ServiceRequest) -> Result<ResponsePayload> {
    let response = client.call(request)?;
    if !response.ok {
        bail!("provider authority rejected request: {:?}", response.error);
    }
    response
        .payload
        .context("provider authority response omitted its payload")
}

fn transaction_payload(payload: Option<ResponsePayload>) -> Result<TransactionRecord> {
    match payload {
        Some(ResponsePayload::Transaction { record }) => Ok(record),
        _ => bail!("provider authority returned the wrong transaction payload"),
    }
}

fn describe_response_error(response: std::io::Result<service_protocol::ServiceResponse>) -> String {
    match response {
        Ok(response) => format!("provider authority rejected request: {:?}", response.error),
        Err(error) => format!("provider authority request was ambiguous: {error}"),
    }
}

fn state_matches(current: &StateResource, target: &RevisionState) -> bool {
    state_binding_matches(current, target) && current.state == target.durable.state
}

fn state_binding_matches(current: &StateResource, target: &RevisionState) -> bool {
    current.revision_id == target.revision_id
        && current.schema_version == target.durable.schema_version
        && current.source_sha256 == target.durable.source_sha256
}

fn display_revision(revision_id: &str) -> &str {
    if revision_id.is_empty() {
        "<uninitialized>"
    } else {
        revision_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_binding_allows_authoritative_interaction_state_to_evolve() {
        let target = RevisionState {
            revision_id: "revision-a".into(),
            durable: DurableState {
                schema_version: 1,
                source_sha256: "source-a".into(),
                state: serde_json::json!({}),
            },
        };
        let current = StateResource {
            revision: 3,
            revision_id: "revision-a".into(),
            schema_version: 1,
            source_sha256: "source-a".into(),
            state: serde_json::json!({"agent_draft": ""}),
        };
        assert!(state_binding_matches(&current, &target));
        assert!(!state_matches(&current, &target));
    }
}
