use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use experience_ir::{
    ExperienceModel, ProviderEffect, ProviderRequest, ProviderResponse, StateEnvelope,
    StateFaultPoint,
};
use serde_json::Value as JsonValue;

const ADDRESS: &str = "127.0.0.1:47777";
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn snapshot() -> Result<ExperienceModel, String> {
    let response = request(ProviderRequest::Snapshot {
        request_id: allocate_request_id(),
    })?;
    response
        .model
        .ok_or_else(|| "provider snapshot response omitted the model".into())
}

pub(super) fn execute(effect: &ProviderEffect) -> Result<JsonValue, String> {
    let response = request(ProviderRequest::Action {
        request_id: allocate_request_id(),
        provider: effect.provider.clone(),
        action: effect.action.clone(),
        payload: effect.payload.clone(),
    })?;
    Ok(response.result.unwrap_or(JsonValue::Null))
}

pub(super) fn load_state() -> Result<StateEnvelope, String> {
    request(ProviderRequest::LoadState {
        request_id: allocate_request_id(),
    })?
    .state
    .ok_or_else(|| "state response omitted its envelope".into())
}

pub(super) fn commit_state(
    expected_revision: u64,
    schema_version: u64,
    state: &JsonValue,
) -> Result<StateEnvelope, String> {
    let stage = request(ProviderRequest::StageState {
        request_id: allocate_request_id(),
        expected_revision,
        schema_version,
        state: state.clone(),
    })?;
    let stage_id = stage
        .stage_id
        .ok_or_else(|| "state stage response omitted its id".to_owned())?;
    match request(ProviderRequest::PromoteState {
        request_id: allocate_request_id(),
        stage_id,
    }) {
        Ok(response) => response
            .state
            .ok_or_else(|| "state promotion response omitted its envelope".into()),
        Err(error) => {
            let current = load_state()?;
            if current.revision == expected_revision.saturating_add(1)
                && current.schema_version == schema_version
                && current.state == *state
            {
                Ok(current)
            } else {
                Err(error)
            }
        }
    }
}

#[allow(dead_code)]
pub(super) fn configure_state_fault(point: Option<StateFaultPoint>) -> Result<(), String> {
    request(ProviderRequest::ConfigureStateFault {
        request_id: allocate_request_id(),
        point,
    })?;
    Ok(())
}

fn request(request: ProviderRequest) -> Result<ProviderResponse, String> {
    let expected_id = request.request_id();
    let address = ADDRESS
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| "provider address did not resolve".to_owned())?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| error.to_string())?;
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
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "provider rejected request".into()));
    }
    Ok(response)
}

fn allocate_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}
