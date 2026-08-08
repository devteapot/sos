use std::{
    env,
    io::{self, BufRead, BufReader, Read as _, Write},
    os::unix::net::UnixStream,
    path::Path,
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::{
    CompositorEvent, CompositorRequest, PresentationEvidence, MAX_CONTROL_LINE_BYTES,
    MAX_SHELL_TOKEN_BYTES,
};

const CONTROL_SOCKET_ENV: &str = "SOS_COMPOSITOR_CONTROL";
const SHELL_TOKEN_ENV: &str = "SOS_COMPOSITOR_TOKEN";
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Debug)]
pub struct Presented {
    pub request_id: u64,
    pub revision_id: String,
    pub commit_sequence: u64,
    pub submit_sequence: u64,
    pub evidence: PresentationEvidence,
}

#[derive(Clone, Debug)]
pub enum FenceEvent {
    Presented(Presented),
    Failed(String),
}

enum FenceCommand {
    Arm {
        request_id: u64,
        revision_id: String,
        reply: mpsc::Sender<std::result::Result<u64, String>>,
    },
}

pub struct CompositorFence {
    commands: mpsc::Sender<FenceCommand>,
    events: async_channel::Receiver<FenceEvent>,
}

impl CompositorFence {
    pub fn from_environment() -> Result<Option<Self>> {
        match (
            env::var_os(CONTROL_SOCKET_ENV),
            env::var_os(SHELL_TOKEN_ENV),
        ) {
            (None, None) => Ok(None),
            (Some(_), None) => {
                bail!("{SHELL_TOKEN_ENV} is required when {CONTROL_SOCKET_ENV} is set")
            }
            (None, Some(_)) => {
                bail!("{CONTROL_SOCKET_ENV} is required when {SHELL_TOKEN_ENV} is set")
            }
            (Some(socket), Some(token)) => {
                let token = token
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("{SHELL_TOKEN_ENV} is not valid UTF-8"))?;
                Self::connect(Path::new(&socket), token).map(Some)
            }
        }
    }

    fn connect(socket_path: &Path, token: String) -> Result<Self> {
        if token.is_empty()
            || token.len() > MAX_SHELL_TOKEN_BYTES
            || token.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
        {
            bail!("invalid compositor shell token");
        }
        let mut stream = UnixStream::connect(socket_path).with_context(|| {
            format!(
                "connect to SOS compositor control socket {}",
                socket_path.display()
            )
        })?;
        stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let pid = std::process::id();
        write_request(
            &mut stream,
            &CompositorRequest::RegisterShell {
                request_id: 0,
                token,
                pid,
            },
        )?;
        match read_event(&mut reader)?.context("compositor closed during shell registration")? {
            CompositorEvent::Registered {
                request_id: 0,
                pid: registered_pid,
            } if registered_pid == pid => {}
            CompositorEvent::Rejected { error, .. } => {
                bail!("compositor rejected shell registration: {error}")
            }
            event => bail!("unexpected compositor registration event: {event:?}"),
        }
        stream.set_read_timeout(Some(EVENT_POLL_INTERVAL))?;

        let (commands_tx, commands_rx) = mpsc::channel();
        let (events_tx, events_rx) = async_channel::unbounded();
        thread::Builder::new()
            .name("sos-compositor-fence".into())
            .spawn(move || {
                if let Err(error) = run_io(stream, reader, commands_rx, &events_tx) {
                    let _ = events_tx.send_blocking(FenceEvent::Failed(error.to_string()));
                }
            })?;

        eprintln!(
            "sos_compositor_registered pid={pid} control_socket={}",
            socket_path.display()
        );
        Ok(Self {
            commands: commands_tx,
            events: events_rx,
        })
    }

    pub fn arm(&self, request_id: u64, revision_id: &str) -> Result<u64> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.commands
            .send(FenceCommand::Arm {
                request_id,
                revision_id: revision_id.into(),
                reply: reply_tx,
            })
            .context("compositor fence thread is unavailable")?;
        reply_rx
            .recv_timeout(CONTROL_TIMEOUT)
            .context("timed out arming compositor presentation fence")?
            .map_err(anyhow::Error::msg)
    }

    pub fn events(&self) -> async_channel::Receiver<FenceEvent> {
        self.events.clone()
    }
}

fn run_io(
    mut stream: UnixStream,
    mut reader: BufReader<UnixStream>,
    commands: mpsc::Receiver<FenceCommand>,
    events: &async_channel::Sender<FenceEvent>,
) -> Result<()> {
    loop {
        if let Ok(FenceCommand::Arm {
            request_id,
            revision_id,
            reply,
        }) = commands.try_recv()
        {
            stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
            write_request(
                &mut stream,
                &CompositorRequest::ArmPresentation {
                    request_id,
                    revision_id: revision_id.clone(),
                },
            )?;
            let armed = loop {
                let event = read_event(&mut reader)?
                    .context("compositor closed while arming presentation fence")?;
                match event {
                    CompositorEvent::Armed {
                        request_id: received,
                        revision_id: received_revision,
                        after_commit_sequence,
                    } if received == request_id && received_revision == revision_id => {
                        break Ok(after_commit_sequence);
                    }
                    CompositorEvent::Rejected {
                        request_id: received,
                        error,
                    } if received == request_id => break Err(error),
                    CompositorEvent::Presented {
                        request_id,
                        revision_id,
                        commit_sequence,
                        submit_sequence,
                        evidence,
                    } => {
                        events.send_blocking(FenceEvent::Presented(Presented {
                            request_id,
                            revision_id,
                            commit_sequence,
                            submit_sequence,
                            evidence,
                        }))?;
                    }
                    event => bail!("unexpected compositor arm event: {event:?}"),
                }
            };
            let _ = reply.send(armed);
            stream.set_read_timeout(Some(EVENT_POLL_INTERVAL))?;
        }

        match read_event(&mut reader) {
            Ok(Some(CompositorEvent::Presented {
                request_id,
                revision_id,
                commit_sequence,
                submit_sequence,
                evidence,
            })) => events.send_blocking(FenceEvent::Presented(Presented {
                request_id,
                revision_id,
                commit_sequence,
                submit_sequence,
                evidence,
            }))?,
            Ok(Some(event)) => bail!("unexpected asynchronous compositor event: {event:?}"),
            Ok(None) => bail!("compositor control connection closed"),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn read_event(reader: &mut BufReader<UnixStream>) -> io::Result<Option<CompositorEvent>> {
    let mut line = String::new();
    let count = (&mut *reader)
        .take((MAX_CONTROL_LINE_BYTES + 1) as u64)
        .read_line(&mut line)?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_CONTROL_LINE_BYTES || !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compositor event exceeded its bound or omitted newline",
        ));
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(io::Error::other)
}

fn write_request(stream: &mut UnixStream, request: &CompositorRequest) -> io::Result<()> {
    serde_json::to_writer(&mut *stream, request).map_err(io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn registers_arms_and_forwards_presented_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("control.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let registration = read_request_for_test(&mut reader);
            let CompositorRequest::RegisterShell {
                request_id,
                pid,
                token,
            } = registration
            else {
                panic!("expected registration")
            };
            assert_eq!(token, "secret");
            write_event_for_test(
                &mut stream,
                &CompositorEvent::Registered { request_id, pid },
            );
            let arm = read_request_for_test(&mut reader);
            let CompositorRequest::ArmPresentation {
                request_id,
                revision_id,
            } = arm
            else {
                panic!("expected arm")
            };
            write_event_for_test(
                &mut stream,
                &CompositorEvent::Armed {
                    request_id,
                    revision_id: revision_id.clone(),
                    after_commit_sequence: 5,
                },
            );
            write_event_for_test(
                &mut stream,
                &CompositorEvent::Presented {
                    request_id,
                    revision_id,
                    commit_sequence: 6,
                    submit_sequence: 9,
                    evidence: PresentationEvidence::NestedBackendSubmit,
                },
            );
        });

        let fence = CompositorFence::connect(&socket, "secret".into()).unwrap();
        let revision = "a".repeat(64);
        assert_eq!(fence.arm(7, &revision).unwrap(), 5);
        let FenceEvent::Presented(presented) =
            fence.events().recv_blocking().expect("presentation event")
        else {
            panic!("expected presentation evidence")
        };
        assert_eq!(presented.request_id, 7);
        assert_eq!(presented.revision_id, revision);
        assert_eq!(presented.commit_sequence, 6);
        assert_eq!(presented.submit_sequence, 9);
        assert_eq!(presented.evidence.name(), "nested_backend_submit");
        server.join().unwrap();
    }

    fn read_request_for_test(reader: &mut BufReader<UnixStream>) -> CompositorRequest {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn write_event_for_test(stream: &mut UnixStream, event: &CompositorEvent) {
        serde_json::to_writer(&mut *stream, event).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
    }
}
