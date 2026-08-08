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
    Prepare {
        request_id: u64,
        revision_id: String,
        revision_path: PathBuf,
        experience_api_version: u32,
    },
    Present {
        request_id: u64,
        revision_id: String,
    },
    Confirm {
        request_id: u64,
        revision_id: String,
    },
    Discard {
        request_id: u64,
        revision_id: String,
    },
    Shutdown {
        request_id: u64,
    },
}

impl HostRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::Boot { request_id, .. }
            | Self::Prepare { request_id, .. }
            | Self::Present { request_id, .. }
            | Self::Confirm { request_id, .. }
            | Self::Discard { request_id, .. }
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
    Presented {
        request_id: u64,
        revision_id: String,
    },
    Confirmed {
        request_id: u64,
        revision_id: String,
    },
    Discarded {
        request_id: u64,
        revision_id: String,
    },
    Rejected {
        request_id: u64,
        revision_id: String,
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
            | Self::Presented { request_id, .. }
            | Self::Confirmed { request_id, .. }
            | Self::Discarded { request_id, .. }
            | Self::Rejected { request_id, .. }
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
