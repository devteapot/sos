mod authoring;
mod system_session;

use std::{fs, path::Path, time::Duration};

use anyhow::{bail, Context as _, Result};
use experience_package::ExperienceId;
use provider_state_service::{state_sha256, ServiceClient};
use revision_supervisor::{DurableState, GraphStore, RevisionStore, VerifiedRevision};
use service_protocol::{
    DataFlowGrant, GrantDecisionResource, GraphExperiencePromotion, GraphPromotionDraft,
    MigrationProof, ResourceQuery, ResourceValue, ResponsePayload, ServiceError, ServiceRequest,
    StateResource, TransactionStatus,
};

pub use authoring::{run_authoring_broker, AuthoringBrokerOptions};
pub use system_session::{
    run_host_proxy, run_system_session, ServiceIdentity, SessionIdentityMode, SystemSessionOptions,
};

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
        let durable = load_revision_state(&revision)?;
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

pub fn review_trusted_graph_grants(
    revision_root: &Path,
    root_experience_id: &ExperienceId,
    trusted_root_revision: &str,
    service_socket: &Path,
    grant_capability: &str,
    timeout: Duration,
) -> Result<usize> {
    let store = RevisionStore::open(revision_root)?;
    let graphs = GraphStore::open(revision_root)?;
    let (_, graph) = graphs
        .current(root_experience_id)?
        .with_context(|| format!("experience `{root_experience_id}` has no active graph"))?;
    let root = graph
        .nodes
        .get(&graph.root)
        .context("trusted graph root is missing")?;
    if root.revision_id.as_str() != trusted_root_revision {
        return Ok(0);
    }
    let client = ServiceClient::new(service_socket, timeout);
    let mut updated = 0usize;
    // Bootstrap only the exact product revision named by the native launcher.
    // Children retain their own Experience grants and are never approved merely
    // because they happen to be mounted by a trusted root.
    for node in std::iter::once(root) {
        let revision = store.verify(node.revision_id.as_str())?;
        let package = revision.package;
        let data_flows: std::collections::BTreeMap<
            experience_package::DependencyAlias,
            DataFlowGrant,
        > = package
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
            .collect();
        if package.provider_capabilities.is_empty() && data_flows.is_empty() {
            continue;
        }
        let queried = client.call(&ServiceRequest::GetResource {
            request_id: 22,
            query: ResourceQuery::GrantDecisionFor {
                experience_id: node.experience_id.clone(),
            },
        })?;
        let (expected_generation, existing) = match (queried.payload, queried.error) {
            (
                Some(ResponsePayload::Resource {
                    value: ResourceValue::GrantDecision(decision),
                }),
                _,
            ) if queried.ok => (decision.generation, Some(decision)),
            (_, Some(ServiceError::NotFound { .. })) => (0, None),
            (_, error) => bail!("grant authority query failed: {error:?}"),
        };
        if existing.as_ref().is_some_and(|decision| {
            decision.reviewed
                && decision.experience_id == node.experience_id
                && package
                    .provider_capabilities
                    .is_subset(&decision.provider_capabilities)
                && data_flows.iter().all(|(alias, requested)| {
                    decision.data_flows.get(alias).is_some_and(|approved| {
                        approved.experience_id == requested.experience_id
                            && approved.export_id == requested.export_id
                            && requested
                                .grant
                                .properties
                                .is_subset(&approved.grant.properties)
                            && requested.grant.events.is_subset(&approved.grant.events)
                    })
                })
        }) {
            continue;
        }
        let mut provider_capabilities = package.provider_capabilities;
        let mut approved_data_flows = data_flows;
        if let Some(existing) = existing {
            provider_capabilities.extend(existing.provider_capabilities);
            for (alias, flow) in existing.data_flows {
                approved_data_flows.entry(alias).or_insert(flow);
            }
        }
        let response = client.call(&ServiceRequest::UpdateGrantDecision {
            request_id: 23,
            expected_generation,
            capability: grant_capability.into(),
            decision: GrantDecisionResource {
                generation: expected_generation.saturating_add(1),
                reviewed: true,
                experience_id: node.experience_id.clone(),
                provider_capabilities,
                data_flows: approved_data_flows,
            },
        })?;
        if !response.ok {
            bail!(
                "grant authority rejected trusted graph review: {:?}",
                response.error
            );
        }
        updated = updated.saturating_add(1);
    }
    Ok(updated)
}

pub fn review_revision_grants(
    revision_root: &Path,
    revision_id: &str,
    service_socket: &Path,
    grant_capability: &str,
    timeout: Duration,
) -> Result<GrantDecisionResource> {
    let store = RevisionStore::open(revision_root)?;
    let revision = store.verify(revision_id)?;
    let package = revision.package;
    let data_flows: std::collections::BTreeMap<experience_package::DependencyAlias, DataFlowGrant> =
        package
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
            .collect();
    let client = ServiceClient::new(service_socket, timeout);
    let queried = client.call(&ServiceRequest::GetResource {
        request_id: 24,
        query: ResourceQuery::GrantDecisionFor {
            experience_id: package.experience_id.clone(),
        },
    })?;
    let (expected_generation, existing) = match (queried.ok, queried.payload, queried.error) {
        (
            true,
            Some(ResponsePayload::Resource {
                value: ResourceValue::GrantDecision(decision),
            }),
            _,
        ) => (decision.generation, Some(decision)),
        (_, _, Some(ServiceError::NotFound { .. })) => (0, None),
        (_, _, error) => bail!("grant authority query failed: {error:?}"),
    };
    if existing.as_ref().is_some_and(|decision| {
        decision.reviewed
            && package
                .provider_capabilities
                .is_subset(&decision.provider_capabilities)
            && data_flows.iter().all(|(alias, requested)| {
                decision.data_flows.get(alias).is_some_and(|approved| {
                    approved.experience_id == requested.experience_id
                        && approved.export_id == requested.export_id
                        && requested
                            .grant
                            .properties
                            .is_subset(&approved.grant.properties)
                        && requested.grant.events.is_subset(&approved.grant.events)
                })
            })
    }) {
        return Ok(existing.expect("checked as present"));
    }
    let mut provider_capabilities = package.provider_capabilities;
    let mut approved_data_flows = data_flows;
    if let Some(existing) = existing {
        provider_capabilities.extend(existing.provider_capabilities);
        for (alias, flow) in existing.data_flows {
            approved_data_flows.entry(alias).or_insert(flow);
        }
    }
    let decision = GrantDecisionResource {
        generation: expected_generation.saturating_add(1),
        reviewed: true,
        experience_id: package.experience_id,
        provider_capabilities,
        data_flows: approved_data_flows,
    };
    match call(
        &client,
        &ServiceRequest::UpdateGrantDecision {
            request_id: 25,
            expected_generation,
            capability: grant_capability.into(),
            decision: decision.clone(),
        },
    )? {
        ResponsePayload::GrantDecisionUpdated { value } if value == decision => Ok(value),
        _ => bail!("grant authority returned the wrong review payload"),
    }
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

pub fn shutdown_authority(service_socket: &Path, timeout: Duration) -> Result<()> {
    let client = ServiceClient::new(service_socket, timeout);
    match call(&client, &ServiceRequest::Shutdown { request_id: 90 })? {
        ResponsePayload::Shutdown => Ok(()),
        _ => bail!("provider authority returned the wrong shutdown payload"),
    }
}

fn load_revision_state(revision: &VerifiedRevision) -> Result<DurableState> {
    let path = revision.directory.join(&revision.manifest.state.path);
    let durable: DurableState = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("read revision state {}", path.display()))?,
    )
    .with_context(|| format!("decode revision state {}", path.display()))?;
    Ok(durable)
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
