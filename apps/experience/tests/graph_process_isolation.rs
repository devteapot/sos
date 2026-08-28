#![cfg(all(target_os = "linux", feature = "linux-host"))]

use std::{collections::BTreeMap, path::Path};

use experience_package::{
    DerivationKind, DerivationRecord, ExperienceContract, ExperienceExport, ExperienceId,
    ExperienceRole, ExportId, GraphNodeId, PackageMetadata, ResolvedGraph, ResolvedGraphNode,
    RevisionId, ValueSchema, ViewportContract, APPEARANCE_ABI_VERSION, CONTRACT_VERSION,
    GRAPH_FORMAT_VERSION, PACKAGE_FORMAT_VERSION,
};
use runtime_luau::{GraphRevisionInput, GraphRuntimeWorker, GraphWorkerResult, RevisionAssetInput};
use serde_json::json;

#[test]
fn graph_runtime_can_execute_behind_a_process_boundary() {
    let experience_id = ExperienceId::parse("process-demo").unwrap();
    let export_id = ExportId::parse("main").unwrap();
    let revision_id = RevisionId::parse("d".repeat(64)).unwrap();
    let node_id = GraphNodeId::parse("root").unwrap();
    let package = PackageMetadata {
        format_version: PACKAGE_FORMAT_VERSION,
        experience_id: experience_id.clone(),
        role: ExperienceRole::Ordinary,
        provider_capabilities: Default::default(),
        contract: ExperienceContract {
            contract_version: CONTRACT_VERSION,
            exports: BTreeMap::from([(
                export_id.clone(),
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
    };
    let graph = ResolvedGraph {
        format_version: GRAPH_FORMAT_VERSION,
        root: node_id.clone(),
        nodes: BTreeMap::from([(
            node_id.clone(),
            ResolvedGraphNode {
                experience_id,
                revision_id: revision_id.clone(),
                export_id,
                parent: None,
                dependency: None,
            },
        )]),
    };
    let inputs = BTreeMap::from([(
        revision_id,
        GraphRevisionInput {
            source: r#"
                return { api_version = 4, exports = { main = {
                    render = function(_, state)
                        return { id = "count", content = {
                            kind = "text", value = tostring(state.count or 0),
                            size = 16, color = 0xffffff,
                        } }
                    end,
                    update = function(_, state, event)
                        if event.action == "increment" then
                            state.count = (state.count or 0) + 1
                        end
                        return { state = state }
                    end,
                } } }
            "#
            .into(),
            sidecars: vec![RevisionAssetInput {
                id: "badge".into(),
                kind: "svg".into(),
                bytes: br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><path d="M0 0h2v2H0z"/></svg>"#.to_vec(),
            }],
            model: providers_fake::snapshot(),
            state: json!({}),
            state_schema_version: 1,
            package,
        },
    )]);

    let executable = Path::new(env!("CARGO_BIN_EXE_sos-experience-host"));
    let (worker, initial) = GraphRuntimeWorker::start_process(executable, graph, inputs).unwrap();
    let process_id = worker.process_id().unwrap();
    assert_ne!(process_id, std::process::id());
    assert_eq!(initial.instances[&node_id].state, json!({}));
    assert_eq!(initial.instances[&node_id].assets[0].id, "badge");
    assert!(initial.instances[&node_id].assets[0]
        .bytes
        .starts_with(b"<svg"));

    worker
        .action(7, node_id.clone(), json!({"action": "increment"}))
        .unwrap();
    match worker.results().recv_blocking().unwrap() {
        GraphWorkerResult::ActionCompleted {
            request_id,
            outcome,
        } => {
            assert_eq!(request_id, 7);
            assert_eq!(
                outcome.snapshot.instances[&node_id].state,
                json!({"count": 1})
            );
        }
        result => panic!("unexpected graph worker result: {result:?}"),
    }

    // A worker process failure must close or reject the runtime channel. It
    // must not take down the host process that owns this test.
    assert_eq!(unsafe { libc::kill(process_id as i32, libc::SIGKILL) }, 0);
    if worker
        .action(8, node_id, json!({"action": "increment"}))
        .is_ok()
    {
        match worker.results().recv_blocking() {
            Ok(GraphWorkerResult::Rejected { request_id: 8, .. }) | Err(_) => {}
            result => panic!("worker crash was not contained: {result:?}"),
        }
    }
    worker.shutdown().unwrap();
}
