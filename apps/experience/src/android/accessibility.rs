use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
};

use experience_ir::{Content, Scene, SceneNode, SemanticRole};
use gpui::{Bounds, Pixels};
use gpui_mobile::android::jni::{activity, find_app_class, get_string, with_env};
use jni::objects::{JObject, JValue};
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct Action {
    pub kind: String,
    pub target: String,
    pub value: String,
}

static BOUNDS: OnceLock<Mutex<HashMap<String, [f32; 4]>>> = OnceLock::new();
static BOUNDS_CHANGED: AtomicBool = AtomicBool::new(true);
static ACTIONS: OnceLock<Mutex<VecDeque<Action>>> = OnceLock::new();

pub fn record_bounds(id: &str, bounds: Bounds<Pixels>) {
    let value = [
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    ];
    let mut all = BOUNDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("accessibility bounds lock");
    if all.get(id) != Some(&value) {
        all.insert(id.to_owned(), value);
        BOUNDS_CHANGED.store(true, Ordering::Release);
    }
}

pub fn take_bounds_changed() -> bool {
    BOUNDS_CHANGED.swap(false, Ordering::AcqRel)
}

pub fn take_action() -> Option<Action> {
    ACTIONS
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .expect("accessibility action lock")
        .pop_front()
}

fn role_name(role: SemanticRole) -> &'static str {
    match role {
        SemanticRole::Button => "button",
        SemanticRole::Image => "image",
        SemanticRole::TextField => "text_field",
        SemanticRole::Header => "header",
        SemanticRole::Status => "status",
    }
}

pub fn summary(scene: &Scene) -> String {
    fn visit(node: &SceneNode, parts: &mut Vec<String>) {
        if let Some(semantic) = &node.semantics {
            let mut part = format!("{}: {}", role_name(semantic.role), semantic.label);
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
        } else if let Some(Content::Text(text)) = &node.content {
            if parts.len() < 2 {
                parts.push(text.value.clone());
            }
        }
        for child in &node.children {
            visit(child, parts);
        }
    }

    let mut parts = Vec::new();
    visit(&scene.root, &mut parts);
    parts.join("; ")
}

fn snapshot(scene: &Scene) -> Value {
    fn visit(
        node: &SceneNode,
        semantic_parent: Option<&str>,
        bounds: &HashMap<String, [f32; 4]>,
        nodes: &mut Vec<Value>,
    ) {
        let mut next_parent = semantic_parent;
        if let (Some(id), Some(semantic)) = (node.id.as_deref(), node.semantics.as_ref()) {
            let value = match &node.content {
                Some(Content::TextSession(input)) => Some(input.value.as_str()),
                _ => semantic.value.as_deref(),
            };
            nodes.push(json!({
                "id": id,
                "parent": semantic_parent,
                "role": role_name(semantic.role),
                "label": semantic.label,
                "value": value,
                "hint": semantic.hint,
                "bounds": bounds.get(id).copied().unwrap_or([0.0; 4]),
                "click_action": node.interaction.tap_action,
                "editable": matches!(node.content, Some(Content::TextSession(_))),
            }));
            next_parent = Some(id);
        }
        for child in &node.children {
            visit(child, next_parent, bounds, nodes);
        }
    }

    let bounds = BOUNDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("accessibility bounds lock");
    let mut nodes = Vec::new();
    visit(&scene.root, None, &bounds, &mut nodes);
    json!({ "summary": summary(scene), "nodes": nodes })
}

pub fn publish(scene: &Scene) -> Result<usize, String> {
    let payload = serde_json::to_string(&snapshot(scene)).map_err(|error| error.to_string())?;
    let bytes = payload.len();
    with_env(|env| {
        let helper = find_app_class(env, "dev.gpui.mobile.GpuiPlatformView")?;
        let activity = activity(env)?;
        let payload = env.new_string(payload).map_err(|error| error.to_string())?;
        env.call_static_method(
            &helper,
            jni::jni_str!("updateAccessibilityTree"),
            jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)V"),
            &[JValue::Object(&activity), JValue::Object(&payload)],
        )
        .map_err(|error| {
            env.exception_clear();
            format!("accessibility JNI call failed: {error}")
        })?;
        Ok(bytes)
    })
}

/// Receives actions from Android's virtual accessibility nodes. The Android
/// render loop polls this bounded queue and applies them on the GPUI thread.
#[no_mangle]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnAccessibilityAction(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    kind: *mut std::ffi::c_void,
    target: *mut std::ffi::c_void,
    value: *mut std::ffi::c_void,
) {
    let _ = with_env(|env| {
        let kind = JObject::from_raw(env, kind as jni::sys::jobject);
        let target = JObject::from_raw(env, target as jni::sys::jobject);
        let value = JObject::from_raw(env, value as jni::sys::jobject);
        let action = Action {
            kind: get_string(env, &kind),
            target: get_string(env, &target),
            value: get_string(env, &value),
        };
        if action.kind.is_empty() || action.target.is_empty() {
            return Ok(());
        }
        let mut actions = ACTIONS
            .get_or_init(|| Mutex::new(VecDeque::new()))
            .lock()
            .expect("accessibility action lock");
        if actions.len() >= 32 {
            actions.pop_front();
        }
        actions.push_back(action);
        Ok(())
    });
}
