use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use service_protocol::{
    EffectReceipt, FaultPoint, MigrationProof, NotesAction, NotesResource, PromotionDraft,
    ProviderAction, ServiceError, ServiceEvent, ServiceEventKind, StateResource, TransactionRecord,
    TransactionStatus, MAX_ACTIONS, MAX_EVENTS_PER_REQUEST, MAX_STATE_BYTES,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

const AUTHORITY_FORMAT_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum AuthorityError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("service error: {0:?}")]
    Service(ServiceError),
}

impl From<ServiceError> for AuthorityError {
    fn from(value: ServiceError) -> Self {
        Self::Service(value)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct AuthorityData {
    format_version: u32,
    current: StateResource,
    notes: NotesResource,
    transactions: BTreeMap<String, TransactionRecord>,
    events: Vec<ServiceEvent>,
    next_event_sequence: u64,
}

impl Default for AuthorityData {
    fn default() -> Self {
        Self {
            format_version: AUTHORITY_FORMAT_VERSION,
            current: StateResource::default(),
            notes: NotesResource::default(),
            transactions: BTreeMap::new(),
            events: Vec::new(),
            next_event_sequence: 1,
        }
    }
}

pub struct Authority {
    state_file: PathBuf,
    data: AuthorityData,
    fault: Option<FaultPoint>,
}

impl Authority {
    pub fn open(state_file: impl Into<PathBuf>) -> Result<Self, AuthorityError> {
        let state_file = state_file.into();
        let parent = state_file.parent().ok_or_else(|| {
            AuthorityError::Service(ServiceError::InvalidRequest {
                message: "authority state file must have a parent directory".into(),
            })
        })?;
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|error| io_context("create authority parent directory", error))?;
        }
        let data = if state_file.exists() {
            serde_json::from_slice(
                &fs::read(&state_file)
                    .map_err(|error| io_context("read authority state file", error))?,
            )?
        } else {
            AuthorityData::default()
        };
        validate_loaded(&data)?;
        let mut authority = Self {
            state_file,
            data,
            fault: None,
        };
        if !authority.state_file.exists() {
            authority.persist(&authority.data)?;
        }
        authority.recover_committing()?;
        Ok(authority)
    }

    pub fn current(&self) -> StateResource {
        self.data.current.clone()
    }

    pub fn notes(&self) -> NotesResource {
        self.data.notes.clone()
    }

    pub fn transaction(&self, transaction_id: &str) -> Result<TransactionRecord, AuthorityError> {
        self.data
            .transactions
            .get(transaction_id)
            .cloned()
            .ok_or_else(|| not_found(format!("unknown transaction: {transaction_id}")))
    }

    pub fn events(&self, after_sequence: u64, limit: usize) -> Vec<ServiceEvent> {
        self.data
            .events
            .iter()
            .filter(|event| event.sequence > after_sequence)
            .take(limit.min(MAX_EVENTS_PER_REQUEST))
            .cloned()
            .collect()
    }

    pub fn configure_fault(&mut self, point: Option<FaultPoint>) {
        self.fault = point;
    }

    pub fn reconcile(&mut self) -> Result<(), AuthorityError> {
        self.recover_committing()
    }

    pub fn stage(&mut self, draft: PromotionDraft) -> Result<TransactionRecord, AuthorityError> {
        self.inject(FaultPoint::BeforeStage)?;
        validate_draft(&draft, &self.data.current)?;
        if let Some(existing) = self.data.transactions.get(&draft.transaction_id) {
            return if existing.draft == draft && existing.status != TransactionStatus::Aborted {
                Ok(existing.clone())
            } else {
                Err(conflict(format!(
                    "transaction ID is already used: {}",
                    draft.transaction_id
                )))
            };
        }
        let transaction_id = draft.transaction_id.clone();
        let record = TransactionRecord {
            draft,
            status: TransactionStatus::Staged,
            committed_revision: None,
            effects: Vec::new(),
        };
        let mut next = self.data.clone();
        next.transactions
            .insert(transaction_id.clone(), record.clone());
        push_event(
            &mut next,
            ServiceEventKind::TransactionStaged {
                transaction_id,
                expected_revision: record.draft.expected_revision,
            },
        );
        self.replace_durably(next)?;
        self.inject(FaultPoint::AfterStage)?;
        Ok(record)
    }

    pub fn promote(&mut self, transaction_id: &str) -> Result<TransactionRecord, AuthorityError> {
        let record = self.transaction(transaction_id)?;
        match record.status {
            TransactionStatus::Committed => return Ok(record),
            TransactionStatus::Aborted => {
                return Err(conflict(format!(
                    "transaction is aborted: {transaction_id}"
                )))
            }
            TransactionStatus::Committing => return self.finalize(transaction_id),
            TransactionStatus::Staged => {}
        }
        if record.draft.expected_revision != self.data.current.revision {
            return Err(conflict(format!(
                "staged revision is stale: expected {}, current {}",
                record.draft.expected_revision, self.data.current.revision
            )));
        }
        self.inject(FaultPoint::BeforePromotion)?;

        let revision = self.data.current.revision.saturating_add(1);
        let mut committing = self.data.clone();
        committing.current = StateResource {
            revision,
            revision_id: record.draft.revision_id.clone(),
            schema_version: record.draft.schema_version,
            source_sha256: record.draft.source_sha256.clone(),
            state: record.draft.state.clone(),
        };
        let committing_record = committing
            .transactions
            .get_mut(transaction_id)
            .expect("staged transaction exists in clone");
        committing_record.status = TransactionStatus::Committing;
        committing_record.committed_revision = Some(revision);
        push_event(
            &mut committing,
            ServiceEventKind::RevisionCommitted {
                transaction_id: transaction_id.into(),
                revision,
                revision_id: record.draft.revision_id,
            },
        );
        self.replace_durably(committing)?;
        self.inject(FaultPoint::DuringPromotion)?;
        let completed = self.finalize(transaction_id)?;
        self.inject(FaultPoint::AfterPromotion)?;
        Ok(completed)
    }

    pub fn abort(&mut self, transaction_id: &str) -> Result<TransactionRecord, AuthorityError> {
        let existing = self.transaction(transaction_id)?;
        match existing.status {
            TransactionStatus::Aborted => return Ok(existing),
            TransactionStatus::Staged => {}
            TransactionStatus::Committing | TransactionStatus::Committed => {
                return Err(conflict(format!(
                    "cannot abort transaction in {:?} state",
                    existing.status
                )))
            }
        }
        let mut next = self.data.clone();
        let record = next
            .transactions
            .get_mut(transaction_id)
            .expect("transaction exists in clone");
        record.status = TransactionStatus::Aborted;
        let result = record.clone();
        push_event(
            &mut next,
            ServiceEventKind::TransactionAborted {
                transaction_id: transaction_id.into(),
            },
        );
        self.replace_durably(next)?;
        Ok(result)
    }

    fn recover_committing(&mut self) -> Result<(), AuthorityError> {
        let transactions = self
            .data
            .transactions
            .iter()
            .filter(|(_, record)| record.status == TransactionStatus::Committing)
            .map(|(transaction_id, _)| transaction_id.clone())
            .collect::<Vec<_>>();
        for transaction_id in transactions {
            self.finalize(&transaction_id)?;
        }
        Ok(())
    }

    fn finalize(&mut self, transaction_id: &str) -> Result<TransactionRecord, AuthorityError> {
        let existing = self.transaction(transaction_id)?;
        if existing.status == TransactionStatus::Committed {
            return Ok(existing);
        }
        if existing.status != TransactionStatus::Committing {
            return Err(conflict(format!(
                "transaction is not committing: {transaction_id}"
            )));
        }
        let revision = existing
            .committed_revision
            .expect("committing transaction has a revision");
        let mut next = self.data.clone();
        let mut receipts = Vec::with_capacity(existing.draft.actions.len());
        for (index, action) in existing.draft.actions.iter().enumerate() {
            let effect_id = format!("{transaction_id}:{index}");
            apply_action(&mut next.notes, action);
            let receipt = EffectReceipt {
                effect_id: effect_id.clone(),
                action: action.clone(),
            };
            receipts.push(receipt);
            push_event(
                &mut next,
                ServiceEventKind::ActionApplied {
                    transaction_id: transaction_id.into(),
                    effect_id,
                    action: action.clone(),
                },
            );
        }
        let record = next
            .transactions
            .get_mut(transaction_id)
            .expect("committing transaction exists in clone");
        record.effects = receipts;
        record.status = TransactionStatus::Committed;
        let result = record.clone();
        push_event(
            &mut next,
            ServiceEventKind::TransactionCompleted {
                transaction_id: transaction_id.into(),
                revision,
            },
        );
        self.replace_durably(next)?;
        Ok(result)
    }

    fn inject(&mut self, point: FaultPoint) -> Result<(), AuthorityError> {
        if self.fault == Some(point) {
            self.fault = None;
            Err(ServiceError::InjectedFault { point }.into())
        } else {
            Ok(())
        }
    }

    fn replace_durably(&mut self, next: AuthorityData) -> Result<(), AuthorityError> {
        self.persist(&next)?;
        self.data = next;
        Ok(())
    }

    fn persist(&self, data: &AuthorityData) -> Result<(), AuthorityError> {
        let parent = self
            .state_file
            .parent()
            .expect("validated authority state parent");
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".authority-{}-{sequence}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(data)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_context("create authority temporary file", error))?;
        let result = (|| -> Result<(), std::io::Error> {
            file.write_all(&bytes)
                .map_err(|error| io_context("write authority temporary file", error))?;
            file.sync_all()
                .map_err(|error| io_context("fsync authority temporary file", error))?;
            fs::rename(&temporary, &self.state_file)
                .map_err(|error| io_context("rename authority state file", error))?;
            File::open(parent)
                .map_err(|error| io_context("open authority parent directory", error))?
                .sync_all()
                .map_err(|error| io_context("fsync authority parent directory", error))
        })();
        if result.is_err() {
            fs::remove_file(&temporary).ok();
        }
        result.map_err(Into::into)
    }
}

fn io_context(operation: &str, error: std::io::Error) -> std::io::Error {
    std::io::Error::new(error.kind(), format!("{operation}: {error}"))
}

fn validate_draft(draft: &PromotionDraft, current: &StateResource) -> Result<(), AuthorityError> {
    if draft.transaction_id.is_empty()
        || draft.transaction_id.len() > 128
        || !draft
            .transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(invalid("invalid transaction ID"));
    }
    validate_sha256("revision ID", &draft.revision_id)?;
    validate_sha256("source SHA-256", &draft.source_sha256)?;
    if draft.schema_version == 0 {
        return Err(invalid("schema version must be positive"));
    }
    if draft.expected_revision != current.revision {
        return Err(conflict(format!(
            "revision conflict: expected {}, current {}",
            draft.expected_revision, current.revision
        )));
    }
    if serde_json::to_vec(&draft.state)?.len() > MAX_STATE_BYTES {
        return Err(invalid("state exceeds the service limit"));
    }
    if draft.actions.len() > MAX_ACTIONS {
        return Err(invalid("too many provider actions"));
    }
    for action in &draft.actions {
        validate_action(action)?;
    }
    validate_migration(draft, current)
}

fn validate_migration(
    draft: &PromotionDraft,
    current: &StateResource,
) -> Result<(), AuthorityError> {
    if draft.schema_version == current.schema_version {
        return if draft.migration.is_none() {
            Ok(())
        } else {
            Err(invalid_migration(
                "migration proof supplied without a schema change",
            ))
        };
    }
    if draft.schema_version < current.schema_version {
        return Err(invalid_migration("schema versions cannot move backwards"));
    }
    let proof = draft
        .migration
        .as_ref()
        .ok_or_else(|| invalid_migration("schema change requires a migration proof"))?;
    let expected = MigrationProof {
        from_schema_version: current.schema_version,
        to_schema_version: draft.schema_version,
        from_state_sha256: state_sha256(&current.state)?,
    };
    if proof != &expected {
        return Err(invalid_migration(
            "migration proof does not bind the current state and schemas",
        ));
    }
    Ok(())
}

fn validate_action(action: &ProviderAction) -> Result<(), AuthorityError> {
    match action {
        ProviderAction::Notes(NotesAction::AttachToEvent {
            note_id,
            event_title,
        }) if !note_id.is_empty()
            && note_id.len() <= 256
            && !event_title.is_empty()
            && event_title.len() <= 1024 =>
        {
            Ok(())
        }
        ProviderAction::Notes(_) => Err(invalid("invalid notes action fields")),
    }
}

fn apply_action(notes: &mut NotesResource, action: &ProviderAction) {
    match action {
        ProviderAction::Notes(NotesAction::AttachToEvent {
            note_id,
            event_title,
        }) => {
            notes
                .attachments
                .insert(note_id.clone(), event_title.clone());
        }
    }
}

fn push_event(data: &mut AuthorityData, kind: ServiceEventKind) {
    let sequence = data.next_event_sequence;
    data.next_event_sequence = sequence.saturating_add(1);
    data.events.push(ServiceEvent { sequence, kind });
}

fn validate_loaded(data: &AuthorityData) -> Result<(), AuthorityError> {
    if data.format_version != AUTHORITY_FORMAT_VERSION
        || data.current.schema_version == 0
        || data.next_event_sequence == 0
    {
        return Err(invalid("invalid authority file header"));
    }
    if !data.current.revision_id.is_empty() {
        validate_sha256("current revision ID", &data.current.revision_id)?;
    }
    if !data.current.source_sha256.is_empty() {
        validate_sha256("current source SHA-256", &data.current.source_sha256)?;
    }
    if data
        .transactions
        .iter()
        .any(|(transaction_id, record)| transaction_id != &record.draft.transaction_id)
    {
        return Err(invalid("transaction index mismatch"));
    }
    if data
        .events
        .windows(2)
        .any(|events| events[0].sequence >= events[1].sequence)
    {
        return Err(invalid("event sequence is not monotonic"));
    }
    if data
        .events
        .last()
        .is_some_and(|event| event.sequence >= data.next_event_sequence)
    {
        return Err(invalid("next event sequence is stale"));
    }
    Ok(())
}

fn validate_sha256(name: &str, value: &str) -> Result<(), AuthorityError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid(format!("{name} must be 64 hexadecimal characters")))
    }
}

pub fn state_sha256(state: &serde_json::Value) -> Result<String, AuthorityError> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(state)?)))
}

fn invalid(message: impl Into<String>) -> AuthorityError {
    ServiceError::InvalidRequest {
        message: message.into(),
    }
    .into()
}

fn invalid_migration(message: impl Into<String>) -> AuthorityError {
    ServiceError::InvalidMigration {
        message: message.into(),
    }
    .into()
}

fn conflict(message: impl Into<String>) -> AuthorityError {
    ServiceError::Conflict {
        message: message.into(),
    }
    .into()
}

fn not_found(message: impl Into<String>) -> AuthorityError {
    ServiceError::NotFound {
        message: message.into(),
    }
    .into()
}
