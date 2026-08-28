use std::{path::PathBuf, thread, time::Duration};

use provider_state_service::ServiceClient;
use revision_supervisor::{
    ExperienceRegistry, GraphResolver, GraphStore, RevisionInput, RevisionPackageInput,
    RevisionStore,
};
use service_protocol::{ResourceQuery, ResourceValue, ResponsePayload, ServiceRequest};
use sos_linux_session::{bootstrap_graph_authority, shutdown_authority, GraphBootstrapOutcome};

#[test]
fn fresh_v4_graph_bootstraps_authority_without_a_singleton_pointer() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("revisions");
    let socket = temporary.path().join("provider.sock");
    let authority_file = temporary.path().join("authority.json");
    let store = RevisionStore::open(&root).unwrap();
    let package: experience_package::PackageMetadata =
        serde_json::from_str(include_str!("../../../experiences/default.package.json")).unwrap();
    let revision = store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: b"return { api_version = 4, exports = { main = {} } }".to_vec(),
                state: serde_json::json!({"fresh": true}),
                schema_version: 1,
                experience_api_version: 4,
                assets: Vec::new(),
            },
            package,
        })
        .unwrap();
    let stock = experience_package::ExperienceId::parse("sos.stock.shell").unwrap();
    let registry = ExperienceRegistry::open(store.clone()).unwrap();
    registry
        .create(
            &stock,
            experience_package::ExperienceRole::Shell,
            &revision.manifest.revision_id,
        )
        .unwrap();
    let graph = GraphResolver::new(store.clone())
        .resolve(
            &revision.manifest.revision_id,
            &experience_package::ExportId::parse("main").unwrap(),
        )
        .unwrap();
    let graphs = GraphStore::open(&root).unwrap();
    let graph_id = graphs.install(&graph).unwrap();
    graphs.set_current(&stock, &graph_id).unwrap();

    let service = start_service(socket.clone(), authority_file);
    assert!(matches!(
        bootstrap_graph_authority(&root, &stock, &socket, Duration::from_secs(2)).unwrap(),
        GraphBootstrapOutcome::Initialized {
            graph_id: initialized,
            experience_count: 1,
            ..
        } if initialized == graph_id
    ));
    let client = ServiceClient::new(&socket, Duration::from_secs(2));
    let state = client
        .call(&ServiceRequest::GetResource {
            request_id: 31,
            query: ResourceQuery::ExperienceStateFor {
                experience_id: stock,
            },
        })
        .unwrap();
    assert!(matches!(
        state.payload,
        Some(ResponsePayload::Resource {
            value: ResourceValue::ExperienceStateFor(state),
        }) if state.resource.revision_id == revision.manifest.revision_id
            && state.resource.state == serde_json::json!({"fresh": true})
    ));
    shutdown_authority(&socket, Duration::from_secs(2)).unwrap();
    service.join().unwrap().unwrap();
}

fn start_service(
    socket: PathBuf,
    authority_file: PathBuf,
) -> thread::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
    let handle = thread::spawn({
        let socket = socket.clone();
        move || provider_state_service::serve(&socket, &authority_file)
    });
    for _ in 0..200 {
        if socket.exists() {
            return handle;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("provider service did not create its socket");
}
