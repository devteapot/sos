use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::PermissionsExt as _,
    os::unix::net::{SocketAddr, UnixListener, UnixStream},
    path::Path,
};

#[cfg(target_os = "android")]
use std::os::android::net::SocketAddrExt;
#[cfg(target_os = "linux")]
use std::os::linux::net::SocketAddrExt;

use service_protocol::{
    ResourceQuery, ResourceValue, ResponsePayload, ServiceError, ServiceRequest,
    ServiceRequestEnvelope, ServiceResponse, LEGACY_PROTOCOL_VERSION, MAX_STATE_BYTES,
    PROTOCOL_VERSION,
};

use crate::{Authority, AuthorityError};

const MAX_REQUEST_BYTES: u64 = (MAX_STATE_BYTES + 512 * 1024) as u64;

pub fn serve(
    socket: &Path,
    state_file: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    serve_with_writers(socket, state_file, None, None)
}

pub fn serve_with_appearance_writer(
    socket: &Path,
    state_file: &Path,
    appearance_writer: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    serve_with_writers(socket, state_file, appearance_writer, None)
}

pub fn serve_with_writers(
    socket: &Path,
    state_file: &Path,
    appearance_writer: Option<&str>,
    grant_writer: Option<&str>,
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
    if !abstract_socket {
        fs::set_permissions(socket, fs::Permissions::from_mode(0o660))?;
    }
    let mut authority = Authority::open(state_file)?;
    if let Some(capability) = appearance_writer {
        authority.configure_appearance_writer(capability)?;
    }
    if let Some(capability) = grant_writer {
        authority.configure_grant_writer(capability)?;
    }
    let result = (|| {
        for stream in listener.incoming() {
            let mut stream = stream?;
            match handle(&mut stream, &mut authority) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(error) => {
                    // A requester may be cancelled or crash after sending its
                    // bounded request. That connection must not take down the
                    // durable provider authority or unrelated subscribers.
                    eprintln!("provider_state_client_failed error={error}");
                }
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
            Ok(envelope)
                if matches!(
                    envelope.protocol_version,
                    LEGACY_PROTOCOL_VERSION | PROTOCOL_VERSION
                ) =>
            {
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
    let written = (|| {
        serde_json::to_writer(&mut *stream, &response).map_err(std::io::Error::other)?;
        stream.write_all(b"\n")?;
        stream.flush()
    })();
    if let Err(error) = written {
        if shutdown {
            // Shutdown authority is carried by the authenticated request, not
            // by the requester's continued presence for the acknowledgement.
            return Ok(true);
        }
        return Err(error);
    }
    Ok(shutdown)
}

pub fn dispatch(request: ServiceRequest, authority: &mut Authority) -> ServiceResponse {
    let request_id = request.request_id();
    if let Err(error) = authority.reconcile() {
        return ServiceResponse::failure(request_id, map_error(error));
    }
    let result = match request {
        ServiceRequest::GetResource { query, .. } => match query {
            ResourceQuery::GrantDecisionFor { experience_id } => authority
                .grant_decision_for(experience_id.as_str())
                .map(|value| ResponsePayload::Resource {
                    value: ResourceValue::GrantDecision(value),
                })
                .ok_or_else(|| {
                    AuthorityError::Service(ServiceError::NotFound {
                        message: format!("no grant decision for experience: {experience_id}"),
                    })
                }),
            query => Ok(ResponsePayload::Resource {
                value: match query {
                    ResourceQuery::ExperienceState => {
                        ResourceValue::ExperienceState(authority.current())
                    }
                    ResourceQuery::ExperienceStateFor { experience_id } => {
                        ResourceValue::ExperienceStateFor(
                            service_protocol::ExperienceStateResource {
                                resource: authority.current_for(experience_id.as_str()),
                                experience_id,
                            },
                        )
                    }
                    ResourceQuery::ExperienceStateAt {
                        experience_id,
                        revision_id,
                    } => ResourceValue::ExperienceStateAt(
                        service_protocol::ExperienceStateResource {
                            resource: authority.current_at(experience_id.as_str(), &revision_id),
                            experience_id,
                        },
                    ),
                    ResourceQuery::Appearance => ResourceValue::Appearance(authority.appearance()),
                    ResourceQuery::Notes => ResourceValue::Notes(authority.notes()),
                    ResourceQuery::GrantDecisionFor { .. } => unreachable!(),
                },
            }),
        },
        ServiceRequest::StagePromotion { draft, .. } => authority
            .stage(draft)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::StageExperiencePromotion { draft, .. } => authority
            .stage_experience(draft)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::StageGraphPromotion { draft, .. } => authority
            .stage_graph(draft)
            .map(|record| ResponsePayload::GraphTransaction { record }),
        ServiceRequest::UpdateAppearance {
            expected_generation,
            capability,
            profile,
            ..
        } => authority
            .update_appearance(expected_generation, &capability, profile)
            .map(|value| ResponsePayload::AppearanceUpdated { value }),
        ServiceRequest::UpdateGrantDecision {
            expected_generation,
            capability,
            decision,
            ..
        } => authority
            .update_grant_decision(expected_generation, &capability, decision)
            .map(|value| ResponsePayload::GrantDecisionUpdated { value }),
        ServiceRequest::Promote { transaction_id, .. } => authority
            .promote(&transaction_id)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::Abort { transaction_id, .. } => authority
            .abort(&transaction_id)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::GetTransaction { transaction_id, .. } => authority
            .transaction(&transaction_id)
            .map(|record| ResponsePayload::Transaction { record }),
        ServiceRequest::PromoteGraph { transaction_id, .. } => authority
            .promote_graph(&transaction_id)
            .map(|record| ResponsePayload::GraphTransaction { record }),
        ServiceRequest::AbortGraph { transaction_id, .. } => authority
            .abort_graph(&transaction_id)
            .map(|record| ResponsePayload::GraphTransaction { record }),
        ServiceRequest::GetGraphTransaction { transaction_id, .. } => authority
            .graph_transaction(&transaction_id)
            .map(|record| ResponsePayload::GraphTransaction { record }),
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
