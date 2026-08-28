use std::{
    ffi::OsString,
    fs, io,
    io::{BufReader, Write as _},
    os::unix::fs::MetadataExt as _,
    os::unix::fs::{FileTypeExt as _, PermissionsExt as _},
    os::unix::net::{UnixDatagram, UnixListener, UnixStream},
    os::unix::process::CommandExt as _,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::read_shell_token_file;
use experience_package::ExperienceId;
use nix::{
    sys::signal::{kill, Signal},
    sys::socket::{getsockopt, sockopt::PeerCredentials},
    unistd::{chown, Gid, Pid, Uid, User},
};
use revision_supervisor::{GraphStore, RevisionStore, STOCK_SHELL_EXPERIENCE_ID};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

use crate::{bootstrap_graph_authority, review_trusted_graph_grants, shutdown_authority};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const HOST_PROXY_DISCONNECT_GRACE: Duration = Duration::from_millis(250);
const SESSION_EXIT_REQUEST: &[u8] = b"logout\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceIdentity {
    pub name: String,
    pub uid: Uid,
    pub gid: Gid,
}

impl ServiceIdentity {
    pub fn resolve(name: &str) -> Result<Self> {
        let user = User::from_name(name)?
            .with_context(|| format!("Linux service account does not exist: {name}"))?;
        Ok(Self {
            name: name.to_owned(),
            uid: user.uid,
            gid: user.gid,
        })
    }

    pub fn current() -> Result<Self> {
        let uid = Uid::effective();
        if uid.is_root() {
            bail!("the selectable SOS login session must not run as root");
        }
        let user = User::from_uid(uid)?
            .with_context(|| format!("Linux login account does not exist for UID {uid}"))?;
        Ok(Self {
            name: user.name,
            uid,
            gid: Gid::effective(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionIdentityMode {
    IsolatedServices,
    SharedLoginUser,
}

#[derive(Clone, Debug)]
pub struct SystemSessionOptions {
    pub revision_root: PathBuf,
    pub runtime_directory: PathBuf,
    pub host_runtime_directory: PathBuf,
    pub host_home_directory: PathBuf,
    pub host_cache_directory: PathBuf,
    pub authority_file: PathBuf,
    pub shell_token_file: PathBuf,
    pub trusted_stock_revision: String,
    pub agent_socket: PathBuf,
    pub compositor_executable: PathBuf,
    pub provider_executable: PathBuf,
    pub supervisor_executable: PathBuf,
    pub host_executable: PathBuf,
    pub compositor_identity: ServiceIdentity,
    pub provider_identity: ServiceIdentity,
    pub supervisor_identity: ServiceIdentity,
    pub host_identity: ServiceIdentity,
    pub identity_mode: SessionIdentityMode,
    pub startup_timeout: Duration,
}

#[derive(Default)]
struct SessionProcesses {
    compositor: Option<Child>,
    provider: Option<Child>,
    supervisor: Option<Child>,
    host_launcher: Option<HostLauncher>,
}

pub fn run_system_session(options: SystemSessionOptions) -> Result<()> {
    validate_options(&options)?;
    let state_directory = options
        .revision_root
        .parent()
        .context("revision root must have a parent state directory")?;
    let registry = ProcessRegistry::new(state_directory.join("session-processes.json"));
    registry.reap_stale(&options)?;
    let stopping = Arc::new(AtomicBool::new(false));
    for signal in [SIGTERM, SIGINT, SIGHUP] {
        signal_hook::flag::register(signal, Arc::clone(&stopping))?;
    }

    let mut processes = SessionProcesses::default();
    let result = start_and_monitor(&options, &stopping, &mut processes, &registry);
    shutdown_processes(&options, &mut processes);
    registry.clear();
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
        (
            "experience host runtime directory",
            options.host_runtime_directory.as_path(),
        ),
        (
            "experience host home directory",
            options.host_home_directory.as_path(),
        ),
        (
            "experience host cache directory",
            options.host_cache_directory.as_path(),
        ),
        ("authority file", options.authority_file.as_path()),
        ("shell credential", options.shell_token_file.as_path()),
        ("agent socket", options.agent_socket.as_path()),
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
    let identities = [
        &options.compositor_identity,
        &options.provider_identity,
        &options.supervisor_identity,
        &options.host_identity,
    ];
    validate_identities(options.identity_mode, identities)
}

fn validate_identities(mode: SessionIdentityMode, identities: [&ServiceIdentity; 4]) -> Result<()> {
    match mode {
        SessionIdentityMode::IsolatedServices => {
            for (index, identity) in identities.iter().enumerate() {
                if identities[..index]
                    .iter()
                    .any(|other| other.uid == identity.uid)
                {
                    bail!(
                        "Linux component service identities must use distinct UIDs; {} reuses {}",
                        identity.name,
                        identity.uid
                    );
                }
            }
        }
        SessionIdentityMode::SharedLoginUser => {
            let current = ServiceIdentity::current()?;
            if identities
                .iter()
                .any(|identity| identity.uid != current.uid || identity.gid != current.gid)
            {
                bail!(
                    "selectable login-session components must all use the current UID {} and GID {}",
                    current.uid,
                    current.gid
                );
            }
        }
    }
    Ok(())
}

fn start_and_monitor(
    options: &SystemSessionOptions,
    stopping: &AtomicBool,
    processes: &mut SessionProcesses,
    registry: &ProcessRegistry,
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
        .revision_root
        .parent()
        .context("revision root must have a parent state directory")?;
    let compositor_cache = options.runtime_directory.join("cache-compositor");
    create_role_directory(&compositor_cache, &options.compositor_identity)?;
    match options.identity_mode {
        SessionIdentityMode::IsolatedServices => {
            create_role_directory(&options.host_cache_directory, &options.host_identity)?;
        }
        SessionIdentityMode::SharedLoginUser => {
            fs::create_dir_all(&options.host_cache_directory).with_context(|| {
                format!(
                    "create login user cache directory {}",
                    options.host_cache_directory.display()
                )
            })?;
        }
    }
    let compositor_token_file = options
        .runtime_directory
        .join(format!("credential-compositor-{}", std::process::id()));
    let host_token_file = options
        .runtime_directory
        .join(format!("credential-host-{}", std::process::id()));
    copy_role_credential(
        &options.shell_token_file,
        &compositor_token_file,
        &options.compositor_identity,
    )?;
    copy_role_credential(
        &options.shell_token_file,
        &host_token_file,
        &options.host_identity,
    )?;

    let store = RevisionStore::open(&options.revision_root)?;
    let stock_experience_id = ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if GraphStore::open(&options.revision_root)?
        .current(&stock_experience_id)?
        .is_none()
    {
        bail!("cannot boot the system session before its v4 graph is initialized");
    }
    let current_revision = recovery_revisions(&store)?.0;
    if current_revision.is_empty() {
        bail!("cannot boot the system session before its revision pointer is initialized");
    }
    let recovery_state_file = options.runtime_directory.join("recovery.json");
    let recovery_command_socket = options.runtime_directory.join("recovery-command.sock");
    let safe_mode_file = state_directory.join("safe-mode");
    let provider_disable_file = state_directory.join("providers-disabled");
    let appearance_capability_file = state_directory.join("appearance-write.capability");
    let provider_appearance_capability = if appearance_capability_file.exists() {
        let destination = options
            .runtime_directory
            .join(format!("credential-appearance-{}", std::process::id()));
        copy_role_credential(
            &appearance_capability_file,
            &destination,
            &options.provider_identity,
        )?;
        Some(destination)
    } else {
        None
    };
    let grant_capability_file = state_directory.join("grant-review.capability");
    let (provider_grant_capability, grant_capability) = if grant_capability_file.exists() {
        let capability = fs::read_to_string(&grant_capability_file)?;
        let capability = capability.trim_end_matches(['\r', '\n']).to_owned();
        if capability.is_empty() || capability.len() > 256 {
            bail!("grant-review capability must contain 1 to 256 bytes");
        }
        let destination = options
            .runtime_directory
            .join(format!("credential-grant-review-{}", std::process::id()));
        copy_role_credential(
            &grant_capability_file,
            &destination,
            &options.provider_identity,
        )?;
        (Some(destination), Some(capability))
    } else {
        (None, None)
    };
    let recovery_socket = UnixDatagram::bind(&recovery_command_socket)
        .with_context(|| format!("bind {}", recovery_command_socket.display()))?;
    fs::set_permissions(&recovery_command_socket, fs::Permissions::from_mode(0o660))?;
    recovery_socket.set_nonblocking(true)?;
    let session_exit_socket_path = options.runtime_directory.join("session-exit.sock");
    let session_exit_socket = if options.identity_mode == SessionIdentityMode::SharedLoginUser {
        let socket = UnixDatagram::bind(&session_exit_socket_path)
            .with_context(|| format!("bind {}", session_exit_socket_path.display()))?;
        fs::set_permissions(&session_exit_socket_path, fs::Permissions::from_mode(0o600))?;
        socket.set_nonblocking(true)?;
        Some(socket)
    } else {
        None
    };
    write_recovery_status(
        &recovery_state_file,
        &store,
        "STARTING SYSTEM SESSION",
        "",
        safe_mode_file.exists(),
        provider_disable_file.exists(),
    )?;
    let supervisor_socket = options.revision_root.join("run/supervisor.sock");
    let stale_supervisor_socket = inspect_refused_stale_socket(&supervisor_socket)?;

    let wayland_display = "wayland-sos";
    let wayland_socket = options.runtime_directory.join(wayland_display);
    let control_socket = options.runtime_directory.join("compositor-control.sock");
    let ready_file = options.runtime_directory.join("compositor-ready");
    let provider_socket = options.runtime_directory.join("provider-state.sock");
    let host_launcher_socket = options.runtime_directory.join("host-launcher.sock");
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

    let mut compositor_command =
        role_command(&options.compositor_executable, &options.compositor_identity);
    if session_exit_socket.is_some() {
        compositor_command.env("SOS_SESSION_EXIT_SOCKET", &session_exit_socket_path);
    }
    let compositor = compositor_command
        .arg("--backend")
        .arg("drm")
        .arg("--socket")
        .arg(wayland_display)
        .arg("--control-socket")
        .arg(&control_socket)
        .arg("--shell-token-file")
        .arg(&compositor_token_file)
        .arg("--ready-file")
        .arg(&ready_file)
        .env("XDG_RUNTIME_DIR", &options.runtime_directory)
        .env("HOME", &compositor_cache)
        .env("XDG_CACHE_HOME", &compositor_cache)
        .env("LIBSEAT_BACKEND", "logind")
        .env("SOS_RECOVERY_STATE_FILE", &recovery_state_file)
        .env("SOS_RECOVERY_COMMAND_SOCKET", &recovery_command_socket)
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
    registry.record(
        "compositor",
        &options.compositor_executable,
        compositor.id(),
    )?;
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
    set_shared_socket_permissions(&wayland_socket)?;
    set_shared_socket_permissions(&control_socket)?;

    prepare_role_file_parent(&options.authority_file, &options.provider_identity)?;
    let mut provider_command =
        role_command(&options.provider_executable, &options.provider_identity);
    if let Some(appearance_capability_file) = &provider_appearance_capability {
        provider_command
            .arg("--appearance-capability-file")
            .arg(&appearance_capability_file);
    }
    if let Some(grant_capability_file) = &provider_grant_capability {
        provider_command
            .arg("--grant-capability-file")
            .arg(grant_capability_file);
    }
    let provider = provider_command
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
    registry.record("provider", &options.provider_executable, provider.id())?;
    processes.provider = Some(provider);
    wait_for_socket(
        &provider_socket,
        "provider/state",
        options.startup_timeout,
        stopping,
        processes.provider.as_mut().unwrap(),
    )?;
    let capability = grant_capability
        .as_deref()
        .context("v4 graph mode requires a grant-review capability")?;
    let graph_bootstrap = bootstrap_graph_authority(
        &options.revision_root,
        &stock_experience_id,
        &provider_socket,
        options.startup_timeout,
    )?;
    println!(
        "linux_system_session_graph_authority experience_id={stock_experience_id} outcome={graph_bootstrap:?}"
    );
    let reviewed = review_trusted_graph_grants(
        &options.revision_root,
        &stock_experience_id,
        options.trusted_stock_revision.as_str(),
        &provider_socket,
        capability,
        options.startup_timeout,
    )?;
    println!("linux_system_session_grants experience_id={stock_experience_id} reviewed={reviewed}");

    processes.host_launcher = Some(HostLauncher::start(
        &host_launcher_socket,
        HostLaunchSpec {
            executable: options.host_executable.clone(),
            args: vec![
                "--service-socket".into(),
                provider_socket.clone().into_os_string(),
                "--agent-socket".into(),
                options.agent_socket.clone().into_os_string(),
            ],
            identity: options.host_identity.clone(),
            runtime_directory: options.host_runtime_directory.clone(),
            home_directory: options.host_home_directory.clone(),
            cache_directory: options.host_cache_directory.clone(),
            // The host may use the login user's XDG runtime directory so
            // providers and launched applications can reach PipeWire,
            // portals, and other user-session services. Keep the compositor
            // socket private and address it explicitly.
            wayland_display: wayland_socket.as_os_str().to_owned(),
            control_socket: control_socket.clone(),
            token_file: host_token_file,
            safe_mode_file: safe_mode_file.clone(),
            provider_disable_file: provider_disable_file.clone(),
            supervisor_uid: options.supervisor_identity.uid,
            registry: registry.clone(),
        },
    )?);
    let mut supervisor_command =
        role_command(&options.supervisor_executable, &options.supervisor_identity);
    supervisor_command
        .arg("serve")
        .arg("--root")
        .arg(&options.revision_root)
        .arg("--host-executable")
        .arg(std::env::current_exe().context("resolve session executable for host proxy")?)
        .arg("--service-socket")
        .arg(&provider_socket)
        .arg("--host-arg")
        .arg("host-proxy")
        .arg("--host-arg")
        .arg("--launcher-socket")
        .arg("--host-arg")
        .arg(&host_launcher_socket);
    supervisor_command
        .arg("--root-experience")
        .arg(STOCK_SHELL_EXPERIENCE_ID);
    let supervisor = supervisor_command
        .env("XDG_RUNTIME_DIR", &options.runtime_directory)
        .env("HOME", state_directory)
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
    registry.record(
        "supervisor",
        &options.supervisor_executable,
        supervisor.id(),
    )?;
    processes.supervisor = Some(supervisor);
    wait_for_fresh_socket(
        &supervisor_socket,
        stale_supervisor_socket,
        "revision supervisor",
        options.startup_timeout,
        stopping,
        processes.supervisor.as_mut().unwrap(),
    )?;
    println!(
        "linux_system_session_ready revision_id={current_revision} graph_protocol=v4 evidence=drm_page_flip"
    );
    write_recovery_status(
        &recovery_state_file,
        &store,
        "RUNNING",
        "",
        safe_mode_file.exists(),
        provider_disable_file.exists(),
    )?;
    let mut observed_recovery_status = recovery_status_key(
        &store,
        safe_mode_file.exists(),
        provider_disable_file.exists(),
    )?;

    loop {
        if stopping.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(socket) = &session_exit_socket {
            if take_session_exit_request(socket)? {
                println!("linux_login_session_stopped reason=user_logout");
                return Ok(());
            }
        }
        for (name, child) in [
            ("compositor", processes.compositor.as_mut().unwrap()),
            ("provider", processes.provider.as_mut().unwrap()),
            ("supervisor", processes.supervisor.as_mut().unwrap()),
        ] {
            if let Some(status) = child.try_wait()? {
                if name == "compositor"
                    && status.success()
                    && options.identity_mode == SessionIdentityMode::SharedLoginUser
                {
                    println!("linux_login_session_stopped reason=user_logout");
                    return Ok(());
                }
                bail!("system session component exited component={name} status={status}");
            }
            if !process_is_running(child.id()) {
                bail!(
                    "system session component disappeared component={name} pid={}",
                    child.id()
                );
            }
        }
        handle_recovery_action(
            &recovery_socket,
            options,
            &store,
            &recovery_state_file,
            &safe_mode_file,
            &provider_disable_file,
        )?;
        let next_recovery_status_key = recovery_status_key(
            &store,
            safe_mode_file.exists(),
            provider_disable_file.exists(),
        )?;
        if next_recovery_status_key != observed_recovery_status {
            write_recovery_status(
                &recovery_state_file,
                &store,
                "RUNNING",
                "",
                next_recovery_status_key.2,
                next_recovery_status_key.3,
            )?;
            observed_recovery_status = next_recovery_status_key;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn take_session_exit_request(socket: &UnixDatagram) -> Result<bool> {
    let mut buffer = [0_u8; 32];
    match socket.recv(&mut buffer) {
        Ok(size) if &buffer[..size] == SESSION_EXIT_REQUEST => Ok(true),
        Ok(size) => bail!(
            "invalid selectable login-session exit request: {:?}",
            String::from_utf8_lossy(&buffer[..size])
        ),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
        Err(error) => Err(error).context("receive selectable login-session exit request"),
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

fn role_command(executable: &Path, identity: &ServiceIdentity) -> Command {
    let mut command = Command::new(executable);
    if identity.gid != Gid::effective() {
        command.gid(identity.gid.as_raw());
    }
    if identity.uid != Uid::effective() {
        command.uid(identity.uid.as_raw());
    }
    // The lifecycle owner needs four narrowly bounded capabilities to create
    // role-owned files, change child credentials, and reap an abandoned tree.
    // Ambient capabilities otherwise survive exec when the compositor child
    // retains the login UID, so every component explicitly starts with empty
    // effective/permitted/inheritable/ambient sets.
    unsafe {
        command.pre_exec(clear_process_capabilities);
    }
    command
}

fn clear_process_capabilities() -> io::Result<()> {
    const LINUX_CAPABILITY_VERSION_3: u32 = 0x2008_0522;
    #[repr(C)]
    struct CapabilityHeader {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CapabilityData {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    let ambient = unsafe {
        nix::libc::prctl(
            nix::libc::PR_CAP_AMBIENT,
            nix::libc::PR_CAP_AMBIENT_CLEAR_ALL,
            0,
            0,
            0,
        )
    };
    if ambient != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut header = CapabilityHeader {
        version: LINUX_CAPABILITY_VERSION_3,
        pid: 0,
    };
    let data = [CapabilityData {
        effective: 0,
        permitted: 0,
        inheritable: 0,
    }; 2];
    let result = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_capset,
            std::ptr::from_mut(&mut header),
            data.as_ptr(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn create_role_directory(path: &Path, identity: &ServiceIdentity) -> Result<()> {
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.is_dir()
            && metadata.uid() == identity.uid.as_raw()
            && metadata.gid() == identity.gid.as_raw()
            && metadata.permissions().mode() & 0o777 == 0o700
        {
            return Ok(());
        }
    }
    fs::create_dir_all(path)
        .with_context(|| format!("create role directory {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    chown(path, Some(identity.uid), Some(identity.gid))
        .with_context(|| format!("assign {} to {}", path.display(), identity.name))?;
    Ok(())
}

fn prepare_role_file_parent(path: &Path, identity: &ServiceIdentity) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("role file has no parent: {}", path.display()))?;
    create_role_directory(parent, identity)
}

fn copy_role_credential(source: &Path, target: &Path, identity: &ServiceIdentity) -> Result<()> {
    let token = read_shell_token_file(source)?;
    fs::write(target, token.as_bytes())?;
    fs::set_permissions(target, fs::Permissions::from_mode(0o400))?;
    chown(target, Some(identity.uid), Some(identity.gid))
        .with_context(|| format!("assign credential to {}", identity.name))?;
    Ok(())
}

fn set_shared_socket_permissions(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o660))
        .with_context(|| format!("set shared socket permissions on {}", path.display()))
}

#[derive(Clone)]
struct ProcessRegistry {
    path: PathBuf,
    records: Arc<Mutex<Vec<ProcessRecord>>>,
}

#[derive(Clone, Debug)]
struct ProcessRecord {
    component: String,
    executable: PathBuf,
    pid: u32,
    start_ticks: u64,
}

impl ProcessRegistry {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn record(&self, component: &str, executable: &Path, pid: u32) -> Result<()> {
        let record = ProcessRecord {
            component: component.to_owned(),
            executable: fs::canonicalize(executable)?,
            pid,
            start_ticks: process_start_ticks(pid)
                .with_context(|| format!("read start time for {component} PID {pid}"))?,
        };
        let mut records = self.records.lock().expect("process registry poisoned");
        records.push(record);
        self.persist(&records)
    }

    fn remove(&self, pid: u32) {
        let mut records = self.records.lock().expect("process registry poisoned");
        records.retain(|record| record.pid != pid);
        let _ = self.persist(&records);
    }

    fn persist(&self, records: &[ProcessRecord]) -> Result<()> {
        let value = records
            .iter()
            .map(|record| {
                serde_json::json!({
                    "component": record.component,
                    "executable": record.executable,
                    "pid": record.pid,
                    "start_ticks": record.start_ticks,
                })
            })
            .collect::<Vec<_>>();
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec(&value)?)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }

    fn clear(&self) {
        self.records
            .lock()
            .expect("process registry poisoned")
            .clear();
        let _ = fs::remove_file(&self.path);
    }

    fn reap_stale(&self, options: &SystemSessionOptions) -> Result<()> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let allowed = [
            &options.compositor_executable,
            &options.provider_executable,
            &options.supervisor_executable,
            &options.host_executable,
        ]
        .into_iter()
        .map(fs::canonicalize)
        .collect::<std::result::Result<Vec<_>, _>>()?;
        let values: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
            .with_context(|| format!("decode stale process registry {}", self.path.display()))?;
        let mut stale = Vec::new();
        for value in values {
            let Some(pid) = value.get("pid").and_then(|value| value.as_u64()) else {
                continue;
            };
            let Ok(pid) = u32::try_from(pid) else {
                continue;
            };
            let Some(start_ticks) = value.get("start_ticks").and_then(|value| value.as_u64())
            else {
                continue;
            };
            let Some(executable) = value.get("executable").and_then(|value| value.as_str()) else {
                continue;
            };
            let executable = PathBuf::from(executable);
            if !allowed.contains(&executable)
                || process_start_ticks(pid).ok() != Some(start_ticks)
                || fs::read_link(format!("/proc/{pid}/exe")).ok() != Some(executable.clone())
            {
                continue;
            }
            stale.push((pid, start_ticks, executable));
        }
        for (pid, _, _) in &stale {
            kill(Pid::from_raw(i32::try_from(*pid)?), Signal::SIGTERM)
                .with_context(|| format!("terminate stale SOS process {pid}"))?;
        }
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        while Instant::now() < deadline
            && stale
                .iter()
                .any(|(pid, ticks, _)| process_start_ticks(*pid).ok() == Some(*ticks))
        {
            thread::sleep(POLL_INTERVAL);
        }
        for (pid, ticks, _) in &stale {
            if process_start_ticks(*pid).ok() == Some(*ticks) {
                kill(Pid::from_raw(i32::try_from(*pid)?), Signal::SIGKILL)
                    .with_context(|| format!("kill stale SOS process {pid}"))?;
            }
        }
        println!(
            "linux_system_session_recovered artifact=abandoned_process_registry reaped={} cleanup={}",
            stale.len(),
            if stale.is_empty() {
                "logind_or_kernel"
            } else {
                "session_owner"
            }
        );
        self.clear();
        Ok(())
    }
}

fn process_start_ticks(pid: u32) -> Result<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let close = stat.rfind(") ").context("invalid /proc stat comm field")?;
    stat[close + 2..]
        .split_whitespace()
        .nth(19)
        .context("missing /proc start-time field")?
        .parse()
        .context("invalid /proc start-time field")
}

fn process_is_running(pid: u32) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let Some(close) = stat.rfind(") ") else {
        return false;
    };
    stat[close + 2..]
        .split_whitespace()
        .next()
        .is_some_and(|state| state != "Z" && state != "X")
}

struct HostLaunchSpec {
    executable: PathBuf,
    args: Vec<OsString>,
    identity: ServiceIdentity,
    runtime_directory: PathBuf,
    home_directory: PathBuf,
    cache_directory: PathBuf,
    wayland_display: OsString,
    control_socket: PathBuf,
    token_file: PathBuf,
    safe_mode_file: PathBuf,
    provider_disable_file: PathBuf,
    supervisor_uid: Uid,
    registry: ProcessRegistry,
}

struct HostLauncher {
    path: PathBuf,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl HostLauncher {
    fn start(path: &Path, spec: HostLaunchSpec) -> Result<Self> {
        let listener = UnixListener::bind(path)
            .with_context(|| format!("bind host launcher {}", path.display()))?;
        listener.set_nonblocking(true)?;
        set_shared_socket_permissions(path)?;
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_stopping = Arc::clone(&stopping);
        let spec = Arc::new(spec);
        let thread = thread::Builder::new()
            .name("sos-host-launcher".into())
            .spawn(move || {
                while !thread_stopping.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let credentials = match getsockopt(&stream, PeerCredentials) {
                                Ok(credentials) => credentials,
                                Err(error) => {
                                    eprintln!("linux_host_launcher_rejected reason=peer_credentials error={error}");
                                    continue;
                                }
                            };
                            if credentials.uid() != spec.supervisor_uid.as_raw() {
                                eprintln!(
                                    "linux_host_launcher_rejected reason=wrong_uid uid={}",
                                    credentials.uid()
                                );
                                continue;
                            }
                            let connection_spec = Arc::clone(&spec);
                            let _ = thread::Builder::new()
                                .name("sos-isolated-host".into())
                                .spawn(move || {
                                    if let Err(error) = launch_host(stream, &connection_spec) {
                                        eprintln!(
                                            "linux_host_launcher_failed error={error:#}"
                                        );
                                    }
                                });
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(POLL_INTERVAL);
                        }
                        Err(error) => {
                            eprintln!("linux_host_launcher_failed error={error}");
                            return;
                        }
                    }
                }
            })?;
        Ok(Self {
            path: path.to_path_buf(),
            stopping,
            thread: Some(thread),
        })
    }
}

impl Drop for HostLauncher {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        let _ = UnixStream::connect(&self.path);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

fn launch_host(mut stream: UnixStream, spec: &HostLaunchSpec) -> Result<()> {
    let mut command = role_command(&spec.executable, &spec.identity);
    let mut child = command
        .args(&spec.args)
        .env("XDG_RUNTIME_DIR", &spec.runtime_directory)
        .env("HOME", &spec.home_directory)
        .env("XDG_CACHE_HOME", &spec.cache_directory)
        .env("WAYLAND_DISPLAY", &spec.wayland_display)
        .env("SOS_COMPOSITOR_CONTROL", &spec.control_socket)
        .env_remove("SOS_COMPOSITOR_TOKEN")
        .env("SOS_COMPOSITOR_TOKEN_FILE", &spec.token_file)
        .env("SOS_SAFE_MODE_FILE", &spec.safe_mode_file)
        .env("SOS_PROVIDER_DISABLE_FILE", &spec.provider_disable_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("launch isolated experience host as {}", spec.identity.name))?;
    let pid = child.id();
    let host_pid = Pid::from_raw(i32::try_from(pid).context("host PID exceeds i32")?);
    spec.registry.record("host", &spec.executable, pid)?;
    println!(
        "linux_system_session_component component=host pid={pid} uid={}",
        spec.identity.uid
    );
    let mut child_input = child.stdin.take().context("host stdin was not piped")?;
    let mut child_output = child.stdout.take().context("host stdout was not piped")?;
    let input_stream = stream.try_clone()?;
    let input = thread::spawn(move || {
        let result = pump_lines(BufReader::new(input_stream), &mut child_input);
        // EOF means the supervisor-side proxy disappeared. The proxy is only
        // transport; allow an already-delivered Shutdown request a short grace
        // period, then reap a host that would otherwise overlap the replacement
        // the supervisor is about to launch.
        let deadline = Instant::now() + HOST_PROXY_DISCONNECT_GRACE;
        while process_is_running(pid) && Instant::now() < deadline {
            thread::sleep(POLL_INTERVAL);
        }
        if process_is_running(pid) {
            let _ = kill(host_pid, Signal::SIGKILL);
        }
        result
    });
    let output_result = pump_lines(BufReader::new(&mut child_output), &mut stream);
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let _ = input.join();
    let status = child.wait()?;
    spec.registry.remove(pid);
    output_result?;
    if !status.success() {
        bail!("isolated experience host exited with {status}");
    }
    Ok(())
}

pub fn run_host_proxy(launcher_socket: PathBuf) -> Result<()> {
    let mut stream = UnixStream::connect(&launcher_socket)
        .with_context(|| format!("connect host launcher {}", launcher_socket.display()))?;
    let mut input_stream = stream.try_clone()?;
    let _input = thread::spawn(move || pump_lines(io::stdin().lock(), &mut input_stream));
    pump_lines(BufReader::new(&mut stream), &mut io::stdout().lock())?;
    io::stdout().flush()?;
    Ok(())
}

fn pump_lines(mut reader: impl io::BufRead, mut writer: impl io::Write) -> io::Result<u64> {
    let mut transferred = 0_u64;
    let mut line = Vec::new();
    loop {
        line.clear();
        let size = reader.read_until(b'\n', &mut line)?;
        if size == 0 {
            return Ok(transferred);
        }
        writer.write_all(&line)?;
        writer.flush()?;
        transferred += u64::try_from(size).unwrap_or(u64::MAX);
    }
}

fn handle_recovery_action(
    socket: &UnixDatagram,
    options: &SystemSessionOptions,
    store: &RevisionStore,
    state_file: &Path,
    safe_mode_file: &Path,
    provider_disable_file: &Path,
) -> Result<()> {
    let mut bytes = [0_u8; 4096];
    let size = match socket.recv(&mut bytes) {
        Ok(size) => size,
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let action = serde_json::from_slice::<serde_json::Value>(&bytes[..size])
        .ok()
        .and_then(|value| value.get("action")?.as_str().map(str::to_owned));
    let Some(action) = action else {
        eprintln!("linux_recovery_rejected reason=invalid_command");
        return Ok(());
    };
    write_recovery_status(
        state_file,
        store,
        &format!("APPLYING {}", action.replace('_', " ").to_uppercase()),
        "",
        safe_mode_file.exists(),
        provider_disable_file.exists(),
    )?;
    let result = match action.as_str() {
        "restart" => restart_host(options),
        "rollback" => (|| -> Result<()> {
            let stock = ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let previous_graph = GraphStore::open(&options.revision_root)?
                .previous(&stock)?
                .context("no previous experience graph is available")?
                .0;
            let status = Command::new(&options.supervisor_executable)
                .arg("activate-graph")
                .arg("--root")
                .arg(&options.revision_root)
                .arg("--graph")
                .arg(previous_graph)
                .status()?;
            status
                .success()
                .then_some(())
                .context("supervisor rejected recovery graph rollback")
        })(),
        "safe_mode" => (|| -> Result<()> {
            toggle_flag(safe_mode_file)?;
            restart_host(options)
        })(),
        "disable_providers" => (|| -> Result<()> {
            toggle_flag(provider_disable_file)?;
            restart_host(options)
        })(),
        _ => {
            eprintln!("linux_recovery_rejected action={action} reason=unsupported");
            return Ok(());
        }
    };
    match result {
        Ok(()) => {
            println!("linux_recovery_action_completed action={action}");
            write_recovery_status(
                state_file,
                store,
                "RUNNING",
                "",
                safe_mode_file.exists(),
                provider_disable_file.exists(),
            )?;
        }
        Err(error) => {
            eprintln!("linux_recovery_action_failed action={action} error={error:#}");
            write_recovery_status(
                state_file,
                store,
                "ACTION FAILED",
                &error.to_string(),
                safe_mode_file.exists(),
                provider_disable_file.exists(),
            )?;
        }
    }
    Ok(())
}

fn restart_host(options: &SystemSessionOptions) -> Result<()> {
    let status = Command::new(&options.supervisor_executable)
        .arg("restart")
        .arg("--root")
        .arg(&options.revision_root)
        .status()?;
    status
        .success()
        .then_some(())
        .context("supervisor rejected recovery restart")
}

fn toggle_flag(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    } else {
        fs::write(path, b"enabled\n")?;
    }
    Ok(())
}

fn write_recovery_status(
    path: &Path,
    store: &RevisionStore,
    progress: &str,
    failure_reason: &str,
    safe_mode: bool,
    providers_disabled: bool,
) -> Result<()> {
    let (current_revision, previous_revision, _, _) =
        recovery_status_key(store, safe_mode, providers_disabled)?;
    let value = serde_json::json!({
        "current_revision": current_revision,
        "previous_revision": previous_revision,
        "failure_reason": failure_reason,
        "progress": progress,
        "safe_mode": safe_mode,
        "providers_disabled": providers_disabled,
    });
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec(&value)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn recovery_status_key(
    store: &RevisionStore,
    safe_mode: bool,
    providers_disabled: bool,
) -> Result<(String, String, bool, bool)> {
    let (current_revision, previous_revision) = recovery_revisions(store)?;
    Ok((
        current_revision,
        previous_revision,
        safe_mode,
        providers_disabled,
    ))
}

fn recovery_revisions(store: &RevisionStore) -> Result<(String, String)> {
    let stock = ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let graphs = GraphStore::open(store.root())?;
    let revision =
        |graph: experience_package::ResolvedGraph| graph.nodes[&graph.root].revision_id.to_string();
    Ok((
        graphs
            .current(&stock)?
            .map(|(_, graph)| revision(graph))
            .unwrap_or_default(),
        graphs
            .previous(&stock)?
            .map(|(_, graph)| revision(graph))
            .unwrap_or_default(),
    ))
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

fn inspect_refused_stale_socket(path: &Path) -> Result<Option<(u64, u64)>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
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
            println!(
                "linux_system_session_detected artifact=stale_supervisor_socket path={} cleanup_owner=supervisor",
                path.display()
            );
            Ok(Some((metadata.dev(), metadata.ino())))
        }
        Err(error) => Err(error).with_context(|| format!("probe {}", path.display())),
    }
}

fn wait_for_fresh_socket(
    path: &Path,
    stale_identity: Option<(u64, u64)>,
    description: &str,
    timeout: Duration,
    stopping: &AtomicBool,
    child: &mut Child,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(metadata) = fs::metadata(path) {
            let identity = (metadata.dev(), metadata.ino());
            if metadata.file_type().is_socket() && Some(identity) != stale_identity {
                return Ok(());
            }
        }
        check_startup_progress(description, deadline, stopping, child)?;
        thread::sleep(POLL_INTERVAL);
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
    // The supervisor may reconnect its host proxy while shutting down. Keep
    // the launcher socket available until the supervisor has actually exited.
    drop(processes.host_launcher.take());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn identity(name: &str, uid: u32) -> ServiceIdentity {
        ServiceIdentity {
            name: name.into(),
            uid: Uid::from_raw(uid),
            gid: Gid::from_raw(uid),
        }
    }

    #[test]
    fn isolated_mode_rejects_a_reused_uid() {
        let compositor = identity("compositor", 10_001);
        let provider = identity("provider", 10_002);
        let supervisor = identity("supervisor", 10_003);
        let host = identity("host", 10_002);
        let error = validate_identities(
            SessionIdentityMode::IsolatedServices,
            [&compositor, &provider, &supervisor, &host],
        )
        .unwrap_err();
        assert!(error.to_string().contains("must use distinct UIDs"));
    }

    #[test]
    fn shared_mode_accepts_only_the_current_login_identity() {
        if Uid::effective().is_root() {
            return;
        }
        let current = ServiceIdentity::current().unwrap();
        validate_identities(
            SessionIdentityMode::SharedLoginUser,
            [&current, &current, &current, &current],
        )
        .unwrap();

        let other = identity("other", current.uid.as_raw().saturating_add(1));
        let error = validate_identities(
            SessionIdentityMode::SharedLoginUser,
            [&current, &current, &current, &other],
        )
        .unwrap_err();
        assert!(error.to_string().contains("must all use the current UID"));
    }

    #[test]
    fn lifecycle_owner_consumes_one_logout_request() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("session-exit.sock");
        let receiver = UnixDatagram::bind(&path).unwrap();
        receiver.set_nonblocking(true).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        sender.send_to(SESSION_EXIT_REQUEST, &path).unwrap();

        assert!(take_session_exit_request(&receiver).unwrap());
        assert!(!take_session_exit_request(&receiver).unwrap());
    }

    #[test]
    fn recovery_status_uses_graph_history_in_v4_mode() {
        let temporary = tempfile::tempdir().unwrap();
        let store = RevisionStore::open(temporary.path()).unwrap();
        let reference = revision_supervisor::install_reference_composition(&store).unwrap();
        let stock = ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID).unwrap();
        let graphs = GraphStore::open(store.root()).unwrap();
        let initial_graph = revision_supervisor::GraphResolver::new(store.clone())
            .resolve(
                &reference.dashboard_revision,
                &experience_package::ExportId::parse("main").unwrap(),
            )
            .unwrap();
        let initial_graph_id = graphs.install(&initial_graph).unwrap();
        graphs.set_current(&stock, &initial_graph_id).unwrap();
        let current = store.verify(&reference.dashboard_revision).unwrap();
        let package = current.package;
        let source = fs::read(current.directory.join(&current.manifest.source.path)).unwrap();
        let candidate = store
            .install_package(revision_supervisor::RevisionPackageInput {
                revision: revision_supervisor::RevisionInput {
                    source: [source, b"\n-- recovery candidate\n".to_vec()].concat(),
                    state: serde_json::json!({}),
                    schema_version: 1,
                    experience_api_version: 4,
                    assets: Vec::new(),
                },
                package,
            })
            .unwrap()
            .manifest
            .revision_id;
        let graph = revision_supervisor::GraphResolver::new(store.clone())
            .resolve(
                &candidate,
                &experience_package::ExportId::parse("main").unwrap(),
            )
            .unwrap();
        let graph_id = graphs.install(&graph).unwrap();
        graphs.set_current(&stock, &graph_id).unwrap();

        assert_eq!(
            recovery_revisions(&store).unwrap(),
            (candidate, reference.dashboard_revision)
        );
    }

    #[test]
    fn graceful_host_shutdown_wins_the_proxy_disconnect_grace() {
        let temporary = tempfile::tempdir().unwrap();
        let spec = HostLaunchSpec {
            executable: PathBuf::from("/usr/bin/bash"),
            args: vec![
                "-c".into(),
                "read -r _; exit 0".into(),
                "graceful-host-fixture".into(),
            ],
            identity: ServiceIdentity::current().unwrap(),
            runtime_directory: temporary.path().to_path_buf(),
            home_directory: temporary.path().to_path_buf(),
            cache_directory: temporary.path().to_path_buf(),
            wayland_display: temporary.path().join("wayland-test").into_os_string(),
            control_socket: temporary.path().join("control.sock"),
            token_file: temporary.path().join("token"),
            safe_mode_file: temporary.path().join("safe-mode"),
            provider_disable_file: temporary.path().join("providers-disabled"),
            supervisor_uid: Uid::effective(),
            registry: ProcessRegistry::new(temporary.path().join("registry.json")),
        };
        let (launcher_stream, mut proxy_stream) = UnixStream::pair().unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = done_tx.send(launch_host(launcher_stream, &spec));
        });

        proxy_stream.write_all(b"shutdown\n").unwrap();
        drop(proxy_stream);

        done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("graceful host launcher remained blocked")
            .expect("graceful host exit was classified as a failure");
    }

    #[test]
    fn proxy_disconnect_reaps_the_actual_isolated_host() {
        let temporary = tempfile::tempdir().unwrap();
        let pid_file = temporary.path().join("host.pid");
        let executable = PathBuf::from("/usr/bin/bash");
        let current = ServiceIdentity::current().unwrap();
        let spec = HostLaunchSpec {
            executable,
            args: vec![
                "-c".into(),
                "echo $$ > \"$1\"; trap '' TERM; while :; do :; done".into(),
                "isolated-host-fixture".into(),
                pid_file.clone().into_os_string(),
            ],
            identity: current,
            runtime_directory: temporary.path().to_path_buf(),
            home_directory: temporary.path().to_path_buf(),
            cache_directory: temporary.path().to_path_buf(),
            wayland_display: temporary.path().join("wayland-test").into_os_string(),
            control_socket: temporary.path().join("control.sock"),
            token_file: temporary.path().join("token"),
            safe_mode_file: temporary.path().join("safe-mode"),
            provider_disable_file: temporary.path().join("providers-disabled"),
            supervisor_uid: Uid::effective(),
            registry: ProcessRegistry::new(temporary.path().join("registry.json")),
        };
        let (launcher_stream, proxy_stream) = UnixStream::pair().unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = done_tx.send(launch_host(launcher_stream, &spec));
        });

        let deadline = Instant::now() + Duration::from_secs(2);
        while !pid_file.exists() && Instant::now() < deadline {
            thread::sleep(POLL_INTERVAL);
        }
        let pid: u32 = fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(process_is_running(pid));

        drop(proxy_stream);
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("host launcher remained blocked after proxy disconnect");
        assert!(result.is_err());
        assert!(!process_is_running(pid));
    }
}
