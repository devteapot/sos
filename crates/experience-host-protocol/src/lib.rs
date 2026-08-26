use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Protocol spoken over a permanent experience host's stdin and stdout.
///
/// Stdout is reserved for newline-delimited serialized [`HostEvent`] values.
/// Hosts must send diagnostics to stderr so they cannot corrupt this stream.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HostRequest {
    Boot {
        request_id: u64,
        revision_id: String,
        revision_path: PathBuf,
        experience_api_version: u32,
    },
    BootGraph {
        request_id: u64,
        graph_id: String,
        graph_path: PathBuf,
        revision_root: PathBuf,
    },
    Prepare {
        request_id: u64,
        revision_id: String,
        revision_path: PathBuf,
        experience_api_version: u32,
    },
    PrepareGraph {
        request_id: u64,
        graph_id: String,
        graph_path: PathBuf,
        revision_root: PathBuf,
    },
    QuiesceInput {
        request_id: u64,
        revision_id: String,
    },
    QuiesceGraphInput {
        request_id: u64,
        graph_id: String,
    },
    Present {
        request_id: u64,
        revision_id: String,
    },
    PresentGraph {
        request_id: u64,
        graph_id: String,
    },
    Confirm {
        request_id: u64,
        revision_id: String,
    },
    ConfirmGraph {
        request_id: u64,
        graph_id: String,
    },
    FinalizeGraph {
        request_id: u64,
        graph_id: String,
    },
    Discard {
        request_id: u64,
        revision_id: String,
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
            Self::Boot { request_id, .. }
            | Self::BootGraph { request_id, .. }
            | Self::Prepare { request_id, .. }
            | Self::PrepareGraph { request_id, .. }
            | Self::QuiesceInput { request_id, .. }
            | Self::QuiesceGraphInput { request_id, .. }
            | Self::Present { request_id, .. }
            | Self::PresentGraph { request_id, .. }
            | Self::Confirm { request_id, .. }
            | Self::ConfirmGraph { request_id, .. }
            | Self::FinalizeGraph { request_id, .. }
            | Self::Discard { request_id, .. }
            | Self::DiscardGraph { request_id, .. }
            | Self::Shutdown { request_id } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HostEvent {
    Prepared {
        request_id: u64,
        revision_id: String,
    },
    GraphPrepared {
        request_id: u64,
        graph_id: String,
    },
    InputQuiesced {
        request_id: u64,
        revision_id: String,
    },
    GraphInputQuiesced {
        request_id: u64,
        graph_id: String,
    },
    Presented {
        request_id: u64,
        revision_id: String,
    },
    GraphPresented {
        request_id: u64,
        graph_id: String,
    },
    Confirmed {
        request_id: u64,
        revision_id: String,
    },
    GraphConfirmed {
        request_id: u64,
        graph_id: String,
    },
    GraphFinalized {
        request_id: u64,
        graph_id: String,
    },
    Discarded {
        request_id: u64,
        revision_id: String,
    },
    GraphDiscarded {
        request_id: u64,
        graph_id: String,
    },
    Rejected {
        request_id: u64,
        revision_id: String,
        error: String,
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
            Self::Prepared { request_id, .. }
            | Self::GraphPrepared { request_id, .. }
            | Self::InputQuiesced { request_id, .. }
            | Self::GraphInputQuiesced { request_id, .. }
            | Self::Presented { request_id, .. }
            | Self::GraphPresented { request_id, .. }
            | Self::Confirmed { request_id, .. }
            | Self::GraphConfirmed { request_id, .. }
            | Self::GraphFinalized { request_id, .. }
            | Self::Discarded { request_id, .. }
            | Self::GraphDiscarded { request_id, .. }
            | Self::Rejected { request_id, .. }
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
        let request = HostRequest::Prepare {
            request_id: 7,
            revision_id: "abc".into(),
            revision_path: PathBuf::from("/var/lib/sos/revisions/abc"),
            experience_api_version: 3,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"action":"prepare","request_id":7,"revision_id":"abc","revision_path":"/var/lib/sos/revisions/abc","experience_api_version":3}"#
        );
        assert_eq!(request.request_id(), 7);

        let quiesce = HostRequest::QuiesceInput {
            request_id: 8,
            revision_id: "abc".into(),
        };
        assert_eq!(
            serde_json::to_string(&quiesce).unwrap(),
            r#"{"action":"quiesce_input","request_id":8,"revision_id":"abc"}"#
        );
        assert_eq!(quiesce.request_id(), 8);

        let quiesced = HostEvent::InputQuiesced {
            request_id: 8,
            revision_id: "abc".into(),
        };
        assert_eq!(
            serde_json::from_str::<HostEvent>(&serde_json::to_string(&quiesced).unwrap()).unwrap(),
            quiesced
        );

        let event = HostEvent::Rejected {
            request_id: 7,
            revision_id: "abc".into(),
            error: "invalid source".into(),
        };
        assert_eq!(event.request_id(), 7);
        assert_eq!(
            serde_json::from_str::<HostEvent>(&serde_json::to_string(&event).unwrap()).unwrap(),
            event
        );
    }
}
