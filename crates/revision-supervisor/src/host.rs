use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::Duration,
};

use experience_host_protocol::{HostEvent, HostRequest};

use crate::{Error, Result, VerifiedRevision};

#[derive(Clone, Debug)]
pub struct HostCommand {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

impl HostCommand {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
        }
    }

    pub fn with_args(executable: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            executable: executable.into(),
            args,
        }
    }
}

pub struct ExperienceHost {
    command: HostCommand,
    child: Child,
    input: ChildStdin,
    events: Receiver<std::result::Result<HostEvent, String>>,
    timeout: Duration,
    next_request_id: u64,
}

impl ExperienceHost {
    pub fn launch(command: HostCommand, timeout: Duration) -> Result<Self> {
        let mut child = Command::new(&command.executable)
            .args(&command.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Error::HostProtocol("host stdin was not piped".into()))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| Error::HostProtocol("host stdout was not piped".into()))?;
        let (events_tx, events_rx) = mpsc::channel();
        thread::Builder::new()
            .name("sos-host-events".into())
            .spawn(move || {
                for line in BufReader::new(output).lines() {
                    let event = line.map_err(|error| error.to_string()).and_then(|line| {
                        serde_json::from_str(&line).map_err(|error| error.to_string())
                    });
                    if events_tx.send(event).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            command,
            child,
            input,
            events: events_rx,
            timeout,
            next_request_id: 1,
        })
    }

    pub fn command(&self) -> &HostCommand {
        &self.command
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        Ok(self.child.try_wait()?)
    }

    pub fn boot(&mut self, revision: &VerifiedRevision) -> Result<()> {
        let request_id = self.request_id();
        let revision_id = revision.manifest.revision_id.clone();
        let event = self.call(HostRequest::Boot {
            request_id,
            revision_id: revision_id.clone(),
            revision_path: revision.directory.clone(),
            experience_api_version: revision.manifest.experience_api_version,
        })?;
        expect_presented(event, request_id, &revision_id)
    }

    pub fn prepare(&mut self, revision: &VerifiedRevision) -> Result<()> {
        let request_id = self.request_id();
        let revision_id = revision.manifest.revision_id.clone();
        let event = self.call(HostRequest::Prepare {
            request_id,
            revision_id: revision_id.clone(),
            revision_path: revision.directory.clone(),
            experience_api_version: revision.manifest.experience_api_version,
        })?;
        match event {
            HostEvent::Prepared {
                request_id: received,
                revision_id: received_revision,
            } if received == request_id && received_revision == revision_id => Ok(()),
            HostEvent::Rejected { error, .. } => Err(Error::HostRejected(error)),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn present(&mut self, revision_id: &str) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::Present {
            request_id,
            revision_id: revision_id.into(),
        })?;
        expect_presented(event, request_id, revision_id)?;
        let request_id = self.request_id();
        let event = self.call(HostRequest::Confirm {
            request_id,
            revision_id: revision_id.into(),
        })?;
        match event {
            HostEvent::Confirmed {
                request_id: received,
                revision_id: received_revision,
            } if received == request_id && received_revision == revision_id => Ok(()),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn quiesce_input(&mut self, revision_id: &str) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::QuiesceInput {
            request_id,
            revision_id: revision_id.into(),
        })?;
        match event {
            HostEvent::InputQuiesced {
                request_id: received,
                revision_id: received_revision,
            } if received == request_id && received_revision == revision_id => Ok(()),
            HostEvent::Rejected { error, .. } => Err(Error::HostRejected(error)),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn discard(&mut self, revision_id: &str) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::Discard {
            request_id,
            revision_id: revision_id.into(),
        })?;
        match event {
            HostEvent::Discarded {
                request_id: received,
                revision_id: received_revision,
            } if received == request_id && received_revision == revision_id => Ok(()),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn terminate(mut self) -> Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        let request_id = self.request_id();
        let _ = self.call(HostRequest::Shutdown { request_id });
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        self.child.wait()?;
        Ok(())
    }

    fn request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    fn call(&mut self, request: HostRequest) -> Result<HostEvent> {
        if let Some(status) = self.child.try_wait()? {
            return Err(Error::HostExited(status));
        }
        serde_json::to_writer(&mut self.input, &request)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        match self.events.recv_timeout(self.timeout) {
            Ok(Ok(event)) if event.request_id() == request.request_id() => Ok(event),
            Ok(Ok(_)) => Err(Error::InvalidHostEvent),
            Ok(Err(error)) => Err(Error::HostProtocol(error)),
            Err(RecvTimeoutError::Timeout) => {
                if let Some(status) = self.child.try_wait()? {
                    Err(Error::HostExited(status))
                } else {
                    Err(Error::HostTimeout(self.timeout))
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                let status = self.child.wait()?;
                Err(Error::HostExited(status))
            }
        }
    }
}

impl Drop for ExperienceHost {
    fn drop(&mut self) {
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            self.child.kill().ok();
        }
        self.child.wait().ok();
    }
}

fn expect_presented(event: HostEvent, request_id: u64, revision_id: &str) -> Result<()> {
    match event {
        HostEvent::Presented {
            request_id: received,
            revision_id: received_revision,
        } if received == request_id && received_revision == revision_id => Ok(()),
        HostEvent::Rejected { error, .. } => Err(Error::HostRejected(error)),
        _ => Err(Error::InvalidHostEvent),
    }
}
