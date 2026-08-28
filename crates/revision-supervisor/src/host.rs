use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::Duration,
};

use experience_host_protocol::{
    ExperienceLifecycleOperation, HostEvent, HostRequest, TopLevelExperience,
};

use crate::{Error, Result};

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
    pending_lifecycle: VecDeque<ExperienceLifecycleRequest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperienceLifecycleRequest {
    pub experience_id: String,
    pub operation: ExperienceLifecycleOperation,
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
            pending_lifecycle: VecDeque::new(),
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

    pub fn take_lifecycle_requests(&mut self) -> Result<Vec<ExperienceLifecycleRequest>> {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Ok(HostEvent::ExperienceLifecycleRequested {
                    experience_id,
                    operation,
                    ..
                }) => self
                    .pending_lifecycle
                    .push_back(ExperienceLifecycleRequest {
                        experience_id,
                        operation,
                    }),
                Ok(_) => return Err(Error::InvalidHostEvent),
                Err(error) => return Err(Error::HostProtocol(error)),
            }
        }
        Ok(self.pending_lifecycle.drain(..).collect())
    }

    pub fn boot_graph(
        &mut self,
        graph_id: &str,
        graph_path: PathBuf,
        revision_root: PathBuf,
        launchable_experiences: Vec<TopLevelExperience>,
    ) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::BootGraph {
            request_id,
            graph_id: graph_id.into(),
            graph_path,
            revision_root,
            launchable_experiences,
        })?;
        expect_graph_presented(event, request_id, graph_id)
    }

    pub fn prepare_graph(
        &mut self,
        graph_id: &str,
        graph_path: PathBuf,
        revision_root: PathBuf,
        launchable_experiences: Vec<TopLevelExperience>,
    ) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::PrepareGraph {
            request_id,
            graph_id: graph_id.into(),
            graph_path,
            revision_root,
            launchable_experiences,
        })?;
        match event {
            HostEvent::GraphPrepared {
                request_id: received,
                graph_id: received_graph,
            } if received == request_id && received_graph == graph_id => Ok(()),
            HostEvent::GraphRejected { error, .. } => Err(Error::HostRejected(error)),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn present_graph(&mut self, graph_id: &str) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::PresentGraph {
            request_id,
            graph_id: graph_id.into(),
        })?;
        expect_graph_presented(event, request_id, graph_id)?;
        let request_id = self.request_id();
        let event = self.call(HostRequest::ConfirmGraph {
            request_id,
            graph_id: graph_id.into(),
        })?;
        match event {
            HostEvent::GraphConfirmed {
                request_id: received,
                graph_id: received_graph,
            } if received == request_id && received_graph == graph_id => Ok(()),
            HostEvent::GraphRejected { error, .. } => Err(Error::HostRejected(error)),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn finalize_graph(&mut self, graph_id: &str) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::FinalizeGraph {
            request_id,
            graph_id: graph_id.into(),
        })?;
        match event {
            HostEvent::GraphFinalized {
                request_id: received,
                graph_id: received_graph,
            } if received == request_id && received_graph == graph_id => Ok(()),
            HostEvent::GraphRejected { error, .. } => Err(Error::HostRejected(error)),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn quiesce_graph_input(&mut self, graph_id: &str) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::QuiesceGraphInput {
            request_id,
            graph_id: graph_id.into(),
        })?;
        match event {
            HostEvent::GraphInputQuiesced {
                request_id: received,
                graph_id: received_graph,
            } if received == request_id && received_graph == graph_id => Ok(()),
            HostEvent::GraphRejected { error, .. } => Err(Error::HostRejected(error)),
            _ => Err(Error::InvalidHostEvent),
        }
    }

    pub fn discard_graph(&mut self, graph_id: &str) -> Result<()> {
        let request_id = self.request_id();
        let event = self.call(HostRequest::DiscardGraph {
            request_id,
            graph_id: graph_id.into(),
        })?;
        match event {
            HostEvent::GraphDiscarded {
                request_id: received,
                graph_id: received_graph,
            } if received == request_id && received_graph == graph_id => Ok(()),
            HostEvent::GraphRejected { error, .. } => Err(Error::HostRejected(error)),
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
        let deadline = std::time::Instant::now() + self.timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match self.events.recv_timeout(remaining) {
                Ok(Ok(HostEvent::ExperienceLifecycleRequested {
                    experience_id,
                    operation,
                    ..
                })) => self
                    .pending_lifecycle
                    .push_back(ExperienceLifecycleRequest {
                        experience_id,
                        operation,
                    }),
                Ok(Ok(event)) if event.request_id() == request.request_id() => return Ok(event),
                Ok(Ok(_)) => return Err(Error::InvalidHostEvent),
                Ok(Err(error)) => return Err(Error::HostProtocol(error)),
                Err(RecvTimeoutError::Timeout) => {
                    return if let Some(status) = self.child.try_wait()? {
                        Err(Error::HostExited(status))
                    } else {
                        Err(Error::HostTimeout(self.timeout))
                    };
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let status = self.child.wait()?;
                    return Err(Error::HostExited(status));
                }
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

fn expect_graph_presented(event: HostEvent, request_id: u64, graph_id: &str) -> Result<()> {
    match event {
        HostEvent::GraphPresented {
            request_id: received,
            graph_id: received_graph,
        } if received == request_id && received_graph == graph_id => Ok(()),
        HostEvent::GraphRejected { error, .. } => Err(Error::HostRejected(error)),
        _ => Err(Error::InvalidHostEvent),
    }
}
