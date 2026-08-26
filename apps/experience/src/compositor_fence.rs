use std::{
    env,
    io::{self, BufRead, BufReader, Read as _, Write},
    os::unix::net::UnixStream,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::{
    read_shell_token_file, valid_shell_token, CompositorEvent, CompositorRequest,
    PresentationEvidence, ShellOverlayConfiguration, ShellStateSnapshot, WindowControlAction,
    WindowSpaceConfiguration, MAX_CONTROL_LINE_BYTES,
};

const CONTROL_SOCKET_ENV: &str = "SOS_COMPOSITOR_CONTROL";
const SHELL_TOKEN_ENV: &str = "SOS_COMPOSITOR_TOKEN";
const SHELL_TOKEN_FILE_ENV: &str = "SOS_COMPOSITOR_TOKEN_FILE";
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
    ShellOverlayMoved(ShellOverlayConfiguration),
    ShellOverlayActivated,
    ShellOverlayHoverChanged(bool),
    ShellStateChanged(ShellStateSnapshot),
    WindowSpaceRejected(String),
    ShellOverlayRejected(String),
    WindowControlRejected(String),
    Failed(String),
}

enum FenceCommand {
    QuiesceInput {
        request_id: u64,
        revision_id: String,
        reply: mpsc::Sender<std::result::Result<(), String>>,
    },
    ResumeInput {
        request_id: u64,
        revision_id: String,
        reply: mpsc::Sender<std::result::Result<(), String>>,
    },
    Arm {
        request_id: u64,
        revision_id: String,
        reply: mpsc::Sender<std::result::Result<u64, String>>,
    },
    ConfigureWindowSpace {
        request_id: u64,
        configuration: WindowSpaceConfiguration,
    },
    ConfigureShellOverlay {
        request_id: u64,
        configuration: ShellOverlayConfiguration,
    },
    ControlWindow {
        request_id: u64,
        window_id: String,
        operation: WindowControlAction,
    },
}

pub struct CompositorFence {
    commands: mpsc::Sender<FenceCommand>,
    events: async_channel::Receiver<FenceEvent>,
    next_window_space_request_id: AtomicU64,
}

impl CompositorFence {
    pub fn from_environment() -> Result<Option<Self>> {
        let socket = env::var_os(CONTROL_SOCKET_ENV);
        let inline_token = env::var_os(SHELL_TOKEN_ENV);
        let token_file = env::var_os(SHELL_TOKEN_FILE_ENV);
        if socket.is_none() {
            if inline_token.is_some() || token_file.is_some() {
                bail!(
                    "{CONTROL_SOCKET_ENV} is required when a compositor shell token is configured"
                );
            }
            return Ok(None);
        }
        let token = match (inline_token, token_file) {
            (Some(token), None) => token
                .into_string()
                .map_err(|_| anyhow::anyhow!("{SHELL_TOKEN_ENV} is not valid UTF-8"))?,
            (None, Some(path)) => read_shell_token_file(Path::new(&path)).with_context(|| {
                format!(
                    "read compositor shell credential from {}",
                    Path::new(&path).display()
                )
            })?,
            (Some(_), Some(_)) => {
                bail!("use exactly one of {SHELL_TOKEN_ENV} and {SHELL_TOKEN_FILE_ENV}")
            }
            (None, None) => bail!(
                "one of {SHELL_TOKEN_ENV} or {SHELL_TOKEN_FILE_ENV} is required when {CONTROL_SOCKET_ENV} is set"
            ),
        };
        Self::connect(Path::new(&socket.unwrap()), token).map(Some)
    }

    fn connect(socket_path: &Path, token: String) -> Result<Self> {
        if !valid_shell_token(&token) {
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
            next_window_space_request_id: AtomicU64::new(1_u64 << 63),
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

    pub fn quiesce_input(&self, request_id: u64, revision_id: &str) -> Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.commands
            .send(FenceCommand::QuiesceInput {
                request_id,
                revision_id: revision_id.into(),
                reply: reply_tx,
            })
            .context("compositor fence thread is unavailable")?;
        reply_rx
            .recv_timeout(CONTROL_TIMEOUT)
            .context("timed out quiescing compositor input")?
            .map_err(anyhow::Error::msg)
    }

    pub fn resume_input(&self, request_id: u64, revision_id: &str) -> Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.commands
            .send(FenceCommand::ResumeInput {
                request_id,
                revision_id: revision_id.into(),
                reply: reply_tx,
            })
            .context("compositor fence thread is unavailable")?;
        reply_rx
            .recv_timeout(CONTROL_TIMEOUT)
            .context("timed out resuming compositor input")?
            .map_err(anyhow::Error::msg)
    }

    pub fn events(&self) -> async_channel::Receiver<FenceEvent> {
        self.events.clone()
    }

    pub fn configure_window_space(&self, configuration: WindowSpaceConfiguration) -> Result<()> {
        let request_id = self
            .next_window_space_request_id
            .fetch_add(1, Ordering::Relaxed);
        self.commands
            .send(FenceCommand::ConfigureWindowSpace {
                request_id,
                configuration,
            })
            .context("compositor fence thread is unavailable")
    }

    pub fn configure_shell_overlay(&self, configuration: ShellOverlayConfiguration) -> Result<()> {
        let request_id = self
            .next_window_space_request_id
            .fetch_add(1, Ordering::Relaxed);
        self.commands
            .send(FenceCommand::ConfigureShellOverlay {
                request_id,
                configuration,
            })
            .context("compositor fence thread is unavailable")
    }

    pub fn control_window(&self, window_id: String, operation: WindowControlAction) -> Result<()> {
        let request_id = self
            .next_window_space_request_id
            .fetch_add(1, Ordering::Relaxed);
        self.commands
            .send(FenceCommand::ControlWindow {
                request_id,
                window_id,
                operation,
            })
            .context("compositor fence thread is unavailable")
    }
}

fn run_io(
    mut stream: UnixStream,
    mut reader: BufReader<UnixStream>,
    commands: mpsc::Receiver<FenceCommand>,
    events: &async_channel::Sender<FenceEvent>,
) -> Result<()> {
    loop {
        if let Ok(command) = commands.try_recv() {
            stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
            match command {
                FenceCommand::QuiesceInput {
                    request_id,
                    revision_id,
                    reply,
                } => {
                    write_request(
                        &mut stream,
                        &CompositorRequest::QuiesceInput {
                            request_id,
                            revision_id: revision_id.clone(),
                        },
                    )?;
                    let result =
                        wait_for_ack(&mut reader, events, request_id, &revision_id, |event| {
                            matches!(event, CompositorEvent::InputQuiesced { .. })
                        })
                        .map(|_| ());
                    let _ = reply.send(result);
                }
                FenceCommand::ResumeInput {
                    request_id,
                    revision_id,
                    reply,
                } => {
                    write_request(
                        &mut stream,
                        &CompositorRequest::ResumeInput {
                            request_id,
                            revision_id: revision_id.clone(),
                        },
                    )?;
                    let result =
                        wait_for_ack(&mut reader, events, request_id, &revision_id, |event| {
                            matches!(event, CompositorEvent::InputResumed { .. })
                        })
                        .map(|_| ());
                    let _ = reply.send(result);
                }
                FenceCommand::Arm {
                    request_id,
                    revision_id,
                    reply,
                } => {
                    write_request(
                        &mut stream,
                        &CompositorRequest::ArmPresentation {
                            request_id,
                            revision_id: revision_id.clone(),
                        },
                    )?;
                    let result =
                        wait_for_ack(&mut reader, events, request_id, &revision_id, |event| {
                            matches!(event, CompositorEvent::Armed { .. })
                        })
                        .map(|event| match event {
                            CompositorEvent::Armed {
                                after_commit_sequence,
                                ..
                            } => after_commit_sequence,
                            _ => unreachable!("arm acknowledgement was matched"),
                        });
                    let _ = reply.send(result);
                }
                FenceCommand::ConfigureWindowSpace {
                    request_id,
                    configuration,
                } => {
                    write_request(
                        &mut stream,
                        &CompositorRequest::ConfigureWindowSpace {
                            request_id,
                            configuration,
                        },
                    )?;
                    match wait_for_window_space_ack(&mut reader, events, request_id, configuration)
                    {
                        Ok(()) => {}
                        Err(WindowSpaceAckError::Rejected(error)) => {
                            events.send_blocking(FenceEvent::WindowSpaceRejected(error))?
                        }
                        Err(WindowSpaceAckError::Fatal(error)) => {
                            return Err(anyhow::Error::msg(error));
                        }
                    }
                }
                FenceCommand::ConfigureShellOverlay {
                    request_id,
                    configuration,
                } => {
                    write_request(
                        &mut stream,
                        &CompositorRequest::ConfigureShellOverlay {
                            request_id,
                            configuration,
                        },
                    )?;
                    match wait_for_shell_overlay_ack(&mut reader, events, request_id, configuration)
                    {
                        Ok(()) => {}
                        Err(WindowSpaceAckError::Rejected(error)) => {
                            events.send_blocking(FenceEvent::ShellOverlayRejected(error))?
                        }
                        Err(WindowSpaceAckError::Fatal(error)) => {
                            return Err(anyhow::Error::msg(error));
                        }
                    }
                }
                FenceCommand::ControlWindow {
                    request_id,
                    window_id,
                    operation,
                } => {
                    write_request(
                        &mut stream,
                        &CompositorRequest::ControlWindow {
                            request_id,
                            window_id: window_id.clone(),
                            operation,
                        },
                    )?;
                    match wait_for_window_control_ack(
                        &mut reader,
                        events,
                        request_id,
                        &window_id,
                        operation,
                    ) {
                        Ok(()) => {}
                        Err(WindowSpaceAckError::Rejected(error)) => {
                            events.send_blocking(FenceEvent::WindowControlRejected(error))?
                        }
                        Err(WindowSpaceAckError::Fatal(error)) => {
                            return Err(anyhow::Error::msg(error));
                        }
                    }
                }
            }
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
            Ok(Some(CompositorEvent::ShellOverlayMoved { configuration, .. })) => {
                events.send_blocking(FenceEvent::ShellOverlayMoved(configuration))?
            }
            Ok(Some(CompositorEvent::ShellOverlayActivated { .. })) => {
                events.send_blocking(FenceEvent::ShellOverlayActivated)?
            }
            Ok(Some(CompositorEvent::ShellOverlayHoverChanged { hovered, .. })) => {
                events.send_blocking(FenceEvent::ShellOverlayHoverChanged(hovered))?
            }
            Ok(Some(CompositorEvent::ShellStateChanged { state, .. })) => {
                events.send_blocking(FenceEvent::ShellStateChanged(state))?
            }
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

fn wait_for_ack(
    reader: &mut BufReader<UnixStream>,
    events: &async_channel::Sender<FenceEvent>,
    request_id: u64,
    revision_id: &str,
    matches_ack: impl Fn(&CompositorEvent) -> bool,
) -> std::result::Result<CompositorEvent, String> {
    loop {
        let event = read_event(reader)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "compositor closed while awaiting control acknowledgement".to_owned())?;
        match event {
            CompositorEvent::Rejected {
                request_id: received,
                error,
            } if received == request_id => return Err(error),
            CompositorEvent::Presented {
                request_id,
                revision_id,
                commit_sequence,
                submit_sequence,
                evidence,
            } => events
                .send_blocking(FenceEvent::Presented(Presented {
                    request_id,
                    revision_id,
                    commit_sequence,
                    submit_sequence,
                    evidence,
                }))
                .map_err(|error| error.to_string())?,
            CompositorEvent::ShellOverlayMoved { configuration, .. } => events
                .send_blocking(FenceEvent::ShellOverlayMoved(configuration))
                .map_err(|error| error.to_string())?,
            CompositorEvent::ShellOverlayActivated { .. } => events
                .send_blocking(FenceEvent::ShellOverlayActivated)
                .map_err(|error| error.to_string())?,
            CompositorEvent::ShellOverlayHoverChanged { hovered, .. } => events
                .send_blocking(FenceEvent::ShellOverlayHoverChanged(hovered))
                .map_err(|error| error.to_string())?,
            CompositorEvent::ShellStateChanged { state, .. } => events
                .send_blocking(FenceEvent::ShellStateChanged(state))
                .map_err(|error| error.to_string())?,
            event
                if event.request_id() == request_id
                    && event_revision(&event) == Some(revision_id)
                    && matches_ack(&event) =>
            {
                return Ok(event)
            }
            event => return Err(format!("unexpected compositor control event: {event:?}")),
        }
    }
}

fn wait_for_window_space_ack(
    reader: &mut BufReader<UnixStream>,
    events: &async_channel::Sender<FenceEvent>,
    request_id: u64,
    expected: WindowSpaceConfiguration,
) -> std::result::Result<(), WindowSpaceAckError> {
    loop {
        let event = read_event(reader)
            .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?
            .ok_or_else(|| {
                WindowSpaceAckError::Fatal(
                    "compositor closed while configuring window space".to_owned(),
                )
            })?;
        match event {
            CompositorEvent::Rejected {
                request_id: received,
                error,
            } if received == request_id => {
                return Err(WindowSpaceAckError::Rejected(error));
            }
            CompositorEvent::Presented {
                request_id,
                revision_id,
                commit_sequence,
                submit_sequence,
                evidence,
            } => events
                .send_blocking(FenceEvent::Presented(Presented {
                    request_id,
                    revision_id,
                    commit_sequence,
                    submit_sequence,
                    evidence,
                }))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellOverlayMoved { configuration, .. } => events
                .send_blocking(FenceEvent::ShellOverlayMoved(configuration))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellOverlayActivated { .. } => events
                .send_blocking(FenceEvent::ShellOverlayActivated)
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellOverlayHoverChanged { hovered, .. } => events
                .send_blocking(FenceEvent::ShellOverlayHoverChanged(hovered))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellStateChanged { state, .. } => events
                .send_blocking(FenceEvent::ShellStateChanged(state))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::WindowSpaceConfigured {
                request_id: received,
                configuration,
            } if received == request_id && configuration == expected => return Ok(()),
            event => {
                return Err(WindowSpaceAckError::Fatal(format!(
                    "unexpected compositor control event: {event:?}"
                )));
            }
        }
    }
}

fn wait_for_shell_overlay_ack(
    reader: &mut BufReader<UnixStream>,
    events: &async_channel::Sender<FenceEvent>,
    request_id: u64,
    expected: ShellOverlayConfiguration,
) -> std::result::Result<(), WindowSpaceAckError> {
    loop {
        let event = read_event(reader)
            .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?
            .ok_or_else(|| {
                WindowSpaceAckError::Fatal(
                    "compositor closed while configuring the shell overlay".to_owned(),
                )
            })?;
        match event {
            CompositorEvent::Rejected {
                request_id: received,
                error,
            } if received == request_id => {
                return Err(WindowSpaceAckError::Rejected(error));
            }
            CompositorEvent::Presented {
                request_id,
                revision_id,
                commit_sequence,
                submit_sequence,
                evidence,
            } => events
                .send_blocking(FenceEvent::Presented(Presented {
                    request_id,
                    revision_id,
                    commit_sequence,
                    submit_sequence,
                    evidence,
                }))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellOverlayMoved { configuration, .. } => events
                .send_blocking(FenceEvent::ShellOverlayMoved(configuration))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellOverlayActivated { .. } => events
                .send_blocking(FenceEvent::ShellOverlayActivated)
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellOverlayHoverChanged { hovered, .. } => events
                .send_blocking(FenceEvent::ShellOverlayHoverChanged(hovered))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellStateChanged { state, .. } => events
                .send_blocking(FenceEvent::ShellStateChanged(state))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::ShellOverlayConfigured {
                request_id: received,
                configuration,
            } if received == request_id && configuration == expected => return Ok(()),
            event => {
                return Err(WindowSpaceAckError::Fatal(format!(
                    "unexpected compositor control event: {event:?}"
                )));
            }
        }
    }
}

fn wait_for_window_control_ack(
    reader: &mut BufReader<UnixStream>,
    events: &async_channel::Sender<FenceEvent>,
    request_id: u64,
    expected_window_id: &str,
    expected_operation: WindowControlAction,
) -> std::result::Result<(), WindowSpaceAckError> {
    loop {
        let event = read_event(reader)
            .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?
            .ok_or_else(|| {
                WindowSpaceAckError::Fatal(
                    "compositor closed while controlling an application window".into(),
                )
            })?;
        match event {
            CompositorEvent::Rejected {
                request_id: received,
                error,
            } if received == request_id => return Err(WindowSpaceAckError::Rejected(error)),
            CompositorEvent::ShellStateChanged { state, .. } => events
                .send_blocking(FenceEvent::ShellStateChanged(state))
                .map_err(|error| WindowSpaceAckError::Fatal(error.to_string()))?,
            CompositorEvent::WindowControlled {
                request_id: received,
                window_id,
                operation,
            } if received == request_id
                && window_id == expected_window_id
                && operation == expected_operation =>
            {
                return Ok(())
            }
            event => {
                return Err(WindowSpaceAckError::Fatal(format!(
                    "unexpected compositor control event: {event:?}"
                )))
            }
        }
    }
}

enum WindowSpaceAckError {
    Rejected(String),
    Fatal(String),
}

fn event_revision(event: &CompositorEvent) -> Option<&str> {
    match event {
        CompositorEvent::Armed { revision_id, .. }
        | CompositorEvent::InputQuiesced { revision_id, .. }
        | CompositorEvent::InputResumed { revision_id, .. }
        | CompositorEvent::Presented { revision_id, .. } => Some(revision_id),
        CompositorEvent::Registered { .. }
        | CompositorEvent::WindowSpaceConfigured { .. }
        | CompositorEvent::ShellOverlayConfigured { .. }
        | CompositorEvent::ShellOverlayMoved { .. }
        | CompositorEvent::ShellOverlayActivated { .. }
        | CompositorEvent::ShellOverlayHoverChanged { .. }
        | CompositorEvent::ShellStateChanged { .. }
        | CompositorEvent::WindowControlled { .. }
        | CompositorEvent::Rejected { .. } => None,
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
    use compositor_control_protocol::{
        WindowLayoutMode, WindowSpaceConfiguration, WindowSpaceGeometry,
    };
    use std::os::unix::net::UnixListener;

    #[test]
    fn registers_quiesces_arms_and_forwards_presented_evidence() {
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
            let quiesce = read_request_for_test(&mut reader);
            let CompositorRequest::QuiesceInput {
                request_id,
                revision_id,
            } = quiesce
            else {
                panic!("expected input quiesce")
            };
            write_event_for_test(
                &mut stream,
                &CompositorEvent::InputQuiesced {
                    request_id,
                    revision_id,
                },
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
            let configure = read_request_for_test(&mut reader);
            let CompositorRequest::ConfigureWindowSpace {
                request_id,
                configuration: _,
            } = configure
            else {
                panic!("expected window-space configuration")
            };
            write_event_for_test(
                &mut stream,
                &CompositorEvent::Rejected {
                    request_id,
                    error: "outside output".into(),
                },
            );
            let configure = read_request_for_test(&mut reader);
            let CompositorRequest::ConfigureWindowSpace {
                request_id,
                configuration,
            } = configure
            else {
                panic!("expected corrected window-space configuration")
            };
            write_event_for_test(
                &mut stream,
                &CompositorEvent::WindowSpaceConfigured {
                    request_id,
                    configuration,
                },
            );
        });

        let fence = CompositorFence::connect(&socket, "secret".into()).unwrap();
        let revision = "a".repeat(64);
        fence.quiesce_input(6, &revision).unwrap();
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
        fence
            .configure_window_space(WindowSpaceConfiguration {
                geometry: WindowSpaceGeometry {
                    x: 20,
                    y: 72,
                    width: 1000,
                    height: 680,
                    gap: 12,
                },
                layout: WindowLayoutMode::Floating,
            })
            .unwrap();
        let FenceEvent::WindowSpaceRejected(error) = fence
            .events()
            .recv_blocking()
            .expect("window-space rejection")
        else {
            panic!("expected non-fatal window-space rejection")
        };
        assert_eq!(error, "outside output");
        fence
            .configure_window_space(WindowSpaceConfiguration {
                geometry: WindowSpaceGeometry {
                    x: 24,
                    y: 72,
                    width: 960,
                    height: 640,
                    gap: 8,
                },
                layout: WindowLayoutMode::Tiling,
            })
            .unwrap();
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
