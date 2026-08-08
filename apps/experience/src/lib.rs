#[cfg(target_os = "android")]
mod android;

pub const DEFAULT_EXPERIENCE: &str = include_str!("../../../experiences/default.luau");
pub const TIMEFLOW_EXPERIENCE: &str = include_str!("../../../experiences/timeflow.luau");

#[cfg(not(target_os = "android"))]
pub fn validate_embedded_experience() -> Result<usize, String> {
    let runtime = runtime_luau::LuauRuntime::compile(DEFAULT_EXPERIENCE)
        .map_err(|error| error.to_string())?;
    let tree = runtime
        .render(&providers_fake::snapshot(), &runtime.initial_state())
        .map_err(|error| error.to_string())?;
    experience_ir::validate_tree(&tree).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_experience_is_valid() {
        assert!(super::validate_embedded_experience().unwrap() > 20);

        let runtime = runtime_luau::LuauRuntime::compile(super::DEFAULT_EXPERIENCE).unwrap();
        let tree = runtime
            .render(&providers_fake::snapshot(), &runtime.initial_state())
            .unwrap();
        fn contains_action(node: &experience_ir::UiNode, action: &str) -> bool {
            node.action.as_deref() == Some(action)
                || node
                    .children
                    .iter()
                    .any(|child| contains_action(child, action))
        }
        assert!(contains_action(&tree, "toggle_music"));

        let timeflow = runtime_luau::LuauRuntime::compile(super::TIMEFLOW_EXPERIENCE).unwrap();
        let timeflow_tree = timeflow
            .render(&providers_fake::snapshot(), &timeflow.initial_state())
            .unwrap();
        assert!(experience_ir::validate_tree(&timeflow_tree).unwrap() > 15);
        assert!(contains_action(&timeflow_tree, "toggle_music"));
    }
}
