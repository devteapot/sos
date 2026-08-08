use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_TREE_DEPTH: usize = 32;
pub const MAX_TREE_NODES: usize = 2_048;
pub const MAX_CHILDREN: usize = 256;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UiEvent {
    pub action: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiNode {
    pub id: Option<String>,
    pub kind: NodeKind,
    pub style: Style,
    pub action: Option<String>,
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
    Spacer,
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
        if let NodeKind::Text(text) = &node.kind {
            if text.len() > MAX_TEXT_BYTES {
                return Err(ValidationError::TextTooLong);
            }
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
        Ok(())
    }

    let mut count = 0;
    visit(root, 1, &mut count, &mut HashSet::new())?;
    Ok(count)
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
}
