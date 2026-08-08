#[cfg(target_os = "android")]
mod android;
#[cfg(any(
    target_os = "android",
    all(target_os = "linux", feature = "linux-host")
))]
mod assets;
#[cfg(all(target_os = "linux", feature = "linux-host"))]
mod linux;
#[cfg(all(test, not(target_os = "android")))]
#[allow(dead_code)]
#[path = "android/pointer_input.rs"]
mod pointer_input;
#[cfg(any(
    target_os = "android",
    all(target_os = "linux", feature = "linux-host")
))]
mod scene_surface;

#[cfg(all(target_os = "linux", feature = "linux-host"))]
pub use linux::run as run_linux_host;

pub const DEFAULT_EXPERIENCE: &str = include_str!("../../../experiences/default.luau");
pub const TIMEFLOW_EXPERIENCE: &str = include_str!("../../../experiences/timeflow.luau");
pub const DAILY_FLOW_EXPERIENCE: &str = include_str!("../../../experiences/daily-flow.luau");
pub const DAILY_FLOW_AGENT_EXPERIENCE: &str =
    include_str!("../../../experiences/daily-flow-agent.luau");

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
        let scene = runtime
            .render(&providers_fake::snapshot(), &runtime.initial_state())
            .unwrap();
        fn contains_action(node: &experience_ir::SceneNode, action: &str) -> bool {
            node.interaction.tap_action.as_deref() == Some(action)
                || node
                    .children
                    .iter()
                    .any(|child| contains_action(child, action))
        }
        assert!(contains_action(&scene.root, "toggle_music"));

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
    }
}
