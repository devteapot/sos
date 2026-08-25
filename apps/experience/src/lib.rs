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
pub use linux::run as run_linux_host;

pub const DEFAULT_EXPERIENCE: &str = include_str!("../../../experiences/default.luau");
pub const TIMEFLOW_EXPERIENCE: &str = include_str!("../../../experiences/timeflow.luau");
pub const DAILY_FLOW_EXPERIENCE: &str = include_str!("../../../experiences/daily-flow.luau");
pub const DAILY_FLOW_AGENT_EXPERIENCE: &str =
    include_str!("../../../experiences/daily-flow-agent.luau");

pub fn deterministic_agent_candidate(current_source: &str) -> &'static str {
    if current_source.trim() == DAILY_FLOW_EXPERIENCE.trim() {
        TIMEFLOW_EXPERIENCE
    } else {
        DAILY_FLOW_EXPERIENCE
    }
}

#[cfg(not(target_os = "android"))]
pub fn validate_embedded_experience() -> Result<usize, String> {
    let runtime = runtime_luau::LuauRuntime::compile(DEFAULT_EXPERIENCE)
        .map_err(|error| error.to_string())?;
    let scene = runtime
        .render(&providers_fake::snapshot(), &runtime.initial_state())
        .map_err(|error| error.to_string())?;
    experience_ir::validate_scene(&scene).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_experience_is_valid() {
        assert!(super::validate_embedded_experience().unwrap() > 20);

        let runtime = runtime_luau::LuauRuntime::compile(super::DEFAULT_EXPERIENCE).unwrap();
        let _scene = runtime
            .render(&providers_fake::snapshot(), &runtime.initial_state())
            .unwrap();
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
        let mut stock_model = providers_fake::snapshot();
        stock_model.providers.abi_version = experience_ir::SYSTEM_PROVIDER_ABI_VERSION;
        stock_model.providers.audio.volume_percent = Some(50);
        stock_model.providers.capabilities = vec![experience_ir::SystemCapability::AudioSetVolume];
        let stock_scene = runtime
            .render(&stock_model, &runtime.initial_state())
            .unwrap();
        assert!(contains_action(&stock_scene.root, "audio_volume_up"));
        assert!(contains_agent_composer(&stock_scene.root));
        for source in [
            super::DEFAULT_EXPERIENCE,
            super::TIMEFLOW_EXPERIENCE,
            super::DAILY_FLOW_EXPERIENCE,
        ] {
            let runtime = runtime_luau::LuauRuntime::compile(source).unwrap();
            let model = providers_fake::snapshot();
            let scene = runtime.render(&model, &runtime.initial_state()).unwrap();
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
            let scene = runtime
                .render(&configurable, &runtime.initial_state())
                .unwrap();
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

        let timeflow = runtime_luau::LuauRuntime::compile(super::TIMEFLOW_EXPERIENCE).unwrap();
        let timeflow_scene = timeflow
            .render(&providers_fake::snapshot(), &timeflow.initial_state())
            .unwrap();
        assert!(experience_ir::validate_scene(&timeflow_scene).unwrap() > 15);
        assert!(contains_action(&timeflow_scene.root, "toggle_music"));

        for source in [
            super::DAILY_FLOW_EXPERIENCE,
            super::DAILY_FLOW_AGENT_EXPERIENCE,
        ] {
            let runtime = runtime_luau::LuauRuntime::compile(source).unwrap();
            let scene = runtime
                .render(&providers_fake::snapshot(), &runtime.initial_state())
                .unwrap();
            assert!(experience_ir::validate_scene(&scene).unwrap() > 15);
            assert!(contains_action(&scene.root, "toggle_music"));
        }

        for source in [super::TIMEFLOW_EXPERIENCE, super::DAILY_FLOW_EXPERIENCE] {
            let runtime = runtime_luau::LuauRuntime::compile(source).unwrap();
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
    }

    #[test]
    fn deterministic_agent_candidate_is_complete_and_visibly_alternates() {
        let first = super::deterministic_agent_candidate(super::TIMEFLOW_EXPERIENCE);
        assert_eq!(first.trim(), super::DAILY_FLOW_EXPERIENCE.trim());
        let second = super::deterministic_agent_candidate(first);
        assert_eq!(second.trim(), super::TIMEFLOW_EXPERIENCE.trim());

        for source in [first, second] {
            let runtime = runtime_luau::LuauRuntime::compile(source).unwrap();
            let scene = runtime
                .render(&providers_fake::snapshot(), &runtime.initial_state())
                .unwrap();
            assert!(experience_ir::validate_scene(&scene).unwrap() > 15);
        }
    }
}
