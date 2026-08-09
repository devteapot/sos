use std::{
    io::Write as _,
    net::Shutdown,
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use provider_state_service::ServiceClient;
use serde_json::json;
use service_protocol::{
    FaultPoint, NotesAction, PromotionDraft, ProviderAction, ResourceQuery, ResponsePayload,
    ServiceError, ServiceRequest, TransactionStatus,
};
use tempfile::TempDir;

#[test]
fn daemon_speaks_typed_unix_protocol_and_survives_ambiguous_promotion() {
    let directory = TempDir::new().unwrap();
    let socket = directory.path().join("provider.sock");
    let state_file = directory.path().join("authority.json");
    let mut daemon = Command::new(
        std::env::var_os("CARGO_BIN_EXE_sos-provider-state-service")
            .expect("Cargo exposes service binary"),
    )
    .args([
        "--socket",
        socket.to_str().unwrap(),
        "--state-file",
        state_file.to_str().unwrap(),
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    wait_for_socket(&mut daemon, &socket);
    let client = ServiceClient::new(&socket, Duration::from_secs(2));

    // A cancelled/crashed caller may stop receiving after submitting work.
    // Its EPIPE must remain scoped to that connection, not kill authority.
    let mut abandoned = UnixStream::connect(&socket).unwrap();
    abandoned.shutdown(Shutdown::Read).unwrap();
    abandoned
        .write_all(
            br#"{"protocol_version":1,"method":"get_resource","request_id":900,"query":"notes"}
"#,
        )
        .unwrap();
    drop(abandoned);

    let staged = client
        .call(&ServiceRequest::StagePromotion {
            request_id: 1,
            draft: PromotionDraft {
                transaction_id: "daemon-tx".into(),
                expected_revision: 0,
                revision_id: "a".repeat(64),
                schema_version: 1,
                source_sha256: "b".repeat(64),
                state: json!({"daemon": true}),
                migration: None,
                actions: vec![ProviderAction::Notes(NotesAction::AttachToEvent {
                    note_id: "note-daemon".into(),
                    event_title: "Daemon review".into(),
                })],
            },
        })
        .unwrap();
    assert!(staged.ok);
    assert!(matches!(
        staged.payload,
        Some(ResponsePayload::Transaction { record })
            if record.status == TransactionStatus::Staged
    ));

    assert!(
        client
            .call(&ServiceRequest::ConfigureFault {
                request_id: 2,
                point: Some(FaultPoint::AfterPromotion),
            })
            .unwrap()
            .ok
    );
    let ambiguous = client
        .call(&ServiceRequest::Promote {
            request_id: 3,
            transaction_id: "daemon-tx".into(),
        })
        .unwrap();
    assert!(matches!(
        ambiguous.error,
        Some(ServiceError::InjectedFault {
            point: FaultPoint::AfterPromotion
        })
    ));
    let retried = client
        .call(&ServiceRequest::Promote {
            request_id: 4,
            transaction_id: "daemon-tx".into(),
        })
        .unwrap();
    assert!(retried.ok);
    let notes = client
        .call(&ServiceRequest::GetResource {
            request_id: 5,
            query: ResourceQuery::Notes,
        })
        .unwrap();
    assert!(matches!(
        notes.payload,
        Some(ResponsePayload::Resource {
            value: service_protocol::ResourceValue::Notes(notes)
        }) if notes.attachments.len() == 1
    ));
    assert!(daemon.try_wait().unwrap().is_none());
    assert!(
        client
            .call(&ServiceRequest::Shutdown { request_id: 6 })
            .unwrap()
            .ok
    );
    assert!(daemon.wait().unwrap().success());
    assert!(state_file.exists());
}

fn wait_for_socket(daemon: &mut Child, socket: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !socket.exists() {
        if let Some(status) = daemon.try_wait().unwrap() {
            panic!("service exited before binding socket: {status}");
        }
        assert!(Instant::now() < deadline, "service socket did not appear");
        thread::sleep(Duration::from_millis(5));
    }
}
