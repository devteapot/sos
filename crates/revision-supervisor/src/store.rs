use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

const FORMAT_VERSION: u32 = 2;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct RevisionStore {
    root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct RevisionInput {
    pub source: Vec<u8>,
    pub state: Value,
    pub schema_version: u64,
    pub experience_api_version: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FileIdentity {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RevisionManifest {
    pub format_version: u32,
    pub revision_id: String,
    pub schema_version: u64,
    pub experience_api_version: u32,
    pub source: FileIdentity,
    pub state: FileIdentity,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct DurableState {
    pub schema_version: u64,
    pub source_sha256: String,
    pub state: Value,
}

#[derive(Clone, Debug)]
pub struct VerifiedRevision {
    pub directory: PathBuf,
    pub manifest: RevisionManifest,
}

impl RevisionStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("revisions"))?;
        fs::create_dir_all(root.join("run"))?;
        sync_directory(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn install(&self, input: RevisionInput) -> Result<VerifiedRevision> {
        if input.schema_version == 0 {
            return Err(Error::InvalidRevision(
                "schema version must be positive".into(),
            ));
        }
        if input.experience_api_version == 0 {
            return Err(Error::InvalidRevision(
                "experience API version must be positive".into(),
            ));
        }
        let source_sha256 = digest(&input.source);
        let durable_state = DurableState {
            schema_version: input.schema_version,
            source_sha256: source_sha256.clone(),
            state: input.state,
        };
        let state = serde_json::to_vec(&durable_state)?;
        let source = identity("source.luau", &input.source);
        let state_identity = identity("state.json", &state);

        let revision_id = revision_identity(
            input.schema_version,
            input.experience_api_version,
            &source,
            &state_identity,
        );
        let manifest = RevisionManifest {
            format_version: FORMAT_VERSION,
            revision_id: revision_id.clone(),
            schema_version: input.schema_version,
            experience_api_version: input.experience_api_version,
            source,
            state: state_identity,
        };
        let destination = self.revision_path(&revision_id)?;
        if destination.exists() {
            return self.verify(&revision_id);
        }

        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = self.root.join("revisions").join(format!(
            ".staging-{}-{}-{sequence}",
            std::process::id(),
            &revision_id[..12]
        ));
        fs::create_dir(&temporary)?;
        let result = (|| {
            write_synced(&temporary.join("source.luau"), &input.source, 0o444)?;
            write_synced(&temporary.join("state.json"), &state, 0o444)?;
            let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
            write_synced(&temporary.join("manifest.json"), &manifest_bytes, 0o444)?;
            set_mode(&temporary, 0o555)?;
            sync_directory(&temporary)?;
            fs::rename(&temporary, &destination)?;
            sync_directory(&self.root.join("revisions"))?;
            self.verify(&revision_id)
        })();
        if result.is_err() && temporary.exists() {
            set_mode(&temporary, 0o755).ok();
            fs::remove_dir_all(&temporary).ok();
        }
        result
    }

    pub fn verify(&self, revision_id: &str) -> Result<VerifiedRevision> {
        let directory = self.revision_path(revision_id)?;
        let manifest: RevisionManifest =
            serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
        if manifest.format_version != FORMAT_VERSION || manifest.revision_id != revision_id {
            return Err(Error::InvalidRevision(
                "manifest format or revision identity mismatch".into(),
            ));
        }
        if revision_identity(
            manifest.schema_version,
            manifest.experience_api_version,
            &manifest.source,
            &manifest.state,
        ) != revision_id
        {
            return Err(Error::InvalidRevision(
                "content-addressed revision identity mismatch".into(),
            ));
        }
        if manifest.schema_version == 0 || manifest.experience_api_version == 0 {
            return Err(Error::InvalidRevision(
                "schema and experience API versions must be positive".into(),
            ));
        }
        verify_file(&directory, &manifest.source)?;
        verify_file(&directory, &manifest.state)?;
        let state: DurableState =
            serde_json::from_slice(&fs::read(directory.join(&manifest.state.path))?)?;
        if state.schema_version != manifest.schema_version
            || state.source_sha256 != manifest.source.sha256
        {
            return Err(Error::InvalidRevision(
                "source, state, and schema do not describe one revision".into(),
            ));
        }
        Ok(VerifiedRevision {
            directory,
            manifest,
        })
    }

    pub fn current(&self) -> Result<Option<VerifiedRevision>> {
        let pointer = self.root.join("current");
        let target = match fs::read_link(&pointer) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let revision_id = target
            .strip_prefix("revisions")
            .ok()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::InvalidPointer(target.clone()))?;
        self.verify(revision_id).map(Some)
    }

    pub fn set_current(&self, revision_id: &str) -> Result<()> {
        self.verify(revision_id)?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = self
            .root
            .join(format!(".current-{}-{sequence}", std::process::id()));
        symlink(Path::new("revisions").join(revision_id), &temporary)?;
        if let Err(error) = fs::rename(&temporary, self.root.join("current")) {
            fs::remove_file(&temporary).ok();
            return Err(error.into());
        }
        sync_directory(&self.root)
    }

    fn revision_path(&self, revision_id: &str) -> Result<PathBuf> {
        let path = Path::new(revision_id);
        if revision_id.len() != 64
            || !revision_id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(Error::InvalidRevisionId(revision_id.into()));
        }
        Ok(self.root.join("revisions").join(revision_id))
    }
}

fn identity(path: &str, bytes: &[u8]) -> FileIdentity {
    FileIdentity {
        path: path.into(),
        size: bytes.len() as u64,
        sha256: digest(bytes),
    }
}

fn verify_file(directory: &Path, expected: &FileIdentity) -> Result<()> {
    let relative = Path::new(&expected.path);
    if relative.components().count() != 1
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(Error::InvalidRevision(format!(
            "invalid manifest path: {}",
            expected.path
        )));
    }
    let bytes = fs::read(directory.join(relative))?;
    if bytes.len() as u64 != expected.size || digest(&bytes) != expected.sha256 {
        return Err(Error::InvalidRevision(format!(
            "content identity mismatch: {}",
            expected.path
        )));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn revision_identity(
    schema_version: u64,
    experience_api_version: u32,
    source: &FileIdentity,
    state: &FileIdentity,
) -> String {
    let mut revision_digest = Sha256::new();
    revision_digest.update(FORMAT_VERSION.to_le_bytes());
    revision_digest.update(schema_version.to_le_bytes());
    revision_digest.update(experience_api_version.to_le_bytes());
    revision_digest.update(source.sha256.as_bytes());
    revision_digest.update(state.sha256.as_bytes());
    format!("{:x}", revision_digest.finalize())
}

fn write_synced(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.sync_all()?;
    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}
