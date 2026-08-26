use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const EXPERIENCE_API_VERSION: u32 = 3;
pub const MAX_SCENE_DEPTH: usize = 32;
pub const MAX_SCENE_NODES: usize = 2_048;
pub const MAX_CHILDREN: usize = 256;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_PAINT_OPS: usize = 4_096;
pub const MAX_PAINT_POINTS: usize = 8_192;
pub const MAX_PAINT_DEPTH: usize = 16;
pub const MAX_GLYPH_RUNS: usize = 256;
pub const MAX_HIT_REGIONS: usize = 256;
pub const MAX_REVISION_ASSETS: usize = 64;
pub const MAX_REVISION_ASSET_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REVISION_ASSET_TOTAL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_EFFECTS: usize = 16;
pub const MAX_EFFECT_PAYLOAD_BYTES: usize = 16 * 1024;
pub const MAX_STATE_BYTES: usize = 1024 * 1024;
pub const MAX_AGENT_MESSAGES: usize = 24;
pub const MAX_AGENT_MESSAGE_BYTES: usize = 32 * 1024;
pub const SYSTEM_PROVIDER_ABI_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExperienceModel {
    pub greeting: String,
    pub date: String,
    pub weather: Weather,
    pub calendar: Vec<CalendarEvent>,
    pub notes: Vec<Note>,
    pub music: Music,
    #[serde(default)]
    pub system: SystemState,
    #[serde(default)]
    pub surfaces: Vec<ProviderSurface>,
    #[serde(default)]
    pub agent: AgentConversation,
    #[serde(default)]
    pub network: NetworkState,
    /// Versioned system facts exposed equally to stock and generated
    /// experiences. Platform objects and authority-only identifiers never
    /// cross this boundary.
    #[serde(default)]
    pub providers: SystemProviders,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemProviders {
    pub abi_version: u32,
    pub observed_at_ms: u64,
    #[serde(default)]
    pub clock: ClockProviderState,
    #[serde(default)]
    pub power: PowerProviderState,
    #[serde(default)]
    pub connectivity: ConnectivityProviderState,
    #[serde(default)]
    pub audio: AudioProviderState,
    #[serde(default)]
    pub apps: AppsProviderState,
    #[serde(default)]
    pub attention: AttentionProviderState,
    #[serde(default)]
    pub capabilities: Vec<SystemCapability>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClockProviderState {
    pub unix_time_ms: u64,
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub time_label: String,
    #[serde(default)]
    pub date_label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowerProviderState {
    pub battery_percent: Option<u8>,
    pub charging: Option<bool>,
    #[serde(default)]
    pub charging_source: String,
    pub battery_temperature_deci_c: Option<i32>,
    pub thermal_status: Option<ThermalStatus>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalStatus {
    None,
    Light,
    Moderate,
    Severe,
    Critical,
    Emergency,
    Shutdown,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectivityProviderState {
    pub wifi_enabled: bool,
    pub connected: bool,
    pub validated: bool,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub network_label: String,
    pub signal_level: Option<u8>,
    #[serde(default)]
    pub online_interfaces: Vec<String>,
    #[serde(default)]
    pub wifi_networks: Vec<SystemWifiNetwork>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemWifiNetwork {
    /// Authority-scoped opaque identifier. It is not an SSID, network id, or
    /// framework handle.
    pub id: String,
    pub label: String,
    pub signal_level: u8,
    pub saved: bool,
    pub connected: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioProviderState {
    pub volume_percent: Option<u8>,
    pub muted: Option<bool>,
    #[serde(default)]
    pub media: MediaProviderState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaProviderState {
    pub active: bool,
    pub playing: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppsProviderState {
    #[serde(default)]
    pub compatible: Vec<SystemApplication>,
    /// Bounded, declarative status contributions from SOS-native
    /// applications. Existing desktop applications do not gain this surface
    /// implicitly; a trusted application provider must publish it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status_widgets: Vec<ApplicationStatusWidget>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemApplication {
    /// Authority-scoped opaque identifier. Package and Activity names remain
    /// inside the framework bridge.
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationStatusWidget {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub value: String,
    /// When present, tapping the contribution requests the existing bounded
    /// application-launch effect for this opaque application selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionProviderState {
    #[serde(default)]
    pub items: Vec<AttentionItem>,
    pub urgent_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionItem {
    /// Authority-scoped opaque identifier. Notification keys and package
    /// names remain inside the framework bridge.
    pub id: String,
    pub occurred_at_ms: u64,
    pub source: String,
    pub kind: AttentionKind,
    pub urgent: bool,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    General,
    Message,
    Call,
    Alarm,
    Media,
    System,
    Background,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SystemCapability {
    AudioSetVolume,
    AudioSetMuted,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    WifiConnect,
    WifiDisconnect,
    AppLaunch,
    AttentionAcknowledge,
    RequestLock,
    RequestRestart,
    RequestShutdown,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkState {
    pub wifi_enabled: bool,
    pub connected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_ssid: Option<String>,
    pub validated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_level: Option<u8>,
    #[serde(default)]
    pub networks: Vec<WifiNetwork>,
    #[serde(default)]
    pub activity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal_level: u8,
    pub security: WifiSecurity,
    pub saved: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WifiSecurity {
    Open,
    Personal,
    Enterprise,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConversation {
    pub available: bool,
    pub busy: bool,
    #[serde(default)]
    pub activity: String,
    #[serde(default)]
    pub messages: Vec<AgentMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub configuration_actions: Vec<AgentConfigurationAction>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentConfigurationAction {
    #[serde(rename = "configure_openai")]
    ConfigureOpenAi,
    #[serde(rename = "configure_openrouter")]
    ConfigureOpenRouter,
    #[serde(rename = "configure_codex")]
    ConfigureCodex,
    #[serde(rename = "use_fake")]
    UseFake,
    #[serde(rename = "clear_credential")]
    ClearCredential,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMessage {
    pub role: AgentMessageRole,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemState {
    pub unix_time_ms: u64,
    pub timezone: String,
    pub online_interfaces: Vec<String>,
    pub battery_percent: Option<u8>,
    pub on_ac_power: Option<bool>,
    pub audio_volume_percent: Option<u8>,
    pub audio_muted: Option<bool>,
    pub connected_displays: Vec<String>,
    pub input_devices: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSurface {
    pub id: String,
    pub kind: ProviderSurfaceKind,
    pub width: u32,
    pub height: u32,
    pub protected: bool,
    pub status: ProviderSurfaceStatus,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSurfaceKind {
    Video,
    Camera,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSurfaceStatus {
    Ready,
    Unavailable,
    ProtectedUnavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Weather {
    pub summary: String,
    pub temperature_c: i32,
    pub high_c: i32,
    pub low_c: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CalendarEvent {
    pub time: String,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Note {
    pub title: String,
    pub preview: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Music {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SceneEvent {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocity_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocity_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_degrees: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderEffect {
    pub provider: String,
    pub action: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StateEnvelope {
    pub revision: u64,
    pub schema_version: u64,
    #[serde(default)]
    pub source_sha256: String,
    #[serde(default)]
    pub state: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateFaultPoint {
    BeforeStage,
    AfterStage,
    BeforePromote,
    AfterPromote,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderRequest {
    Snapshot {
        request_id: u64,
    },
    Action {
        request_id: u64,
        provider: String,
        action: String,
        payload: serde_json::Value,
    },
    LoadState {
        request_id: u64,
    },
    StageState {
        request_id: u64,
        expected_revision: u64,
        schema_version: u64,
        state: serde_json::Value,
        #[serde(default)]
        source_sha256: String,
        #[serde(default)]
        effects: Vec<ProviderEffect>,
    },
    PromoteState {
        request_id: u64,
        stage_id: u64,
    },
    AbortState {
        request_id: u64,
        stage_id: u64,
    },
    ConfigureStateFault {
        request_id: u64,
        point: Option<StateFaultPoint>,
    },
}

impl ProviderRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::Snapshot { request_id }
            | Self::Action { request_id, .. }
            | Self::LoadState { request_id }
            | Self::StageState { request_id, .. }
            | Self::PromoteState { request_id, .. }
            | Self::AbortState { request_id, .. }
            | Self::ConfigureStateFault { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderResponse {
    pub request_id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ExperienceModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<StateEnvelope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub root: SceneNode,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneNode {
    pub id: Option<String>,
    pub layout: Layout,
    pub content: Option<Content>,
    pub paint: Vec<PaintOp>,
    pub interaction: Interaction,
    pub animation: Option<Animation>,
    pub semantics: Option<Semantics>,
    pub children: Vec<SceneNode>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub flow: Flow,
    pub wrap: bool,
    pub scroll_y: bool,
    pub padding: Option<f32>,
    pub gap: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub position: Option<LayoutPosition>,
    pub program: Option<LayoutProgram>,
    pub clip_bounds: bool,
    pub grow: bool,
    pub align: Option<Align>,
    pub justify: Option<Justify>,
}

/// A bounded retained program evaluated by the host layout engine. Fractions
/// are relative to the containing block, so responsive layout does not call
/// back into Luau during frame construction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutProgram {
    pub measure_width: Option<f32>,
    pub measure_height: Option<f32>,
    pub arrange_x: Option<f32>,
    pub arrange_y: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flow {
    #[default]
    Overlay,
    Column,
    Row,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(TextContent),
    TextSession(TextSession),
    Image(ImageContent),
    ProviderSurface(ProviderSurfaceContent),
    /// A compositor-owned application surface region placed by the shell.
    /// The host reports the final logical bounds to the compositor; no native
    /// surface identity or authority crosses into the revision.
    WindowSpace(WindowSpaceContent),
    /// A single trusted shell overlay rendered into a separate transparent
    /// host surface. The compositor validates its requested output bounds.
    ShellOverlay(ShellOverlayContent),
    /// Source-defined application content rendered into its own native
    /// toplevel. The compositor manages this surface in the same window space
    /// as compatible desktop applications.
    ApplicationSurface(ApplicationSurfaceContent),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextContent {
    pub value: String,
    pub size: f32,
    pub color: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextSession {
    pub state_key: String,
    pub value: String,
    pub placeholder: String,
    pub submit_action: Option<String>,
    pub autofocus: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageContent {
    pub asset: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSurfaceContent {
    pub surface: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSpaceContent {
    pub layout: WindowLayoutMode,
    pub gap: f32,
    pub fallback: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellOverlayContent {
    /// Legacy absolute surface origin. Used directly when `anchor` is absent.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Optional stable action bounds. The host centers the overlay surface on
    /// this anchor, clamps the surface to the output, and keeps the anchor at
    /// the requested screen position. This lets expanded overlays reflow at
    /// an edge without moving their floating action.
    pub anchor: Option<ShellOverlayAnchor>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellOverlayAnchor {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub above: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationSurfaceContent {
    pub title: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowLayoutMode {
    #[default]
    Floating,
    Tiling,
    Scrolling,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaintOp {
    FillBounds {
        color: u32,
        radius: f32,
    },
    Path {
        points: Vec<PaintPoint>,
        color: u32,
        width: Option<f32>,
        closed: bool,
    },
    Quad {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        color: u32,
    },
    Glyphs {
        x: f32,
        y: f32,
        size: f32,
        line_height: Option<f32>,
        max_width: Option<f32>,
        runs: Vec<GlyphRun>,
    },
    /// Execute a revision-owned, host-validated WGSL module into a bounded
    /// offscreen target and composite the result into the retained scene.
    Shader {
        asset: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Layer {
        clip: Option<ClipRect>,
        transform: Transform2D,
        opacity: f32,
        operations: Vec<PaintOp>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub text: String,
    pub color: u32,
    pub font_family: Option<String>,
    pub weight: u16,
    pub italic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform2D {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation_degrees: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interaction {
    pub tap_action: Option<String>,
    pub hover_action: Option<String>,
    /// Lets a node act as the drag handle for its compositor-managed host
    /// surface. Hosts ignore this outside an explicitly movable surface such
    /// as a trusted shell overlay.
    pub surface_drag: bool,
    pub double_tap_action: Option<String>,
    pub long_press_action: Option<String>,
    pub swipe_action: Option<String>,
    pub pointer_action: Option<String>,
    pub multi_pointer_action: Option<String>,
    pub capture: PointerCapture,
    pub hit_regions: Vec<HitRegion>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerCapture {
    #[default]
    None,
    Pointer,
    Surface,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HitRegion {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub press_action: Option<String>,
    pub drag_action: Option<String>,
    pub drop_action: Option<String>,
    pub tap_action: Option<String>,
    pub double_tap_action: Option<String>,
    pub long_press_action: Option<String>,
    pub swipe_action: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub kind: AnimationKind,
    pub duration_ms: u64,
    pub repeat: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationKind {
    Pulse,
    FadeIn,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Semantics {
    pub role: SemanticRole,
    pub label: String,
    pub value: Option<String>,
    pub hint: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticRole {
    Button,
    Image,
    TextField,
    Header,
    Status,
    ScrollArea,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Justify {
    Start,
    Center,
    End,
    Between,
}

#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("scene exceeds maximum depth of {MAX_SCENE_DEPTH}")]
    TooDeep,
    #[error("scene exceeds maximum node count of {MAX_SCENE_NODES}")]
    TooManyNodes,
    #[error("node exceeds maximum child count of {MAX_CHILDREN}")]
    TooManyChildren,
    #[error("text exceeds maximum length of {MAX_TEXT_BYTES} bytes")]
    TextTooLong,
    #[error("duplicate node id: {0}")]
    DuplicateId(String),
    #[error("invalid finite dimension in {0}")]
    InvalidDimension(&'static str),
    #[error("interactive node requires a stable id")]
    MissingInteractiveId,
    #[error("semantic text exceeds maximum length of {MAX_TEXT_BYTES} bytes")]
    SemanticTextTooLong,
    #[error("animation duration must be between 16 and 60000 ms")]
    InvalidAnimationDuration,
    #[error("scene node exceeds maximum paint operation count of {MAX_PAINT_OPS}")]
    TooManyPaintOps,
    #[error("scene node exceeds maximum paint point count of {MAX_PAINT_POINTS}")]
    TooManyPaintPoints,
    #[error("paint list exceeds maximum nesting depth of {MAX_PAINT_DEPTH}")]
    PaintTooDeep,
    #[error("scene node exceeds maximum glyph run count of {MAX_GLYPH_RUNS}")]
    TooManyGlyphRuns,
    #[error("scene node exceeds maximum hit-region count of {MAX_HIT_REGIONS}")]
    TooManyHitRegions,
    #[error("layout program contains an invalid relative fraction")]
    InvalidLayoutProgram,
    #[error("window space requires a stable id")]
    MissingWindowSpaceId,
    #[error("scene contains more than one window space")]
    DuplicateWindowSpace,
    #[error("shell overlay requires a stable id")]
    MissingShellOverlayId,
    #[error("scene contains more than one shell overlay")]
    DuplicateShellOverlay,
    #[error("application surface requires a stable id")]
    MissingApplicationSurfaceId,
    #[error("scene contains more than one application surface")]
    DuplicateApplicationSurface,
    #[error("application surface title must contain 1 through 256 bytes")]
    InvalidApplicationSurfaceTitle,
}

pub fn validate_scene(scene: &Scene) -> Result<usize, ValidationError> {
    fn visit(
        node: &SceneNode,
        depth: usize,
        count: &mut usize,
        ids: &mut HashSet<String>,
        window_spaces: &mut usize,
        shell_overlays: &mut usize,
        application_surfaces: &mut usize,
    ) -> Result<(), ValidationError> {
        if depth > MAX_SCENE_DEPTH {
            return Err(ValidationError::TooDeep);
        }
        *count += 1;
        if *count > MAX_SCENE_NODES {
            return Err(ValidationError::TooManyNodes);
        }
        if node.children.len() > MAX_CHILDREN {
            return Err(ValidationError::TooManyChildren);
        }
        match &node.content {
            Some(Content::Text(text)) => {
                if text.value.len() > MAX_TEXT_BYTES {
                    return Err(ValidationError::TextTooLong);
                }
                if !valid_dimension(text.size) || text.size <= 0.0 {
                    return Err(ValidationError::InvalidDimension("text size"));
                }
            }
            Some(Content::TextSession(input)) => {
                if node.id.is_none() {
                    return Err(ValidationError::MissingInteractiveId);
                }
                if input.state_key.len() > 256
                    || input.value.len() > MAX_TEXT_BYTES
                    || input.placeholder.len() > MAX_TEXT_BYTES
                {
                    return Err(ValidationError::SemanticTextTooLong);
                }
            }
            Some(Content::ProviderSurface(surface))
                if surface.surface.is_empty()
                    || surface.surface.len() > 128
                    || !surface.surface.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    }) =>
            {
                return Err(ValidationError::InvalidDimension("provider surface id"));
            }
            Some(Content::WindowSpace(space)) => {
                if node.id.is_none() {
                    return Err(ValidationError::MissingWindowSpaceId);
                }
                *window_spaces += 1;
                if *window_spaces > 1 {
                    return Err(ValidationError::DuplicateWindowSpace);
                }
                if !valid_dimension(space.gap)
                    || space.gap > 128.0
                    || space.fallback.len() > MAX_TEXT_BYTES
                {
                    return Err(ValidationError::InvalidDimension("window space"));
                }
            }
            Some(Content::ShellOverlay(overlay)) => {
                if node.id.is_none() {
                    return Err(ValidationError::MissingShellOverlayId);
                }
                *shell_overlays += 1;
                if *shell_overlays > 1 {
                    return Err(ValidationError::DuplicateShellOverlay);
                }
                if !valid_scene_number(overlay.x)
                    || !valid_scene_number(overlay.y)
                    || !valid_dimension(overlay.width)
                    || !valid_dimension(overlay.height)
                    || !(48.0..=720.0).contains(&overlay.width)
                    || !(48.0..=360.0).contains(&overlay.height)
                {
                    return Err(ValidationError::InvalidDimension("shell overlay"));
                }
                if overlay.anchor.is_some_and(|anchor| {
                    !valid_scene_number(anchor.x)
                        || !valid_scene_number(anchor.y)
                        || !valid_dimension(anchor.width)
                        || !valid_dimension(anchor.height)
                        || anchor.width > overlay.width
                        || anchor.height > overlay.height
                }) {
                    return Err(ValidationError::InvalidDimension("shell overlay anchor"));
                }
            }
            Some(Content::ApplicationSurface(application)) => {
                if node.id.is_none() {
                    return Err(ValidationError::MissingApplicationSurfaceId);
                }
                *application_surfaces += 1;
                if *application_surfaces > 1 {
                    return Err(ValidationError::DuplicateApplicationSurface);
                }
                if application.title.is_empty() || application.title.len() > 256 {
                    return Err(ValidationError::InvalidApplicationSurfaceTitle);
                }
            }
            _ => {}
        }
        if (node.layout.scroll_y
            || node.interaction.tap_action.is_some()
            || node.interaction.hover_action.is_some()
            || node.interaction.surface_drag
            || node.interaction.double_tap_action.is_some()
            || node.interaction.long_press_action.is_some()
            || node.interaction.swipe_action.is_some()
            || node.interaction.pointer_action.is_some()
            || node.interaction.multi_pointer_action.is_some()
            || !node.interaction.hit_regions.is_empty()
            || node.animation.is_some())
            && node.id.is_none()
        {
            return Err(ValidationError::MissingInteractiveId);
        }
        if let Some(id) = &node.id {
            if !ids.insert(id.clone()) {
                return Err(ValidationError::DuplicateId(id.clone()));
            }
        }
        for (name, value) in [
            ("padding", node.layout.padding),
            ("gap", node.layout.gap),
            ("width", node.layout.width),
            ("height", node.layout.height),
            ("min width", node.layout.min_width),
            ("min height", node.layout.min_height),
            ("max width", node.layout.max_width),
            ("max height", node.layout.max_height),
        ] {
            if value.is_some_and(|value| !valid_dimension(value)) {
                return Err(ValidationError::InvalidDimension(name));
            }
        }
        if node
            .layout
            .aspect_ratio
            .is_some_and(|value| !valid_dimension(value) || value <= 0.0)
        {
            return Err(ValidationError::InvalidDimension("aspect ratio"));
        }
        if let Some(position) = node.layout.position {
            if !valid_scene_number(position.x) || !valid_scene_number(position.y) {
                return Err(ValidationError::InvalidDimension("layout position"));
            }
        }
        if node.layout.program.is_some_and(|program| {
            [
                program.measure_width,
                program.measure_height,
                program.arrange_x,
                program.arrange_y,
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || !(-4.0..=4.0).contains(&value))
        }) {
            return Err(ValidationError::InvalidLayoutProgram);
        }
        for child in &node.children {
            visit(
                child,
                depth + 1,
                count,
                ids,
                window_spaces,
                shell_overlays,
                application_surfaces,
            )?;
        }
        if let Some(animation) = &node.animation {
            if !(16..=60_000).contains(&animation.duration_ms) {
                return Err(ValidationError::InvalidAnimationDuration);
            }
        }
        if node.interaction.hit_regions.len() > MAX_HIT_REGIONS {
            return Err(ValidationError::TooManyHitRegions);
        }
        let mut paint_operations = 0;
        let mut paint_points = 0;
        let mut glyph_runs = 0;
        validate_paint(
            &node.paint,
            1,
            &mut paint_operations,
            &mut paint_points,
            &mut glyph_runs,
        )?;
        for region in &node.interaction.hit_regions {
            if region.id.is_empty()
                || [region.x, region.y, region.width, region.height]
                    .into_iter()
                    .any(|value| !valid_scene_number(value))
                || region.width <= 0.0
                || region.height <= 0.0
            {
                return Err(ValidationError::InvalidDimension("hit region"));
            }
        }
        if let Some(semantics) = &node.semantics {
            if node.id.is_none() {
                return Err(ValidationError::MissingInteractiveId);
            }
            if semantics.label.len() > MAX_TEXT_BYTES
                || semantics
                    .value
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_TEXT_BYTES)
                || semantics
                    .hint
                    .as_ref()
                    .is_some_and(|hint| hint.len() > MAX_TEXT_BYTES)
            {
                return Err(ValidationError::SemanticTextTooLong);
            }
        }
        Ok(())
    }

    let mut count = 0;
    let mut window_spaces = 0;
    let mut shell_overlays = 0;
    let mut application_surfaces = 0;
    visit(
        &scene.root,
        1,
        &mut count,
        &mut HashSet::new(),
        &mut window_spaces,
        &mut shell_overlays,
        &mut application_surfaces,
    )?;
    Ok(count)
}

fn validate_paint(
    operations: &[PaintOp],
    depth: usize,
    operation_count: &mut usize,
    point_count: &mut usize,
    glyph_run_count: &mut usize,
) -> Result<(), ValidationError> {
    if depth > MAX_PAINT_DEPTH {
        return Err(ValidationError::PaintTooDeep);
    }
    for operation in operations {
        *operation_count += 1;
        if *operation_count > MAX_PAINT_OPS {
            return Err(ValidationError::TooManyPaintOps);
        }
        match operation {
            PaintOp::FillBounds { radius, .. } => {
                if !valid_scene_number(*radius) || *radius < 0.0 {
                    return Err(ValidationError::InvalidDimension("fill bounds"));
                }
            }
            PaintOp::Path { points, width, .. } => {
                *point_count += points.len();
                if *point_count > MAX_PAINT_POINTS {
                    return Err(ValidationError::TooManyPaintPoints);
                }
                if points.len() < 2
                    || points
                        .iter()
                        .any(|point| !valid_scene_number(point.x) || !valid_scene_number(point.y))
                    || width.is_some_and(|width| !valid_scene_number(width) || width <= 0.0)
                {
                    return Err(ValidationError::InvalidDimension("paint path"));
                }
            }
            PaintOp::Quad {
                x,
                y,
                width,
                height,
                radius,
                ..
            } => {
                if [*x, *y, *width, *height, *radius]
                    .into_iter()
                    .any(|value| !valid_scene_number(value))
                    || *width <= 0.0
                    || *height <= 0.0
                    || *radius < 0.0
                {
                    return Err(ValidationError::InvalidDimension("paint quad"));
                }
            }
            PaintOp::Glyphs {
                x,
                y,
                size,
                line_height,
                max_width,
                runs,
            } => {
                *glyph_run_count += runs.len();
                if *glyph_run_count > MAX_GLYPH_RUNS {
                    return Err(ValidationError::TooManyGlyphRuns);
                }
                if runs.is_empty()
                    || !valid_scene_number(*x)
                    || !valid_scene_number(*y)
                    || !valid_dimension(*size)
                    || *size <= 0.0
                    || line_height.is_some_and(|value| !valid_dimension(value) || value <= 0.0)
                    || max_width.is_some_and(|value| !valid_dimension(value) || value <= 0.0)
                    || runs.iter().any(|run| {
                        run.text.len() > MAX_TEXT_BYTES
                            || run
                                .font_family
                                .as_ref()
                                .is_some_and(|family| family.len() > 256)
                            || !(100..=900).contains(&run.weight)
                    })
                {
                    return Err(ValidationError::InvalidDimension("paint glyphs"));
                }
            }
            PaintOp::Shader {
                asset,
                x,
                y,
                width,
                height,
            } => {
                if asset.is_empty()
                    || asset.len() > 256
                    || [*x, *y, *width, *height]
                        .into_iter()
                        .any(|value| !valid_scene_number(value))
                    || *width <= 0.0
                    || *height <= 0.0
                    || *width > 1024.0
                    || *height > 1024.0
                {
                    return Err(ValidationError::InvalidDimension("paint shader"));
                }
            }
            PaintOp::Layer {
                clip,
                transform,
                opacity,
                operations,
            } => {
                if !opacity.is_finite()
                    || !(0.0..=1.0).contains(opacity)
                    || [
                        transform.translate_x,
                        transform.translate_y,
                        transform.scale_x,
                        transform.scale_y,
                        transform.rotation_degrees,
                    ]
                    .into_iter()
                    .any(|value| !valid_scene_number(value))
                    || transform.scale_x == 0.0
                    || transform.scale_y == 0.0
                    || clip.is_some_and(|clip| {
                        [clip.x, clip.y, clip.width, clip.height]
                            .into_iter()
                            .any(|value| !valid_scene_number(value))
                            || clip.width <= 0.0
                            || clip.height <= 0.0
                    })
                {
                    return Err(ValidationError::InvalidDimension("paint layer"));
                }
                validate_paint(
                    operations,
                    depth + 1,
                    operation_count,
                    point_count,
                    glyph_run_count,
                )?;
            }
        }
    }
    Ok(())
}

fn valid_dimension(value: f32) -> bool {
    value.is_finite() && (0.0..=10_000.0).contains(&value)
}

fn valid_scene_number(value: f32) -> bool {
    value.is_finite() && (-10_000.0..=10_000.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_configuration_actions_have_stable_wire_names() {
        assert_eq!(
            serde_json::to_value([
                AgentConfigurationAction::ConfigureOpenAi,
                AgentConfigurationAction::ConfigureOpenRouter,
                AgentConfigurationAction::ConfigureCodex,
                AgentConfigurationAction::UseFake,
                AgentConfigurationAction::ClearCredential,
            ])
            .unwrap(),
            serde_json::json!([
                "configure_openai",
                "configure_openrouter",
                "configure_codex",
                "use_fake",
                "clear_credential"
            ])
        );
    }

    #[test]
    fn rejects_duplicate_ids() {
        let scene = Scene {
            root: SceneNode {
                children: vec![
                    SceneNode {
                        id: Some("same".into()),
                        ..Default::default()
                    },
                    SceneNode {
                        id: Some("same".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        };
        assert_eq!(
            validate_scene(&scene),
            Err(ValidationError::DuplicateId("same".into()))
        );
    }

    #[test]
    fn keyed_native_nodes_require_stable_ids() {
        let scene = Scene {
            root: SceneNode {
                content: Some(Content::TextSession(TextSession {
                    state_key: "draft".into(),
                    value: String::new(),
                    placeholder: String::new(),
                    submit_action: None,
                    autofocus: false,
                })),
                ..Default::default()
            },
        };
        assert_eq!(
            validate_scene(&scene),
            Err(ValidationError::MissingInteractiveId)
        );
    }

    #[test]
    fn window_space_is_single_bounded_and_keyed() {
        let window_space = || SceneNode {
            id: Some("applications".into()),
            content: Some(Content::WindowSpace(WindowSpaceContent {
                layout: WindowLayoutMode::Floating,
                gap: 12.0,
                fallback: "No applications are open".into(),
            })),
            ..Default::default()
        };
        assert_eq!(
            validate_scene(&Scene {
                root: window_space()
            }),
            Ok(1)
        );

        let mut unkeyed = window_space();
        unkeyed.id = None;
        assert_eq!(
            validate_scene(&Scene { root: unkeyed }),
            Err(ValidationError::MissingWindowSpaceId)
        );

        let second = SceneNode {
            id: Some("applications-two".into()),
            ..window_space()
        };
        assert_eq!(
            validate_scene(&Scene {
                root: SceneNode {
                    children: vec![window_space(), second],
                    ..Default::default()
                }
            }),
            Err(ValidationError::DuplicateWindowSpace)
        );
    }

    #[test]
    fn shell_overlay_and_application_surface_are_single_keyed_primitives() {
        let overlay = || SceneNode {
            id: Some("agent-overlay".into()),
            content: Some(Content::ShellOverlay(ShellOverlayContent {
                x: 20.0,
                y: 30.0,
                width: 64.0,
                height: 64.0,
                anchor: None,
            })),
            interaction: Interaction {
                hover_action: Some("overlay_hover".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let application = || SceneNode {
            id: Some("notes".into()),
            content: Some(Content::ApplicationSurface(ApplicationSurfaceContent {
                title: "Notes".into(),
            })),
            ..Default::default()
        };
        assert_eq!(
            validate_scene(&Scene {
                root: SceneNode {
                    children: vec![overlay(), application()],
                    ..Default::default()
                }
            }),
            Ok(3)
        );

        let second_overlay = SceneNode {
            id: Some("second-overlay".into()),
            ..overlay()
        };
        assert_eq!(
            validate_scene(&Scene {
                root: SceneNode {
                    children: vec![overlay(), second_overlay],
                    ..Default::default()
                }
            }),
            Err(ValidationError::DuplicateShellOverlay)
        );

        let second_application = SceneNode {
            id: Some("calendar".into()),
            ..application()
        };
        assert_eq!(
            validate_scene(&Scene {
                root: SceneNode {
                    children: vec![application(), second_application],
                    ..Default::default()
                }
            }),
            Err(ValidationError::DuplicateApplicationSurface)
        );
    }

    #[test]
    fn rejects_animation_outside_the_bounded_range() {
        let scene = Scene {
            root: SceneNode {
                id: Some("animated".into()),
                animation: Some(Animation {
                    kind: AnimationKind::Pulse,
                    duration_ms: 15,
                    repeat: true,
                }),
                ..Default::default()
            },
        };
        assert_eq!(
            validate_scene(&scene),
            Err(ValidationError::InvalidAnimationDuration)
        );
    }

    #[test]
    fn orthogonal_facets_can_share_one_node() {
        let scene = Scene {
            root: SceneNode {
                id: Some("invented-control".into()),
                layout: Layout {
                    flow: Flow::Row,
                    width: Some(240.0),
                    height: Some(80.0),
                    ..Default::default()
                },
                content: Some(Content::Text(TextContent {
                    value: "Drag me".into(),
                    size: 18.0,
                    color: 0xffffff,
                })),
                paint: vec![PaintOp::FillBounds {
                    color: 0x223344,
                    radius: 20.0,
                }],
                interaction: Interaction {
                    tap_action: Some("activate".into()),
                    ..Default::default()
                },
                semantics: Some(Semantics {
                    role: SemanticRole::Button,
                    label: "Invented control".into(),
                    value: None,
                    hint: None,
                }),
                ..Default::default()
            },
        };
        assert_eq!(validate_scene(&scene), Ok(1));
    }
}
