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
    Error, HostCommand, RevisionInput, RevisionStore, RevisionSupervisor, SupervisorEvent,
};
use serde_json::json;
use tempfile::TempDir;

fn host_executable() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_supervisor-test-host")
        .map(Into::into)
        .expect("Cargo exposes the test host binary")
}

fn supervisor_executable() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_sos-revision-supervisor")
        .map(Into::into)
        .expect("Cargo exposes the supervisor binary")
}

fn install(store: &RevisionStore, source: &str) -> String {
    install_api(store, source, 1)
}

fn install_api(store: &RevisionStore, source: &str, experience_api_version: u32) -> String {
    store
        .install(RevisionInput {
            source: source.as_bytes().to_vec(),
            state: json!({"source": source}),
            schema_version: 1,
            experience_api_version,
        })
        .unwrap()
        .manifest
        .revision_id
}

fn supervisor(store: &RevisionStore, timeout: Duration) -> RevisionSupervisor {
    RevisionSupervisor::new(store.clone(), HostCommand::new(host_executable()), timeout)
}

#[test]
fn installs_verified_read_only_luau_revisions_without_native_payloads() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let revision_id = install(&store, "return { render = true }");
    let revision = store.verify(&revision_id).unwrap();
    assert_eq!(revision.manifest.revision_id, revision_id);
    assert_eq!(revision.manifest.schema_version, 1);
    assert_eq!(revision.manifest.experience_api_version, 1);
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
    assert!(!revision.directory.join("experience").exists());
    let duplicate = install(&store, "return { render = true }");
    assert_eq!(duplicate, revision_id);
}

#[test]
fn verification_rejects_state_source_or_schema_drift() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let revision_id = install(&store, "source-a");
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
fn verification_recomputes_api_bound_content_identity() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let revision_id = install(&store, "source-a");
    let revision = store.verify(&revision_id).unwrap();
    let manifest_file = revision.directory.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_file).unwrap()).unwrap();
    manifest["experience_api_version"] = json!(2);
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
    let first = install(&store, "first");
    let second = install(&store, "second");
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
fn rejected_luau_candidate_preserves_active_scene_host_and_pointer() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let broken = install(&store, "host:reject");
    store.set_current(&accepted).unwrap();
    let mut supervisor = supervisor(&store, Duration::from_secs(2));
    supervisor.boot().unwrap();
    let host_pid = supervisor.host_pid();
    assert!(matches!(
        supervisor.activate(&broken),
        Err(Error::HostRejected(_))
    ));
    assert_eq!(supervisor.host_pid(), host_pid);
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn unsupported_experience_api_rejects_before_scene_activation() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let unsupported = install_api(&store, "candidate", 2);
    store.set_current(&accepted).unwrap();
    let mut supervisor = supervisor(&store, Duration::from_secs(2));
    supervisor.boot().unwrap();
    assert!(matches!(
        supervisor.activate(&unsupported),
        Err(Error::HostRejected(_))
    ));
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn activation_presents_in_the_existing_host_and_then_advances_pointer() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let candidate = install(&store, "candidate");
    store.set_current(&accepted).unwrap();
    let mut supervisor = supervisor(&store, Duration::from_secs(2));
    supervisor.boot().unwrap();
    let host_pid = supervisor.host_pid().unwrap();
    assert!(matches!(
        supervisor.activate(&candidate).unwrap(),
        SupervisorEvent::Activated {
            host_pid: activated_pid,
            ..
        } if activated_pid == host_pid
    ));
    assert_eq!(supervisor.host_pid(), Some(host_pid));
    assert_eq!(supervisor.active_revision(), Some(candidate.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        candidate
    );
}

#[test]
fn presentation_failure_preserves_the_previous_pointer() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let candidate = install(&store, "host:exit-before-present");
    store.set_current(&accepted).unwrap();
    let mut supervisor = supervisor(&store, Duration::from_secs(2));
    supervisor.boot().unwrap();
    let failed_host_pid = supervisor.host_pid();
    assert!(matches!(
        supervisor.activate(&candidate),
        Err(Error::HostExited(_))
    ));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
    assert_ne!(supervisor.host_pid(), failed_host_pid);
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
}

#[test]
fn host_exit_after_present_before_pointer_preserves_previous_revision() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let candidate = install(&store, "host:exit-immediately-after-present");
    store.set_current(&accepted).unwrap();
    let mut supervisor = supervisor(&store, Duration::from_secs(2));
    supervisor.boot().unwrap();
    let failed_host_pid = supervisor.host_pid();
    assert!(matches!(
        supervisor.activate(&candidate),
        Err(Error::HostExited(_))
    ));
    assert_ne!(supervisor.host_pid(), failed_host_pid);
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn preparation_timeout_leaves_the_accepted_revision_running() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let candidate = install(&store, "host:no-response");
    store.set_current(&accepted).unwrap();
    let mut supervisor = supervisor(&store, Duration::from_millis(30));
    supervisor.boot().unwrap();
    let initial_host_pid = supervisor.host_pid();
    assert!(matches!(
        supervisor.activate(&candidate),
        Err(Error::HostTimeout(_))
    ));
    assert_ne!(supervisor.host_pid(), initial_host_pid);
    assert_eq!(supervisor.active_revision(), Some(accepted.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        accepted
    );
}

#[test]
fn permanent_host_crash_restarts_the_committed_revision_without_pointer_rollback() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let candidate = install(&store, "host:crash-later");
    store.set_current(&accepted).unwrap();
    let mut supervisor = supervisor(&store, Duration::from_secs(2));
    supervisor.boot().unwrap();
    supervisor.activate(&candidate).unwrap();
    let failed_host_pid = supervisor.host_pid().unwrap();
    thread::sleep(Duration::from_millis(300));
    let event = supervisor.poll().unwrap().unwrap();
    assert!(matches!(
        event,
        SupervisorEvent::HostRestarted {
            ref revision_id,
            failed_host_pid: failed,
            host_pid,
        } if revision_id == &candidate && failed == failed_host_pid && host_pid != failed_host_pid
    ));
    assert_eq!(supervisor.active_revision(), Some(candidate.as_str()));
    assert_eq!(
        store.current().unwrap().unwrap().manifest.revision_id,
        candidate
    );
}

#[test]
fn daemon_activates_luau_in_one_stable_host_process() {
    let directory = TempDir::new().unwrap();
    let store = RevisionStore::open(directory.path()).unwrap();
    let accepted = install(&store, "accepted");
    let candidate = install(&store, "candidate");
    store.set_current(&accepted).unwrap();
    let mut daemon = Command::new(supervisor_executable())
        .args([
            "serve",
            "--root",
            directory.path().to_str().unwrap(),
            "--host-executable",
            host_executable().to_str().unwrap(),
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
    let host_pid = status["host_pid"].as_u64().unwrap();
    assert_eq!(status["active_revision"], accepted);
    let activated = control(
        &socket,
        json!({"action":"activate", "revision_id": candidate}),
    );
    assert_eq!(activated["ok"], true);
    assert_eq!(activated["active_revision"], candidate);
    assert_eq!(activated["host_pid"], host_pid);

    assert_eq!(control(&socket, json!({"action":"shutdown"}))["ok"], true);
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
