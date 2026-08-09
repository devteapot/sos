use std::{
    fs, io,
    os::unix::fs::{FileTypeExt as _, PermissionsExt as _},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::read_shell_token_file;
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use revision_supervisor::RevisionStore;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

use crate::{bootstrap_authority, shutdown_authority};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct SystemSessionOptions {
    pub revision_root: PathBuf,
    pub runtime_directory: PathBuf,
    pub authority_file: PathBuf,
    pub shell_token_file: PathBuf,
    pub compositor_executable: PathBuf,
    pub provider_executable: PathBuf,
    pub supervisor_executable: PathBuf,
    pub host_executable: PathBuf,
    pub startup_timeout: Duration,
}

#[derive(Default)]
struct SessionProcesses {
    compositor: Option<Child>,
    provider: Option<Child>,
    supervisor: Option<Child>,
}

pub fn run_system_session(options: SystemSessionOptions) -> Result<()> {
    validate_options(&options)?;
    let stopping = Arc::new(AtomicBool::new(false));
    for signal in [SIGTERM, SIGINT, SIGHUP] {
        signal_hook::flag::register(signal, Arc::clone(&stopping))?;
    }

    let mut processes = SessionProcesses::default();
    let result = start_and_monitor(&options, &stopping, &mut processes);
    shutdown_processes(&options, &mut processes);
    if stopping.load(Ordering::Relaxed) {
        println!("linux_system_session_stopped reason=signal");
        Ok(())
    } else {
        result
    }
}

fn validate_options(options: &SystemSessionOptions) -> Result<()> {
    for (name, path) in [
        ("revision root", options.revision_root.as_path()),
        ("runtime directory", options.runtime_directory.as_path()),
        ("authority file", options.authority_file.as_path()),
        ("shell credential", options.shell_token_file.as_path()),
        (
            "compositor executable",
            options.compositor_executable.as_path(),
        ),
        ("provider executable", options.provider_executable.as_path()),
        (
            "supervisor executable",
            options.supervisor_executable.as_path(),
        ),
        ("host executable", options.host_executable.as_path()),
    ] {
        if !path.is_absolute() {
            bail!("{name} path must be absolute: {}", path.display());
        }
    }
    for (name, path) in [
        ("compositor", options.compositor_executable.as_path()),
        ("provider", options.provider_executable.as_path()),
        ("supervisor", options.supervisor_executable.as_path()),
        ("host", options.host_executable.as_path()),
    ] {
        let metadata = fs::metadata(path)
            .with_context(|| format!("inspect {name} executable {}", path.display()))?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            bail!("{name} is not an executable file: {}", path.display());
        }
    }
    read_shell_token_file(&options.shell_token_file).with_context(|| {
        format!(
            "validate compositor shell credential {}",
            options.shell_token_file.display()
        )
    })?;
    if options.startup_timeout.is_zero() {
        bail!("startup timeout must be greater than zero");
    }
    Ok(())
}

fn start_and_monitor(
    options: &SystemSessionOptions,
    stopping: &AtomicBool,
    processes: &mut SessionProcesses,
) -> Result<()> {
    fs::create_dir_all(&options.runtime_directory).with_context(|| {
        format!(
            "create system session runtime directory {}",
            options.runtime_directory.display()
        )
    })?;
    fs::create_dir_all(&options.revision_root).with_context(|| {
        format!(
            "create system session revision root {}",
            options.revision_root.display()
        )
    })?;
    if let Some(parent) = options.authority_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create authority directory {}", parent.display()))?;
    }
    let state_directory = options
        .authority_file
        .parent()
        .context("authority file must have a parent directory")?;
    let cache_directory = state_directory.join("cache");
    fs::create_dir_all(&cache_directory).with_context(|| {
        format!(
            "create system session cache directory {}",
            cache_directory.display()
        )
    })?;

    let store = RevisionStore::open(&options.revision_root)?;
    let current_revision = store
        .current()?
        .context("cannot boot the system session before the revision pointer is initialized")?
        .manifest
        .revision_id;
    let supervisor_socket = options.revision_root.join("run/supervisor.sock");
    remove_refused_stale_socket(&supervisor_socket)?;

    let wayland_display = "wayland-sos";
    let wayland_socket = options.runtime_directory.join(wayland_display);
    let control_socket = options.runtime_directory.join("compositor-control.sock");
    let ready_file = options.runtime_directory.join("compositor-ready");
    let provider_socket = options.runtime_directory.join("provider-state.sock");
    for path in [
        &wayland_socket,
        &control_socket,
        &ready_file,
        &provider_socket,
    ] {
        if path.exists() {
            bail!(
                "system session runtime path already exists: {}",
                path.display()
            );
        }
    }

    let compositor = Command::new(&options.compositor_executable)
        .arg("--backend")
        .arg("drm")
        .arg("--socket")
        .arg(wayland_display)
        .arg("--control-socket")
        .arg(&control_socket)
        .arg("--shell-token-file")
        .arg(&options.shell_token_file)
        .arg("--ready-file")
        .arg(&ready_file)
        .env("XDG_RUNTIME_DIR", &options.runtime_directory)
        .env("HOME", state_directory)
        .env("XDG_CACHE_HOME", &cache_directory)
        .env("LIBSEAT_BACKEND", "logind")
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "start SOS compositor {}",
                options.compositor_executable.display()
            )
        })?;
    println!(
        "linux_system_session_component component=compositor pid={}",
        compositor.id()
    );
    processes.compositor = Some(compositor);
    wait_for_socket(
        &wayland_socket,
        "Wayland",
        options.startup_timeout,
        stopping,
        processes.compositor.as_mut().unwrap(),
    )?;
    wait_for_socket(
        &control_socket,
        "compositor control",
        options.startup_timeout,
        stopping,
        processes.compositor.as_mut().unwrap(),
    )?;
    wait_for_ready_file(
        &ready_file,
        options.startup_timeout,
        stopping,
        processes.compositor.as_mut().unwrap(),
    )?;

    let provider = Command::new(&options.provider_executable)
        .arg("--socket")
        .arg(&provider_socket)
        .arg("--state-file")
        .arg(&options.authority_file)
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "start provider/state authority {}",
                options.provider_executable.display()
            )
        })?;
    println!(
        "linux_system_session_component component=provider pid={}",
        provider.id()
    );
    processes.provider = Some(provider);
    wait_for_socket(
        &provider_socket,
        "provider/state",
        options.startup_timeout,
        stopping,
        processes.provider.as_mut().unwrap(),
    )?;
    let bootstrap = bootstrap_authority(
        &options.revision_root,
        &provider_socket,
        options.startup_timeout,
    )?;
    println!("linux_system_session_authority outcome={bootstrap:?}");

    let supervisor = Command::new(&options.supervisor_executable)
        .arg("serve")
        .arg("--root")
        .arg(&options.revision_root)
        .arg("--host-executable")
        .arg(&options.host_executable)
        .arg("--service-socket")
        .arg(&provider_socket)
        .arg("--host-arg")
        .arg("--service-socket")
        .arg("--host-arg")
        .arg(&provider_socket)
        .env("XDG_RUNTIME_DIR", &options.runtime_directory)
        .env("HOME", state_directory)
        .env("XDG_CACHE_HOME", &cache_directory)
        .env("WAYLAND_DISPLAY", wayland_display)
        .env("SOS_COMPOSITOR_CONTROL", &control_socket)
        .env_remove("SOS_COMPOSITOR_TOKEN")
        .env("SOS_COMPOSITOR_TOKEN_FILE", &options.shell_token_file)
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "start revision supervisor {}",
                options.supervisor_executable.display()
            )
        })?;
    println!(
        "linux_system_session_component component=supervisor pid={}",
        supervisor.id()
    );
    processes.supervisor = Some(supervisor);
    wait_for_socket(
        &supervisor_socket,
        "revision supervisor",
        options.startup_timeout,
        stopping,
        processes.supervisor.as_mut().unwrap(),
    )?;
    println!("linux_system_session_ready revision_id={current_revision} evidence=drm_page_flip");

    loop {
        if stopping.load(Ordering::Relaxed) {
            return Ok(());
        }
        for (name, child) in [
            ("compositor", processes.compositor.as_mut().unwrap()),
            ("provider", processes.provider.as_mut().unwrap()),
            ("supervisor", processes.supervisor.as_mut().unwrap()),
        ] {
            if let Some(status) = child.try_wait()? {
                bail!("system session component exited component={name} status={status}");
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_socket(
    path: &Path,
    description: &str,
    timeout: Duration,
    stopping: &AtomicBool,
    child: &mut Child,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.file_type().is_socket() {
                return Ok(());
            }
            bail!(
                "{description} readiness path is not a socket: {}",
                path.display()
            );
        }
        check_startup_progress(description, deadline, stopping, child)?;
        thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_ready_file(
    path: &Path,
    timeout: Duration,
    stopping: &AtomicBool,
    child: &mut Child,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match fs::read_to_string(path) {
            Ok(contents) if contents == "evidence=drm_page_flip\n" => return Ok(()),
            Ok(contents) if !contents.is_empty() => {
                bail!("unexpected compositor readiness evidence: {contents:?}")
            }
            Ok(_) | Err(_) => {}
        }
        check_startup_progress("compositor presentation", deadline, stopping, child)?;
        thread::sleep(POLL_INTERVAL);
    }
}

fn check_startup_progress(
    description: &str,
    deadline: Instant,
    stopping: &AtomicBool,
    child: &mut Child,
) -> Result<()> {
    if stopping.load(Ordering::Relaxed) {
        bail!("system session stop requested while waiting for {description}");
    }
    if let Some(status) = child.try_wait()? {
        bail!("{description} component exited during startup: {status}");
    }
    if Instant::now() >= deadline {
        bail!("timed out waiting for {description}");
    }
    Ok(())
}

fn remove_refused_stale_socket(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    };
    if !metadata.file_type().is_socket() {
        bail!(
            "supervisor control path is not a socket: {}",
            path.display()
        );
    }
    match UnixStream::connect(path) {
        Ok(_) => bail!(
            "a revision supervisor is already listening at {}",
            path.display()
        ),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            fs::remove_file(path)
                .with_context(|| format!("remove stale supervisor socket {}", path.display()))?;
            println!(
                "linux_system_session_recovered artifact=stale_supervisor_socket path={}",
                path.display()
            );
            Ok(())
        }
        Err(error) => Err(error).with_context(|| format!("probe {}", path.display())),
    }
}

fn shutdown_processes(options: &SystemSessionOptions, processes: &mut SessionProcesses) {
    if processes.supervisor.as_mut().is_some_and(child_is_running) {
        let _ = Command::new(&options.supervisor_executable)
            .arg("shutdown")
            .arg("--root")
            .arg(&options.revision_root)
            .stdout(Stdio::null())
            .status();
    }
    if processes.provider.as_mut().is_some_and(child_is_running) {
        let provider_socket = options.runtime_directory.join("provider-state.sock");
        let _ = shutdown_authority(&provider_socket, SHUTDOWN_TIMEOUT);
    }
    terminate_child("supervisor", &mut processes.supervisor);
    terminate_child("provider", &mut processes.provider);
    terminate_child("compositor", &mut processes.compositor);
}

fn child_is_running(child: &mut Child) -> bool {
    child.try_wait().ok().flatten().is_none()
}

fn terminate_child(name: &str, slot: &mut Option<Child>) {
    let Some(child) = slot.as_mut() else {
        return;
    };
    if !child_is_running(child) {
        let _ = child.wait();
        return;
    }
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
    }
    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    while Instant::now() < deadline {
        if !child_is_running(child) {
            let _ = child.wait();
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
    eprintln!(
        "linux_system_session_forced_stop component={name} pid={}",
        child.id()
    );
    let _ = child.kill();
    let _ = child.wait();
}
