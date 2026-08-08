use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use provider_state_service::{serve, ServiceClient};
use revision_supervisor::{
    CoordinatedSupervisor, CoordinationError, CoordinationEvent, CoordinatorFaultPoint,
    HostCommand, JournalPhase, RevisionInput, RevisionStore, RevisionSupervisor,
};
use serde_json::json;
use service_protocol::{
    FaultPoint, PromotionDraft, ResponsePayload, ServiceRequest, TransactionRecord,
    TransactionStatus,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

struct ServiceHarness {
    socket: PathBuf,
    thread: Option<thread::JoinHandle<()>>,
}

impl ServiceHarness {
    fn start(root: &Path) -> Self {
        let socket = root.join("authority.sock");
        let state_file = root.join("authority.json");
        let thread_socket = socket.clone();
        let handle = thread::spawn(move || serve(&thread_socket, &state_file).unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !socket.exists() {
            assert!(Instant::now() < deadline, "service socket did not appear");
            thread::sleep(Duration::from_millis(2));
        }
        Self {
            socket,
            thread: Some(handle),
        }
    }

    fn client(&self) -> ServiceClient {
        ServiceClient::new(&self.socket, Duration::from_secs(2))
    }

    fn transaction(&self, transaction_id: &str) -> TransactionRecord {
        let response = self
            .client()
            .call(&ServiceRequest::GetTransaction {
                request_id: 900,
                transaction_id: transaction_id.into(),
            })
            .unwrap();
        match response.payload.unwrap() {
            ResponsePayload::Transaction { record } => record,
            _ => panic!("wrong transaction response"),
        }
    }

    fn configure_fault(&self, point: FaultPoint) {
        assert!(
            self.client()
                .call(&ServiceRequest::ConfigureFault {
                    request_id: 901,
                    point: Some(point),
                })
                .unwrap()
                .ok
        );
    }
}

impl Drop for ServiceHarness {
    fn drop(&mut self) {
        if let Some(handle) = self.thread.take() {
            let _ = self
                .client()
                .call(&ServiceRequest::Shutdown { request_id: 999 });
            handle.join().unwrap();
        }
    }
}

struct Fixture {
    // Fields drop in declaration order; stop the socket service before TempDir
    // removes its pathname.
    service: ServiceHarness,
    _directory: TempDir,
    store: RevisionStore,
    accepted: String,
    candidate: String,
    transaction_id: String,
}

impl Fixture {
    fn new(candidate_mode: &str) -> Self {
        let directory = TempDir::new().unwrap();
        let store = RevisionStore::open(directory.path().join("revisions-root")).unwrap();
        let service = ServiceHarness::start(&directory.path().join("service"));
        let accepted_source = "accepted-source";
        let accepted_state = json!({"screen":"accepted"});
        let accepted = install(&store, accepted_source, accepted_state.clone());
        stage(
            &service.client(),
            "bootstrap-tx",
            0,
            &accepted,
            accepted_source,
            accepted_state,
        );
        promote_service(&service.client(), "bootstrap-tx");
        store.set_current(&accepted).unwrap();

        let candidate_source = match candidate_mode {
            "stay" => "candidate-source",
            "crash-later" => "host:crash-later",
            "crash-before" => "host:reject",
            mode => panic!("unsupported test host mode: {mode}"),
        };
        let candidate_state = json!({"screen":"candidate"});
        let candidate = install(&store, candidate_source, candidate_state.clone());
        let transaction_id = "candidate-tx".to_owned();
        stage(
            &service.client(),
            &transaction_id,
            1,
            &candidate,
            candidate_source,
            candidate_state,
        );
        Self {
            service,
            _directory: directory,
            store,
            accepted,
            candidate,
            transaction_id,
        }
    }

    fn coordinator(&self) -> CoordinatedSupervisor {
        CoordinatedSupervisor::new(
            self.store.clone(),
            RevisionSupervisor::new(
                self.store.clone(),
                HostCommand::new(host_executable()),
                Duration::from_secs(2),
            ),
            self.service.client(),
        )
    }

    fn current(&self) -> String {
        self.store.current().unwrap().unwrap().manifest.revision_id
    }
}

fn host_executable() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_supervisor-test-host")
        .map(Into::into)
        .expect("Cargo exposes host binary")
}

fn supervisor_executable() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_sos-revision-supervisor")
        .map(Into::into)
        .expect("Cargo exposes supervisor binary")
}

fn install(store: &RevisionStore, source: &str, state: serde_json::Value) -> String {
    store
        .install(RevisionInput {
            source: source.as_bytes().to_vec(),
            state,
            schema_version: 1,
            experience_api_version: 1,
        })
        .unwrap()
        .manifest
        .revision_id
}

fn stage(
    client: &ServiceClient,
    transaction_id: &str,
    expected_revision: u64,
    revision_id: &str,
    source: &str,
    state: serde_json::Value,
) {
    let response = client
        .call(&ServiceRequest::StagePromotion {
            request_id: 1,
            draft: PromotionDraft {
                transaction_id: transaction_id.into(),
                expected_revision,
                revision_id: revision_id.into(),
                schema_version: 1,
                source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
                state,
                migration: None,
                actions: Vec::new(),
            },
        })
        .unwrap();
    assert!(response.ok, "stage failed: {:?}", response.error);
}

fn promote_service(client: &ServiceClient, transaction_id: &str) {
    let response = client
        .call(&ServiceRequest::Promote {
            request_id: 2,
            transaction_id: transaction_id.into(),
        })
        .unwrap();
    assert!(response.ok, "promotion failed: {:?}", response.error);
}

#[test]
fn coordinated_activation_commits_service_before_pointer_and_clears_journal() {
    let fixture = Fixture::new("stay");
    let mut coordinator = fixture.coordinator();
    coordinator.boot().unwrap();
    let event = coordinator
        .activate(&fixture.transaction_id, &fixture.candidate)
        .unwrap();
    assert!(matches!(event, CoordinationEvent::Activated { .. }));
    assert_eq!(fixture.current(), fixture.candidate);
    assert_eq!(
        fixture.service.transaction(&fixture.transaction_id).status,
        TransactionStatus::Committed
    );
    assert!(coordinator.journal().unwrap().is_none());
}

#[test]
fn crash_after_intent_recovers_previous_and_aborts_staged_transaction() {
    let fixture = Fixture::new("stay");
    {
        let mut coordinator = fixture.coordinator();
        coordinator.boot().unwrap();
        coordinator.configure_fault(Some(CoordinatorFaultPoint::AfterIntent));
        assert!(matches!(
            coordinator.activate(&fixture.transaction_id, &fixture.candidate),
            Err(CoordinationError::InjectedFault(
                CoordinatorFaultPoint::AfterIntent
            ))
        ));
        assert_eq!(
            coordinator.journal().unwrap().unwrap().phase,
            JournalPhase::Intent
        );
    }
    let mut recovered = fixture.coordinator();
    recovered.boot().unwrap();
    assert_eq!(fixture.current(), fixture.accepted);
    assert_eq!(
        fixture.service.transaction(&fixture.transaction_id).status,
        TransactionStatus::Aborted
    );
    assert!(recovered.journal().unwrap().is_none());
}

#[test]
fn crash_after_service_commit_recovers_candidate_and_pointer() {
    let fixture = Fixture::new("stay");
    {
        let mut coordinator = fixture.coordinator();
        coordinator.boot().unwrap();
        coordinator.configure_fault(Some(CoordinatorFaultPoint::AfterServiceCommit));
        assert!(matches!(
            coordinator.activate(&fixture.transaction_id, &fixture.candidate),
            Err(CoordinationError::InjectedFault(
                CoordinatorFaultPoint::AfterServiceCommit
            ))
        ));
        assert_eq!(fixture.current(), fixture.accepted);
        assert_eq!(
            coordinator.journal().unwrap().unwrap().phase,
            JournalPhase::ServiceCommitted
        );
    }
    let mut recovered = fixture.coordinator();
    recovered.boot().unwrap();
    assert_eq!(fixture.current(), fixture.candidate);
    assert_eq!(
        recovered.active_revision(),
        Some(fixture.candidate.as_str())
    );
    assert!(recovered.journal().unwrap().is_none());
}

#[test]
fn crash_after_pointer_commit_boots_candidate_and_only_cleans_journal() {
    let fixture = Fixture::new("stay");
    {
        let mut coordinator = fixture.coordinator();
        coordinator.boot().unwrap();
        coordinator.configure_fault(Some(CoordinatorFaultPoint::AfterPointerCommit));
        assert!(matches!(
            coordinator.activate(&fixture.transaction_id, &fixture.candidate),
            Err(CoordinationError::InjectedFault(
                CoordinatorFaultPoint::AfterPointerCommit
            ))
        ));
        assert_eq!(fixture.current(), fixture.candidate);
        assert_eq!(
            coordinator.journal().unwrap().unwrap().phase,
            JournalPhase::PointerCommitted
        );
    }
    let mut recovered = fixture.coordinator();
    recovered.boot().unwrap();
    assert_eq!(fixture.current(), fixture.candidate);
    assert!(recovered.journal().unwrap().is_none());
}

#[test]
fn service_middle_fault_is_reconciled_before_pointer_commit() {
    let fixture = Fixture::new("stay");
    fixture.service.configure_fault(FaultPoint::DuringPromotion);
    let mut coordinator = fixture.coordinator();
    coordinator.boot().unwrap();
    coordinator
        .activate(&fixture.transaction_id, &fixture.candidate)
        .unwrap();
    assert_eq!(fixture.current(), fixture.candidate);
    assert_eq!(
        fixture.service.transaction(&fixture.transaction_id).status,
        TransactionStatus::Committed
    );
}

#[test]
fn service_precommit_fault_aborts_transaction_and_keeps_previous() {
    let fixture = Fixture::new("stay");
    fixture.service.configure_fault(FaultPoint::BeforePromotion);
    let mut coordinator = fixture.coordinator();
    coordinator.boot().unwrap();
    assert!(coordinator
        .activate(&fixture.transaction_id, &fixture.candidate)
        .is_err());
    assert_eq!(fixture.current(), fixture.accepted);
    assert_eq!(
        fixture.service.transaction(&fixture.transaction_id).status,
        TransactionStatus::Aborted
    );
    assert!(coordinator.journal().unwrap().is_none());
}

#[test]
fn accepted_crash_relaunches_committed_current_instead_of_splitting_state() {
    let fixture = Fixture::new("crash-later");
    let mut coordinator = fixture.coordinator();
    coordinator.boot().unwrap();
    coordinator
        .activate(&fixture.transaction_id, &fixture.candidate)
        .unwrap();
    thread::sleep(Duration::from_millis(300));
    let event = coordinator.poll().unwrap();
    assert!(event.is_some(), "supervisor did not observe accepted crash");
    assert_eq!(fixture.current(), fixture.candidate);
    assert_eq!(
        coordinator.active_revision(),
        Some(fixture.candidate.as_str())
    );
    assert_eq!(
        fixture.service.transaction(&fixture.transaction_id).status,
        TransactionStatus::Committed
    );
}

#[test]
fn candidate_validation_rejection_aborts_transaction_and_preserves_previous() {
    let fixture = Fixture::new("crash-before");
    let mut coordinator = fixture.coordinator();
    coordinator.boot().unwrap();
    assert!(coordinator
        .activate(&fixture.transaction_id, &fixture.candidate)
        .is_err());
    assert_eq!(fixture.current(), fixture.accepted);
    assert_eq!(
        fixture.service.transaction(&fixture.transaction_id).status,
        TransactionStatus::Aborted
    );
    assert!(coordinator.journal().unwrap().is_none());
}

#[test]
fn mismatched_immutable_state_is_rejected_before_journal_or_launch() {
    let fixture = Fixture::new("stay");
    let mismatched = "mismatched-tx";
    stage(
        &fixture.service.client(),
        mismatched,
        1,
        &fixture.candidate,
        "candidate-source",
        json!({"screen":"different"}),
    );
    let mut coordinator = fixture.coordinator();
    coordinator.boot().unwrap();
    assert!(matches!(
        coordinator.activate(mismatched, &fixture.candidate),
        Err(CoordinationError::InvalidBinding(_))
    ));
    assert_eq!(fixture.current(), fixture.accepted);
    assert!(coordinator.journal().unwrap().is_none());
}

#[test]
fn standalone_daemon_exposes_coordinated_activation_control() {
    let fixture = Fixture::new("stay");
    let mut daemon = Command::new(supervisor_executable())
        .args([
            "serve",
            "--root",
            fixture.store.root().to_str().unwrap(),
            "--timeout-ms",
            "2000",
            "--host-executable",
            host_executable().to_str().unwrap(),
            "--service-socket",
            fixture.service.socket.to_str().unwrap(),
            "--service-timeout-ms",
            "2000",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let socket = fixture.store.root().join("run/supervisor.sock");
    wait_for_process_socket(&mut daemon, &socket);
    let response = control(
        &socket,
        json!({
            "action":"activate",
            "revision_id":fixture.candidate,
            "transaction_id":fixture.transaction_id,
        }),
    );
    assert_eq!(response["ok"], true, "response: {response}");
    assert_eq!(response["active_revision"], fixture.candidate);
    assert_eq!(fixture.current(), fixture.candidate);
    assert_eq!(
        fixture.service.transaction(&fixture.transaction_id).status,
        TransactionStatus::Committed
    );
    assert_eq!(control(&socket, json!({"action":"shutdown"}))["ok"], true);
    assert!(daemon.wait().unwrap().success());
}

fn wait_for_process_socket(process: &mut Child, socket: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !socket.exists() {
        if let Some(status) = process.try_wait().unwrap() {
            panic!("supervisor exited before binding control socket: {status}");
        }
        assert!(Instant::now() < deadline, "control socket did not appear");
        thread::sleep(Duration::from_millis(2));
    }
}

fn control(socket: &Path, request: serde_json::Value) -> serde_json::Value {
    let mut stream = UnixStream::connect(socket).unwrap();
    serde_json::to_writer(&mut stream, &request).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
