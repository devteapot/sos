use std::{
    collections::HashSet,
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

const FORMAT_VERSION: u32 = 3;
pub const MAX_REVISION_ASSETS: usize = 64;
pub const MAX_REVISION_ASSET_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REVISION_ASSET_TOTAL_BYTES: usize = 16 * 1024 * 1024;
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
    pub assets: Vec<RevisionAssetInput>,
}

#[derive(Clone, Debug)]
pub struct RevisionAssetInput {
    pub id: String,
    pub kind: String,
    pub bytes: Vec<u8>,
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
    pub assets: Vec<AssetIdentity>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AssetIdentity {
    pub id: String,
    pub kind: String,
    pub file: FileIdentity,
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
        let assets = prepare_assets(input.assets)?;
        let durable_state = DurableState {
            schema_version: input.schema_version,
            source_sha256: source_sha256.clone(),
            state: input.state,
        };
        let state = serde_json::to_vec(&durable_state)?;
        let source = identity("source.luau", &input.source);
        let state_identity = identity("state.json", &state);
        let asset_identities = assets
            .iter()
            .map(|asset| asset.identity.clone())
            .collect::<Vec<_>>();

        let revision_id = revision_identity(
            input.schema_version,
            input.experience_api_version,
            &source,
            &state_identity,
            &asset_identities,
        );
        let manifest = RevisionManifest {
            format_version: FORMAT_VERSION,
            revision_id: revision_id.clone(),
            schema_version: input.schema_version,
            experience_api_version: input.experience_api_version,
            source,
            state: state_identity,
            assets: asset_identities,
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
            if !assets.is_empty() {
                let assets_directory = temporary.join("assets");
                fs::create_dir(&assets_directory)?;
                let mut written = HashSet::new();
                for asset in &assets {
                    if written.insert(asset.identity.file.path.clone()) {
                        write_synced(
                            &temporary.join(&asset.identity.file.path),
                            &asset.bytes,
                            0o444,
                        )?;
                    }
                }
                set_mode(&assets_directory, 0o555)?;
            }
            let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
            write_synced(&temporary.join("manifest.json"), &manifest_bytes, 0o444)?;
            if let Some(key) = signing_key("SOS_REVISION_SIGNING_KEY_FILE")? {
                let signature = hmac_sha256(&key, &manifest_bytes);
                write_synced(
                    &temporary.join("manifest.hmac-sha256"),
                    format!("{signature}\n").as_bytes(),
                    0o444,
                )?;
            }
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
        let manifest_bytes = fs::read(directory.join("manifest.json"))?;
        if let Some(key) = signing_key("SOS_REVISION_VERIFY_KEY_FILE")? {
            let expected = hmac_sha256(&key, &manifest_bytes);
            let supplied = fs::read_to_string(directory.join("manifest.hmac-sha256"))?;
            if !constant_time_equal(expected.as_bytes(), supplied.trim().as_bytes()) {
                return Err(Error::InvalidRevision(
                    "revision manifest signature mismatch".into(),
                ));
            }
        }
        let manifest: RevisionManifest = serde_json::from_slice(&manifest_bytes)?;
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
            &manifest.assets,
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
        validate_asset_identities(&manifest.assets)?;
        for asset in &manifest.assets {
            verify_file(&directory, &asset.file)?;
            validate_asset_bytes(&asset.kind, &fs::read(directory.join(&asset.file.path))?)?;
        }
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

    pub fn previous(&self) -> Result<Option<VerifiedRevision>> {
        let pointer = self.root.join("previous");
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
        if let Ok(current_target) = fs::read_link(self.root.join("current")) {
            let previous_temporary = self
                .root
                .join(format!(".previous-{}-{sequence}", std::process::id()));
            symlink(current_target, &previous_temporary)?;
            if let Err(error) = fs::rename(&previous_temporary, self.root.join("previous")) {
                fs::remove_file(&previous_temporary).ok();
                return Err(error.into());
            }
        }
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

fn signing_key(variable: &str) -> Result<Option<Vec<u8>>> {
    let Some(path) = std::env::var_os(variable).map(PathBuf::from) else {
        return Ok(None);
    };
    let metadata = fs::metadata(&path)?;
    if metadata.permissions().mode() & 0o077 != 0 || metadata.len() < 32 || metadata.len() > 4096 {
        return Err(Error::InvalidRevision(format!(
            "{variable} must name a private 32..4096-byte key file"
        )));
    }
    Ok(Some(fs::read(path)?))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> String {
    let mut block = [0_u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner_key = block;
    let mut outer_key = block;
    for byte in &mut inner_key {
        *byte ^= 0x36;
    }
    for byte in &mut outer_key {
        *byte ^= 0x5c;
    }
    let mut inner = Sha256::new();
    inner.update(inner_key);
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_key);
    outer.update(inner);
    format!("{:x}", outer.finalize())
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn verify_file(directory: &Path, expected: &FileIdentity) -> Result<()> {
    let relative = Path::new(&expected.path);
    if relative.components().count() > 2
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(Error::InvalidRevision(format!(
            "invalid manifest path: {}",
            expected.path
        )));
    }
    let path = directory.join(relative);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() || metadata.len() != expected.size {
        return Err(Error::InvalidRevision(format!(
            "invalid revision file type or size: {}",
            expected.path
        )));
    }
    let bytes = fs::read(path)?;
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
    assets: &[AssetIdentity],
) -> String {
    let mut revision_digest = Sha256::new();
    revision_digest.update(FORMAT_VERSION.to_le_bytes());
    revision_digest.update(schema_version.to_le_bytes());
    revision_digest.update(experience_api_version.to_le_bytes());
    revision_digest.update(source.sha256.as_bytes());
    revision_digest.update(state.sha256.as_bytes());
    for asset in assets {
        revision_digest.update(asset.id.as_bytes());
        revision_digest.update([0]);
        revision_digest.update(asset.kind.as_bytes());
        revision_digest.update([0]);
        revision_digest.update(asset.file.path.as_bytes());
        revision_digest.update(asset.file.size.to_le_bytes());
        revision_digest.update(asset.file.sha256.as_bytes());
    }
    format!("{:x}", revision_digest.finalize())
}

#[derive(Clone)]
struct PreparedAsset {
    identity: AssetIdentity,
    bytes: Vec<u8>,
}

fn prepare_assets(mut inputs: Vec<RevisionAssetInput>) -> Result<Vec<PreparedAsset>> {
    if inputs.len() > MAX_REVISION_ASSETS {
        return Err(Error::InvalidRevision("too many revision assets".into()));
    }
    inputs.sort_by(|left, right| left.id.cmp(&right.id));
    let total = inputs.iter().map(|asset| asset.bytes.len()).sum::<usize>();
    if total > MAX_REVISION_ASSET_TOTAL_BYTES {
        return Err(Error::InvalidRevision(
            "revision asset package is too large".into(),
        ));
    }
    let mut ids = HashSet::new();
    inputs
        .into_iter()
        .map(|asset| {
            validate_asset_id(&asset.id)?;
            if !ids.insert(asset.id.clone()) {
                return Err(Error::InvalidRevision(format!(
                    "duplicate revision asset id: {}",
                    asset.id
                )));
            }
            validate_asset_bytes(&asset.kind, &asset.bytes)?;
            let sha256 = digest(&asset.bytes);
            let extension = asset_extension(&asset.kind).ok_or_else(|| {
                Error::InvalidRevision(format!("unsupported revision asset kind: {}", asset.kind))
            })?;
            let path = format!("assets/{sha256}.{extension}");
            Ok(PreparedAsset {
                identity: AssetIdentity {
                    id: asset.id,
                    kind: asset.kind,
                    file: identity(&path, &asset.bytes),
                },
                bytes: asset.bytes,
            })
        })
        .collect()
}

fn validate_asset_identities(assets: &[AssetIdentity]) -> Result<()> {
    if assets.len() > MAX_REVISION_ASSETS {
        return Err(Error::InvalidRevision("too many revision assets".into()));
    }
    let mut ids = HashSet::new();
    let mut previous = None;
    let mut total = 0usize;
    for asset in assets {
        validate_asset_id(&asset.id)?;
        if !ids.insert(asset.id.clone())
            || previous.is_some_and(|value: &str| value > asset.id.as_str())
        {
            return Err(Error::InvalidRevision(
                "asset identities must have unique sorted ids".into(),
            ));
        }
        previous = Some(asset.id.as_str());
        if asset_extension(&asset.kind).is_none() {
            return Err(Error::InvalidRevision(format!(
                "unsupported revision asset kind: {}",
                asset.kind
            )));
        }
        if asset.file.size == 0 || asset.file.size > MAX_REVISION_ASSET_BYTES as u64 {
            return Err(Error::InvalidRevision(format!(
                "invalid {} asset size",
                asset.kind
            )));
        }
        let expected_suffix = format!(".{}", asset_extension(&asset.kind).unwrap());
        if !asset.file.path.starts_with("assets/") || !asset.file.path.ends_with(&expected_suffix) {
            return Err(Error::InvalidRevision(format!(
                "invalid asset path: {}",
                asset.file.path
            )));
        }
        total = total.saturating_add(asset.file.size as usize);
    }
    if total > MAX_REVISION_ASSET_TOTAL_BYTES {
        return Err(Error::InvalidRevision(
            "revision asset package is too large".into(),
        ));
    }
    Ok(())
}

fn validate_asset_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(Error::InvalidRevision(format!(
            "invalid revision asset id: {id}"
        )));
    }
    Ok(())
}

fn asset_extension(kind: &str) -> Option<&'static str> {
    match kind {
        "svg" => Some("svg"),
        "png" => Some("png"),
        "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        "font" => Some("font"),
        "shader" => Some("wgsl"),
        _ => None,
    }
}

fn validate_asset_bytes(kind: &str, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_REVISION_ASSET_BYTES {
        return Err(Error::InvalidRevision(format!("invalid {kind} asset size")));
    }
    let valid = match kind {
        "svg" => std::str::from_utf8(bytes).is_ok_and(|text| {
            let text = text.to_ascii_lowercase();
            text.contains("<svg")
                && text.contains("</svg>")
                && ![
                    "<script",
                    "javascript:",
                    "<!doctype",
                    "<!entity",
                    "<foreignobject",
                    "xlink:href",
                    "href=\"http",
                    "href='http",
                    "url(http",
                ]
                .iter()
                .any(|needle| text.contains(needle))
        }),
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        "font" => {
            bytes.starts_with(&[0, 1, 0, 0])
                || bytes.starts_with(b"OTTO")
                || bytes.starts_with(b"ttcf")
                || bytes.starts_with(b"wOFF")
                || bytes.starts_with(b"wOF2")
        }
        "shader" => std::str::from_utf8(bytes)
            .ok()
            .is_some_and(validate_shader_asset),
        _ => false,
    };
    if !valid {
        return Err(Error::InvalidRevision(format!(
            "invalid or unsupported {kind} revision asset"
        )));
    }
    Ok(())
}

fn validate_shader_asset(source: &str) -> bool {
    let Ok(module) = naga::front::wgsl::parse_str(source) else {
        return false;
    };
    if module
        .global_variables
        .iter()
        .any(|(_, global)| global.binding.is_some())
    {
        return false;
    }
    let has_vertex = module
        .entry_points
        .iter()
        .any(|entry| entry.name == "vs_main" && entry.stage == naga::ShaderStage::Vertex);
    let has_fragment = module
        .entry_points
        .iter()
        .any(|entry| entry.name == "fs_main" && entry.stage == naga::ShaderStage::Fragment);
    let has_compute = module
        .entry_points
        .iter()
        .any(|entry| entry.stage == naga::ShaderStage::Compute);
    has_vertex
        && has_fragment
        && !has_compute
        && naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .is_ok()
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

#[cfg(test)]
mod signing_tests {
    use super::{constant_time_equal, hmac_sha256};

    #[test]
    fn detached_manifest_hmac_matches_standard_vector() {
        assert_eq!(
            hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
        assert!(constant_time_equal(b"same", b"same"));
        assert!(!constant_time_equal(b"same", b"diff"));
    }
}
