use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    thread,
    time::Duration,
};

use provider_state_service::ServiceClient;
use revision_supervisor::{
    CoordinatedSupervisor, RevisionInput, RevisionStore, RevisionSupervisor, SupervisorEvent,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ControlRequest {
    Promote {
        revision_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transaction_id: Option<String>,
    },
    Status,
    Shutdown,
}

#[derive(Debug, Deserialize, Serialize)]
struct ControlResponse {
    ok: bool,
    active_revision: Option<String>,
    event: Option<String>,
    error: Option<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("revision_supervisor_failed error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("install") => install(parse_options(args.collect())?),
        Some("bootstrap") => bootstrap(parse_options(args.collect())?),
        Some("promote") => control_command(parse_options(args.collect())?, "promote"),
        Some("daemon-status") => control_command(parse_options(args.collect())?, "status"),
        Some("shutdown") => control_command(parse_options(args.collect())?, "shutdown"),
        Some("serve") => serve(parse_options(args.collect())?),
        Some("status") => status(parse_options(args.collect())?),
        _ => Err(usage().into()),
    }
}

fn bootstrap(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let store = RevisionStore::open(options.required("--root")?)?;
    if store.current()?.is_some() {
        return Err("current is already initialized; use supervised promotion".into());
    }
    let revision_id = options.required("--revision")?;
    store.set_current(&revision_id)?;
    println!("{revision_id}");
    Ok(())
}

fn control_command(options: Options, action: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(options.required("--root")?);
    let request = match action {
        "promote" => ControlRequest::Promote {
            revision_id: options.required("--revision")?,
            transaction_id: options.optional("--transaction"),
        },
        "status" => ControlRequest::Status,
        "shutdown" => ControlRequest::Shutdown,
        _ => unreachable!(),
    };
    let response = send_control(&root.join("run/supervisor.sock"), &request)?;
    println!("{}", serde_json::to_string(&response)?);
    if response.ok {
        Ok(())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "supervisor request failed".into())
            .into())
    }
}

fn install(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let source = fs::read(options.required("--source")?)?;
    let state = serde_json::from_slice(&fs::read(options.required("--state")?)?)?;
    let schema_version = options.required("--schema")?.parse()?;
    let executable = PathBuf::from(options.required("--executable")?);
    let store = RevisionStore::open(root)?;
    let revision = store.install(RevisionInput {
        source,
        state,
        schema_version,
        executable,
        args: options.values("--arg"),
    })?;
    println!("{}", revision.manifest.revision_id);
    Ok(())
}

fn status(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let store = RevisionStore::open(options.required("--root")?)?;
    match store.current()? {
        Some(revision) => println!("{}", revision.manifest.revision_id),
        None => println!("none"),
    }
    Ok(())
}

fn serve(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let store = RevisionStore::open(options.required("--root")?)?;
    let timeout = Duration::from_millis(
        options
            .optional("--timeout-ms")
            .unwrap_or_else(|| "5000".into())
            .parse()?,
    );
    let socket = store.root().join("run/supervisor.sock");
    if socket.exists() {
        return Err(format!("control socket already exists: {}", socket.display()).into());
    }
    let listener = UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;
    let supervisor = RevisionSupervisor::new(store.clone(), timeout);
    let mut runtime = if let Some(service_socket) = options.optional("--service-socket") {
        let service_timeout = Duration::from_millis(
            options
                .optional("--service-timeout-ms")
                .unwrap_or_else(|| "5000".into())
                .parse()?,
        );
        let mut coordinated = CoordinatedSupervisor::new(
            store,
            supervisor,
            ServiceClient::new(service_socket, service_timeout),
        );
        if let Some(event) = coordinated.boot()? {
            log_event(&event);
        }
        Runtime::Coordinated(coordinated)
    } else {
        let mut standalone = supervisor;
        if let Some(event) = standalone.boot()? {
            log_event(&event);
        }
        Runtime::Standalone(standalone)
    };
    println!("revision_supervisor_listening socket={}", socket.display());
    let result = serve_loop(&listener, &mut runtime);
    runtime.shutdown().ok();
    fs::remove_file(&socket).ok();
    result
}

fn serve_loop(
    listener: &UnixListener,
    runtime: &mut Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match runtime.poll() {
            Ok(Some(event)) => println!("revision_supervisor_event event={event}"),
            Ok(None) => {}
            Err(error) => eprintln!("revision_supervisor_recovery_failed error={error}"),
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut line = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut line)?;
                let request = serde_json::from_str::<ControlRequest>(&line);
                let (response, shutdown) = match request {
                    Ok(ControlRequest::Promote {
                        revision_id,
                        transaction_id,
                    }) => match runtime.promote(&revision_id, transaction_id.as_deref()) {
                        Ok(event) => {
                            println!("revision_supervisor_event event={event}");
                            (success(runtime, Some(event)), false)
                        }
                        Err(error) => (failure(runtime, error.to_string()), false),
                    },
                    Ok(ControlRequest::Status) => (success(runtime, None), false),
                    Ok(ControlRequest::Shutdown) => (success(runtime, None), true),
                    Err(error) => (failure(runtime, error.to_string()), false),
                };
                serde_json::to_writer(&mut stream, &response)?;
                stream.write_all(b"\n")?;
                stream.flush()?;
                if shutdown {
                    return Ok(());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

enum Runtime {
    Standalone(RevisionSupervisor),
    Coordinated(CoordinatedSupervisor),
}

impl Runtime {
    fn promote(
        &mut self,
        revision_id: &str,
        transaction_id: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            Self::Standalone(supervisor) => {
                if transaction_id.is_some() {
                    return Err("standalone supervisor does not accept a transaction ID".into());
                }
                Ok(format!("{:?}", supervisor.promote(revision_id)?))
            }
            Self::Coordinated(supervisor) => {
                let transaction_id =
                    transaction_id.ok_or("coordinated supervisor requires --transaction")?;
                Ok(format!(
                    "{:?}",
                    supervisor.promote(transaction_id, revision_id)?
                ))
            }
        }
    }

    fn poll(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        match self {
            Self::Standalone(supervisor) => {
                Ok(supervisor.poll()?.map(|event| format!("{event:?}")))
            }
            Self::Coordinated(supervisor) => {
                Ok(supervisor.poll()?.map(|event| format!("{event:?}")))
            }
        }
    }

    fn active_revision(&self) -> Option<&str> {
        match self {
            Self::Standalone(supervisor) => supervisor.active_revision(),
            Self::Coordinated(supervisor) => supervisor.active_revision(),
        }
    }

    fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Standalone(supervisor) => supervisor.shutdown()?,
            Self::Coordinated(supervisor) => supervisor.shutdown()?,
        }
        Ok(())
    }
}

fn success(runtime: &Runtime, event: Option<String>) -> ControlResponse {
    ControlResponse {
        ok: true,
        active_revision: runtime.active_revision().map(str::to_owned),
        event,
        error: None,
    }
}

fn failure(runtime: &Runtime, error: String) -> ControlResponse {
    ControlResponse {
        ok: false,
        active_revision: runtime.active_revision().map(str::to_owned),
        event: None,
        error: Some(error),
    }
}

fn log_event(event: &SupervisorEvent) {
    println!("revision_supervisor_event event={event:?}");
}

#[derive(Default)]
struct Options(Vec<(String, String)>);

impl Options {
    fn required(&self, name: &str) -> Result<String, String> {
        self.optional(name)
            .ok_or_else(|| format!("missing required option: {name}"))
    }

    fn optional(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }

    fn values(&self, name: &str) -> Vec<String> {
        self.0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .collect()
    }
}

fn parse_options(arguments: Vec<String>) -> Result<Options, String> {
    if !arguments.len().is_multiple_of(2) {
        return Err("every option requires a value".into());
    }
    let mut options = Options::default();
    for pair in arguments.chunks_exact(2) {
        if !pair[0].starts_with("--") {
            return Err(format!("expected option, got {}", pair[0]));
        }
        options.0.push((pair[0].clone(), pair[1].clone()));
    }
    Ok(options)
}

fn usage() -> &'static str {
    "usage:\n  sos-revision-supervisor install --root DIR --source FILE --state FILE --schema N --executable FILE [--arg VALUE ...]\n  sos-revision-supervisor bootstrap --root DIR --revision ID\n  sos-revision-supervisor serve --root DIR [--timeout-ms N] [--service-socket PATH --service-timeout-ms N]\n  sos-revision-supervisor promote --root DIR --revision ID [--transaction ID]\n  sos-revision-supervisor daemon-status --root DIR\n  sos-revision-supervisor shutdown --root DIR\n  sos-revision-supervisor status --root DIR"
}

fn send_control(
    socket: &std::path::Path,
    request: &ControlRequest,
) -> std::io::Result<ControlResponse> {
    let mut stream = UnixStream::connect(socket)?;
    serde_json::to_writer(&mut stream, request).map_err(std::io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
}
