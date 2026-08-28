use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use experience_package::{canonical_json, DependencyPolicy, ExperienceId};
use serde::{Deserialize, Serialize};

use crate::{Error, ExperienceRegistry, GraphStore, Result, RevisionStore};

const REVERSE_INDEX_FORMAT_VERSION: u32 = 1;
static INDEX_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReverseDependencyData {
    pub format_version: u32,
    #[serde(default)]
    pub dependents: BTreeMap<ExperienceId, BTreeSet<ExperienceId>>,
}

#[derive(Clone, Debug)]
pub struct ReverseDependencyIndex {
    root: PathBuf,
}

impl ReverseDependencyIndex {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn rebuild(
        &self,
        revisions: &RevisionStore,
        registry: &ExperienceRegistry,
    ) -> Result<ReverseDependencyData> {
        let mut data = ReverseDependencyData {
            format_version: REVERSE_INDEX_FORMAT_VERSION,
            dependents: BTreeMap::new(),
        };
        for record in registry.list()? {
            let Some(revision) = registry.current(&record.experience_id)? else {
                continue;
            };
            let package = revision.package;
            for binding in package.dependencies.values() {
                if binding.policy == DependencyPolicy::Tracked {
                    data.dependents
                        .entry(binding.experience_id.clone())
                        .or_default()
                        .insert(package.experience_id.clone());
                }
            }
            revisions.verify(&revision.manifest.revision_id)?;
        }
        self.write(&data)?;
        Ok(data)
    }

    pub fn load(&self) -> Result<ReverseDependencyData> {
        let bytes = match fs::read(self.path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ReverseDependencyData {
                    format_version: REVERSE_INDEX_FORMAT_VERSION,
                    dependents: BTreeMap::new(),
                })
            }
            Err(error) => return Err(error.into()),
        };
        let data: ReverseDependencyData = serde_json::from_slice(&bytes)?;
        if data.format_version != REVERSE_INDEX_FORMAT_VERSION {
            return Err(Error::InvalidGraph(
                "invalid reverse dependency index version".into(),
            ));
        }
        if canonical_json(&data).map_err(|error| Error::InvalidGraph(error.to_string()))? != bytes {
            return Err(Error::InvalidGraph(
                "reverse dependency index is not canonical".into(),
            ));
        }
        for (dependency, dependents) in &data.dependents {
            ExperienceId::parse(dependency.as_str())
                .map_err(|error| Error::InvalidGraph(error.to_string()))?;
            for dependent in dependents {
                ExperienceId::parse(dependent.as_str())
                    .map_err(|error| Error::InvalidGraph(error.to_string()))?;
                if dependency == dependent {
                    return Err(Error::InvalidGraph(
                        "reverse dependency index contains a self edge".into(),
                    ));
                }
            }
        }
        Ok(data)
    }

    pub fn affected_active_roots(
        &self,
        changed: &ExperienceId,
        graphs: &GraphStore,
    ) -> Result<BTreeSet<ExperienceId>> {
        let data = self.load()?;
        let active = graphs.active_experiences()?;
        let mut affected = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([changed.clone()]);
        while let Some(experience_id) = queue.pop_front() {
            if !visited.insert(experience_id.clone()) {
                continue;
            }
            if experience_id != *changed && active.contains(&experience_id) {
                affected.insert(experience_id.clone());
            }
            if let Some(dependents) = data.dependents.get(&experience_id) {
                queue.extend(dependents.iter().cloned());
            }
        }
        Ok(affected)
    }

    fn write(&self, data: &ReverseDependencyData) -> Result<()> {
        let bytes = canonical_json(data).map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let temporary = self.root.join(format!(
            ".reverse-dependencies-{}-{}.tmp",
            std::process::id(),
            INDEX_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.sync_all()?;
        fs::rename(&temporary, self.path())?;
        sync_directory(&self.root)
    }

    fn path(&self) -> PathBuf {
        self.root.join("reverse-dependencies.json")
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    OpenOptions::new().read(true).open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use experience_package::{DependencyAlias, ExportId};
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        install_reference_composition, GraphResolver, RevisionInput, RevisionPackageInput,
    };

    #[test]
    fn persistent_index_finds_transitive_active_tracked_roots() {
        let directory = TempDir::new().unwrap();
        let store = RevisionStore::open(directory.path()).unwrap();
        let reference = install_reference_composition(&store).unwrap();
        let dashboard_id = ExperienceId::parse("sos.example.dashboard").unwrap();
        let agenda_id = ExperienceId::parse("sos.example.agenda").unwrap();
        let registry = ExperienceRegistry::open(store.clone()).unwrap();
        let dashboard = store.verify(&reference.dashboard_revision).unwrap();
        let source = fs::read(dashboard.directory.join(&dashboard.manifest.source.path)).unwrap();
        let mut package = dashboard.package;
        package
            .dependencies
            .get_mut(&DependencyAlias::parse("agenda").unwrap())
            .unwrap()
            .policy = DependencyPolicy::Tracked;
        let tracked_revision = store
            .install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source,
                    state: json!({}),
                    schema_version: 1,
                    experience_api_version: 4,
                    assets: Vec::new(),
                },
                package,
            })
            .unwrap()
            .manifest
            .revision_id;
        registry
            .set_current(&dashboard_id, &tracked_revision)
            .unwrap();
        let graph = GraphResolver::new(store.clone())
            .resolve_tracked(
                &tracked_revision,
                &ExportId::parse("main").unwrap(),
                &registry,
            )
            .unwrap();
        let graphs = GraphStore::open(store.root()).unwrap();
        let graph_id = graphs.install(&graph).unwrap();
        graphs.set_current(&dashboard_id, &graph_id).unwrap();
        let index = ReverseDependencyIndex::open(store.root());
        let data = index.rebuild(&store, &registry).unwrap();
        assert!(data.dependents[&agenda_id].contains(&dashboard_id));
        assert_eq!(
            ReverseDependencyIndex::open(store.root())
                .affected_active_roots(&agenda_id, &graphs)
                .unwrap(),
            BTreeSet::from([dashboard_id])
        );
    }
}
