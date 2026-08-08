use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use experience_ir::{ProviderEffect, ProviderRequest, ProviderResponse, StateEnvelope};
use providers_fake::state_service::StateService;
use serde_json::json;

const ADDRESS: &str = "127.0.0.1:47777";
const MAX_REQUEST_BYTES: u64 = (experience_ir::MAX_STATE_BYTES + 64 * 1024) as u64;

struct DaemonState {
    states: StateService,
    staged_effects: HashMap<u64, Vec<ProviderEffect>>,
    executed_effects: Vec<String>,
    state_file: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state_file = parse_state_file()?;
    let initial = state_file
        .as_ref()
        .and_then(|path| fs::read(path).ok())
        .map(|bytes| serde_json::from_slice::<StateEnvelope>(&bytes))
        .transpose()?
        .unwrap_or_else(|| StateService::default().load());
    let shared = Arc::new(Mutex::new(DaemonState {
        states: StateService::new(initial),
        staged_effects: HashMap::new(),
        executed_effects: Vec::new(),
        state_file,
    }));
    let listener = TcpListener::bind(ADDRESS)?;
    println!("providerd_listening address={ADDRESS}");
    for stream in listener.incoming() {
        match stream.and_then(|stream| handle(stream, &shared)) {
            Ok(()) => {}
            Err(error) => eprintln!("providerd_request_failed error={error}"),
        }
    }
    Ok(())
}

fn parse_state_file() -> Result<Option<PathBuf>, String> {
    let mut args = env::args().skip(1);
    match args.next() {
        None => Ok(None),
        Some(flag) if flag == "--state-file" => args
            .next()
            .map(PathBuf::from)
            .map(Some)
            .ok_or_else(|| "--state-file requires a path".into()),
        Some(argument) => Err(format!("unexpected argument: {argument}")),
    }
}

fn handle(mut stream: TcpStream, shared: &Arc<Mutex<DaemonState>>) -> std::io::Result<()> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?)
        .take(MAX_REQUEST_BYTES + 1)
        .read_line(&mut line)?;
    if line.len() as u64 > MAX_REQUEST_BYTES {
        return Err(std::io::Error::other("provider request is too large"));
    }
    let request = serde_json::from_str::<ProviderRequest>(&line).map_err(std::io::Error::other)?;
    let request_id = request.request_id();
    let response = dispatch(request, shared);
    serde_json::to_writer(&mut stream, &response).map_err(std::io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    println!(
        "provider_request_completed request_id={request_id} ok={}",
        response.ok
    );
    Ok(())
}

fn dispatch(request: ProviderRequest, shared: &Arc<Mutex<DaemonState>>) -> ProviderResponse {
    let request_id = request.request_id();
    match request {
        ProviderRequest::Snapshot { .. } => ProviderResponse {
            model: Some(providers_fake::snapshot()),
            ..response(request_id, true)
        },
        ProviderRequest::Action {
            provider,
            action,
            payload,
            ..
        } if provider == "notes" && action == "attach_to_event" => {
            let note_id = payload.get("note_id").and_then(|value| value.as_str());
            let event_title = payload.get("event_title").and_then(|value| value.as_str());
            match (note_id, event_title) {
                (Some(note_id), Some(event_title)) => {
                    println!(
                        "provider_action request_id={request_id} provider=notes action=attach_to_event note_id={note_id} event_title={event_title}"
                    );
                    ProviderResponse {
                        result: Some(json!({
                            "receipt": format!("notes:{note_id}->{event_title}"),
                        })),
                        ..response(request_id, true)
                    }
                }
                _ => failure(request_id, "note_id and event_title are required"),
            }
        }
        ProviderRequest::Action {
            provider, action, ..
        } => failure(
            request_id,
            &format!("unsupported provider action: {provider}.{action}"),
        ),
        ProviderRequest::LoadState { .. } => {
            let state = shared.lock().expect("state service lock").states.load();
            ProviderResponse {
                state: Some(state),
                ..response(request_id, true)
            }
        }
        ProviderRequest::StageState {
            expected_revision,
            schema_version,
            mut state,
            source_sha256,
            effects,
            ..
        } => {
            if let Err(error) = validate_effects(&effects, &mut state) {
                return failure(request_id, &error);
            }
            let mut daemon = shared.lock().expect("state service lock");
            match daemon
                .states
                .stage(expected_revision, schema_version, state, source_sha256)
            {
                Ok(stage_id) => {
                    daemon.staged_effects.insert(stage_id, effects);
                    ProviderResponse {
                        stage_id: Some(stage_id),
                        ..response(request_id, true)
                    }
                }
                Err(error) => failure(request_id, &error),
            }
        }
        ProviderRequest::PromoteState { stage_id, .. } => {
            let mut daemon = shared.lock().expect("state service lock");
            let before_revision = daemon.states.load().revision;
            let promoted = daemon.states.promote(stage_id);
            let current = daemon.states.load();
            if current.revision > before_revision {
                if let Some(effects) = daemon.staged_effects.remove(&stage_id) {
                    let receipts = execute_effects(current.revision, &effects);
                    daemon.executed_effects.extend(receipts);
                }
            }
            let persist_result = persist(&daemon);
            match (promoted, persist_result) {
                (Ok(state), Ok(())) => ProviderResponse {
                    state: Some(state),
                    ..response(request_id, true)
                },
                (Err(error), _) | (_, Err(error)) => failure(request_id, &error),
            }
        }
        ProviderRequest::AbortState { stage_id, .. } => {
            let mut daemon = shared.lock().expect("state service lock");
            let removed = daemon.states.abort(stage_id);
            daemon.staged_effects.remove(&stage_id);
            ProviderResponse {
                result: Some(json!({ "removed": removed })),
                ..response(request_id, true)
            }
        }
        ProviderRequest::ConfigureStateFault { point, .. } => {
            shared
                .lock()
                .expect("state service lock")
                .states
                .configure_fault(point);
            response(request_id, true)
        }
    }
}

fn validate_effects(
    effects: &[ProviderEffect],
    state: &mut serde_json::Value,
) -> Result<(), String> {
    if effects.len() > experience_ir::MAX_EFFECTS {
        return Err("too many staged provider effects".into());
    }
    for effect in effects {
        if (effect.provider.as_str(), effect.action.as_str()) != ("notes", "attach_to_event") {
            return Err(format!(
                "unsupported staged provider action: {}.{}",
                effect.provider, effect.action
            ));
        }
        let note_id = effect
            .payload
            .get("note_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "note_id is required".to_owned())?;
        let event_title = effect
            .payload
            .get("event_title")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "event_title is required".to_owned())?;
        if !state.is_object() {
            *state = json!({});
        }
        state.as_object_mut().expect("state object").insert(
            "provider_receipt".into(),
            json!({ "receipt": format!("notes:{note_id}->{event_title}") }),
        );
    }
    Ok(())
}

fn execute_effects(revision: u64, effects: &[ProviderEffect]) -> Vec<String> {
    let mut receipts = Vec::with_capacity(effects.len());
    for effect in effects {
        let note_id = effect.payload["note_id"].as_str().unwrap_or("missing");
        let event_title = effect.payload["event_title"].as_str().unwrap_or("missing");
        println!(
            "provider_effect_promoted revision={revision} provider={} action={} note_id={note_id} event_title={event_title}",
            effect.provider, effect.action
        );
        receipts.push(format!(
            "{}:{}:{}:{}",
            revision, effect.provider, effect.action, note_id
        ));
    }
    receipts
}

fn persist(daemon: &DaemonState) -> Result<(), String> {
    let Some(path) = &daemon.state_file else {
        return Ok(());
    };
    let bytes =
        serde_json::to_vec_pretty(&daemon.states.load()).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn response(request_id: u64, ok: bool) -> ProviderResponse {
    ProviderResponse {
        request_id,
        ok,
        model: None,
        result: None,
        state: None,
        stage_id: None,
        error: None,
    }
}

fn failure(request_id: u64, error: &str) -> ProviderResponse {
    ProviderResponse {
        error: Some(error.into()),
        ..response(request_id, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use experience_ir::StateFaultPoint;

    fn shared() -> Arc<Mutex<DaemonState>> {
        Arc::new(Mutex::new(DaemonState {
            states: StateService::default(),
            staged_effects: HashMap::new(),
            executed_effects: Vec::new(),
            state_file: None,
        }))
    }

    fn effect() -> ProviderEffect {
        ProviderEffect {
            provider: "notes".into(),
            action: "attach_to_event".into(),
            payload: json!({"note_id":"note-1", "event_title":"Design review"}),
        }
    }

    #[test]
    fn provider_effect_is_inert_until_state_promotion() {
        let shared = shared();
        let staged = dispatch(
            ProviderRequest::StageState {
                request_id: 1,
                expected_revision: 0,
                schema_version: 1,
                state: json!({}),
                source_sha256: "a".repeat(64),
                effects: vec![effect()],
            },
            &shared,
        );
        assert!(staged.ok);
        assert!(shared.lock().unwrap().executed_effects.is_empty());
        let promoted = dispatch(
            ProviderRequest::PromoteState {
                request_id: 2,
                stage_id: staged.stage_id.unwrap(),
            },
            &shared,
        );
        assert!(promoted.ok);
        let daemon = shared.lock().unwrap();
        assert_eq!(daemon.states.load().revision, 1);
        assert_eq!(daemon.executed_effects.len(), 1);
    }

    #[test]
    fn ambiguous_post_promotion_fault_executes_effect_exactly_once() {
        let shared = shared();
        let staged = dispatch(
            ProviderRequest::StageState {
                request_id: 1,
                expected_revision: 0,
                schema_version: 1,
                state: json!({}),
                source_sha256: "b".repeat(64),
                effects: vec![effect()],
            },
            &shared,
        );
        shared
            .lock()
            .unwrap()
            .states
            .configure_fault(Some(StateFaultPoint::AfterPromote));
        let first = dispatch(
            ProviderRequest::PromoteState {
                request_id: 2,
                stage_id: staged.stage_id.unwrap(),
            },
            &shared,
        );
        assert!(!first.ok);
        let second = dispatch(
            ProviderRequest::PromoteState {
                request_id: 3,
                stage_id: staged.stage_id.unwrap(),
            },
            &shared,
        );
        assert!(!second.ok);
        let daemon = shared.lock().unwrap();
        assert_eq!(daemon.states.load().revision, 1);
        assert_eq!(daemon.executed_effects.len(), 1);
    }
}
