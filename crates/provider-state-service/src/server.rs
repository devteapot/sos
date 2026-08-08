use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{SocketAddr, UnixListener, UnixStream},
    path::Path,
};

#[cfg(target_os = "android")]
use std::os::android::net::SocketAddrExt;
#[cfg(target_os = "linux")]
use std::os::linux::net::SocketAddrExt;

use service_protocol::{
    ResourceQuery, ResourceValue, ResponsePayload, ServiceError, ServiceRequest,
    ServiceRequestEnvelope, ServiceResponse, MAX_STATE_BYTES, PROTOCOL_VERSION,
};

use crate::{Authority, AuthorityError};

const MAX_REQUEST_BYTES: u64 = (MAX_STATE_BYTES + 512 * 1024) as u64;

pub fn serve(
    socket: &Path,
    state_file: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let abstract_socket = socket.as_os_str().as_encoded_bytes().starts_with(b"@");
    if !abstract_socket && socket.exists() {
        return Err(format!("service socket already exists: {}", socket.display()).into());
    }
    if !abstract_socket {
        if let Some(parent) = socket.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|error| {
                    std::io::Error::new(
                        error.kind(),
                        format!("create service socket directory: {error}"),
                    )
                })?;
            }
        }
    }
    let listener = bind(socket).map_err(|error| {
        std::io::Error::new(error.kind(), format!("bind service socket: {error}"))
    })?;
    let mut authority = Authority::open(state_file)?;
    let result = (|| {
        for stream in listener.incoming() {
            let mut stream = stream?;
            if handle(&mut stream, &mut authority)? {
                return Ok(());
            }
        }
        Ok(())
    })();
    if !abstract_socket {
        fs::remove_file(socket).ok();
    }
    result
}

fn bind(socket: &Path) -> std::io::Result<UnixListener> {
    let value = socket.as_os_str().as_encoded_bytes();
    if let Some(name) = value.strip_prefix(b"@") {
        UnixListener::bind_addr(&SocketAddr::from_abstract_name(name)?)
    } else {
        UnixListener::bind(socket)
    }
}

fn handle(stream: &mut UnixStream, authority: &mut Authority) -> std::io::Result<bool> {
    let mut request_bytes = Vec::new();
    BufReader::new(stream.try_clone()?)
        .take(MAX_REQUEST_BYTES + 1)
        .read_until(b'\n', &mut request_bytes)?;
    let (response, shutdown) = if request_bytes.len() as u64 > MAX_REQUEST_BYTES {
        (
            ServiceResponse::failure(
                0,
                ServiceError::InvalidRequest {
                    message: "request exceeds the service limit".into(),
                },
            ),
            false,
        )
    } else {
        match serde_json::from_slice::<ServiceRequestEnvelope>(&request_bytes) {
            Ok(envelope) if envelope.protocol_version == PROTOCOL_VERSION => {
                let shutdown = matches!(envelope.request, ServiceRequest::Shutdown { .. });
                (dispatch(envelope.request, authority), shutdown)
            }
            Ok(envelope) => {
                let request_id = envelope.request.request_id();
                (
                    ServiceResponse::failure(
                        request_id,
                        ServiceError::InvalidRequest {
                            message: format!(
                                "unsupported protocol version: {}",
                                envelope.protocol_version
                            ),
                        },
                    ),
                    false,
                )
            }
            Err(error) => (
                ServiceResponse::failure(
                    0,
                    ServiceError::InvalidRequest {
                        message: error.to_string(),
                    },
                ),
                false,
            ),
        }
    };
    serde_json::to_writer(&mut *stream, &response).map_err(std::io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(shutdown)
}

pub fn dispatch(request: ServiceRequest, authority: &mut Authority) -> ServiceResponse {
    let request_id = request.request_id();
    if let Err(error) = authority.reconcile() {
        return ServiceResponse::failure(request_id, map_error(error));
    }
    let result = match request {
        ServiceRequest::GetResource { query, .. } => Ok(ResponsePayload::Resource {
            value: match query {
                ResourceQuery::ExperienceState => {
                    ResourceValue::ExperienceState(authority.current())
                }
                ResourceQuery::Notes => ResourceValue::Notes(authority.notes()),
            },
        }),
        ServiceRequest::StagePromotion { draft, .. } => authority
            .stage(draft)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::Promote { transaction_id, .. } => authority
            .promote(&transaction_id)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::Abort { transaction_id, .. } => authority
            .abort(&transaction_id)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::GetTransaction { transaction_id, .. } => authority
            .transaction(&transaction_id)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::ListEvents {
            after_sequence,
            limit,
            ..
        } => Ok(ResponsePayload::Events {
            events: authority.events(after_sequence, limit),
        }),
        ServiceRequest::ConfigureFault { point, .. } => {
            authority.configure_fault(point);
            Ok(ResponsePayload::FaultConfigured)
        }
        ServiceRequest::Shutdown { .. } => Ok(ResponsePayload::Shutdown),
    };
    match result {
        Ok(payload) => ServiceResponse::success(request_id, payload),
        Err(error) => ServiceResponse::failure(request_id, map_error(error)),
    }
}

fn map_error(error: AuthorityError) -> ServiceError {
    match error {
        AuthorityError::Service(error) => error,
        AuthorityError::Io(error) => ServiceError::Internal {
            message: error.to_string(),
        },
        AuthorityError::Json(error) => ServiceError::Internal {
            message: error.to_string(),
        },
    }
}
