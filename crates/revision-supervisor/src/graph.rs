use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use experience_package::{
    canonical_json, hex_sha256, DependencyAlias, DependencyBinding, ExperienceId, ExportId,
    GraphNodeId, ResolvedGraph, ResolvedGraphNode, RevisionId, ValueSchema, GRAPH_FORMAT_VERSION,
    MAX_GRAPH_DEPTH, MAX_GRAPH_INSTANCES,
};

use crate::{Error, ExperienceRegistry, Result, RevisionStore};

static GRAPH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct GraphResolver {
    revisions: RevisionStore,
}

impl GraphResolver {
    pub fn new(revisions: RevisionStore) -> Self {
        Self { revisions }
    }

    pub fn resolve(&self, revision_id: &str, export_id: &ExportId) -> Result<ResolvedGraph> {
        self.resolve_inner(revision_id, export_id, None, &BTreeMap::new())
    }

    pub fn resolve_tracked(
        &self,
        revision_id: &str,
        export_id: &ExportId,
        registry: &ExperienceRegistry,
    ) -> Result<ResolvedGraph> {
        self.resolve_inner(revision_id, export_id, Some(registry), &BTreeMap::new())
    }

    pub fn resolve_tracked_with_overrides(
        &self,
        revision_id: &str,
        export_id: &ExportId,
        registry: &ExperienceRegistry,
        overrides: &BTreeMap<ExperienceId, RevisionId>,
    ) -> Result<ResolvedGraph> {
        self.resolve_inner(revision_id, export_id, Some(registry), overrides)
    }

    fn resolve_inner(
        &self,
        revision_id: &str,
        export_id: &ExportId,
        registry: Option<&ExperienceRegistry>,
        overrides: &BTreeMap<ExperienceId, RevisionId>,
    ) -> Result<ResolvedGraph> {
        let root_revision = RevisionId::parse(revision_id)
            .map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let root_id =
            GraphNodeId::parse("root").map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let mut graph = ResolvedGraph {
            format_version: GRAPH_FORMAT_VERSION,
            root: root_id.clone(),
            nodes: BTreeMap::new(),
        };
        let mut stack = Vec::new();
        self.resolve_node(
            &mut graph,
            root_id,
            root_revision,
            export_id.clone(),
            None,
            None,
            0,
            &mut stack,
            registry,
            overrides,
        )?;
        graph
            .validate()
            .map_err(|error| Error::InvalidGraph(error.to_string()))?;
        Ok(graph)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_node(
        &self,
        graph: &mut ResolvedGraph,
        node_id: GraphNodeId,
        revision_id: RevisionId,
        export_id: ExportId,
        parent: Option<GraphNodeId>,
        dependency: Option<DependencyAlias>,
        depth: usize,
        stack: &mut Vec<RevisionId>,
        registry: Option<&ExperienceRegistry>,
        overrides: &BTreeMap<ExperienceId, RevisionId>,
    ) -> Result<()> {
        if depth > MAX_GRAPH_DEPTH {
            return Err(Error::InvalidGraph(format!(
                "dependency graph exceeds depth {MAX_GRAPH_DEPTH}"
            )));
        }
        if graph.nodes.len() >= MAX_GRAPH_INSTANCES {
            return Err(Error::InvalidGraph(format!(
                "dependency graph exceeds {MAX_GRAPH_INSTANCES} instances"
            )));
        }
        if stack.contains(&revision_id) {
            return Err(Error::InvalidGraph(format!(
                "dependency cycle reaches revision {revision_id}"
            )));
        }
        let revision = self.revisions.verify(revision_id.as_str())?;
        let package = revision.package.ok_or_else(|| {
            Error::InvalidGraph(format!("revision {revision_id} has no v4 package metadata"))
        })?;
        if !package.contract.exports.contains_key(&export_id) {
            return Err(Error::InvalidGraph(format!(
                "revision {revision_id} does not export `{export_id}`"
            )));
        }
        graph.nodes.insert(
            node_id.clone(),
            ResolvedGraphNode {
                experience_id: package.experience_id.clone(),
                revision_id: revision_id.clone(),
                export_id,
                parent,
                dependency,
            },
        );
        stack.push(revision_id);
        for (alias, binding) in &package.dependencies {
            self.validate_binding(alias, binding)?;
            let resolved_revision_id = match (binding.policy, registry) {
                (experience_package::DependencyPolicy::Tracked, Some(registry)) => {
                    if let Some(revision_id) = overrides.get(&binding.experience_id) {
                        revision_id.to_string()
                    } else {
                        registry
                            .current(&binding.experience_id)?
                            .ok_or_else(|| {
                                Error::InvalidGraph(format!(
                                    "tracked dependency `{alias}` has no active revision"
                                ))
                            })?
                            .manifest
                            .revision_id
                    }
                }
                _ => binding.revision_id.to_string(),
            };
            let resolved_revision_id = RevisionId::parse(resolved_revision_id)
                .map_err(|error| Error::InvalidGraph(error.to_string()))?;
            let child_revision = self.revisions.verify(resolved_revision_id.as_str())?;
            let child_package = child_revision.package.as_ref().ok_or_else(|| {
                Error::InvalidGraph(format!("dependency `{alias}` names a legacy revision"))
            })?;
            if child_package.role == experience_package::ExperienceRole::Shell {
                return Err(Error::InvalidGraph(format!(
                    "dependency `{alias}` cannot mount a shell experience"
                )));
            }
            if child_package.experience_id != binding.experience_id {
                return Err(Error::InvalidGraph(format!(
                    "dependency `{alias}` revision belongs to `{}` instead of `{}`",
                    child_package.experience_id, binding.experience_id
                )));
            }
            let digest = child_package
                .contract
                .digest()
                .map_err(|error| Error::InvalidGraph(error.to_string()))?;
            if digest != binding.contract_digest {
                return Err(Error::InvalidGraph(format!(
                    "dependency `{alias}` contract digest changed"
                )));
            }
            let child_export = child_package
                .contract
                .exports
                .get(&binding.export_id)
                .ok_or_else(|| {
                    Error::InvalidGraph(format!(
                        "dependency `{alias}` export `{}` is missing",
                        binding.export_id
                    ))
                })?;
            validate_grant(
                alias,
                binding,
                &child_export.properties,
                &child_export.events,
            )?;
            let child_id = child_node_id(&node_id, alias)?;
            self.resolve_node(
                graph,
                child_id,
                resolved_revision_id,
                binding.export_id.clone(),
                Some(node_id.clone()),
                Some(alias.clone()),
                depth + 1,
                stack,
                registry,
                overrides,
            )?;
        }
        stack.pop();
        Ok(())
    }

    fn validate_binding(&self, alias: &DependencyAlias, binding: &DependencyBinding) -> Result<()> {
        binding
            .validate(alias)
            .map_err(|error| Error::InvalidGraph(error.to_string()))
    }
}

#[derive(Clone, Debug)]
pub struct GraphStore {
    root: PathBuf,
}

impl GraphStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into().join("graphs");
        fs::create_dir_all(root.join("snapshots"))?;
        fs::create_dir_all(root.join("active"))?;
        sync_directory(&root)?;
        Ok(Self { root })
    }

    pub fn install(&self, graph: &ResolvedGraph) -> Result<String> {
        let graph_id = graph
            .id()
            .map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let bytes =
            canonical_json(graph).map_err(|error| Error::InvalidGraph(error.to_string()))?;
        let destination = self.root.join("snapshots").join(format!("{graph_id}.json"));
        if destination.exists() {
            self.verify(&graph_id)?;
            return Ok(graph_id);
        }
        let temporary = self.root.join("snapshots").join(format!(
            ".graph-{}-{}.tmp",
            std::process::id(),
            GRAPH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        write_synced(&temporary, &bytes, 0o444)?;
        fs::rename(&temporary, &destination)?;
        sync_directory(&self.root.join("snapshots"))?;
        self.verify(&graph_id)?;
        Ok(graph_id)
    }

    pub fn snapshot_path(&self, graph_id: &str) -> Result<PathBuf> {
        self.verify(graph_id)?;
        Ok(self.root.join("snapshots").join(format!("{graph_id}.json")))
    }

    pub fn verify(&self, graph_id: &str) -> Result<ResolvedGraph> {
        validate_digest(graph_id)?;
        let bytes = fs::read(self.root.join("snapshots").join(format!("{graph_id}.json")))?;
        if hex_sha256(&bytes) != graph_id {
            return Err(Error::InvalidGraph("graph content digest mismatch".into()));
        }
        ResolvedGraph::from_canonical_bytes(&bytes)
            .map_err(|error| Error::InvalidGraph(error.to_string()))
    }

    pub fn set_current(&self, experience_id: &ExperienceId, graph_id: &str) -> Result<()> {
        self.verify(graph_id)?;
        if self
            .current(experience_id)?
            .is_some_and(|(current, _)| current == graph_id)
        {
            return Ok(());
        }
        let directory = self.root.join("active").join(experience_id.as_str());
        fs::create_dir_all(&directory)?;
        let sequence = GRAPH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        if let Ok(current) = fs::read_link(directory.join("current")) {
            let temporary = directory.join(format!(".previous-{}-{sequence}", std::process::id()));
            symlink(current, &temporary)?;
            if let Err(error) = fs::rename(&temporary, directory.join("previous")) {
                fs::remove_file(&temporary).ok();
                return Err(error.into());
            }
        }
        let temporary = directory.join(format!(".current-{}-{sequence}", std::process::id()));
        symlink(
            Path::new("../..")
                .join("snapshots")
                .join(format!("{graph_id}.json")),
            &temporary,
        )?;
        if let Err(error) = fs::rename(&temporary, directory.join("current")) {
            fs::remove_file(&temporary).ok();
            return Err(error.into());
        }
        sync_directory(&directory)
    }

    pub fn current(&self, experience_id: &ExperienceId) -> Result<Option<(String, ResolvedGraph)>> {
        self.pointer(experience_id, "current")
    }

    pub fn previous(
        &self,
        experience_id: &ExperienceId,
    ) -> Result<Option<(String, ResolvedGraph)>> {
        self.pointer(experience_id, "previous")
    }

    pub fn active_experiences(&self) -> Result<BTreeSet<ExperienceId>> {
        let mut experiences = BTreeSet::new();
        for entry in fs::read_dir(self.root.join("active"))? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::InvalidGraph("non-UTF-8 active graph directory".into()))?;
            let experience_id = ExperienceId::parse(name)
                .map_err(|error| Error::InvalidGraph(error.to_string()))?;
            if self.current(&experience_id)?.is_some() {
                experiences.insert(experience_id);
            }
        }
        Ok(experiences)
    }

    fn pointer(
        &self,
        experience_id: &ExperienceId,
        name: &str,
    ) -> Result<Option<(String, ResolvedGraph)>> {
        let path = self
            .root
            .join("active")
            .join(experience_id.as_str())
            .join(name);
        let target = match fs::read_link(path) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let file = target
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::InvalidPointer(target.clone()))?;
        let graph_id = file
            .strip_suffix(".json")
            .ok_or_else(|| Error::InvalidPointer(target.clone()))?;
        Ok(Some((graph_id.into(), self.verify(graph_id)?)))
    }
}

fn validate_grant(
    alias: &DependencyAlias,
    binding: &DependencyBinding,
    properties: &ValueSchema,
    events: &BTreeMap<experience_package::EventId, ValueSchema>,
) -> Result<()> {
    let ValueSchema::Record { fields } = properties else {
        return Err(Error::InvalidGraph(format!(
            "dependency `{alias}` must expose a closed record property schema"
        )));
    };
    let required = fields
        .iter()
        .filter_map(|(name, field)| field.required.then_some(name))
        .collect::<BTreeSet<_>>();
    if binding
        .grant
        .properties
        .iter()
        .any(|name| !fields.contains_key(name))
        || required
            .iter()
            .any(|name| !binding.grant.properties.contains(*name))
    {
        return Err(Error::InvalidGraph(format!(
            "dependency `{alias}` property grant does not match its export schema"
        )));
    }
    if binding
        .grant
        .events
        .iter()
        .any(|event| !events.contains_key(event))
    {
        return Err(Error::InvalidGraph(format!(
            "dependency `{alias}` grants an unknown child event"
        )));
    }
    Ok(())
}

fn child_node_id(parent: &GraphNodeId, alias: &DependencyAlias) -> Result<GraphNodeId> {
    let material = format!("{}\0{}", parent.as_str(), alias.as_str());
    GraphNodeId::parse(format!("n.{}", &hex_sha256(material.as_bytes())[..32]))
        .map_err(|error| Error::InvalidGraph(error.to_string()))
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(Error::InvalidGraph("invalid graph ID".into()))
    }
}

fn write_synced(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.sync_all()?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use experience_package::{
        BoundaryGrant, ContractDigest, DependencyPolicy, DerivationKind, DerivationRecord, EventId,
        ExperienceContract, ExperienceExport, ExperienceRole, FieldSchema, PackageMetadata,
        ViewportContract, APPEARANCE_ABI_VERSION, CONTRACT_VERSION, PACKAGE_FORMAT_VERSION,
    };
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::{RevisionInput, RevisionPackageInput};

    fn export(properties: ValueSchema) -> ExperienceExport {
        ExperienceExport {
            properties,
            events: BTreeMap::from([(
                EventId::parse("open").unwrap(),
                ValueSchema::empty_record(),
            )]),
            viewport: ViewportContract {
                min_width: 100,
                min_height: 80,
                max_width: 1920,
                max_height: 1080,
            },
            appearance_abi: APPEARANCE_ABI_VERSION,
            accepts_container_appearance: false,
        }
    }

    fn package(
        experience_id: &str,
        export_id: &str,
        export: ExperienceExport,
        dependencies: BTreeMap<DependencyAlias, DependencyBinding>,
    ) -> PackageMetadata {
        PackageMetadata {
            format_version: PACKAGE_FORMAT_VERSION,
            experience_id: ExperienceId::parse(experience_id).unwrap(),
            role: ExperienceRole::Ordinary,
            provider_capabilities: Default::default(),
            contract: ExperienceContract {
                contract_version: CONTRACT_VERSION,
                exports: BTreeMap::from([(ExportId::parse(export_id).unwrap(), export)]),
            },
            dependencies,
            derivation: DerivationRecord {
                kind: DerivationKind::Original,
                parents: vec![],
                request_sha256: None,
                rationale: None,
            },
            state_migration: None,
        }
    }

    fn install(store: &RevisionStore, package: PackageMetadata, source: &str) -> String {
        store
            .install_package(RevisionPackageInput {
                revision: RevisionInput {
                    source: source.as_bytes().to_vec(),
                    state: json!({}),
                    schema_version: 1,
                    experience_api_version: 4,
                    assets: vec![],
                },
                package,
            })
            .unwrap()
            .manifest
            .revision_id
    }

    #[test]
    fn resolver_binds_exact_contracts_and_graph_store_switches_atomically() {
        let directory = TempDir::new().unwrap();
        let revisions = RevisionStore::open(directory.path()).unwrap();
        let title_schema = ValueSchema::Record {
            fields: BTreeMap::from([(
                "title".into(),
                FieldSchema {
                    required: true,
                    value: ValueSchema::String {
                        max_bytes: 64,
                        choices: BTreeSet::new(),
                    },
                },
            )]),
        };
        let agenda_package = package("agenda", "summary", export(title_schema), BTreeMap::new());
        let agenda_digest = agenda_package.contract.digest().unwrap();
        let agenda_revision = install(&revisions, agenda_package, "agenda");
        let dashboard_package = package(
            "dashboard",
            "main",
            export(ValueSchema::empty_record()),
            BTreeMap::from([(
                DependencyAlias::parse("agenda").unwrap(),
                DependencyBinding {
                    experience_id: ExperienceId::parse("agenda").unwrap(),
                    revision_id: RevisionId::parse(&agenda_revision).unwrap(),
                    export_id: ExportId::parse("summary").unwrap(),
                    contract_digest: agenda_digest,
                    policy: DependencyPolicy::Locked,
                    grant: BoundaryGrant {
                        properties: BTreeSet::from(["title".into()]),
                        events: BTreeSet::from([EventId::parse("open").unwrap()]),
                    },
                },
            )]),
        );
        let dashboard_revision = install(&revisions, dashboard_package, "dashboard");

        let graph = GraphResolver::new(revisions)
            .resolve(&dashboard_revision, &ExportId::parse("main").unwrap())
            .unwrap();
        assert_eq!(graph.nodes.len(), 2);
        let graph_store = GraphStore::open(directory.path()).unwrap();
        let graph_id = graph_store.install(&graph).unwrap();
        let dashboard_id = ExperienceId::parse("dashboard").unwrap();
        graph_store.set_current(&dashboard_id, &graph_id).unwrap();
        assert_eq!(
            graph_store.current(&dashboard_id).unwrap().unwrap().0,
            graph_id
        );
    }

    #[test]
    fn resolver_rejects_a_stale_contract_digest() {
        let directory = TempDir::new().unwrap();
        let revisions = RevisionStore::open(directory.path()).unwrap();
        let agenda = package(
            "agenda",
            "summary",
            export(ValueSchema::empty_record()),
            BTreeMap::new(),
        );
        let agenda_revision = install(&revisions, agenda, "agenda");
        let dashboard = package(
            "dashboard",
            "main",
            export(ValueSchema::empty_record()),
            BTreeMap::from([(
                DependencyAlias::parse("agenda").unwrap(),
                DependencyBinding {
                    experience_id: ExperienceId::parse("agenda").unwrap(),
                    revision_id: RevisionId::parse(agenda_revision).unwrap(),
                    export_id: ExportId::parse("summary").unwrap(),
                    contract_digest: ContractDigest::parse("f".repeat(64)).unwrap(),
                    policy: DependencyPolicy::Locked,
                    grant: BoundaryGrant::default(),
                },
            )]),
        );
        let dashboard_revision = install(&revisions, dashboard, "dashboard");
        assert!(matches!(
            GraphResolver::new(revisions)
                .resolve(&dashboard_revision, &ExportId::parse("main").unwrap()),
            Err(Error::InvalidGraph(message)) if message.contains("contract digest")
        ));
    }

    #[test]
    fn tracked_resolution_uses_the_current_compatible_child_only_when_requested() {
        let directory = TempDir::new().unwrap();
        let revisions = RevisionStore::open(directory.path()).unwrap();
        let agenda_package = package(
            "agenda",
            "summary",
            export(ValueSchema::empty_record()),
            BTreeMap::new(),
        );
        let digest = agenda_package.contract.digest().unwrap();
        let agenda_first = install(&revisions, agenda_package.clone(), "agenda first");
        let agenda_second = install(&revisions, agenda_package, "agenda second");
        let registry = ExperienceRegistry::open(revisions.clone()).unwrap();
        let agenda_id = ExperienceId::parse("agenda").unwrap();
        registry
            .create(&agenda_id, ExperienceRole::Ordinary, &agenda_first)
            .unwrap();
        registry.set_current(&agenda_id, &agenda_second).unwrap();

        let dashboard = package(
            "dashboard",
            "main",
            export(ValueSchema::empty_record()),
            BTreeMap::from([(
                DependencyAlias::parse("agenda").unwrap(),
                DependencyBinding {
                    experience_id: agenda_id,
                    revision_id: RevisionId::parse(&agenda_first).unwrap(),
                    export_id: ExportId::parse("summary").unwrap(),
                    contract_digest: digest,
                    policy: DependencyPolicy::Tracked,
                    grant: BoundaryGrant::default(),
                },
            )]),
        );
        let dashboard_revision = install(&revisions, dashboard, "dashboard");
        let resolver = GraphResolver::new(revisions);
        let locked = resolver
            .resolve(&dashboard_revision, &ExportId::parse("main").unwrap())
            .unwrap();
        let tracked = resolver
            .resolve_tracked(
                &dashboard_revision,
                &ExportId::parse("main").unwrap(),
                &registry,
            )
            .unwrap();
        let locked_child = locked
            .nodes
            .values()
            .find(|node| node.parent.is_some())
            .unwrap();
        let tracked_child = tracked
            .nodes
            .values()
            .find(|node| node.parent.is_some())
            .unwrap();
        assert_eq!(locked_child.revision_id.as_str(), agenda_first);
        assert_eq!(tracked_child.revision_id.as_str(), agenda_second);
        assert_ne!(locked.id().unwrap(), tracked.id().unwrap());
    }
}
