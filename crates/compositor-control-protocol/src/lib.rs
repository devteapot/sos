use std::{
    fs::File,
    io::{self, Read as _},
    path::Path,
};

use serde::{Deserialize, Serialize};

pub const MAX_CONTROL_LINE_BYTES: usize = 8 * 1024;
pub const MAX_SHELL_TOKEN_BYTES: usize = 256;

pub fn valid_shell_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_SHELL_TOKEN_BYTES
        && !token.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
}

pub fn read_shell_token_file(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::with_capacity(MAX_SHELL_TOKEN_BYTES + 1);
    File::open(path)?
        .take((MAX_SHELL_TOKEN_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_SHELL_TOKEN_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compositor shell token exceeds its bound",
        ));
    }
    let token = String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "compositor shell token is not valid UTF-8",
        )
    })?;
    if !valid_shell_token(&token) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid compositor shell token",
        ));
    }
    Ok(token)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CompositorRequest {
    RegisterShell {
        request_id: u64,
        token: String,
        pid: u32,
    },
    ArmPresentation {
        request_id: u64,
        revision_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresentationClock {
    Monotonic,
    Realtime,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresentationEvidence {
    NestedBackendSubmit,
    DrmPageFlip {
        output_sequence: u64,
        timestamp_seconds: u64,
        timestamp_nanoseconds: u32,
        clock: PresentationClock,
    },
}

impl PresentationEvidence {
    pub fn name(&self) -> &'static str {
        match self {
            Self::NestedBackendSubmit => "nested_backend_submit",
            Self::DrmPageFlip { .. } => "drm_page_flip",
        }
    }
}

impl CompositorRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::RegisterShell { request_id, .. } | Self::ArmPresentation { request_id, .. } => {
                *request_id
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CompositorEvent {
    Registered {
        request_id: u64,
        pid: u32,
    },
    Armed {
        request_id: u64,
        revision_id: String,
        after_commit_sequence: u64,
    },
    Presented {
        request_id: u64,
        revision_id: String,
        commit_sequence: u64,
        submit_sequence: u64,
        evidence: PresentationEvidence,
    },
    Rejected {
        request_id: u64,
        error: String,
    },
}

impl CompositorEvent {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::Registered { request_id, .. }
            | Self::Armed { request_id, .. }
            | Self::Presented { request_id, .. }
            | Self::Rejected { request_id, .. } => *request_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn control_wire_is_bounded_newline_json() {
        let request = CompositorRequest::ArmPresentation {
            request_id: 9,
            revision_id: "abc".into(),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"action":"arm_presentation","request_id":9,"revision_id":"abc"}"#
        );
        assert_eq!(request.request_id(), 9);

        let event = CompositorEvent::Presented {
            request_id: 9,
            revision_id: "abc".into(),
            commit_sequence: 3,
            submit_sequence: 5,
            evidence: PresentationEvidence::DrmPageFlip {
                output_sequence: 17,
                timestamp_seconds: 2,
                timestamp_nanoseconds: 3,
                clock: PresentationClock::Monotonic,
            },
        };
        assert_eq!(
            serde_json::from_str::<CompositorEvent>(&serde_json::to_string(&event).unwrap())
                .unwrap(),
            event
        );
        assert_eq!(event.request_id(), 9);
    }

    #[test]
    fn credential_file_preserves_exact_bounded_token() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shell-token");
        fs::write(&path, "credential-secret").unwrap();
        assert_eq!(read_shell_token_file(&path).unwrap(), "credential-secret");

        fs::write(&path, "credential-secret\n").unwrap();
        assert_eq!(
            read_shell_token_file(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        fs::write(&path, vec![b'x'; MAX_SHELL_TOKEN_BYTES + 1]).unwrap();
        assert_eq!(
            read_shell_token_file(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
