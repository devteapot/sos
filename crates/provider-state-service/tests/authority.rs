use provider_state_service::{state_sha256, Authority, AuthorityError};
use serde_json::json;
use service_protocol::{
    FaultPoint, MigrationProof, NotesAction, PromotionDraft, ProviderAction, ServiceError,
    ServiceEventKind, TransactionStatus,
};
use tempfile::TempDir;

fn open(directory: &TempDir) -> Authority {
    Authority::open(directory.path().join("authority.json")).unwrap()
}

fn draft(
    transaction_id: &str,
    expected_revision: u64,
    schema_version: u64,
    state: serde_json::Value,
) -> PromotionDraft {
    PromotionDraft {
        transaction_id: transaction_id.into(),
        expected_revision,
        revision_id: "a".repeat(64),
        schema_version,
        source_sha256: "b".repeat(64),
        state,
        migration: None,
        actions: vec![ProviderAction::Notes(NotesAction::AttachToEvent {
            note_id: "note-1".into(),
            event_title: "Design review".into(),
        })],
    }
}

fn injected(error: AuthorityError, expected: FaultPoint) {
    assert!(matches!(
        error,
        AuthorityError::Service(ServiceError::InjectedFault { point }) if point == expected
    ));
}

#[test]
fn stage_and_promote_are_idempotent_and_expose_typed_resources_and_events() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    let draft = draft("tx-1", 0, 1, json!({"attached": true}));
    let staged = authority.stage(draft.clone()).unwrap();
    assert_eq!(staged.status, TransactionStatus::Staged);
    assert_eq!(authority.stage(draft).unwrap(), staged);
    assert!(authority.notes().attachments.is_empty());

    let committed = authority.promote("tx-1").unwrap();
    assert_eq!(committed.status, TransactionStatus::Committed);
    assert_eq!(committed.committed_revision, Some(1));
    assert_eq!(committed.effects.len(), 1);
    assert_eq!(authority.promote("tx-1").unwrap(), committed);
    assert_eq!(authority.current().revision, 1);
    assert_eq!(
        authority.notes().attachments.get("note-1").unwrap(),
        "Design review"
    );
    assert_eq!(
        authority
            .events(0, 100)
            .iter()
            .filter(|event| matches!(event.kind, ServiceEventKind::ActionApplied { .. }))
            .count(),
        1
    );
}

#[test]
fn schema_change_requires_a_proof_bound_to_the_current_state() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    let mut missing = draft("tx-missing", 0, 2, json!({"migrated": true}));
    assert!(matches!(
        authority.stage(missing.clone()),
        Err(AuthorityError::Service(
            ServiceError::InvalidMigration { .. }
        ))
    ));
    missing.migration = Some(MigrationProof {
        from_schema_version: 1,
        to_schema_version: 2,
        from_state_sha256: "0".repeat(64),
    });
    assert!(matches!(
        authority.stage(missing),
        Err(AuthorityError::Service(
            ServiceError::InvalidMigration { .. }
        ))
    ));

    let mut valid = draft("tx-valid", 0, 2, json!({"migrated": true}));
    valid.migration = Some(MigrationProof {
        from_schema_version: 1,
        to_schema_version: 2,
        from_state_sha256: state_sha256(&json!({})).unwrap(),
    });
    authority.stage(valid).unwrap();
    authority.promote("tx-valid").unwrap();
    assert_eq!(authority.current().schema_version, 2);
    assert_eq!(authority.current().state, json!({"migrated": true}));
}

#[test]
fn before_and_after_stage_faults_have_reconcilable_boundaries() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    authority.configure_fault(Some(FaultPoint::BeforeStage));
    injected(
        authority
            .stage(draft("tx-before", 0, 1, json!({})))
            .unwrap_err(),
        FaultPoint::BeforeStage,
    );
    assert!(matches!(
        authority.transaction("tx-before"),
        Err(AuthorityError::Service(ServiceError::NotFound { .. }))
    ));

    authority.configure_fault(Some(FaultPoint::AfterStage));
    let after = draft("tx-after", 0, 1, json!({}));
    injected(
        authority.stage(after.clone()).unwrap_err(),
        FaultPoint::AfterStage,
    );
    assert_eq!(
        authority.transaction("tx-after").unwrap().status,
        TransactionStatus::Staged
    );
    assert_eq!(
        authority.stage(after).unwrap().status,
        TransactionStatus::Staged
    );
}

#[test]
fn before_promotion_fault_preserves_current_and_staged_transaction() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    authority
        .stage(draft("tx-before", 0, 1, json!({"value": 1})))
        .unwrap();
    authority.configure_fault(Some(FaultPoint::BeforePromotion));
    injected(
        authority.promote("tx-before").unwrap_err(),
        FaultPoint::BeforePromotion,
    );
    assert_eq!(authority.current().revision, 0);
    assert_eq!(
        authority.transaction("tx-before").unwrap().status,
        TransactionStatus::Staged
    );
    assert!(authority.notes().attachments.is_empty());
}

#[test]
fn during_promotion_fault_recovers_after_restart_and_applies_effect_once() {
    let directory = TempDir::new().unwrap();
    {
        let mut authority = open(&directory);
        authority
            .stage(draft("tx-during", 0, 1, json!({"value": 2})))
            .unwrap();
        authority.configure_fault(Some(FaultPoint::DuringPromotion));
        injected(
            authority.promote("tx-during").unwrap_err(),
            FaultPoint::DuringPromotion,
        );
        assert_eq!(authority.current().revision, 1);
        assert_eq!(
            authority.transaction("tx-during").unwrap().status,
            TransactionStatus::Committing
        );
        assert!(authority.notes().attachments.is_empty());
    }

    let mut recovered = open(&directory);
    assert_eq!(
        recovered.transaction("tx-during").unwrap().status,
        TransactionStatus::Committed
    );
    assert_eq!(recovered.notes().attachments.len(), 1);
    assert_eq!(recovered.promote("tx-during").unwrap().effects.len(), 1);
    assert_eq!(
        recovered
            .events(0, 100)
            .iter()
            .filter(|event| matches!(event.kind, ServiceEventKind::ActionApplied { .. }))
            .count(),
        1
    );
}

#[test]
fn after_promotion_fault_is_ambiguous_but_retry_does_not_repeat_effect() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    authority
        .stage(draft("tx-after", 0, 1, json!({"value": 3})))
        .unwrap();
    authority.configure_fault(Some(FaultPoint::AfterPromotion));
    injected(
        authority.promote("tx-after").unwrap_err(),
        FaultPoint::AfterPromotion,
    );
    assert_eq!(authority.current().revision, 1);
    assert_eq!(
        authority.transaction("tx-after").unwrap().status,
        TransactionStatus::Committed
    );
    authority.promote("tx-after").unwrap();
    let reopened = open(&directory);
    assert_eq!(reopened.notes().attachments.len(), 1);
    assert_eq!(
        reopened
            .events(0, 100)
            .iter()
            .filter(|event| matches!(event.kind, ServiceEventKind::ActionApplied { .. }))
            .count(),
        1
    );
}

#[test]
fn competing_staged_transactions_are_checked_again_at_promotion() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    authority
        .stage(draft("tx-first", 0, 1, json!({"winner": 1})))
        .unwrap();
    authority
        .stage(draft("tx-second", 0, 1, json!({"winner": 2})))
        .unwrap();
    authority.promote("tx-first").unwrap();
    assert!(matches!(
        authority.promote("tx-second"),
        Err(AuthorityError::Service(ServiceError::Conflict { .. }))
    ));
    assert_eq!(authority.current().state, json!({"winner": 1}));
}

#[test]
fn aborted_transactions_cannot_promote() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    authority.stage(draft("tx-abort", 0, 1, json!({}))).unwrap();
    assert_eq!(
        authority.abort("tx-abort").unwrap().status,
        TransactionStatus::Aborted
    );
    assert!(authority.promote("tx-abort").is_err());
    assert_eq!(authority.current().revision, 0);
}

#[test]
fn event_limit_is_bounded() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    authority
        .stage(draft("tx-events", 0, 1, json!({})))
        .unwrap();
    authority.promote("tx-events").unwrap();
    assert_eq!(authority.events(0, 1).len(), 1);
    assert!(authority.events(10_000, 100).is_empty());
}
