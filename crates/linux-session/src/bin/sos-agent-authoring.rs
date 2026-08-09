use std::{collections::BTreeMap, env, path::PathBuf, time::Duration};

use anyhow::{bail, Context as _, Result};
use nix::unistd::User;
use sos_linux_session::{run_authoring_broker, AuthoringBrokerOptions};

fn main() {
    if let Err(error) = run() {
        eprintln!("sos_agent_authoring_failed error={error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let options = Options::parse(env::args().skip(1).collect())?;
    options.ensure_only(&[
        "--root",
        "--service-socket",
        "--supervisor-socket",
        "--listen-socket",
        "--agent-user",
        "--timeout-ms",
    ])?;
    let agent_name = options.required("--agent-user")?;
    let agent = User::from_name(agent_name)?
        .with_context(|| format!("Linux agent account does not exist: {agent_name}"))?;
    run_authoring_broker(AuthoringBrokerOptions {
        revision_root: PathBuf::from(options.required("--root")?),
        service_socket: PathBuf::from(options.required("--service-socket")?),
        supervisor_socket: PathBuf::from(options.required("--supervisor-socket")?),
        listen_socket: PathBuf::from(options.required("--listen-socket")?),
        agent_uid: agent.uid,
        timeout: Duration::from_millis(
            options
                .optional("--timeout-ms")
                .unwrap_or("30000")
                .parse()
                .context("--timeout-ms must be an integer")?,
        ),
    })
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
                bail!("unexpected option: {name}");
            }
        }
        Ok(())
    }
}

fn usage() -> &'static str {
    "usage: sos-agent-authoring --root DIR --service-socket PATH --supervisor-socket PATH --listen-socket PATH --agent-user USER [--timeout-ms N]"
}
