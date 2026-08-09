use std::{collections::BTreeMap, env, path::PathBuf, time::Duration};

use anyhow::{bail, Context as _, Result};
use sos_linux_session::{
    bootstrap_authority, run_host_proxy, run_system_session, shutdown_authority, stage_revision,
    BootstrapOutcome, ServiceIdentity, SessionIdentityMode, SystemSessionOptions,
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
        "bootstrap" => {
            options.ensure_only(&["--root", "--service-socket", "--timeout-ms"])?;
            let root = PathBuf::from(options.required("--root")?);
            let service_socket = PathBuf::from(options.required("--service-socket")?);
            match bootstrap_authority(&root, &service_socket, timeout)? {
                BootstrapOutcome::Initialized {
                    transaction_id,
                    revision_id,
                } => println!(
                    "authority_initialized transaction_id={transaction_id} revision_id={revision_id}"
                ),
                BootstrapOutcome::AlreadyBound { revision_id } => {
                    println!("authority_already_bound revision_id={revision_id}")
                }
                BootstrapOutcome::RecoveryRequired {
                    pointer_revision,
                    authority_revision,
                } => println!(
                    "authority_recovery_required pointer_revision={pointer_revision} authority_revision={authority_revision}"
                ),
            }
        }
        "stage" => {
            options.ensure_only(&["--root", "--revision", "--service-socket", "--timeout-ms"])?;
            let root = PathBuf::from(options.required("--root")?);
            let revision_id = options.required("--revision")?;
            let service_socket = PathBuf::from(options.required("--service-socket")?);
            println!(
                "{}",
                stage_revision(&root, revision_id, &service_socket, timeout)?
            );
        }
        "shutdown" => {
            options.ensure_only(&["--service-socket", "--timeout-ms"])?;
            let service_socket = PathBuf::from(options.required("--service-socket")?);
            shutdown_authority(&service_socket, timeout)?;
        }
        "run" | "run-user" => {
            let shared_login_user = command == "run-user";
            options.ensure_only(&[
                "--root",
                "--runtime-dir",
                "--authority-file",
                "--shell-token-file",
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
                runtime_directory: PathBuf::from(options.required("--runtime-dir")?),
                authority_file: PathBuf::from(options.required("--authority-file")?),
                shell_token_file: PathBuf::from(options.required("--shell-token-file")?),
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
    "usage:\n  sos-linux-session bootstrap --root DIR --service-socket PATH [--timeout-ms N]\n  sos-linux-session stage --root DIR --revision ID --service-socket PATH [--timeout-ms N]\n  sos-linux-session shutdown --service-socket PATH [--timeout-ms N]\n  sos-linux-session run --root DIR --runtime-dir DIR --authority-file FILE --shell-token-file FILE --agent-socket PATH --compositor FILE --provider FILE --supervisor FILE --host FILE --compositor-user USER --provider-user USER --supervisor-user USER --host-user USER [--timeout-ms N]\n  sos-linux-session run-user --root DIR --runtime-dir DIR --authority-file FILE --shell-token-file FILE --agent-socket PATH --compositor FILE --provider FILE --supervisor FILE --host FILE [--timeout-ms N]"
}
