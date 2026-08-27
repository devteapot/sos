use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use experience_package::{ExperienceId, ExperienceRole};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, RevisionStore, VerifiedRevision};

pub const STOCK_SHELL_EXPERIENCE_ID: &str = "sos.stock.shell";
const REGISTRY_FORMAT_VERSION: u32 = 1;
static REGISTRY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExperienceRecord {
    pub format_version: u32,
    pub experience_id: ExperienceId,
    pub role: ExperienceRole,
    #[serde(default)]
    pub accepts_legacy_revisions: bool,
}

#[derive(Clone, Debug)]
pub struct ExperienceRegistry {
    store: RevisionStore,
    root: PathBuf,
}

impl ExperienceRegistry {
    pub fn open(store: RevisionStore) -> Result<Self> {
        let root = store.root().join("experiences");
        fs::create_dir_all(&root)?;
        sync_directory(&root)?;
        Ok(Self { store, root })
    }

    pub fn create(
        &self,
        experience_id: &ExperienceId,
        role: ExperienceRole,
        revision_id: &str,
    ) -> Result<ExperienceRecord> {
        let revision = self.store.verify(revision_id)?;
        let package = revision.package.as_ref().ok_or_else(|| {
            Error::InvalidRegistry("new experiences require package format v4".into())
        })?;
        if &package.experience_id != experience_id || package.role != role {
            return Err(Error::InvalidRegistry(
                "revision package identity or role does not match the registry record".into(),
            ));
        }
        self.create_record(
            ExperienceRecord {
                format_version: REGISTRY_FORMAT_VERSION,
                experience_id: experience_id.clone(),
                role,
                accepts_legacy_revisions: false,
            },
            revision_id,
        )
    }

    pub fn migrate_legacy_current(&self) -> Result<Option<ExperienceRecord>> {
        let stock_id = ExperienceId::parse(STOCK_SHELL_EXPERIENCE_ID)
            .map_err(|error| Error::InvalidRegistry(error.to_string()))?;
        if let Some(record) = self.get(&stock_id)? {
            return Ok(Some(record));
        }
        let Some(revision) = self.store.current()? else {
            return Ok(None);
        };
        if revision.package.is_some() {
            return Err(Error::InvalidRegistry(
                "legacy current pointer unexpectedly names a v4 package".into(),
            ));
        }
        self.create_record(
            ExperienceRecord {
                format_version: REGISTRY_FORMAT_VERSION,
                experience_id: stock_id,
                role: ExperienceRole::Shell,
                accepts_legacy_revisions: true,
            },
            &revision.manifest.revision_id,
        )
        .map(Some)
    }

    pub fn get(&self, experience_id: &ExperienceId) -> Result<Option<ExperienceRecord>> {
        let path = self.record_path(experience_id).join("identity.json");
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let record: ExperienceRecord = serde_json::from_slice(&bytes)?;
        self.validate_record(&record, experience_id)?;
        Ok(Some(record))
    }

    pub fn list(&self) -> Result<Vec<ExperienceRecord>> {
        let mut records = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::InvalidRegistry("non-UTF-8 experience directory".into()))?;
            let id = ExperienceId::parse(name)
                .map_err(|error| Error::InvalidRegistry(error.to_string()))?;
            let record = self.get(&id)?.ok_or_else(|| {
                Error::InvalidRegistry(format!("experience `{id}` lacks identity.json"))
            })?;
            records.push(record);
        }
        records.sort_by(|left, right| left.experience_id.cmp(&right.experience_id));
        Ok(records)
    }

    pub fn current(&self, experience_id: &ExperienceId) -> Result<Option<VerifiedRevision>> {
        self.pointer(experience_id, "current")
    }

    pub fn previous(&self, experience_id: &ExperienceId) -> Result<Option<VerifiedRevision>> {
        self.pointer(experience_id, "previous")
    }

    pub fn set_current(&self, experience_id: &ExperienceId, revision_id: &str) -> Result<()> {
        let record = self.get(experience_id)?.ok_or_else(|| {
            Error::InvalidRegistry(format!("unknown experience `{experience_id}`"))
        })?;
        let revision = self.store.verify(revision_id)?;
        self.validate_revision_binding(&record, &revision)?;
        if self
            .current(experience_id)?
            .is_some_and(|current| current.manifest.revision_id == revision_id)
        {
            return Ok(());
        }
        let directory = self.record_path(experience_id);
        let sequence = REGISTRY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        if let Ok(current_target) = fs::read_link(directory.join("current")) {
            let previous_temporary =
                directory.join(format!(".previous-{}-{sequence}", std::process::id()));
            symlink(current_target, &previous_temporary)?;
            if let Err(error) = fs::rename(&previous_temporary, directory.join("previous")) {
                fs::remove_file(&previous_temporary).ok();
                return Err(error.into());
            }
        }
        let temporary = directory.join(format!(".current-{}-{sequence}", std::process::id()));
        symlink(
            Path::new("../..").join("revisions").join(revision_id),
            &temporary,
        )?;
        if let Err(error) = fs::rename(&temporary, directory.join("current")) {
            fs::remove_file(&temporary).ok();
            return Err(error.into());
        }
        sync_directory(&directory)
    }

    fn create_record(
        &self,
        record: ExperienceRecord,
        revision_id: &str,
    ) -> Result<ExperienceRecord> {
        let directory = self.record_path(&record.experience_id);
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = self.get(&record.experience_id)?.ok_or_else(|| {
                    Error::InvalidRegistry("existing experience record is incomplete".into())
                })?;
                if existing != record {
                    return Err(Error::InvalidRegistry(format!(
                        "experience `{}` already has a different identity",
                        record.experience_id
                    )));
                }
                self.set_current(&record.experience_id, revision_id)?;
                return Ok(existing);
            }
            Err(error) => return Err(error.into()),
        }
        let result = (|| {
            let identity = serde_json::to_vec_pretty(&record)?;
            write_synced(&directory.join("identity.json"), &identity, 0o444)?;
            set_mode(&directory, 0o755)?;
            self.set_current_for_new_record(&record, revision_id)?;
            sync_directory(&directory)?;
            sync_directory(&self.root)?;
            Ok(record.clone())
        })();
        if result.is_err() {
            set_mode(&directory, 0o755).ok();
            fs::remove_dir_all(&directory).ok();
        }
        result
    }

    fn set_current_for_new_record(
        &self,
        record: &ExperienceRecord,
        revision_id: &str,
    ) -> Result<()> {
        let revision = self.store.verify(revision_id)?;
        self.validate_revision_binding(record, &revision)?;
        symlink(
            Path::new("../..").join("revisions").join(revision_id),
            self.record_path(&record.experience_id).join("current"),
        )?;
        Ok(())
    }

    fn pointer(
        &self,
        experience_id: &ExperienceId,
        name: &str,
    ) -> Result<Option<VerifiedRevision>> {
        self.get(experience_id)?.ok_or_else(|| {
            Error::InvalidRegistry(format!("unknown experience `{experience_id}`"))
        })?;
        let target = match fs::read_link(self.record_path(experience_id).join(name)) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let revision_id = target
            .strip_prefix(Path::new("../..").join("revisions"))
            .ok()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::InvalidPointer(target.clone()))?;
        let revision = self.store.verify(revision_id)?;
        let record = self.get(experience_id)?.unwrap();
        self.validate_revision_binding(&record, &revision)?;
        Ok(Some(revision))
    }

    fn validate_record(&self, record: &ExperienceRecord, requested: &ExperienceId) -> Result<()> {
        if record.format_version != REGISTRY_FORMAT_VERSION || &record.experience_id != requested {
            return Err(Error::InvalidRegistry(
                "experience identity file does not match its directory".into(),
            ));
        }
        Ok(())
    }

    fn validate_revision_binding(
        &self,
        record: &ExperienceRecord,
        revision: &VerifiedRevision,
    ) -> Result<()> {
        match &revision.package {
            Some(package)
                if package.experience_id == record.experience_id && package.role == record.role =>
            {
                Ok(())
            }
            None if record.accepts_legacy_revisions => Ok(()),
            Some(_) => Err(Error::InvalidRegistry(
                "revision belongs to a different experience or role".into(),
            )),
            None => Err(Error::InvalidRegistry(
                "experience does not accept legacy revisions".into(),
            )),
        }
    }

    fn record_path(&self, experience_id: &ExperienceId) -> PathBuf {
        self.root.join(experience_id.as_str())
    }
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
mod tests {
    use std::collections::BTreeMap;

    use experience_package::{
        DerivationKind, DerivationRecord, ExperienceContract, ExperienceExport, ExportId,
        PackageMetadata, ValueSchema, ViewportContract, APPEARANCE_ABI_VERSION, CONTRACT_VERSION,
        PACKAGE_FORMAT_VERSION,
    };
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::{RevisionInput, RevisionPackageInput};

    fn package(id: &str, role: ExperienceRole) -> PackageMetadata {
        PackageMetadata {
            format_version: PACKAGE_FORMAT_VERSION,
            experience_id: ExperienceId::parse(id).unwrap(),
            role,
            provider_capabilities: Default::default(),
            contract: ExperienceContract {
                contract_version: CONTRACT_VERSION,
                exports: BTreeMap::from([(
                    ExportId::parse("main").unwrap(),
                    ExperienceExport {
                        properties: ValueSchema::empty_record(),
                        events: BTreeMap::new(),
                        viewport: ViewportContract {
                            min_width: 1,
                            min_height: 1,
                            max_width: 4096,
                            max_height: 4096,
                        },
                        appearance_abi: APPEARANCE_ABI_VERSION,
                        accepts_container_appearance: false,
                    },
                )]),
            },
            dependencies: BTreeMap::new(),
            derivation: DerivationRecord {
                kind: DerivationKind::Original,
                parents: vec![],
                request_sha256: None,
                rationale: None,
            },
            state_migration: None,
        }
    }

    fn install_v4(store: &RevisionStore, id: &str, role: ExperienceRole, source: &str) -> String {
        store
            .install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: source.as_bytes().to_vec(),
                    state: json!({}),
                    schema_version: 1,
                    experience_api_version: 4,
                    assets: vec![],
                },
                package: package(id, role),
            })
            .unwrap()
            .manifest
            .revision_id
    }

    #[test]
    fn registry_keeps_independent_current_and_previous_pointers() {
        let directory = TempDir::new().unwrap();
        let store = RevisionStore::open(directory.path()).unwrap();
        let registry = ExperienceRegistry::open(store.clone()).unwrap();
        let agenda = ExperienceId::parse("agenda").unwrap();
        let media = ExperienceId::parse("media").unwrap();
        let agenda_one = install_v4(&store, "agenda", ExperienceRole::Ordinary, "agenda-one");
        let agenda_two = install_v4(&store, "agenda", ExperienceRole::Ordinary, "agenda-two");
        let media_one = install_v4(&store, "media", ExperienceRole::Ordinary, "media-one");

        registry
            .create(&agenda, ExperienceRole::Ordinary, &agenda_one)
            .unwrap();
        registry
            .create(&media, ExperienceRole::Ordinary, &media_one)
            .unwrap();
        registry.set_current(&agenda, &agenda_two).unwrap();

        assert_eq!(
            registry
                .current(&agenda)
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            agenda_two
        );
        assert_eq!(
            registry
                .previous(&agenda)
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            agenda_one
        );
        assert_eq!(
            registry
                .current(&media)
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            media_one
        );
        assert_eq!(registry.list().unwrap().len(), 2);
    }

    #[test]
    fn legacy_current_migrates_only_to_the_stock_shell_record() {
        let directory = TempDir::new().unwrap();
        let store = RevisionStore::open(directory.path()).unwrap();
        let legacy = store
            .install(RevisionInput {
                source: b"legacy".to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 3,
                assets: vec![],
            })
            .unwrap()
            .manifest
            .revision_id;
        store.set_current(&legacy).unwrap();
        let registry = ExperienceRegistry::open(store).unwrap();
        let record = registry.migrate_legacy_current().unwrap().unwrap();
        assert_eq!(record.experience_id.as_str(), STOCK_SHELL_EXPERIENCE_ID);
        assert!(record.accepts_legacy_revisions);
        assert_eq!(
            registry
                .current(&record.experience_id)
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            legacy
        );
    }
}
