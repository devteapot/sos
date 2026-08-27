use experience_ir::{Content, Scene, SceneNode};
use experience_package::GraphNodeId;
use runtime_luau::{GraphRuntimeSnapshot, RuntimeInstanceStatus};

pub(crate) fn composed_graph_scene(snapshot: &GraphRuntimeSnapshot) -> Scene {
    fn compose(
        snapshot: &GraphRuntimeSnapshot,
        owner: &GraphNodeId,
        node: &SceneNode,
    ) -> SceneNode {
        let mut composed = node.clone();
        let instance_id = &snapshot.instances[owner].instance_id;
        composed.id = node.id.as_ref().map(|id| format!("{instance_id}::{id}"));
        composed.children = node
            .children
            .iter()
            .map(|child| compose(snapshot, owner, child))
            .collect();

        if let Some(Content::ExperienceMount(mount)) = &node.content {
            composed.content = None;
            if let Some((child_id, child)) = snapshot.instances.iter().find(|(_, instance)| {
                instance.parent.as_ref() == Some(owner)
                    && instance.dependency.as_ref().map(|alias| alias.as_str())
                        == Some(mount.dependency.as_str())
            }) {
                if child.status == RuntimeInstanceStatus::Ready {
                    if let Some(scene) = &child.scene {
                        composed
                            .children
                            .push(compose(snapshot, child_id, &scene.root));
                    }
                }
            }
        }

        composed
    }

    let root = snapshot
        .instances
        .get(&snapshot.root)
        .and_then(|instance| instance.scene.as_ref())
        .map(|scene| compose(snapshot, &snapshot.root, &scene.root))
        .unwrap_or_default();
    Scene { root }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use experience_ir::{Content, ExperienceMountContent, Scene, SceneNode};
    use experience_package::{
        DependencyAlias, ExperienceId, ExportId, GraphNodeId, InstanceId, RevisionId,
    };
    use runtime_luau::{GraphRuntimeSnapshot, RuntimeInstanceSnapshot, RuntimeInstanceStatus};

    use super::composed_graph_scene;

    fn node(id: &str, children: Vec<SceneNode>) -> SceneNode {
        SceneNode {
            id: Some(id.into()),
            children,
            ..Default::default()
        }
    }

    fn mount(id: &str, dependency: &str) -> SceneNode {
        SceneNode {
            id: Some(id.into()),
            content: Some(Content::ExperienceMount(ExperienceMountContent {
                dependency: dependency.into(),
                properties: serde_json::json!({}),
                container_appearance: None,
            })),
            ..Default::default()
        }
    }

    fn instance(
        instance_id: &str,
        experience_id: &str,
        revision_byte: char,
        parent: Option<GraphNodeId>,
        dependency: Option<&str>,
        scene: Option<Scene>,
        status: RuntimeInstanceStatus,
    ) -> RuntimeInstanceSnapshot {
        RuntimeInstanceSnapshot {
            instance_id: InstanceId::parse(instance_id).unwrap(),
            experience_id: ExperienceId::parse(experience_id).unwrap(),
            revision_id: RevisionId::parse(revision_byte.to_string().repeat(64)).unwrap(),
            export_id: ExportId::parse("main").unwrap(),
            parent,
            dependency: dependency.map(|alias| DependencyAlias::parse(alias).unwrap()),
            state: serde_json::json!({}),
            scene,
            status,
            assets: Vec::new(),
        }
    }

    fn collect_ids<'a>(node: &'a SceneNode, output: &mut Vec<&'a str>) {
        if let Some(id) = node.id.as_deref() {
            output.push(id);
        }
        for child in &node.children {
            collect_ids(child, output);
        }
    }

    #[test]
    fn namespaces_mounted_children_and_contains_failed_siblings() {
        let root = GraphNodeId::parse("root").unwrap();
        let agenda = GraphNodeId::parse("agenda-child").unwrap();
        let media = GraphNodeId::parse("media-child").unwrap();
        let snapshot = GraphRuntimeSnapshot {
            graph_id: "d".repeat(64),
            root: root.clone(),
            instances: BTreeMap::from([
                (
                    root.clone(),
                    instance(
                        "root-instance",
                        "sos.example.dashboard",
                        'a',
                        None,
                        None,
                        Some(Scene {
                            root: node(
                                "shared-action",
                                vec![mount("agenda-slot", "agenda"), mount("media-slot", "media")],
                            ),
                        }),
                        RuntimeInstanceStatus::Ready,
                    ),
                ),
                (
                    agenda,
                    instance(
                        "agenda-instance",
                        "sos.example.agenda",
                        'b',
                        Some(root.clone()),
                        Some("agenda"),
                        Some(Scene {
                            root: node("shared-action", vec![node("agenda-row", Vec::new())]),
                        }),
                        RuntimeInstanceStatus::Ready,
                    ),
                ),
                (
                    media,
                    instance(
                        "media-instance",
                        "sos.example.media",
                        'c',
                        Some(root),
                        Some("media"),
                        Some(Scene {
                            root: node("stale-media-scene", Vec::new()),
                        }),
                        RuntimeInstanceStatus::Failed("injected child failure".into()),
                    ),
                ),
            ]),
        };

        let composed = composed_graph_scene(&snapshot);
        assert_eq!(
            composed.root.id.as_deref(),
            Some("root-instance::shared-action")
        );
        let agenda_slot = &composed.root.children[0];
        assert_eq!(
            agenda_slot.id.as_deref(),
            Some("root-instance::agenda-slot")
        );
        assert!(agenda_slot.content.is_none());
        assert_eq!(agenda_slot.children.len(), 1);
        assert_eq!(
            agenda_slot.children[0].id.as_deref(),
            Some("agenda-instance::shared-action")
        );
        let media_slot = &composed.root.children[1];
        assert_eq!(media_slot.id.as_deref(), Some("root-instance::media-slot"));
        assert!(media_slot.content.is_none());
        assert!(media_slot.children.is_empty());

        let mut ids = Vec::new();
        collect_ids(&composed.root, &mut ids);
        assert_eq!(
            ids.len(),
            ids.iter().copied().collect::<BTreeSet<_>>().len()
        );
        assert!(ids.iter().all(|id| id.contains("::")));
    }
}
