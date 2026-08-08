use experience_ir::{AccessibilityRole, NodeKind, UiNode};
use gpui_mobile::android::jni::{activity, find_app_class, with_env};
use jni::objects::JValue;

pub fn summary(root: &UiNode) -> String {
    fn visit(node: &UiNode, parts: &mut Vec<String>) {
        if let Some(semantic) = &node.accessibility {
            let role = match semantic.role {
                AccessibilityRole::Button => "button",
                AccessibilityRole::Image => "image",
                AccessibilityRole::TextField => "text field",
                AccessibilityRole::Header => "heading",
                AccessibilityRole::Status => "status",
            };
            let mut part = format!("{role}: {}", semantic.label);
            if let Some(value) = &semantic.value {
                if !value.is_empty() {
                    part.push_str(", ");
                    part.push_str(value);
                }
            }
            if let Some(hint) = &semantic.hint {
                if !hint.is_empty() {
                    part.push_str(". ");
                    part.push_str(hint);
                }
            }
            parts.push(part);
        } else if let NodeKind::Text(text) = &node.kind {
            if parts.len() < 2 {
                parts.push(text.clone());
            }
        }
        for child in &node.children {
            visit(child, parts);
        }
    }

    let mut parts = Vec::new();
    visit(root, &mut parts);
    parts.join("; ")
}

pub fn publish(summary: &str) -> Result<(), String> {
    with_env(|env| {
        let helper = find_app_class(env, "dev.gpui.mobile.GpuiPlatformView")?;
        let activity = activity(env)?;
        let summary = env.new_string(summary).map_err(|error| error.to_string())?;
        env.call_static_method(
            &helper,
            jni::jni_str!("updateAccessibilitySummary"),
            jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)V"),
            &[JValue::Object(&activity), JValue::Object(&summary)],
        )
        .map_err(|error| {
            env.exception_clear();
            format!("accessibility JNI call failed: {error}")
        })?;
        Ok(())
    })
}
