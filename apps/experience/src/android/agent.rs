#[cfg(feature = "core-native")]
use std::io::{Read, Write};
#[cfg(feature = "core-native")]
use std::os::fd::AsRawFd;
#[cfg(feature = "core-native")]
use std::process::{Child, Command, ExitStatus, Stdio};
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

#[derive(Deserialize)]
struct LiveEnvelope {
    #[cfg(not(feature = "core-native"))]
    #[serde(default)]
    ok: Option<bool>,
    source: Option<String>,
    summary: Option<String>,
    error: Option<String>,
    #[serde(default)]
    actions: Vec<String>,
}

struct LiveCandidate {
    source: String,
    summary: String,
}

#[cfg(feature = "core-native")]
const CORE_PI_TIMEOUT_SECONDS: u64 = 30;
#[cfg(feature = "core-native")]
const CORE_PI_TIMEOUT: Duration = Duration::from_secs(CORE_PI_TIMEOUT_SECONDS);
#[cfg(feature = "core-native")]
const MAX_PI_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

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
    let faux_candidate = if status.provider == "fake" {
        Some(deterministic_agent_candidate(current_source))
    } else {
        None
    };
    let candidate = run_live(prompt, current_source, faux_candidate)?;
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
) -> Result<(), String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| {
            format!("common Pi runner timed out after {CORE_PI_TIMEOUT_SECONDS} seconds")
        })?;
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
        return Err(format!(
            "common Pi runner timed out after {CORE_PI_TIMEOUT_SECONDS} seconds"
        ));
    }
    Ok(())
}

#[cfg(feature = "core-native")]
fn run_core_pi(request: &[u8]) -> Result<(ExitStatus, Vec<u8>), String> {
    let child = Command::new("/system_ext/bin/sos-node")
        .args([
            "/system_ext/etc/sos-agent/agent-runner.cjs",
            "stdio",
            "--api-doc",
            "/system_ext/etc/sos-agent/experience-api.md",
            "--example",
            "/system_ext/etc/sos-agent/example-primary.luau",
            "--example-secondary",
            "/system_ext/etc/sos-agent/example-secondary.luau",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("start common Pi runner: {error}"))?;
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

    let deadline = Instant::now() + CORE_PI_TIMEOUT;
    let mut written = 0;
    let mut response = Vec::new();
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
        poll_until(&mut descriptors, deadline, Duration::from_secs(1))?;

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
        poll_until(&mut [], deadline, Duration::from_millis(10))?;
    }
    Ok((child.finish()?, response))
}

#[cfg(feature = "core-native")]
fn run_live(
    prompt: &str,
    current_source: &str,
    faux_candidate: Option<&str>,
) -> Result<LiveCandidate, String> {
    let candidate = faux_candidate.ok_or_else(|| {
        "Core live-agent credentials require a trusted native ceremony".to_owned()
    })?;
    if prompt.is_empty() || prompt.len() > 32 * 1024 {
        return Err("agent prompt is outside the bounded size".into());
    }
    let request = serde_json::to_vec(&serde_json::json!({
        "action": "prompt",
        "provider": "faux",
        "prompt": prompt,
        "currentSource": current_source,
        "candidateSource": candidate,
    }))
    .map_err(|error| format!("encode Pi request: {error}"))?;
    if request.len() > 1024 * 1024 {
        return Err("Pi request is outside the bounded size".into());
    }
    let (status, response) = run_core_pi(&request)?;
    let line = response
        .split(|byte| *byte == b'\n')
        .rev()
        .find(|line| !line.is_empty())
        .ok_or_else(|| "common Pi runner returned no response".to_owned())?;
    let envelope: LiveEnvelope = serde_json::from_slice(line)
        .map_err(|_| "common Pi runner returned an invalid response".to_owned())?;
    if !status.success() || envelope.source.is_none() {
        return Err(envelope
            .error
            .unwrap_or_else(|| "common Pi runner did not produce a candidate".into()));
    }
    verify_actions(&envelope.actions)?;
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
    prompt: &str,
    current_source: &str,
    faux_candidate: Option<&str>,
) -> Result<LiveCandidate, String> {
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let prompt = JObject::from(env.new_string(prompt).map_err(|error| error.to_string())?);
        let source = JObject::from(
            env.new_string(current_source)
                .map_err(|error| error.to_string())?,
        );
        let candidate = JObject::from(
            env.new_string(faux_candidate.unwrap_or_default())
                .map_err(|error| error.to_string())?,
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
        if envelope.ok != Some(true) {
            return Err(envelope
                .error
                .unwrap_or_else(|| "Pi did not produce a candidate".into()));
        }
        verify_actions(&envelope.actions)?;
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

fn verify_actions(actions: &[String]) -> Result<(), String> {
    const EXPECTED: [&str; 3] = [
        "get_experience_context",
        "validate_experience",
        "submit_experience",
    ];
    (actions.iter().map(String::as_str).eq(EXPECTED))
        .then_some(())
        .ok_or_else(|| "Pi runner used an unexpected tool sequence".into())
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
