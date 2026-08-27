use std::{
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
    request_revision_over_stream, GraphEffectWire, GraphStateUpdateWire, RevisionAssetWire,
    RevisionRequest, RevisionResponse,
};
use experience_package::{AppearanceProfile, ExperienceId, PackageMetadata};
use runtime_luau::{RevisionAsset, RevisionAssetInput};
use serde_json::Value as JsonValue;
use service_protocol::{AppearanceResource, ExperienceStateResource};

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn current_with_retry() -> Result<RevisionResponse, String> {
    current_request_with_retry(false)
}

pub(super) fn current_graph_with_retry() -> Result<RevisionResponse, String> {
    current_request_with_retry(true)
}

pub(super) fn present_experience(
    expected_graph_id: String,
    experience_id: ExperienceId,
) -> Result<RevisionResponse, String> {
    request(RevisionRequest::PresentExperience {
        request_id: allocate_request_id(),
        expected_graph_id,
        experience_id,
    })
}

pub(super) fn dismiss_experience(
    expected_graph_id: String,
    experience_id: ExperienceId,
) -> Result<RevisionResponse, String> {
    request(RevisionRequest::DismissExperience {
        request_id: allocate_request_id(),
        expected_graph_id,
        experience_id,
    })
}

fn current_request_with_retry(graph: bool) -> Result<RevisionResponse, String> {
    let mut last_error = String::new();
    for _ in 0..100 {
        let request_id = allocate_request_id();
        let current = if graph {
            RevisionRequest::CurrentGraph { request_id }
        } else {
            RevisionRequest::Current { request_id }
        };
        match request(current) {
            Ok(response) => return Ok(response),
            Err(error) => last_error = error,
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "on-device revision authority was unavailable for 5 seconds: {last_error}"
    ))
}

pub(super) fn confirm_graph(graph_id: String) -> Result<RevisionResponse, String> {
    request(RevisionRequest::ConfirmGraph {
        request_id: allocate_request_id(),
        graph_id,
    })
}

pub(super) fn rollback_graph(failed_graph_id: String) -> Result<RevisionResponse, String> {
    request(RevisionRequest::RollbackGraph {
        request_id: allocate_request_id(),
        failed_graph_id,
    })
}

pub(super) fn stage_graph_revision(
    expected_graph_id: String,
    package: PackageMetadata,
    source: String,
    state: JsonValue,
    schema_version: u64,
    assets: Vec<RevisionAssetWire>,
) -> Result<RevisionResponse, String> {
    request(RevisionRequest::StageGraphRevision {
        request_id: allocate_request_id(),
        expected_graph_id,
        package,
        source,
        state,
        schema_version,
        assets,
    })
}

pub(super) fn discard_graph(graph_id: String) -> Result<RevisionResponse, String> {
    request(RevisionRequest::DiscardGraph {
        request_id: allocate_request_id(),
        graph_id,
    })
}

pub(super) fn commit_graph_action(
    graph_id: String,
    updates: Vec<GraphStateUpdateWire>,
    effects: Vec<GraphEffectWire>,
) -> Result<Vec<ExperienceStateResource>, String> {
    request(RevisionRequest::CommitGraphAction {
        request_id: allocate_request_id(),
        graph_id,
        updates,
        effects,
    })
    .map(|response| response.states)
}

pub(super) fn current_appearance() -> Result<AppearanceResource, String> {
    request(RevisionRequest::CurrentAppearance {
        request_id: allocate_request_id(),
    })?
    .appearance
    .ok_or_else(|| "appearance response omitted its resource".into())
}

pub(super) fn set_experience_appearance(
    expected_graph_id: String,
    expected_generation: u64,
    profile: AppearanceProfile,
) -> Result<AppearanceResource, String> {
    let writer_experience_id =
        ExperienceId::parse("sos.stock.mobile").map_err(|error| error.to_string())?;
    request(RevisionRequest::SetExperienceAppearance {
        request_id: allocate_request_id(),
        expected_graph_id,
        writer_experience_id,
        expected_generation,
        profile,
    })?
    .appearance
    .ok_or_else(|| "appearance write response omitted its resource".into())
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
    #[cfg(feature = "core-native")]
    let stream = connect_core()?;
    #[cfg(not(feature = "core-native"))]
    let stream = connect_tcp()?;
    request_revision_over_stream(stream, request)
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

fn allocate_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}
