use std::{
    fs,
    io::{self, BufRead, BufReader, Read as _, Write},
    os::unix::{fs::PermissionsExt as _, net::UnixListener},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::{
    valid_shell_token, CompositorEvent, CompositorRequest, MAX_CONTROL_LINE_BYTES,
    MAX_SHELL_TOKEN_BYTES,
};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use smithay::reexports::calloop::{
    channel::{self, Event},
    EventLoop,
};

use crate::CompositorData;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(20);

pub enum ControlCommand {
    Register {
        pid: u32,
        events: mpsc::Sender<CompositorEvent>,
        reply: mpsc::Sender<std::result::Result<(), String>>,
    },
    Arm {
        pid: u32,
        request_id: u64,
        revision_id: String,
        reply: mpsc::Sender<std::result::Result<u64, String>>,
    },
    QuiesceInput {
        pid: u32,
        request_id: u64,
        revision_id: String,
        reply: mpsc::Sender<std::result::Result<bool, String>>,
    },
    ResumeInput {
        pid: u32,
        request_id: u64,
        revision_id: String,
        reply: mpsc::Sender<std::result::Result<bool, String>>,
    },
    Disconnected {
        pid: u32,
    },
}

pub struct ControlSocketGuard(PathBuf);

impl Drop for ControlSocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn init_control(
    event_loop: &mut EventLoop<CompositorData>,
    socket_path: &Path,
    shell_token: String,
) -> Result<ControlSocketGuard> {
    if !valid_shell_token(&shell_token) {
        bail!("shell token must contain 1 through {MAX_SHELL_TOKEN_BYTES} non-newline bytes");
    }
    let parent = socket_path
        .parent()
        .context("compositor control socket has no parent")?;
    if !parent.is_dir() {
        bail!(
            "compositor control socket parent does not exist: {}",
            parent.display()
        );
    }
    if socket_path.exists() {
        bail!(
            "compositor control socket already exists: {}",
            socket_path.display()
        );
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("bind compositor control socket {}", socket_path.display()))?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;

    let (commands, source) = channel::channel();
    event_loop
        .handle()
        .insert_source(source, |event, _, data| {
            if let Event::Msg(command) = event {
                data.state.handle_control(command);
            }
        })
        .map_err(|_| anyhow::anyhow!("insert compositor control source"))?;

    thread::Builder::new()
        .name("sos-compositor-control".into())
        .spawn(move || {
            for connection in listener.incoming() {
                match connection {
                    Ok(stream) => {
                        if let Err(error) = serve_shell_connection(stream, &shell_token, &commands)
                        {
                            tracing::warn!(error = %error, "compositor control connection failed");
                        }
                    }
                    Err(error) => {
                        tracing::error!(error = %error, "compositor control listener failed");
                        return;
                    }
                }
            }
        })?;

    Ok(ControlSocketGuard(socket_path.to_path_buf()))
}

fn serve_shell_connection(
    mut stream: std::os::unix::net::UnixStream,
    shell_token: &str,
    commands: &channel::Sender<ControlCommand>,
) -> Result<()> {
    let credentials = getsockopt(&stream, PeerCredentials)?;
    let peer_pid = u32::try_from(credentials.pid()).context("control peer PID is invalid")?;
    stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let request =
        read_request(&mut reader)?.context("control client closed before registration")?;
    let CompositorRequest::RegisterShell {
        request_id,
        token,
        pid,
    } = request
    else {
        write_event(
            &mut stream,
            &CompositorEvent::Rejected {
                request_id: request.request_id(),
                error: "first compositor request must register the shell".into(),
            },
        )?;
        return Ok(());
    };
    if token != shell_token || pid != peer_pid {
        write_event(
            &mut stream,
            &CompositorEvent::Rejected {
                request_id,
                error: "shell token or peer PID did not match".into(),
            },
        )?;
        return Ok(());
    }

    let (events, event_rx) = mpsc::channel();
    let (reply, reply_rx) = mpsc::channel();
    commands.send(ControlCommand::Register { pid, events, reply })?;
    match reply_rx.recv_timeout(CONTROL_TIMEOUT)? {
        Ok(()) => write_event(
            &mut stream,
            &CompositorEvent::Registered { request_id, pid },
        )?,
        Err(error) => {
            write_event(
                &mut stream,
                &CompositorEvent::Rejected { request_id, error },
            )?;
            return Ok(());
        }
    }
    tracing::info!(pid, "authenticated SOS shell control connection");
    stream.set_read_timeout(Some(EVENT_POLL_INTERVAL))?;

    let result = loop {
        while let Ok(event) = event_rx.try_recv() {
            write_event(&mut stream, &event)?;
        }
        match read_request(&mut reader) {
            Ok(Some(CompositorRequest::QuiesceInput {
                request_id,
                revision_id,
            })) => {
                let (reply, reply_rx) = mpsc::channel();
                commands.send(ControlCommand::QuiesceInput {
                    pid,
                    request_id,
                    revision_id: revision_id.clone(),
                    reply,
                })?;
                let event = match reply_rx.recv_timeout(CONTROL_TIMEOUT)? {
                    Ok(_) => CompositorEvent::InputQuiesced {
                        request_id,
                        revision_id,
                    },
                    Err(error) => CompositorEvent::Rejected { request_id, error },
                };
                write_event(&mut stream, &event)?;
            }
            Ok(Some(CompositorRequest::ResumeInput {
                request_id,
                revision_id,
            })) => {
                let (reply, reply_rx) = mpsc::channel();
                commands.send(ControlCommand::ResumeInput {
                    pid,
                    request_id,
                    revision_id: revision_id.clone(),
                    reply,
                })?;
                let event = match reply_rx.recv_timeout(CONTROL_TIMEOUT)? {
                    Ok(_) => CompositorEvent::InputResumed {
                        request_id,
                        revision_id,
                    },
                    Err(error) => CompositorEvent::Rejected { request_id, error },
                };
                write_event(&mut stream, &event)?;
            }
            Ok(Some(CompositorRequest::ArmPresentation {
                request_id,
                revision_id,
            })) => {
                let (reply, reply_rx) = mpsc::channel();
                commands.send(ControlCommand::Arm {
                    pid,
                    request_id,
                    revision_id: revision_id.clone(),
                    reply,
                })?;
                let event = match reply_rx.recv_timeout(CONTROL_TIMEOUT)? {
                    Ok(after_commit_sequence) => CompositorEvent::Armed {
                        request_id,
                        revision_id,
                        after_commit_sequence,
                    },
                    Err(error) => CompositorEvent::Rejected { request_id, error },
                };
                write_event(&mut stream, &event)?;
            }
            Ok(Some(CompositorRequest::RegisterShell { request_id, .. })) => write_event(
                &mut stream,
                &CompositorEvent::Rejected {
                    request_id,
                    error: "shell is already registered on this connection".into(),
                },
            )?,
            Ok(None) => break Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => break Err(error.into()),
        }
    };
    let _ = commands.send(ControlCommand::Disconnected { pid });
    result
}

fn read_request(
    reader: &mut BufReader<std::os::unix::net::UnixStream>,
) -> io::Result<Option<CompositorRequest>> {
    let mut line = String::new();
    let count = reader
        .take((MAX_CONTROL_LINE_BYTES + 1) as u64)
        .read_line(&mut line)?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_CONTROL_LINE_BYTES || !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compositor control line exceeded its bound or omitted newline",
        ));
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(io::Error::other)
}

fn write_event(
    stream: &mut std::os::unix::net::UnixStream,
    event: &CompositorEvent,
) -> io::Result<()> {
    serde_json::to_writer(&mut *stream, event).map_err(io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()
}
