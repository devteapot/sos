use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use async_channel::Sender;
use serde::{Deserialize, Serialize};

const MAX_EVENT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentUpdate {
    Started { prompt: String },
    TextDelta(String),
    ToolStarted(String),
    ToolFinished { name: String, ok: bool },
    Completed,
    Failed(String),
}

#[derive(Serialize)]
struct PromptRequest<'a> {
    action: &'static str,
    prompt: &'a str,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AgentEvent {
    Accepted,
    TextDelta { delta: String },
    ToolStart { name: String },
    ToolEnd { name: String, ok: bool },
    Completed,
    Failed { error: String },
}

pub fn spawn_prompt(socket: PathBuf, prompt: String, updates: Sender<AgentUpdate>) {
    thread::Builder::new()
        .name("sos-agent-client".into())
        .spawn(move || {
            if let Err(error) = run_prompt(&socket, &prompt, &updates) {
                let _ = updates.send_blocking(AgentUpdate::Failed(error));
            }
        })
        .expect("agent client thread must start");
}

fn run_prompt(socket: &Path, prompt: &str, updates: &Sender<AgentUpdate>) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("connect resident agent {}: {error}", socket.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(180)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    serde_json::to_writer(
        &mut stream,
        &PromptRequest {
            action: "prompt",
            prompt,
        },
    )
    .map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read = reader
            .by_ref()
            .take(MAX_EVENT_BYTES + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("resident agent closed the stream before completion".into());
        }
        if bytes.len() as u64 > MAX_EVENT_BYTES {
            return Err("resident agent event exceeded the host limit".into());
        }
        if !bytes.ends_with(b"\n") {
            return Err("resident agent event was not newline terminated".into());
        }
        let event: AgentEvent = serde_json::from_slice(&bytes)
            .map_err(|error| format!("decode agent event: {error}"))?;
        let (update, done) = match event {
            AgentEvent::Accepted => continue,
            AgentEvent::TextDelta { delta } => (AgentUpdate::TextDelta(delta), false),
            AgentEvent::ToolStart { name } => (AgentUpdate::ToolStarted(name), false),
            AgentEvent::ToolEnd { name, ok } => (AgentUpdate::ToolFinished { name, ok }, false),
            AgentEvent::Completed => (AgentUpdate::Completed, true),
            AgentEvent::Failed { error } => (AgentUpdate::Failed(error), true),
        };
        if updates.send_blocking(update).is_err() || done {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn streams_the_bounded_agent_protocol_into_host_updates() {
        let temporary = tempfile::tempdir().unwrap();
        let socket = temporary.path().join("agent.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            assert!(request.contains("make it quiet"));
            stream
                .write_all(
                    b"{\"type\":\"accepted\"}\n{\"type\":\"tool_start\",\"name\":\"get_experience_context\"}\n{\"type\":\"text_delta\",\"delta\":\"Done\"}\n{\"type\":\"completed\"}\n",
                )
                .unwrap();
        });
        let (sender, receiver) = async_channel::unbounded();
        spawn_prompt(socket, "make it quiet".into(), sender);
        assert_eq!(
            receiver.recv_blocking().unwrap(),
            AgentUpdate::ToolStarted("get_experience_context".into())
        );
        assert_eq!(
            receiver.recv_blocking().unwrap(),
            AgentUpdate::TextDelta("Done".into())
        );
        assert_eq!(receiver.recv_blocking().unwrap(), AgentUpdate::Completed);
        server.join().unwrap();
    }
}
