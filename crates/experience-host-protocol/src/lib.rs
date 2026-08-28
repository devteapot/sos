use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const MAX_LAUNCHABLE_EXPERIENCES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TopLevelExperience {
    pub experience_id: String,
    pub title: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExperienceLifecycleOperation {
    Present,
    Dismiss,
}

/// Protocol spoken over a permanent experience host's stdin and stdout.
///
/// Stdout is reserved for newline-delimited serialized [`HostEvent`] values.
/// Hosts must send diagnostics to stderr so they cannot corrupt this stream.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HostRequest {
    BootGraph {
        request_id: u64,
        graph_id: String,
        graph_path: PathBuf,
        revision_root: PathBuf,
        #[serde(default)]
        launchable_experiences: Vec<TopLevelExperience>,
    },
    PrepareGraph {
        request_id: u64,
        graph_id: String,
        graph_path: PathBuf,
        revision_root: PathBuf,
        #[serde(default)]
        launchable_experiences: Vec<TopLevelExperience>,
    },
    QuiesceGraphInput {
        request_id: u64,
        graph_id: String,
    },
    PresentGraph {
        request_id: u64,
        graph_id: String,
    },
    ConfirmGraph {
        request_id: u64,
        graph_id: String,
    },
    FinalizeGraph {
        request_id: u64,
        graph_id: String,
    },
    DiscardGraph {
        request_id: u64,
        graph_id: String,
    },
    Shutdown {
        request_id: u64,
    },
}

impl HostRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::BootGraph { request_id, .. }
            | Self::PrepareGraph { request_id, .. }
            | Self::QuiesceGraphInput { request_id, .. }
            | Self::PresentGraph { request_id, .. }
            | Self::ConfirmGraph { request_id, .. }
            | Self::FinalizeGraph { request_id, .. }
            | Self::DiscardGraph { request_id, .. }
            | Self::Shutdown { request_id } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HostEvent {
    ExperienceLifecycleRequested {
        request_id: u64,
        experience_id: String,
        operation: ExperienceLifecycleOperation,
    },
    GraphPrepared {
        request_id: u64,
        graph_id: String,
    },
    GraphInputQuiesced {
        request_id: u64,
        graph_id: String,
    },
    GraphPresented {
        request_id: u64,
        graph_id: String,
    },
    GraphConfirmed {
        request_id: u64,
        graph_id: String,
    },
    GraphFinalized {
        request_id: u64,
        graph_id: String,
    },
    GraphDiscarded {
        request_id: u64,
        graph_id: String,
    },
    GraphRejected {
        request_id: u64,
        graph_id: String,
        error: String,
    },
    Shutdown {
        request_id: u64,
    },
}

impl HostEvent {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::ExperienceLifecycleRequested { request_id, .. }
            | Self::GraphPrepared { request_id, .. }
            | Self::GraphInputQuiesced { request_id, .. }
            | Self::GraphPresented { request_id, .. }
            | Self::GraphConfirmed { request_id, .. }
            | Self::GraphFinalized { request_id, .. }
            | Self::GraphDiscarded { request_id, .. }
            | Self::GraphRejected { request_id, .. }
            | Self::Shutdown { request_id } => *request_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_remains_newline_json_compatible() {
        let request = HostRequest::PrepareGraph {
            request_id: 7,
            graph_id: "abc".into(),
            graph_path: PathBuf::from("/var/lib/sos/graphs/abc.json"),
            revision_root: PathBuf::from("/var/lib/sos"),
            launchable_experiences: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"action":"prepare_graph","request_id":7,"graph_id":"abc","graph_path":"/var/lib/sos/graphs/abc.json","revision_root":"/var/lib/sos","launchable_experiences":[]}"#
        );
        assert_eq!(request.request_id(), 7);

        let quiesce = HostRequest::QuiesceGraphInput {
            request_id: 8,
            graph_id: "abc".into(),
        };
        assert_eq!(
            serde_json::to_string(&quiesce).unwrap(),
            r#"{"action":"quiesce_graph_input","request_id":8,"graph_id":"abc"}"#
        );
        assert_eq!(quiesce.request_id(), 8);

        let quiesced = HostEvent::GraphInputQuiesced {
            request_id: 8,
            graph_id: "abc".into(),
        };
        assert_eq!(
            serde_json::from_str::<HostEvent>(&serde_json::to_string(&quiesced).unwrap()).unwrap(),
            quiesced
        );

        let event = HostEvent::GraphRejected {
            request_id: 7,
            graph_id: "abc".into(),
            error: "invalid source".into(),
        };
        assert_eq!(event.request_id(), 7);
        assert_eq!(
            serde_json::from_str::<HostEvent>(&serde_json::to_string(&event).unwrap()).unwrap(),
            event
        );
    }

    #[test]
    fn graph_protocol_rejects_retired_single_revision_actions() {
        for action in ["boot", "prepare", "present", "confirm", "discard"] {
            let wire = format!(r#"{{"action":"{action}","request_id":1}}"#);
            assert!(serde_json::from_str::<HostRequest>(&wire).is_err());
        }
    }
}
