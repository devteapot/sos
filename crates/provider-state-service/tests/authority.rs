use std::collections::{BTreeMap, BTreeSet};

use provider_state_service::{state_sha256, Authority, AuthorityError};
use serde_json::json;
use service_protocol::{
    DataFlowGrant, ExperiencePromotionDraft, FaultPoint, GrantDecisionResource,
    GraphExperiencePromotion, GraphPromotionDraft, MigrationProof, NotesAction, PromotionDraft,
    ProviderAction, ServiceError, ServiceEventKind, TransactionStatus,
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

fn graph_draft(transaction_id: &str) -> GraphPromotionDraft {
    GraphPromotionDraft {
        transaction_id: transaction_id.into(),
        activate: false,
        promotions: [
            ("dashboard", 'd', json!({"selected": "agenda"})),
            ("agenda", 'a', json!({"open": true})),
        ]
        .into_iter()
        .map(
            |(experience_id, revision, state)| GraphExperiencePromotion {
                experience_id: experience_package::ExperienceId::parse(experience_id).unwrap(),
                expected_revision: 0,
                revision_id: revision.to_string().repeat(64),
                schema_version: 1,
                source_sha256: revision.to_ascii_uppercase().to_string().repeat(64),
                state,
                migration: None,
                actions: vec![],
            },
        )
        .collect(),
    }
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

#[test]
fn graph_promotion_commits_all_experience_states_in_one_durable_transition() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    let draft = graph_draft("graph-1");
    let staged = authority.stage_graph(draft.clone()).unwrap();
    assert_eq!(staged.status, TransactionStatus::Staged);
    assert_eq!(authority.stage_graph(draft).unwrap(), staged);
    assert_eq!(authority.current_for("dashboard").revision, 0);
    assert_eq!(authority.current_for("agenda").revision, 0);

    let committed = authority.promote_graph("graph-1").unwrap();
    assert_eq!(committed.status, TransactionStatus::Committed);
    assert_eq!(committed.committed_revisions.len(), 2);
    assert_eq!(
        authority.current_for("dashboard").state["selected"],
        "agenda"
    );
    assert_eq!(authority.current_for("agenda").state["open"], true);

    let reopened = open(&directory);
    assert_eq!(reopened.current_for("dashboard").revision, 1);
    assert_eq!(reopened.current_for("agenda").revision, 1);
}

#[test]
fn graph_promotion_recovers_all_nodes_after_a_committing_fault() {
    let directory = TempDir::new().unwrap();
    {
        let mut authority = open(&directory);
        authority.stage_graph(graph_draft("graph-recover")).unwrap();
        authority.configure_fault(Some(FaultPoint::DuringPromotion));
        injected(
            authority.promote_graph("graph-recover").unwrap_err(),
            FaultPoint::DuringPromotion,
        );
        assert_eq!(
            authority.graph_transaction("graph-recover").unwrap().status,
            TransactionStatus::Committing
        );
        assert_eq!(authority.current_for("dashboard").revision, 1);
        assert_eq!(authority.current_for("agenda").revision, 1);
    }
    let recovered = open(&directory);
    assert_eq!(
        recovered.graph_transaction("graph-recover").unwrap().status,
        TransactionStatus::Committed
    );
    assert_eq!(recovered.current_for("dashboard").revision, 1);
    assert_eq!(recovered.current_for("agenda").revision, 1);
}

#[test]
fn experience_state_and_appearance_are_independent_resources() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    let experience_id = experience_package::ExperienceId::parse("agenda").unwrap();
    authority
        .stage_experience(ExperiencePromotionDraft {
            experience_id: experience_id.clone(),
            draft: draft("agenda-state", 0, 1, json!({"filter": "today"})),
        })
        .unwrap();
    authority.promote("agenda-state").unwrap();
    let mut appearance = authority.appearance().profile;
    appearance.generation = 1;
    appearance.colors.insert(
        experience_package::TokenId::parse("accent").unwrap(),
        "#00ffffff".into(),
    );
    assert!(authority
        .update_appearance(0, "ungranted", appearance.clone())
        .is_err());
    authority
        .configure_appearance_writer("appearance-test-capability")
        .unwrap();
    authority
        .update_appearance(0, "appearance-test-capability", appearance.clone())
        .unwrap();

    assert_eq!(
        authority.current_for(experience_id.as_str()).state["filter"],
        "today"
    );
    assert_eq!(authority.appearance().profile, appearance);
    assert_eq!(authority.current().revision, 0);
    drop(authority);
    assert_eq!(open(&directory).appearance().profile, appearance);
}

#[test]
fn stable_experience_grants_are_capability_protected_versioned_and_persistent() {
    let directory = TempDir::new().unwrap();
    let mut authority = open(&directory);
    let decision = GrantDecisionResource {
        generation: 1,
        reviewed: true,
        experience_id: experience_package::ExperienceId::parse("dashboard").unwrap(),
        provider_capabilities: BTreeSet::from(["notes_read".into()]),
        data_flows: BTreeMap::from([(
            experience_package::DependencyAlias::parse("agenda").unwrap(),
            DataFlowGrant {
                experience_id: experience_package::ExperienceId::parse("agenda").unwrap(),
                export_id: experience_package::ExportId::parse("summary").unwrap(),
                grant: experience_package::BoundaryGrant {
                    properties: BTreeSet::from(["title".into()]),
                    events: BTreeSet::new(),
                },
            },
        )]),
    };
    assert!(authority
        .update_grant_decision(0, "ungranted", decision.clone())
        .is_err());
    authority.configure_grant_writer("review-secret").unwrap();
    authority
        .update_grant_decision(0, "review-secret", decision.clone())
        .unwrap();
    assert_eq!(
        authority.grant_decision_for("dashboard"),
        Some(decision.clone())
    );
    assert!(authority
        .update_grant_decision(0, "review-secret", decision.clone())
        .is_err());
    let expanded = GrantDecisionResource {
        generation: 2,
        provider_capabilities: BTreeSet::from(["notes_read".into(), "notes_write".into()]),
        ..decision.clone()
    };
    authority
        .update_grant_decision(1, "review-secret", expanded.clone())
        .unwrap();
    drop(authority);
    assert_eq!(
        open(&directory).grant_decision_for("dashboard"),
        Some(expanded)
    );
}

#[test]
fn locked_revision_state_survives_a_newer_current_revision_and_restart() {
    let directory = TempDir::new().unwrap();
    let experience_id = experience_package::ExperienceId::parse("agenda").unwrap();
    let first_revision = "a".repeat(64);
    let second_revision = "c".repeat(64);
    {
        let mut authority = open(&directory);
        let mut first = draft("agenda-first", 0, 1, json!({"selected": "first"}));
        first.revision_id = first_revision.clone();
        authority
            .stage_experience(ExperiencePromotionDraft {
                experience_id: experience_id.clone(),
                draft: first,
            })
            .unwrap();
        authority.promote("agenda-first").unwrap();

        let mut second = draft("agenda-second", 1, 1, json!({"selected": "second"}));
        second.revision_id = second_revision.clone();
        authority
            .stage_experience(ExperiencePromotionDraft {
                experience_id: experience_id.clone(),
                draft: second,
            })
            .unwrap();
        authority.promote("agenda-second").unwrap();

        authority
            .stage_graph(GraphPromotionDraft {
                transaction_id: "locked-graph-action".into(),
                activate: false,
                promotions: vec![GraphExperiencePromotion {
                    experience_id: experience_id.clone(),
                    expected_revision: 1,
                    revision_id: first_revision.clone(),
                    schema_version: 1,
                    source_sha256: "b".repeat(64),
                    state: json!({"selected": "locked"}),
                    migration: None,
                    actions: vec![],
                }],
            })
            .unwrap();
        authority.promote_graph("locked-graph-action").unwrap();
        assert_eq!(
            authority
                .current_at(experience_id.as_str(), &first_revision)
                .state["selected"],
            "locked"
        );
        assert_eq!(
            authority.current_for(experience_id.as_str()).revision_id,
            second_revision
        );
    }
    let recovered = open(&directory);
    assert_eq!(
        recovered
            .current_at(experience_id.as_str(), &first_revision)
            .state["selected"],
        "locked"
    );
    assert_eq!(
        recovered.current_for(experience_id.as_str()).revision_id,
        second_revision
    );
}
