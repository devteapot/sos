use std::io::{BufRead, BufReader, Read, Write};

use experience_ir::{ProviderRequest, ProviderResponse, StateEnvelope, MAX_STATE_BYTES};
use serde::{Deserialize, Serialize};

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
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RevisionRequest {
    Current {
        request_id: u64,
    },
    Install {
        request_id: u64,
        source: String,
        state: serde_json::Value,
        schema_version: u64,
        experience_api_version: u32,
        assets: Vec<RevisionAssetWire>,
    },
    Activate {
        request_id: u64,
        revision_id: String,
        state_stage_id: u64,
    },
    /// Restore the authority-pinned stock experience after the active
    /// generated revision fails validation during host startup. The failed id
    /// prevents a stale host from rolling back a newer activation.
    FallbackToStock {
        request_id: u64,
        failed_revision_id: String,
    },
}

impl RevisionRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::Current { request_id }
            | Self::Install { request_id, .. }
            | Self::Activate { request_id, .. }
            | Self::FallbackToStock { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct RevisionResponse {
    pub request_id: u64,
    pub ok: bool,
    pub revision_id: Option<String>,
    pub source: Option<String>,
    pub state: Option<StateEnvelope>,
    pub assets: Vec<RevisionAssetWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_revision_id: Option<String>,
    #[serde(default)]
    pub stock_trusted: bool,
    #[serde(default)]
    pub fallback_performed: bool,
    pub error: Option<String>,
}

#[cfg(all(test, unix))]
mod tests {
    use std::{io::Write as _, net::Shutdown, os::unix::net::UnixStream, thread, time::Duration};

    use experience_ir::{ProviderRequest, ProviderResponse};

    use super::{read_provider_request, request_provider_over_stream, write_provider_response};

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
}
