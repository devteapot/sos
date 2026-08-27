use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    os::unix::{
        fs::{FileTypeExt as _, PermissionsExt as _},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use experience_ir::{Content, SceneNode};
use experience_package::{
    canonical_sha256, hex_sha256, BoundaryGrant, DependencyAlias, DependencyBinding,
    DependencyPolicy, DerivationKind, DerivationParent, DerivationRecord, ExperienceContract,
    ExperienceId, ExportId, PackageMetadata, RevisionId, StateMigrationRecord,
    StateMigrationSource, PACKAGE_FORMAT_VERSION,
};
use nix::{
    sys::socket::{getsockopt, sockopt::PeerCredentials},
    unistd::Uid,
};
use provider_state_service::ServiceClient;
use revision_supervisor::{
    DurableState, ExperienceRegistry, GraphResolver, GraphStore, ReverseDependencyIndex,
    RevisionAssetInput as StoreAssetInput, RevisionInput, RevisionPackageInput, RevisionStore,
    VerifiedRevision, STOCK_SHELL_EXPERIENCE_ID,
};
use runtime_luau::{
    load_revision_assets, LuauRuntime, RevisionAssetInput as RuntimeAssetInput, ValidationReport,
    MAX_SOURCE_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use service_protocol::{
    DataFlowGrant, ResourceQuery, ResourceValue, ResponsePayload, ServiceError, ServiceRequest,
    StateResource,
};

const MAX_AUTHORING_MODULES: usize = 16;
const MAX_AUTHORING_MODULE_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_BYTES: u64 =
    ((MAX_SOURCE_BYTES + MAX_AUTHORING_MODULE_BYTES) as u64 * 8) + 64 * 1024;

#[derive(Clone, Debug)]
pub struct AuthoringBrokerOptions {
    pub revision_root: PathBuf,
    pub service_socket: PathBuf,
    pub supervisor_socket: PathBuf,
    pub listen_socket: PathBuf,
    pub agent_uid: Uid,
    pub timeout: Duration,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum AuthoringRequest {
    GetExperienceContext,
    GetDerivationContext {
        parents: Vec<AuthoringParent>,
    },
    GetCompositionContext {
        dependencies: Vec<AuthoringDependency>,
    },
    ValidateExperience {
        source: String,
        #[serde(default)]
        modules: Option<Vec<AuthoringModule>>,
    },
    SubmitExperience {
        source: String,
        #[serde(default)]
        modules: Option<Vec<AuthoringModule>>,
    },
    ValidateDerivedExperience {
        target_experience_id: ExperienceId,
        parents: Vec<AuthoringParent>,
        state_source: AuthoringStateSource,
        provider_capabilities: Vec<String>,
        request: String,
        rationale: String,
        contract: ExperienceContract,
        source: String,
        #[serde(default)]
        modules: Option<Vec<AuthoringModule>>,
    },
    SubmitDerivedExperience {
        target_experience_id: ExperienceId,
        parents: Vec<AuthoringParent>,
        state_source: AuthoringStateSource,
        provider_capabilities: Vec<String>,
        request: String,
        rationale: String,
        contract: ExperienceContract,
        source: String,
        #[serde(default)]
        modules: Option<Vec<AuthoringModule>>,
        #[serde(default)]
        replace_existing: bool,
    },
    ValidateComposedExperience {
        target_experience_id: ExperienceId,
        dependencies: Vec<AuthoringDependency>,
        state_source: AuthoringStateSource,
        provider_capabilities: Vec<String>,
        contract: ExperienceContract,
        source: String,
        #[serde(default)]
        modules: Option<Vec<AuthoringModule>>,
    },
    SubmitComposedExperience {
        target_experience_id: ExperienceId,
        dependencies: Vec<AuthoringDependency>,
        state_source: AuthoringStateSource,
        provider_capabilities: Vec<String>,
        contract: ExperienceContract,
        source: String,
        #[serde(default)]
        modules: Option<Vec<AuthoringModule>>,
        #[serde(default)]
        replace_existing: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct AuthoringParent {
    experience_id: ExperienceId,
    revision_id: RevisionId,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct AuthoringDependency {
    alias: DependencyAlias,
    experience_id: ExperienceId,
    revision_id: RevisionId,
    export_id: ExportId,
    policy: DependencyPolicy,
    #[serde(default)]
    grant: BoundaryGrant,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AuthoringStateSource {
    Fresh,
    ExperienceRevision {
        experience_id: ExperienceId,
        revision_id: RevisionId,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AuthoringModule {
    id: String,
    source: String,
}

#[derive(Debug, Serialize)]
struct AuthoringResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct GraphSupervisorRequest<'a> {
    action: &'static str,
    graph_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct SupervisorResponse {
    ok: bool,
    active_revision: Option<String>,
    active_graph: Option<String>,
    event: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
struct ValidatedCandidate {
    source: Vec<u8>,
    state: Value,
    schema_version: u64,
    assets: Vec<StoreAssetInput>,
    validation: ValidationReport,
    package: PackageMetadata,
}

struct ActiveAuthoringExperience {
    graph_id: String,
    revision: VerifiedRevision,
    package: PackageMetadata,
}

#[derive(Debug)]
struct ValidatedDerivedCandidate {
    source: Vec<u8>,
    state: Value,
    schema_version: u64,
    assets: Vec<StoreAssetInput>,
    package: PackageMetadata,
    exports_validated: usize,
}

struct ResolvedAuthoringState {
    schema_version: u64,
    state: Value,
    source: StateMigrationSource,
}

pub fn run_authoring_broker(options: AuthoringBrokerOptions) -> Result<()> {
    if options.timeout.is_zero() {
        bail!("authoring timeout must be greater than zero");
    }
    prepare_socket(&options.listen_socket)?;
    let listener = UnixListener::bind(&options.listen_socket)
        .with_context(|| format!("bind authoring socket {}", options.listen_socket.display()))?;
    fs::set_permissions(&options.listen_socket, fs::Permissions::from_mode(0o660))?;
    println!(
        "sos_agent_authoring_listening socket={} agent_uid={}",
        options.listen_socket.display(),
        options.agent_uid
    );

    let result = serve(&listener, &options);
    fs::remove_file(&options.listen_socket).ok();
    result
}

fn prepare_socket(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create authoring socket directory {}", parent.display()))?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => match UnixStream::connect(path) {
            Ok(mut stream) => {
                // Wake a same-UID development broker's bounded reader instead of
                // leaving its single serialized request loop waiting for timeout.
                stream.write_all(b"\n").ok();
                bail!("an active authoring broker already owns {}", path.display())
            }
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                fs::remove_file(path)?;
            }
            Err(error) => return Err(error.into()),
        },
        Ok(_) => bail!("refusing to replace non-socket path {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn serve(listener: &UnixListener, options: &AuthoringBrokerOptions) -> Result<()> {
    for connection in listener.incoming() {
        let mut stream = connection?;
        let credentials = getsockopt(&stream, PeerCredentials)?;
        if credentials.uid() != options.agent_uid.as_raw() {
            write_response(
                &mut stream,
                AuthoringResponse {
                    ok: false,
                    result: None,
                    error: Some("authoring client identity is not authorized".into()),
                },
            )?;
            continue;
        }
        stream.set_read_timeout(Some(options.timeout))?;
        stream.set_write_timeout(Some(options.timeout))?;
        let response = match read_request(&stream).and_then(|request| handle(options, request)) {
            Ok(result) => AuthoringResponse {
                ok: true,
                result: Some(result),
                error: None,
            },
            Err(error) => {
                eprintln!("sos_agent_authoring_request_failed error={error:#}");
                AuthoringResponse {
                    ok: false,
                    result: None,
                    error: Some(format!("{error:#}")),
                }
            }
        };
        write_response(&mut stream, response)?;
    }
    Ok(())
}

fn read_request(stream: &UnixStream) -> Result<AuthoringRequest> {
    let mut bytes = Vec::new();
    BufReader::new(stream)
        .take(MAX_REQUEST_BYTES + 1)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        bail!("authoring request is too large");
    }
    if !bytes.ends_with(b"\n") {
        bail!("authoring request must be newline terminated");
    }
    serde_json::from_slice(&bytes).context("decode authoring request")
}

fn write_response(stream: &mut UnixStream, response: AuthoringResponse) -> Result<()> {
    serde_json::to_writer(&mut *stream, &response)?;
    stream.write_all(b"\n")?;
    Ok(())
}

fn handle(options: &AuthoringBrokerOptions, request: AuthoringRequest) -> Result<Value> {
    let store = RevisionStore::open(&options.revision_root)?;
    match request {
        AuthoringRequest::GetExperienceContext => {
            let active = active_authoring_experience(&store)?;
            let current = active.revision;
            let source = fs::read_to_string(current.directory.join(&current.manifest.source.path))?;
            let durable =
                load_durable_state(&current.directory.join(&current.manifest.state.path))?;
            let modules = load_revision_assets(&current.directory)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .into_iter()
                .filter(|asset| asset.kind == "luau")
                .map(|asset| {
                    Ok(AuthoringModule {
                        id: asset.id,
                        source: String::from_utf8(asset.bytes)
                            .context("active revision contains a non-UTF-8 Luau module")?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({
                "revision_id": current.manifest.revision_id,
                "experience_id": active.package.experience_id,
                "graph_id": active.graph_id,
                "package": active.package,
                "source": source,
                "modules": modules,
                "schema_version": durable.schema_version,
                "experience_api_version": current.manifest.experience_api_version,
                "assets_supported": false,
                "modules_supported": true,
            }))
        }
        AuthoringRequest::GetDerivationContext { parents } => {
            let contexts = inspect_parents(&store, &parents)?;
            Ok(json!({
                "parents": contexts,
                "max_parents": experience_package::MAX_DERIVATION_PARENTS,
                "result_must_be_self_contained": true,
                "grants_are_not_inherited": true,
            }))
        }
        AuthoringRequest::GetCompositionContext { dependencies } => {
            let (bindings, contexts) = inspect_dependencies(&store, &dependencies)?;
            Ok(json!({
                "dependencies": contexts,
                "resolved_bindings": bindings,
                "boundary_values_are_schema_validated": true,
                "child_state_and_grants_remain_independent": true,
            }))
        }
        AuthoringRequest::ValidateExperience { source, modules } => {
            let active = active_authoring_experience(&store)?;
            let authority = get_authority_experience_state(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &active.package.experience_id,
                &active.revision.manifest.revision_id,
            )?;
            let candidate = evaluate_candidate(&store, &authority, source, modules, false)?;
            let grant_review_required = grant_review_required(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &candidate.package,
            )?;
            Ok(json!({
                "valid": candidate.validation.valid,
                "source_bytes": candidate.source.len(),
                "module_count": candidate.assets.iter().filter(|asset| asset.kind == "luau").count(),
                "schema_version": candidate.schema_version,
                "experience_id": candidate.package.experience_id,
                "package": candidate.package,
                "grant_review_required": grant_review_required,
                "report": candidate.validation,
            }))
        }
        AuthoringRequest::SubmitExperience { source, modules } => {
            let active = active_authoring_experience(&store)?;
            let authority = get_authority_experience_state(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &active.package.experience_id,
                &active.revision.manifest.revision_id,
            )?;
            let candidate = validate_candidate(&store, &authority, source, modules)?;
            let grant_review_required = grant_review_required(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &candidate.package,
            )?;
            let revision = store.install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: candidate.source,
                    state: candidate.state,
                    schema_version: candidate.schema_version,
                    experience_api_version: experience_ir::EXPERIENCE_API_VERSION_V4,
                    assets: candidate.assets,
                },
                package: candidate.package,
            })?;
            let graph = GraphResolver::new(store.clone()).resolve(
                &revision.manifest.revision_id,
                &ExportId::parse("main").map_err(|error| anyhow::anyhow!(error.to_string()))?,
            )?;
            let graphs = GraphStore::open(store.root())?;
            let graph_id = graphs.install(&graph)?;
            if active.graph_id == graph_id {
                return Ok(json!({
                    "revision_id": revision.manifest.revision_id,
                    "experience_id": active.package.experience_id,
                    "graph_id": graph_id,
                    "active_revision": active.revision.manifest.revision_id,
                    "active_graph": active.graph_id,
                    "activated": false,
                    "event": "already_active",
                }));
            }
            if grant_review_required {
                return Ok(json!({
                    "revision_id": revision.manifest.revision_id,
                    "experience_id": active.package.experience_id,
                    "graph_id": graph_id,
                    "active_revision": active.revision.manifest.revision_id,
                    "active_graph": active.graph_id,
                    "activated": false,
                    "activation_required": true,
                    "grant_review_required": true,
                }));
            }
            let supervisor =
                activate_graph(&options.supervisor_socket, &graph_id, options.timeout)?;
            Ok(json!({
                "revision_id": revision.manifest.revision_id,
                "experience_id": active.package.experience_id,
                "graph_id": graph_id,
                "active_revision": supervisor.active_revision,
                "active_graph": supervisor.active_graph,
                "activated": true,
                "event": supervisor.event,
            }))
        }
        AuthoringRequest::ValidateDerivedExperience {
            target_experience_id,
            parents,
            state_source,
            provider_capabilities,
            request,
            rationale,
            contract,
            source,
            modules,
        } => {
            let candidate = evaluate_derived_candidate(
                &store,
                target_experience_id,
                parents,
                state_source,
                provider_capabilities,
                request,
                rationale,
                contract,
                source,
                modules,
                Some(&ServiceClient::new(
                    &options.service_socket,
                    options.timeout,
                )),
            )?;
            let grant_review_required = grant_review_required(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &candidate.package,
            )?;
            Ok(json!({
                "valid": true,
                "source_bytes": candidate.source.len(),
                "module_count": candidate.assets.iter().filter(|asset| asset.kind == "luau").count(),
                "schema_version": candidate.schema_version,
                "exports_validated": candidate.exports_validated,
                "package": candidate.package,
                "grant_review_required": grant_review_required,
            }))
        }
        AuthoringRequest::SubmitDerivedExperience {
            target_experience_id,
            parents,
            state_source,
            provider_capabilities,
            request,
            rationale,
            contract,
            source,
            modules,
            replace_existing,
        } => {
            let candidate = evaluate_derived_candidate(
                &store,
                target_experience_id.clone(),
                parents,
                state_source,
                provider_capabilities,
                request,
                rationale,
                contract,
                source,
                modules,
                Some(&ServiceClient::new(
                    &options.service_socket,
                    options.timeout,
                )),
            )?;
            let grant_review_required = grant_review_required(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &candidate.package,
            )?;
            let revision = store.install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: candidate.source,
                    state: candidate.state,
                    schema_version: candidate.schema_version,
                    experience_api_version: experience_ir::EXPERIENCE_API_VERSION_V4,
                    assets: candidate.assets,
                },
                package: candidate.package,
            })?;
            let registry = ExperienceRegistry::open(store.clone())?;
            let registry_current_changed = register_authoring_candidate(
                &registry,
                &target_experience_id,
                &revision.manifest.revision_id,
                replace_existing,
            )?;
            ReverseDependencyIndex::open(store.root()).rebuild(&store, &registry)?;
            let graph_id = if revision.package.as_ref().is_some_and(|package| {
                package
                    .contract
                    .exports
                    .keys()
                    .any(|export| export.as_str() == "main")
            }) {
                let main =
                    ExportId::parse("main").map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let graph = GraphResolver::new(store.clone())
                    .resolve(&revision.manifest.revision_id, &main)?;
                Some(GraphStore::open(store.root())?.install(&graph)?)
            } else {
                None
            };
            Ok(json!({
                "revision_id": revision.manifest.revision_id,
                "experience_id": target_experience_id,
                "graph_id": graph_id,
                "registered": true,
                "registry_current_changed": registry_current_changed,
                "activated": false,
                "activation_required": true,
                "grant_review_required": grant_review_required,
            }))
        }
        AuthoringRequest::ValidateComposedExperience {
            target_experience_id,
            dependencies,
            state_source,
            provider_capabilities,
            contract,
            source,
            modules,
        } => {
            let candidate = evaluate_composed_candidate(
                &store,
                target_experience_id,
                dependencies,
                state_source,
                provider_capabilities,
                contract,
                source,
                modules,
                Some(&ServiceClient::new(
                    &options.service_socket,
                    options.timeout,
                )),
            )?;
            let grant_review_required = grant_review_required(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &candidate.package,
            )?;
            Ok(json!({
                "valid": true,
                "source_bytes": candidate.source.len(),
                "module_count": candidate.assets.iter().filter(|asset| asset.kind == "luau").count(),
                "schema_version": candidate.schema_version,
                "exports_validated": candidate.exports_validated,
                "dependencies_validated": candidate.package.dependencies.len(),
                "package": candidate.package,
                "grant_review_required": grant_review_required,
            }))
        }
        AuthoringRequest::SubmitComposedExperience {
            target_experience_id,
            dependencies,
            state_source,
            provider_capabilities,
            contract,
            source,
            modules,
            replace_existing,
        } => {
            let candidate = evaluate_composed_candidate(
                &store,
                target_experience_id.clone(),
                dependencies,
                state_source,
                provider_capabilities,
                contract,
                source,
                modules,
                Some(&ServiceClient::new(
                    &options.service_socket,
                    options.timeout,
                )),
            )?;
            let grant_review_required = grant_review_required(
                &ServiceClient::new(&options.service_socket, options.timeout),
                &candidate.package,
            )?;
            let revision = store.install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: candidate.source,
                    state: candidate.state,
                    schema_version: candidate.schema_version,
                    experience_api_version: experience_ir::EXPERIENCE_API_VERSION_V4,
                    assets: candidate.assets,
                },
                package: candidate.package,
            })?;
            let registry = ExperienceRegistry::open(store.clone())?;
            let registry_current_changed = register_authoring_candidate(
                &registry,
                &target_experience_id,
                &revision.manifest.revision_id,
                replace_existing,
            )?;
            ReverseDependencyIndex::open(store.root()).rebuild(&store, &registry)?;
            let graph = GraphResolver::new(store.clone()).resolve(
                &revision.manifest.revision_id,
                &ExportId::parse("main").map_err(|error| anyhow::anyhow!(error.to_string()))?,
            )?;
            let graphs = GraphStore::open(store.root())?;
            let graph_id = graphs.install(&graph)?;
            Ok(json!({
                "revision_id": revision.manifest.revision_id,
                "experience_id": target_experience_id,
                "graph_id": graph_id,
                "registered": true,
                "registry_current_changed": registry_current_changed,
                "activated": false,
                "activation_required": true,
                "grant_review_required": grant_review_required,
            }))
        }
    }
}

fn register_authoring_candidate(
    registry: &ExperienceRegistry,
    target_experience_id: &ExperienceId,
    revision_id: &str,
    replace_existing: bool,
) -> Result<bool> {
    match registry.get(target_experience_id)? {
        Some(record) if record.role != experience_package::ExperienceRole::Ordinary => bail!(
            "authoring cannot replace non-ordinary experience `{target_experience_id}`"
        ),
        Some(_) if !replace_existing => bail!(
            "target experience `{target_experience_id}` already exists; replacement was not authorized"
        ),
        Some(_) => Ok(false),
        None => {
            registry.create(
                target_experience_id,
                experience_package::ExperienceRole::Ordinary,
                revision_id,
            )?;
            Ok(true)
        }
    }
}

fn inspect_parents(store: &RevisionStore, parents: &[AuthoringParent]) -> Result<Vec<Value>> {
    validate_parent_selection(store, parents)?;
    parents
        .iter()
        .map(|parent| {
            let revision = store.verify(parent.revision_id.as_str())?;
            let source =
                fs::read_to_string(revision.directory.join(&revision.manifest.source.path))?;
            let modules = load_revision_assets(&revision.directory)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .into_iter()
                .filter(|asset| asset.kind == "luau")
                .map(|asset| {
                    Ok(AuthoringModule {
                        id: asset.id,
                        source: String::from_utf8(asset.bytes)
                            .context("parent contains a non-UTF-8 Luau module")?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({
                "experience_id": parent.experience_id,
                "revision_id": parent.revision_id,
                "source": source,
                "modules": modules,
                "assets": revision.manifest.assets,
                "schema_version": revision.manifest.schema_version,
                "package": revision.package,
            }))
        })
        .collect()
}

fn inspect_dependencies(
    store: &RevisionStore,
    dependencies: &[AuthoringDependency],
) -> Result<(BTreeMap<DependencyAlias, DependencyBinding>, Vec<Value>)> {
    if dependencies.is_empty() || dependencies.len() > experience_package::MAX_DEPENDENCIES {
        bail!("composition requires a bounded non-empty dependency selection");
    }
    let mut bindings = BTreeMap::new();
    let mut contexts = Vec::new();
    for dependency in dependencies {
        let revision = store.verify(dependency.revision_id.as_str())?;
        let package = revision
            .package
            .as_ref()
            .context("composition dependencies must use package format v4")?;
        if package.experience_id != dependency.experience_id {
            bail!("composition dependency revision belongs to a different experience");
        }
        let export = package
            .contract
            .exports
            .get(&dependency.export_id)
            .context("composition dependency names an unknown export")?;
        let binding = DependencyBinding {
            experience_id: dependency.experience_id.clone(),
            revision_id: dependency.revision_id.clone(),
            export_id: dependency.export_id.clone(),
            contract_digest: package
                .contract
                .digest()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            policy: dependency.policy,
            grant: dependency.grant.clone(),
        };
        binding
            .validate(&dependency.alias)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        if bindings
            .insert(dependency.alias.clone(), binding.clone())
            .is_some()
        {
            bail!("composition dependency aliases must be unique");
        }
        let source = fs::read_to_string(revision.directory.join(&revision.manifest.source.path))?;
        contexts.push(json!({
            "alias": dependency.alias,
            "experience_id": dependency.experience_id,
            "revision_id": dependency.revision_id,
            "export_id": dependency.export_id,
            "policy": dependency.policy,
            "grant": dependency.grant,
            "source": source,
            "contract": package.contract,
            "selected_export": export,
            "assets": revision.manifest.assets,
        }));
    }
    Ok((bindings, contexts))
}

fn evaluate_composed_candidate(
    store: &RevisionStore,
    target_experience_id: ExperienceId,
    dependencies: Vec<AuthoringDependency>,
    state_source: AuthoringStateSource,
    provider_capabilities: Vec<String>,
    contract: ExperienceContract,
    source: String,
    modules: Option<Vec<AuthoringModule>>,
    authority: Option<&ServiceClient>,
) -> Result<ValidatedDerivedCandidate> {
    let (bindings, _) = inspect_dependencies(store, &dependencies)?;
    contract
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if !contract
        .exports
        .contains_key(&ExportId::parse("main").map_err(|error| anyhow::anyhow!(error.to_string()))?)
    {
        bail!("a composed top-level experience must export `main`");
    }
    let allowed_state_sources = current_target_state_source(store, &target_experience_id)?
        .into_iter()
        .collect();
    let initial_state =
        resolve_authoring_state(store, state_source, &allowed_state_sources, authority)?;
    let package = PackageMetadata {
        format_version: PACKAGE_FORMAT_VERSION,
        experience_id: target_experience_id,
        role: experience_package::ExperienceRole::Ordinary,
        provider_capabilities: authoring_provider_capabilities(provider_capabilities)?,
        contract,
        dependencies: bindings,
        derivation: DerivationRecord {
            kind: DerivationKind::Original,
            parents: vec![],
            request_sha256: None,
            rationale: None,
        },
        state_migration: None,
    };
    package
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    evaluate_v4_package_candidate(store, package, source, modules, true, initial_state)
}

fn evaluate_v4_package_candidate(
    store: &RevisionStore,
    mut package: PackageMetadata,
    source: String,
    modules: Option<Vec<AuthoringModule>>,
    validate_mounts: bool,
    initial_state: ResolvedAuthoringState,
) -> Result<ValidatedDerivedCandidate> {
    if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
        bail!("experience source is outside the bounded size");
    }
    let assets = new_candidate_assets(modules)?;
    let runtime_assets = assets
        .iter()
        .map(|asset| RuntimeAssetInput {
            id: asset.id.clone(),
            kind: asset.kind.clone(),
            bytes: asset.bytes.clone(),
        })
        .collect();
    let runtime = LuauRuntime::compile_with_assets(&source, runtime_assets)
        .map_err(|error| anyhow::anyhow!("compile API v4 experience: {error}"))?;
    if runtime.api_version() != experience_ir::EXPERIENCE_API_VERSION_V4 {
        bail!("package experiences must use experience API v4");
    }
    let implemented = runtime
        .export_ids()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let declared = package
        .contract
        .exports
        .keys()
        .map(ToString::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    if implemented != declared {
        bail!("source exports do not exactly match the declared contract");
    }
    let state = runtime
        .migrate_state(initial_state.schema_version, &initial_state.state)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let schema_version = runtime
        .state_schema_version()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    package.state_migration = Some(StateMigrationRecord {
        source: initial_state.source,
        target_schema_version: schema_version,
        result_state_sha256: canonical_sha256(&state)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
    });
    package
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut model = providers_fake::snapshot();
    for (export_id, export) in &package.contract.exports {
        let properties = export.properties.example_value();
        for (width, height) in [
            (export.viewport.min_width, export.viewport.min_height),
            (export.viewport.max_width, export.viewport.max_height),
        ] {
            let scene = runtime
                .render_export(
                    export_id.as_str(),
                    &model,
                    &state,
                    &properties,
                    experience_ir::ExperienceViewport {
                        width,
                        height,
                        scale_milli: 1000,
                    },
                    None,
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            if validate_mounts && export_id.as_str() == "main" {
                validate_composition_mounts(store, &package, &scene)?;
            }
        }
        model.appearance.contrast = experience_package::Contrast::High;
        model.appearance.reduce_motion = true;
        runtime
            .render_export(
                export_id.as_str(),
                &model,
                &state,
                &properties,
                experience_ir::ExperienceViewport {
                    width: export.viewport.min_width,
                    height: export.viewport.min_height,
                    scale_milli: 1000,
                },
                None,
            )
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    }
    Ok(ValidatedDerivedCandidate {
        source: source.into_bytes(),
        state,
        schema_version,
        assets,
        exports_validated: package.contract.exports.len(),
        package,
    })
}

fn validate_composition_mounts(
    store: &RevisionStore,
    package: &PackageMetadata,
    scene: &experience_ir::Scene,
) -> Result<()> {
    fn visit<'a>(node: &'a SceneNode, mounts: &mut Vec<&'a experience_ir::ExperienceMountContent>) {
        if let Some(Content::ExperienceMount(mount)) = &node.content {
            mounts.push(mount);
        }
        for child in &node.children {
            visit(child, mounts);
        }
    }
    let mut mounts = Vec::new();
    visit(&scene.root, &mut mounts);
    for (alias, binding) in &package.dependencies {
        let matching = mounts
            .iter()
            .filter(|mount| mount.dependency == alias.as_str())
            .copied()
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            bail!("dependency `{alias}` must have exactly one mount in the main export");
        }
        let child = store.verify(binding.revision_id.as_str())?;
        let child_package = child.package.context("mounted child package is missing")?;
        let export = &child_package.contract.exports[&binding.export_id];
        let experience_package::ValueSchema::Record { fields } = &export.properties else {
            bail!("dependency `{alias}` must expose a closed record property schema");
        };
        if binding
            .grant
            .properties
            .iter()
            .any(|name| !fields.contains_key(name))
            || fields
                .iter()
                .any(|(name, field)| field.required && !binding.grant.properties.contains(name))
            || binding
                .grant
                .events
                .iter()
                .any(|event| !export.events.contains_key(event))
        {
            bail!("dependency `{alias}` has an invalid boundary grant");
        }
        let property_fields = matching[0]
            .properties
            .as_object()
            .context("mounted properties must be a closed record")?;
        if property_fields
            .keys()
            .any(|name| !binding.grant.properties.contains(name))
        {
            bail!("dependency `{alias}` receives a property outside its boundary grant");
        }
        export
            .properties
            .validate_value(&matching[0].properties)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        if matching[0].container_appearance.is_some() && !export.accepts_container_appearance {
            bail!("dependency `{alias}` does not accept container appearance");
        }
    }
    if mounts.iter().any(|mount| {
        !package
            .dependencies
            .keys()
            .any(|alias| alias.as_str() == mount.dependency)
    }) {
        bail!("main export mounts an undeclared dependency");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn evaluate_derived_candidate(
    store: &RevisionStore,
    target_experience_id: ExperienceId,
    parents: Vec<AuthoringParent>,
    state_source: AuthoringStateSource,
    provider_capabilities: Vec<String>,
    request: String,
    rationale: String,
    contract: ExperienceContract,
    source: String,
    modules: Option<Vec<AuthoringModule>>,
    authority: Option<&ServiceClient>,
) -> Result<ValidatedDerivedCandidate> {
    validate_parent_selection(store, &parents)?;
    if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
        bail!("derived experience source is outside the bounded size");
    }
    if request.trim().is_empty() || request.len() > 16 * 1024 {
        bail!("derivation request is outside the bounded size");
    }
    if rationale.trim().is_empty() || rationale.len() > 4096 {
        bail!("derivation rationale is outside the bounded size");
    }
    contract
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut allowed_state_sources = parents
        .iter()
        .map(|parent| (parent.experience_id.clone(), parent.revision_id.clone()))
        .collect::<std::collections::BTreeSet<_>>();
    allowed_state_sources.extend(current_target_state_source(store, &target_experience_id)?);
    let initial_state =
        resolve_authoring_state(store, state_source, &allowed_state_sources, authority)?;
    let derivation = DerivationRecord {
        kind: if parents.len() == 1 {
            DerivationKind::Fork
        } else {
            DerivationKind::Remix
        },
        parents: parents
            .into_iter()
            .map(|parent| DerivationParent {
                experience_id: parent.experience_id,
                revision_id: parent.revision_id,
            })
            .collect(),
        request_sha256: Some(hex_sha256(request.as_bytes())),
        rationale: Some(rationale),
    };
    let package = PackageMetadata {
        format_version: PACKAGE_FORMAT_VERSION,
        experience_id: target_experience_id,
        role: experience_package::ExperienceRole::Ordinary,
        provider_capabilities: authoring_provider_capabilities(provider_capabilities)?,
        contract,
        dependencies: Default::default(),
        derivation,
        state_migration: None,
    };
    package
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    evaluate_v4_package_candidate(store, package, source, modules, false, initial_state)
}

fn current_target_state_source(
    store: &RevisionStore,
    target_experience_id: &ExperienceId,
) -> Result<Option<(ExperienceId, RevisionId)>> {
    let registry = ExperienceRegistry::open(store.clone())?;
    if registry.get(target_experience_id)?.is_none() {
        return Ok(None);
    }
    let Some(current) = registry.current(target_experience_id)? else {
        return Ok(None);
    };
    Ok(Some((
        target_experience_id.clone(),
        RevisionId::parse(current.manifest.revision_id)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
    )))
}

fn authoring_provider_capabilities(
    capabilities: Vec<String>,
) -> Result<std::collections::BTreeSet<String>> {
    if capabilities.len() > experience_package::MAX_SCHEMA_FIELDS {
        bail!("provider capability request is too large");
    }
    let unique = capabilities
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != capabilities.len() {
        bail!("provider capability request contains a duplicate");
    }
    Ok(unique)
}

fn grant_review_required(client: &ServiceClient, package: &PackageMetadata) -> Result<bool> {
    let data_flows: BTreeMap<DependencyAlias, DataFlowGrant> = package
        .dependencies
        .iter()
        .filter(|(_, binding)| {
            !binding.grant.properties.is_empty() || !binding.grant.events.is_empty()
        })
        .map(|(alias, binding)| {
            (
                alias.clone(),
                DataFlowGrant {
                    experience_id: binding.experience_id.clone(),
                    export_id: binding.export_id.clone(),
                    grant: binding.grant.clone(),
                },
            )
        })
        .collect();
    if package.provider_capabilities.is_empty() && data_flows.is_empty() {
        return Ok(false);
    }
    let response = client.call(&ServiceRequest::GetResource {
        request_id: 41,
        query: ResourceQuery::GrantDecisionFor {
            experience_id: package.experience_id.clone(),
        },
    })?;
    let decision = match (response.ok, response.payload, response.error) {
        (
            true,
            Some(ResponsePayload::Resource {
                value: ResourceValue::GrantDecision(decision),
            }),
            _,
        ) => decision,
        (_, _, Some(ServiceError::NotFound { .. })) => return Ok(true),
        (_, _, error) => bail!("grant authority query failed: {error:?}"),
    };
    Ok(!decision.reviewed
        || decision.experience_id != package.experience_id
        || !package
            .provider_capabilities
            .is_subset(&decision.provider_capabilities)
        || data_flows.iter().any(|(alias, requested)| {
            decision.data_flows.get(alias).is_none_or(|approved| {
                approved.experience_id != requested.experience_id
                    || approved.export_id != requested.export_id
                    || !requested
                        .grant
                        .properties
                        .is_subset(&approved.grant.properties)
                    || !requested.grant.events.is_subset(&approved.grant.events)
            })
        }))
}

fn resolve_authoring_state(
    store: &RevisionStore,
    source: AuthoringStateSource,
    allowed: &std::collections::BTreeSet<(ExperienceId, RevisionId)>,
    authority: Option<&ServiceClient>,
) -> Result<ResolvedAuthoringState> {
    match source {
        AuthoringStateSource::Fresh => Ok(ResolvedAuthoringState {
            schema_version: 1,
            state: json!({}),
            source: StateMigrationSource::Fresh,
        }),
        AuthoringStateSource::ExperienceRevision {
            experience_id,
            revision_id,
        } => {
            if !allowed.contains(&(experience_id.clone(), revision_id.clone())) {
                bail!(
                    "state migration source must be an exact selected parent or the current target revision"
                );
            }
            let revision = store.verify(revision_id.as_str())?;
            let package = revision
                .package
                .as_ref()
                .context("state migration sources must use package format v4")?;
            if package.experience_id != experience_id {
                bail!("state migration source revision belongs to a different experience");
            }
            let authority = authority
                .context("revision-backed state migration requires the durable state authority")?;
            let resource =
                get_authority_experience_state(authority, &experience_id, revision_id.as_str())?;
            let durable =
                load_durable_state(&revision.directory.join(&revision.manifest.state.path))?;
            if resource.revision_id != revision_id.as_str()
                || resource.schema_version != durable.schema_version
                || resource.source_sha256 != durable.source_sha256
            {
                bail!("state authority does not retain the exact selected source revision");
            }
            let state_sha256 = canonical_sha256(&resource.state)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            Ok(ResolvedAuthoringState {
                schema_version: resource.schema_version,
                state: resource.state,
                source: StateMigrationSource::ExperienceRevision {
                    experience_id,
                    revision_id,
                    schema_version: resource.schema_version,
                    state_sha256,
                },
            })
        }
    }
}

fn validate_parent_selection(store: &RevisionStore, parents: &[AuthoringParent]) -> Result<()> {
    if parents.is_empty() || parents.len() > experience_package::MAX_DERIVATION_PARENTS {
        bail!("derivation requires a bounded non-empty parent selection");
    }
    let mut unique = HashSet::new();
    for parent in parents {
        if !unique.insert((parent.experience_id.clone(), parent.revision_id.clone())) {
            bail!("derivation parent selection contains a duplicate");
        }
        let revision = store.verify(parent.revision_id.as_str())?;
        let package = revision
            .package
            .as_ref()
            .context("derivation parents must use package format v4")?;
        if package.experience_id != parent.experience_id {
            bail!("derivation parent revision belongs to a different experience");
        }
    }
    Ok(())
}

fn new_candidate_assets(modules: Option<Vec<AuthoringModule>>) -> Result<Vec<StoreAssetInput>> {
    let modules = modules.unwrap_or_default();
    if modules.len() > MAX_AUTHORING_MODULES {
        bail!("candidate has more than {MAX_AUTHORING_MODULES} Luau modules");
    }
    if modules
        .iter()
        .map(|module| module.source.len())
        .sum::<usize>()
        > MAX_AUTHORING_MODULE_BYTES
    {
        bail!("candidate Luau modules are larger than {MAX_AUTHORING_MODULE_BYTES} bytes");
    }
    let mut ids = HashSet::new();
    modules
        .into_iter()
        .map(|module| {
            if module.source.is_empty() || !ids.insert(module.id.clone()) {
                bail!("derived candidate contains an empty or duplicate module");
            }
            Ok(StoreAssetInput {
                id: module.id,
                kind: "luau".into(),
                bytes: module.source.into_bytes(),
            })
        })
        .collect()
}

fn validate_candidate(
    store: &RevisionStore,
    authority: &StateResource,
    source: String,
    modules: Option<Vec<AuthoringModule>>,
) -> Result<ValidatedCandidate> {
    evaluate_candidate(store, authority, source, modules, true)
}

fn active_authoring_experience(store: &RevisionStore) -> Result<ActiveAuthoringExperience> {
    let experience_id = ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let graphs = GraphStore::open(store.root())?;
    let (graph_id, graph) = graphs.current(&experience_id)?.context(
        "the active Stock experience has not migrated to a v4 graph; legacy v3 authoring is disabled",
    )?;
    let root = graph
        .nodes
        .get(&graph.root)
        .context("the active Stock graph has no root node")?;
    if root.experience_id != experience_id {
        bail!("the active Stock graph root has the wrong experience identity");
    }
    let revision = store.verify(root.revision_id.as_str())?;
    let package = revision
        .package
        .clone()
        .context("the active authoring target is not a v4 package")?;
    let registry = ExperienceRegistry::open(store.clone())?;
    let registry_revision = registry
        .current(&experience_id)?
        .context("the active Stock registry record has no current revision")?;
    if registry_revision.manifest.revision_id != revision.manifest.revision_id {
        bail!("the active Stock registry and graph root disagree");
    }
    Ok(ActiveAuthoringExperience {
        graph_id,
        revision,
        package,
    })
}

fn get_authority_experience_state(
    client: &ServiceClient,
    experience_id: &ExperienceId,
    revision_id: &str,
) -> Result<StateResource> {
    let response = client.call(&ServiceRequest::GetResource {
        request_id: 40,
        query: ResourceQuery::ExperienceStateAt {
            experience_id: experience_id.clone(),
            revision_id: revision_id.into(),
        },
    })?;
    if !response.ok {
        bail!(
            "provider authority rejected the experience-state request: {:?}",
            response.error
        );
    }
    match response.payload {
        Some(ResponsePayload::Resource {
            value: ResourceValue::ExperienceStateAt(state),
        }) => Ok(state.resource),
        _ => bail!("provider authority returned the wrong experience-state payload"),
    }
}

fn evaluate_candidate(
    store: &RevisionStore,
    authority: &StateResource,
    source: String,
    modules: Option<Vec<AuthoringModule>>,
    require_valid: bool,
) -> Result<ValidatedCandidate> {
    if source.len() > MAX_SOURCE_BYTES {
        bail!("experience source is larger than {MAX_SOURCE_BYTES} bytes");
    }
    let active = active_authoring_experience(store)?;
    let current = active.revision;
    let durable = load_durable_state(&current.directory.join(&current.manifest.state.path))?;
    if authority.revision_id != current.manifest.revision_id
        || authority.source_sha256 != durable.source_sha256
        || authority.schema_version != durable.schema_version
    {
        bail!("provider authority does not match the active experience binding");
    }
    let assets = candidate_assets(&current.directory, modules)?;
    let runtime_assets = assets
        .iter()
        .map(|asset| RuntimeAssetInput {
            id: asset.id.clone(),
            kind: asset.kind.clone(),
            bytes: asset.bytes.clone(),
        })
        .collect();
    let runtime = LuauRuntime::compile_with_assets(&source, runtime_assets)
        .map_err(|error| anyhow::anyhow!("compile candidate experience: {error}"))?;
    if runtime.api_version() != experience_ir::EXPERIENCE_API_VERSION_V4 {
        bail!("new authoring submissions must use experience API v4");
    }
    let implemented = runtime
        .export_ids()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let declared = active
        .package
        .contract
        .exports
        .keys()
        .map(ToString::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    if implemented != declared {
        bail!("candidate exports do not exactly match the active v4 contract");
    }
    let state = runtime
        .migrate_state(authority.schema_version, &authority.state)
        .map_err(|error| anyhow::anyhow!("migrate candidate experience state: {error}"))?;
    let schema_version = runtime
        .state_schema_version()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut package = active.package;
    package.state_migration = Some(StateMigrationRecord {
        source: StateMigrationSource::ExperienceRevision {
            experience_id: package.experience_id.clone(),
            revision_id: RevisionId::parse(current.manifest.revision_id.clone())
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            schema_version: authority.schema_version,
            state_sha256: canonical_sha256(&authority.state)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        },
        target_schema_version: schema_version,
        result_state_sha256: canonical_sha256(&state)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?,
    });
    package
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let report = runtime
        .validate_all(&providers_fake::snapshot(), &state)
        .map_err(|error| anyhow::anyhow!("validate candidate scenarios: {error}"))?;
    if require_valid && !report.valid {
        let failures = report
            .scenarios
            .iter()
            .filter_map(|scenario| {
                scenario.diagnostic.as_ref().map(|diagnostic| {
                    format!(
                        "{} at {}: {}",
                        scenario.name,
                        diagnostic.path.as_deref().unwrap_or("module"),
                        diagnostic.message
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("; ");
        bail!("candidate validation scenarios failed: {failures}");
    }
    if report.valid {
        if package.role == experience_package::ExperienceRole::Shell
            && !has_shell_agent_composer(&runtime, &state)?
        {
            bail!("candidate must retain a Luau text_session whose submit_action is agent_submit");
        }
        validate_export_viewports(store, &package, &runtime, &state)?;
    }
    Ok(ValidatedCandidate {
        source: source.into_bytes(),
        state,
        schema_version,
        assets,
        validation: report,
        package,
    })
}

fn validate_export_viewports(
    store: &RevisionStore,
    package: &PackageMetadata,
    runtime: &LuauRuntime,
    state: &Value,
) -> Result<()> {
    let mut model = providers_fake::snapshot();
    for (export_id, export) in &package.contract.exports {
        let properties = export.properties.example_value();
        for (width, height) in [
            (export.viewport.min_width, export.viewport.min_height),
            (export.viewport.max_width, export.viewport.max_height),
        ] {
            let scene = runtime
                .render_export(
                    export_id.as_str(),
                    &model,
                    state,
                    &properties,
                    experience_ir::ExperienceViewport {
                        width,
                        height,
                        scale_milli: 1000,
                    },
                    None,
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            if export_id.as_str() == "main" {
                validate_composition_mounts(store, package, &scene)?;
            }
        }
        model.appearance.contrast = experience_package::Contrast::High;
        model.appearance.reduce_motion = true;
        runtime
            .render_export(
                export_id.as_str(),
                &model,
                state,
                &properties,
                experience_ir::ExperienceViewport {
                    width: export.viewport.min_width,
                    height: export.viewport.min_height,
                    scale_milli: 1000,
                },
                None,
            )
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    }
    Ok(())
}

fn candidate_assets(
    current_directory: &Path,
    modules: Option<Vec<AuthoringModule>>,
) -> Result<Vec<StoreAssetInput>> {
    let current = load_revision_assets(current_directory)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut assets = current
        .into_iter()
        .filter(|asset| modules.is_none() || asset.kind != "luau")
        .map(|asset| StoreAssetInput {
            id: asset.id,
            kind: asset.kind,
            bytes: asset.bytes,
        })
        .collect::<Vec<_>>();
    let Some(modules) = modules else {
        return Ok(assets);
    };
    if modules.len() > MAX_AUTHORING_MODULES {
        bail!("candidate has more than {MAX_AUTHORING_MODULES} Luau modules");
    }
    let total = modules
        .iter()
        .map(|module| module.source.len())
        .sum::<usize>();
    if total > MAX_AUTHORING_MODULE_BYTES {
        bail!("candidate Luau modules are larger than {MAX_AUTHORING_MODULE_BYTES} bytes");
    }
    let mut ids = assets
        .iter()
        .map(|asset| asset.id.clone())
        .collect::<HashSet<_>>();
    for module in modules {
        if module.source.is_empty() {
            bail!("candidate Luau module {} is empty", module.id);
        }
        if !ids.insert(module.id.clone()) {
            bail!("candidate has duplicate Luau module id: {}", module.id);
        }
        assets.push(StoreAssetInput {
            id: module.id,
            kind: "luau".into(),
            bytes: module.source.into_bytes(),
        });
    }
    Ok(assets)
}

fn has_agent_composer(node: &SceneNode) -> bool {
    matches!(
        &node.content,
        Some(Content::TextSession(session))
            if session.submit_action.as_deref() == Some("agent_submit")
    ) || node.children.iter().any(has_agent_composer)
}

fn has_shell_agent_composer(runtime: &LuauRuntime, state: &Value) -> Result<bool> {
    for (key, value) in [("active_workspace", "agent"), ("shell_panel", "agent")] {
        let mut candidate = state.clone();
        let object = candidate
            .as_object_mut()
            .context("shell experience state must be a record")?;
        object.insert(key.into(), Value::String(value.into()));
        let scene = runtime
            .render(&providers_fake::snapshot(), &candidate)
            .map_err(|error| anyhow::anyhow!("render shell agent composer branch: {error}"))?;
        if has_agent_composer(&scene.root) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn load_durable_state(path: &Path) -> Result<DurableState> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("decode {}", path.display()))
}

fn activate_graph(socket: &Path, graph_id: &str, timeout: Duration) -> Result<SupervisorResponse> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("connect supervisor socket {}", socket.display()))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    serde_json::to_writer(
        &mut stream,
        &GraphSupervisorRequest {
            action: "activate_graph",
            graph_id,
        },
    )?;
    stream.write_all(b"\n")?;
    let mut line = String::new();
    BufReader::new(stream)
        .take(64 * 1024)
        .read_line(&mut line)?;
    let response: SupervisorResponse =
        serde_json::from_str(&line).context("decode supervisor response")?;
    if !response.ok {
        bail!(
            "supervisor rejected candidate: {}",
            response
                .error
                .unwrap_or_else(|| "unknown activation error".into())
        );
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use revision_supervisor::RevisionAssetInput;
    use tempfile::TempDir;

    fn initialized_store() -> (TempDir, RevisionStore) {
        let temporary = TempDir::new().unwrap();
        let store = RevisionStore::open(temporary.path()).unwrap();
        let source = include_str!("../../../experiences/default.luau");
        let package: PackageMetadata =
            serde_json::from_str(include_str!("../../../experiences/default.package.json"))
                .unwrap();
        let revision = store
            .install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: source.as_bytes().to_vec(),
                    state: json!({}),
                    schema_version: 1,
                    experience_api_version: experience_ir::EXPERIENCE_API_VERSION_V4,
                    assets: vec![RevisionAssetInput {
                        id: "stock.theme".into(),
                        kind: "luau".into(),
                        bytes: include_bytes!("../../../experiences/modules/stock-theme.luau")
                            .to_vec(),
                    }],
                },
                package,
            })
            .unwrap();
        let stock = ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID).unwrap();
        ExperienceRegistry::open(store.clone())
            .unwrap()
            .create(
                &stock,
                experience_package::ExperienceRole::Shell,
                &revision.manifest.revision_id,
            )
            .unwrap();
        let graph = GraphResolver::new(store.clone())
            .resolve(
                &revision.manifest.revision_id,
                &ExportId::parse("main").unwrap(),
            )
            .unwrap();
        let graphs = GraphStore::open(store.root()).unwrap();
        let graph_id = graphs.install(&graph).unwrap();
        graphs.set_current(&stock, &graph_id).unwrap();
        (temporary, store)
    }

    fn authority(store: &RevisionStore) -> StateResource {
        let current = active_authoring_experience(store).unwrap().revision;
        let durable =
            load_durable_state(&current.directory.join(&current.manifest.state.path)).unwrap();
        StateResource {
            revision: 1,
            revision_id: current.manifest.revision_id,
            schema_version: durable.schema_version,
            source_sha256: durable.source_sha256,
            state: durable.state,
        }
    }

    #[test]
    fn validates_an_experience_without_exposing_host_capabilities() {
        let (_temporary, store) = initialized_store();
        let candidate = validate_candidate(
            &store,
            &authority(&store),
            include_str!("../../../experiences/default.luau").into(),
            None,
        )
        .unwrap();
        assert_eq!(candidate.schema_version, 1);
        assert!(!candidate.source.is_empty());
    }

    #[test]
    fn validates_a_visible_complete_stock_edit_with_its_revision_local_theme() {
        let (_temporary, store) = initialized_store();
        let source = include_str!("../../../experiences/default.luau").replace(
            "A responsive workspace assembled entirely by this Luau revision.",
            "Direct input gate active in this complete Stock revision.",
        );
        assert!(source.contains("agent_submit"));
        assert!(source.contains("Direct input gate active in this complete Stock revision."));
        let candidate = validate_candidate(&store, &authority(&store), source, None).unwrap();
        assert!(candidate.validation.valid);
        assert_eq!(candidate.package.experience_id.as_str(), "sos.stock.shell");
        assert_eq!(candidate.assets.len(), 1);
        assert_eq!(candidate.assets[0].id, "stock.theme");
    }

    #[test]
    fn rejects_a_module_that_cannot_render() {
        let (_temporary, store) = initialized_store();
        let error = validate_candidate(
            &store,
            &authority(&store),
            "return { api_version = 4, exports = { main = { render = function() return 5 end } } }"
                .into(),
            None,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("candidate validation scenarios failed"));
    }

    #[test]
    fn validation_returns_the_structured_report_before_submission_rejects() {
        let (_temporary, store) = initialized_store();
        let source = r#"return {
            api_version = 4,
            exports = { main = { render = function()
                return { id = "root", children = {{ interaction = { tap_action = "open" } }} }
            end } },
        }"#;
        let evaluated =
            evaluate_candidate(&store, &authority(&store), source.into(), None, false).unwrap();
        assert!(!evaluated.validation.valid);
        let diagnostic = evaluated.validation.scenarios[0]
            .diagnostic
            .as_ref()
            .unwrap();
        assert_eq!(diagnostic.path.as_deref(), Some("root#root.children[0]"));

        let error =
            validate_candidate(&store, &authority(&store), source.into(), None).unwrap_err();
        assert!(error
            .to_string()
            .contains("candidate validation scenarios failed"));
    }

    #[test]
    fn rejects_an_agent_candidate_that_removes_its_luau_composer() {
        let (_temporary, store) = initialized_store();
        let error = validate_candidate(
            &store,
            &authority(&store),
            r#"return {
                api_version = 4,
                exports = { main = { render = function() return { id = "root" } end } },
            }"#
            .into(),
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("retain a Luau text_session"));
    }

    #[test]
    fn validates_revision_local_modules_as_one_candidate_package() {
        let (_temporary, store) = initialized_store();
        let source = r#"
            local composer = require("test.composer")
            return {
                api_version = 4,
                exports = { main = { render = function()
                    return { id = "root", children = { composer } }
                end } },
            }
        "#;
        let candidate = validate_candidate(
            &store,
            &authority(&store),
            source.into(),
            Some(vec![AuthoringModule {
                id: "test.composer".into(),
                source: r#"return {
                    id = "agent",
                    content = {
                        kind = "text_session",
                        state_key = "draft",
                        value = "",
                        submit_action = "agent_submit",
                    },
                }"#
                .into(),
            }]),
        )
        .unwrap();
        assert!(candidate.validation.valid);
        assert_eq!(candidate.assets.len(), 1);
        assert_eq!(candidate.assets[0].id, "test.composer");
    }

    #[test]
    fn validates_a_self_contained_remix_against_exact_parent_revisions() {
        let temporary = TempDir::new().unwrap();
        let store = RevisionStore::open(temporary.path()).unwrap();
        let reference = revision_supervisor::install_reference_composition(&store).unwrap();
        let remix = store.verify(&reference.remix_revision).unwrap();
        let candidate = evaluate_derived_candidate(
            &store,
            ExperienceId::parse("sos.example.user-remix").unwrap(),
            vec![
                AuthoringParent {
                    experience_id: ExperienceId::parse("sos.example.agenda").unwrap(),
                    revision_id: RevisionId::parse(reference.agenda_revision).unwrap(),
                },
                AuthoringParent {
                    experience_id: ExperienceId::parse("sos.example.media").unwrap(),
                    revision_id: RevisionId::parse(reference.media_revision).unwrap(),
                },
            ],
            AuthoringStateSource::Fresh,
            Vec::new(),
            "Combine agenda and media".into(),
            "The result needs one information architecture.".into(),
            remix.package.unwrap().contract,
            include_str!("../../../experiences/composition/agenda-media-remix.luau").into(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(candidate.package.derivation.kind, DerivationKind::Remix);
        assert_eq!(candidate.package.derivation.parents.len(), 2);
        assert!(candidate.package.dependencies.is_empty());
        assert!(matches!(
            candidate.package.state_migration,
            Some(StateMigrationRecord {
                source: StateMigrationSource::Fresh,
                ..
            })
        ));
        assert_eq!(candidate.exports_validated, 1);
    }

    #[test]
    fn validates_a_live_composition_against_exact_dependency_boundaries() {
        let temporary = TempDir::new().unwrap();
        let store = RevisionStore::open(temporary.path()).unwrap();
        let reference = revision_supervisor::install_reference_composition(&store).unwrap();
        let dashboard = store.verify(&reference.dashboard_revision).unwrap();
        let package = dashboard.package.unwrap();
        let dependencies = package
            .dependencies
            .iter()
            .map(|(alias, binding)| AuthoringDependency {
                alias: alias.clone(),
                experience_id: binding.experience_id.clone(),
                revision_id: binding.revision_id.clone(),
                export_id: binding.export_id.clone(),
                policy: binding.policy,
                grant: binding.grant.clone(),
            })
            .collect();
        let candidate = evaluate_composed_candidate(
            &store,
            ExperienceId::parse("sos.example.user-dashboard").unwrap(),
            dependencies,
            AuthoringStateSource::Fresh,
            Vec::new(),
            package.contract,
            include_str!("../../../experiences/composition/dashboard.luau").into(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(candidate.package.derivation.kind, DerivationKind::Original);
        assert_eq!(candidate.package.dependencies.len(), 2);
        assert!(candidate.package.state_migration.is_some());
        assert_eq!(candidate.exports_validated, 1);
    }

    #[test]
    fn submitting_a_replacement_leaves_the_registry_pointer_for_activation() {
        let temporary = TempDir::new().unwrap();
        let store = RevisionStore::open(temporary.path()).unwrap();
        let reference = revision_supervisor::install_reference_composition(&store).unwrap();
        let dashboard_id = ExperienceId::parse("sos.example.dashboard").unwrap();
        let registry = ExperienceRegistry::open(store.clone()).unwrap();
        let current_before = registry
            .current(&dashboard_id)
            .unwrap()
            .unwrap()
            .manifest
            .revision_id;
        assert_eq!(current_before, reference.dashboard_revision);

        let dashboard = store.verify(&reference.dashboard_revision).unwrap();
        let package = dashboard.package.unwrap();
        let dependencies = package
            .dependencies
            .iter()
            .map(|(alias, binding)| AuthoringDependency {
                alias: alias.clone(),
                experience_id: binding.experience_id.clone(),
                revision_id: binding.revision_id.clone(),
                export_id: binding.export_id.clone(),
                policy: binding.policy,
                grant: binding.grant.clone(),
            })
            .collect();
        let candidate = evaluate_composed_candidate(
            &store,
            dashboard_id.clone(),
            dependencies,
            AuthoringStateSource::Fresh,
            Vec::new(),
            package.contract,
            format!(
                "{}\n",
                include_str!("../../../experiences/composition/dashboard.luau")
            ),
            None,
            None,
        )
        .unwrap();
        let replacement = store
            .install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: candidate.source,
                    state: candidate.state,
                    schema_version: candidate.schema_version,
                    experience_api_version: experience_ir::EXPERIENCE_API_VERSION_V4,
                    assets: candidate.assets,
                },
                package: candidate.package,
            })
            .unwrap();
        assert_ne!(replacement.manifest.revision_id, current_before);

        let changed = register_authoring_candidate(
            &registry,
            &dashboard_id,
            &replacement.manifest.revision_id,
            true,
        )
        .unwrap();
        assert!(!changed);
        assert_eq!(
            registry
                .current(&dashboard_id)
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            current_before
        );
    }
}
