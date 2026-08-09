use std::{fs, path::PathBuf, thread, time::Duration};

use provider_state_service::ServiceClient;
use revision_supervisor::{ActivationJournal, JournalPhase, RevisionInput, RevisionStore};
use service_protocol::{
    PromotionDraft, ResourceQuery, ResourceValue, ResponsePayload, ServiceRequest,
    TransactionStatus,
};
use sos_linux_session::{
    bootstrap_authority, shutdown_authority, stage_revision, BootstrapOutcome,
};

#[test]
fn bootstraps_current_and_stages_the_next_verified_revision() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("revisions");
    let socket = temporary.path().join("provider.sock");
    let authority_file = temporary.path().join("authority.json");
    let store = RevisionStore::open(&root).unwrap();
    let initial = store
        .install(RevisionInput {
            source: b"return { api_version = 3 }".to_vec(),
            state: serde_json::json!({"count": 0}),
            schema_version: 1,
            experience_api_version: 3,
            assets: Vec::new(),
        })
        .unwrap();
    store.set_current(&initial.manifest.revision_id).unwrap();
    let candidate = store
        .install(RevisionInput {
            source: b"return { api_version = 3, revision = 2 }".to_vec(),
            state: serde_json::json!({"count": 1}),
            schema_version: 2,
            experience_api_version: 3,
            assets: Vec::new(),
        })
        .unwrap();
    let mut service = start_service(socket.clone(), authority_file.clone());

    let outcome = bootstrap_authority(&root, &socket, Duration::from_secs(2)).unwrap();
    assert!(matches!(
        outcome,
        BootstrapOutcome::Initialized { revision_id, .. }
            if revision_id == initial.manifest.revision_id
    ));
    assert!(matches!(
        bootstrap_authority(&root, &socket, Duration::from_secs(2)).unwrap(),
        BootstrapOutcome::AlreadyBound { revision_id }
            if revision_id == initial.manifest.revision_id
    ));

    let client = ServiceClient::new(&socket, Duration::from_secs(2));
    let state = client
        .call(&ServiceRequest::GetResource {
            request_id: 8,
            query: ResourceQuery::ExperienceState,
        })
        .unwrap();
    let state = match state.payload {
        Some(ResponsePayload::Resource {
            value: ResourceValue::ExperienceState(state),
        }) => state,
        _ => panic!("state response omitted the experience state"),
    };
    let interaction = "same-revision-interaction".to_owned();
    client
        .call(&ServiceRequest::StagePromotion {
            request_id: 9,
            draft: PromotionDraft {
                transaction_id: interaction.clone(),
                expected_revision: state.revision,
                revision_id: state.revision_id,
                schema_version: state.schema_version,
                source_sha256: state.source_sha256,
                state: serde_json::json!({"count": 7}),
                migration: None,
                actions: Vec::new(),
            },
        })
        .unwrap();
    client
        .call(&ServiceRequest::Promote {
            request_id: 10,
            transaction_id: interaction,
        })
        .unwrap();
    shutdown_authority(&socket, Duration::from_secs(2)).unwrap();
    service.join().unwrap().unwrap();
    service = start_service(socket.clone(), authority_file);
    assert!(matches!(
        bootstrap_authority(&root, &socket, Duration::from_secs(2)).unwrap(),
        BootstrapOutcome::AlreadyBound { revision_id }
            if revision_id == initial.manifest.revision_id
    ));

    let transaction_id = stage_revision(
        &root,
        &candidate.manifest.revision_id,
        &socket,
        Duration::from_secs(2),
    )
    .unwrap();
    let transaction = client
        .call(&ServiceRequest::GetTransaction {
            request_id: 11,
            transaction_id: transaction_id.clone(),
        })
        .unwrap();
    let record = match transaction.payload {
        Some(ResponsePayload::Transaction { record }) => record,
        _ => panic!("transaction response omitted the record"),
    };
    assert_eq!(record.draft.revision_id, candidate.manifest.revision_id);
    assert_eq!(record.draft.expected_revision, 2);
    assert_eq!(record.draft.schema_version, 2);
    assert!(record.draft.migration.is_some());
    assert!(record.draft.actions.is_empty());

    let state = client
        .call(&ServiceRequest::GetResource {
            request_id: 12,
            query: ResourceQuery::ExperienceState,
        })
        .unwrap();
    assert!(matches!(
        state.payload,
        Some(ResponsePayload::Resource {
            value: ResourceValue::ExperienceState(state),
        }) if state.revision_id == initial.manifest.revision_id
    ));

    let promoted = client
        .call(&ServiceRequest::Promote {
            request_id: 13,
            transaction_id,
        })
        .unwrap();
    assert!(matches!(
        promoted.payload,
        Some(ResponsePayload::Transaction { record })
            if record.status == TransactionStatus::Committed
    ));
    let mismatch = bootstrap_authority(&root, &socket, Duration::from_secs(2)).unwrap_err();
    assert!(mismatch
        .to_string()
        .contains("provider authority is already initialized"));

    fs::write(
        root.join("activation-journal.json"),
        serde_json::to_vec(&ActivationJournal {
            format_version: 1,
            transaction_id: "recovery-transaction".into(),
            previous_revision: initial.manifest.revision_id.clone(),
            candidate_revision: candidate.manifest.revision_id.clone(),
            phase: JournalPhase::ServiceCommitted,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        bootstrap_authority(&root, &socket, Duration::from_secs(2)).unwrap(),
        BootstrapOutcome::RecoveryRequired {
            pointer_revision,
            authority_revision,
        } if pointer_revision == initial.manifest.revision_id
            && authority_revision == candidate.manifest.revision_id
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
