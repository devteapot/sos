mod process;
mod store;

use std::{path::PathBuf, process::ExitStatus, time::Duration};

pub use process::{launch_until_first_frame, CandidateEvent, ManagedCandidate};
pub use store::{
    DurableState, FileIdentity, RevisionInput, RevisionManifest, RevisionStore, VerifiedRevision,
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
    #[error("candidate exited before its first frame: {0}")]
    CandidateExitedBeforeFirstFrame(ExitStatus),
    #[error("candidate did not report its first frame within {0:?}")]
    FirstFrameTimeout(Duration),
    #[error("candidate sent an invalid first-frame event")]
    InvalidCandidateEvent,
    #[error("no rollback revision is available")]
    NoRollbackRevision,
    #[error("no current revision is initialized")]
    NoCurrentRevision,
    #[error("the supervisor has no active process")]
    NoActiveProcess,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupervisorEvent {
    Booted {
        revision_id: String,
        pid: u32,
    },
    Promoted {
        revision_id: String,
        pid: u32,
    },
    RolledBack {
        failed_revision: String,
        restored_revision: String,
        restored_pid: u32,
    },
}

pub struct RevisionSupervisor {
    store: RevisionStore,
    first_frame_timeout: Duration,
    active: Option<ManagedCandidate>,
    rollback_revision: Option<String>,
}

impl RevisionSupervisor {
    pub fn new(store: RevisionStore, first_frame_timeout: Duration) -> Self {
        Self {
            store,
            first_frame_timeout,
            active: None,
            rollback_revision: None,
        }
    }

    pub fn boot(&mut self) -> Result<Option<SupervisorEvent>> {
        let Some(current) = self.store.current()? else {
            return Ok(None);
        };
        let candidate = launch_until_first_frame(&current, self.first_frame_timeout)?;
        let event = SupervisorEvent::Booted {
            revision_id: candidate.revision_id.clone(),
            pid: candidate.id(),
        };
        self.rollback_revision = Some(candidate.revision_id.clone());
        self.active = Some(candidate);
        Ok(Some(event))
    }

    pub fn promote(&mut self, revision_id: &str) -> Result<SupervisorEvent> {
        let previous = self
            .store
            .current()?
            .map(|revision| revision.manifest.revision_id)
            .ok_or(Error::NoCurrentRevision)?;
        let revision = self.store.verify(revision_id)?;
        let candidate = launch_until_first_frame(&revision, self.first_frame_timeout)?;
        self.store.set_current(revision_id)?;
        let pid = candidate.id();
        let replaced = self.active.replace(candidate);
        self.rollback_revision = Some(previous);
        if let Some(active) = replaced {
            active.terminate()?;
        }
        let event = SupervisorEvent::Promoted {
            revision_id: revision_id.into(),
            pid,
        };
        Ok(event)
    }

    pub fn poll(&mut self) -> Result<Option<SupervisorEvent>> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        if active.try_wait()?.is_none() {
            return Ok(None);
        }
        let failed_revision = self
            .active
            .take()
            .expect("active process was checked")
            .revision_id
            .clone();
        let restored_revision = self
            .rollback_revision
            .take()
            .ok_or(Error::NoRollbackRevision)?;
        self.store.set_current(&restored_revision)?;
        let restored = self.store.verify(&restored_revision)?;
        let candidate = launch_until_first_frame(&restored, self.first_frame_timeout)?;
        let event = SupervisorEvent::RolledBack {
            failed_revision,
            restored_revision: restored_revision.clone(),
            restored_pid: candidate.id(),
        };
        self.rollback_revision = Some(restored_revision);
        self.active = Some(candidate);
        Ok(Some(event))
    }

    pub fn active_revision(&self) -> Option<&str> {
        self.active
            .as_ref()
            .map(|candidate| candidate.revision_id.as_str())
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(active) = self.active.take() {
            active.terminate()?;
        }
        Ok(())
    }
}

impl Drop for RevisionSupervisor {
    fn drop(&mut self) {
        self.shutdown().ok();
    }
}
