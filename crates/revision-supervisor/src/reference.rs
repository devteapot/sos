use std::collections::{BTreeMap, BTreeSet};

use experience_package::{
    hex_sha256, BoundaryGrant, DependencyAlias, DependencyBinding, DependencyPolicy,
    DerivationKind, DerivationParent, DerivationRecord, EventId, ExperienceContract,
    ExperienceExport, ExperienceId, ExperienceRole, ExportId, FieldSchema, PackageMetadata,
    RevisionId, ValueSchema, ViewportContract, APPEARANCE_ABI_VERSION, CONTRACT_VERSION,
    PACKAGE_FORMAT_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    ExperienceRegistry, GraphResolver, GraphStore, Result, RevisionInput, RevisionPackageInput,
    RevisionStore,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReferenceComposition {
    pub agenda_revision: String,
    pub media_revision: String,
    pub dashboard_revision: String,
    pub dashboard_graph: String,
    pub remix_revision: String,
}

pub fn install_reference_composition(store: &RevisionStore) -> Result<ReferenceComposition> {
    let agenda_id = ExperienceId::parse("sos.example.agenda").expect("static experience ID");
    let media_id = ExperienceId::parse("sos.example.media").expect("static experience ID");
    let dashboard_id = ExperienceId::parse("sos.example.dashboard").expect("static experience ID");
    let remix_id =
        ExperienceId::parse("sos.example.agenda-media-remix").expect("static experience ID");
    let summary = ExportId::parse("summary").expect("static export ID");
    let main = ExportId::parse("main").expect("static export ID");

    let string = || ValueSchema::String {
        max_bytes: 256,
        choices: BTreeSet::new(),
    };
    let agenda_contract = ExperienceContract {
        contract_version: CONTRACT_VERSION,
        exports: BTreeMap::from([
            (
                main.clone(),
                export(ValueSchema::empty_record(), BTreeMap::new(), false),
            ),
            (
                summary.clone(),
                export(
                    ValueSchema::Record {
                        fields: BTreeMap::from([(
                            "title".into(),
                            FieldSchema {
                                required: true,
                                value: string(),
                            },
                        )]),
                    },
                    BTreeMap::from([(
                        EventId::parse("open").expect("static event ID"),
                        ValueSchema::Record {
                            fields: BTreeMap::from([(
                                "item".into(),
                                FieldSchema {
                                    required: true,
                                    value: string(),
                                },
                            )]),
                        },
                    )]),
                    true,
                ),
            ),
        ]),
    };
    let media_contract = ExperienceContract {
        contract_version: CONTRACT_VERSION,
        exports: BTreeMap::from([
            (
                main.clone(),
                export(ValueSchema::empty_record(), BTreeMap::new(), false),
            ),
            (
                summary.clone(),
                export(
                    ValueSchema::empty_record(),
                    BTreeMap::from([(
                        EventId::parse("playback_changed").expect("static event ID"),
                        ValueSchema::Record {
                            fields: BTreeMap::from([(
                                "playing".into(),
                                FieldSchema {
                                    required: true,
                                    value: ValueSchema::Boolean,
                                },
                            )]),
                        },
                    )]),
                    false,
                ),
            ),
        ]),
    };
    let agenda_revision = install(
        store,
        include_str!("../../../experiences/composition/agenda.luau"),
        package(
            agenda_id.clone(),
            agenda_contract.clone(),
            BTreeMap::new(),
            original(),
        ),
    )?;
    let media_revision = install(
        store,
        include_str!("../../../experiences/composition/media.luau"),
        package(
            media_id.clone(),
            media_contract.clone(),
            BTreeMap::new(),
            original(),
        ),
    )?;
    let dashboard_contract = ExperienceContract {
        contract_version: CONTRACT_VERSION,
        exports: BTreeMap::from([(
            main.clone(),
            export(ValueSchema::empty_record(), BTreeMap::new(), false),
        )]),
    };
    let dashboard_revision = install(
        store,
        include_str!("../../../experiences/composition/dashboard.luau"),
        package(
            dashboard_id.clone(),
            dashboard_contract,
            BTreeMap::from([
                (
                    DependencyAlias::parse("agenda").expect("static alias"),
                    DependencyBinding {
                        experience_id: agenda_id.clone(),
                        revision_id: RevisionId::parse(&agenda_revision)
                            .expect("installed revision ID"),
                        export_id: summary.clone(),
                        contract_digest: agenda_contract
                            .digest()
                            .map_err(|error| crate::Error::InvalidGraph(error.to_string()))?,
                        policy: DependencyPolicy::Locked,
                        grant: BoundaryGrant {
                            properties: BTreeSet::from(["title".into()]),
                            events: BTreeSet::from([
                                EventId::parse("open").expect("static event ID")
                            ]),
                        },
                    },
                ),
                (
                    DependencyAlias::parse("media").expect("static alias"),
                    DependencyBinding {
                        experience_id: media_id.clone(),
                        revision_id: RevisionId::parse(&media_revision)
                            .expect("installed revision ID"),
                        export_id: summary,
                        contract_digest: media_contract
                            .digest()
                            .map_err(|error| crate::Error::InvalidGraph(error.to_string()))?,
                        policy: DependencyPolicy::Locked,
                        grant: BoundaryGrant {
                            properties: BTreeSet::new(),
                            events: BTreeSet::from([
                                EventId::parse("playback_changed").expect("static event ID")
                            ]),
                        },
                    },
                ),
            ]),
            original(),
        ),
    )?;
    let remix_revision = install(
        store,
        include_str!("../../../experiences/composition/agenda-media-remix.luau"),
        package(
            remix_id.clone(),
            ExperienceContract {
                contract_version: CONTRACT_VERSION,
                exports: BTreeMap::from([(
                    main.clone(),
                    export(ValueSchema::empty_record(), BTreeMap::new(), false),
                )]),
            },
            BTreeMap::new(),
            DerivationRecord {
                kind: DerivationKind::Remix,
                parents: vec![
                    DerivationParent {
                        experience_id: agenda_id.clone(),
                        revision_id: RevisionId::parse(&agenda_revision)
                            .expect("installed revision ID"),
                    },
                    DerivationParent {
                        experience_id: media_id.clone(),
                        revision_id: RevisionId::parse(&media_revision)
                            .expect("installed revision ID"),
                    },
                ],
                request_sha256: Some(hex_sha256(
                    b"Combine Agenda and Media into one integrated experience",
                )),
                rationale: Some(
                    "One information architecture and shared interaction state were requested."
                        .into(),
                ),
            },
        ),
    )?;

    let registry = ExperienceRegistry::open(store.clone())?;
    for (id, revision) in [
        (&agenda_id, &agenda_revision),
        (&media_id, &media_revision),
        (&dashboard_id, &dashboard_revision),
        (&remix_id, &remix_revision),
    ] {
        if registry.get(id)?.is_none() {
            registry.create(id, ExperienceRole::Ordinary, revision)?;
        } else {
            registry.set_current(id, revision)?;
        }
    }
    crate::ReverseDependencyIndex::open(store.root()).rebuild(store, &registry)?;
    let graph = GraphResolver::new(store.clone()).resolve(&dashboard_revision, &main)?;
    let graphs = GraphStore::open(store.root())?;
    let dashboard_graph = graphs.install(&graph)?;
    graphs.set_current(&dashboard_id, &dashboard_graph)?;

    Ok(ReferenceComposition {
        agenda_revision,
        media_revision,
        dashboard_revision,
        dashboard_graph,
        remix_revision,
    })
}

fn install(store: &RevisionStore, source: &str, package: PackageMetadata) -> Result<String> {
    Ok(store
        .install_package(RevisionPackageInput {
            revision: RevisionInput {
                source: source.as_bytes().to_vec(),
                state: json!({}),
                schema_version: 1,
                experience_api_version: 4,
                assets: vec![],
            },
            package,
        })?
        .manifest
        .revision_id)
}

fn package(
    experience_id: ExperienceId,
    contract: ExperienceContract,
    dependencies: BTreeMap<DependencyAlias, DependencyBinding>,
    derivation: DerivationRecord,
) -> PackageMetadata {
    PackageMetadata {
        format_version: PACKAGE_FORMAT_VERSION,
        experience_id,
        role: ExperienceRole::Ordinary,
        contract,
        dependencies,
        derivation,
    }
}

fn export(
    properties: ValueSchema,
    events: BTreeMap<EventId, ValueSchema>,
    accepts_container_appearance: bool,
) -> ExperienceExport {
    ExperienceExport {
        properties,
        events,
        viewport: ViewportContract {
            min_width: 160,
            min_height: 96,
            max_width: 1920,
            max_height: 1080,
        },
        appearance_abi: APPEARANCE_ABI_VERSION,
        accepts_container_appearance,
    }
}

fn original() -> DerivationRecord {
    DerivationRecord {
        kind: DerivationKind::Original,
        parents: vec![],
        request_sha256: None,
        rationale: None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use experience_package::ColorScheme;
    use runtime_luau::{GraphRevisionInput, GraphRuntime, RuntimeInstanceStatus};
    use tempfile::TempDir;

    use super::*;
    use crate::DurableState;

    #[test]
    fn reference_gate_installs_a_live_graph_and_a_self_contained_remix() {
        let directory = TempDir::new().unwrap();
        let store = RevisionStore::open(directory.path()).unwrap();
        let installed = install_reference_composition(&store).unwrap();
        let graphs = GraphStore::open(directory.path()).unwrap();
        let graph = graphs.verify(&installed.dashboard_graph).unwrap();
        assert_eq!(graph.nodes.len(), 3);

        let mut inputs = BTreeMap::new();
        let model = providers_fake::snapshot();
        for node in graph.nodes.values() {
            let revision = store.verify(node.revision_id.as_str()).unwrap();
            let durable: DurableState = serde_json::from_slice(
                &fs::read(revision.directory.join(&revision.manifest.state.path)).unwrap(),
            )
            .unwrap();
            inputs
                .entry(node.revision_id.clone())
                .or_insert_with(|| GraphRevisionInput {
                    source: fs::read_to_string(
                        revision.directory.join(&revision.manifest.source.path),
                    )
                    .unwrap(),
                    sidecars: vec![],
                    model: model.clone(),
                    state: durable.state,
                    state_schema_version: durable.schema_version,
                    package: revision.package.unwrap(),
                });
        }
        let mut runtime = GraphRuntime::start(graph, inputs).unwrap();
        let before = runtime.snapshot();
        assert!(before
            .instances
            .values()
            .all(|instance| instance.status == RuntimeInstanceStatus::Ready));
        let agenda = before
            .instances
            .iter()
            .find(|(_, instance)| instance.experience_id.as_str() == "sos.example.agenda")
            .map(|(node, instance)| (node.clone(), instance.scene.clone()))
            .unwrap();
        let media_before = before
            .instances
            .values()
            .find(|instance| instance.experience_id.as_str() == "sos.example.media")
            .unwrap()
            .scene
            .clone();
        let outcome = runtime
            .dispatch_event(
                &agenda.0,
                &json!({"action":"open_first", "target":"agenda-open"}),
            )
            .unwrap();
        let root = &outcome.snapshot.instances[&outcome.snapshot.root];
        assert_eq!(root.state["opened"], "Design review");
        assert_eq!(
            outcome.snapshot.instances[&agenda.0].state["selected"],
            "Design review"
        );

        let mut appearance = model.appearance;
        appearance.generation = 1;
        appearance.scheme = ColorScheme::Light;
        let themed = runtime.apply_appearance(appearance).unwrap();
        assert_ne!(
            themed.instances[&agenda.0].scene, agenda.1,
            "inheriting child must rerender after appearance change"
        );
        assert_eq!(
            themed
                .instances
                .values()
                .find(|instance| instance.experience_id.as_str() == "sos.example.media")
                .unwrap()
                .scene,
            media_before,
            "custom child keeps its local visual result"
        );

        let remix = store.verify(&installed.remix_revision).unwrap();
        let package = remix.package.unwrap();
        assert!(package.dependencies.is_empty());
        assert_eq!(package.derivation.kind, DerivationKind::Remix);
        assert_eq!(package.derivation.parents.len(), 2);
    }
}
