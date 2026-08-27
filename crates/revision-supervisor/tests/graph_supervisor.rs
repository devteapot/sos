use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    thread,
    time::Duration,
};

use experience_package::{
    DependencyAlias, DependencyPolicy, DerivationKind, DerivationRecord, ExperienceContract,
    ExperienceExport, ExperienceId, ExperienceRole, ExportId, GraphNodeId, PackageMetadata,
    ResolvedGraph, ResolvedGraphNode, RevisionId, StateMigrationRecord, StateMigrationSource,
    ValueSchema, ViewportContract, APPEARANCE_ABI_VERSION, CONTRACT_VERSION, GRAPH_FORMAT_VERSION,
    PACKAGE_FORMAT_VERSION,
};
use revision_supervisor::{
    install_reference_composition, DurableState, Error, ExperienceGraphSupervisor,
    ExperienceRegistry, GraphActivationFaultPoint, GraphResolver, GraphStore, HostCommand,
    RevisionInput, RevisionPackageInput, RevisionStore,
};
use serde_json::json;
use service_protocol::{
    DataFlowGrant, GrantDecisionResource, GraphExperiencePromotion, GraphPromotionDraft,
    ResourceQuery, ResourceValue, ResponsePayload, ServiceRequest, TransactionStatus,
};
use tempfile::TempDir;

fn host_executable() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_supervisor-test-host")
        .map(Into::into)
        .expect("Cargo exposes the test host binary")
}

fn package() -> PackageMetadata {
    PackageMetadata {
        format_version: PACKAGE_FORMAT_VERSION,
        experience_id: ExperienceId::parse("dashboard").unwrap(),
        role: ExperienceRole::Ordinary,
        provider_capabilities: Default::default(),
        contract: ExperienceContract {
            contract_version: CONTRACT_VERSION,
            exports: BTreeMap::from([(
                ExportId::parse("main").unwrap(),
                ExperienceExport {
                    properties: ValueSchema::empty_record(),
                    events: BTreeMap::new(),
                    viewport: ViewportContract {
                        min_width: 100,
                        min_height: 100,
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
    }
}

fn install(store: &RevisionStore, source: &str) -> String {
    store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: source.as_bytes().to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package: package(),
        })
        .unwrap()
        .manifest
        .revision_id
}

fn graph(revision_id: &str) -> ResolvedGraph {
    graph_for(&ExperienceId::parse("dashboard").unwrap(), revision_id)
}

fn graph_for(experience_id: &ExperienceId, revision_id: &str) -> ResolvedGraph {
    let root = GraphNodeId::parse("root").unwrap();
    ResolvedGraph {
        format_version: GRAPH_FORMAT_VERSION,
        root: root.clone(),
        nodes: BTreeMap::from([(
            root,
            ResolvedGraphNode {
                experience_id: experience_id.clone(),
                revision_id: RevisionId::parse(revision_id).unwrap(),
                export_id: ExportId::parse("main").unwrap(),
                parent: None,
                dependency: None,
            },
        )]),
    }
}

#[test]
fn registry_shell_lifecycle_request_presents_an_independent_experience() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let shell_id = ExperienceId::parse("sos.stock.shell").unwrap();
    let application_id = ExperienceId::parse("sos.example.notes").unwrap();

    let mut shell_package = package();
    shell_package.experience_id = shell_id.clone();
    shell_package.role = ExperienceRole::Shell;
    let shell_revision = store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: b"shell".to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package: shell_package,
        })
        .unwrap()
        .manifest
        .revision_id;
    registry
        .create(&shell_id, ExperienceRole::Shell, &shell_revision)
        .unwrap();
    let shell_graph = graphs
        .install(&graph_for(&shell_id, &shell_revision))
        .unwrap();
    graphs.set_current(&shell_id, &shell_graph).unwrap();

    let mut application_package = package();
    application_package.experience_id = application_id.clone();
    let application_revision = store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: b"application".to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package: application_package,
        })
        .unwrap()
        .manifest
        .revision_id;
    registry
        .create(
            &application_id,
            ExperienceRole::Ordinary,
            &application_revision,
        )
        .unwrap();
    let application_graph = graphs
        .install(&graph_for(&application_id, &application_revision))
        .unwrap();
    graphs
        .set_current(&application_id, &application_graph)
        .unwrap();

    let command = HostCommand::with_args(
        host_executable(),
        vec![
            "--emit-present-from".into(),
            shell_id.to_string(),
            application_id.to_string(),
        ],
    );
    let mut supervisor =
        ExperienceGraphSupervisor::new(store, registry, graphs, command, Duration::from_secs(2));
    supervisor.boot(&shell_id).unwrap();
    for _ in 0..200 {
        supervisor.poll().unwrap();
        if supervisor.presented_graphs().contains_key(&application_id) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(supervisor.presented_graphs().contains_key(&application_id));
    assert_eq!(supervisor.presented_graphs().len(), 2);
    supervisor.shutdown().unwrap();
}

fn start_authority(
    socket: PathBuf,
    state_file: PathBuf,
) -> thread::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
    let handle = thread::spawn({
        let socket = socket.clone();
        move || {
            provider_state_service::serve_with_writers(
                &socket,
                &state_file,
                None,
                Some("test-grant-review"),
            )
        }
    });
    for _ in 0..200 {
        if socket.exists() {
            return handle;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("provider authority did not create its socket");
}

fn seed_authority(
    client: &provider_state_service::ServiceClient,
    store: &RevisionStore,
    experience_id: &ExperienceId,
    revision_id: &str,
) {
    let revision = store.verify(revision_id).unwrap();
    let durable: DurableState = serde_json::from_slice(
        &fs::read(revision.directory.join(&revision.manifest.state.path)).unwrap(),
    )
    .unwrap();
    let transaction_id = "seed-active-graph".to_owned();
    let draft = GraphPromotionDraft {
        transaction_id: transaction_id.clone(),
        activate: true,
        promotions: vec![GraphExperiencePromotion {
            experience_id: experience_id.clone(),
            expected_revision: 0,
            revision_id: revision_id.into(),
            schema_version: durable.schema_version,
            source_sha256: durable.source_sha256,
            state: durable.state,
            migration: None,
            actions: Vec::new(),
        }],
    };
    client
        .call(&ServiceRequest::StageGraphPromotion {
            request_id: 1,
            draft,
        })
        .unwrap();
    let response = client
        .call(&ServiceRequest::PromoteGraph {
            request_id: 2,
            transaction_id,
        })
        .unwrap();
    assert!(matches!(
        response.payload,
        Some(ResponsePayload::GraphTransaction { record })
            if record.status == TransactionStatus::Committed
    ));
}

fn seed_authority_graphs(
    client: &provider_state_service::ServiceClient,
    store: &RevisionStore,
    graphs: &[ResolvedGraph],
    review_grants: bool,
) {
    let mut revisions = BTreeMap::new();
    for graph in graphs {
        for node in graph.nodes.values() {
            revisions
                .entry(node.experience_id.clone())
                .or_insert_with(|| node.revision_id.to_string());
        }
    }
    let promotions = revisions
        .into_iter()
        .map(|(experience_id, revision_id)| {
            let revision = store.verify(&revision_id).unwrap();
            let durable: DurableState = serde_json::from_slice(
                &fs::read(revision.directory.join(&revision.manifest.state.path)).unwrap(),
            )
            .unwrap();
            GraphExperiencePromotion {
                experience_id,
                expected_revision: 0,
                revision_id,
                schema_version: durable.schema_version,
                source_sha256: durable.source_sha256,
                state: durable.state,
                migration: None,
                actions: Vec::new(),
            }
        })
        .collect();
    let transaction_id = "seed-multi-root-graphs".to_owned();
    client
        .call(&ServiceRequest::StageGraphPromotion {
            request_id: 10,
            draft: GraphPromotionDraft {
                transaction_id: transaction_id.clone(),
                activate: true,
                promotions,
            },
        })
        .unwrap();
    let response = client
        .call(&ServiceRequest::PromoteGraph {
            request_id: 11,
            transaction_id,
        })
        .unwrap();
    assert!(matches!(
        response.payload,
        Some(ResponsePayload::GraphTransaction { record })
            if record.status == TransactionStatus::Committed
    ));
    if !review_grants {
        return;
    }
    let mut reviewed = BTreeSet::new();
    for graph in graphs {
        for node in graph.nodes.values() {
            if !reviewed.insert(node.experience_id.clone()) {
                continue;
            }
            let revision = store.verify(node.revision_id.as_str()).unwrap();
            let package = revision.package.unwrap();
            let data_flows: BTreeMap<DependencyAlias, DataFlowGrant> = package
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
            let response = client
                .call(&ServiceRequest::UpdateGrantDecision {
                    request_id: 14,
                    expected_generation: 0,
                    capability: "test-grant-review".into(),
                    decision: GrantDecisionResource {
                        generation: 1,
                        reviewed: true,
                        experience_id: node.experience_id.clone(),
                        provider_capabilities: package.provider_capabilities,
                        data_flows,
                    },
                })
                .unwrap();
            assert!(response.ok);
        }
    }
}

#[test]
fn package_install_rejects_a_state_migration_result_that_does_not_match_state() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let mut package = package();
    package.state_migration = Some(StateMigrationRecord {
        source: StateMigrationSource::Fresh,
        target_schema_version: 1,
        result_state_sha256: "0".repeat(64),
    });
    assert!(matches!(
        store.install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: b"return { api_version = 4, exports = {} }".to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package,
        }),
        Err(Error::InvalidRevision(message)) if message.contains("migration result")
    ));
}

#[test]
fn authority_commit_and_graph_pointers_recover_as_one_activation() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let first_revision = install(&store, "first");
    let second_revision = install(&store, "second");
    let root = ExperienceId::parse("dashboard").unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    registry
        .create(&root, ExperienceRole::Ordinary, &first_revision)
        .unwrap();
    let graphs = GraphStore::open(directory.path()).unwrap();
    let first_graph = graphs.install(&graph(&first_revision)).unwrap();
    let second_graph = graphs.install(&graph(&second_revision)).unwrap();
    graphs.set_current(&root, &first_graph).unwrap();
    let socket = directory.path().join("authority.sock");
    let service = start_authority(socket.clone(), directory.path().join("authority.json"));
    let client = provider_state_service::ServiceClient::new(&socket, Duration::from_secs(2));
    seed_authority(&client, &store, &root, &first_revision);

    let mut supervisor = ExperienceGraphSupervisor::new(
        store.clone(),
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    )
    .with_authority(client.clone());
    supervisor.boot(&root).unwrap();
    let prepared = supervisor.prepare(&root, &second_graph).unwrap();
    supervisor.configure_fault(Some(GraphActivationFaultPoint::AfterAuthorityCommit));
    assert!(matches!(
        supervisor.commit(prepared),
        Err(Error::InjectedGraphActivationFault(_))
    ));
    assert_eq!(graphs.current(&root).unwrap().unwrap().0, first_graph);
    supervisor.shutdown().unwrap();

    let mut recovered = ExperienceGraphSupervisor::new(
        store,
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    )
    .with_authority(client.clone());
    assert_eq!(
        recovered.recover().unwrap().as_deref(),
        Some(second_graph.as_str())
    );
    assert_eq!(graphs.current(&root).unwrap().unwrap().0, second_graph);
    assert_eq!(
        registry
            .current(&root)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        second_revision
    );
    let state = client
        .call(&ServiceRequest::GetResource {
            request_id: 3,
            query: ResourceQuery::ExperienceStateFor {
                experience_id: root,
            },
        })
        .unwrap();
    assert!(matches!(
        state.payload,
        Some(ResponsePayload::Resource {
            value: ResourceValue::ExperienceStateFor(state),
        }) if state.resource.revision_id == second_revision
    ));
    client
        .call(&ServiceRequest::Shutdown { request_id: 4 })
        .unwrap();
    service.join().unwrap().unwrap();
}

#[test]
fn activation_journal_completes_a_presented_graph_after_registry_commit() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let first_revision = install(&store, "first");
    let second_revision = install(&store, "second");
    let root = ExperienceId::parse("dashboard").unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    registry
        .create(&root, ExperienceRole::Ordinary, &first_revision)
        .unwrap();
    let graphs = GraphStore::open(directory.path()).unwrap();
    let first_graph = graphs.install(&graph(&first_revision)).unwrap();
    let second_graph = graphs.install(&graph(&second_revision)).unwrap();
    graphs.set_current(&root, &first_graph).unwrap();

    let mut supervisor = ExperienceGraphSupervisor::new(
        store.clone(),
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&root).unwrap();
    let prepared = supervisor.prepare(&root, &second_graph).unwrap();
    supervisor.configure_fault(Some(GraphActivationFaultPoint::AfterRegistryCommit));
    assert!(matches!(
        supervisor.commit(prepared),
        Err(Error::InjectedGraphActivationFault(_))
    ));
    assert!(supervisor.journal().unwrap().is_some());
    supervisor.shutdown().unwrap();

    let mut recovered = ExperienceGraphSupervisor::new(
        store,
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    assert_eq!(
        recovered.recover().unwrap().as_deref(),
        Some(second_graph.as_str())
    );
    assert!(recovered.journal().unwrap().is_none());
    assert_eq!(
        registry
            .current(&root)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        second_revision
    );
    assert_eq!(graphs.current(&root).unwrap().unwrap().0, second_graph);
}

#[test]
fn activation_journal_rolls_back_when_only_presentation_completed() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let first_revision = install(&store, "first");
    let second_revision = install(&store, "second");
    let root = ExperienceId::parse("dashboard").unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    registry
        .create(&root, ExperienceRole::Ordinary, &first_revision)
        .unwrap();
    let graphs = GraphStore::open(directory.path()).unwrap();
    let first_graph = graphs.install(&graph(&first_revision)).unwrap();
    let second_graph = graphs.install(&graph(&second_revision)).unwrap();
    graphs.set_current(&root, &first_graph).unwrap();
    let mut supervisor = ExperienceGraphSupervisor::new(
        store.clone(),
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&root).unwrap();
    let prepared = supervisor.prepare(&root, &second_graph).unwrap();
    supervisor.configure_fault(Some(GraphActivationFaultPoint::AfterPresented));
    assert!(supervisor.commit(prepared).is_err());
    supervisor.shutdown().unwrap();

    let mut recovered = ExperienceGraphSupervisor::new(
        store,
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    assert_eq!(
        recovered.recover().unwrap().as_deref(),
        Some(first_graph.as_str())
    );
    assert_eq!(
        registry
            .current(&root)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        first_revision
    );
    assert_eq!(graphs.current(&root).unwrap().unwrap().0, first_graph);
}

#[test]
fn activation_journal_recovers_an_atomic_graph_at_every_durable_phase() {
    let cases = [
        (GraphActivationFaultPoint::AfterIntent, false),
        (GraphActivationFaultPoint::AfterPresented, false),
        (GraphActivationFaultPoint::AfterAuthorityCommit, true),
        (GraphActivationFaultPoint::AfterRegistryCommit, true),
        (GraphActivationFaultPoint::AfterGraphCommit, true),
    ];

    for (fault, candidate_wins) in cases {
        let directory = TempDir::new().unwrap();
        let store = RevisionStore::open(directory.path()).unwrap();
        let first_revision = install(&store, "first");
        let second_revision = install(&store, "second");
        let root = ExperienceId::parse("dashboard").unwrap();
        let registry = ExperienceRegistry::open(store.clone()).unwrap();
        registry
            .create(&root, ExperienceRole::Ordinary, &first_revision)
            .unwrap();
        let graphs = GraphStore::open(directory.path()).unwrap();
        let first_graph = graphs.install(&graph(&first_revision)).unwrap();
        let second_graph = graphs.install(&graph(&second_revision)).unwrap();
        graphs.set_current(&root, &first_graph).unwrap();

        let mut supervisor = ExperienceGraphSupervisor::new(
            store.clone(),
            registry.clone(),
            graphs.clone(),
            HostCommand::new(host_executable()),
            Duration::from_secs(2),
        );
        supervisor.boot(&root).unwrap();
        let prepared = supervisor.prepare(&root, &second_graph).unwrap();
        supervisor.configure_fault(Some(fault));
        assert!(matches!(
            supervisor.commit(prepared),
            Err(Error::InjectedGraphActivationFault(_))
        ));
        assert!(supervisor.journal().unwrap().is_some());
        drop(supervisor);

        let mut recovered = ExperienceGraphSupervisor::new(
            store,
            registry.clone(),
            graphs.clone(),
            HostCommand::new(host_executable()),
            Duration::from_secs(2),
        );
        let expected_graph = if candidate_wins {
            &second_graph
        } else {
            &first_graph
        };
        let expected_revision = if candidate_wins {
            &second_revision
        } else {
            &first_revision
        };
        assert_eq!(
            recovered.recover().unwrap().as_deref(),
            Some(expected_graph.as_str()),
            "wrong recovery result at {fault:?}"
        );
        assert!(recovered.journal().unwrap().is_none());
        assert_eq!(
            graphs.current(&root).unwrap().unwrap().0,
            *expected_graph,
            "graph pointer split at {fault:?}"
        );
        assert_eq!(
            registry
                .current(&root)
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            *expected_revision,
            "registry pointer split at {fault:?}"
        );
    }
}

#[test]
fn tracked_child_update_activates_the_complete_graph_and_restarts_exactly() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let root = ExperienceId::parse("sos.example.dashboard").unwrap();
    let agenda_id = ExperienceId::parse("sos.example.agenda").unwrap();
    let main = ExportId::parse("main").unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(directory.path()).unwrap();

    let old_agenda = store.verify(&reference.agenda_revision).unwrap();
    let agenda_package = old_agenda.package.unwrap();
    let agenda_source =
        fs::read_to_string(old_agenda.directory.join(&old_agenda.manifest.source.path)).unwrap();
    let new_agenda = store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: format!("{agenda_source}\n-- compatible tracked update\n").into_bytes(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package: agenda_package,
        })
        .unwrap()
        .manifest
        .revision_id;
    let old_dashboard = store.verify(&reference.dashboard_revision).unwrap();
    let mut dashboard_package = old_dashboard.package.unwrap();
    dashboard_package
        .dependencies
        .get_mut(&DependencyAlias::parse("agenda").unwrap())
        .unwrap()
        .policy = DependencyPolicy::Tracked;
    let dashboard_source = fs::read(
        old_dashboard
            .directory
            .join(&old_dashboard.manifest.source.path),
    )
    .unwrap();
    let tracked_dashboard = store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: dashboard_source,
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package: dashboard_package,
        })
        .unwrap()
        .manifest
        .revision_id;

    let resolver = GraphResolver::new(store.clone());
    let initial_tracked = resolver
        .resolve_tracked(&tracked_dashboard, &main, &registry)
        .unwrap();
    assert!(initial_tracked.nodes.values().any(|node| {
        node.experience_id == agenda_id && node.revision_id.as_str() == reference.agenda_revision
    }));
    let initial_tracked_graph = graphs.install(&initial_tracked).unwrap();

    let mut supervisor = ExperienceGraphSupervisor::new(
        store.clone(),
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&root).unwrap();
    let prepared = supervisor.prepare(&root, &initial_tracked_graph).unwrap();
    supervisor.commit(prepared).unwrap();
    let activated = supervisor
        .advance_experience(&agenda_id, &new_agenda)
        .unwrap();
    let tracked_graph = activated.graph_updates[0].graph_id.clone();
    assert_eq!(supervisor.active_graph(), Some(tracked_graph.as_str()));
    assert_eq!(graphs.current(&root).unwrap().unwrap().0, tracked_graph);
    assert_eq!(
        registry
            .current(&agenda_id)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        new_agenda
    );
    assert!(graphs
        .current(&root)
        .unwrap()
        .unwrap()
        .1
        .nodes
        .values()
        .any(|node| {
            node.experience_id == agenda_id && node.revision_id.as_str() == new_agenda
        }));
    assert_eq!(
        registry
            .current(&root)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        tracked_dashboard
    );
    supervisor.shutdown().unwrap();

    let mut restarted = ExperienceGraphSupervisor::new(
        store,
        registry,
        graphs,
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    restarted.boot(&root).unwrap();
    assert_eq!(restarted.active_graph(), Some(tracked_graph.as_str()));
    restarted.shutdown().unwrap();
}

#[test]
fn locked_child_update_advances_only_the_child_registry_pointer() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let root = ExperienceId::parse("sos.example.dashboard").unwrap();
    let agenda_id = ExperienceId::parse("sos.example.agenda").unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(directory.path()).unwrap();
    let old_graph = graphs.current(&root).unwrap().unwrap().0;
    let old_agenda = store.verify(&reference.agenda_revision).unwrap();
    let agenda_package = old_agenda.package.unwrap();
    let agenda_source =
        fs::read_to_string(old_agenda.directory.join(&old_agenda.manifest.source.path)).unwrap();
    let new_agenda = store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: format!("{agenda_source}\n-- locked child update\n").into_bytes(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package: agenda_package,
        })
        .unwrap()
        .manifest
        .revision_id;

    let mut supervisor = ExperienceGraphSupervisor::new(
        store,
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&root).unwrap();
    assert!(supervisor
        .advance_experience(&agenda_id, &new_agenda)
        .unwrap()
        .graph_updates
        .is_empty());
    assert_eq!(graphs.current(&root).unwrap().unwrap().0, old_graph);
    assert_eq!(
        registry
            .current(&agenda_id)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        new_agenda
    );
    assert!(graphs
        .current(&root)
        .unwrap()
        .unwrap()
        .1
        .nodes
        .values()
        .any(|node| {
            node.experience_id == agenda_id
                && node.revision_id.as_str() == reference.agenda_revision
        }));
    supervisor.shutdown().unwrap();
}

fn install_tracked_roots(
    store: &RevisionStore,
    reference: &revision_supervisor::ReferenceComposition,
    registry: &ExperienceRegistry,
    graphs: &GraphStore,
) -> (ExperienceId, ExperienceId, String, String) {
    let installed =
        install_tracked_roots_named(store, reference, registry, graphs, &["one", "two"]);
    (
        installed[0].0.clone(),
        installed[1].0.clone(),
        installed[0].1.clone(),
        installed[1].1.clone(),
    )
}

fn install_tracked_roots_named(
    store: &RevisionStore,
    reference: &revision_supervisor::ReferenceComposition,
    registry: &ExperienceRegistry,
    graphs: &GraphStore,
    suffixes: &[&str],
) -> Vec<(ExperienceId, String)> {
    let dashboard = store.verify(&reference.dashboard_revision).unwrap();
    let source = fs::read(dashboard.directory.join(&dashboard.manifest.source.path)).unwrap();
    let base = dashboard.package.unwrap();
    let main = ExportId::parse("main").unwrap();
    let mut installed = Vec::new();
    for suffix in suffixes {
        let experience_id = ExperienceId::parse(format!("sos.example.dashboard.{suffix}")).unwrap();
        let mut package = base.clone();
        package.experience_id = experience_id.clone();
        package
            .dependencies
            .get_mut(&DependencyAlias::parse("agenda").unwrap())
            .unwrap()
            .policy = DependencyPolicy::Tracked;
        let revision_id = store
            .install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: source.clone(),
                    state: json!({}),
                    schema_version: 1,
                    experience_api_version: 4,
                    assets: Vec::new(),
                },
                package,
            })
            .unwrap()
            .manifest
            .revision_id;
        registry
            .create(&experience_id, ExperienceRole::Ordinary, &revision_id)
            .unwrap();
        let graph = GraphResolver::new(store.clone())
            .resolve_tracked(&revision_id, &main, registry)
            .unwrap();
        let graph_id = graphs.install(&graph).unwrap();
        graphs.set_current(&experience_id, &graph_id).unwrap();
        installed.push((experience_id, graph_id));
    }
    installed
}

fn install_agenda_update(store: &RevisionStore, revision_id: &str, marker: &str) -> String {
    let agenda = store.verify(revision_id).unwrap();
    let package = agenda.package.unwrap();
    let source = fs::read(agenda.directory.join(&agenda.manifest.source.path)).unwrap();
    store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: [source, format!("\n-- {marker}\n").into_bytes()].concat(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: Vec::new(),
            },
            package,
        })
        .unwrap()
        .manifest
        .revision_id
}

#[test]
fn tracked_update_presents_every_affected_root_as_one_set() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let (first, second, first_graph, second_graph) =
        install_tracked_roots(&store, &reference, &registry, &graphs);
    let agenda_id = ExperienceId::parse("sos.example.agenda").unwrap();
    let candidate = install_agenda_update(&store, &reference.agenda_revision, "multi-root update");
    let mut supervisor = ExperienceGraphSupervisor::new(
        store,
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&first).unwrap();
    supervisor.boot(&second).unwrap();
    let activated = supervisor
        .advance_experience(&agenda_id, &candidate)
        .unwrap();
    assert_eq!(activated.graph_updates.len(), 2);
    assert!(activated
        .graph_updates
        .iter()
        .all(|update| update.host_pid.is_some()));
    assert_ne!(graphs.current(&first).unwrap().unwrap().0, first_graph);
    assert_ne!(graphs.current(&second).unwrap().unwrap().0, second_graph);
    for root in [&first, &second] {
        assert!(graphs
            .current(root)
            .unwrap()
            .unwrap()
            .1
            .nodes
            .values()
            .any(|node| {
                node.experience_id == agenda_id && node.revision_id.as_str() == candidate
            }));
    }
    assert_eq!(
        registry
            .current(&agenda_id)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        candidate
    );
    supervisor.shutdown().unwrap();
}

#[test]
fn tracked_update_advances_presented_and_inactive_roots_atomically() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let (presented, inactive, presented_graph, inactive_graph) =
        install_tracked_roots(&store, &reference, &registry, &graphs);
    let agenda_id = ExperienceId::parse("sos.example.agenda").unwrap();
    let candidate = install_agenda_update(&store, &reference.agenda_revision, "inactive root");
    let mut supervisor = ExperienceGraphSupervisor::new(
        store,
        registry,
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&presented).unwrap();
    let advanced = supervisor
        .advance_experience(&agenda_id, &candidate)
        .unwrap();
    assert_eq!(advanced.graph_updates.len(), 2);
    assert_eq!(
        advanced
            .graph_updates
            .iter()
            .filter(|update| update.host_pid.is_some())
            .count(),
        1
    );
    assert!(advanced
        .graph_updates
        .iter()
        .any(|update| { update.root_experience_id == inactive && update.host_pid.is_none() }));
    assert_ne!(
        graphs.current(&presented).unwrap().unwrap().0,
        presented_graph
    );
    assert_ne!(
        graphs.current(&inactive).unwrap().unwrap().0,
        inactive_graph
    );
    supervisor.shutdown().unwrap();
}

#[test]
fn top_level_presentation_enforces_the_aggregate_instance_budget() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let installed = install_tracked_roots_named(
        &store,
        &reference,
        &registry,
        &graphs,
        &["budget-one", "budget-two", "budget-three"],
    );
    let mut supervisor = ExperienceGraphSupervisor::new(
        store,
        registry,
        graphs,
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&installed[0].0).unwrap();
    supervisor.boot(&installed[1].0).unwrap();
    assert!(matches!(
        supervisor.boot(&installed[2].0),
        Err(Error::InvalidGraph(message)) if message.contains("limit is 8")
    ));
    assert_eq!(supervisor.presented_graphs().len(), 2);
    assert!(supervisor.dismiss(&installed[1].0).unwrap().is_some());
    assert!(supervisor.boot(&installed[2].0).unwrap().is_some());
    assert_eq!(supervisor.presented_graphs().len(), 2);
    supervisor.shutdown().unwrap();
}

#[test]
fn authority_commits_one_state_promotion_for_a_multi_root_update() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let (first, second, _, _) = install_tracked_roots(&store, &reference, &registry, &graphs);
    let first_resolved = graphs.current(&first).unwrap().unwrap().1;
    let second_resolved = graphs.current(&second).unwrap().unwrap().1;
    let socket = directory.path().join("authority-multi.sock");
    let service = start_authority(
        socket.clone(),
        directory.path().join("authority-multi.json"),
    );
    let client = provider_state_service::ServiceClient::new(&socket, Duration::from_secs(2));
    seed_authority_graphs(&client, &store, &[first_resolved, second_resolved], true);
    let agenda_id = ExperienceId::parse("sos.example.agenda").unwrap();
    let candidate = install_agenda_update(&store, &reference.agenda_revision, "authority set");
    let mut supervisor = ExperienceGraphSupervisor::new(
        store,
        registry,
        graphs,
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    )
    .with_authority(client.clone());
    supervisor.boot(&first).unwrap();
    supervisor.boot(&second).unwrap();
    let advanced = supervisor
        .advance_experience(&agenda_id, &candidate)
        .unwrap();
    assert_eq!(advanced.graph_updates.len(), 2);
    let state = client
        .call(&ServiceRequest::GetResource {
            request_id: 12,
            query: ResourceQuery::ExperienceStateFor {
                experience_id: agenda_id,
            },
        })
        .unwrap();
    assert!(matches!(
        state.payload,
        Some(ResponsePayload::Resource {
            value: ResourceValue::ExperienceStateFor(state),
        }) if state.resource.revision_id == candidate
    ));
    supervisor.shutdown().unwrap();
    client
        .call(&ServiceRequest::Shutdown { request_id: 13 })
        .unwrap();
    service.join().unwrap().unwrap();
}

#[test]
fn graph_boot_rejects_unreviewed_cross_experience_data_flows() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let (root, _, _, _) = install_tracked_roots(&store, &reference, &registry, &graphs);
    let resolved = graphs.current(&root).unwrap().unwrap().1;
    let socket = directory.path().join("authority-unreviewed.sock");
    let service = start_authority(
        socket.clone(),
        directory.path().join("authority-unreviewed.json"),
    );
    let client = provider_state_service::ServiceClient::new(&socket, Duration::from_secs(2));
    seed_authority_graphs(&client, &store, &[resolved], false);
    let mut supervisor = ExperienceGraphSupervisor::new(
        store,
        registry,
        graphs,
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    )
    .with_authority(client.clone());
    assert!(matches!(
        supervisor.boot(&root),
        Err(Error::InvalidGraph(_))
    ));
    client
        .call(&ServiceRequest::Shutdown { request_id: 15 })
        .unwrap();
    service.join().unwrap().unwrap();
}

#[test]
fn stable_experience_grant_authorizes_a_later_revision_with_the_same_boundary() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let installed =
        install_tracked_roots_named(&store, &reference, &registry, &graphs, &["stable-grant"]);
    let root = installed[0].0.clone();
    let current_graph = graphs.current(&root).unwrap().unwrap().1;
    let current_root = &current_graph.nodes[&current_graph.root];
    let current = store.verify(current_root.revision_id.as_str()).unwrap();
    let package = current.package.unwrap();
    let source = fs::read(current.directory.join(&current.manifest.source.path)).unwrap();

    let socket = directory.path().join("authority-stable-grant.sock");
    let service = start_authority(
        socket.clone(),
        directory.path().join("authority-stable-grant.json"),
    );
    let client = provider_state_service::ServiceClient::new(&socket, Duration::from_secs(2));
    seed_authority_graphs(&client, &store, &[current_graph], true);

    let replacement = store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: [source, b"\n-- stable grant replacement\n".to_vec()].concat(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package,
        })
        .unwrap()
        .manifest
        .revision_id;
    let replacement_graph = GraphResolver::new(store.clone())
        .resolve_tracked(&replacement, &ExportId::parse("main").unwrap(), &registry)
        .unwrap();
    let replacement_graph_id = graphs.install(&replacement_graph).unwrap();
    let mut supervisor = ExperienceGraphSupervisor::new(
        store,
        registry,
        graphs,
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    )
    .with_authority(client.clone());
    supervisor.boot(&root).unwrap();
    let prepared = supervisor.prepare(&root, &replacement_graph_id).unwrap();
    supervisor.discard(prepared).unwrap();
    supervisor.shutdown().unwrap();
    client
        .call(&ServiceRequest::Shutdown { request_id: 16 })
        .unwrap();
    service.join().unwrap().unwrap();
}

#[test]
fn multi_root_presentation_fault_recovers_every_old_graph() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let reference = install_reference_composition(&store).unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let graphs = GraphStore::open(store.root()).unwrap();
    let (first, second, first_graph, second_graph) =
        install_tracked_roots(&store, &reference, &registry, &graphs);
    let agenda_id = ExperienceId::parse("sos.example.agenda").unwrap();
    let candidate = install_agenda_update(&store, &reference.agenda_revision, "faulted set");
    let mut supervisor = ExperienceGraphSupervisor::new(
        store.clone(),
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&first).unwrap();
    supervisor.boot(&second).unwrap();
    let prepared = supervisor
        .prepare_tracked_update_set(&agenda_id, &candidate)
        .unwrap();
    supervisor.configure_fault(Some(GraphActivationFaultPoint::AfterPresented));
    assert!(matches!(
        supervisor.commit_set(prepared),
        Err(Error::InjectedGraphActivationFault(_))
    ));
    drop(supervisor);

    let mut recovered = ExperienceGraphSupervisor::new(
        store,
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    assert_eq!(
        recovered.recover().unwrap().as_deref(),
        Some(first_graph.as_str())
    );
    assert_eq!(graphs.current(&first).unwrap().unwrap().0, first_graph);
    assert_eq!(graphs.current(&second).unwrap().unwrap().0, second_graph);
    assert_eq!(
        registry
            .current(&agenda_id)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id,
        reference.agenda_revision
    );
}
