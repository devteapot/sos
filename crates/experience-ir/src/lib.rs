use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_TREE_DEPTH: usize = 32;
pub const MAX_TREE_NODES: usize = 2_048;
pub const MAX_CHILDREN: usize = 256;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_CANVAS_COMMANDS: usize = 4_096;
pub const MAX_CANVAS_POINTS: usize = 8_192;
pub const MAX_HIT_REGIONS: usize = 256;
pub const MAX_EFFECTS: usize = 16;
pub const MAX_EFFECT_PAYLOAD_BYTES: usize = 16 * 1024;
pub const MAX_STATE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExperienceModel {
    pub greeting: String,
    pub date: String,
    pub weather: Weather,
    pub calendar: Vec<CalendarEvent>,
    pub notes: Vec<Note>,
    pub music: Music,
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
pub struct UiEvent {
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
pub struct UiNode {
    pub id: Option<String>,
    pub kind: NodeKind,
    pub style: Style,
    pub action: Option<String>,
    pub animation: Option<Animation>,
    pub accessibility: Option<Accessibility>,
    pub children: Vec<UiNode>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum NodeKind {
    #[default]
    Box,
    Column,
    Row,
    Scroll,
    Text(String),
    TextInput(TextInput),
    Image(Image),
    Canvas(Canvas),
    Spacer,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextInput {
    pub state_key: String,
    pub value: String,
    pub placeholder: String,
    pub submit_action: Option<String>,
    pub autofocus: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub asset: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Canvas {
    pub commands: Vec<CanvasCommand>,
    pub hit_regions: Vec<HitRegion>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CanvasCommand {
    Path {
        points: Vec<CanvasPoint>,
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasPoint {
    pub x: f32,
    pub y: f32,
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
pub struct Accessibility {
    pub role: AccessibilityRole,
    pub label: String,
    pub value: Option<String>,
    pub hint: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AccessibilityRole {
    Button,
    Image,
    TextField,
    Header,
    Status,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub background: Option<u32>,
    pub color: Option<u32>,
    pub padding: Option<f32>,
    pub gap: Option<f32>,
    pub radius: Option<f32>,
    pub text_size: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub grow: bool,
    pub align: Option<Align>,
    pub justify: Option<Justify>,
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
    #[error("tree exceeds maximum depth of {MAX_TREE_DEPTH}")]
    TooDeep,
    #[error("tree exceeds maximum node count of {MAX_TREE_NODES}")]
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
    #[error("canvas exceeds maximum command count of {MAX_CANVAS_COMMANDS}")]
    TooManyCanvasCommands,
    #[error("canvas exceeds maximum point count of {MAX_CANVAS_POINTS}")]
    TooManyCanvasPoints,
    #[error("canvas exceeds maximum hit-region count of {MAX_HIT_REGIONS}")]
    TooManyHitRegions,
}

pub fn validate_tree(root: &UiNode) -> Result<usize, ValidationError> {
    fn visit(
        node: &UiNode,
        depth: usize,
        count: &mut usize,
        ids: &mut HashSet<String>,
    ) -> Result<(), ValidationError> {
        if depth > MAX_TREE_DEPTH {
            return Err(ValidationError::TooDeep);
        }
        *count += 1;
        if *count > MAX_TREE_NODES {
            return Err(ValidationError::TooManyNodes);
        }
        if node.children.len() > MAX_CHILDREN {
            return Err(ValidationError::TooManyChildren);
        }
        match &node.kind {
            NodeKind::Text(text) if text.len() > MAX_TEXT_BYTES => {
                return Err(ValidationError::TextTooLong);
            }
            NodeKind::TextInput(input) => {
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
            NodeKind::Image(_) | NodeKind::Canvas(_) if node.id.is_none() => {
                return Err(ValidationError::MissingInteractiveId);
            }
            _ => {}
        }
        if let Some(id) = &node.id {
            if !ids.insert(id.clone()) {
                return Err(ValidationError::DuplicateId(id.clone()));
            }
        }
        for (name, value) in [
            ("padding", node.style.padding),
            ("gap", node.style.gap),
            ("radius", node.style.radius),
            ("text_size", node.style.text_size),
            ("width", node.style.width),
            ("height", node.style.height),
        ] {
            if value.is_some_and(|value| !(0.0..=10_000.0).contains(&value)) {
                return Err(ValidationError::InvalidDimension(name));
            }
        }
        for child in &node.children {
            visit(child, depth + 1, count, ids)?;
        }
        if let Some(animation) = &node.animation {
            if !(16..=60_000).contains(&animation.duration_ms) {
                return Err(ValidationError::InvalidAnimationDuration);
            }
        }
        if let NodeKind::Canvas(canvas) = &node.kind {
            if canvas.commands.len() > MAX_CANVAS_COMMANDS {
                return Err(ValidationError::TooManyCanvasCommands);
            }
            if canvas.hit_regions.len() > MAX_HIT_REGIONS {
                return Err(ValidationError::TooManyHitRegions);
            }
            let point_count = canvas
                .commands
                .iter()
                .map(|command| match command {
                    CanvasCommand::Path { points, .. } => points.len(),
                    CanvasCommand::Quad { .. } => 0,
                })
                .sum::<usize>();
            if point_count > MAX_CANVAS_POINTS {
                return Err(ValidationError::TooManyCanvasPoints);
            }
            for command in &canvas.commands {
                match command {
                    CanvasCommand::Path { points, width, .. } => {
                        if points.len() < 2
                            || points.iter().any(|point| {
                                !valid_canvas_number(point.x) || !valid_canvas_number(point.y)
                            })
                            || width
                                .is_some_and(|width| !valid_canvas_number(width) || width <= 0.0)
                        {
                            return Err(ValidationError::InvalidDimension("canvas path"));
                        }
                    }
                    CanvasCommand::Quad {
                        x,
                        y,
                        width,
                        height,
                        radius,
                        ..
                    } => {
                        if [*x, *y, *width, *height, *radius]
                            .into_iter()
                            .any(|value| !valid_canvas_number(value))
                            || *width <= 0.0
                            || *height <= 0.0
                            || *radius < 0.0
                        {
                            return Err(ValidationError::InvalidDimension("canvas quad"));
                        }
                    }
                }
            }
            for region in &canvas.hit_regions {
                if region.id.is_empty()
                    || [region.x, region.y, region.width, region.height]
                        .into_iter()
                        .any(|value| !valid_canvas_number(value))
                    || region.width <= 0.0
                    || region.height <= 0.0
                {
                    return Err(ValidationError::InvalidDimension("canvas hit region"));
                }
            }
        }
        if let Some(accessibility) = &node.accessibility {
            if accessibility.label.len() > MAX_TEXT_BYTES
                || accessibility
                    .value
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_TEXT_BYTES)
                || accessibility
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
    visit(root, 1, &mut count, &mut HashSet::new())?;
    Ok(count)
}

fn valid_canvas_number(value: f32) -> bool {
    value.is_finite() && (-10_000.0..=10_000.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_ids() {
        let node = UiNode {
            children: vec![
                UiNode {
                    id: Some("same".into()),
                    ..Default::default()
                },
                UiNode {
                    id: Some("same".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            validate_tree(&node),
            Err(ValidationError::DuplicateId("same".into()))
        );
    }

    #[test]
    fn keyed_native_nodes_require_stable_ids() {
        let input = UiNode {
            kind: NodeKind::TextInput(TextInput {
                state_key: "draft".into(),
                value: String::new(),
                placeholder: String::new(),
                submit_action: None,
                autofocus: false,
            }),
            ..Default::default()
        };
        assert_eq!(
            validate_tree(&input),
            Err(ValidationError::MissingInteractiveId)
        );

        let image = UiNode {
            kind: NodeKind::Image(Image {
                asset: "album-orbit".into(),
            }),
            ..Default::default()
        };
        assert_eq!(
            validate_tree(&image),
            Err(ValidationError::MissingInteractiveId)
        );
    }

    #[test]
    fn rejects_animation_outside_the_bounded_range() {
        let node = UiNode {
            animation: Some(Animation {
                kind: AnimationKind::Pulse,
                duration_ms: 15,
                repeat: true,
            }),
            ..Default::default()
        };
        assert_eq!(
            validate_tree(&node),
            Err(ValidationError::InvalidAnimationDuration)
        );
    }
}
