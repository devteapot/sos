use std::{env, path::PathBuf, time::Duration};

use provider_state_service::ServiceClient;
use serde_json::json;
use service_protocol::{
    FaultPoint, NotesAction, PromotionDraft, ProviderAction, ResourceQuery, ResourceValue,
    ResponsePayload, ServiceRequest, StateResource, TransactionStatus,
};
use sha2::{Digest, Sha256};

fn main() {
    if let Err(error) = run() {
        eprintln!("provider_state_probe_failed error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut socket = None;
    let mut mode = "promote".to_owned();
    let mut fault = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--socket" => socket = arguments.next().map(PathBuf::from),
            "--mode" => mode = arguments.next().ok_or("--mode requires a value")?,
            "--fault" => {
                fault = Some(parse_fault(
                    &arguments.next().ok_or("--fault requires a value")?,
                )?)
            }
            _ => return Err(format!("unexpected argument: {argument}").into()),
        }
    }
    let socket = socket.ok_or("--socket requires a path")?;
    let client = ServiceClient::new(socket, Duration::from_secs(5));
    match mode.as_str() {
        "promote" => promote(&client, fault),
        "status" => status(&client),
        _ => Err(format!("unsupported mode: {mode}").into()),
    }
}

fn promote(
    client: &ServiceClient,
    fault: Option<FaultPoint>,
) -> Result<(), Box<dyn std::error::Error>> {
    let current = load_state(client, 1)?;
    let target_revision = current.revision + 1;
    let transaction_id = format!("device-probe-{target_revision}");
    let revision_id = format!("{:x}", Sha256::digest(transaction_id.as_bytes()));
    let staged = client.call(&ServiceRequest::StagePromotion {
        request_id: 2,
        draft: PromotionDraft {
            transaction_id: transaction_id.clone(),
            expected_revision: current.revision,
            revision_id,
            schema_version: current.schema_version,
            source_sha256: format!(
                "{:x}",
                Sha256::digest(format!("source-{target_revision}").as_bytes())
            ),
            state: json!({
                "device_probe": true,
                "target_revision": target_revision,
            }),
            migration: None,
            actions: vec![ProviderAction::Notes(NotesAction::AttachToEvent {
                note_id: format!("device-note-{target_revision}"),
                event_title: "Android service probe".into(),
            })],
        },
    })?;
    ensure_ok("stage", &staged)?;
    if let Some(point) = fault {
        ensure_ok(
            "configure fault",
            &client.call(&ServiceRequest::ConfigureFault {
                request_id: 3,
                point: Some(point),
            })?,
        )?;
    }
    let first = client.call(&ServiceRequest::Promote {
        request_id: 4,
        transaction_id: transaction_id.clone(),
    })?;
    let ambiguous = !first.ok;
    let completed = if ambiguous {
        client.call(&ServiceRequest::Promote {
            request_id: 5,
            transaction_id: transaction_id.clone(),
        })?
    } else {
        first
    };
    ensure_ok("promote/reconcile", &completed)?;
    let record = match completed.payload {
        Some(ResponsePayload::Transaction { record }) => record,
        _ => return Err("promotion did not return a transaction".into()),
    };
    if record.status != TransactionStatus::Committed || record.effects.len() != 1 {
        return Err("transaction did not commit exactly one effect".into());
    }
    let current = load_state(client, 6)?;
    let notes = load_notes(client, 7)?;
    println!(
        "provider_state_probe_promoted transaction_id={} revision={} schema_version={} effects={} attachments={} ambiguous={ambiguous}",
        transaction_id,
        current.revision,
        current.schema_version,
        record.effects.len(),
        notes.attachments.len()
    );
    Ok(())
}

fn status(client: &ServiceClient) -> Result<(), Box<dyn std::error::Error>> {
    let state = load_state(client, 1)?;
    let notes = load_notes(client, 2)?;
    let events = client.call(&ServiceRequest::ListEvents {
        request_id: 3,
        after_sequence: 0,
        limit: 1_000,
    })?;
    ensure_ok("list events", &events)?;
    let event_count = match events.payload {
        Some(ResponsePayload::Events { events }) => events.len(),
        _ => return Err("event query returned the wrong payload".into()),
    };
    println!(
        "provider_state_probe_status revision={} revision_id={} schema_version={} attachments={} events={event_count}",
        state.revision,
        state.revision_id,
        state.schema_version,
        notes.attachments.len()
    );
    Ok(())
}

fn load_state(
    client: &ServiceClient,
    request_id: u64,
) -> Result<StateResource, Box<dyn std::error::Error>> {
    let response = client.call(&ServiceRequest::GetResource {
        request_id,
        query: ResourceQuery::ExperienceState,
    })?;
    ensure_ok("load state", &response)?;
    match response.payload {
        Some(ResponsePayload::Resource {
            value: ResourceValue::ExperienceState(state),
        }) => Ok(state),
        _ => Err("state query returned the wrong payload".into()),
    }
}

fn load_notes(
    client: &ServiceClient,
    request_id: u64,
) -> Result<service_protocol::NotesResource, Box<dyn std::error::Error>> {
    let response = client.call(&ServiceRequest::GetResource {
        request_id,
        query: ResourceQuery::Notes,
    })?;
    ensure_ok("load notes", &response)?;
    match response.payload {
        Some(ResponsePayload::Resource {
            value: ResourceValue::Notes(notes),
        }) => Ok(notes),
        _ => Err("notes query returned the wrong payload".into()),
    }
}

fn ensure_ok(
    operation: &str,
    response: &service_protocol::ServiceResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    if response.ok {
        Ok(())
    } else {
        Err(format!("{operation} failed: {:?}", response.error).into())
    }
}

fn parse_fault(value: &str) -> Result<FaultPoint, String> {
    match value {
        "before_stage" => Ok(FaultPoint::BeforeStage),
        "after_stage" => Ok(FaultPoint::AfterStage),
        "before_promotion" => Ok(FaultPoint::BeforePromotion),
        "during_promotion" => Ok(FaultPoint::DuringPromotion),
        "after_promotion" => Ok(FaultPoint::AfterPromotion),
        _ => Err(format!("unknown fault point: {value}")),
    }
}
