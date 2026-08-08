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

use super::native_input::AccessibilityTextState;

#[derive(Clone, Debug)]
pub struct Action {
    pub kind: String,
    pub target: String,
    pub value: String,
}

static BOUNDS: OnceLock<Mutex<HashMap<String, [f32; 4]>>> = OnceLock::new();
static BOUNDS_CHANGED: AtomicBool = AtomicBool::new(true);
static STATE_CHANGED: AtomicBool = AtomicBool::new(true);
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

pub fn bounds(id: &str) -> Option<[f32; 4]> {
    BOUNDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("accessibility bounds lock")
        .get(id)
        .copied()
}

pub fn take_bounds_changed() -> bool {
    BOUNDS_CHANGED.swap(false, Ordering::AcqRel)
}

pub fn mark_state_changed() {
    STATE_CHANGED.store(true, Ordering::Release);
}

pub fn take_state_changed() -> bool {
    STATE_CHANGED.swap(false, Ordering::AcqRel)
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
        SemanticRole::ScrollArea => "scroll_area",
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

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollState {
    pub offset_y: f32,
    pub max_offset_y: f32,
    pub bounds: [f32; 4],
}

fn snapshot(
    scene: &Scene,
    text: &HashMap<String, AccessibilityTextState>,
    scroll: &HashMap<String, ScrollState>,
) -> Value {
    fn visit(
        node: &SceneNode,
        semantic_parent: Option<&str>,
        bounds: &HashMap<String, [f32; 4]>,
        text: &HashMap<String, AccessibilityTextState>,
        scroll: &HashMap<String, ScrollState>,
        nodes: &mut Vec<Value>,
    ) {
        let mut next_parent = semantic_parent;
        if let Some(id) = node
            .id
            .as_deref()
            .filter(|_| node.semantics.is_some() || node.layout.scroll_y)
        {
            let semantic = node.semantics.as_ref();
            let text_state = text.get(id);
            let scroll_state = scroll.get(id).copied().unwrap_or_default();
            let value =
                text_state
                    .map(|state| state.value.as_str())
                    .or_else(|| match &node.content {
                        Some(Content::TextSession(input)) => Some(input.value.as_str()),
                        _ => semantic.and_then(|semantic| semantic.value.as_deref()),
                    });
            let selection = text_state.map(|state| state.selection.clone());
            let marked = text_state.and_then(|state| state.marked.clone());
            let semantic_bounds = scroll
                .get(id)
                .map(|state| state.bounds)
                .or_else(|| bounds.get(id).copied())
                .unwrap_or([0.0; 4]);
            nodes.push(json!({
                "id": id,
                "parent": semantic_parent,
                "role": semantic.map(|value| role_name(value.role)).unwrap_or("scroll_area"),
                "label": semantic.map(|value| value.label.as_str()).unwrap_or("Scrollable content"),
                "value": value,
                "hint": semantic.and_then(|value| value.hint.as_deref()),
                "bounds": semantic_bounds,
                "click_action": node.interaction.tap_action,
                "editable": matches!(node.content, Some(Content::TextSession(_))),
                "selection_start": selection.as_ref().map(|range| range.start),
                "selection_end": selection.as_ref().map(|range| range.end),
                "marked_start": marked.as_ref().map(|range| range.start),
                "marked_end": marked.as_ref().map(|range| range.end),
                "scrollable": node.layout.scroll_y,
                "scroll_offset_y": scroll_state.offset_y,
                "scroll_max_y": scroll_state.max_offset_y,
            }));
            next_parent = Some(id);
        }
        for child in &node.children {
            visit(child, next_parent, bounds, text, scroll, nodes);
        }
    }

    let bounds = BOUNDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("accessibility bounds lock");
    let mut nodes = Vec::new();
    visit(&scene.root, None, &bounds, text, scroll, &mut nodes);
    json!({ "summary": summary(scene), "nodes": nodes })
}

pub fn publish(
    scene: &Scene,
    text: &HashMap<String, AccessibilityTextState>,
    scroll: &HashMap<String, ScrollState>,
) -> Result<usize, String> {
    let payload =
        serde_json::to_string(&snapshot(scene, text, scroll)).map_err(|error| error.to_string())?;
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
        super::request_host_frame();
        Ok(())
    });
}
