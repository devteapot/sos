use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use provider_state_service::ServiceClient;
use serde::{Deserialize, Serialize};
use service_protocol::{
    ResourceQuery, ResourceValue, ResponsePayload, ServiceError, ServiceRequest, StateResource,
    TransactionRecord, TransactionStatus,
};
use thiserror::Error;

use crate::{DurableState, RevisionStore, RevisionSupervisor, SupervisorEvent};

const JOURNAL_FORMAT_VERSION: u32 = 1;
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalPhase {
    Intent,
    ServiceCommitted,
    PointerCommitted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ActivationJournal {
    pub format_version: u32,
    pub transaction_id: String,
    pub previous_revision: String,
    pub candidate_revision: String,
    pub phase: JournalPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorFaultPoint {
    AfterIntent,
    AfterServiceCommit,
    AfterPointerCommit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordinationEvent {
    Activated {
        transaction_id: String,
        revision_id: String,
        host_pid: u32,
    },
    RecoveredPrevious {
        transaction_id: String,
        revision_id: String,
    },
    RecoveredCandidate {
        transaction_id: String,
        revision_id: String,
    },
}

#[derive(Debug, Error)]
pub enum CoordinationError {
    #[error("revision supervisor error: {0}")]
    Supervisor(#[from] crate::Error),
    #[error("journal I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("journal JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("service communication failed: {0}")]
    ServiceCommunication(std::io::Error),
    #[error("service rejected coordination: {0:?}")]
    Service(ServiceError),
    #[error("service returned the wrong response payload")]
    UnexpectedServiceResponse,
    #[error("revision/service binding is inconsistent: {0}")]
    InvalidBinding(String),
    #[error("injected coordinator fault: {0:?}")]
    InjectedFault(CoordinatorFaultPoint),
}

pub struct CoordinatedSupervisor {
    supervisor: RevisionSupervisor,
    store: RevisionStore,
    service: ServiceClient,
    journal_file: PathBuf,
    fault: Option<CoordinatorFaultPoint>,
}

impl CoordinatedSupervisor {
    pub fn new(
        store: RevisionStore,
        supervisor: RevisionSupervisor,
        service: ServiceClient,
    ) -> Self {
        let journal_file = store.root().join("activation-journal.json");
        Self {
            supervisor,
            store,
            service,
            journal_file,
            fault: None,
        }
    }

    pub fn boot(&mut self) -> Result<Option<SupervisorEvent>, CoordinationError> {
        let event = self.supervisor.boot()?;
        self.recover()?;
        Ok(event)
    }

    pub fn configure_fault(&mut self, point: Option<CoordinatorFaultPoint>) {
        self.fault = point;
    }

    pub fn activate(
        &mut self,
        transaction_id: &str,
        revision_id: &str,
    ) -> Result<CoordinationEvent, CoordinationError> {
        if self.load_journal()?.is_some() {
            self.recover()?;
        }
        let previous_revision = self
            .supervisor
            .current_revision()?
            .ok_or(crate::Error::NoCurrentRevision)?;
        let current_state = self.state_resource()?;
        if current_state.revision_id != previous_revision {
            return Err(CoordinationError::InvalidBinding(format!(
                "current pointer {} differs from service revision {}",
                previous_revision, current_state.revision_id
            )));
        }
        let transaction = self.transaction(transaction_id)?;
        self.validate_binding(&transaction, revision_id)?;
        if !matches!(
            transaction.status,
            TransactionStatus::Staged | TransactionStatus::Committed
        ) {
            return Err(CoordinationError::InvalidBinding(format!(
                "transaction is not promotable: {:?}",
                transaction.status
            )));
        }
        let mut journal = ActivationJournal {
            format_version: JOURNAL_FORMAT_VERSION,
            transaction_id: transaction_id.into(),
            previous_revision,
            candidate_revision: revision_id.into(),
            phase: JournalPhase::Intent,
        };
        self.write_journal(&journal)?;
        self.inject(CoordinatorFaultPoint::AfterIntent)?;

        let mut prepared = match self.supervisor.prepare(revision_id) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.call(ServiceRequest::Abort {
                    request_id: next_request_id(),
                    transaction_id: transaction_id.into(),
                })?;
                self.clear_journal()?;
                return Err(error.into());
            }
        };
        if let Err(error) = self.supervisor.quiesce_prepared(&mut prepared) {
            self.call(ServiceRequest::Abort {
                request_id: next_request_id(),
                transaction_id: transaction_id.into(),
            })?;
            self.supervisor.discard_prepared(prepared).ok();
            self.clear_journal()?;
            return Err(error.into());
        }
        let committed = if transaction.status == TransactionStatus::Committed {
            transaction
        } else {
            let response = self.call(ServiceRequest::Promote {
                request_id: next_request_id(),
                transaction_id: transaction_id.into(),
            });
            match response {
                Ok(_) => {}
                Err(CoordinationError::Service(_)) => {}
                Err(error) => return Err(error),
            }
            self.transaction(transaction_id)?
        };
        if committed.status != TransactionStatus::Committed {
            if committed.status == TransactionStatus::Staged {
                self.call(ServiceRequest::Abort {
                    request_id: next_request_id(),
                    transaction_id: transaction_id.into(),
                })?;
            }
            self.supervisor.discard_prepared(prepared)?;
            self.clear_journal()?;
            return Err(CoordinationError::InvalidBinding(format!(
                "service transaction did not commit: {:?}",
                committed.status
            )));
        }
        journal.phase = JournalPhase::ServiceCommitted;
        self.write_journal(&journal)?;
        self.inject(CoordinatorFaultPoint::AfterServiceCommit)?;

        let supervisor_event = self.supervisor.commit_prepared(prepared)?;
        journal.phase = JournalPhase::PointerCommitted;
        self.write_journal(&journal)?;
        self.inject(CoordinatorFaultPoint::AfterPointerCommit)?;
        self.clear_journal()?;
        let host_pid = match supervisor_event {
            SupervisorEvent::Activated { host_pid, .. } => host_pid,
            _ => unreachable!("prepared commit always activates"),
        };
        Ok(CoordinationEvent::Activated {
            transaction_id: transaction_id.into(),
            revision_id: revision_id.into(),
            host_pid,
        })
    }

    pub fn recover(&mut self) -> Result<Option<CoordinationEvent>, CoordinationError> {
        let Some(journal) = self.load_journal()? else {
            return Ok(None);
        };
        self.validate_journal(&journal)?;
        let transaction = self.transaction(&journal.transaction_id)?;
        self.validate_binding(&transaction, &journal.candidate_revision)?;
        let current = self
            .supervisor
            .current_revision()?
            .ok_or(crate::Error::NoCurrentRevision)?;
        match transaction.status {
            TransactionStatus::Committed => {
                if current != journal.candidate_revision {
                    self.supervisor.activate(&journal.candidate_revision)?;
                }
                self.clear_journal()?;
                Ok(Some(CoordinationEvent::RecoveredCandidate {
                    transaction_id: journal.transaction_id,
                    revision_id: journal.candidate_revision,
                }))
            }
            TransactionStatus::Staged | TransactionStatus::Aborted => {
                if current != journal.previous_revision {
                    self.supervisor.activate(&journal.previous_revision)?;
                }
                if transaction.status == TransactionStatus::Staged {
                    self.call(ServiceRequest::Abort {
                        request_id: next_request_id(),
                        transaction_id: journal.transaction_id.clone(),
                    })?;
                }
                self.clear_journal()?;
                Ok(Some(CoordinationEvent::RecoveredPrevious {
                    transaction_id: journal.transaction_id,
                    revision_id: journal.previous_revision,
                }))
            }
            TransactionStatus::Committing => Err(CoordinationError::InvalidBinding(
                "service exposed an unreconciled committing transaction".into(),
            )),
        }
    }

    pub fn poll(&mut self) -> Result<Option<SupervisorEvent>, CoordinationError> {
        self.supervisor.poll().map_err(Into::into)
    }

    pub fn active_revision(&self) -> Option<&str> {
        self.supervisor.active_revision()
    }

    pub fn host_pid(&self) -> Option<u32> {
        self.supervisor.host_pid()
    }

    pub fn journal(&self) -> Result<Option<ActivationJournal>, CoordinationError> {
        self.load_journal()
    }

    pub fn shutdown(&mut self) -> Result<(), CoordinationError> {
        self.supervisor.shutdown().map_err(Into::into)
    }

    pub fn restart_host(&mut self) -> Result<SupervisorEvent, CoordinationError> {
        self.supervisor.restart_host().map_err(Into::into)
    }

    fn validate_binding(
        &self,
        transaction: &TransactionRecord,
        revision_id: &str,
    ) -> Result<(), CoordinationError> {
        if transaction.draft.revision_id != revision_id {
            return Err(CoordinationError::InvalidBinding(
                "transaction revision ID differs from candidate".into(),
            ));
        }
        let revision = self.store.verify(revision_id)?;
        let state: DurableState = serde_json::from_slice(&fs::read(
            revision.directory.join(&revision.manifest.state.path),
        )?)?;
        if state.schema_version != transaction.draft.schema_version
            || state.source_sha256 != transaction.draft.source_sha256
            || state.state != transaction.draft.state
        {
            return Err(CoordinationError::InvalidBinding(
                "immutable revision state/source/schema differs from service transaction".into(),
            ));
        }
        Ok(())
    }

    fn state_resource(&self) -> Result<StateResource, CoordinationError> {
        match self.call(ServiceRequest::GetResource {
            request_id: next_request_id(),
            query: ResourceQuery::ExperienceState,
        })? {
            ResponsePayload::Resource {
                value: ResourceValue::ExperienceState(state),
            } => Ok(state),
            _ => Err(CoordinationError::UnexpectedServiceResponse),
        }
    }

    fn transaction(&self, transaction_id: &str) -> Result<TransactionRecord, CoordinationError> {
        match self.call(ServiceRequest::GetTransaction {
            request_id: next_request_id(),
            transaction_id: transaction_id.into(),
        })? {
            ResponsePayload::Transaction { record } => Ok(record),
            _ => Err(CoordinationError::UnexpectedServiceResponse),
        }
    }

    fn call(&self, request: ServiceRequest) -> Result<ResponsePayload, CoordinationError> {
        let response = self
            .service
            .call(&request)
            .map_err(CoordinationError::ServiceCommunication)?;
        if response.ok {
            response
                .payload
                .ok_or(CoordinationError::UnexpectedServiceResponse)
        } else {
            Err(CoordinationError::Service(response.error.unwrap_or(
                ServiceError::Internal {
                    message: "service returned neither payload nor error".into(),
                },
            )))
        }
    }

    fn inject(&mut self, point: CoordinatorFaultPoint) -> Result<(), CoordinationError> {
        if self.fault == Some(point) {
            self.fault = None;
            Err(CoordinationError::InjectedFault(point))
        } else {
            Ok(())
        }
    }

    fn validate_journal(&self, journal: &ActivationJournal) -> Result<(), CoordinationError> {
        if journal.format_version != JOURNAL_FORMAT_VERSION
            || !is_sha256(&journal.previous_revision)
            || !is_sha256(&journal.candidate_revision)
            || journal.transaction_id.is_empty()
        {
            return Err(CoordinationError::InvalidBinding(
                "invalid activation journal".into(),
            ));
        }
        Ok(())
    }

    fn load_journal(&self) -> Result<Option<ActivationJournal>, CoordinationError> {
        match fs::read(&self.journal_file) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_journal(&self, journal: &ActivationJournal) -> Result<(), CoordinationError> {
        let sequence = FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = self.store.root().join(format!(
            ".activation-journal-{}-{sequence}.tmp",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> Result<(), std::io::Error> {
            file.write_all(&serde_json::to_vec_pretty(journal).map_err(std::io::Error::other)?)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.journal_file)?;
            sync_directory(self.store.root())
        })();
        if result.is_err() {
            fs::remove_file(&temporary).ok();
        }
        result.map_err(Into::into)
    }

    fn clear_journal(&self) -> Result<(), CoordinationError> {
        match fs::remove_file(&self.journal_file) {
            Ok(()) => sync_directory(self.store.root()).map_err(Into::into),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn next_request_id() -> u64 {
    REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}
