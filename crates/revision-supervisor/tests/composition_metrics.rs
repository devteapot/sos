use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};

use experience_package::{ColorScheme, ExperienceId, ExportId, RevisionId};
use revision_supervisor::{
    install_reference_composition, DurableState, ExperienceGraphSupervisor, ExperienceRegistry,
    GraphActivationFaultPoint, GraphResolver, GraphStore, HostCommand, RevisionInput,
    RevisionPackageInput, RevisionStore,
};
use runtime_luau::{GraphRevisionInput, GraphRuntime, RuntimeInstanceStatus};
use serde_json::json;
use tempfile::TempDir;

fn host_executable() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_supervisor-test-host")
        .map(Into::into)
        .expect("Cargo exposes the test host binary")
}

fn runtime_inputs(
    store: &RevisionStore,
    graph: &experience_package::ResolvedGraph,
) -> BTreeMap<RevisionId, GraphRevisionInput> {
    let model = providers_fake::snapshot();
    graph
        .nodes
        .values()
        .map(|node| node.revision_id.clone())
        .map(|revision_id| {
            let revision = store.verify(revision_id.as_str()).unwrap();
            let durable: DurableState = serde_json::from_slice(
                &fs::read(revision.directory.join(&revision.manifest.state.path)).unwrap(),
            )
            .unwrap();
            (
                revision_id,
                GraphRevisionInput {
                    source: fs::read_to_string(
                        revision.directory.join(&revision.manifest.source.path),
                    )
                    .unwrap(),
                    sidecars: vec![],
                    model: model.clone(),
                    state: durable.state,
                    state_schema_version: durable.schema_version,
                    package: revision.package,
                },
            )
        })
        .collect()
}

fn install_dashboard_candidate(
    store: &RevisionStore,
    parent_revision: &str,
    sequence: u8,
) -> String {
    let parent = store.verify(parent_revision).unwrap();
    let durable: DurableState = serde_json::from_slice(
        &fs::read(parent.directory.join(&parent.manifest.state.path)).unwrap(),
    )
    .unwrap();
    let mut source = fs::read(parent.directory.join(&parent.manifest.source.path)).unwrap();
    source.extend_from_slice(format!("\n-- desktop metrics candidate {sequence}\n").as_bytes());
    store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source,
                state: durable.state,
                schema_version: durable.schema_version,
                experience_api_version: 4,
                assets: vec![],
            },
            package: parent.package,
        })
        .unwrap()
        .manifest
        .revision_id
}

fn resident_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmRSS:")?.trim();
        value.split_whitespace().next()?.parse().ok()
    })
}

#[test]
#[ignore = "explicit desktop performance evidence campaign"]
fn measure_reference_composition_desktop() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let install_started = Instant::now();
    let reference = install_reference_composition(&store).unwrap();
    let graph_store = GraphStore::open(directory.path()).unwrap();
    let graph = graph_store.verify(&reference.dashboard_graph).unwrap();
    let package_resolve_us = install_started.elapsed().as_micros();
    let inputs = runtime_inputs(&store, &graph);
    let rss_before_runtime_kib = resident_kib();

    let runtime_started = Instant::now();
    let mut runtime = GraphRuntime::start(graph, inputs).unwrap();
    let graph_cold_start_to_all_mounts_ready_us = runtime_started.elapsed().as_micros();
    let initial = runtime.snapshot();
    assert_eq!(initial.instances.len(), 3);
    assert!(initial.instances.values().all(|instance| instance.status
        == RuntimeInstanceStatus::Ready
        && instance.scene.is_some()));
    let rss_after_runtime_kib = resident_kib();
    let rss_runtime_delta_kib = rss_after_runtime_kib
        .zip(rss_before_runtime_kib)
        .map(|(after, before)| after.saturating_sub(before));
    let rss_delta_per_instance_kib =
        rss_runtime_delta_kib.map(|delta| delta / u64::try_from(initial.instances.len()).unwrap());

    let agenda = initial
        .instances
        .iter()
        .find(|(_, instance)| instance.experience_id.as_str() == "sos.example.agenda")
        .map(|(node_id, _)| node_id.clone())
        .unwrap();
    let event_started = Instant::now();
    let event = runtime
        .dispatch_event(
            &agenda,
            &json!({"action":"open_first", "target":"agenda-open"}),
        )
        .unwrap();
    let child_event_to_composed_snapshot_us = event_started.elapsed().as_micros();
    assert_eq!(
        event.snapshot.instances[&event.snapshot.root].state["opened"],
        "Design review"
    );

    let mut appearance = providers_fake::snapshot().appearance;
    appearance.generation = 1;
    appearance.scheme = ColorScheme::Light;
    let appearance_started = Instant::now();
    let themed = runtime.apply_appearance(appearance).unwrap();
    let appearance_to_composed_snapshot_us = appearance_started.elapsed().as_micros();
    assert_eq!(themed.instances.len(), 3);

    let root = ExperienceId::parse("sos.example.dashboard").unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    let candidate_one = install_dashboard_candidate(&store, &reference.dashboard_revision, 1);
    let candidate_one_graph = graph_store
        .install(
            &GraphResolver::new(store.clone())
                .resolve(&candidate_one, &ExportId::parse("main").unwrap())
                .unwrap(),
        )
        .unwrap();
    let mut supervisor = ExperienceGraphSupervisor::new(
        store.clone(),
        registry.clone(),
        graph_store.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    supervisor.boot(&root).unwrap();
    let activation_started = Instant::now();
    let prepared = supervisor.prepare(&root, &candidate_one_graph).unwrap();
    supervisor.commit(prepared).unwrap();
    let graph_prepare_present_commit_us = activation_started.elapsed().as_micros();

    let candidate_two = install_dashboard_candidate(&store, &candidate_one, 2);
    let candidate_two_graph = graph_store
        .install(
            &GraphResolver::new(store.clone())
                .resolve(&candidate_two, &ExportId::parse("main").unwrap())
                .unwrap(),
        )
        .unwrap();
    let prepared = supervisor.prepare(&root, &candidate_two_graph).unwrap();
    supervisor.configure_fault(Some(GraphActivationFaultPoint::AfterRegistryCommit));
    assert!(supervisor.commit(prepared).is_err());
    drop(supervisor);

    let mut recovered = ExperienceGraphSupervisor::new(
        store,
        registry,
        graph_store.clone(),
        HostCommand::new(host_executable()),
        Duration::from_secs(2),
    );
    let recovery_started = Instant::now();
    assert_eq!(
        recovered.recover().unwrap().as_deref(),
        Some(candidate_two_graph.as_str())
    );
    let committed_graph_recovery_us = recovery_started.elapsed().as_micros();
    assert_eq!(
        graph_store.current(&root).unwrap().unwrap().0,
        candidate_two_graph
    );

    println!(
        "sos_composition_desktop_metrics={}",
        serde_json::to_string(&json!({
            "release_profile": !cfg!(debug_assertions),
            "instances": initial.instances.len(),
            "package_install_resolve_us": package_resolve_us,
            "graph_cold_start_to_all_mounts_ready_us": graph_cold_start_to_all_mounts_ready_us,
            "child_event_to_composed_snapshot_us": child_event_to_composed_snapshot_us,
            "appearance_to_composed_snapshot_us": appearance_to_composed_snapshot_us,
            "graph_prepare_present_commit_us": graph_prepare_present_commit_us,
            "committed_graph_recovery_us": committed_graph_recovery_us,
            "process_rss_before_runtime_kib": rss_before_runtime_kib,
            "process_rss_after_runtime_kib": rss_after_runtime_kib,
            "runtime_rss_delta_kib": rss_runtime_delta_kib,
            "runtime_rss_delta_per_instance_kib": rss_delta_per_instance_kib,
        }))
        .unwrap()
    );
}
