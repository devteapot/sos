use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use service_protocol::{
    AppearanceResource, EffectReceipt, ExperiencePromotionDraft, FaultPoint, GrantDecisionResource,
    GraphEffectReceipt, GraphPromotionDraft, GraphTransactionRecord, MigrationProof, NotesAction,
    NotesResource, PromotionDraft, ProviderAction, ServiceError, ServiceEvent, ServiceEventKind,
    StateResource, TransactionRecord, TransactionStatus, MAX_ACTIONS, MAX_EVENTS_PER_REQUEST,
    MAX_GRAPH_PROMOTIONS, MAX_STATE_BYTES,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

const LEGACY_AUTHORITY_FORMAT_VERSION: u32 = 1;
const EXPERIENCE_AUTHORITY_FORMAT_VERSION: u32 = 2;
const AUTHORITY_FORMAT_VERSION: u32 = 3;
const STOCK_SHELL_EXPERIENCE_ID: &str = "sos.stock.shell";
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
    #[serde(default)]
    experiences: BTreeMap<String, StateResource>,
    #[serde(default)]
    experience_revisions: BTreeMap<String, BTreeMap<String, StateResource>>,
    #[serde(default)]
    appearance: AppearanceResource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    appearance_writer_sha256: Option<String>,
    #[serde(default)]
    grant_decisions: BTreeMap<String, GrantDecisionResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    grant_writer_sha256: Option<String>,
    notes: NotesResource,
    transactions: BTreeMap<String, TransactionRecord>,
    #[serde(default)]
    graph_transactions: BTreeMap<String, GraphTransactionRecord>,
    events: Vec<ServiceEvent>,
    next_event_sequence: u64,
}

impl Default for AuthorityData {
    fn default() -> Self {
        Self {
            format_version: AUTHORITY_FORMAT_VERSION,
            current: StateResource::default(),
            experiences: BTreeMap::from([(
                STOCK_SHELL_EXPERIENCE_ID.into(),
                StateResource::default(),
            )]),
            experience_revisions: BTreeMap::new(),
            appearance: AppearanceResource::default(),
            appearance_writer_sha256: None,
            grant_decisions: BTreeMap::new(),
            grant_writer_sha256: None,
            notes: NotesResource::default(),
            transactions: BTreeMap::new(),
            graph_transactions: BTreeMap::new(),
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
        let mut data = if state_file.exists() {
            serde_json::from_slice(
                &fs::read(&state_file)
                    .map_err(|error| io_context("read authority state file", error))?,
            )?
        } else {
            AuthorityData::default()
        };
        let migrated = migrate_loaded(&mut data)?;
        validate_loaded(&data)?;
        let mut authority = Self {
            state_file,
            data,
            fault: None,
        };
        if !authority.state_file.exists() || migrated {
            authority.persist(&authority.data)?;
        }
        authority.recover_committing()?;
        Ok(authority)
    }

    pub fn current(&self) -> StateResource {
        self.data.current.clone()
    }

    pub fn current_for(&self, experience_id: &str) -> StateResource {
        self.data
            .experiences
            .get(experience_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn current_at(&self, experience_id: &str, revision_id: &str) -> StateResource {
        self.data
            .experience_revisions
            .get(experience_id)
            .and_then(|revisions| revisions.get(revision_id))
            .cloned()
            .or_else(|| {
                self.data
                    .experiences
                    .get(experience_id)
                    .filter(|state| state.revision_id == revision_id)
                    .cloned()
            })
            .unwrap_or_default()
    }

    pub fn appearance(&self) -> AppearanceResource {
        self.data.appearance.clone()
    }

    pub fn grant_decision_for(&self, experience_id: &str) -> Option<GrantDecisionResource> {
        self.data.grant_decisions.get(experience_id).cloned()
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

    pub fn graph_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<GraphTransactionRecord, AuthorityError> {
        self.data
            .graph_transactions
            .get(transaction_id)
            .cloned()
            .ok_or_else(|| not_found(format!("unknown graph transaction: {transaction_id}")))
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
        self.stage_inner(None, draft)
    }

    pub fn stage_experience(
        &mut self,
        request: ExperiencePromotionDraft,
    ) -> Result<TransactionRecord, AuthorityError> {
        experience_package::ExperienceId::parse(request.experience_id.as_str())
            .map_err(|error| invalid(error.to_string()))?;
        self.stage_inner(Some(request.experience_id), request.draft)
    }

    pub fn stage_graph(
        &mut self,
        draft: GraphPromotionDraft,
    ) -> Result<GraphTransactionRecord, AuthorityError> {
        self.inject(FaultPoint::BeforeStage)?;
        validate_transaction_id(&draft.transaction_id)?;
        if draft.promotions.is_empty() || draft.promotions.len() > MAX_GRAPH_PROMOTIONS {
            return Err(invalid(format!(
                "graph transaction must contain 1 to {MAX_GRAPH_PROMOTIONS} promotions"
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        for promotion in &draft.promotions {
            if !seen.insert(promotion.experience_id.clone()) {
                return Err(invalid(format!(
                    "duplicate experience in graph transaction: {}",
                    promotion.experience_id
                )));
            }
            let promotion_draft = promotion.as_promotion(&draft.transaction_id);
            let current = if draft.activate {
                initial_state_for(
                    &promotion_draft,
                    self.data.experiences.get(promotion.experience_id.as_str()),
                )
            } else {
                revision_state_for(
                    &self.data,
                    promotion.experience_id.as_str(),
                    &promotion.revision_id,
                    &promotion_draft,
                )
            };
            validate_draft(&promotion_draft, &current)?;
        }
        if self.data.transactions.contains_key(&draft.transaction_id) {
            return Err(conflict(format!(
                "transaction ID is already used: {}",
                draft.transaction_id
            )));
        }
        if let Some(existing) = self.data.graph_transactions.get(&draft.transaction_id) {
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
        let record = GraphTransactionRecord {
            draft,
            status: TransactionStatus::Staged,
            committed_revisions: BTreeMap::new(),
            effects: Vec::new(),
        };
        let mut next = self.data.clone();
        next.graph_transactions
            .insert(transaction_id.clone(), record.clone());
        push_event(
            &mut next,
            ServiceEventKind::GraphTransactionStaged {
                transaction_id,
                experience_count: record.draft.promotions.len(),
            },
        );
        self.replace_durably(next)?;
        self.inject(FaultPoint::AfterStage)?;
        Ok(record)
    }

    fn stage_inner(
        &mut self,
        experience_id: Option<experience_package::ExperienceId>,
        draft: PromotionDraft,
    ) -> Result<TransactionRecord, AuthorityError> {
        self.inject(FaultPoint::BeforeStage)?;
        validate_transaction_id(&draft.transaction_id)?;
        let current = experience_id
            .as_ref()
            .map(|id| initial_state_for(&draft, self.data.experiences.get(id.as_str())))
            .unwrap_or_else(|| self.data.current.clone());
        validate_draft(&draft, &current)?;
        if let Some(existing) = self.data.transactions.get(&draft.transaction_id) {
            return if existing.draft == draft
                && existing.experience_id == experience_id
                && existing.status != TransactionStatus::Aborted
            {
                Ok(existing.clone())
            } else {
                Err(conflict(format!(
                    "transaction ID is already used: {}",
                    draft.transaction_id
                )))
            };
        }
        if self
            .data
            .graph_transactions
            .contains_key(&draft.transaction_id)
        {
            return Err(conflict(format!(
                "transaction ID is already used: {}",
                draft.transaction_id
            )));
        }
        let transaction_id = draft.transaction_id.clone();
        let record = TransactionRecord {
            experience_id,
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

    pub fn update_appearance(
        &mut self,
        expected_generation: u64,
        capability: &str,
        profile: experience_package::AppearanceProfile,
    ) -> Result<AppearanceResource, AuthorityError> {
        if capability.is_empty()
            || capability.len() > 256
            || self.data.appearance_writer_sha256.as_deref()
                != Some(format!("{:x}", Sha256::digest(capability.as_bytes())).as_str())
        {
            return Err(ServiceError::Denied {
                message: "appearance-write capability denied".into(),
            }
            .into());
        }
        profile
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        if self.data.appearance.profile.generation != expected_generation {
            return Err(conflict(format!(
                "appearance generation conflict: expected {expected_generation}, current {}",
                self.data.appearance.profile.generation
            )));
        }
        if profile.generation != expected_generation.saturating_add(1) {
            return Err(invalid(
                "new appearance generation must follow the expected generation",
            ));
        }
        let mut next = self.data.clone();
        next.appearance = AppearanceResource { profile };
        let result = next.appearance.clone();
        self.replace_durably(next)?;
        Ok(result)
    }

    pub fn configure_appearance_writer(&mut self, capability: &str) -> Result<(), AuthorityError> {
        if capability.is_empty() || capability.len() > 256 {
            return Err(invalid(
                "appearance-write capability must contain 1 to 256 bytes",
            ));
        }
        let digest = format!("{:x}", Sha256::digest(capability.as_bytes()));
        match self.data.appearance_writer_sha256.as_deref() {
            Some(current) if current == digest => Ok(()),
            Some(_) => Err(conflict(
                "appearance-write capability does not match authority",
            )),
            None => {
                let mut next = self.data.clone();
                next.appearance_writer_sha256 = Some(digest);
                self.replace_durably(next)
            }
        }
    }

    pub fn update_grant_decision(
        &mut self,
        expected_generation: u64,
        capability: &str,
        decision: GrantDecisionResource,
    ) -> Result<GrantDecisionResource, AuthorityError> {
        self.require_capability(
            capability,
            self.data.grant_writer_sha256.as_deref(),
            "grant-review",
        )?;
        validate_grant_decision(&decision)?;
        let current_generation = self
            .data
            .grant_decisions
            .get(decision.experience_id.as_str())
            .map_or(0, |current| current.generation);
        if current_generation != expected_generation {
            return Err(conflict(format!(
                "grant decision generation conflict: expected {expected_generation}, current {current_generation}"
            )));
        }
        if decision.generation != expected_generation.saturating_add(1) {
            return Err(invalid(
                "new grant decision generation must follow the expected generation",
            ));
        }
        let mut next = self.data.clone();
        next.grant_decisions
            .insert(decision.experience_id.to_string(), decision.clone());
        self.replace_durably(next)?;
        Ok(decision)
    }

    pub fn configure_grant_writer(&mut self, capability: &str) -> Result<(), AuthorityError> {
        if capability.is_empty() || capability.len() > 256 {
            return Err(invalid(
                "grant-review capability must contain 1 to 256 bytes",
            ));
        }
        let digest = format!("{:x}", Sha256::digest(capability.as_bytes()));
        match self.data.grant_writer_sha256.as_deref() {
            Some(current) if current == digest => Ok(()),
            Some(_) => Err(conflict("grant-review capability does not match authority")),
            None => {
                let mut next = self.data.clone();
                next.grant_writer_sha256 = Some(digest);
                self.replace_durably(next)
            }
        }
    }

    fn require_capability(
        &self,
        capability: &str,
        expected_sha256: Option<&str>,
        name: &str,
    ) -> Result<(), AuthorityError> {
        if capability.is_empty()
            || capability.len() > 256
            || expected_sha256
                != Some(format!("{:x}", Sha256::digest(capability.as_bytes())).as_str())
        {
            return Err(ServiceError::Denied {
                message: format!("{name} capability denied"),
            }
            .into());
        }
        Ok(())
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
        let current = record
            .experience_id
            .as_ref()
            .map(|id| initial_state_for(&record.draft, self.data.experiences.get(id.as_str())))
            .unwrap_or_else(|| self.data.current.clone());
        if record.draft.expected_revision != current.revision {
            return Err(conflict(format!(
                "staged revision is stale: expected {}, current {}",
                record.draft.expected_revision, current.revision
            )));
        }
        self.inject(FaultPoint::BeforePromotion)?;

        let revision = current.revision.saturating_add(1);
        let mut committing = self.data.clone();
        let next_state = StateResource {
            revision,
            revision_id: record.draft.revision_id.clone(),
            schema_version: record.draft.schema_version,
            source_sha256: record.draft.source_sha256.clone(),
            state: record.draft.state.clone(),
        };
        if let Some(experience_id) = &record.experience_id {
            committing
                .experiences
                .insert(experience_id.to_string(), next_state.clone());
            committing
                .experience_revisions
                .entry(experience_id.to_string())
                .or_default()
                .insert(record.draft.revision_id.clone(), next_state);
        } else {
            committing.current = next_state.clone();
            committing
                .experiences
                .insert(STOCK_SHELL_EXPERIENCE_ID.into(), next_state.clone());
            committing
                .experience_revisions
                .entry(STOCK_SHELL_EXPERIENCE_ID.into())
                .or_default()
                .insert(record.draft.revision_id.clone(), next_state);
        }
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

    pub fn promote_graph(
        &mut self,
        transaction_id: &str,
    ) -> Result<GraphTransactionRecord, AuthorityError> {
        let record = self.graph_transaction(transaction_id)?;
        match record.status {
            TransactionStatus::Committed => return Ok(record),
            TransactionStatus::Aborted => {
                return Err(conflict(format!(
                    "graph transaction is aborted: {transaction_id}"
                )))
            }
            TransactionStatus::Committing => return self.finalize_graph(transaction_id),
            TransactionStatus::Staged => {}
        }
        for promotion in &record.draft.promotions {
            let promotion_draft = promotion.as_promotion(transaction_id);
            let current = if record.draft.activate {
                initial_state_for(
                    &promotion_draft,
                    self.data.experiences.get(promotion.experience_id.as_str()),
                )
            } else {
                revision_state_for(
                    &self.data,
                    promotion.experience_id.as_str(),
                    &promotion.revision_id,
                    &promotion_draft,
                )
            };
            if promotion.expected_revision != current.revision {
                return Err(conflict(format!(
                    "staged experience `{}` is stale: expected {}, current {}",
                    promotion.experience_id, promotion.expected_revision, current.revision
                )));
            }
        }
        self.inject(FaultPoint::BeforePromotion)?;

        let mut committing = self.data.clone();
        let mut committed_revisions = BTreeMap::new();
        for promotion in &record.draft.promotions {
            let promotion_draft = promotion.as_promotion(transaction_id);
            let current = if record.draft.activate {
                initial_state_for(
                    &promotion_draft,
                    committing.experiences.get(promotion.experience_id.as_str()),
                )
            } else {
                revision_state_for(
                    &committing,
                    promotion.experience_id.as_str(),
                    &promotion.revision_id,
                    &promotion_draft,
                )
            };
            let revision = current.revision.saturating_add(1);
            let next_state = StateResource {
                revision,
                revision_id: promotion.revision_id.clone(),
                schema_version: promotion.schema_version,
                source_sha256: promotion.source_sha256.clone(),
                state: promotion.state.clone(),
            };
            committing
                .experience_revisions
                .entry(promotion.experience_id.to_string())
                .or_default()
                .insert(promotion.revision_id.clone(), next_state.clone());
            let is_current = record.draft.activate
                || committing
                    .experiences
                    .get(promotion.experience_id.as_str())
                    .is_none_or(|state| {
                        state.revision_id.is_empty() || state.revision_id == promotion.revision_id
                    });
            if is_current {
                committing
                    .experiences
                    .insert(promotion.experience_id.to_string(), next_state.clone());
            }
            if is_current && promotion.experience_id.as_str() == STOCK_SHELL_EXPERIENCE_ID {
                committing.current = next_state;
            }
            committed_revisions.insert(promotion.experience_id.clone(), revision);
        }
        let committing_record = committing
            .graph_transactions
            .get_mut(transaction_id)
            .expect("staged graph transaction exists in clone");
        committing_record.status = TransactionStatus::Committing;
        committing_record.committed_revisions = committed_revisions.clone();
        push_event(
            &mut committing,
            ServiceEventKind::GraphRevisionsCommitted {
                transaction_id: transaction_id.into(),
                revisions: committed_revisions,
            },
        );
        self.replace_durably(committing)?;
        self.inject(FaultPoint::DuringPromotion)?;
        let completed = self.finalize_graph(transaction_id)?;
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

    pub fn abort_graph(
        &mut self,
        transaction_id: &str,
    ) -> Result<GraphTransactionRecord, AuthorityError> {
        let existing = self.graph_transaction(transaction_id)?;
        match existing.status {
            TransactionStatus::Aborted => return Ok(existing),
            TransactionStatus::Staged => {}
            TransactionStatus::Committing | TransactionStatus::Committed => {
                return Err(conflict(format!(
                    "cannot abort graph transaction in {:?} state",
                    existing.status
                )))
            }
        }
        let mut next = self.data.clone();
        let record = next
            .graph_transactions
            .get_mut(transaction_id)
            .expect("graph transaction exists in clone");
        record.status = TransactionStatus::Aborted;
        let result = record.clone();
        push_event(
            &mut next,
            ServiceEventKind::GraphTransactionAborted {
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
        let graph_transactions = self
            .data
            .graph_transactions
            .iter()
            .filter(|(_, record)| record.status == TransactionStatus::Committing)
            .map(|(transaction_id, _)| transaction_id.clone())
            .collect::<Vec<_>>();
        for transaction_id in graph_transactions {
            self.finalize_graph(&transaction_id)?;
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

    fn finalize_graph(
        &mut self,
        transaction_id: &str,
    ) -> Result<GraphTransactionRecord, AuthorityError> {
        let existing = self.graph_transaction(transaction_id)?;
        if existing.status == TransactionStatus::Committed {
            return Ok(existing);
        }
        if existing.status != TransactionStatus::Committing {
            return Err(conflict(format!(
                "graph transaction is not committing: {transaction_id}"
            )));
        }
        let mut next = self.data.clone();
        let mut receipts = Vec::new();
        for promotion in &existing.draft.promotions {
            for (index, action) in promotion.actions.iter().enumerate() {
                let effect_id = format!("{transaction_id}:{}:{index}", promotion.experience_id);
                apply_action(&mut next.notes, action);
                receipts.push(GraphEffectReceipt {
                    experience_id: promotion.experience_id.clone(),
                    effect_id: effect_id.clone(),
                    action: action.clone(),
                });
                push_event(
                    &mut next,
                    ServiceEventKind::GraphActionApplied {
                        transaction_id: transaction_id.into(),
                        experience_id: promotion.experience_id.clone(),
                        effect_id,
                        action: action.clone(),
                    },
                );
            }
        }
        let record = next
            .graph_transactions
            .get_mut(transaction_id)
            .expect("committing graph transaction exists in clone");
        record.effects = receipts;
        record.status = TransactionStatus::Committed;
        let result = record.clone();
        push_event(
            &mut next,
            ServiceEventKind::GraphTransactionCompleted {
                transaction_id: transaction_id.into(),
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
    validate_transaction_id(&draft.transaction_id)?;
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

fn validate_transaction_id(transaction_id: &str) -> Result<(), AuthorityError> {
    if transaction_id.is_empty()
        || transaction_id.len() > 128
        || !transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        Err(invalid("invalid transaction ID"))
    } else {
        Ok(())
    }
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
    data.appearance
        .profile
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    if let Some(digest) = &data.appearance_writer_sha256 {
        validate_sha256("appearance writer capability", digest)?;
    }
    if let Some(digest) = &data.grant_writer_sha256 {
        validate_sha256("grant writer capability", digest)?;
    }
    for (experience_id, decision) in &data.grant_decisions {
        if experience_id != decision.experience_id.as_str() {
            return Err(invalid("grant decision index mismatch"));
        }
        validate_grant_decision(decision)?;
    }
    for (experience_id, state) in &data.experiences {
        experience_package::ExperienceId::parse(experience_id)
            .map_err(|error| invalid(error.to_string()))?;
        validate_state_resource(state)?;
    }
    for (experience_id, revisions) in &data.experience_revisions {
        experience_package::ExperienceId::parse(experience_id)
            .map_err(|error| invalid(error.to_string()))?;
        for (revision_id, state) in revisions {
            validate_sha256("revision-state key", revision_id)?;
            if state.revision_id != *revision_id {
                return Err(invalid("revision-state index mismatch"));
            }
            validate_state_resource(state)?;
        }
    }
    if data
        .transactions
        .iter()
        .any(|(transaction_id, record)| transaction_id != &record.draft.transaction_id)
    {
        return Err(invalid("transaction index mismatch"));
    }
    if data
        .graph_transactions
        .iter()
        .any(|(transaction_id, record)| transaction_id != &record.draft.transaction_id)
    {
        return Err(invalid("graph transaction index mismatch"));
    }
    if data
        .transactions
        .keys()
        .any(|transaction_id| data.graph_transactions.contains_key(transaction_id))
    {
        return Err(invalid("transaction ID is shared across transaction kinds"));
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

fn migrate_loaded(data: &mut AuthorityData) -> Result<bool, AuthorityError> {
    match data.format_version {
        AUTHORITY_FORMAT_VERSION => Ok(backfill_revision_states(data)),
        EXPERIENCE_AUTHORITY_FORMAT_VERSION => {
            data.format_version = AUTHORITY_FORMAT_VERSION;
            backfill_revision_states(data);
            Ok(true)
        }
        LEGACY_AUTHORITY_FORMAT_VERSION => {
            data.format_version = AUTHORITY_FORMAT_VERSION;
            data.experiences
                .entry(STOCK_SHELL_EXPERIENCE_ID.into())
                .or_insert_with(|| data.current.clone());
            backfill_revision_states(data);
            Ok(true)
        }
        _ => Err(invalid("invalid authority file version")),
    }
}

fn validate_grant_decision(decision: &GrantDecisionResource) -> Result<(), AuthorityError> {
    experience_package::ExperienceId::parse(decision.experience_id.as_str())
        .map_err(|error| invalid(error.to_string()))?;
    if decision.generation == 0
        || decision.provider_capabilities.len() > experience_package::MAX_SCHEMA_FIELDS
    {
        return Err(invalid(
            "grant decision generation or capability count is invalid",
        ));
    }
    for capability in &decision.provider_capabilities {
        if capability.is_empty()
            || capability.len() > experience_package::MAX_NAME_BYTES
            || !capability
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase())
            || capability.bytes().any(|byte| {
                !(byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-'))
            })
        {
            return Err(invalid("grant decision contains an invalid capability"));
        }
    }
    if decision.data_flows.len() > experience_package::MAX_DEPENDENCIES {
        return Err(invalid("grant decision contains too many data flows"));
    }
    for (alias, flow) in &decision.data_flows {
        experience_package::DependencyAlias::parse(alias.as_str())
            .map_err(|error| invalid(error.to_string()))?;
        experience_package::ExperienceId::parse(flow.experience_id.as_str())
            .map_err(|error| invalid(error.to_string()))?;
        experience_package::ExportId::parse(flow.export_id.as_str())
            .map_err(|error| invalid(error.to_string()))?;
        for property in &flow.grant.properties {
            if property.is_empty() || property.len() > experience_package::MAX_NAME_BYTES {
                return Err(invalid("grant decision contains an invalid property"));
            }
        }
        for event in &flow.grant.events {
            experience_package::EventId::parse(event.as_str())
                .map_err(|error| invalid(error.to_string()))?;
        }
    }
    Ok(())
}

fn backfill_revision_states(data: &mut AuthorityData) -> bool {
    let mut changed = false;
    for (experience_id, state) in &data.experiences {
        if state.revision_id.is_empty() {
            continue;
        }
        let revisions = data
            .experience_revisions
            .entry(experience_id.clone())
            .or_default();
        if !revisions.contains_key(&state.revision_id) {
            revisions.insert(state.revision_id.clone(), state.clone());
            changed = true;
        }
    }
    changed
}

fn initial_state_for(draft: &PromotionDraft, current: Option<&StateResource>) -> StateResource {
    current.cloned().unwrap_or_else(|| StateResource {
        revision: 0,
        revision_id: String::new(),
        schema_version: draft.schema_version,
        source_sha256: String::new(),
        state: serde_json::json!({}),
    })
}

fn revision_state_for(
    data: &AuthorityData,
    experience_id: &str,
    revision_id: &str,
    draft: &PromotionDraft,
) -> StateResource {
    let exact = data
        .experience_revisions
        .get(experience_id)
        .and_then(|revisions| revisions.get(revision_id));
    let current = data
        .experiences
        .get(experience_id)
        .filter(|state| state.revision_id == revision_id);
    initial_state_for(draft, exact.or(current))
}

fn validate_state_resource(state: &StateResource) -> Result<(), AuthorityError> {
    if state.schema_version == 0 {
        return Err(invalid("state schema version must be positive"));
    }
    if !state.revision_id.is_empty() {
        validate_sha256("state revision ID", &state.revision_id)?;
    }
    if !state.source_sha256.is_empty() {
        validate_sha256("state source SHA-256", &state.source_sha256)?;
    }
    if serde_json::to_vec(&state.state)?.len() > MAX_STATE_BYTES {
        return Err(invalid("stored state exceeds the service limit"));
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
