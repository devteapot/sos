use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
};

use experience_ir::{ProviderEffect, ProviderRequest, ProviderResponse, MAX_STATE_BYTES};
use experience_package::{
    AppearanceProfile, ExperienceId, GraphNodeId, InstanceId, PackageMetadata, ResolvedGraph,
    RevisionId,
};
use serde::{Deserialize, Serialize};
use service_protocol::{
    AppearanceResource, ExperienceStateResource, GrantDecisionResource, StateResource,
};

pub const REVISION_ADDRESS: &str = "127.0.0.1:47778";
pub const CORE_PROVIDER_SOCKET: &str = "/data/misc/sos/provider.sock";
pub const CORE_REVISION_SOCKET: &str = "/data/misc/sos/revision.sock";
pub const MAX_PROVIDER_REQUEST_BYTES: u64 = (MAX_STATE_BYTES + 64 * 1024) as u64;
// RevisionAssetWire uses serde's JSON byte-array representation. The runtime's
// 16 MiB raw sidecar ceiling can therefore expand to roughly 64 MiB on the
// wire; retain an explicit bound with enough headroom for source and metadata.
pub const MAX_REVISION_REQUEST_BYTES: u64 = 96 * 1024 * 1024;
pub const MAX_REVISION_RESPONSE_BYTES: u64 = 96 * 1024 * 1024;

pub fn read_provider_request<R: Read>(reader: &mut R) -> std::io::Result<ProviderRequest> {
    let mut line = String::new();
    {
        let mut bounded = BufReader::new(reader).take(MAX_PROVIDER_REQUEST_BYTES + 1);
        bounded.read_line(&mut line)?;
    }
    if line.len() as u64 > MAX_PROVIDER_REQUEST_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "provider request exceeded its size limit",
        ));
    }
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

pub fn write_provider_response<W: Write>(
    writer: &mut W,
    response: &ProviderResponse,
) -> std::io::Result<()> {
    let mut encoded = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    encoded.push(b'\n');
    writer.write_all(&encoded)?;
    writer.flush()
}

pub fn request_provider_over_stream<S: Read + Write>(
    mut stream: S,
    request: ProviderRequest,
) -> Result<ProviderResponse, String> {
    let expected_id = request.request_id();
    serde_json::to_writer(&mut stream, &request).map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;

    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let response: ProviderResponse =
        serde_json::from_str(&line).map_err(|error| error.to_string())?;
    if response.request_id != expected_id {
        return Err("provider response request id did not match".into());
    }
    Ok(response)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RevisionAssetWire {
    pub id: String,
    pub kind: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphRevisionWire {
    pub revision_id: String,
    pub source: String,
    pub assets: Vec<RevisionAssetWire>,
    pub package: PackageMetadata,
    pub state: ExperienceStateResource,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphBundle {
    pub graph_id: String,
    pub graph: ResolvedGraph,
    pub revisions: Vec<GraphRevisionWire>,
    pub appearance: AppearanceResource,
    #[serde(default)]
    pub grants: Vec<GrantDecisionResource>,
    #[serde(default)]
    pub migration_pending: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphStateUpdateWire {
    pub node_id: GraphNodeId,
    pub instance_id: InstanceId,
    pub experience_id: ExperienceId,
    pub revision_id: RevisionId,
    pub expected_revision: u64,
    pub state: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AuthorityAuditSnapshot {
    pub format_version: u32,
    #[serde(default)]
    pub presented_experience: Option<ExperienceId>,
    #[serde(default)]
    pub states: BTreeMap<ExperienceId, StateResource>,
    #[serde(default)]
    pub appearance: AppearanceResource,
    #[serde(default)]
    pub grants: BTreeMap<ExperienceId, GrantDecisionResource>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphEffectWire {
    pub node_id: GraphNodeId,
    pub instance_id: InstanceId,
    pub revision_id: RevisionId,
    pub effect: ProviderEffect,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RevisionRequest {
    CurrentGraph {
        request_id: u64,
    },
    AuditSnapshot {
        request_id: u64,
    },
    PresentExperience {
        request_id: u64,
        expected_graph_id: String,
        experience_id: ExperienceId,
    },
    DismissExperience {
        request_id: u64,
        expected_graph_id: String,
        experience_id: ExperienceId,
    },
    ConfirmGraph {
        request_id: u64,
        graph_id: String,
    },
    RollbackGraph {
        request_id: u64,
        failed_graph_id: String,
    },
    StageGraphRevision {
        request_id: u64,
        expected_graph_id: String,
        package: PackageMetadata,
        source: String,
        state: serde_json::Value,
        schema_version: u64,
        assets: Vec<RevisionAssetWire>,
    },
    DiscardGraph {
        request_id: u64,
        graph_id: String,
    },
    CommitGraphAction {
        request_id: u64,
        graph_id: String,
        updates: Vec<GraphStateUpdateWire>,
        effects: Vec<GraphEffectWire>,
    },
    CurrentAppearance {
        request_id: u64,
    },
    SetExperienceAppearance {
        request_id: u64,
        expected_graph_id: String,
        writer_experience_id: ExperienceId,
        expected_generation: u64,
        profile: AppearanceProfile,
    },
    UpdateAppearance {
        request_id: u64,
        expected_generation: u64,
        capability: String,
        profile: AppearanceProfile,
    },
}

impl RevisionRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::CurrentGraph { request_id }
            | Self::AuditSnapshot { request_id }
            | Self::PresentExperience { request_id, .. }
            | Self::DismissExperience { request_id, .. }
            | Self::ConfirmGraph { request_id, .. }
            | Self::RollbackGraph { request_id, .. }
            | Self::StageGraphRevision { request_id, .. }
            | Self::DiscardGraph { request_id, .. }
            | Self::CommitGraphAction { request_id, .. }
            | Self::CurrentAppearance { request_id }
            | Self::SetExperienceAppearance { request_id, .. }
            | Self::UpdateAppearance { request_id, .. } => *request_id,
        }
    }
}

pub fn read_revision_request<R: Read>(reader: &mut R) -> std::io::Result<RevisionRequest> {
    let mut line = Vec::new();
    {
        let mut bounded = BufReader::new(reader).take(MAX_REVISION_REQUEST_BYTES + 1);
        bounded.read_until(b'\n', &mut line)?;
    }
    if line.len() as u64 > MAX_REVISION_REQUEST_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "revision request exceeded its size limit",
        ));
    }
    if !line.ends_with(b"\n") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "revision request ended before its newline delimiter",
        ));
    }
    serde_json::from_slice(&line)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct RevisionResponse {
    pub request_id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_snapshot: Option<AuthorityAuditSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<AppearanceResource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<ExperienceStateResource>,
    pub error: Option<String>,
}

pub fn write_revision_response<W: Write>(
    writer: &mut W,
    response: &RevisionResponse,
) -> std::io::Result<()> {
    let mut encoded = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    encoded.push(b'\n');
    writer.write_all(&encoded)?;
    writer.flush()
}

pub fn request_revision_over_stream<S: Read + Write>(
    mut stream: S,
    request: RevisionRequest,
) -> Result<RevisionResponse, String> {
    let expected_id = request.request_id();
    serde_json::to_writer(&mut stream, &request).map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;

    let mut line = Vec::new();
    BufReader::new(stream)
        .take(MAX_REVISION_RESPONSE_BYTES + 1)
        .read_until(b'\n', &mut line)
        .map_err(|error| error.to_string())?;
    if line.len() as u64 > MAX_REVISION_RESPONSE_BYTES {
        return Err("revision response exceeded its size limit".into());
    }
    if !line.ends_with(b"\n") {
        return Err(if line.is_empty() {
            "revision authority closed the connection before a response".into()
        } else {
            "revision authority returned a truncated response".into()
        });
    }
    let response: RevisionResponse =
        serde_json::from_slice(&line).map_err(|error| error.to_string())?;
    if response.request_id != expected_id {
        return Err("revision response request id did not match".into());
    }
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "revision authority rejected request".into()));
    }
    Ok(response)
}

#[cfg(all(test, unix))]
mod tests {
    use std::{io::Write as _, net::Shutdown, os::unix::net::UnixStream, thread, time::Duration};

    use experience_ir::{ProviderRequest, ProviderResponse};

    use super::{
        read_provider_request, read_revision_request, request_provider_over_stream,
        request_revision_over_stream, write_provider_response, RevisionRequest,
    };

    fn response(request_id: u64) -> ProviderResponse {
        ProviderResponse {
            request_id,
            ok: true,
            model: None,
            result: None,
            state: None,
            stage_id: None,
            error: None,
        }
    }

    #[test]
    fn client_rejects_eof_before_response() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            read_provider_request(&mut server).unwrap();
            server.shutdown(Shutdown::Write).unwrap();
        });
        let error =
            request_provider_over_stream(client, ProviderRequest::Snapshot { request_id: 1 })
                .unwrap_err();
        worker.join().unwrap();
        assert!(error.contains("EOF"));
    }

    #[test]
    fn client_rejects_truncated_response() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            read_provider_request(&mut server).unwrap();
            server.write_all(br#"{"request_id":2,"ok":true"#).unwrap();
        });
        let result =
            request_provider_over_stream(client, ProviderRequest::Snapshot { request_id: 2 });
        worker.join().unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn short_client_deadline_closes_before_delayed_server_write() {
        let (client, mut server) = UnixStream::pair().unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        let worker = thread::spawn(move || {
            read_provider_request(&mut server).unwrap();
            thread::sleep(Duration::from_millis(50));
            write_provider_response(&mut server, &response(3)).unwrap_err()
        });
        let result =
            request_provider_over_stream(client, ProviderRequest::Snapshot { request_id: 3 });
        assert!(result.is_err());
        let write_error = worker.join().unwrap();
        assert!(matches!(
            write_error.kind(),
            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
        ));
    }

    #[test]
    fn revision_server_rejects_empty_and_unterminated_requests() {
        let empty = read_revision_request(&mut &b""[..]).unwrap_err();
        assert_eq!(empty.kind(), std::io::ErrorKind::UnexpectedEof);
        let truncated =
            read_revision_request(&mut &br#"{"action":"current_graph","request_id":4}"#[..])
                .unwrap_err();
        assert_eq!(truncated.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    fn revision_response_error(response: &'static [u8]) -> String {
        let (client, mut server) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            read_revision_request(&mut server).unwrap();
            server.write_all(response).unwrap();
            server.shutdown(Shutdown::Write).unwrap();
        });
        let error =
            request_revision_over_stream(client, RevisionRequest::CurrentGraph { request_id: 17 })
                .unwrap_err();
        worker.join().unwrap();
        error
    }

    #[test]
    fn revision_client_reports_empty_and_truncated_responses() {
        assert_eq!(
            revision_response_error(b""),
            "revision authority closed the connection before a response"
        );
        assert_eq!(
            revision_response_error(br#"{"request_id":17,"ok":true}"#),
            "revision authority returned a truncated response"
        );
    }

    #[test]
    fn audit_snapshot_has_a_stable_bounded_wire_action() {
        let request = RevisionRequest::AuditSnapshot { request_id: 91 };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"action": "audit_snapshot", "request_id": 91})
        );
        assert_eq!(request.request_id(), 91);
    }

    #[test]
    fn graph_protocol_rejects_retired_single_revision_actions() {
        for action in ["current", "install", "activate", "fallback_to_stock"] {
            let wire = format!(r#"{{"action":"{action}","request_id":1}}"#);
            assert!(serde_json::from_str::<RevisionRequest>(&wire).is_err());
        }
    }
}
