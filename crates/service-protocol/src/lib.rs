use std::collections::{BTreeMap, BTreeSet};

use experience_package::{
    AppearanceProfile, BoundaryGrant, ColorScheme, Contrast, DependencyAlias, ExperienceId,
    ExportId, TokenId, TypographyToken, APPEARANCE_ABI_VERSION,
};
use serde::{Deserialize, Serialize};

pub const LEGACY_PROTOCOL_VERSION: u32 = 1;
pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_STATE_BYTES: usize = 1024 * 1024;
pub const MAX_ACTIONS: usize = 64;
pub const MAX_EVENTS_PER_REQUEST: usize = 1_000;
pub const MAX_GRAPH_PROMOTIONS: usize = experience_package::MAX_GRAPH_INSTANCES;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct StateResource {
    pub revision: u64,
    pub revision_id: String,
    pub schema_version: u64,
    pub source_sha256: String,
    #[serde(default)]
    pub state: serde_json::Value,
}

impl Default for StateResource {
    fn default() -> Self {
        Self {
            revision: 0,
            revision_id: String::new(),
            schema_version: 1,
            source_sha256: String::new(),
            state: serde_json::json!({}),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ExperienceStateResource {
    pub experience_id: ExperienceId,
    #[serde(flatten)]
    pub resource: StateResource,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppearanceResource {
    #[serde(flatten)]
    pub profile: AppearanceProfile,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GrantDecisionResource {
    pub generation: u64,
    pub reviewed: bool,
    pub experience_id: ExperienceId,
    #[serde(default)]
    pub provider_capabilities: BTreeSet<String>,
    #[serde(default)]
    pub data_flows: BTreeMap<DependencyAlias, DataFlowGrant>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DataFlowGrant {
    pub experience_id: ExperienceId,
    pub export_id: ExportId,
    #[serde(flatten)]
    pub grant: BoundaryGrant,
}

impl Default for AppearanceResource {
    fn default() -> Self {
        Self {
            profile: stock_appearance_profile(ColorScheme::Dark),
        }
    }
}

/// The semantic appearance installed by both platform authorities before a
/// user-authored appearance exists. Experiences still own their style rules;
/// this resource only gives every graph instance the same bounded tokens.
pub fn stock_appearance_profile(scheme: ColorScheme) -> AppearanceProfile {
    fn token(value: &str) -> TokenId {
        TokenId::parse(value).expect("stock appearance tokens are fixed and valid")
    }

    let (canvas, primary, secondary, panel, border, text, muted, accent_subtle, danger_subtle) =
        match scheme {
            ColorScheme::Dark => (
                "#000000ff",
                "#0a0a0aff",
                "#111111ff",
                "#1b1b1bff",
                "#2a2a2aff",
                "#edededff",
                "#8b8b8bff",
                "#181818ff",
                "#2a0f12ff",
            ),
            ColorScheme::Light => (
                "#ffffffff",
                "#fafafaff",
                "#f4f4f5ff",
                "#e4e4e7ff",
                "#d4d4d8ff",
                "#09090bff",
                "#71717aff",
                "#f4f4f5ff",
                "#fff1f2ff",
            ),
        };
    let colors = BTreeMap::from([
        (token("surface.canvas"), canvas.into()),
        (token("surface.primary"), primary.into()),
        (token("surface.secondary"), secondary.into()),
        (token("surface.panel"), panel.into()),
        (token("border.subtle"), border.into()),
        (token("text.primary"), text.into()),
        (token("text.muted"), muted.into()),
        (token("accent.primary"), text.into()),
        (token("accent.subtle"), accent_subtle.into()),
        (token("status.warning"), "#f5a524ff".into()),
        (token("status.danger"), "#ef4444ff".into()),
        (token("status.danger_subtle"), danger_subtle.into()),
        // Reference experiences written before the semantic surface names use
        // this generic token. Keep it aligned with the stock canvas.
        (token("background"), canvas.into()),
    ]);
    let spacing = BTreeMap::from([
        (token("space.xs"), 4),
        (token("space.sm"), 8),
        (token("space.md"), 12),
        (token("space.lg"), 16),
        (token("space.xl"), 24),
    ]);
    let radii = BTreeMap::from([
        (token("radius.control"), 6),
        (token("radius.card"), 8),
        (token("radius.panel"), 10),
    ]);
    let typography = BTreeMap::from([
        (
            token("type.label"),
            TypographyToken {
                family: "Geist".into(),
                size_milli_points: 10_000,
                weight: 500,
                line_height_milli: 1_200,
            },
        ),
        (
            token("type.body"),
            TypographyToken {
                family: "Geist".into(),
                size_milli_points: 13_000,
                weight: 400,
                line_height_milli: 1_400,
            },
        ),
        (
            token("type.title"),
            TypographyToken {
                family: "Geist".into(),
                size_milli_points: 24_000,
                weight: 500,
                line_height_milli: 1_150,
            },
        ),
    ]);
    let profile = AppearanceProfile {
        abi_version: APPEARANCE_ABI_VERSION,
        generation: 0,
        scheme,
        contrast: Contrast::High,
        text_scale_milli: 1_000,
        reduce_motion: false,
        colors,
        spacing,
        radii,
        typography,
    };
    profile
        .validate()
        .expect("stock appearance profile must satisfy the wire contract");
    profile
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NotesResource {
    pub attachments: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "resource", rename_all = "snake_case")]
pub enum ResourceQuery {
    ExperienceState,
    ExperienceStateFor {
        experience_id: ExperienceId,
    },
    ExperienceStateAt {
        experience_id: ExperienceId,
        revision_id: String,
    },
    GrantDecisionFor {
        experience_id: ExperienceId,
    },
    Appearance,
    Notes,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "resource", content = "value", rename_all = "snake_case")]
pub enum ResourceValue {
    ExperienceState(StateResource),
    ExperienceStateFor(ExperienceStateResource),
    ExperienceStateAt(ExperienceStateResource),
    GrantDecision(GrantDecisionResource),
    Appearance(AppearanceResource),
    Notes(NotesResource),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "provider", content = "action", rename_all = "snake_case")]
pub enum ProviderAction {
    Notes(NotesAction),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotesAction {
    AttachToEvent {
        note_id: String,
        event_title: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct MigrationProof {
    pub from_schema_version: u64,
    pub to_schema_version: u64,
    pub from_state_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PromotionDraft {
    pub transaction_id: String,
    pub expected_revision: u64,
    pub revision_id: String,
    pub schema_version: u64,
    pub source_sha256: String,
    pub state: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationProof>,
    #[serde(default)]
    pub actions: Vec<ProviderAction>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ExperiencePromotionDraft {
    pub experience_id: ExperienceId,
    #[serde(flatten)]
    pub draft: PromotionDraft,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphExperiencePromotion {
    pub experience_id: ExperienceId,
    pub expected_revision: u64,
    pub revision_id: String,
    pub schema_version: u64,
    pub source_sha256: String,
    pub state: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationProof>,
    #[serde(default)]
    pub actions: Vec<ProviderAction>,
}

impl GraphExperiencePromotion {
    pub fn as_promotion(&self, transaction_id: &str) -> PromotionDraft {
        PromotionDraft {
            transaction_id: transaction_id.into(),
            expected_revision: self.expected_revision,
            revision_id: self.revision_id.clone(),
            schema_version: self.schema_version,
            source_sha256: self.source_sha256.clone(),
            state: self.state.clone(),
            migration: self.migration.clone(),
            actions: self.actions.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphPromotionDraft {
    pub transaction_id: String,
    #[serde(default)]
    pub activate: bool,
    pub promotions: Vec<GraphExperiencePromotion>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    Staged,
    Committing,
    Committed,
    Aborted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct EffectReceipt {
    pub effect_id: String,
    pub action: ProviderAction,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TransactionRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experience_id: Option<ExperienceId>,
    pub draft: PromotionDraft,
    pub status: TransactionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_revision: Option<u64>,
    #[serde(default)]
    pub effects: Vec<EffectReceipt>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GraphEffectReceipt {
    pub experience_id: ExperienceId,
    pub effect_id: String,
    pub action: ProviderAction,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GraphTransactionRecord {
    pub draft: GraphPromotionDraft,
    pub status: TransactionStatus,
    #[serde(default)]
    pub committed_revisions: BTreeMap<ExperienceId, u64>,
    #[serde(default)]
    pub effects: Vec<GraphEffectReceipt>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ServiceEvent {
    pub sequence: u64,
    #[serde(flatten)]
    pub kind: ServiceEventKind,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ServiceEventKind {
    TransactionStaged {
        transaction_id: String,
        expected_revision: u64,
    },
    RevisionCommitted {
        transaction_id: String,
        revision: u64,
        revision_id: String,
    },
    ActionApplied {
        transaction_id: String,
        effect_id: String,
        action: ProviderAction,
    },
    TransactionCompleted {
        transaction_id: String,
        revision: u64,
    },
    TransactionAborted {
        transaction_id: String,
    },
    GraphTransactionStaged {
        transaction_id: String,
        experience_count: usize,
    },
    GraphRevisionsCommitted {
        transaction_id: String,
        revisions: BTreeMap<ExperienceId, u64>,
    },
    GraphActionApplied {
        transaction_id: String,
        experience_id: ExperienceId,
        effect_id: String,
        action: ProviderAction,
    },
    GraphTransactionCompleted {
        transaction_id: String,
    },
    GraphTransactionAborted {
        transaction_id: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaultPoint {
    BeforeStage,
    AfterStage,
    BeforePromotion,
    DuringPromotion,
    AfterPromotion,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ServiceRequest {
    GetResource {
        request_id: u64,
        query: ResourceQuery,
    },
    StagePromotion {
        request_id: u64,
        draft: PromotionDraft,
    },
    StageExperiencePromotion {
        request_id: u64,
        draft: ExperiencePromotionDraft,
    },
    StageGraphPromotion {
        request_id: u64,
        draft: GraphPromotionDraft,
    },
    UpdateAppearance {
        request_id: u64,
        expected_generation: u64,
        capability: String,
        profile: AppearanceProfile,
    },
    UpdateGrantDecision {
        request_id: u64,
        expected_generation: u64,
        capability: String,
        decision: GrantDecisionResource,
    },
    Promote {
        request_id: u64,
        transaction_id: String,
    },
    Abort {
        request_id: u64,
        transaction_id: String,
    },
    GetTransaction {
        request_id: u64,
        transaction_id: String,
    },
    PromoteGraph {
        request_id: u64,
        transaction_id: String,
    },
    AbortGraph {
        request_id: u64,
        transaction_id: String,
    },
    GetGraphTransaction {
        request_id: u64,
        transaction_id: String,
    },
    ListEvents {
        request_id: u64,
        after_sequence: u64,
        limit: usize,
    },
    ConfigureFault {
        request_id: u64,
        point: Option<FaultPoint>,
    },
    Shutdown {
        request_id: u64,
    },
}

impl ServiceRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::GetResource { request_id, .. }
            | Self::StagePromotion { request_id, .. }
            | Self::StageExperiencePromotion { request_id, .. }
            | Self::StageGraphPromotion { request_id, .. }
            | Self::UpdateAppearance { request_id, .. }
            | Self::UpdateGrantDecision { request_id, .. }
            | Self::Promote { request_id, .. }
            | Self::Abort { request_id, .. }
            | Self::GetTransaction { request_id, .. }
            | Self::PromoteGraph { request_id, .. }
            | Self::AbortGraph { request_id, .. }
            | Self::GetGraphTransaction { request_id, .. }
            | Self::ListEvents { request_id, .. }
            | Self::ConfigureFault { request_id, .. }
            | Self::Shutdown { request_id } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ServiceRequestEnvelope {
    pub protocol_version: u32,
    #[serde(flatten)]
    pub request: ServiceRequest,
}

impl ServiceRequestEnvelope {
    pub fn new(request: ServiceRequest) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ResponsePayload {
    Resource { value: ResourceValue },
    Transaction { record: TransactionRecord },
    GraphTransaction { record: GraphTransactionRecord },
    AppearanceUpdated { value: AppearanceResource },
    GrantDecisionUpdated { value: GrantDecisionResource },
    Events { events: Vec<ServiceEvent> },
    FaultConfigured,
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ServiceError {
    InvalidRequest { message: String },
    Denied { message: String },
    Conflict { message: String },
    NotFound { message: String },
    InvalidMigration { message: String },
    InjectedFault { point: FaultPoint },
    Internal { message: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ServiceResponse {
    pub protocol_version: u32,
    pub request_id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<ResponsePayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ServiceError>,
}

impl ServiceResponse {
    pub fn success(request_id: u64, payload: ResponsePayload) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn failure(request_id: u64, error: ServiceError) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            ok: false,
            payload: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color<'a>(profile: &'a AppearanceProfile, name: &str) -> &'a str {
        let token = TokenId::parse(name).unwrap();
        profile.colors.get(&token).unwrap()
    }

    #[test]
    fn default_appearance_is_the_high_contrast_dark_stock_profile() {
        let profile = AppearanceResource::default().profile;
        assert_eq!(profile.scheme, ColorScheme::Dark);
        assert_eq!(profile.contrast, Contrast::High);
        assert_eq!(color(&profile, "surface.canvas"), "#000000ff");
        assert_eq!(color(&profile, "text.primary"), "#edededff");
        assert_eq!(profile.typography.len(), 3);
        assert!(profile
            .typography
            .values()
            .all(|typography| typography.family == "Geist"));
        profile.validate().unwrap();
    }

    #[test]
    fn light_stock_profile_changes_semantic_surfaces_without_changing_type() {
        let profile = stock_appearance_profile(ColorScheme::Light);
        assert_eq!(color(&profile, "surface.canvas"), "#ffffffff");
        assert_eq!(color(&profile, "text.primary"), "#09090bff");
        assert!(profile
            .typography
            .values()
            .all(|typography| typography.family == "Geist"));
        profile.validate().unwrap();
    }
}
