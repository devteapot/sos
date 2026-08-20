pub const OPENROUTER_MODEL: &str = "deepseek/deepseek-v4-flash-0731";
pub const OPENAI_MODEL: &str = "gpt-5.6-luna";
pub const CODEX_MODEL: &str = "gpt-5.6-sol";
pub const FAUX_MODEL: &str = "faux";
#[cfg(any(feature = "core-native", test))]
pub const FAUX_PI_TIMEOUT_SECONDS: u64 = 30;
#[cfg(any(feature = "core-native", test))]
pub const LIVE_PI_TIMEOUT_SECONDS: u64 = 240;
pub const VERIFIED_ACTIONS: [&str; 3] = [
    "get_experience_context",
    "validate_experience",
    "submit_experience",
];
#[cfg(any(feature = "core-dev-credential", test))]
pub const CORE_DEV_AGENT_SMOKE_PROMPT: &str =
    "Create one visible item titled Blue smoke check with body Fixed Core development smoke item.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentUiTransport {
    ValidatedNetwork,
    #[cfg(any(feature = "core-dev-credential", test))]
    CoreDevFixedTunnel,
}

#[cfg(any(feature = "core-dev-credential", test))]
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreDevSmokeAuthorization {
    armed: bool,
}

#[cfg(any(feature = "core-dev-credential", test))]
impl CoreDevSmokeAuthorization {
    pub const fn new() -> Self {
        Self { armed: false }
    }

    pub fn arm_authenticated(&mut self) -> bool {
        if self.armed {
            return false;
        }
        self.armed = true;
        true
    }

    pub fn consume_fixed_prompt(&mut self, prompt: &str) -> Option<AgentUiTransport> {
        if !std::mem::take(&mut self.armed) || prompt != CORE_DEV_AGENT_SMOKE_PROMPT {
            return None;
        }
        Some(AgentUiTransport::CoreDevFixedTunnel)
    }

    #[cfg(test)]
    pub fn is_armed(&self) -> bool {
        self.armed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentUiAttemptEventKind {
    Received,
    DispatchStarted,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentUiAttemptEvent {
    pub attempt_id: u64,
    pub kind: AgentUiAttemptEventKind,
    pub provider: &'static str,
    pub model: &'static str,
    pub configured: bool,
    pub busy: bool,
    pub input_present: bool,
    pub stage: &'static str,
    pub category: &'static str,
}

impl AgentUiAttemptEvent {
    pub fn request_terminal_marker(&self) -> Result<String, &'static str> {
        if self.kind != AgentUiAttemptEventKind::Terminal {
            return Err("agent request terminal marker requires a terminal event");
        }
        Ok(format!(
            "android_agent_request_terminal stage={} category={} provider={} model={} model_policy=pinned attempt={} correlation=serialized",
            self.stage, self.category, self.provider, self.model, self.attempt_id
        ))
    }

    pub fn ui_terminal_marker(&self) -> Result<String, &'static str> {
        if self.kind != AgentUiAttemptEventKind::Terminal {
            return Err("agent UI terminal marker requires a terminal event");
        }
        let status = if self.category == "completed" {
            "completed"
        } else {
            "failed"
        };
        Ok(format!(
            "android_agent_ui_terminal status={status} stage={} category={} provider={} model={} model_policy=pinned attempt={} correlation=serialized",
            self.stage, self.category, self.provider, self.model, self.attempt_id
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentUiFailure {
    pub stage: &'static str,
    pub category: &'static str,
    pub display: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentUiAttemptPhase {
    Received,
    DispatchStarted,
    Accepted,
    Terminal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUiAttempt {
    attempt_id: u64,
    provider: &'static str,
    model: &'static str,
    configured: bool,
    busy: bool,
    input_present: bool,
    network_available: bool,
    transport: AgentUiTransport,
    phase: AgentUiAttemptPhase,
}

impl AgentUiAttempt {
    pub fn receive(
        attempt_id: u64,
        provider: &str,
        configured: bool,
        busy: bool,
        input_present: bool,
        network_available: bool,
    ) -> (Self, AgentUiAttemptEvent) {
        let provider = safe_provider_identity(provider);
        let model = expected_model(provider).unwrap_or("unsupported");
        let attempt = Self {
            attempt_id,
            provider,
            model,
            configured,
            busy,
            input_present,
            network_available,
            transport: AgentUiTransport::ValidatedNetwork,
            phase: AgentUiAttemptPhase::Received,
        };
        let received = attempt.event(AgentUiAttemptEventKind::Received, "ui", "none");
        (attempt, received)
    }

    pub fn preflight(&self) -> Result<(), AgentUiFailure> {
        if !self.input_present {
            Err(ui_failure("preflight", "empty_input"))
        } else if self.busy {
            Err(ui_failure("preflight", "busy"))
        } else if self.model == "unsupported" {
            Err(ui_failure("preflight", "model_policy"))
        } else if self.provider != "fake" && !self.configured {
            Err(ui_failure("preflight", "credential_missing"))
        } else if self.provider != "fake" && !self.transport_ready(self.network_available) {
            Err(ui_failure("preflight", "network_unavailable"))
        } else {
            Ok(())
        }
    }

    pub fn dispatch_started(&mut self) -> Result<AgentUiAttemptEvent, &'static str> {
        if self.phase != AgentUiAttemptPhase::Received {
            return Err("agent UI attempt dispatch is out of order");
        }
        self.phase = AgentUiAttemptPhase::DispatchStarted;
        Ok(self.event(AgentUiAttemptEventKind::DispatchStarted, "dispatch", "none"))
    }

    pub fn accepted_marker(&mut self) -> Result<String, &'static str> {
        if self.phase != AgentUiAttemptPhase::DispatchStarted {
            return Err("agent UI attempt acceptance is out of order");
        }
        self.phase = AgentUiAttemptPhase::Accepted;
        Ok(format!(
            "android_agent_request_accepted provider={} model={} model_policy=pinned attempt={} correlation=serialized",
            self.provider, self.model, self.attempt_id
        ))
    }

    pub fn terminal(
        &mut self,
        failure: Option<AgentUiFailure>,
    ) -> Result<AgentUiAttemptEvent, &'static str> {
        if self.phase == AgentUiAttemptPhase::Terminal {
            return Err("agent UI attempt already has a terminal event");
        }
        self.phase = AgentUiAttemptPhase::Terminal;
        let (stage, category) = failure
            .map(|failure| (failure.stage, failure.category))
            .unwrap_or(("ui", "completed"));
        Ok(self.event(AgentUiAttemptEventKind::Terminal, stage, category))
    }

    pub fn attempt_id(&self) -> u64 {
        self.attempt_id
    }

    pub fn provider(&self) -> &'static str {
        self.provider
    }

    #[cfg(any(feature = "core-dev-credential", test))]
    pub fn receive_core_dev_fixed_smoke(
        attempt_id: u64,
        provider: &str,
        configured: bool,
        busy: bool,
        prompt: &str,
        transport: AgentUiTransport,
    ) -> Result<(Self, AgentUiAttemptEvent), AgentUiFailure> {
        if transport != AgentUiTransport::CoreDevFixedTunnel
            || prompt != CORE_DEV_AGENT_SMOKE_PROMPT
            || safe_provider_identity(provider) != "openrouter"
        {
            return Err(ui_failure("preflight", "model_policy"));
        }
        let (mut attempt, received) =
            Self::receive(attempt_id, provider, configured, busy, true, false);
        attempt.transport = AgentUiTransport::CoreDevFixedTunnel;
        Ok((attempt, received))
    }

    #[cfg(test)]
    pub fn transport(&self) -> AgentUiTransport {
        self.transport
    }

    pub fn uses_core_dev_fixed_tunnel(&self) -> bool {
        match self.transport {
            AgentUiTransport::ValidatedNetwork => false,
            #[cfg(any(feature = "core-dev-credential", test))]
            AgentUiTransport::CoreDevFixedTunnel => true,
        }
    }

    pub fn transport_ready(&self, network_available: bool) -> bool {
        match self.transport {
            AgentUiTransport::ValidatedNetwork => network_available,
            #[cfg(any(feature = "core-dev-credential", test))]
            AgentUiTransport::CoreDevFixedTunnel => true,
        }
    }

    pub fn prompt_matches_transport(&self, _prompt: &str) -> bool {
        match self.transport {
            AgentUiTransport::ValidatedNetwork => true,
            #[cfg(any(feature = "core-dev-credential", test))]
            AgentUiTransport::CoreDevFixedTunnel => _prompt == CORE_DEV_AGENT_SMOKE_PROMPT,
        }
    }

    fn event(
        &self,
        kind: AgentUiAttemptEventKind,
        stage: &'static str,
        category: &'static str,
    ) -> AgentUiAttemptEvent {
        AgentUiAttemptEvent {
            attempt_id: self.attempt_id,
            kind,
            provider: self.provider,
            model: self.model,
            configured: self.configured,
            busy: self.busy,
            input_present: self.input_present,
            stage,
            category,
        }
    }
}

pub fn safe_provider_identity(provider: &str) -> &'static str {
    match provider {
        "fake" | "faux" => "fake",
        "openrouter" => "openrouter",
        "openai" => "openai",
        "openai-codex" => "openai-codex",
        _ => "unsupported",
    }
}

pub fn ui_failure(stage: &str, category: &str) -> AgentUiFailure {
    let stage = safe_ui_failure_stage(stage);
    let category = safe_ui_failure_category(category);
    let display = match category {
        "empty_input" => "Enter a request before submitting.",
        "credential_missing" => "The selected provider is not configured.",
        "busy" => "SOS is already handling a request.",
        "model_policy" => "The selected provider does not match the pinned model policy.",
        "dispatch_channel" => "The request could not reach the experience runtime.",
        "runtime_start" => "The request runtime could not start.",
        "network_unavailable" => "Connect to a validated network before submitting.",
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
        "linker_or_exit" | "exit_failure" | "signal" => {
            "The local Pi process exited before returning a response."
        }
        "process_io" => "The local Pi process could not be observed.",
        "invalid_response" => "The local Pi process returned an invalid response.",
        "unexpected_response" => "The local Pi process returned an unexpected response type.",
        "refresh_failed" => "The refreshed provider credential could not be stored.",
        "request_io" => "The local Pi process could not accept its request.",
        "internal" => "The trusted on-device Pi bridge failed.",
        "protocol_error" => "The Pi protocol failed.",
        _ => "The provider failure category was unknown.",
    };
    AgentUiFailure {
        stage,
        category,
        display,
    }
}

fn safe_ui_failure_stage(value: &str) -> &'static str {
    match value {
        "preflight" => "preflight",
        "dispatch" => "dispatch",
        "runtime" => "runtime",
        other => safe_failure_stage(Some(other)),
    }
}

fn safe_ui_failure_category(value: &str) -> &'static str {
    match value {
        "empty_input" => "empty_input",
        "credential_missing" => "credential_missing",
        "busy" => "busy",
        "model_policy" => "model_policy",
        "dispatch_channel" => "dispatch_channel",
        "runtime_start" => "runtime_start",
        "network_unavailable" => "network_unavailable",
        other => safe_failure_category(Some(other)),
    }
}

pub fn safe_failure_stage(value: Option<&str>) -> &'static str {
    match value {
        Some("request") => "request",
        Some("credential") => "credential",
        Some("transport") => "transport",
        Some("provider") => "provider",
        Some("protocol") => "protocol",
        Some("validation") => "validation",
        Some("bridge") => "bridge",
        Some("child") => "child",
        _ => "protocol",
    }
}

pub fn safe_failure_category(value: Option<&str>) -> &'static str {
    match value {
        Some("invalid_request") => "invalid_request",
        Some("credential_rejected") => "credential_rejected",
        Some("provider_rejected") => "provider_rejected",
        Some("rate_limited") => "rate_limited",
        Some("provider_unavailable") => "provider_unavailable",
        Some("provider_error") => "provider_error",
        Some("dns_resolution") => "dns_resolution",
        Some("dns_timeout") => "dns_timeout",
        Some("dns_proxy_unavailable") => "dns_proxy_unavailable",
        Some("connect_timeout") => "connect_timeout",
        Some("connect_refused") => "connect_refused",
        Some("connect_reset") => "connect_reset",
        Some("network_unreachable") => "network_unreachable",
        Some("tls_failure") => "tls_failure",
        Some("tool_sequence") => "tool_sequence",
        Some("invalid_candidate") => "invalid_candidate",
        Some("protocol_error") => "protocol_error",
        Some("unknown") => "unknown",
        Some("internal") => "internal",
        Some("launch_failure") => "launch_failure",
        Some("timeout") => "timeout",
        Some("response_timeout") => "response_timeout",
        Some("response_io") => "response_io",
        Some("empty_response") => "empty_response",
        Some("linker_or_exit") => "linker_or_exit",
        Some("exit_failure") => "exit_failure",
        Some("signal") => "signal",
        Some("process_io") => "process_io",
        Some("invalid_response") => "invalid_response",
        Some("unexpected_response") => "unexpected_response",
        Some("wrong_model") => "wrong_model",
        Some("refresh_failed") => "refresh_failed",
        Some("request_io") => "request_io",
        _ => "unknown",
    }
}

pub fn safe_http_status(value: Option<u16>) -> Option<u16> {
    value.filter(|status| (100..=599).contains(status))
}

#[cfg(any(feature = "core-native", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreChildLaunchContract {
    pub node_path: &'static str,
    pub runner_path: &'static str,
    pub node_identity: &'static str,
    pub runner_identity: &'static str,
    pub expected_domain: &'static str,
}

#[cfg(any(feature = "core-native", test))]
#[cfg(not(feature = "core-dev-credential"))]
pub const CORE_CHILD_LAUNCH: CoreChildLaunchContract = CoreChildLaunchContract {
    node_path: "/system_ext/bin/sos-node",
    runner_path: "/system_ext/etc/sos-agent/agent-runner.cjs",
    node_identity: "ordinary_node",
    runner_identity: "ordinary_runner",
    expected_domain: "sos_core_agent",
};

#[cfg(any(feature = "core-native", test))]
#[cfg(feature = "core-dev-credential")]
pub const CORE_CHILD_LAUNCH: CoreChildLaunchContract = CoreChildLaunchContract {
    node_path: "/system_ext/bin/sos-node-core-dev",
    runner_path: "/system_ext/etc/sos-agent/agent-runner-core-dev.cjs",
    node_identity: "core_dev_node",
    runner_identity: "core_dev_runner",
    expected_domain: "sos_core_dev_agent",
};

#[cfg(any(feature = "core-native", test))]
pub const CORE_NODE_ARGS: [&str; 9] = [
    "--jitless",
    CORE_CHILD_LAUNCH.runner_path,
    "stdio",
    "--api-doc",
    "/system_ext/etc/sos-agent/experience-api.md",
    "--example",
    "/system_ext/etc/sos-agent/example-primary.luau",
    "--example-secondary",
    "/system_ext/etc/sos-agent/example-secondary.luau",
];

#[cfg(any(feature = "core-native", test))]
pub fn safe_core_launch_cause(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "path_missing",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::OutOfMemory => "resource_exhausted",
        std::io::ErrorKind::Unsupported => "unsupported",
        _ => "other",
    }
}

#[cfg(any(feature = "core-native", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafeCorePiFailure {
    pub stage: &'static str,
    pub category: &'static str,
    pub detail: &'static str,
    pub child_exit: Option<i32>,
    pub signal: Option<i32>,
}

#[cfg(any(feature = "core-native", test))]
impl SafeCorePiFailure {
    pub const fn launch() -> Self {
        Self::new(
            "child",
            "launch_failure",
            "The local Pi process could not start.",
        )
    }

    pub const fn timeout() -> Self {
        Self::new("child", "timeout", "The local Pi process timed out.")
    }

    pub const fn request_io() -> Self {
        Self::new(
            "child",
            "request_io",
            "The local Pi process could not accept its request.",
        )
    }

    pub const fn response_io() -> Self {
        Self::new(
            "child",
            "response_io",
            "The local Pi response could not be read.",
        )
    }

    pub const fn process_io() -> Self {
        Self::new(
            "child",
            "process_io",
            "The local Pi process could not be observed.",
        )
    }

    pub const fn unsuccessful_exit(code: i32) -> Self {
        Self::new(
            "child",
            "exit_failure",
            "The local Pi process exited unsuccessfully.",
        )
        .with_child_exit(code)
    }

    pub const fn signal(signal: i32) -> Self {
        Self::new(
            "child",
            "signal",
            "The local Pi process was terminated by a signal.",
        )
        .with_signal(signal)
    }

    pub const fn empty_response(child_exit: i32) -> Self {
        Self::new(
            "protocol",
            "empty_response",
            "The local Pi process returned no response.",
        )
        .with_child_exit(child_exit)
    }

    pub const fn invalid_response(child_exit: Option<i32>, signal: Option<i32>) -> Self {
        let mut failure = Self::new(
            "protocol",
            "invalid_response",
            "The local Pi process returned an invalid response.",
        );
        failure.child_exit = child_exit;
        failure.signal = signal;
        failure
    }

    const fn new(stage: &'static str, category: &'static str, detail: &'static str) -> Self {
        Self {
            stage,
            category,
            detail,
            child_exit: None,
            signal: None,
        }
    }

    const fn with_child_exit(mut self, child_exit: i32) -> Self {
        self.child_exit = Some(child_exit);
        self
    }

    const fn with_signal(mut self, signal: i32) -> Self {
        self.signal = Some(signal);
        self
    }

    pub fn tagged_error(self, model: &str) -> String {
        let metadata = self.safe_metadata();
        format!(
            "{} [{}/{}{}; model={model}]",
            self.detail, self.stage, self.category, metadata
        )
    }

    pub fn safe_metadata(self) -> String {
        match (self.child_exit, self.signal) {
            (Some(code), None) => format!("; child_exit={code}"),
            (None, Some(signal)) => format!("; signal={signal}"),
            _ => String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentActivationPhase {
    Submitted,
    Validated,
    Staged,
    Committed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentActivationEvidence {
    request_id: u64,
    phase: AgentActivationPhase,
}

impl AgentActivationEvidence {
    pub fn submitted(request_id: u64) -> Self {
        Self {
            request_id,
            phase: AgentActivationPhase::Submitted,
        }
    }

    pub fn advance(&mut self, next: AgentActivationPhase) -> Result<(), &'static str> {
        let allowed = matches!(
            (self.phase, next),
            (
                AgentActivationPhase::Submitted,
                AgentActivationPhase::Validated
            ) | (
                AgentActivationPhase::Validated,
                AgentActivationPhase::Staged
            ) | (
                AgentActivationPhase::Staged,
                AgentActivationPhase::Committed
            )
        );
        if !allowed {
            return Err("agent activation evidence phase is missing or out of order");
        }
        self.phase = next;
        Ok(())
    }

    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    #[cfg(test)]
    pub fn phase(&self) -> AgentActivationPhase {
        self.phase
    }
}

pub fn verified_action_sequence(actions: &[String]) -> Option<&[String]> {
    actions
        .iter()
        .map(String::as_str)
        .eq(VERIFIED_ACTIONS)
        .then_some(actions)
}

pub fn expected_model(provider: &str) -> Option<&'static str> {
    match provider {
        "fake" | "faux" => Some(FAUX_MODEL),
        "openai" => Some(OPENAI_MODEL),
        "openrouter" => Some(OPENROUTER_MODEL),
        "openai-codex" => Some(CODEX_MODEL),
        _ => None,
    }
}

pub fn model_is_exact(provider: &str, model: &str) -> bool {
    expected_model(provider) == Some(model)
}

pub fn reconciled_request_error(
    current: Option<String>,
    intentional_clear: bool,
) -> Option<String> {
    if intentional_clear {
        None
    } else {
        current
    }
}

#[cfg(any(feature = "core-native", test))]
pub fn pi_timeout_seconds(provider: &str) -> Option<u64> {
    expected_model(provider).map(|model| {
        if model == FAUX_MODEL {
            FAUX_PI_TIMEOUT_SECONDS
        } else {
            LIVE_PI_TIMEOUT_SECONDS
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_contract_pins_models_and_live_timeout() {
        assert_eq!(expected_model("fake"), Some("faux"));
        assert_eq!(expected_model("openrouter"), Some(OPENROUTER_MODEL));
        assert!(model_is_exact(
            "openrouter",
            "deepseek/deepseek-v4-flash-0731"
        ));
        for rejected in [
            "deepseek/deepseek-v4-flash",
            "deepseek/deepseek-v4-flash-latest",
            "deepseek/deepseek-v4-flash-0731:free",
            "deepseek/deepseek-v4-flash-0731-extra",
        ] {
            assert!(!model_is_exact("openrouter", rejected));
        }
        assert_eq!(pi_timeout_seconds("fake"), Some(30));
        assert_eq!(pi_timeout_seconds("openrouter"), Some(240));
        assert_eq!(expected_model("unknown"), None);
        assert_eq!(pi_timeout_seconds("unknown"), None);
    }

    #[test]
    fn structured_failures_are_allowlisted_and_unknown_content_stays_unknown() {
        for (input, expected) in [
            ("dns_resolution", "dns_resolution"),
            ("dns_timeout", "dns_timeout"),
            ("dns_proxy_unavailable", "dns_proxy_unavailable"),
            ("connect_timeout", "connect_timeout"),
            ("connect_refused", "connect_refused"),
            ("connect_reset", "connect_reset"),
            ("network_unreachable", "network_unreachable"),
            ("tls_failure", "tls_failure"),
            ("rate_limited", "rate_limited"),
            ("exit_failure", "exit_failure"),
        ] {
            assert_eq!(safe_failure_category(Some(input)), expected);
        }
        assert_eq!(
            safe_failure_category(Some("ENOTFOUND\nandroid_agent_failure category=injected")),
            "unknown"
        );
        assert_eq!(safe_failure_stage(Some("provider\nchild")), "protocol");
        assert_eq!(safe_http_status(Some(429)), Some(429));
        assert_eq!(safe_http_status(Some(99)), None);
        assert_eq!(safe_http_status(Some(600)), None);
    }

    #[test]
    fn routine_status_preserves_a_request_error_until_an_intentional_action() {
        let error = Some("Provider request failed (provider/rate_limited).".to_owned());
        assert_eq!(reconciled_request_error(error.clone(), false), error);
        assert_eq!(reconciled_request_error(error, true), None);
    }

    #[test]
    fn action_evidence_requires_the_complete_exact_order() {
        let exact = VERIFIED_ACTIONS.map(str::to_owned);
        assert_eq!(verified_action_sequence(&exact), Some(exact.as_slice()));
        for rejected in [
            vec!["get_experience_context", "validate_experience"],
            vec![
                "validate_experience",
                "get_experience_context",
                "submit_experience",
            ],
            vec![
                "get_experience_context",
                "validate_experience",
                "submit_experience",
                "submit_experience",
            ],
        ] {
            let rejected = rejected.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(verified_action_sequence(&rejected), None);
        }
    }

    #[test]
    fn core_node_hardening_precedes_the_fixed_script_and_request_mode() {
        assert_eq!(CORE_NODE_ARGS[0], "--jitless");
        #[cfg(not(feature = "core-dev-credential"))]
        assert_eq!(
            CORE_CHILD_LAUNCH,
            CoreChildLaunchContract {
                node_path: "/system_ext/bin/sos-node",
                runner_path: "/system_ext/etc/sos-agent/agent-runner.cjs",
                node_identity: "ordinary_node",
                runner_identity: "ordinary_runner",
                expected_domain: "sos_core_agent",
            }
        );
        #[cfg(feature = "core-dev-credential")]
        assert_eq!(
            CORE_CHILD_LAUNCH,
            CoreChildLaunchContract {
                node_path: "/system_ext/bin/sos-node-core-dev",
                runner_path: "/system_ext/etc/sos-agent/agent-runner-core-dev.cjs",
                node_identity: "core_dev_node",
                runner_identity: "core_dev_runner",
                expected_domain: "sos_core_dev_agent",
            }
        );
        assert_eq!(CORE_NODE_ARGS[1], CORE_CHILD_LAUNCH.runner_path);
        assert_eq!(CORE_NODE_ARGS[2], "stdio");
        assert_eq!(
            CORE_NODE_ARGS
                .iter()
                .filter(|arg| **arg == "--jitless")
                .count(),
            1
        );
        assert!(CORE_NODE_ARGS.iter().all(|arg| !arg.contains("credential")));
    }

    #[test]
    fn launch_failure_causes_are_allowlisted_without_error_text() {
        assert_eq!(
            safe_core_launch_cause(std::io::ErrorKind::NotFound),
            "path_missing"
        );
        assert_eq!(
            safe_core_launch_cause(std::io::ErrorKind::PermissionDenied),
            "permission_denied"
        );
        assert_eq!(
            safe_core_launch_cause(std::io::ErrorKind::WouldBlock),
            "resource_exhausted"
        );
        assert_eq!(
            safe_core_launch_cause(std::io::ErrorKind::Unsupported),
            "unsupported"
        );
        assert_eq!(safe_core_launch_cause(std::io::ErrorKind::Other), "other");
    }

    #[test]
    fn core_child_and_protocol_failures_are_distinct_and_secret_safe() {
        let cases = [
            SafeCorePiFailure::launch(),
            SafeCorePiFailure::timeout(),
            SafeCorePiFailure::request_io(),
            SafeCorePiFailure::response_io(),
            SafeCorePiFailure::process_io(),
            SafeCorePiFailure::unsuccessful_exit(23),
            SafeCorePiFailure::signal(6),
            SafeCorePiFailure::empty_response(0),
            SafeCorePiFailure::invalid_response(Some(0), None),
        ];
        assert_eq!(
            cases.map(|failure| failure.category),
            [
                "launch_failure",
                "timeout",
                "request_io",
                "response_io",
                "process_io",
                "exit_failure",
                "signal",
                "empty_response",
                "invalid_response",
            ]
        );
        let reports = cases.map(|failure| failure.tagged_error(OPENROUTER_MODEL));
        assert!(reports[5].contains("child_exit=23"));
        assert!(reports[6].contains("signal=6"));
        for report in reports {
            for secret in [
                "stderr-secret",
                "provider-body",
                "request-secret",
                "sk-or-v1-",
            ] {
                assert!(!report.contains(secret));
            }
        }
    }

    #[test]
    fn activation_evidence_cannot_claim_commit_from_staged_or_validated_state() {
        let mut evidence = AgentActivationEvidence::submitted(41);
        assert_eq!(evidence.request_id(), 41);
        assert_eq!(
            evidence.advance(AgentActivationPhase::Committed),
            Err("agent activation evidence phase is missing or out of order")
        );
        evidence.advance(AgentActivationPhase::Validated).unwrap();
        assert_eq!(evidence.phase(), AgentActivationPhase::Validated);
        assert!(evidence.advance(AgentActivationPhase::Committed).is_err());
        evidence.advance(AgentActivationPhase::Staged).unwrap();
        assert_eq!(evidence.phase(), AgentActivationPhase::Staged);
        evidence.advance(AgentActivationPhase::Committed).unwrap();
        assert_eq!(evidence.phase(), AgentActivationPhase::Committed);
        assert!(evidence.advance(AgentActivationPhase::Committed).is_err());
    }

    #[test]
    fn every_ui_preflight_exit_has_one_allowlisted_terminal() {
        let cases = [
            ("openrouter", true, false, false, true, "empty_input"),
            ("openrouter", true, true, true, true, "busy"),
            ("openrouter", false, false, true, true, "credential_missing"),
            (
                "openrouter",
                true,
                false,
                true,
                false,
                "network_unavailable",
            ),
            (
                "provider-body\nforged",
                true,
                false,
                true,
                true,
                "model_policy",
            ),
        ];
        for (index, (provider, configured, busy, input_present, network, category)) in
            cases.into_iter().enumerate()
        {
            let (mut attempt, received) = AgentUiAttempt::receive(
                index as u64 + 1,
                provider,
                configured,
                busy,
                input_present,
                network,
            );
            assert_eq!(received.kind, AgentUiAttemptEventKind::Received);
            let failure = attempt.preflight().unwrap_err();
            assert_eq!(failure.category, category);
            let terminal = attempt.terminal(Some(failure)).unwrap();
            assert_eq!(terminal.kind, AgentUiAttemptEventKind::Terminal);
            assert_eq!(terminal.category, category);
            assert!(terminal
                .request_terminal_marker()
                .unwrap()
                .starts_with("android_agent_request_terminal stage=preflight category="));
            assert!(terminal
                .ui_terminal_marker()
                .unwrap()
                .starts_with("android_agent_ui_terminal status=failed stage=preflight category="));
            assert!(attempt.terminal(Some(failure)).is_err());
            assert!(!terminal.provider.contains('\n'));
        }
    }

    #[test]
    fn full_ui_dispatch_orders_received_start_and_one_terminal() {
        let (mut attempt, received) =
            AgentUiAttempt::receive(17, "openrouter", true, false, true, true);
        assert_eq!(attempt.attempt_id(), 17);
        assert_eq!(attempt.provider(), "openrouter");
        assert_eq!(attempt.preflight(), Ok(()));
        let started = attempt.dispatch_started().unwrap();
        let accepted = attempt.accepted_marker().unwrap();
        let terminal = attempt.terminal(None).unwrap();
        assert_eq!(
            [received.kind, started.kind, terminal.kind],
            [
                AgentUiAttemptEventKind::Received,
                AgentUiAttemptEventKind::DispatchStarted,
                AgentUiAttemptEventKind::Terminal,
            ]
        );
        assert_eq!(terminal.category, "completed");
        let request_terminal = terminal.request_terminal_marker().unwrap();
        assert!(accepted.starts_with("android_agent_request_accepted provider="));
        assert!(request_terminal.starts_with("android_agent_request_terminal stage="));
        assert_eq!(
            [accepted.as_str(), request_terminal.as_str()]
                .iter()
                .filter(|marker| marker.starts_with("android_agent_request_terminal stage="))
                .count(),
            1
        );
        assert_eq!(
            terminal.ui_terminal_marker().unwrap(),
            "android_agent_ui_terminal status=completed stage=ui category=completed provider=openrouter model=deepseek/deepseek-v4-flash-0731 model_policy=pinned attempt=17 correlation=serialized"
        );
        assert!(attempt.terminal(None).is_err());
    }

    #[test]
    fn accepted_marker_follows_dispatch_and_emits_exactly_once() {
        let (mut attempt, received) =
            AgentUiAttempt::receive(18, "openrouter", true, false, true, true);
        assert_eq!(received.kind, AgentUiAttemptEventKind::Received);
        assert!(attempt.accepted_marker().is_err());
        let started = attempt.dispatch_started().unwrap();
        assert_eq!(started.kind, AgentUiAttemptEventKind::DispatchStarted);

        let marker = attempt.accepted_marker().unwrap();
        assert_eq!(
            marker,
            "android_agent_request_accepted provider=openrouter model=deepseek/deepseek-v4-flash-0731 model_policy=pinned attempt=18 correlation=serialized"
        );
        assert!(attempt.accepted_marker().is_err());
        assert_eq!(
            attempt.terminal(None).unwrap().kind,
            AgentUiAttemptEventKind::Terminal
        );
    }

    #[test]
    fn rejected_attempts_do_not_accept_and_marker_metadata_is_secret_safe() {
        let (mut rejected, _) = AgentUiAttempt::receive(19, "openrouter", false, false, true, true);
        let failure = rejected.preflight().unwrap_err();
        assert_eq!(failure.category, "credential_missing");
        let rejected_terminal = rejected.terminal(Some(failure)).unwrap();
        assert!(rejected.accepted_marker().is_err());
        assert_eq!(
            rejected_terminal.request_terminal_marker().unwrap(),
            "android_agent_request_terminal stage=preflight category=credential_missing provider=openrouter model=deepseek/deepseek-v4-flash-0731 model_policy=pinned attempt=19 correlation=serialized"
        );

        let (mut sanitized, _) = AgentUiAttempt::receive(
            20,
            "provider-body\nprompt=secret key=sk-or-v1-response-secret",
            true,
            false,
            true,
            true,
        );
        sanitized.dispatch_started().unwrap();
        let marker = sanitized.accepted_marker().unwrap();
        assert_eq!(
            marker,
            "android_agent_request_accepted provider=unsupported model=unsupported model_policy=pinned attempt=20 correlation=serialized"
        );
        for secret in ["provider-body", "prompt", "secret", "key", "response"] {
            assert!(!marker.contains(secret));
        }
        let terminal = sanitized
            .terminal(Some(ui_failure(
                "provider\nresponse-secret",
                "provider-body prompt=secret key=sk-or-v1-",
            )))
            .unwrap();
        for terminal_marker in [
            terminal.request_terminal_marker().unwrap(),
            terminal.ui_terminal_marker().unwrap(),
        ] {
            assert!(terminal_marker.contains("provider=unsupported model=unsupported"));
            assert!(terminal_marker.contains("stage=protocol category=unknown"));
            for secret in [
                "provider-body",
                "response-secret",
                "prompt=secret",
                "sk-or-v1-",
            ] {
                assert!(!terminal_marker.contains(secret));
            }
        }
    }

    #[test]
    fn offline_authenticated_fixed_tunnel_dispatches_but_ordinary_requests_still_reject() {
        let (ordinary, _) = AgentUiAttempt::receive(18, "openrouter", true, false, true, false);
        assert_eq!(
            ordinary.preflight().unwrap_err().category,
            "network_unavailable"
        );

        let mut authorization = CoreDevSmokeAuthorization::new();
        assert!(authorization.arm_authenticated());
        let transport = authorization
            .consume_fixed_prompt(CORE_DEV_AGENT_SMOKE_PROMPT)
            .unwrap();
        let (mut attempt, _) = AgentUiAttempt::receive_core_dev_fixed_smoke(
            19,
            "openrouter",
            true,
            false,
            CORE_DEV_AGENT_SMOKE_PROMPT,
            transport,
        )
        .unwrap();
        assert_eq!(attempt.transport(), AgentUiTransport::CoreDevFixedTunnel);
        assert!(attempt.uses_core_dev_fixed_tunnel());
        assert_eq!(attempt.preflight(), Ok(()));
        assert!(attempt.transport_ready(false));
        assert!(attempt.prompt_matches_transport(CORE_DEV_AGENT_SMOKE_PROMPT));
        assert_eq!(
            attempt.dispatch_started().unwrap().kind,
            AgentUiAttemptEventKind::DispatchStarted
        );

        assert!(authorization
            .consume_fixed_prompt(CORE_DEV_AGENT_SMOKE_PROMPT)
            .is_none());
    }

    #[test]
    fn fixed_tunnel_authorization_is_single_use_fail_closed_and_credential_preserving() {
        let mut authorization = CoreDevSmokeAuthorization::new();
        assert!(authorization
            .consume_fixed_prompt(CORE_DEV_AGENT_SMOKE_PROMPT)
            .is_none());
        assert!(authorization.arm_authenticated());
        assert!(!authorization.arm_authenticated());
        assert!(authorization
            .consume_fixed_prompt("forged prompt")
            .is_none());
        assert!(!authorization.is_armed());
        assert!(authorization
            .consume_fixed_prompt(CORE_DEV_AGENT_SMOKE_PROMPT)
            .is_none());

        assert!(authorization.arm_authenticated());
        let transport = authorization
            .consume_fixed_prompt(CORE_DEV_AGENT_SMOKE_PROMPT)
            .unwrap();
        assert!(!authorization.is_armed());
        let (missing_credential, _) = AgentUiAttempt::receive_core_dev_fixed_smoke(
            20,
            "openrouter",
            false,
            false,
            CORE_DEV_AGENT_SMOKE_PROMPT,
            transport,
        )
        .unwrap();
        assert_eq!(
            missing_credential.preflight().unwrap_err().category,
            "credential_missing"
        );
        assert!(AgentUiAttempt::receive_core_dev_fixed_smoke(
            21,
            "openai",
            true,
            false,
            CORE_DEV_AGENT_SMOKE_PROMPT,
            AgentUiTransport::CoreDevFixedTunnel,
        )
        .is_err());
        assert!(AgentUiAttempt::receive_core_dev_fixed_smoke(
            22,
            "openrouter",
            true,
            false,
            "forged prompt",
            AgentUiTransport::CoreDevFixedTunnel,
        )
        .is_err());
    }

    #[test]
    fn fixed_tunnel_state_is_consumed_before_every_exit_and_terminal_is_ordered_once() {
        for (attempt_id, configured, busy, expected_category) in [
            (30, false, false, "credential_missing"),
            (31, true, true, "busy"),
        ] {
            let mut authorization = CoreDevSmokeAuthorization::new();
            assert!(authorization.arm_authenticated());
            let transport = authorization
                .consume_fixed_prompt(CORE_DEV_AGENT_SMOKE_PROMPT)
                .unwrap();
            let (mut attempt, received) = AgentUiAttempt::receive_core_dev_fixed_smoke(
                attempt_id,
                "openrouter",
                configured,
                busy,
                CORE_DEV_AGENT_SMOKE_PROMPT,
                transport,
            )
            .unwrap();
            assert!(!authorization.is_armed());
            let failure = attempt.preflight().unwrap_err();
            assert_eq!(failure.category, expected_category);
            let terminal = attempt.terminal(Some(failure)).unwrap();
            assert_eq!(
                [received.kind, terminal.kind],
                [
                    AgentUiAttemptEventKind::Received,
                    AgentUiAttemptEventKind::Terminal
                ]
            );
            assert!(attempt.terminal(Some(failure)).is_err());
        }

        let mut authorization = CoreDevSmokeAuthorization::new();
        assert!(authorization.arm_authenticated());
        let transport = authorization
            .consume_fixed_prompt(CORE_DEV_AGENT_SMOKE_PROMPT)
            .unwrap();
        let (mut attempt, received) = AgentUiAttempt::receive_core_dev_fixed_smoke(
            32,
            "openrouter",
            true,
            false,
            CORE_DEV_AGENT_SMOKE_PROMPT,
            transport,
        )
        .unwrap();
        let dispatch = attempt.dispatch_started().unwrap();
        let accepted = attempt.accepted_marker().unwrap();
        let terminal = attempt.terminal(None).unwrap();
        assert!(!authorization.is_armed());
        assert_eq!(
            [received.kind, dispatch.kind, terminal.kind],
            [
                AgentUiAttemptEventKind::Received,
                AgentUiAttemptEventKind::DispatchStarted,
                AgentUiAttemptEventKind::Terminal,
            ]
        );
        assert!(accepted.starts_with("android_agent_request_accepted provider=openrouter"));
        assert!(attempt.terminal(None).is_err());
    }

    #[test]
    fn every_post_dispatch_error_is_terminal_and_displayed_from_the_same_mapping() {
        for (index, (stage, category)) in [
            ("dispatch", "dispatch_channel"),
            ("runtime", "runtime_start"),
            ("transport", "dns_resolution"),
            ("provider", "rate_limited"),
            ("protocol", "invalid_response"),
            ("validation", "invalid_candidate"),
        ]
        .into_iter()
        .enumerate()
        {
            let (mut attempt, _) =
                AgentUiAttempt::receive(index as u64 + 30, "openrouter", true, false, true, true);
            attempt.dispatch_started().unwrap();
            let failure = ui_failure(stage, category);
            assert!(!failure.display.is_empty());
            let terminal = attempt.terminal(Some(failure)).unwrap();
            assert_eq!(terminal.stage, failure.stage);
            assert_eq!(terminal.category, failure.category);
            let marker = terminal.request_terminal_marker().unwrap();
            assert!(marker.starts_with("android_agent_request_terminal stage="));
            assert!(marker.contains(&format!(" category={} ", failure.category)));
            assert!(attempt.terminal(Some(failure)).is_err());
        }
    }

    #[test]
    fn nonterminal_events_cannot_construct_terminal_markers() {
        let (mut attempt, received) =
            AgentUiAttempt::receive(61, "openrouter", true, false, true, true);
        assert!(received.request_terminal_marker().is_err());
        assert!(received.ui_terminal_marker().is_err());
        let started = attempt.dispatch_started().unwrap();
        assert!(started.request_terminal_marker().is_err());
        assert!(started.ui_terminal_marker().is_err());
    }

    #[test]
    fn ui_failure_mapping_cannot_display_provider_or_protocol_content() {
        let injected = ui_failure(
            "provider\ncore_ui_attempt_terminal",
            "provider-body secret prompt sk-or-v1-",
        );
        assert_eq!(injected.stage, "protocol");
        assert_eq!(injected.category, "unknown");
        assert_eq!(
            injected.display,
            "The provider failure category was unknown."
        );
    }

    #[test]
    fn development_smoke_prompt_is_fixed_bounded_and_non_secret() {
        assert!(!CORE_DEV_AGENT_SMOKE_PROMPT.trim().is_empty());
        assert!(CORE_DEV_AGENT_SMOKE_PROMPT.len() < 256);
        assert!(CORE_DEV_AGENT_SMOKE_PROMPT.contains("Blue smoke check"));
        for forbidden in ["sk-or-v1-", "credential", "Authorization", "Bearer"] {
            assert!(!CORE_DEV_AGENT_SMOKE_PROMPT.contains(forbidden));
        }
    }

    #[test]
    fn every_packaged_agent_composer_uses_the_single_trusted_submit_contract() {
        for source in [
            include_str!("../../../experiences/default.luau"),
            include_str!("../../../experiences/timeflow.luau"),
            include_str!("../../../experiences/daily-flow.luau"),
        ] {
            assert_eq!(
                source.matches("submit_action = \"agent_submit\"").count(),
                1
            );
            assert!(source.contains("event.action == \"agent_submit\""));
            assert!(source.contains("provider = \"agent\", action = \"prompt\""));
            assert!(source.contains("id = \"agent-prompt\""));
        }
    }
}
