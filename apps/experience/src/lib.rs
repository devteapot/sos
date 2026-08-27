#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod agent_bridge;
#[cfg(target_os = "android")]
mod android;
#[cfg(any(target_os = "android", test))]
mod android_agent_contract;
#[cfg(any(all(target_os = "android", not(feature = "core-native")), test))]
mod android_interaction_contract;
#[cfg(any(
    target_os = "android",
    all(target_os = "linux", feature = "linux-host")
))]
mod assets;
#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod compositor_fence;
#[cfg(any(all(target_os = "android", feature = "core-native"), test))]
mod core_credential;
#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod linux;
#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod linux_accessibility;
#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod linux_input;
#[cfg(any(
    target_os = "android",
    all(target_os = "linux", feature = "linux-host"),
    all(test, not(target_os = "android"))
))]
#[allow(dead_code)]
#[path = "android/pointer_input.rs"]
mod pointer_input;
#[cfg(any(
    target_os = "android",
    all(target_os = "linux", feature = "linux-host")
))]
mod scene_surface;
#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod shader_paint;
#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod window_space;

#[cfg(all(target_os = "linux", feature = "linux-host"))]
pub use linux::run as run_linux_host;

pub const DEFAULT_EXPERIENCE: &str = include_str!("../../../experiences/default.luau");
pub const TIMEFLOW_EXPERIENCE: &str = include_str!("../../../experiences/timeflow.luau");
pub const STOCK_THEME_MODULE: &str = include_str!("../../../experiences/modules/stock-theme.luau");

pub fn deterministic_agent_candidate(current_source: &str) -> &'static str {
    if current_source.trim() == TIMEFLOW_EXPERIENCE.trim() {
        DEFAULT_EXPERIENCE
    } else {
        TIMEFLOW_EXPERIENCE
    }
}

#[cfg(not(target_os = "android"))]
fn compile_built_in(source: &str) -> Result<runtime_luau::LuauRuntime, runtime_luau::RuntimeError> {
    let sidecars = (source.trim() == DEFAULT_EXPERIENCE.trim())
        .then(|| runtime_luau::RevisionAssetInput {
            id: "stock.theme".into(),
            kind: "luau".into(),
            bytes: STOCK_THEME_MODULE.as_bytes().to_vec(),
        })
        .into_iter()
        .collect();
    runtime_luau::LuauRuntime::compile_with_assets(source, sidecars)
}

#[cfg(not(target_os = "android"))]
pub fn validate_embedded_experience() -> Result<usize, String> {
    let runtime = compile_built_in(DEFAULT_EXPERIENCE).map_err(|error| error.to_string())?;
    let scene = runtime
        .render(&providers_fake::snapshot(), &runtime.initial_state())
        .map_err(|error| error.to_string())?;
    experience_ir::validate_scene(&scene).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_experience_is_valid() {
        let runtime = super::compile_built_in(super::DEFAULT_EXPERIENCE).unwrap();
        let initial_scene = runtime
            .render(&providers_fake::snapshot(), &runtime.initial_state())
            .unwrap();
        fn unstable_interactive_nodes(
            node: &experience_ir::SceneNode,
            path: &str,
            output: &mut Vec<String>,
        ) {
            let interactive = node.layout.scroll_y
                || node.interaction.tap_action.is_some()
                || node.interaction.double_tap_action.is_some()
                || node.interaction.long_press_action.is_some()
                || node.interaction.swipe_action.is_some()
                || node.interaction.pointer_action.is_some()
                || node.interaction.multi_pointer_action.is_some()
                || !node.interaction.hit_regions.is_empty()
                || node.animation.is_some()
                || matches!(node.content, Some(experience_ir::Content::TextSession(_)));
            if interactive && node.id.is_none() {
                output.push(path.to_owned());
            }
            for (index, child) in node.children.iter().enumerate() {
                unstable_interactive_nodes(child, &format!("{path}/{index}"), output);
            }
        }
        let mut unstable = Vec::new();
        unstable_interactive_nodes(&initial_scene.root, "root", &mut unstable);
        assert!(
            unstable.is_empty(),
            "interactive nodes without IDs: {unstable:?}"
        );
        assert!(experience_ir::validate_scene(&initial_scene).unwrap() > 20);
        fn contains_action(node: &experience_ir::SceneNode, action: &str) -> bool {
            node.interaction.tap_action.as_deref() == Some(action)
                || node
                    .children
                    .iter()
                    .any(|child| contains_action(child, action))
        }
        fn contains_agent_composer(node: &experience_ir::SceneNode) -> bool {
            matches!(
                &node.content,
                Some(experience_ir::Content::TextSession(session))
                    if session.submit_action.as_deref() == Some("agent_submit")
            ) || node.children.iter().any(contains_agent_composer)
        }
        fn contains_id(node: &experience_ir::SceneNode, id: &str) -> bool {
            node.id.as_deref() == Some(id)
                || node.children.iter().any(|child| contains_id(child, id))
        }
        fn node_by_id<'a>(
            node: &'a experience_ir::SceneNode,
            id: &str,
        ) -> Option<&'a experience_ir::SceneNode> {
            if node.id.as_deref() == Some(id) {
                return Some(node);
            }
            node.children.iter().find_map(|child| node_by_id(child, id))
        }
        fn contains_wrapping_layout(node: &experience_ir::SceneNode) -> bool {
            node.layout.wrap || node.children.iter().any(contains_wrapping_layout)
        }
        fn window_space(
            node: &experience_ir::SceneNode,
        ) -> Option<&experience_ir::WindowSpaceContent> {
            if let Some(experience_ir::Content::WindowSpace(space)) = &node.content {
                return Some(space);
            }
            node.children.iter().find_map(window_space)
        }
        let mut stock_model = providers_fake::snapshot();
        stock_model.providers.abi_version = experience_ir::SYSTEM_PROVIDER_ABI_VERSION;
        stock_model.providers.audio.volume_percent = Some(50);
        stock_model
            .providers
            .apps
            .compatible
            .push(experience_ir::SystemApplication {
                id: "app-timer".into(),
                label: "Timer".into(),
            });
        stock_model.providers.apps.status_widgets = vec![experience_ir::ApplicationStatusWidget {
            id: "widget-timer".into(),
            label: "TIMER".into(),
            value: "04:20".into(),
            application_id: Some("app-timer".into()),
        }];
        stock_model.providers.capabilities = vec![
            experience_ir::SystemCapability::AudioSetVolume,
            experience_ir::SystemCapability::AppLaunch,
        ];
        assert_eq!(runtime.assets().len(), 1);
        let stock_scene = runtime
            .render(&stock_model, &runtime.initial_state())
            .unwrap();
        assert!(contains_id(&stock_scene.root, "workspace-home"));
        assert!(contains_id(&stock_scene.root, "shell-top-bar"));
        assert!(contains_id(&stock_scene.root, "shell-rail"));
        assert!(contains_action(&stock_scene.root, "toggle_command_center"));
        assert!(contains_action(&stock_scene.root, "toggle_agent_panel"));
        assert!(contains_id(&stock_scene.root, "shell-app-widget-1"));
        assert!(contains_action(&stock_scene.root, "shell_app_widget_1"));
        assert_eq!(
            window_space(&stock_scene.root).map(|space| space.layout),
            Some(experience_ir::WindowLayoutMode::Floating)
        );
        assert!(contains_wrapping_layout(&stock_scene.root));
        let home_grid = node_by_id(&stock_scene.root, "home-responsive-grid").unwrap();
        assert!(home_grid.layout.wrap);
        let content_frame = node_by_id(&stock_scene.root, "stock-content-frame").unwrap();
        assert!(content_frame.layout.scroll_y);
        let command_state = runtime
            .update(
                &stock_model,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "toggle_command_center".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let command_scene = runtime.render(&stock_model, &command_state).unwrap();
        assert!(contains_id(&command_scene.root, "shell-command-center"));
        for workspace in [
            "home",
            "agenda",
            "notes",
            "media",
            "attention",
            "system",
            "apps",
            "agent",
        ] {
            assert!(contains_action(
                &command_scene.root,
                &format!("navigate_{workspace}")
            ));
            let state = runtime
                .update(
                    &stock_model,
                    &runtime.initial_state(),
                    &experience_ir::SceneEvent {
                        action: format!("navigate_{workspace}"),
                        ..Default::default()
                    },
                )
                .unwrap();
            let scene = runtime.render(&stock_model, &state).unwrap();
            assert!(contains_id(&scene.root, &format!("workspace-{workspace}")));
        }
        let tiled_state = runtime
            .update(
                &stock_model,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "window_layout_tiling".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let tiled_scene = runtime.render(&stock_model, &tiled_state).unwrap();
        assert_eq!(
            window_space(&tiled_scene.root).map(|space| space.layout),
            Some(experience_ir::WindowLayoutMode::Tiling)
        );
        let system_state = runtime
            .update(
                &stock_model,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "navigate_system".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let system_scene = runtime.render(&stock_model, &system_state).unwrap();
        assert!(contains_action(&system_scene.root, "audio_volume_up"));
        let agent_state = runtime
            .update(
                &stock_model,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "navigate_agent".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let agent_scene = runtime.render(&stock_model, &agent_state).unwrap();
        assert!(contains_agent_composer(&agent_scene.root));
        let agent_panel_state = runtime
            .update(
                &stock_model,
                &agent_state,
                &experience_ir::SceneEvent {
                    action: "toggle_agent_panel".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let agent_panel_scene = runtime.render(&stock_model, &agent_panel_state).unwrap();
        assert!(contains_id(&agent_panel_scene.root, "shell-rail-agent"));
        assert!(contains_id(&agent_panel_scene.root, "panel-agent-prompt"));
        assert!(experience_ir::validate_scene(&agent_panel_scene).is_ok());
        for source in [super::DEFAULT_EXPERIENCE, super::TIMEFLOW_EXPERIENCE] {
            let runtime = super::compile_built_in(source).unwrap();
            let model = providers_fake::snapshot();
            let state = if source == super::DEFAULT_EXPERIENCE {
                runtime
                    .update(
                        &model,
                        &runtime.initial_state(),
                        &experience_ir::SceneEvent {
                            action: "navigate_agent".into(),
                            ..Default::default()
                        },
                    )
                    .unwrap()
            } else {
                runtime.initial_state()
            };
            let scene = runtime.render(&model, &state).unwrap();
            for action in [
                "agent_configure_openai",
                "agent_configure_openrouter",
                "agent_configure_codex",
                "agent_use_fake",
                "agent_clear_credential",
            ] {
                assert!(!contains_action(&scene.root, action));
            }

            let mut configurable = model;
            configurable.agent.configuration_actions = vec![
                experience_ir::AgentConfigurationAction::ConfigureCodex,
                experience_ir::AgentConfigurationAction::UseFake,
            ];
            let scene = runtime.render(&configurable, &state).unwrap();
            assert!(contains_action(&scene.root, "agent_configure_codex"));
            assert!(contains_action(&scene.root, "agent_use_fake"));
            assert!(!contains_action(&scene.root, "agent_configure_openai"));
            assert!(!contains_action(&scene.root, "agent_configure_openrouter"));
        }
        let outcome = runtime
            .update_with_effects(
                &stock_model,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "audio_volume_up".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(outcome.effects[0].provider, "audio");
        assert_eq!(outcome.effects[0].action, "adjust_volume");
        assert_eq!(outcome.effects[0].payload["delta"], 10);

        let note = runtime
            .update_with_effects(
                &stock_model,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "note_submit".into(),
                    target: Some("note-composer".into()),
                    value: Some("Provider architecture\nKeep applications replaceable.".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(note.effects[0].provider, "notes");
        assert_eq!(note.effects[0].action, "write");
        assert!(note.effects[0].payload["content"]
            .as_str()
            .unwrap()
            .starts_with("# Provider architecture"));
        let note_state = runtime
            .update(
                &stock_model,
                &note.state,
                &experience_ir::SceneEvent {
                    action: "navigate_notes".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let note_scene = runtime.render(&stock_model, &note_state).unwrap();
        let note_status = node_by_id(&note_scene.root, "note-composer-status").unwrap();
        assert_eq!(
            note_status.semantics.as_ref().unwrap().role,
            experience_ir::SemanticRole::Status
        );
        assert_eq!(
            note_status.semantics.as_ref().unwrap().label,
            "Saved “Provider architecture”."
        );

        let agenda = runtime
            .update_with_effects(
                &stock_model,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "agenda_submit".into(),
                    target: Some("agenda-composer".into()),
                    value: Some("09:30 Provider review".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(agenda.effects[0].provider, "calendar");
        assert_eq!(agenda.effects[0].action, "append");
        assert_eq!(agenda.effects[0].payload["time"], "09:30");

        let mut unavailable = providers_fake::snapshot();
        unavailable.providers = Default::default();
        let unavailable_state = runtime
            .update(
                &unavailable,
                &runtime.initial_state(),
                &experience_ir::SceneEvent {
                    action: "navigate_system".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let unavailable_scene = runtime.render(&unavailable, &unavailable_state).unwrap();
        assert!(contains_id(
            &unavailable_scene.root,
            "system-providers-unavailable"
        ));
        assert!(!contains_action(&unavailable_scene.root, "audio_volume_up"));

        let timeflow = runtime_luau::LuauRuntime::compile(super::TIMEFLOW_EXPERIENCE).unwrap();
        let timeflow_scene = timeflow
            .render(&providers_fake::snapshot(), &timeflow.initial_state())
            .unwrap();
        assert!(experience_ir::validate_scene(&timeflow_scene).unwrap() > 15);
        assert!(contains_action(&timeflow_scene.root, "toggle_music"));

        let runtime = runtime_luau::LuauRuntime::compile(super::TIMEFLOW_EXPERIENCE).unwrap();
        let model = providers_fake::snapshot();
        let state = runtime.initial_state();
        let scene = runtime.render(&model, &state).unwrap();
        assert!(contains_agent_composer(&scene.root));
        let outcome = runtime
            .update_with_effects(
                &model,
                &state,
                &experience_ir::SceneEvent {
                    action: "agent_submit".into(),
                    target: Some("agent-prompt".into()),
                    value: Some("Make this calmer".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(outcome.effects.len(), 1);
        assert_eq!(outcome.effects[0].provider, "agent");
        assert_eq!(outcome.effects[0].action, "prompt");
        assert_eq!(outcome.effects[0].payload["prompt"], "Make this calmer");
    }

    #[test]
    fn deterministic_agent_candidate_is_complete_and_visibly_alternates() {
        let first = super::deterministic_agent_candidate(super::TIMEFLOW_EXPERIENCE);
        assert_eq!(first.trim(), super::DEFAULT_EXPERIENCE.trim());
        let second = super::deterministic_agent_candidate(first);
        assert_eq!(second.trim(), super::TIMEFLOW_EXPERIENCE.trim());

        for source in [first, second] {
            let runtime = super::compile_built_in(source).unwrap();
            let scene = runtime
                .render(&providers_fake::snapshot(), &runtime.initial_state())
                .unwrap();
            assert!(experience_ir::validate_scene(&scene).unwrap() > 15);
        }
    }
}
