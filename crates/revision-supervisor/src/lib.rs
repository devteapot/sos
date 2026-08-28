mod graph;
mod graph_supervisor;
mod host;
mod reference;
mod registry;
mod reverse_index;
mod store;

use std::{path::PathBuf, process::ExitStatus, time::Duration};

pub use experience_host_protocol::{ExperienceLifecycleOperation, HostEvent, HostRequest};
pub use graph::{GraphResolver, GraphStore};
pub use graph_supervisor::{
    ExperienceAdvance, ExperienceGraphAdvance, ExperienceGraphSupervisor,
    GraphActivationFaultPoint, GraphActivationJournal, GraphActivationPhase, GraphPointerUpdate,
    PreparedGraphActivation, PreparedGraphSetActivation, RegistryPointerUpdate,
};
pub use host::{ExperienceHost, HostCommand};
pub use reference::{install_reference_composition, ReferenceComposition};
pub use registry::{
    is_pinned_stock_experience, ExperienceRecord, ExperienceRegistry, STOCK_MOBILE_EXPERIENCE_ID,
    STOCK_SHELL_EXPERIENCE_ID,
};
pub use reverse_index::{ReverseDependencyData, ReverseDependencyIndex};
pub use store::{
    AssetIdentity, DurableState, FileIdentity, RevisionAssetInput, RevisionInput, RevisionManifest,
    RevisionPackageInput, RevisionStore, VerifiedRevision, MAX_REVISION_ASSETS,
    MAX_REVISION_ASSET_BYTES, MAX_REVISION_ASSET_TOTAL_BYTES,
};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid revision id: {0}")]
    InvalidRevisionId(String),
    #[error("invalid revision: {0}")]
    InvalidRevision(String),
    #[error("invalid current pointer: {0}")]
    InvalidPointer(PathBuf),
    #[error("invalid experience registry: {0}")]
    InvalidRegistry(String),
    #[error("invalid experience graph: {0}")]
    InvalidGraph(String),
    #[error("experience host exited: {0}")]
    HostExited(ExitStatus),
    #[error("experience host did not respond within {0:?}")]
    HostTimeout(Duration),
    #[error("experience host sent an invalid event")]
    InvalidHostEvent,
    #[error("experience host protocol failed: {0}")]
    HostProtocol(String),
    #[error("experience revision was rejected: {0}")]
    HostRejected(String),
    #[error("no current revision is initialized")]
    NoCurrentRevision,
    #[error("the supervisor has no active experience host")]
    NoActiveHost,
    #[error("injected graph activation fault: {0}")]
    InjectedGraphActivationFault(String),
}
