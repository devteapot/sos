use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use experience_ir::{ProviderRequest, ProviderResponse, StateEnvelope};
use providers_fake::state_service::StateService;
use serde_json::json;

const ADDRESS: &str = "127.0.0.1:47777";
const MAX_REQUEST_BYTES: u64 = (experience_ir::MAX_STATE_BYTES + 64 * 1024) as u64;

struct DaemonState {
    states: StateService,
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
            state,
            ..
        } => match shared.lock().expect("state service lock").states.stage(
            expected_revision,
            schema_version,
            state,
        ) {
            Ok(stage_id) => ProviderResponse {
                stage_id: Some(stage_id),
                ..response(request_id, true)
            },
            Err(error) => failure(request_id, &error),
        },
        ProviderRequest::PromoteState { stage_id, .. } => {
            let mut daemon = shared.lock().expect("state service lock");
            let promoted = daemon.states.promote(stage_id);
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
            let removed = shared
                .lock()
                .expect("state service lock")
                .states
                .abort(stage_id);
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
