mod coordinator;
mod graph;
mod graph_supervisor;
mod host;
mod reference;
mod registry;
mod store;

use std::{path::PathBuf, process::ExitStatus, time::Duration};

pub use coordinator::{
    ActivationJournal, CoordinatedSupervisor, CoordinationError, CoordinationEvent,
    CoordinatorFaultPoint, JournalPhase,
};
pub use experience_host_protocol::{HostEvent, HostRequest};
pub use graph::{GraphResolver, GraphStore};
pub use graph_supervisor::{
    ExperienceGraphSupervisor, GraphActivationFaultPoint, GraphActivationJournal,
    GraphActivationPhase, PreparedGraphActivation,
};
pub use host::{ExperienceHost, HostCommand};
pub use reference::{install_reference_composition, ReferenceComposition};
pub use registry::{ExperienceRecord, ExperienceRegistry, STOCK_SHELL_EXPERIENCE_ID};
pub use store::{
    AssetIdentity, DurableState, FileIdentity, RevisionAssetInput, RevisionInput, RevisionManifest,
    RevisionPackageInput, RevisionStore, VerifiedRevision, MAX_REVISION_ASSETS,
    MAX_REVISION_ASSET_BYTES, MAX_REVISION_ASSET_TOTAL_BYTES,
};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid revision id: {0}")]
    InvalidRevisionId(String),
    #[error("invalid revision: {0}")]
    InvalidRevision(String),
    #[error("invalid current pointer: {0}")]
    InvalidPointer(PathBuf),
    #[error("invalid experience registry: {0}")]
    InvalidRegistry(String),
    #[error("invalid experience graph: {0}")]
    InvalidGraph(String),
    #[error("experience host exited: {0}")]
    HostExited(ExitStatus),
    #[error("experience host did not respond within {0:?}")]
    HostTimeout(Duration),
    #[error("experience host sent an invalid event")]
    InvalidHostEvent,
    #[error("experience host protocol failed: {0}")]
    HostProtocol(String),
    #[error("experience revision was rejected: {0}")]
    HostRejected(String),
    #[error("no current revision is initialized")]
    NoCurrentRevision,
    #[error("the supervisor has no active experience host")]
    NoActiveHost,
    #[error("injected graph activation fault: {0}")]
    InjectedGraphActivationFault(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupervisorEvent {
    Booted {
        revision_id: String,
        host_pid: u32,
    },
    Activated {
        revision_id: String,
        host_pid: u32,
    },
    HostRestarted {
        revision_id: String,
        failed_host_pid: u32,
        host_pid: u32,
    },
}

pub struct RevisionSupervisor {
    store: RevisionStore,
    host_command: HostCommand,
    host_timeout: Duration,
    host: Option<ExperienceHost>,
    active_revision: Option<String>,
}

pub struct PreparedRevision {
    revision_id: String,
    previous_revision: String,
    input_quiesced: bool,
}

impl PreparedRevision {
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }

    pub fn previous_revision(&self) -> &str {
        &self.previous_revision
    }
}

impl RevisionSupervisor {
    pub fn new(store: RevisionStore, host_command: HostCommand, host_timeout: Duration) -> Self {
        Self {
            store,
            host_command,
            host_timeout,
            host: None,
            active_revision: None,
        }
    }

    pub fn boot(&mut self) -> Result<Option<SupervisorEvent>> {
        let Some(current) = self.store.current()? else {
            return Ok(None);
        };
        let mut host = ExperienceHost::launch(self.host_command.clone(), self.host_timeout)?;
        host.boot(&current)?;
        let event = SupervisorEvent::Booted {
            revision_id: current.manifest.revision_id.clone(),
            host_pid: host.id(),
        };
        self.active_revision = Some(current.manifest.revision_id);
        self.host = Some(host);
        Ok(Some(event))
    }

    pub fn activate(&mut self, revision_id: &str) -> Result<SupervisorEvent> {
        let prepared = self.prepare(revision_id)?;
        self.commit_prepared(prepared)
    }

    pub fn prepare(&mut self, revision_id: &str) -> Result<PreparedRevision> {
        let previous_revision = self
            .store
            .current()?
            .map(|revision| revision.manifest.revision_id)
            .ok_or(Error::NoCurrentRevision)?;
        let revision = self.store.verify(revision_id)?;
        let prepared = self
            .host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .prepare(&revision);
        if let Err(error) = prepared {
            if !matches!(&error, Error::HostRejected(_)) {
                self.restart_current_host()?;
            }
            return Err(error);
        }
        Ok(PreparedRevision {
            revision_id: revision_id.into(),
            previous_revision,
            input_quiesced: false,
        })
    }

    pub fn quiesce_prepared(&mut self, prepared: &mut PreparedRevision) -> Result<()> {
        if prepared.input_quiesced {
            return Ok(());
        }
        let quiesced = self
            .host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .quiesce_input(&prepared.revision_id);
        if let Err(error) = quiesced {
            if !matches!(&error, Error::HostRejected(_)) {
                self.restart_current_host()?;
            }
            return Err(error);
        }
        prepared.input_quiesced = true;
        Ok(())
    }

    pub fn discard_prepared(&mut self, prepared: PreparedRevision) -> Result<()> {
        self.host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .discard(&prepared.revision_id)
    }

    pub fn commit_prepared(&mut self, mut prepared: PreparedRevision) -> Result<SupervisorEvent> {
        self.quiesce_prepared(&mut prepared)?;
        let presented = self
            .host
            .as_mut()
            .ok_or(Error::NoActiveHost)?
            .present(&prepared.revision_id);
        if let Err(error) = presented {
            if matches!(&error, Error::HostRejected(_)) {
                if let Some(host) = self.host.as_mut() {
                    host.discard(&prepared.revision_id).ok();
                }
            } else {
                self.restart_current_host()?;
            }
            return Err(error);
        }
        if let Some(status) = self.host.as_mut().ok_or(Error::NoActiveHost)?.try_wait()? {
            let error = Error::HostExited(status);
            self.restart_current_host()?;
            return Err(error);
        }
        let host = self.host.as_mut().ok_or(Error::NoActiveHost)?;
        if let Err(error) = self.store.set_current(&prepared.revision_id) {
            if let Ok(previous) = self.store.verify(&prepared.previous_revision) {
                host.prepare(&previous).ok();
                host.present(&prepared.previous_revision).ok();
            }
            return Err(error);
        }
        self.active_revision = Some(prepared.revision_id.clone());
        Ok(SupervisorEvent::Activated {
            revision_id: prepared.revision_id,
            host_pid: host.id(),
        })
    }

    pub fn poll(&mut self) -> Result<Option<SupervisorEvent>> {
        let Some(host) = self.host.as_mut() else {
            return Ok(None);
        };
        if host.try_wait()?.is_none() {
            return Ok(None);
        }
        let (revision_id, failed_host_pid, host_pid) = self.restart_current_host()?;
        Ok(Some(SupervisorEvent::HostRestarted {
            revision_id,
            failed_host_pid,
            host_pid,
        }))
    }

    pub fn active_revision(&self) -> Option<&str> {
        self.active_revision.as_deref()
    }

    pub fn host_pid(&self) -> Option<u32> {
        self.host.as_ref().map(ExperienceHost::id)
    }

    pub fn current_revision(&self) -> Result<Option<String>> {
        Ok(self
            .store
            .current()?
            .map(|revision| revision.manifest.revision_id))
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(host) = self.host.take() {
            host.terminate()?;
        }
        self.active_revision = None;
        Ok(())
    }

    pub fn restart_host(&mut self) -> Result<SupervisorEvent> {
        let (revision_id, failed_host_pid, host_pid) = self.restart_current_host()?;
        Ok(SupervisorEvent::HostRestarted {
            revision_id,
            failed_host_pid,
            host_pid,
        })
    }

    fn restart_current_host(&mut self) -> Result<(String, u32, u32)> {
        let failed_host_pid = self
            .host
            .take()
            .map(|host| {
                let pid = host.id();
                drop(host);
                pid
            })
            .ok_or(Error::NoActiveHost)?;
        let current = self.store.current()?.ok_or(Error::NoCurrentRevision)?;
        let mut replacement = ExperienceHost::launch(self.host_command.clone(), self.host_timeout)?;
        replacement.boot(&current)?;
        let host_pid = replacement.id();
        let revision_id = current.manifest.revision_id;
        self.active_revision = Some(revision_id.clone());
        self.host = Some(replacement);
        Ok((revision_id, failed_host_pid, host_pid))
    }
}

impl Drop for RevisionSupervisor {
    fn drop(&mut self) {
        self.shutdown().ok();
    }
}
