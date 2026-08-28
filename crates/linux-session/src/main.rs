use std::{
    collections::BTreeMap, env, fs, os::unix::fs::PermissionsExt as _, path::PathBuf,
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use sos_linux_session::{
    bootstrap_graph_authority, review_revision_grants, review_trusted_graph_grants, run_host_proxy,
    run_system_session, shutdown_authority, GraphBootstrapOutcome, ServiceIdentity,
    SessionIdentityMode, SystemSessionOptions,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("linux_session_failed error={error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().context(usage())?;
    let options = Options::parse(arguments.collect())?;
    let timeout = Duration::from_millis(
        options
            .optional("--timeout-ms")
            .unwrap_or("5000")
            .parse()
            .context("--timeout-ms must be an integer")?,
    );
    match command.as_str() {
        "bootstrap-graph" => {
            options.ensure_only(&["--root", "--experience", "--service-socket", "--timeout-ms"])?;
            let root = PathBuf::from(options.required("--root")?);
            let experience_id =
                experience_package::ExperienceId::parse(options.required("--experience")?)?;
            let service_socket = PathBuf::from(options.required("--service-socket")?);
            match bootstrap_graph_authority(&root, &experience_id, &service_socket, timeout)? {
                GraphBootstrapOutcome::Initialized {
                    transaction_id,
                    graph_id,
                    experience_count,
                } => println!(
                    "graph_authority_initialized transaction_id={transaction_id} graph_id={graph_id} experience_count={experience_count}"
                ),
                GraphBootstrapOutcome::AlreadyBound { graph_id } => {
                    println!("graph_authority_already_bound graph_id={graph_id}")
                }
            }
        }
        "shutdown" => {
            options.ensure_only(&["--service-socket", "--timeout-ms"])?;
            let service_socket = PathBuf::from(options.required("--service-socket")?);
            shutdown_authority(&service_socket, timeout)?;
        }
        "review-grants" => {
            options.ensure_only(&[
                "--root",
                "--revision",
                "--service-socket",
                "--capability-file",
                "--timeout-ms",
            ])?;
            let capability_path = PathBuf::from(options.required("--capability-file")?);
            let capability = read_private_capability(&capability_path)?;
            let decision = review_revision_grants(
                PathBuf::from(options.required("--root")?).as_path(),
                options.required("--revision")?,
                PathBuf::from(options.required("--service-socket")?).as_path(),
                &capability,
                timeout,
            )?;
            println!("{}", serde_json::to_string(&decision)?);
        }
        "review-graph-grants" => {
            options.ensure_only(&[
                "--root",
                "--experience",
                "--revision",
                "--service-socket",
                "--capability-file",
                "--timeout-ms",
            ])?;
            let capability_path = PathBuf::from(options.required("--capability-file")?);
            let capability = read_private_capability(&capability_path)?;
            let experience_id =
                experience_package::ExperienceId::parse(options.required("--experience")?)?;
            let reviewed = review_trusted_graph_grants(
                PathBuf::from(options.required("--root")?).as_path(),
                &experience_id,
                options.required("--revision")?,
                PathBuf::from(options.required("--service-socket")?).as_path(),
                &capability,
                timeout,
            )?;
            println!("reviewed={reviewed}");
        }
        "run" | "run-user" => {
            let shared_login_user = command == "run-user";
            options.ensure_only(&[
                "--root",
                "--runtime-dir",
                "--authority-file",
                "--shell-token-file",
                "--trusted-stock-revision",
                "--agent-socket",
                "--compositor",
                "--provider",
                "--supervisor",
                "--host",
                "--compositor-user",
                "--provider-user",
                "--supervisor-user",
                "--host-user",
                "--timeout-ms",
            ])?;
            let shared_identity = shared_login_user
                .then(ServiceIdentity::current)
                .transpose()?;
            let runtime_directory = PathBuf::from(options.required("--runtime-dir")?);
            let host_runtime_directory = if shared_login_user {
                absolute_environment_path("XDG_RUNTIME_DIR")?
            } else {
                runtime_directory.clone()
            };
            let host_home_directory = if shared_login_user {
                absolute_environment_path("HOME")?
            } else {
                runtime_directory.join("cache-host")
            };
            let host_cache_directory = if shared_login_user {
                match std::env::var_os("XDG_CACHE_HOME") {
                    Some(_) => absolute_environment_path("XDG_CACHE_HOME")?,
                    None => host_home_directory.join(".cache"),
                }
            } else {
                runtime_directory.join("cache-host")
            };
            let resolve_identity = |option: &str| -> Result<ServiceIdentity> {
                if let Some(identity) = &shared_identity {
                    if options.optional(option).is_some() {
                        bail!("{option} is not accepted by run-user");
                    }
                    Ok(identity.clone())
                } else {
                    ServiceIdentity::resolve(options.required(option)?)
                }
            };
            run_system_session(SystemSessionOptions {
                revision_root: PathBuf::from(options.required("--root")?),
                runtime_directory,
                host_runtime_directory,
                host_home_directory,
                host_cache_directory,
                authority_file: PathBuf::from(options.required("--authority-file")?),
                shell_token_file: PathBuf::from(options.required("--shell-token-file")?),
                trusted_stock_revision: options.required("--trusted-stock-revision")?.into(),
                agent_socket: PathBuf::from(options.required("--agent-socket")?),
                compositor_executable: PathBuf::from(options.required("--compositor")?),
                provider_executable: PathBuf::from(options.required("--provider")?),
                supervisor_executable: PathBuf::from(options.required("--supervisor")?),
                host_executable: PathBuf::from(options.required("--host")?),
                compositor_identity: resolve_identity("--compositor-user")?,
                provider_identity: resolve_identity("--provider-user")?,
                supervisor_identity: resolve_identity("--supervisor-user")?,
                host_identity: resolve_identity("--host-user")?,
                identity_mode: if shared_login_user {
                    SessionIdentityMode::SharedLoginUser
                } else {
                    SessionIdentityMode::IsolatedServices
                },
                startup_timeout: timeout,
            })?;
        }
        "host-proxy" => {
            options.ensure_only(&["--launcher-socket"])?;
            run_host_proxy(PathBuf::from(options.required("--launcher-socket")?))?;
        }
        _ => bail!(usage()),
    }
    Ok(())
}

fn absolute_environment_path(name: &str) -> Result<PathBuf> {
    let value = std::env::var_os(name).with_context(|| format!("{name} is not set"))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        bail!("{name} must be an absolute path: {}", path.display());
    }
    Ok(path)
}

fn read_private_capability(path: &PathBuf) -> Result<String> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() == 0
        || metadata.len() > 257
    {
        bail!("grant capability file must be a private 1 to 256 byte regular file");
    }
    let capability = fs::read_to_string(path)?;
    let capability = capability.trim_end_matches(['\r', '\n']).to_owned();
    if capability.is_empty() || capability.len() > 256 {
        bail!("grant capability file must contain 1 to 256 bytes");
    }
    Ok(capability)
}

struct Options(BTreeMap<String, String>);

impl Options {
    fn parse(arguments: Vec<String>) -> Result<Self> {
        if !arguments.len().is_multiple_of(2) {
            bail!("every option requires a value\n{}", usage());
        }
        let mut options = BTreeMap::new();
        for pair in arguments.chunks_exact(2) {
            if !pair[0].starts_with("--") {
                bail!("expected option, got {}", pair[0]);
            }
            if options.insert(pair[0].clone(), pair[1].clone()).is_some() {
                bail!("duplicate option: {}", pair[0]);
            }
        }
        Ok(Self(options))
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.optional(name)
            .with_context(|| format!("missing required option: {name}"))
    }

    fn optional(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    fn ensure_only(&self, allowed: &[&str]) -> Result<()> {
        for name in self.0.keys() {
            if !allowed.contains(&name.as_str()) {
                bail!("unexpected option for this command: {name}");
            }
        }
        Ok(())
    }
}

fn usage() -> &'static str {
    "usage:\n  sos-linux-session bootstrap-graph --root DIR --experience ID --service-socket PATH [--timeout-ms N]\n  sos-linux-session shutdown --service-socket PATH [--timeout-ms N]\n  sos-linux-session review-grants --root DIR --revision ID --service-socket PATH --capability-file FILE [--timeout-ms N]\n  sos-linux-session review-graph-grants --root DIR --experience ID --revision ID --service-socket PATH --capability-file FILE [--timeout-ms N]\n  sos-linux-session run --root DIR --runtime-dir DIR --authority-file FILE --shell-token-file FILE --trusted-stock-revision ID --agent-socket PATH --compositor FILE --provider FILE --supervisor FILE --host FILE --compositor-user USER --provider-user USER --supervisor-user USER --host-user USER [--timeout-ms N]\n  sos-linux-session run-user --root DIR --runtime-dir DIR --authority-file FILE --shell-token-file FILE --trusted-stock-revision ID --agent-socket PATH --compositor FILE --provider FILE --supervisor FILE --host FILE [--timeout-ms N]"
}
