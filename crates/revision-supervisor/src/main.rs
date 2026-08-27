use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt as _,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    thread,
    time::Duration,
};

use provider_state_service::ServiceClient;
use revision_supervisor::{
    install_reference_composition, CoordinatedSupervisor, DurableState, ExperienceGraphSupervisor,
    ExperienceRegistry, GraphResolver, GraphStore, HostCommand, ReverseDependencyIndex,
    RevisionAssetInput, RevisionInput, RevisionPackageInput, RevisionStore, RevisionSupervisor,
    SupervisorEvent, STOCK_SHELL_EXPERIENCE_ID,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ControlRequest {
    Activate {
        revision_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transaction_id: Option<String>,
    },
    ActivateGraph {
        graph_id: String,
    },
    AdvanceExperience {
        experience_id: String,
        revision_id: String,
    },
    PresentExperience {
        experience_id: String,
    },
    DismissExperience {
        experience_id: String,
    },
    RefreshTracked,
    Status,
    Restart,
    Shutdown,
}

#[derive(Debug, Deserialize, Serialize)]
struct ControlResponse {
    ok: bool,
    active_revision: Option<String>,
    active_graph: Option<String>,
    host_pid: Option<u32>,
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
        Some("install") => Err(
            "bare revision installation is disabled; use install-package for Experience API v4"
                .into(),
        ),
        Some("install-package") => install_package(parse_options(args.collect())?),
        Some("migrate-stock-v4") => migrate_stock_v4(parse_options(args.collect())?),
        Some("install-composition-demo") => {
            install_composition_demo(parse_options(args.collect())?)
        }
        Some("bootstrap") => bootstrap(parse_options(args.collect())?),
        Some("bootstrap-graph") => bootstrap_graph(parse_options(args.collect())?),
        Some("resolve-graph") => resolve_graph(parse_options(args.collect())?),
        Some("graph-status") => graph_status(parse_options(args.collect())?),
        Some("experience-status") => experience_status(parse_options(args.collect())?),
        Some("retire-experience") => retire_experience(parse_options(args.collect())?),
        Some("activate") => control_command(parse_options(args.collect())?, "activate"),
        Some("activate-graph") => control_command(parse_options(args.collect())?, "activate-graph"),
        Some("advance-experience") => {
            control_command(parse_options(args.collect())?, "advance-experience")
        }
        Some("present-experience") => {
            control_command(parse_options(args.collect())?, "present-experience")
        }
        Some("dismiss-experience") => {
            control_command(parse_options(args.collect())?, "dismiss-experience")
        }
        Some("refresh-tracked") => {
            control_command(parse_options(args.collect())?, "refresh-tracked")
        }
        Some("daemon-status") => control_command(parse_options(args.collect())?, "status"),
        Some("restart") => control_command(parse_options(args.collect())?, "restart"),
        Some("shutdown") => control_command(parse_options(args.collect())?, "shutdown"),
        Some("serve") => serve(parse_options(args.collect())?),
        Some("status") => status(parse_options(args.collect())?),
        _ => Err(usage().into()),
    }
}

fn install_composition_demo(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let store = RevisionStore::open(options.required("--root")?)?;
    let installed = install_reference_composition(&store)?;
    println!("{}", serde_json::to_string(&installed)?);
    Ok(())
}

fn bootstrap(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let store = RevisionStore::open(options.required("--root")?)?;
    if store.current()?.is_some() {
        return Err("current is already initialized; use supervised activation".into());
    }
    let revision_id = options.required("--revision")?;
    store.set_current(&revision_id)?;
    println!("{revision_id}");
    Ok(())
}

fn control_command(options: Options, action: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(options.required("--root")?);
    let request = match action {
        "activate" => ControlRequest::Activate {
            revision_id: options.required("--revision")?,
            transaction_id: options.optional("--transaction"),
        },
        "activate-graph" => ControlRequest::ActivateGraph {
            graph_id: options.required("--graph")?,
        },
        "advance-experience" => ControlRequest::AdvanceExperience {
            experience_id: options.required("--experience")?,
            revision_id: options.required("--revision")?,
        },
        "present-experience" => ControlRequest::PresentExperience {
            experience_id: options.required("--experience")?,
        },
        "dismiss-experience" => ControlRequest::DismissExperience {
            experience_id: options.required("--experience")?,
        },
        "refresh-tracked" => ControlRequest::RefreshTracked,
        "status" => ControlRequest::Status,
        "restart" => ControlRequest::Restart,
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

fn install_package(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let source = fs::read(options.required("--source")?)?;
    let state = serde_json::from_slice(&fs::read(options.required("--state")?)?)?;
    let package = serde_json::from_slice(&fs::read(options.required("--package")?)?)?;
    let schema_version = options.required("--schema")?.parse()?;
    let assets = read_assets(&options)?;
    let store = RevisionStore::open(&root)?;
    let revision = store.install_package(RevisionPackageInput {
        revision: RevisionInput {
            source,
            state,
            schema_version,
            experience_api_version: 4,
            assets,
        },
        package,
    })?;
    println!("{}", revision.manifest.revision_id);
    Ok(())
}

#[derive(Serialize)]
struct StockMigrationResult {
    revision_id: String,
    graph_id: String,
    legacy_current_unchanged: bool,
}

fn migrate_stock_v4(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let source = fs::read(options.required("--source")?)?;
    let package: experience_package::PackageMetadata =
        serde_json::from_slice(&fs::read(options.required("--package")?)?)?;
    let stock_id = experience_package::ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID)?;
    if package.experience_id != stock_id
        || package.role != experience_package::ExperienceRole::Shell
    {
        return Err("Stock migration requires the reserved shell package identity".into());
    }
    let assets = read_assets(&options)?;
    let store = RevisionStore::open(&root)?;
    let legacy = store
        .current()?
        .ok_or("Stock migration requires an initialized legacy current pointer")?;
    let durable: DurableState = serde_json::from_slice(&fs::read(
        legacy.directory.join(&legacy.manifest.state.path),
    )?)?;
    let revision = store.install_package(RevisionPackageInput {
        revision: RevisionInput {
            source,
            state: durable.state,
            schema_version: durable.schema_version,
            experience_api_version: 4,
            assets,
        },
        package,
    })?;
    let revision_id = revision.manifest.revision_id;
    let registry = ExperienceRegistry::open(store.clone())?;
    if registry.get(&stock_id)?.is_none() {
        if legacy.package.is_none() {
            registry.migrate_legacy_current()?;
        } else {
            registry.create(
                &stock_id,
                experience_package::ExperienceRole::Shell,
                &revision_id,
            )?;
        }
    }
    registry.set_current(&stock_id, &revision_id)?;
    ReverseDependencyIndex::open(store.root()).rebuild(&store, &registry)?;
    let graph = GraphResolver::new(store.clone())
        .resolve(&revision_id, &experience_package::ExportId::parse("main")?)?;
    let graphs = GraphStore::open(store.root())?;
    let graph_id = graphs.install(&graph)?;
    graphs.set_current(&stock_id, &graph_id)?;
    let legacy_current_unchanged = store
        .current()?
        .is_some_and(|current| current.manifest.revision_id == legacy.manifest.revision_id);
    println!(
        "{}",
        serde_json::to_string(&StockMigrationResult {
            revision_id,
            graph_id,
            legacy_current_unchanged,
        })?
    );
    Ok(())
}

fn bootstrap_graph(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let experience_id = experience_package::ExperienceId::parse(options.required("--experience")?)?;
    let revision_id = options.required("--revision")?;
    let export_id = experience_package::ExportId::parse(
        options
            .optional("--export")
            .unwrap_or_else(|| "main".into()),
    )?;
    let store = RevisionStore::open(&root)?;
    let revision = store.verify(&revision_id)?;
    let package = revision
        .package
        .as_ref()
        .ok_or("graph bootstrap requires a package v4 revision")?;
    if package.experience_id != experience_id {
        return Err("graph bootstrap experience does not match the revision package".into());
    }
    let registry = ExperienceRegistry::open(store.clone())?;
    if registry.get(&experience_id)?.is_none() {
        registry.create(&experience_id, package.role, &revision_id)?;
    } else {
        registry.set_current(&experience_id, &revision_id)?;
    }
    ReverseDependencyIndex::open(store.root()).rebuild(&store, &registry)?;
    let graph = GraphResolver::new(store).resolve(&revision_id, &export_id)?;
    let graphs = GraphStore::open(root)?;
    let graph_id = graphs.install(&graph)?;
    graphs.set_current(&experience_id, &graph_id)?;
    println!("{graph_id}");
    Ok(())
}

fn graph_status(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let experience_id = experience_package::ExperienceId::parse(options.required("--experience")?)?;
    let graphs = GraphStore::open(root)?;
    match graphs.current(&experience_id)? {
        Some((graph_id, _)) => println!("{graph_id}"),
        None => println!("none"),
    }
    Ok(())
}

fn experience_status(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let experience_id = experience_package::ExperienceId::parse(options.required("--experience")?)?;
    let store = RevisionStore::open(root)?;
    let registry = ExperienceRegistry::open(store)?;
    match registry.get(&experience_id)? {
        Some(_) => match registry.current(&experience_id)? {
            Some(revision) => println!("{}", revision.manifest.revision_id),
            None => println!("none"),
        },
        None => println!("none"),
    }
    Ok(())
}

fn retire_experience(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let experience_id = experience_package::ExperienceId::parse(options.required("--experience")?)?;
    let store = RevisionStore::open(&root)?;
    let registry = ExperienceRegistry::open(store.clone())?;
    let retired = registry.retire(&experience_id)?;
    ReverseDependencyIndex::open(store.root()).rebuild(&store, &registry)?;
    println!("retired={retired} experience_id={experience_id}");
    Ok(())
}

fn resolve_graph(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let root = options.required("--root")?;
    let revision_id = options.required("--revision")?;
    let export_id = experience_package::ExportId::parse(
        options
            .optional("--export")
            .unwrap_or_else(|| "main".into()),
    )?;
    let store = RevisionStore::open(&root)?;
    let graph = GraphResolver::new(store.clone()).resolve(&revision_id, &export_id)?;
    println!("{}", GraphStore::open(store.root())?.install(&graph)?);
    Ok(())
}

fn read_assets(options: &Options) -> Result<Vec<RevisionAssetInput>, Box<dyn std::error::Error>> {
    options
        .values("--asset")
        .into_iter()
        .map(
            |specification| -> Result<RevisionAssetInput, Box<dyn std::error::Error>> {
                let mut parts = specification.splitn(3, ':');
                let id = parts.next().unwrap_or_default();
                let kind = parts.next().unwrap_or_default();
                let path = parts.next().unwrap_or_default();
                if id.is_empty() || kind.is_empty() || path.is_empty() {
                    return Err(format!(
                        "invalid --asset {specification:?}; expected ID:KIND:FILE"
                    )
                    .into());
                }
                Ok(RevisionAssetInput {
                    id: id.into(),
                    kind: kind.into(),
                    bytes: fs::read(path)?,
                })
            },
        )
        .collect::<Result<Vec<_>, _>>()
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
        match UnixStream::connect(&socket) {
            Ok(_) => {
                return Err(format!(
                    "an active revision supervisor already owns {}",
                    socket.display()
                )
                .into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                fs::remove_file(&socket)?;
                println!(
                    "revision_supervisor_recovered artifact=stale_control_socket path={}",
                    socket.display()
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    let host_command = HostCommand::with_args(
        options.required("--host-executable")?,
        options.values("--host-arg"),
    );
    let mut runtime = if let Some(root_experience) = options.optional("--root-experience") {
        let root_experience = experience_package::ExperienceId::parse(root_experience)?;
        let registry = ExperienceRegistry::open(store.clone())?;
        let graphs = GraphStore::open(store.root())?;
        let graph = ExperienceGraphSupervisor::new(
            store.clone(),
            registry.clone(),
            graphs.clone(),
            host_command,
            timeout,
        );
        let mut graph = if let Some(service_socket) = options.optional("--service-socket") {
            let service_timeout = Duration::from_millis(
                options
                    .optional("--service-timeout-ms")
                    .unwrap_or_else(|| "5000".into())
                    .parse()?,
            );
            graph.with_authority(ServiceClient::new(service_socket, service_timeout))
        } else {
            graph
        };
        graph.boot(&root_experience)?;
        Runtime::Graph {
            supervisor: graph,
            root: root_experience,
            store,
            registry,
            graphs,
        }
    } else if let Some(service_socket) = options.optional("--service-socket") {
        let supervisor = RevisionSupervisor::new(store.clone(), host_command, timeout);
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
        let supervisor = RevisionSupervisor::new(store.clone(), host_command, timeout);
        let mut standalone = supervisor;
        if let Some(event) = standalone.boot()? {
            log_event(&event);
        }
        Runtime::Standalone(standalone)
    };
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o660))?;
    listener.set_nonblocking(true)?;
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
                    Ok(ControlRequest::Activate {
                        revision_id,
                        transaction_id,
                    }) => match runtime.activate(&revision_id, transaction_id.as_deref()) {
                        Ok(event) => {
                            println!("revision_supervisor_event event={event}");
                            (success(runtime, Some(event)), false)
                        }
                        Err(error) => (failure(runtime, error.to_string()), false),
                    },
                    Ok(ControlRequest::ActivateGraph { graph_id }) => {
                        match runtime.activate_graph(&graph_id) {
                            Ok(event) => (success(runtime, Some(event)), false),
                            Err(error) => (failure(runtime, error.to_string()), false),
                        }
                    }
                    Ok(ControlRequest::AdvanceExperience {
                        experience_id,
                        revision_id,
                    }) => match runtime.advance_experience(&experience_id, &revision_id) {
                        Ok(event) => (success(runtime, Some(event)), false),
                        Err(error) => (failure(runtime, error.to_string()), false),
                    },
                    Ok(ControlRequest::PresentExperience { experience_id }) => {
                        match runtime.present_experience(&experience_id) {
                            Ok(event) => (success(runtime, Some(event)), false),
                            Err(error) => (failure(runtime, error.to_string()), false),
                        }
                    }
                    Ok(ControlRequest::DismissExperience { experience_id }) => {
                        match runtime.dismiss_experience(&experience_id) {
                            Ok(event) => (success(runtime, Some(event)), false),
                            Err(error) => (failure(runtime, error.to_string()), false),
                        }
                    }
                    Ok(ControlRequest::RefreshTracked) => match runtime.refresh_tracked() {
                        Ok(event) => (success(runtime, Some(event)), false),
                        Err(error) => (failure(runtime, error.to_string()), false),
                    },
                    Ok(ControlRequest::Status) => (success(runtime, None), false),
                    Ok(ControlRequest::Restart) => match runtime.restart_host() {
                        Ok(event) => (success(runtime, Some(event)), false),
                        Err(error) => (failure(runtime, error.to_string()), false),
                    },
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
    Graph {
        supervisor: ExperienceGraphSupervisor,
        root: experience_package::ExperienceId,
        store: RevisionStore,
        registry: ExperienceRegistry,
        graphs: GraphStore,
    },
}

impl Runtime {
    fn activate(
        &mut self,
        revision_id: &str,
        transaction_id: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            Self::Standalone(supervisor) => {
                if transaction_id.is_some() {
                    return Err("standalone supervisor does not accept a transaction ID".into());
                }
                Ok(format!("{:?}", supervisor.activate(revision_id)?))
            }
            Self::Coordinated(supervisor) => {
                let transaction_id =
                    transaction_id.ok_or("coordinated supervisor requires --transaction")?;
                Ok(format!(
                    "{:?}",
                    supervisor.activate(transaction_id, revision_id)?
                ))
            }
            Self::Graph { .. } => Err("graph supervisor requires activate_graph".into()),
        }
    }

    fn activate_graph(&mut self, graph_id: &str) -> Result<String, Box<dyn std::error::Error>> {
        let Self::Graph {
            supervisor, root, ..
        } = self
        else {
            return Err("revision supervisor is not running in graph mode".into());
        };
        let prepared = supervisor.prepare(root, graph_id)?;
        let pid = supervisor.commit(prepared)?;
        Ok(format!(
            "graph_activated graph_id={graph_id} host_pid={pid}"
        ))
    }

    fn refresh_tracked(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let Self::Graph {
            supervisor,
            root,
            store,
            registry,
            graphs,
        } = self
        else {
            return Err("revision supervisor is not running in graph mode".into());
        };
        let root_revision = registry
            .current(root)?
            .ok_or("root experience has no active revision")?;
        let graph = GraphResolver::new(store.clone()).resolve_tracked(
            &root_revision.manifest.revision_id,
            &experience_package::ExportId::parse("main")?,
            registry,
        )?;
        let graph_id = graphs.install(&graph)?;
        if supervisor.active_graph() == Some(&graph_id) {
            return Ok(format!("graph_unchanged graph_id={graph_id}"));
        }
        let prepared = supervisor.prepare(root, &graph_id)?;
        let pid = supervisor.commit(prepared)?;
        Ok(format!(
            "graph_refreshed graph_id={graph_id} host_pid={pid}"
        ))
    }

    fn advance_experience(
        &mut self,
        experience_id: &str,
        revision_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let Self::Graph { supervisor, .. } = self else {
            return Err("revision supervisor is not running in graph mode".into());
        };
        let experience_id = experience_package::ExperienceId::parse(experience_id)?;
        let activated = supervisor.advance_experience(&experience_id, revision_id)?;
        if activated.graph_updates.is_empty() {
            Ok(format!(
                "experience_advanced experience_id={experience_id} revision_id={revision_id} graph_id=unchanged"
            ))
        } else {
            let graphs = activated
                .graph_updates
                .iter()
                .map(|update| {
                    format!(
                        "{}:{}:{}",
                        update.root_experience_id,
                        update.graph_id,
                        update
                            .host_pid
                            .map_or_else(|| "not_presented".into(), |pid| pid.to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            Ok(format!(
                "experience_advanced experience_id={experience_id} revision_id={revision_id} presented_graphs={graphs}"
            ))
        }
    }

    fn present_experience(
        &mut self,
        experience_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let Self::Graph { supervisor, .. } = self else {
            return Err("revision supervisor is not running in graph mode".into());
        };
        let experience_id = experience_package::ExperienceId::parse(experience_id)?;
        let pid = supervisor
            .boot(&experience_id)?
            .ok_or("experience has no active graph")?;
        let graph_id = supervisor
            .active_graph_for(&experience_id)
            .ok_or("presented experience omitted its active graph")?;
        Ok(format!(
            "experience_presented experience_id={experience_id} graph_id={graph_id} host_pid={pid}"
        ))
    }

    fn dismiss_experience(
        &mut self,
        experience_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let Self::Graph {
            supervisor, root, ..
        } = self
        else {
            return Err("revision supervisor is not running in graph mode".into());
        };
        let experience_id = experience_package::ExperienceId::parse(experience_id)?;
        if &experience_id == root {
            return Err("the configured root experience cannot be dismissed".into());
        }
        let pid = supervisor
            .dismiss(&experience_id)?
            .ok_or("experience is not presented")?;
        Ok(format!(
            "experience_dismissed experience_id={experience_id} host_pid={pid}"
        ))
    }

    fn poll(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        match self {
            Self::Standalone(supervisor) => {
                Ok(supervisor.poll()?.map(|event| format!("{event:?}")))
            }
            Self::Coordinated(supervisor) => {
                Ok(supervisor.poll()?.map(|event| format!("{event:?}")))
            }
            Self::Graph { supervisor, .. } => Ok(supervisor
                .poll()?
                .map(|(graph, failed, current)| format!(
                    "GraphHostRestarted {{ graph_id: {graph}, failed_host_pid: {failed}, host_pid: {current} }}"
                ))),
        }
    }

    fn active_revision(&self) -> Option<String> {
        match self {
            Self::Standalone(supervisor) => supervisor.active_revision().map(str::to_owned),
            Self::Coordinated(supervisor) => supervisor.active_revision().map(str::to_owned),
            Self::Graph { root, registry, .. } => registry
                .current(root)
                .ok()
                .flatten()
                .map(|revision| revision.manifest.revision_id),
        }
    }

    fn active_graph(&self) -> Option<&str> {
        match self {
            Self::Graph { supervisor, .. } => supervisor.active_graph(),
            _ => None,
        }
    }

    fn host_pid(&self) -> Option<u32> {
        match self {
            Self::Standalone(supervisor) => supervisor.host_pid(),
            Self::Coordinated(supervisor) => supervisor.host_pid(),
            Self::Graph { supervisor, .. } => supervisor.host_pid(),
        }
    }

    fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Standalone(supervisor) => supervisor.shutdown()?,
            Self::Coordinated(supervisor) => supervisor.shutdown()?,
            Self::Graph { supervisor, .. } => supervisor.shutdown()?,
        }
        Ok(())
    }

    fn restart_host(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            Self::Standalone(supervisor) => Ok(format!("{:?}", supervisor.restart_host()?)),
            Self::Coordinated(supervisor) => Ok(format!("{:?}", supervisor.restart_host()?)),
            Self::Graph { supervisor, .. } => {
                let (graph_id, previous_pid, host_pid) = supervisor.restart_host()?;
                Ok(format!(
                    "graph_host_restarted graph_id={graph_id} previous_host_pid={previous_pid} host_pid={host_pid}"
                ))
            }
        }
    }
}

fn success(runtime: &Runtime, event: Option<String>) -> ControlResponse {
    ControlResponse {
        ok: true,
        active_revision: runtime.active_revision(),
        active_graph: runtime.active_graph().map(str::to_owned),
        host_pid: runtime.host_pid(),
        event,
        error: None,
    }
}

fn failure(runtime: &Runtime, error: String) -> ControlResponse {
    ControlResponse {
        ok: false,
        active_revision: runtime.active_revision(),
        active_graph: runtime.active_graph().map(str::to_owned),
        host_pid: runtime.host_pid(),
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
    "usage:\n  sos-revision-supervisor install-package --root DIR --source FILE --state FILE --schema N --package FILE [--asset ID:KIND:FILE ...]\n  sos-revision-supervisor migrate-stock-v4 --root DIR --source FILE --package FILE [--asset ID:KIND:FILE ...]\n  sos-revision-supervisor install-composition-demo --root DIR\n  sos-revision-supervisor bootstrap --root DIR --revision ID\n  sos-revision-supervisor bootstrap-graph --root DIR --experience ID --revision ID [--export ID]\n  sos-revision-supervisor resolve-graph --root DIR --revision ID [--export ID]\n  sos-revision-supervisor graph-status --root DIR --experience ID\n  sos-revision-supervisor experience-status --root DIR --experience ID\n  sos-revision-supervisor retire-experience --root DIR --experience ID\n  sos-revision-supervisor serve --root DIR --host-executable FILE [--host-arg VALUE ...] [--root-experience ID] [--timeout-ms N] [--service-socket PATH --service-timeout-ms N]\n  sos-revision-supervisor activate --root DIR --revision ID [--transaction ID]\n  sos-revision-supervisor activate-graph --root DIR --graph ID\n  sos-revision-supervisor advance-experience --root DIR --experience ID --revision ID\n  sos-revision-supervisor present-experience --root DIR --experience ID\n  sos-revision-supervisor dismiss-experience --root DIR --experience ID\n  sos-revision-supervisor refresh-tracked --root DIR\n  sos-revision-supervisor daemon-status --root DIR\n  sos-revision-supervisor restart --root DIR\n  sos-revision-supervisor shutdown --root DIR\n  sos-revision-supervisor status --root DIR"
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
