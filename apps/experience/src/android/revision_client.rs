use std::{
    io::{BufRead, BufReader, Read, Write},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

#[cfg(not(feature = "core-native"))]
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(feature = "core-native")]
use std::os::unix::net::UnixStream;

#[cfg(feature = "core-native")]
use android_authority_protocol::CORE_REVISION_SOCKET;
#[cfg(not(feature = "core-native"))]
use android_authority_protocol::REVISION_ADDRESS;
use android_authority_protocol::{
    RevisionAssetWire, RevisionRequest, RevisionResponse, MAX_REVISION_RESPONSE_BYTES,
};
use runtime_luau::{RevisionAsset, RevisionAssetInput};
use serde_json::Value as JsonValue;

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn current_with_retry() -> Result<RevisionResponse, String> {
    let mut last_error = String::new();
    for _ in 0..100 {
        match request(RevisionRequest::Current {
            request_id: allocate_request_id(),
        }) {
            Ok(response) => return Ok(response),
            Err(error) => last_error = error,
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "on-device revision authority was unavailable for 5 seconds: {last_error}"
    ))
}

pub(super) fn install(
    source: String,
    state: JsonValue,
    schema_version: u64,
    assets: &[RevisionAsset],
) -> Result<String, String> {
    let response = request(RevisionRequest::Install {
        request_id: allocate_request_id(),
        source,
        state,
        schema_version,
        experience_api_version: experience_ir::EXPERIENCE_API_VERSION,
        assets: assets
            .iter()
            .map(|asset| RevisionAssetWire {
                id: asset.id.clone(),
                kind: asset.kind.clone(),
                bytes: asset.bytes.clone(),
            })
            .collect(),
    })?;
    response
        .revision_id
        .ok_or_else(|| "revision install response omitted its id".to_owned())
}

pub(super) fn activate(
    revision_id: String,
    state_stage_id: u64,
) -> Result<RevisionResponse, String> {
    request(RevisionRequest::Activate {
        request_id: allocate_request_id(),
        revision_id,
        state_stage_id,
    })
}

pub(super) fn fallback_to_stock(failed_revision_id: String) -> Result<RevisionResponse, String> {
    request(RevisionRequest::FallbackToStock {
        request_id: allocate_request_id(),
        failed_revision_id,
    })
}

pub(super) fn inputs(assets: Vec<RevisionAssetWire>) -> Vec<RevisionAssetInput> {
    assets
        .into_iter()
        .map(|asset| RevisionAssetInput {
            id: asset.id,
            kind: asset.kind,
            bytes: asset.bytes,
        })
        .collect()
}

fn request(request: RevisionRequest) -> Result<RevisionResponse, String> {
    let expected_id = request.request_id();
    #[cfg(feature = "core-native")]
    let stream = connect_core()?;
    #[cfg(not(feature = "core-native"))]
    let stream = connect_tcp()?;
    request_over_stream(stream, request, expected_id)
}

#[cfg(feature = "core-native")]
fn connect_core() -> Result<UnixStream, String> {
    let stream = UnixStream::connect(CORE_REVISION_SOCKET).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    Ok(stream)
}

#[cfg(not(feature = "core-native"))]
fn connect_tcp() -> Result<TcpStream, String> {
    let address = REVISION_ADDRESS
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| "revision authority address did not resolve".to_owned())?;
    let stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    Ok(stream)
}

fn request_over_stream<S: Read + Write>(
    mut stream: S,
    request: RevisionRequest,
    expected_id: u64,
) -> Result<RevisionResponse, String> {
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

fn allocate_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}
