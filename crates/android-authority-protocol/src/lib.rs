use experience_ir::StateEnvelope;
use serde::{Deserialize, Serialize};

pub const REVISION_ADDRESS: &str = "127.0.0.1:47778";
// RevisionAssetWire uses serde's JSON byte-array representation. The runtime's
// 16 MiB raw sidecar ceiling can therefore expand to roughly 64 MiB on the
// wire; retain an explicit bound with enough headroom for source and metadata.
pub const MAX_REVISION_REQUEST_BYTES: u64 = 96 * 1024 * 1024;
pub const MAX_REVISION_RESPONSE_BYTES: u64 = 96 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RevisionAssetWire {
    pub id: String,
    pub kind: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RevisionRequest {
    Current {
        request_id: u64,
    },
    Install {
        request_id: u64,
        source: String,
        state: serde_json::Value,
        schema_version: u64,
        experience_api_version: u32,
        assets: Vec<RevisionAssetWire>,
    },
    Activate {
        request_id: u64,
        revision_id: String,
        state_stage_id: u64,
    },
}

impl RevisionRequest {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::Current { request_id }
            | Self::Install { request_id, .. }
            | Self::Activate { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct RevisionResponse {
    pub request_id: u64,
    pub ok: bool,
    pub revision_id: Option<String>,
    pub source: Option<String>,
    pub state: Option<StateEnvelope>,
    pub assets: Vec<RevisionAssetWire>,
    pub error: Option<String>,
}
