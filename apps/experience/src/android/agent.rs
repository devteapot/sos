#[cfg(feature = "core-native")]
use std::io::{Read, Write};
#[cfg(feature = "core-native")]
use std::os::fd::AsRawFd;
#[cfg(feature = "core-native")]
use std::os::unix::process::ExitStatusExt;
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
use experience_ir::{AgentConversation, ExperienceModel};
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
    expected_model, model_is_exact, reconciled_request_error, safe_failure_category,
    safe_failure_stage, safe_http_status, ui_failure, verified_action_sequence, AgentUiAttempt,
    AgentUiFailure,
};
#[cfg(feature = "core-native")]
use crate::android_agent_contract::{
    pi_timeout_seconds, safe_core_launch_cause, SafeCorePiFailure, CORE_CHILD_LAUNCH,
    CORE_NODE_ARGS, OPENROUTER_MODEL,
};
#[cfg(feature = "core-native")]
use crate::core_child_fds::restrict_to_standard_fds;
#[cfg(feature = "core-native")]
use crate::core_credential::{CeremonySnapshot, CredentialState};
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
    Started {
        attempt_id: u64,
        prompt: String,
    },
    ToolStarted {
        attempt_id: u64,
        name: String,
    },
    ToolFinished {
        attempt_id: u64,
        name: String,
        ok: bool,
    },
    Candidate {
        attempt_id: u64,
        source: String,
        summary: String,
    },
    Completed {
        attempt_id: u64,
    },
    Failed {
        attempt_id: u64,
        failure: AgentUiFailure,
    },
}

#[derive(Deserialize)]
struct LiveEnvelope {
    protocol_version: Option<u8>,
    terminal: Option<String>,
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

#[cfg(feature = "core-dev-credential")]
pub(super) fn install_dev_openrouter_credential(key: &[u8]) -> bool {
    let installed = core_credential()
        .lock()
        .expect("Core credential lock")
        .install_openrouter(key);
    if installed {
        CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
        log::info!("core_dev_credential state=set");
    }
    installed
}

#[cfg(feature = "core-dev-credential")]
pub(super) fn clear_dev_openrouter_credential() {
    core_credential()
        .lock()
        .expect("Core credential lock")
        .clear();
    CORE_CREDENTIAL_CHANGED.store(true, Ordering::Release);
    log::info!("core_dev_credential state=cleared");
}

#[cfg(feature = "core-dev-credential")]
pub(super) fn dev_openrouter_credential_configured() -> bool {
    core_credential()
        .lock()
        .expect("Core credential lock")
        .configured()
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
            match child.wait() {
                Ok(status) => log_child_exit(status, "forced_cleanup"),
                Err(_) => log::warn!(
                    "android_agent_child_exit code=unknown signal=unknown cleanup=wait_failed platform=core"
                ),
            }
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
    if !conversation.busy {
        conversation.activity = status.activity.clone();
    }
    conversation.error = reconciled_request_error(conversation.error.take(), false);
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

fn accepted_request_marker(attempt: &mut AgentUiAttempt) -> Result<String, &'static str> {
    let marker = attempt.accepted_marker()?;
    if !marker.starts_with("android_agent_request_accepted provider=") {
        return Err("agent request acceptance marker violated its sanitized prefix");
    }
    Ok(marker)
}

pub fn spawn_prompt(
    attempt: AgentUiAttempt,
    status: AgentStatus,
    prompt: String,
    current_source: String,
    model: ExperienceModel,
    updates: Sender<AgentUpdate>,
) -> Result<(), String> {
    let provider = status.provider.clone();
    let model_name = expected_model(&provider).unwrap_or("unsupported");
    let core_dev_tunnel = attempt.uses_core_dev_fixed_tunnel();
    thread::Builder::new()
        .name("sos-android-agent".into())
        .spawn(move || {
            let attempt_id = attempt.attempt_id();
            let mut attempt = attempt;
            let accepted_marker = accepted_request_marker(&mut attempt)
                .expect("agent request acceptance must follow UI dispatch");
            log::info!("{accepted_marker}");
            if updates
                .send_blocking(AgentUpdate::Started {
                    attempt_id,
                    prompt: prompt.clone(),
                })
                .is_err()
            {
                let failure = ui_failure("dispatch", "dispatch_channel");
                super::log_agent_attempt_event(
                    attempt
                        .terminal(Some(failure))
                        .expect("agent attempt must have one terminal"),
                );
                return;
            }
            match run_prompt(
                attempt_id,
                &status,
                &prompt,
                &current_source,
                &model,
                &updates,
                core_dev_tunnel,
            ) {
                Ok(()) => {
                    super::log_agent_attempt_event(
                        attempt
                            .terminal(None)
                            .expect("agent attempt must have one terminal"),
                    );
                    let _ = updates.send_blocking(AgentUpdate::Completed { attempt_id });
                }
                Err(error) => {
                    let (stage, category) = failure_marker(&error);
                    let status = failure_http_status(&error)
                        .map_or("none".to_owned(), |status| status.to_string());
                    log::warn!(
                        "android_agent_request_result attempt={attempt_id} stage={stage} category={category} status={status} model={model_name} correlation=serialized"
                    );
                    let failure = ui_failure(stage, category);
                    super::log_agent_attempt_event(
                        attempt
                            .terminal(Some(failure))
                            .expect("agent attempt must have one terminal"),
                    );
                    let _ = updates.send_blocking(AgentUpdate::Failed {
                        attempt_id,
                        failure,
                    });
                }
            }
        })
        .map(|_| ())
        .map_err(|_| "Android agent thread could not start".to_owned())
}

fn failure_marker(error: &str) -> (&'static str, &'static str) {
    let Some((_, tagged)) = error.rsplit_once('[') else {
        return ("protocol", "unknown");
    };
    let Some((stage, rest)) = tagged.split_once('/') else {
        return ("protocol", "unknown");
    };
    let category = rest.split([';', ']']).next();
    (
        safe_failure_stage(Some(stage)),
        safe_failure_category(category),
    )
}

fn failure_http_status(error: &str) -> Option<u16> {
    let marker = "; HTTP ";
    let start = error.find(marker)? + marker.len();
    let digits = error[start..].split([';', ']']).next()?;
    digits
        .parse()
        .ok()
        .filter(|status| (100..=599).contains(status))
}

fn run_prompt(
    attempt_id: u64,
    status: &AgentStatus,
    prompt: &str,
    current_source: &str,
    model: &ExperienceModel,
    updates: &Sender<AgentUpdate>,
    core_dev_tunnel: bool,
) -> Result<(), String> {
    let model_name = expected_model(&status.provider).unwrap_or("unsupported");
    log::info!(
        "android_agent_request_start provider={} model={model_name}",
        status.provider
    );
    let faux_candidate = if status.provider == "fake" {
        Some(deterministic_agent_candidate(current_source))
    } else {
        None
    };
    let candidate = run_live(
        &status.provider,
        prompt,
        current_source,
        faux_candidate,
        core_dev_tunnel,
    )?;
    emit_completed_tool(attempt_id, updates, "get_experience_context")?;
    emit_tool(attempt_id, updates, "validate_experience", || {
        validate_candidate(&candidate.source, model)
    })?;
    emit_completed_tool(attempt_id, updates, "submit_experience")?;
    updates
        .send_blocking(AgentUpdate::Candidate {
            attempt_id,
            source: candidate.source,
            summary: candidate.summary,
        })
        .map_err(|_| "agent host stopped receiving updates".to_owned())?;
    Ok(())
}

fn emit_completed_tool(
    attempt_id: u64,
    updates: &Sender<AgentUpdate>,
    name: &str,
) -> Result<(), String> {
    emit_tool(attempt_id, updates, name, || Ok(()))
}

fn emit_tool<T>(
    attempt_id: u64,
    updates: &Sender<AgentUpdate>,
    name: &str,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    updates
        .send_blocking(AgentUpdate::ToolStarted {
            attempt_id,
            name: name.into(),
        })
        .map_err(|_| "agent host stopped receiving updates".to_owned())?;
    let result = operation();
    let _ = updates.send_blocking(AgentUpdate::ToolFinished {
        attempt_id,
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
    let runtime = runtime_luau::LuauRuntime::compile(source).map_err(|_| {
        "The agent candidate did not compile. [validation/invalid_candidate]".to_owned()
    })?;
    let scene = runtime
        .render(model, &runtime.initial_state())
        .map_err(|_| {
            "The agent candidate did not render. [validation/invalid_candidate]".to_owned()
        })?;
    experience_ir::validate_scene(&scene)
        .map(|_| ())
        .map_err(|_| {
            "The agent candidate scene is invalid. [validation/invalid_candidate]".to_owned()
        })
}

fn structured_failure(envelope: &LiveEnvelope, expected_model: &str) -> String {
    let stage = safe_failure_stage(envelope.stage.as_deref());
    let category = safe_failure_category(envelope.category.as_deref());
    let (stage, category) = if envelope
        .model
        .as_deref()
        .is_some_and(|model| model != expected_model)
    {
        ("protocol", "wrong_model")
    } else {
        (stage, category)
    };
    let status = safe_http_status(envelope.status);
    let detail = match category {
        "credential_rejected" => "The provider rejected the configured credential.",
        "provider_rejected" => "The provider rejected this request.",
        "rate_limited" => "The provider rate-limited this request.",
        "provider_unavailable" => "The provider is temporarily unavailable.",
        "provider_error" => "The provider request failed.",
        "dns_resolution" => "The provider hostname could not be resolved.",
        "dns_timeout" => "Provider DNS resolution timed out.",
        "dns_proxy_unavailable" => "The Android DNS proxy was unavailable.",
        "connect_timeout" => "The provider connection timed out.",
        "connect_refused" => "The provider connection was refused.",
        "connect_reset" => "The provider connection was reset.",
        "network_unreachable" => "The provider network was unreachable.",
        "tls_failure" => "The provider TLS handshake or certificate validation failed.",
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
        "unknown" => "The provider failure category was unknown.",
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
fn process_status_failure(status: ExitStatus) -> SafeCorePiFailure {
    if let Some(code) = status.code() {
        SafeCorePiFailure::unsuccessful_exit(code)
    } else {
        SafeCorePiFailure::signal(status.signal().unwrap_or(0))
    }
}

#[cfg(feature = "core-native")]
fn log_child_exit(status: ExitStatus, cleanup: &str) {
    log::info!(
        "android_agent_child_exit code={} signal={} cleanup={cleanup} platform=core",
        status
            .code()
            .map_or("none".to_owned(), |code| code.to_string()),
        status
            .signal()
            .map_or("none".to_owned(), |signal| signal.to_string())
    );
}

#[cfg(feature = "core-native")]
fn report_core_pi_failure(failure: SafeCorePiFailure, model: &str) -> String {
    log::warn!(
        "android_agent_failure stage={} category={} model={model}{}",
        failure.stage,
        failure.category,
        failure.safe_metadata()
    );
    failure.tagged_error(model)
}

#[cfg(feature = "core-native")]
fn run_core_pi(
    request: &[u8],
    timeout: Duration,
    model: &str,
    core_dev_tunnel: bool,
) -> Result<(ExitStatus, Zeroizing<Vec<u8>>), String> {
    let mut command = Command::new(CORE_CHILD_LAUNCH.node_path);
    command
        .args(CORE_NODE_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    restrict_to_standard_fds(&mut command);
    let child = command.spawn().map_err(|error| {
        log::warn!(
            "android_agent_child_launch_failure cause={} node={} runner={} expected_domain={} platform=core",
            safe_core_launch_cause(error.kind()),
            CORE_CHILD_LAUNCH.node_identity,
            CORE_CHILD_LAUNCH.runner_identity,
            CORE_CHILD_LAUNCH.expected_domain,
        );
        "start common Pi runner".to_owned()
    })?;
    #[cfg(feature = "core-dev-credential")]
    let transport = if core_dev_tunnel {
        "adb_reverse_connect"
    } else {
        "direct"
    };
    #[cfg(not(feature = "core-dev-credential"))]
    let transport = {
        debug_assert!(!core_dev_tunnel);
        "direct"
    };
    log::info!(
        "android_agent_child_start pid={} expected_domain={} node={} runner={} provider_identity=openrouter model={model} platform=core hardening=jitless fd_boundary=stdio_only stderr=discarded transport={}",
        child.id(),
        CORE_CHILD_LAUNCH.expected_domain,
        CORE_CHILD_LAUNCH.node_identity,
        CORE_CHILD_LAUNCH.runner_identity,
        transport,
    );
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
                            log::info!(
                                "android_agent_child_request state=written provider_identity=openrouter model={model}"
                            );
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
    log_child_exit(status, "normal");
    Ok((status, response))
}

#[cfg(feature = "core-native")]
fn run_live(
    provider: &str,
    prompt: &str,
    current_source: &str,
    faux_candidate: Option<&str>,
    core_dev_tunnel: bool,
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
        #[cfg(feature = "core-dev-credential")]
        #[serde(skip_serializing_if = "Option::is_none")]
        core_dev_proxy: Option<&'static str>,
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
            #[cfg(feature = "core-dev-credential")]
            core_dev_proxy: core_dev_tunnel.then_some("http://127.0.0.1:37173"),
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
    let expected_model = expected_model(provider)
        .ok_or_else(|| "Core selected an unsupported Pi provider".to_owned())?;
    let (status, response) = run_core_pi(&request, timeout, expected_model, core_dev_tunnel)
        .map_err(|error| {
            let failure = if error.contains("timed out") {
                SafeCorePiFailure::timeout()
            } else if error.starts_with("start common Pi runner") {
                SafeCorePiFailure::launch()
            } else if error.contains("request") || error.contains("stdin") {
                SafeCorePiFailure::request_io()
            } else if error.contains("response") || error.contains("stdout") {
                SafeCorePiFailure::response_io()
            } else {
                SafeCorePiFailure::process_io()
            };
            report_core_pi_failure(failure, expected_model)
        })?;
    let line = response
        .split(|byte| *byte == b'\n')
        .rev()
        .find(|line| !line.is_empty());
    let Some(line) = line else {
        let failure = if status.success() {
            SafeCorePiFailure::empty_response(status.code().unwrap_or(0))
        } else {
            process_status_failure(status)
        };
        return Err(report_core_pi_failure(failure, expected_model));
    };
    let envelope: LiveEnvelope = serde_json::from_slice(line).map_err(|_| {
        report_core_pi_failure(
            SafeCorePiFailure::invalid_response(status.code(), status.signal()),
            expected_model,
        )
    })?;
    if envelope.protocol_version != Some(2) {
        return Err(report_core_pi_failure(
            SafeCorePiFailure::invalid_response(status.code(), status.signal()),
            expected_model,
        ));
    }
    let response_type = if envelope.source.is_some() {
        "prompt_complete"
    } else if envelope.category.is_some() {
        "error"
    } else {
        "unexpected"
    };
    let expected_terminal = if response_type == "prompt_complete" {
        "completed"
    } else if response_type == "error" {
        "failed"
    } else {
        "unknown"
    };
    if envelope.terminal.as_deref() != Some(expected_terminal) {
        return Err(report_core_pi_failure(
            SafeCorePiFailure::invalid_response(status.code(), status.signal()),
            expected_model,
        ));
    }
    log::info!(
        "android_agent_child_response_header protocol=2 type={response_type} terminal={expected_terminal} provider={provider} model={expected_model}"
    );
    if envelope.category.is_some() {
        return Err(structured_failure(&envelope, expected_model));
    }
    if !status.success() {
        return Err(report_core_pi_failure(
            process_status_failure(status),
            expected_model,
        ));
    }
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
    core_dev_tunnel: bool,
) -> Result<LiveCandidate, String> {
    debug_assert!(!core_dev_tunnel);
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
