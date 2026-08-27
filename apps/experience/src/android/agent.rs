#[cfg(feature = "core-native")]
use std::io::{Read, Write};
#[cfg(feature = "core-native")]
use std::os::fd::AsRawFd;
#[cfg(feature = "core-native")]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(feature = "core-native")]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
use std::thread;
#[cfg(feature = "core-native")]
use std::time::{Duration, Instant};

use async_channel::Sender;
use experience_ir::{
    AgentConfigurationAction, AgentConversation, Content, ExperienceModel, SceneNode,
};
#[cfg(not(feature = "core-native"))]
use gpui_mobile::android::jni::{activity, find_app_class, get_string, with_env};
#[cfg(not(feature = "core-native"))]
use jni::objects::{JObject, JValue};
use serde::Deserialize;
#[cfg(feature = "core-native")]
use serde::Serialize;
#[cfg(feature = "core-native")]
use zeroize::{Zeroize, Zeroizing};

use crate::android_agent_contract::{
    expected_model, model_is_exact, reconciled_request_error, verified_action_sequence,
};
#[cfg(feature = "core-native")]
use crate::android_agent_contract::{pi_timeout_seconds, OPENROUTER_MODEL};
#[cfg(feature = "core-native")]
use crate::core_credential::{CeremonySnapshot, CredentialState};
use crate::{deterministic_stock_agent_candidate, STOCK_THEME_MODULE};

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

#[derive(Deserialize)]
struct LiveEnvelope {
    #[cfg(not(feature = "core-native"))]
    #[serde(default)]
    ok: Option<bool>,
    source: Option<String>,
    summary: Option<String>,
    stage: Option<String>,
    category: Option<String>,
    status: Option<u16>,
    #[serde(default)]
    actions: Vec<String>,
    model: Option<String>,
    #[cfg(feature = "core-native")]
    provider: Option<String>,
    #[cfg(feature = "core-native")]
    credential: Option<ApiCredential>,
}

#[cfg(feature = "core-native")]
#[derive(Deserialize)]
struct ApiCredential {
    #[serde(rename = "type")]
    kind: String,
    key: String,
}

#[cfg(feature = "core-native")]
impl Drop for ApiCredential {
    fn drop(&mut self) {
        self.kind.zeroize();
        self.key.zeroize();
    }
}

struct LiveCandidate {
    source: String,
    summary: String,
}

#[cfg(feature = "core-native")]
const MAX_PI_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
#[cfg(feature = "core-native")]
static CORE_CREDENTIAL: OnceLock<Mutex<CredentialState>> = OnceLock::new();
#[cfg(feature = "core-native")]
static CORE_CREDENTIAL_CHANGED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "core-native")]
fn core_credential() -> &'static Mutex<CredentialState> {
    CORE_CREDENTIAL.get_or_init(|| Mutex::new(CredentialState::default()))
}

#[cfg(feature = "core-native")]
struct ReapedChild {
    child: Option<Child>,
}

#[cfg(feature = "core-native")]
impl ReapedChild {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("managed Pi child must exist")
    }

    fn finish(mut self) -> Result<ExitStatus, String> {
        let status = self
            .child_mut()
            .wait()
            .map_err(|error| format!("wait for common Pi runner: {error}"))?;
        self.child.take();
        Ok(status)
    }
}

#[cfg(feature = "core-native")]
impl Drop for ReapedChild {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(feature = "core-native")]
pub fn status() -> Result<AgentStatus, String> {
    let configured = core_credential()
        .lock()
        .expect("Core credential lock")
        .configured();
    Ok(if configured {
        AgentStatus {
            provider: "openrouter".into(),
            configured: true,
            activity: format!("OpenRouter ready · {OPENROUTER_MODEL}"),
        }
    } else {
        AgentStatus {
            provider: "fake".into(),
            configured: true,
            activity: "Deterministic native provider ready".into(),
        }
    })
}

#[cfg(feature = "core-native")]
pub fn credential_snapshot() -> CeremonySnapshot {
    core_credential()
        .lock()
        .expect("Core credential lock")
        .snapshot()
}

#[cfg(feature = "core-native")]
pub fn take_credential_changed() -> bool {
    CORE_CREDENTIAL_CHANGED.swap(false, Ordering::AcqRel)
}

#[cfg(feature = "core-native")]
pub fn apply_credential_input(text: &str) -> bool {
    let mut state = core_credential().lock().expect("Core credential lock");
    let was_visible = state.snapshot().visible;
    let applied = state.apply_input(text);
    if was_visible && !state.snapshot().visible {
        CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
        log::info!("core_agent_credential_ceremony state=saved");
    }
    applied
}

#[cfg(feature = "core-native")]
pub fn save_credential() -> bool {
    let saved = core_credential()
        .lock()
        .expect("Core credential lock")
        .save();
    CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
    log::info!(
        "core_agent_credential_ceremony state={}",
        if saved { "saved" } else { "rejected" }
    );
    saved
}

#[cfg(feature = "core-native")]
pub fn cancel_credential() {
    core_credential()
        .lock()
        .expect("Core credential lock")
        .cancel();
    CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
    log::info!("core_agent_credential_ceremony state=cancelled");
}

#[cfg(feature = "core-native")]
pub fn zeroize_credential_on_exit() {
    core_credential()
        .lock()
        .expect("Core credential lock")
        .clear();
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
    conversation.configuration_actions = supported_configuration_actions();
    if !conversation.busy {
        conversation.activity = status.activity.clone();
    }
    conversation.error = reconciled_request_error(conversation.error.take(), false);
}

fn supported_configuration_actions() -> Vec<AgentConfigurationAction> {
    #[cfg(feature = "core-native")]
    {
        vec![
            AgentConfigurationAction::ConfigureOpenRouter,
            AgentConfigurationAction::UseFake,
            AgentConfigurationAction::ClearCredential,
        ]
    }
    #[cfg(not(feature = "core-native"))]
    {
        vec![
            AgentConfigurationAction::ConfigureOpenAi,
            AgentConfigurationAction::ConfigureOpenRouter,
            AgentConfigurationAction::ConfigureCodex,
            AgentConfigurationAction::UseFake,
            AgentConfigurationAction::ClearCredential,
        ]
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
    let provider = status.provider.clone();
    let model_name = expected_model(&provider).unwrap_or("unsupported");
    log::info!("android_agent_thread_start provider={provider} model={model_name}");
    thread::Builder::new()
        .name("sos-android-agent".into())
        .spawn(move || {
            let _ = updates.send_blocking(AgentUpdate::Started {
                prompt: prompt.clone(),
            });
            if let Err(error) = run_prompt(&status, &prompt, &current_source, &model, &updates) {
                let (stage, category) = failure_marker(&error);
                log::warn!(
                    "android_agent_failure stage={stage} category={category} model={model_name}"
                );
                let _ = updates.send_blocking(AgentUpdate::Failed(error));
            }
        })
        .expect("Android agent thread must start");
}

fn failure_marker(error: &str) -> (&'static str, &'static str) {
    if error.contains("[credential/") {
        ("credential", "credential_error")
    } else if error.contains("[provider/") {
        ("provider", "provider_error")
    } else if error.contains("[validation/invalid_candidate") {
        ("validation", "invalid_candidate")
    } else if error.contains("[protocol/wrong_model") {
        ("protocol", "wrong_model")
    } else if error.contains("[protocol/tool_sequence") {
        ("protocol", "tool_sequence")
    } else if error.contains("[child/") {
        ("child", "process_failure")
    } else if error.contains("[bridge/") {
        ("bridge", "bridge_failure")
    } else {
        ("protocol", "request_failed")
    }
}

fn run_prompt(
    status: &AgentStatus,
    prompt: &str,
    current_source: &str,
    model: &ExperienceModel,
    updates: &Sender<AgentUpdate>,
) -> Result<(), String> {
    let model_name = expected_model(&status.provider).unwrap_or("unsupported");
    log::info!(
        "android_agent_request_start provider={} model={model_name}",
        status.provider
    );
    let faux_candidate = if status.provider == "fake" {
        Some(deterministic_stock_agent_candidate(current_source))
    } else {
        None
    };
    let candidate = run_live(
        &status.provider,
        prompt,
        current_source,
        faux_candidate.as_deref(),
    )?;
    emit_completed_tool(updates, "get_experience_context")?;
    emit_tool(updates, "validate_experience", || {
        validate_candidate(&candidate.source, model)
    })?;
    emit_completed_tool(updates, "submit_experience")?;
    updates
        .send_blocking(AgentUpdate::Candidate {
            source: candidate.source,
            summary: candidate.summary,
        })
        .map_err(|_| "agent host stopped receiving updates".to_owned())?;
    let _ = updates.send_blocking(AgentUpdate::Completed);
    Ok(())
}

fn emit_completed_tool(updates: &Sender<AgentUpdate>, name: &str) -> Result<(), String> {
    emit_tool(updates, name, || Ok(()))
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
        return Err(
            "The agent candidate is outside the bounded source size. [validation/invalid_candidate]"
                .into(),
        );
    }
    let runtime = runtime_luau::LuauRuntime::compile_with_assets(
        source,
        vec![runtime_luau::RevisionAssetInput {
            id: "stock.theme".into(),
            kind: "luau".into(),
            bytes: STOCK_THEME_MODULE.as_bytes().to_vec(),
        }],
    )
    .map_err(|_| {
        "The agent candidate did not compile. [validation/invalid_candidate]".to_owned()
    })?;
    if runtime.api_version() != experience_ir::EXPERIENCE_API_VERSION_V4 {
        return Err(
            "The agent candidate must use Experience API v4. [validation/invalid_candidate]".into(),
        );
    }
    let scene = runtime
        .render(model, &runtime.initial_state())
        .map_err(|_| {
            "The agent candidate did not render. [validation/invalid_candidate]".to_owned()
        })?;
    experience_ir::validate_scene(&scene)
        .map(|_| ())
        .map_err(|_| {
            "The agent candidate scene is invalid. [validation/invalid_candidate]".to_owned()
        })?;
    if !has_shell_agent_composer(&runtime, model)? {
        return Err(
            "The agent candidate removed the Stock agent composer. [validation/invalid_candidate]"
                .into(),
        );
    }
    Ok(())
}

fn has_agent_composer(node: &SceneNode) -> bool {
    matches!(
        &node.content,
        Some(Content::TextSession(session))
            if session.submit_action.as_deref() == Some("agent_submit")
    ) || node.children.iter().any(has_agent_composer)
}

fn has_shell_agent_composer(
    runtime: &runtime_luau::LuauRuntime,
    model: &ExperienceModel,
) -> Result<bool, String> {
    for (key, value) in [("active_workspace", "agent"), ("shell_panel", "agent")] {
        let mut state = runtime.initial_state();
        let object = state.as_object_mut().ok_or_else(|| {
            "The Stock shell state is not a record. [validation/invalid_candidate]".to_owned()
        })?;
        object.insert(key.into(), serde_json::Value::String(value.into()));
        let scene = runtime.render(model, &state).map_err(|_| {
            "The Stock agent branch did not render. [validation/invalid_candidate]".to_owned()
        })?;
        if has_agent_composer(&scene.root) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn structured_failure(envelope: &LiveEnvelope, expected_model: &str) -> String {
    const STAGES: [&str; 7] = [
        "request",
        "credential",
        "provider",
        "protocol",
        "validation",
        "bridge",
        "child",
    ];
    const CATEGORIES: [&str; 21] = [
        "invalid_request",
        "credential_rejected",
        "provider_rejected",
        "rate_limited",
        "provider_unavailable",
        "provider_error",
        "tool_sequence",
        "invalid_candidate",
        "protocol_error",
        "internal",
        "launch_failure",
        "timeout",
        "response_timeout",
        "response_io",
        "empty_response",
        "linker_or_exit",
        "invalid_response",
        "unexpected_response",
        "wrong_model",
        "refresh_failed",
        "request_io",
    ];
    let stage = envelope
        .stage
        .as_deref()
        .filter(|value| STAGES.contains(value))
        .unwrap_or("protocol");
    let category = envelope
        .category
        .as_deref()
        .filter(|value| CATEGORIES.contains(value))
        .unwrap_or("protocol_error");
    let (stage, category) = if envelope
        .model
        .as_deref()
        .is_some_and(|model| model != expected_model)
    {
        ("protocol", "wrong_model")
    } else {
        (stage, category)
    };
    let status = envelope
        .status
        .filter(|status| (100..=599).contains(status));
    log::warn!(
        "android_agent_failure stage={stage} category={category} model={expected_model} status={}",
        status.map_or("none".to_owned(), |value| value.to_string())
    );
    let detail = match category {
        "credential_rejected" => "The provider rejected the configured credential.",
        "provider_rejected" => "The provider rejected this request.",
        "rate_limited" => "The provider rate-limited this request.",
        "provider_unavailable" => "The provider is temporarily unavailable.",
        "provider_error" => "The provider request failed.",
        "invalid_request" => "The Pi request was invalid.",
        "tool_sequence" => "Pi used an invalid authoring tool sequence.",
        "invalid_candidate" => "Pi proposed an invalid candidate.",
        "wrong_model" => "Pi returned a response for the wrong model.",
        "launch_failure" => "The local Pi process could not start.",
        "timeout" => "The local Pi process timed out.",
        "response_timeout" => "The local Pi response reader timed out.",
        "response_io" => "The local Pi response could not be read.",
        "empty_response" => "The local Pi process returned no response.",
        "linker_or_exit" => "The local Pi process exited before returning a response.",
        "invalid_response" => "The local Pi process returned an invalid response.",
        "unexpected_response" => "The local Pi process returned an unexpected response type.",
        "refresh_failed" => "The refreshed provider credential could not be stored.",
        "request_io" => "The local Pi process could not accept its request.",
        "internal" => "The trusted on-device Pi bridge failed.",
        _ => "The Pi protocol failed.",
    };
    match status {
        Some(status) => {
            format!("{detail} [{stage}/{category}; HTTP {status}; model {expected_model}]")
        }
        None => format!("{detail} [{stage}/{category}; model {expected_model}]"),
    }
}

#[cfg(feature = "core-native")]
fn set_nonblocking(fd: libc::c_int) -> Result<(), String> {
    // SAFETY: fd belongs to a live child pipe and fcntl does not retain it.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(format!(
            "inspect common Pi pipe flags: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: fd is still live and the new flags preserve all existing bits.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(format!(
            "bound common Pi pipe: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(feature = "core-native")]
fn poll_until(
    descriptors: &mut [libc::pollfd],
    deadline: Instant,
    maximum_wait: Duration,
    timeout: Duration,
) -> Result<(), String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| common_pi_timeout_error(timeout))?;
    let milliseconds = remaining
        .min(maximum_wait)
        .as_millis()
        .clamp(1, libc::c_int::MAX as u128) as libc::c_int;
    // SAFETY: descriptors is a valid mutable pollfd slice for the duration of the call.
    let result = unsafe {
        libc::poll(
            descriptors.as_mut_ptr(),
            descriptors.len() as libc::nfds_t,
            milliseconds,
        )
    };
    if result < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(format!("poll common Pi runner: {error}"));
        }
    }
    if Instant::now() >= deadline {
        return Err(common_pi_timeout_error(timeout));
    }
    Ok(())
}

#[cfg(feature = "core-native")]
fn common_pi_timeout_error(timeout: Duration) -> String {
    format!(
        "common Pi runner timed out after {} seconds",
        timeout.as_secs()
    )
}

#[cfg(feature = "core-native")]
fn run_core_pi(
    request: &[u8],
    timeout: Duration,
) -> Result<(ExitStatus, Zeroizing<Vec<u8>>), String> {
    let child = Command::new("/system_ext/bin/sos-node")
        .args([
            "/system_ext/etc/sos-agent/agent-runner.cjs",
            "stdio",
            "--api-doc",
            "/system_ext/etc/sos-agent/experience-api.md",
            "--example",
            "/system_ext/etc/sos-agent/example-primary.luau",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("start common Pi runner: {error}"))?;
    log::info!("android_agent_child_start pid={} platform=core", child.id());
    let mut child = ReapedChild::new(child);
    let mut input = Some(
        child
            .child_mut()
            .stdin
            .take()
            .ok_or_else(|| "common Pi runner omitted stdin".to_owned())?,
    );
    let mut output = Some(
        child
            .child_mut()
            .stdout
            .take()
            .ok_or_else(|| "common Pi runner omitted stdout".to_owned())?,
    );
    set_nonblocking(input.as_ref().unwrap().as_raw_fd())?;
    set_nonblocking(output.as_ref().unwrap().as_raw_fd())?;

    let deadline = Instant::now() + timeout;
    let mut written = 0;
    let mut response = Zeroizing::new(Vec::new());
    while input.is_some() || output.is_some() {
        let mut descriptors = [
            libc::pollfd {
                fd: input.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLOUT,
                revents: 0,
            },
            libc::pollfd {
                fd: output.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        poll_until(&mut descriptors, deadline, Duration::from_secs(1), timeout)?;

        if let Some(stream) = input.as_mut() {
            let events = descriptors[0].revents;
            if events & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err("common Pi runner closed stdin before the bounded request".into());
            }
            if events & libc::POLLOUT != 0 {
                match stream.write(&request[written..]) {
                    Ok(0) => return Err("common Pi runner stopped accepting its request".into()),
                    Ok(count) => {
                        written += count;
                        if written == request.len() {
                            input.take();
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(format!("write common Pi request: {error}")),
                }
            }
        }

        if let Some(stream) = output.as_mut() {
            let events = descriptors[1].revents;
            if events & (libc::POLLERR | libc::POLLNVAL) != 0 {
                return Err("common Pi runner response pipe failed".into());
            }
            if events & (libc::POLLIN | libc::POLLHUP) != 0 {
                let mut buffer = [0_u8; 8192];
                loop {
                    match stream.read(&mut buffer) {
                        Ok(0) => {
                            output.take();
                            break;
                        }
                        Ok(count) => {
                            response.extend_from_slice(&buffer[..count]);
                            if response.len() > MAX_PI_RESPONSE_BYTES {
                                return Err("common Pi response is outside the bounded size".into());
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(error) => return Err(format!("read common Pi response: {error}")),
                    }
                }
            }
        }
    }

    loop {
        if child
            .child_mut()
            .try_wait()
            .map_err(|error| format!("observe common Pi runner: {error}"))?
            .is_some()
        {
            break;
        }
        poll_until(&mut [], deadline, Duration::from_millis(10), timeout)?;
    }
    let status = child.finish()?;
    log::info!(
        "android_agent_child_exit code={} platform=core",
        status
            .code()
            .map_or("signal".to_owned(), |code| code.to_string())
    );
    Ok((status, response))
}

#[cfg(feature = "core-native")]
fn run_live(
    provider: &str,
    prompt: &str,
    current_source: &str,
    faux_candidate: Option<&str>,
) -> Result<LiveCandidate, String> {
    if prompt.is_empty() || prompt.len() > 32 * 1024 {
        return Err("agent prompt is outside the bounded size".into());
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct FauxRequest<'a> {
        action: &'static str,
        provider: &'static str,
        prompt: &'a str,
        current_source: &'a str,
        candidate_source: &'a str,
    }
    #[derive(Serialize)]
    struct CredentialRequest<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        key: &'a str,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LiveRequest<'a> {
        action: &'static str,
        provider: &'static str,
        model: &'static str,
        credential: CredentialRequest<'a>,
        prompt: &'a str,
        current_source: &'a str,
    }

    let credential = faux_candidate
        .is_none()
        .then(|| {
            core_credential()
                .lock()
                .expect("Core credential lock")
                .credential()
                .ok_or_else(|| "OpenRouter is not configured".to_owned())
        })
        .transpose()?
        .map(Zeroizing::new);
    let request = if let Some(candidate) = faux_candidate {
        serde_json::to_vec(&FauxRequest {
            action: "prompt",
            provider: "faux",
            prompt,
            current_source,
            candidate_source: candidate,
        })
    } else {
        let credential = credential
            .as_deref()
            .and_then(|value| std::str::from_utf8(value).ok())
            .ok_or_else(|| "OpenRouter credential is not valid ASCII".to_owned())?;
        serde_json::to_vec(&LiveRequest {
            action: "prompt",
            provider: "openrouter",
            model: OPENROUTER_MODEL,
            credential: CredentialRequest {
                kind: "api_key",
                key: credential,
            },
            prompt,
            current_source,
        })
    }
    .map(Zeroizing::new)
    .map_err(|_| {
        "The bounded Pi request could not be encoded. [protocol/protocol_error]".to_owned()
    })?;
    if request.len() > 1024 * 1024 {
        return Err("Pi request is outside the bounded size".into());
    }
    let timeout = Duration::from_secs(
        pi_timeout_seconds(provider)
            .ok_or_else(|| "Core selected an unsupported Pi provider".to_owned())?,
    );
    let (status, response) = run_core_pi(&request, timeout).map_err(|error| {
        let (category, detail) = if error.contains("timed out") {
            ("timeout", "The local Pi process timed out.")
        } else if error.starts_with("start common Pi runner") {
            ("launch_failure", "The local Pi process could not start.")
        } else {
            ("process_io", "The local Pi process failed.")
        };
        log::warn!(
            "android_agent_failure stage=child category={category} model={}",
            expected_model(provider).unwrap_or("unsupported")
        );
        format!(
            "{detail} [child/{category}; model {}]",
            expected_model(provider).unwrap_or("unsupported")
        )
    })?;
    let line = response
        .split(|byte| *byte == b'\n')
        .rev()
        .find(|line| !line.is_empty())
        .ok_or_else(|| "common Pi runner returned no response".to_owned())?;
    let envelope: LiveEnvelope = serde_json::from_slice(line)
        .map_err(|_| "common Pi runner returned an invalid response".to_owned())?;
    let response_type = if envelope.source.is_some() {
        "prompt_complete"
    } else if envelope.category.is_some() {
        "error"
    } else {
        "unexpected"
    };
    log::info!(
        "android_agent_child_response type={response_type} provider={provider} model={}",
        expected_model(provider).unwrap_or("unsupported")
    );
    if !status.success() {
        let expected_model = expected_model(provider)
            .ok_or_else(|| "Core selected an unsupported Pi provider".to_owned())?;
        return Err(structured_failure(&envelope, expected_model));
    }
    let expected_model = expected_model(provider)
        .ok_or_else(|| "Core selected an unsupported Pi provider".to_owned())?;
    if !envelope
        .model
        .as_deref()
        .is_some_and(|model| model_is_exact(provider, model))
    {
        return Err(format!(
            "Pi returned a response for the wrong model. [protocol/wrong_model; model {expected_model}]"
        ));
    }
    log::info!("android_agent_pi_response provider={provider} model={expected_model}");
    if envelope.source.is_none() {
        return Err("common Pi runner did not produce a candidate".into());
    }
    emit_verified_actions(provider, expected_model, &envelope.actions)?;
    if faux_candidate.is_none() {
        if envelope.provider.as_deref() != Some("openrouter") {
            return Err("common Pi runner returned the wrong live provider".into());
        }
        let refreshed = envelope
            .credential
            .as_ref()
            .filter(|credential| credential.kind == "api_key")
            .ok_or_else(|| {
                "common Pi runner omitted the refreshed OpenRouter credential".to_owned()
            })?;
        if !core_credential()
            .lock()
            .expect("Core credential lock")
            .accept_refreshed("openrouter", refreshed.key.as_bytes())
        {
            return Err("common Pi runner returned an invalid OpenRouter credential".into());
        }
    }
    Ok(LiveCandidate {
        source: envelope
            .source
            .ok_or_else(|| "common Pi runner omitted candidate source".to_owned())?,
        summary: envelope
            .summary
            .ok_or_else(|| "common Pi runner omitted candidate summary".to_owned())?,
    })
}

#[cfg(not(feature = "core-native"))]
fn run_live(
    provider: &str,
    prompt: &str,
    current_source: &str,
    faux_candidate: Option<&str>,
) -> Result<LiveCandidate, String> {
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let prompt = JObject::from(
            env.new_string(prompt)
                .map_err(|_| "Pi prompt could not cross the trusted bridge".to_owned())?,
        );
        let source = JObject::from(
            env.new_string(current_source)
                .map_err(|_| "Pi source could not cross the trusted bridge".to_owned())?,
        );
        let candidate = JObject::from(
            env.new_string(faux_candidate.unwrap_or_default())
                .map_err(|_| "Pi candidate could not cross the trusted bridge".to_owned())?,
        );
        let result = env
            .call_static_method(
                &helper,
                jni::jni_str!("run"),
                jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
                &[
                    JValue::Object(&activity),
                    JValue::Object(&prompt),
                    JValue::Object(&source),
                    JValue::Object(&candidate),
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
        let expected_model = expected_model(provider).ok_or_else(|| {
            "trusted Android bridge selected an unsupported Pi provider".to_owned()
        })?;
        if envelope.ok != Some(true) {
            return Err(structured_failure(&envelope, expected_model));
        }
        if !envelope
            .model
            .as_deref()
            .is_some_and(|model| model_is_exact(provider, model))
        {
            return Err("trusted Android bridge returned the wrong Pi model".into());
        }
        log::info!("android_agent_pi_response provider={provider} model={expected_model}");
        emit_verified_actions(provider, expected_model, &envelope.actions)?;
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

fn emit_verified_actions(provider: &str, model: &str, actions: &[String]) -> Result<(), String> {
    let actions = verified_action_sequence(actions).ok_or_else(|| {
        "Pi used an invalid authoring tool sequence. [protocol/tool_sequence]".to_owned()
    })?;
    log::info!(
        "android_agent_action_sequence_verified provider={provider} model={model} count={}",
        actions.len()
    );
    for (index, action) in actions.iter().enumerate() {
        log::info!(
            "android_agent_action_verified ordinal={} action={} provider={provider} model={model}",
            index + 1,
            action
        );
    }
    Ok(())
}

#[cfg(feature = "core-native")]
fn call_bool(_method: &str) -> Result<(), String> {
    let mut state = core_credential().lock().expect("Core credential lock");
    match _method {
        "configureOpenRouter" => {
            state.begin();
            CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
            log::info!("core_agent_credential_ceremony state=opened provider=openrouter model={OPENROUTER_MODEL}");
            Ok(())
        }
        "useFake" => {
            state.use_faux();
            CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
            Ok(())
        }
        "clearCredential" => {
            state.clear();
            CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
            log::info!("core_agent_credential_ceremony state=cleared");
            Ok(())
        }
        "configureOpenAi" | "configureCodex" => {
            Err("Core live agents support only the pinned OpenRouter provider".into())
        }
        _ => Err("unsupported trusted Core agent action".into()),
    }
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
            .map_err(|_error| {
                env.exception_clear();
                "trusted agent helper rejected the request".to_owned()
            })?;
        ok.then_some(())
            .ok_or_else(|| "trusted agent helper rejected the request".into())
    })
}
