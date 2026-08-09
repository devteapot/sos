use std::{
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
use nix::{
    sys::socket::{getsockopt, sockopt::PeerCredentials},
    unistd::Uid,
};
use provider_state_service::ServiceClient;
use revision_supervisor::{DurableState, RevisionInput, RevisionStore};
use runtime_luau::{LuauRuntime, MAX_SOURCE_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use service_protocol::StateResource;

use crate::stage_revision;

const MAX_REQUEST_BYTES: u64 = (MAX_SOURCE_BYTES as u64) + 16 * 1024;
const EXPERIENCE_API_VERSION: u32 = 3;

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
    ValidateExperience { source: String },
    SubmitExperience { source: String },
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
struct SupervisorRequest<'a> {
    action: &'static str,
    revision_id: &'a str,
    transaction_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct SupervisorResponse {
    ok: bool,
    active_revision: Option<String>,
    event: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
struct ValidatedCandidate {
    source: Vec<u8>,
    state: Value,
    schema_version: u64,
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
            let current = store
                .current()?
                .context("the Linux session has no active experience")?;
            let source = fs::read_to_string(current.directory.join(&current.manifest.source.path))?;
            let durable =
                load_durable_state(&current.directory.join(&current.manifest.state.path))?;
            Ok(json!({
                "revision_id": current.manifest.revision_id,
                "source": source,
                "schema_version": durable.schema_version,
                "experience_api_version": current.manifest.experience_api_version,
                "assets_supported": false,
            }))
        }
        AuthoringRequest::ValidateExperience { source } => {
            let authority = crate::get_state(&ServiceClient::new(
                &options.service_socket,
                options.timeout,
            ))?;
            let candidate = validate_candidate(&store, &authority, source)?;
            Ok(json!({
                "valid": true,
                "source_bytes": candidate.source.len(),
                "schema_version": candidate.schema_version,
            }))
        }
        AuthoringRequest::SubmitExperience { source } => {
            let authority = crate::get_state(&ServiceClient::new(
                &options.service_socket,
                options.timeout,
            ))?;
            let candidate = validate_candidate(&store, &authority, source)?;
            let revision = store.install(RevisionInput {
                source: candidate.source,
                state: candidate.state,
                schema_version: candidate.schema_version,
                experience_api_version: EXPERIENCE_API_VERSION,
                assets: Vec::new(),
            })?;
            let current_id = store.current()?.map(|current| current.manifest.revision_id);
            if current_id.as_deref() == Some(&revision.manifest.revision_id) {
                return Ok(json!({
                    "revision_id": revision.manifest.revision_id,
                    "active_revision": current_id,
                    "activated": false,
                    "event": "already_active",
                }));
            }
            let transaction_id = stage_revision(
                &options.revision_root,
                &revision.manifest.revision_id,
                &options.service_socket,
                options.timeout,
            )?;
            let supervisor = activate(
                &options.supervisor_socket,
                &revision.manifest.revision_id,
                &transaction_id,
                options.timeout,
            )?;
            Ok(json!({
                "revision_id": revision.manifest.revision_id,
                "active_revision": supervisor.active_revision,
                "transaction_id": transaction_id,
                "activated": true,
                "event": supervisor.event,
            }))
        }
    }
}

fn validate_candidate(
    store: &RevisionStore,
    authority: &StateResource,
    source: String,
) -> Result<ValidatedCandidate> {
    if source.len() > MAX_SOURCE_BYTES {
        bail!("experience source is larger than {MAX_SOURCE_BYTES} bytes");
    }
    let current = store
        .current()?
        .context("the Linux session has no active experience")?;
    let durable = load_durable_state(&current.directory.join(&current.manifest.state.path))?;
    if authority.revision_id != current.manifest.revision_id
        || authority.source_sha256 != durable.source_sha256
        || authority.schema_version != durable.schema_version
    {
        bail!("provider authority does not match the active experience binding");
    }
    let runtime = LuauRuntime::compile(&source)
        .map_err(|error| anyhow::anyhow!("compile candidate experience: {error}"))?;
    let state = runtime
        .migrate_state(authority.schema_version, &authority.state)
        .map_err(|error| anyhow::anyhow!("migrate candidate experience state: {error}"))?;
    let schema_version = runtime
        .state_schema_version()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let scene = runtime
        .render(&providers_fake::snapshot(), &state)
        .map_err(|error| {
            anyhow::anyhow!("render candidate with the deterministic provider snapshot: {error}")
        })?;
    if !has_agent_composer(&scene.root) {
        bail!("candidate must retain a Luau text_session whose submit_action is agent_submit");
    }
    Ok(ValidatedCandidate {
        source: source.into_bytes(),
        state,
        schema_version,
    })
}

fn has_agent_composer(node: &SceneNode) -> bool {
    matches!(
        &node.content,
        Some(Content::TextSession(session))
            if session.submit_action.as_deref() == Some("agent_submit")
    ) || node.children.iter().any(has_agent_composer)
}

fn load_durable_state(path: &Path) -> Result<DurableState> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("decode {}", path.display()))
}

fn activate(
    socket: &Path,
    revision_id: &str,
    transaction_id: &str,
    timeout: Duration,
) -> Result<SupervisorResponse> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("connect supervisor socket {}", socket.display()))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    serde_json::to_writer(
        &mut stream,
        &SupervisorRequest {
            action: "activate",
            revision_id,
            transaction_id,
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
        let revision = store
            .install(RevisionInput {
                source: source.as_bytes().to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: EXPERIENCE_API_VERSION,
                assets: Vec::<RevisionAssetInput>::new(),
            })
            .unwrap();
        store.set_current(&revision.manifest.revision_id).unwrap();
        (temporary, store)
    }

    fn authority(store: &RevisionStore) -> StateResource {
        let current = store.current().unwrap().unwrap();
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
            include_str!("../../../experiences/daily-flow.luau").into(),
        )
        .unwrap();
        assert_eq!(candidate.schema_version, 1);
        assert!(!candidate.source.is_empty());
    }

    #[test]
    fn rejects_a_module_that_cannot_render() {
        let (_temporary, store) = initialized_store();
        let error = validate_candidate(
            &store,
            &authority(&store),
            "return { api_version = 3, render = function() return 5 end }".into(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("render candidate"));
    }

    #[test]
    fn rejects_an_agent_candidate_that_removes_its_luau_composer() {
        let (_temporary, store) = initialized_store();
        let error = validate_candidate(
            &store,
            &authority(&store),
            r#"return {
                api_version = 3,
                render = function() return { id = "root" } end,
            }"#
            .into(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("retain a Luau text_session"));
    }
}
