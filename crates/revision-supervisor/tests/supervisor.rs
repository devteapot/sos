use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixStream,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use revision_supervisor::{
    Error, RevisionInput, RevisionStore, RevisionSupervisor, SupervisorEvent,
};
use serde_json::json;
use tempfile::TempDir;

fn candidate_executable() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_supervisor-test-candidate")
        .map(Into::into)
        .expect("Cargo exposes the test candidate binary")
}

fn supervisor_executable() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_sos-revision-supervisor")
        .map(Into::into)
        .expect("Cargo exposes the supervisor binary")
}

fn install(store: &RevisionStore, source: &str, mode: &str) -> String {
    store
        .install(RevisionInput {
            source: source.as_bytes().to_vec(),
            state: json!({"source": source}),
            schema_version: 1,
            executable: candidate_executable(),
            args: vec![mode.into()],
        })
        .unwrap()
        .manifest
        .revision_id
}

#[test]
fn installs_verified_read_only_content_addressed_revisions() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let revision_id = install(&store, "return { render = true }", "stay");
    let revision = store.verify(&revision_id).unwrap();
    assert_eq!(revision.manifest.revision_id, revision_id);
    assert_eq!(revision.manifest.schema_version, 1);
    assert_eq!(
        fs::metadata(&revision.directory)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o555
    );
    assert_eq!(
        fs::metadata(revision.directory.join("source.luau"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o444
    );
    let duplicate = install(&store, "return { render = true }", "stay");
    assert_eq!(duplicate, revision_id);
}

#[test]
fn verification_rejects_state_source_or_schema_drift() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let revision_id = install(&store, "source-a", "stay");
    let revision = store.verify(&revision_id).unwrap();
    let state_file = revision.directory.join("state.json");
    fs::set_permissions(&state_file, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(
        &state_file,
        serde_json::to_vec(&json!({
            "schema_version": 2,
            "source_sha256": "0".repeat(64),
            "state": {}
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        store.verify(&revision_id),
        Err(Error::InvalidRevision(_))
    ));
}

#[test]
fn verification_recomputes_the_content_addressed_directory_identity() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let revision_id = install(&store, "source-a", "stay");
    let revision = store.verify(&revision_id).unwrap();
    let manifest_file = revision.directory.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_file).unwrap()).unwrap();
    manifest["args"] = json!(["stay", "unhashed-change"]);
    fs::set_permissions(&manifest_file, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&manifest_file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(matches!(
        store.verify(&revision_id),
        Err(Error::InvalidRevision(_))
    ));
}

#[test]
fn current_pointer_is_atomic_for_concurrent_readers() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let first = install(&store, "first", "stay");
    let second = install(&store, "second", "stay");
    store.set_current(&first).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = stop.clone();
    let first_reader = first.clone();
    let second_reader = second.clone();
    let pointer = directory.path().join("current");
    let reader = thread::spawn(move || {
        while !reader_stop.load(Ordering::Relaxed) {
            let target = fs::read_link(&pointer).unwrap();
            let current = target.file_name().unwrap().to_str().unwrap();
            assert!(
                current == first_reader || current == second_reader,
                "reader observed a partial pointer: {}",
                target.display()
            );
        }
    });
    for index in 0..50 {
        store
            .set_current(if index % 2 == 0 { &second } else { &first })
            .unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    reader.join().unwrap();
}

#[test]
fn crash_before_first_frame_preserves_the_accepted_process_and_pointer() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted", "stay");
    let broken = install(&store, "broken", "crash-before");
    store.set_current(&accepted).unwrap();
    let mut supervisor = RevisionSupervisor::new(store.clone(), Duration::from_secs(2));
    supervisor.boot().unwrap();
    assert!(matches!(
        supervisor.promote(&broken),
        Err(Error::CandidateExitedBeforeFirstFrame(_))
    ));
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn first_frame_promotes_and_a_later_crash_rolls_back_and_relaunches() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted", "stay");
    let candidate = install(&store, "candidate", "crash-after");
    store.set_current(&accepted).unwrap();
    let mut supervisor = RevisionSupervisor::new(store.clone(), Duration::from_secs(2));
    supervisor.boot().unwrap();
    assert!(matches!(
        supervisor.promote(&candidate).unwrap(),
        SupervisorEvent::Promoted { .. }
    ));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        candidate
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    let rollback = loop {
        if let Some(event) = supervisor.poll().unwrap() {
            break event;
        }
        assert!(Instant::now() < deadline, "candidate did not crash");
        thread::sleep(Duration::from_millis(5));
    };
    assert!(matches!(
        rollback,
        SupervisorEvent::RolledBack {
            ref failed_revision,
            ref restored_revision,
            ..
        } if failed_revision == &candidate && restored_revision == &accepted
    ));
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn supervisor_survives_and_relaunches_the_boot_revision_it_accepted() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted", "crash-after");
    store.set_current(&accepted).unwrap();
    let mut supervisor = RevisionSupervisor::new(store, Duration::from_secs(2));
    supervisor.boot().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let event = loop {
        if let Some(event) = supervisor.poll().unwrap() {
            break event;
        }
        assert!(Instant::now() < deadline, "accepted process did not exit");
        thread::sleep(Duration::from_millis(5));
    };
    assert!(matches!(
        event,
        SupervisorEvent::RolledBack {
            ref failed_revision,
            ref restored_revision,
            ..
        } if failed_revision == &accepted && restored_revision == &accepted
    ));
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
}

#[test]
fn first_frame_timeout_kills_candidate_without_promotion() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted", "stay");
    let candidate = install(&store, "candidate", "no-ready");
    store.set_current(&accepted).unwrap();
    let mut supervisor = RevisionSupervisor::new(store.clone(), Duration::from_millis(30));
    supervisor.boot().unwrap();
    assert!(matches!(
        supervisor.promote(&candidate),
        Err(Error::FirstFrameTimeout(_))
    ));
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn candidate_exit_between_first_frame_and_pointer_commit_is_rejected() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted", "stay");
    let candidate = install(&store, "candidate", "exit-after");
    store.set_current(&accepted).unwrap();
    let mut supervisor = RevisionSupervisor::new(store.clone(), Duration::from_secs(2));
    supervisor.boot().unwrap();
    let prepared = supervisor.prepare(&candidate).unwrap();
    thread::sleep(Duration::from_millis(80));
    assert!(matches!(
        supervisor.commit_prepared(prepared),
        Err(Error::CandidateExitedBeforePointerCommit(_))
    ));
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn standalone_daemon_survives_candidate_crash_and_accepts_more_control_requests() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted", "stay");
    let candidate = install(&store, "candidate", "crash-after");
    store.set_current(&accepted).unwrap();
    let mut daemon = Command::new(supervisor_executable())
        .args([
            "serve",
            "--root",
            directory.path().to_str().unwrap(),
            "--timeout-ms",
            "2000",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let socket = directory.path().join("run/supervisor.sock");
    wait_for_socket(&mut daemon, &socket);

    let status = control(&socket, json!({"action":"status"}));
    assert_eq!(status["active_revision"], accepted);
    let promoted = control(
        &socket,
        json!({"action":"promote", "revision_id": candidate}),
    );
    assert_eq!(promoted["ok"], true);
    assert_eq!(promoted["active_revision"], candidate);

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = control(&socket, json!({"action":"status"}));
        if status["active_revision"] == accepted {
            break;
        }
        assert!(Instant::now() < deadline, "daemon did not recover");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(daemon.try_wait().unwrap().is_none());
    let shutdown = control(&socket, json!({"action":"shutdown"}));
    assert_eq!(shutdown["ok"], true);
    assert!(daemon.wait().unwrap().success());
}

fn wait_for_socket(daemon: &mut Child, socket: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !socket.exists() {
        if let Some(status) = daemon.try_wait().unwrap() {
            panic!("supervisor exited before binding its socket: {status}");
        }
        assert!(
            Instant::now() < deadline,
            "supervisor socket did not appear"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn control(socket: &std::path::Path, request: serde_json::Value) -> serde_json::Value {
    let mut stream = UnixStream::connect(socket).unwrap();
    serde_json::to_writer(&mut stream, &request).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).unwrap();
    serde_json::from_str(&response).unwrap()
}
