use std::{collections::BTreeMap, fs, path::PathBuf, thread, time::Duration};

use experience_package::{
    DependencyAlias, DependencyPolicy, DerivationKind, DerivationRecord, ExperienceContract,
    ExperienceExport, ExperienceId, ExperienceRole, ExportId, GraphNodeId, PackageMetadata,
    ResolvedGraph, ResolvedGraphNode, RevisionId, ValueSchema, ViewportContract,
    APPEARANCE_ABI_VERSION, CONTRACT_VERSION, GRAPH_FORMAT_VERSION, PACKAGE_FORMAT_VERSION,
};
use revision_supervisor::{
    install_reference_composition, DurableState, Error, ExperienceGraphSupervisor,
    ExperienceRegistry, GraphActivationFaultPoint, GraphResolver, GraphStore, HostCommand,
    RevisionInput, RevisionPackageInput, RevisionStore,
};
use serde_json::json;
use service_protocol::{
    GraphExperiencePromotion, GraphPromotionDraft, ResourceQuery, ResourceValue, ResponsePayload,
    ServiceRequest, TransactionStatus,
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
    let root = GraphNodeId::parse("root").unwrap();
    ResolvedGraph {
        format_version: GRAPH_FORMAT_VERSION,
        root: root.clone(),
        nodes: BTreeMap::from([(
            root,
            ResolvedGraphNode {
                experience_id: ExperienceId::parse("dashboard").unwrap(),
                revision_id: RevisionId::parse(revision_id).unwrap(),
                export_id: ExportId::parse("main").unwrap(),
                parent: None,
                dependency: None,
            },
        )]),
    }
}

fn start_authority(
    socket: PathBuf,
    state_file: PathBuf,
) -> thread::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
    let handle = thread::spawn({
        let socket = socket.clone();
        move || provider_state_service::serve(&socket, &state_file)
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
fn tracked_refresh_activates_the_complete_graph_and_restarts_exactly() {
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
    registry.set_current(&agenda_id, &new_agenda).unwrap();

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
    let locked = resolver
        .resolve_tracked(&reference.dashboard_revision, &main, &registry)
        .unwrap();
    assert!(locked.nodes.values().any(|node| {
        node.experience_id == agenda_id && node.revision_id.as_str() == reference.agenda_revision
    }));
    let tracked = resolver
        .resolve_tracked(&tracked_dashboard, &main, &registry)
        .unwrap();
    assert!(tracked.nodes.values().any(|node| {
        node.experience_id == agenda_id && node.revision_id.as_str() == new_agenda
    }));
    let tracked_graph = graphs.install(&tracked).unwrap();

    let mut supervisor = ExperienceGraphSupervisor::new(
        store.clone(),
        registry.clone(),
        graphs.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&root).unwrap();
    let prepared = supervisor.prepare(&root, &tracked_graph).unwrap();
    supervisor.commit(prepared).unwrap();
    assert_eq!(supervisor.active_graph(), Some(tracked_graph.as_str()));
    assert_eq!(graphs.current(&root).unwrap().unwrap().0, tracked_graph);
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
