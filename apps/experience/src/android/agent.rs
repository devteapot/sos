use std::thread;

use async_channel::Sender;
use experience_ir::{AgentConversation, ExperienceModel};
#[cfg(not(feature = "core-native"))]
use gpui_mobile::android::jni::{activity, find_app_class, get_string, with_env};
#[cfg(not(feature = "core-native"))]
use jni::objects::{JObject, JValue};
use serde::Deserialize;

use crate::deterministic_agent_candidate;

#[cfg(not(feature = "core-native"))]
const HELPER_CLASS: &str = "dev.gpui.mobile.GpuiAgent";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AgentStatus {
    pub provider: String,
    pub configured: bool,
    pub activity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentUpdate {
    Started { prompt: String },
    ToolStarted(String),
    ToolFinished { name: String, ok: bool },
    Candidate { source: String, summary: String },
    Completed,
    Failed(String),
}

#[cfg(not(feature = "core-native"))]
#[derive(Deserialize)]
struct LiveEnvelope {
    ok: bool,
    source: Option<String>,
    summary: Option<String>,
    error: Option<String>,
}

struct LiveCandidate {
    source: String,
    summary: String,
}

#[cfg(feature = "core-native")]
pub fn status() -> Result<AgentStatus, String> {
    Ok(AgentStatus {
        provider: "fake".into(),
        configured: true,
        activity: "Deterministic native provider ready".into(),
    })
}

#[cfg(not(feature = "core-native"))]
pub fn status() -> Result<AgentStatus, String> {
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let result = env
            .call_static_method(
                &helper,
                jni::jni_str!("status"),
                jni::jni_sig!("(Landroid/app/Activity;)Ljava/lang/String;"),
                &[JValue::Object(&activity)],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if result.is_null() {
            return Err("agent status returned no data".into());
        }
        serde_json::from_str(&get_string(env, &result))
            .map_err(|error| format!("decode agent status: {error}"))
    })
}

pub fn apply_status(conversation: &mut AgentConversation, status: &AgentStatus) {
    conversation.available = status.provider == "fake" || status.configured;
    if !conversation.busy {
        conversation.activity = status.activity.clone();
    }
    if conversation.available {
        conversation.error = None;
    }
}

pub fn configure_openai() -> Result<(), String> {
    call_bool("configureOpenAi")
}

pub fn configure_openrouter() -> Result<(), String> {
    call_bool("configureOpenRouter")
}

pub fn configure_codex() -> Result<(), String> {
    call_bool("configureCodex")
}

pub fn use_fake() -> Result<(), String> {
    call_bool("useFake")
}

pub fn clear_credential() -> Result<(), String> {
    call_bool("clearCredential")
}

pub fn spawn_prompt(
    status: AgentStatus,
    prompt: String,
    current_source: String,
    model: ExperienceModel,
    updates: Sender<AgentUpdate>,
) {
    thread::Builder::new()
        .name("sos-android-agent".into())
        .spawn(move || {
            let _ = updates.send_blocking(AgentUpdate::Started {
                prompt: prompt.clone(),
            });
            if let Err(error) = run_prompt(&status, &prompt, &current_source, &model, &updates) {
                let _ = updates.send_blocking(AgentUpdate::Failed(error));
            }
        })
        .expect("Android agent thread must start");
}

fn run_prompt(
    status: &AgentStatus,
    prompt: &str,
    current_source: &str,
    model: &ExperienceModel,
    updates: &Sender<AgentUpdate>,
) -> Result<(), String> {
    emit_tool(updates, "get_experience_context", || Ok(()))?;
    let candidate = if status.provider != "fake" {
        emit_tool(updates, "propose_experience", || {
            let candidate = run_live(prompt, current_source)?;
            validate_candidate(&candidate.source, model)?;
            Ok(candidate)
        })?
    } else {
        emit_tool(updates, "propose_experience", || {
            let source = deterministic_agent_candidate(current_source).to_owned();
            validate_candidate(&source, model)?;
            Ok(LiveCandidate {
                source,
                summary:
                    "The deterministic on-device agent proposed a complete alternate experience."
                        .into(),
            })
        })?
    };
    updates
        .send_blocking(AgentUpdate::Candidate {
            source: candidate.source,
            summary: candidate.summary,
        })
        .map_err(|_| "agent host stopped receiving updates".to_owned())?;
    let _ = updates.send_blocking(AgentUpdate::Completed);
    Ok(())
}

fn emit_tool<T>(
    updates: &Sender<AgentUpdate>,
    name: &str,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    updates
        .send_blocking(AgentUpdate::ToolStarted(name.into()))
        .map_err(|_| "agent host stopped receiving updates".to_owned())?;
    let result = operation();
    let _ = updates.send_blocking(AgentUpdate::ToolFinished {
        name: name.into(),
        ok: result.is_ok(),
    });
    result
}

fn validate_candidate(source: &str, model: &ExperienceModel) -> Result<(), String> {
    if source.is_empty() || source.len() > 256 * 1024 {
        return Err("agent candidate is outside the bounded source size".into());
    }
    let runtime = runtime_luau::LuauRuntime::compile(source)
        .map_err(|error| format!("agent candidate did not compile: {error}"))?;
    let scene = runtime
        .render(model, &runtime.initial_state())
        .map_err(|error| format!("agent candidate did not render: {error}"))?;
    experience_ir::validate_scene(&scene)
        .map(|_| ())
        .map_err(|error| format!("agent candidate scene is invalid: {error}"))
}

#[cfg(feature = "core-native")]
fn run_live(_prompt: &str, _current_source: &str) -> Result<LiveCandidate, String> {
    Err("Core live-agent credentials require a trusted native ceremony".into())
}

#[cfg(not(feature = "core-native"))]
fn run_live(prompt: &str, current_source: &str) -> Result<LiveCandidate, String> {
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let prompt = JObject::from(env.new_string(prompt).map_err(|error| error.to_string())?);
        let source = JObject::from(
            env.new_string(current_source)
                .map_err(|error| error.to_string())?,
        );
        let result = env
            .call_static_method(
                &helper,
                jni::jni_str!("run"),
                jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
                &[
                    JValue::Object(&activity),
                    JValue::Object(&prompt),
                    JValue::Object(&source),
                ],
            )
            .and_then(|value| value.l())
            .map_err(|_error| {
                env.exception_clear();
                "Pi request failed in the trusted Android bridge".to_owned()
            })?;
        if result.is_null() {
            return Err("Pi returned no candidate".into());
        }
        let envelope: LiveEnvelope = serde_json::from_str(&get_string(env, &result))
            .map_err(|_| "Pi returned an invalid candidate envelope".to_owned())?;
        if !envelope.ok {
            return Err(envelope
                .error
                .unwrap_or_else(|| "Pi did not produce a candidate".into()));
        }
        Ok(LiveCandidate {
            source: envelope
                .source
                .ok_or_else(|| "OpenAI candidate omitted source".to_owned())?,
            summary: envelope
                .summary
                .ok_or_else(|| "OpenAI candidate omitted summary".to_owned())?,
        })
    })
}

#[cfg(feature = "core-native")]
fn call_bool(_method: &str) -> Result<(), String> {
    Err("Core agent credentials require a trusted native ceremony".into())
}

#[cfg(not(feature = "core-native"))]
fn call_bool(method: &str) -> Result<(), String> {
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let method = jni::strings::JNIString::new(method);
        let ok = env
            .call_static_method(
                &helper,
                &method,
                jni::jni_sig!("(Landroid/app/Activity;)Z"),
                &[JValue::Object(&activity)],
            )
            .and_then(|value| value.z())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        ok.then_some(())
            .ok_or_else(|| "trusted agent helper rejected the request".into())
    })
}
