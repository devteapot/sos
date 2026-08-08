use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
};

use experience_ir::{ProviderRequest, ProviderResponse};
use serde_json::json;

const ADDRESS: &str = "127.0.0.1:47777";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(ADDRESS)?;
    println!("providerd_listening address={ADDRESS}");
    for stream in listener.incoming() {
        match stream.and_then(handle) {
            Ok(()) => {}
            Err(error) => eprintln!("providerd_request_failed error={error}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    let request = serde_json::from_str::<ProviderRequest>(&line).map_err(std::io::Error::other)?;
    let request_id = request.request_id();
    let response = match request {
        ProviderRequest::Snapshot { .. } => ProviderResponse {
            request_id,
            ok: true,
            model: Some(providers_fake::snapshot()),
            result: None,
            error: None,
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
                        request_id,
                        ok: true,
                        model: None,
                        result: Some(json!({
                            "receipt": format!("notes:{note_id}->{event_title}"),
                        })),
                        error: None,
                    }
                }
                _ => ProviderResponse {
                    request_id,
                    ok: false,
                    model: None,
                    result: None,
                    error: Some("note_id and event_title are required".into()),
                },
            }
        }
        ProviderRequest::Action {
            provider, action, ..
        } => ProviderResponse {
            request_id,
            ok: false,
            model: None,
            result: None,
            error: Some(format!("unsupported provider action: {provider}.{action}")),
        },
    };
    serde_json::to_writer(&mut stream, &response).map_err(std::io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()
}
