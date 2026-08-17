use std::{thread, time::Duration};

use experience_ir::{
    ExperienceModel, ProviderEffect, ProviderRequest, ProviderResponse, SystemCapability,
};
use serde_json::{json, Value};

const FIRST_REQUEST_ID: u64 = 0x534f_5300;
const OBSERVATION_ATTEMPTS: usize = 20;
const OBSERVATION_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeStatus {
    Pass,
    Fail,
    Skip,
}

impl ProbeStatus {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::Skip => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Skip => "SKIP",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReport {
    pub status: ProbeStatus,
    pub lines: Vec<String>,
}

pub fn run_probe<F>(mode: &str, mut request: F) -> ProbeReport
where
    F: FnMut(ProviderRequest) -> Result<ProviderResponse, String>,
{
    let mut probe = Probe {
        request: &mut request,
        next_request_id: FIRST_REQUEST_ID,
    };
    match mode {
        "snapshot" => probe.snapshot_report(),
        "security" => probe.security_report(),
        "unavailable" => probe.unavailable_report(),
        "audio-restore" => probe.audio_report(),
        "wifi-restore" => probe.wifi_report(),
        _ => report(ProbeStatus::Fail, "mode", "reason=unsupported_mode".into()),
    }
}

struct Probe<'a, F> {
    request: &'a mut F,
    next_request_id: u64,
}

impl<F> Probe<'_, F>
where
    F: FnMut(ProviderRequest) -> Result<ProviderResponse, String>,
{
    fn request(
        &mut self,
        build: impl FnOnce(u64) -> ProviderRequest,
    ) -> Result<ProviderResponse, ()> {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(FIRST_REQUEST_ID);
        let response = (self.request)(build(request_id)).map_err(|_| ())?;
        (response.request_id == request_id)
            .then_some(response)
            .ok_or(())
    }

    fn snapshot(&mut self) -> Result<ExperienceModel, ()> {
        let response = self.request(|request_id| ProviderRequest::Snapshot { request_id })?;
        if !response.ok {
            return Err(());
        }
        response.model.ok_or(())
    }

    fn action(&mut self, provider: &str, action: &str, payload: Value) -> Result<bool, ()> {
        self.request(|request_id| ProviderRequest::Action {
            request_id,
            provider: provider.into(),
            action: action.into(),
            payload,
        })
        .map(|response| response.ok)
    }

    fn snapshot_report(&mut self) -> ProbeReport {
        match self.snapshot() {
            Ok(model) => report(ProbeStatus::Pass, "snapshot", snapshot_summary(&model)),
            Err(()) => report(
                ProbeStatus::Fail,
                "snapshot",
                "reason=transport_or_framing".into(),
            ),
        }
    }

    fn security_report(&mut self) -> ProbeReport {
        let state_response =
            match self.request(|request_id| ProviderRequest::LoadState { request_id }) {
                Ok(response) if response.ok => response,
                Err(()) => {
                    return report(
                        ProbeStatus::Fail,
                        "security",
                        "reason=transport_or_framing".into(),
                    )
                }
                Ok(_) => {
                    return report(
                        ProbeStatus::Fail,
                        "security",
                        "reason=load_state_rejected".into(),
                    )
                }
            };
        let Some(state) = state_response.state else {
            return report(ProbeStatus::Fail, "security", "reason=missing_state".into());
        };
        let stage = self.request(|request_id| ProviderRequest::StageState {
            request_id,
            expected_revision: state.revision,
            schema_version: state.schema_version,
            state: state.state,
            source_sha256: state.source_sha256,
            effects: vec![ProviderEffect {
                provider: "power".into(),
                action: "request_restart".into(),
                payload: json!({}),
            }],
        });
        match stage {
            Ok(response)
                if !response.ok
                    && response.error.as_deref()
                        == Some("provider capability is not granted: power.request_restart") =>
            {
                report(
                    ProbeStatus::Pass,
                    "security",
                    "injected_privileged_capability=rejected state_mutation=false".into(),
                )
            }
            Ok(response) if response.ok => {
                let cleanup_ok = response.stage_id.is_some_and(|stage_id| {
                    self.request(|request_id| ProviderRequest::AbortState {
                        request_id,
                        stage_id,
                    })
                    .is_ok_and(|response| response.ok)
                });
                report(
                    ProbeStatus::Fail,
                    "security",
                    format!(
                        "injected_privileged_capability=accepted cleanup={}",
                        if cleanup_ok { "restored" } else { "failed" }
                    ),
                )
            }
            Err(()) => report(
                ProbeStatus::Fail,
                "security",
                "reason=transport_or_framing".into(),
            ),
            Ok(_) => report(
                ProbeStatus::Fail,
                "security",
                "reason=wrong_rejection".into(),
            ),
        }
    }

    fn unavailable_report(&mut self) -> ProbeReport {
        let model = match self.snapshot() {
            Ok(model) => model,
            Err(()) => {
                return report(
                    ProbeStatus::Fail,
                    "unavailable",
                    "reason=transport_or_framing".into(),
                )
            }
        };
        let capabilities = &model.providers.capabilities;
        let candidate = [
            (
                SystemCapability::AppLaunch,
                "apps",
                "launch",
                json!({ "app_id": "probe_missing" }),
            ),
            (
                SystemCapability::AttentionAcknowledge,
                "attention",
                "acknowledge",
                json!({ "attention_id": "probe_missing" }),
            ),
            (
                SystemCapability::WifiConnect,
                "network",
                "connect",
                json!({ "network_id": "probe_missing" }),
            ),
        ]
        .into_iter()
        .find(|(capability, _, _, _)| !capabilities.contains(capability));
        let Some((_, provider, action, payload)) = candidate else {
            return report(
                ProbeStatus::Skip,
                "unavailable",
                "reason=no_safe_absent_capability".into(),
            );
        };
        let expected = format!("provider capability is not granted: {provider}.{action}");
        match self.request(|request_id| ProviderRequest::Action {
            request_id,
            provider: provider.into(),
            action: action.into(),
            payload,
        }) {
            Ok(response) if !response.ok && response.error.as_deref() == Some(&expected) => report(
                ProbeStatus::Pass,
                "unavailable",
                format!("action={provider}.{action} semantics=explicit_rejection"),
            ),
            Ok(response) if response.ok => report(
                ProbeStatus::Fail,
                "unavailable",
                format!("action={provider}.{action} reason=unexpected_success"),
            ),
            Err(()) => report(
                ProbeStatus::Fail,
                "unavailable",
                format!("action={provider}.{action} reason=transport_or_framing"),
            ),
            Ok(_) => report(
                ProbeStatus::Fail,
                "unavailable",
                format!("action={provider}.{action} reason=wrong_rejection"),
            ),
        }
    }

    fn audio_report(&mut self) -> ProbeReport {
        let initial = match self.snapshot() {
            Ok(model) => model,
            Err(()) => return report(ProbeStatus::Fail, "audio", "reason=snapshot".into()),
        };
        let mut lines = Vec::new();
        let mut attempted = false;

        if initial
            .providers
            .capabilities
            .contains(&SystemCapability::AudioSetVolume)
        {
            if let Some(original) = initial.providers.audio.volume_percent {
                attempted = true;
                let changed = if original > 50 {
                    original.saturating_sub(10)
                } else {
                    original.saturating_add(10).min(100)
                };
                let applied = self
                    .action("audio", "set_volume", json!({ "percent": changed }))
                    .is_ok_and(|ok| ok);
                let observed = self.snapshot().is_ok_and(|model| {
                    model.providers.audio.volume_percent.is_some()
                        && model.providers.audio.volume_percent != Some(original)
                });
                let restore_sent = self
                    .action("audio", "set_volume", json!({ "percent": original }))
                    .is_ok_and(|ok| ok);
                let restored = self
                    .snapshot()
                    .is_ok_and(|model| model.providers.audio.volume_percent == Some(original));
                let status = applied && observed && restore_sent && restored;
                lines.push(case_line(
                    "audio_volume",
                    if status {
                        ProbeStatus::Pass
                    } else {
                        ProbeStatus::Fail
                    },
                    format!(
                        "before=captured changed={} restored={restored}",
                        applied && observed
                    ),
                ));
                if !status {
                    return ProbeReport {
                        status: ProbeStatus::Fail,
                        lines,
                    };
                }
            }
        }

        let current = match self.snapshot() {
            Ok(model) => model,
            Err(()) => {
                lines.push(case_line(
                    "audio_mute",
                    ProbeStatus::Fail,
                    "reason=snapshot".into(),
                ));
                return ProbeReport {
                    status: ProbeStatus::Fail,
                    lines,
                };
            }
        };
        if current
            .providers
            .capabilities
            .contains(&SystemCapability::AudioSetMuted)
        {
            if let Some(original) = current.providers.audio.muted {
                attempted = true;
                let changed = !original;
                let applied = self
                    .action("audio", "set_muted", json!({ "muted": changed }))
                    .is_ok_and(|ok| ok);
                let observed = self
                    .snapshot()
                    .is_ok_and(|model| model.providers.audio.muted == Some(changed));
                let restore_sent = self
                    .action("audio", "set_muted", json!({ "muted": original }))
                    .is_ok_and(|ok| ok);
                let restored = self
                    .snapshot()
                    .is_ok_and(|model| model.providers.audio.muted == Some(original));
                let status = applied && observed && restore_sent && restored;
                lines.push(case_line(
                    "audio_mute",
                    if status {
                        ProbeStatus::Pass
                    } else {
                        ProbeStatus::Fail
                    },
                    format!(
                        "before=captured changed={} restored={restored}",
                        applied && observed
                    ),
                ));
                if !status {
                    return ProbeReport {
                        status: ProbeStatus::Fail,
                        lines,
                    };
                }
            }
        }

        if attempted {
            ProbeReport {
                status: ProbeStatus::Pass,
                lines,
            }
        } else {
            report(
                ProbeStatus::Skip,
                "audio",
                "reason=capability_or_state_unavailable".into(),
            )
        }
    }

    fn wifi_report(&mut self) -> ProbeReport {
        let initial = match self.snapshot() {
            Ok(model) => model,
            Err(()) => return report(ProbeStatus::Fail, "wifi", "reason=snapshot".into()),
        };
        if initial
            .providers
            .connectivity
            .wifi_networks
            .iter()
            .any(|network| network.connected)
        {
            self.wifi_restore_connected(initial)
        } else {
            self.wifi_restore_disconnected(initial)
        }
    }

    fn wifi_restore_connected(&mut self, initial: ExperienceModel) -> ProbeReport {
        let connectivity = &initial.providers.connectivity;
        let Some(original) = connectivity
            .wifi_networks
            .iter()
            .find(|network| network.saved && network.connected)
            .map(|network| network.id.clone())
        else {
            return report(
                ProbeStatus::Skip,
                "wifi",
                "reason=no_restorable_connected_network".into(),
            );
        };
        if !initial
            .providers
            .capabilities
            .contains(&SystemCapability::WifiDisconnect)
        {
            return report(
                ProbeStatus::Skip,
                "wifi",
                "reason=disconnect_unavailable".into(),
            );
        }

        let disconnected_request = self
            .action("network", "disconnect", json!({}))
            .is_ok_and(|ok| ok);
        let disconnected = self.wait_for_snapshot(|model| {
            model
                .providers
                .connectivity
                .wifi_networks
                .iter()
                .all(|network| !network.connected)
        });
        let restore_id = original.clone();
        let restore_sent = self
            .action("network", "connect", json!({ "network_id": original }))
            .is_ok_and(|ok| ok);
        let restored = self.wait_for_snapshot(|model| {
            model
                .providers
                .connectivity
                .wifi_networks
                .iter()
                .any(|network| network.id == restore_id && network.saved && network.connected)
        });
        let passed = disconnected_request && disconnected && restore_sent && restored;
        report(
            if passed {
                ProbeStatus::Pass
            } else {
                ProbeStatus::Fail
            },
            "wifi",
            format!("before=connected changed={disconnected} restored={restored}"),
        )
    }

    fn wifi_restore_disconnected(&mut self, initial: ExperienceModel) -> ProbeReport {
        let Some(candidate) = initial
            .providers
            .connectivity
            .wifi_networks
            .iter()
            .find(|network| network.saved)
            .map(|network| network.id.clone())
        else {
            return report(ProbeStatus::Skip, "wifi", "reason=no_saved_network".into());
        };
        if !initial
            .providers
            .capabilities
            .contains(&SystemCapability::WifiConnect)
        {
            return report(
                ProbeStatus::Skip,
                "wifi",
                "reason=connect_unavailable".into(),
            );
        }

        let expected_id = candidate.clone();
        let connected_request = self
            .action("network", "connect", json!({ "network_id": candidate }))
            .is_ok_and(|ok| ok);
        let connected = self.wait_for_snapshot(|model| {
            model
                .providers
                .connectivity
                .wifi_networks
                .iter()
                .any(|network| network.id == expected_id && network.connected)
        });
        let restore_sent = self
            .action("network", "disconnect", json!({}))
            .is_ok_and(|ok| ok);
        let restored = self.wait_for_snapshot(|model| {
            model
                .providers
                .connectivity
                .wifi_networks
                .iter()
                .all(|network| !network.connected)
        });
        let passed = connected_request && connected && restore_sent && restored;
        report(
            if passed {
                ProbeStatus::Pass
            } else {
                ProbeStatus::Fail
            },
            "wifi",
            format!("before=disconnected changed={connected} restored={restored}"),
        )
    }

    fn wait_for_snapshot(&mut self, predicate: impl Fn(&ExperienceModel) -> bool) -> bool {
        for attempt in 0..OBSERVATION_ATTEMPTS {
            if self.snapshot().is_ok_and(|model| predicate(&model)) {
                return true;
            }
            if attempt + 1 < OBSERVATION_ATTEMPTS {
                thread::sleep(OBSERVATION_INTERVAL);
            }
        }
        false
    }
}

fn report(status: ProbeStatus, case: &str, detail: String) -> ProbeReport {
    ProbeReport {
        status,
        lines: vec![case_line(case, status, detail)],
    }
}

fn case_line(case: &str, status: ProbeStatus, detail: String) -> String {
    format!(
        "core_provider_probe case={case} status={} {detail}",
        status.label()
    )
}

fn snapshot_summary(model: &ExperienceModel) -> String {
    let providers = &model.providers;
    let connectivity = &providers.connectivity;
    format!(
        "abi={} battery={} charging={} temperature={} thermal={} audio_volume={} audio_muted={} media_active={} media_playing={} wifi_enabled={} link_connected={} link_validated={} link_transport={} interfaces={} supplicant_saved_networks={} apps={} attention={} urgent={} capabilities={}",
        providers.abi_version,
        presence(providers.power.battery_percent),
        presence(providers.power.charging),
        presence(providers.power.battery_temperature_deci_c),
        presence(providers.power.thermal_status),
        presence(providers.audio.volume_percent),
        presence(providers.audio.muted),
        providers.audio.media.active,
        providers.audio.media.playing,
        connectivity.wifi_enabled,
        connectivity.connected,
        connectivity.validated,
        transport_class(&connectivity.transport),
        connectivity.online_interfaces.len(),
        connectivity.wifi_networks.len(),
        providers.apps.compatible.len(),
        providers.attention.items.len(),
        providers.attention.urgent_count,
        capability_list(&providers.capabilities),
    )
}

fn presence<T>(value: Option<T>) -> &'static str {
    if value.is_some() {
        "present"
    } else {
        "absent"
    }
}

fn transport_class(value: &str) -> &'static str {
    match value {
        "" => "none",
        "wifi" => "wifi",
        "network" => "network",
        _ => "other",
    }
}

fn capability_list(capabilities: &[SystemCapability]) -> String {
    let mut names: Vec<_> = capabilities.iter().map(capability_name).collect();
    names.sort_unstable();
    names.dedup();
    if names.is_empty() {
        "none".into()
    } else {
        names.join(",")
    }
}

fn capability_name(capability: &SystemCapability) -> &'static str {
    match capability {
        SystemCapability::AudioSetVolume => "audio_set_volume",
        SystemCapability::AudioSetMuted => "audio_set_muted",
        SystemCapability::MediaPlayPause => "media_play_pause",
        SystemCapability::MediaNext => "media_next",
        SystemCapability::MediaPrevious => "media_previous",
        SystemCapability::WifiConnect => "wifi_connect",
        SystemCapability::WifiDisconnect => "wifi_disconnect",
        SystemCapability::AppLaunch => "app_launch",
        SystemCapability::AttentionAcknowledge => "attention_acknowledge",
        SystemCapability::RequestLock => "request_lock",
        SystemCapability::RequestRestart => "request_restart",
        SystemCapability::RequestShutdown => "request_shutdown",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use experience_ir::{ProviderResponse, StateEnvelope, SystemApplication, SystemWifiNetwork};
    use serde_json::json;

    use super::*;

    fn response(request_id: u64, ok: bool) -> ProviderResponse {
        ProviderResponse {
            request_id,
            ok,
            model: None,
            result: None,
            state: None,
            stage_id: None,
            error: None,
        }
    }

    fn snapshot_response(request_id: u64, model: ExperienceModel) -> ProviderResponse {
        ProviderResponse {
            model: Some(model),
            ..response(request_id, true)
        }
    }

    #[test]
    fn snapshot_is_redacted_and_deterministic() {
        let mut model = providers_fake::snapshot();
        model.providers.connectivity.network_label = "SECRET_WIFI".into();
        model.providers.connectivity.online_interfaces = vec!["wlan-secret".into()];
        model.providers.connectivity.wifi_networks = vec![SystemWifiNetwork {
            id: "secret-network-id".into(),
            label: "SECRET_WIFI".into(),
            signal_level: 3,
            saved: true,
            connected: true,
        }];
        model.providers.apps.compatible = vec![SystemApplication {
            id: "secret-app-id".into(),
            label: "Secret App".into(),
        }];
        let report = run_probe("snapshot", |request| {
            Ok(snapshot_response(request.request_id(), model.clone()))
        });
        assert_eq!(report.status, ProbeStatus::Pass);
        let output = report.lines.join("\n");
        assert!(!output.contains("SECRET_WIFI"));
        assert!(!output.contains("secret-network-id"));
        assert!(!output.contains("secret-app-id"));
        assert!(output.contains("supplicant_saved_networks=1 apps=1"));
    }

    #[test]
    fn privileged_effect_is_rejected_before_staging() {
        let mut requests = 0;
        let report = run_probe("security", |request| {
            requests += 1;
            match request {
                ProviderRequest::LoadState { request_id } => Ok(ProviderResponse {
                    state: Some(StateEnvelope {
                        revision: 7,
                        schema_version: 1,
                        source_sha256: "source".into(),
                        state: json!({ "safe": true }),
                    }),
                    ..response(request_id, true)
                }),
                ProviderRequest::StageState {
                    request_id,
                    effects,
                    ..
                } => {
                    assert_eq!(effects.len(), 1);
                    assert_eq!(effects[0].provider, "power");
                    assert_eq!(effects[0].action, "request_restart");
                    Ok(ProviderResponse {
                        error: Some(
                            "provider capability is not granted: power.request_restart".into(),
                        ),
                        ..response(request_id, false)
                    })
                }
                _ => panic!("unexpected request"),
            }
        });
        assert_eq!(report.status, ProbeStatus::Pass);
        assert_eq!(requests, 2);
    }

    #[test]
    fn accidentally_accepted_privileged_stage_is_aborted() {
        let mut aborted = false;
        let report = run_probe("security", |request| match request {
            ProviderRequest::LoadState { request_id } => Ok(ProviderResponse {
                state: Some(StateEnvelope {
                    revision: 7,
                    schema_version: 1,
                    source_sha256: "source".into(),
                    state: json!({ "safe": true }),
                }),
                ..response(request_id, true)
            }),
            ProviderRequest::StageState { request_id, .. } => Ok(ProviderResponse {
                stage_id: Some(99),
                ..response(request_id, true)
            }),
            ProviderRequest::AbortState {
                request_id,
                stage_id,
            } => {
                assert_eq!(stage_id, 99);
                aborted = true;
                Ok(response(request_id, true))
            }
            _ => panic!("unexpected request"),
        });
        assert_eq!(report.status, ProbeStatus::Fail);
        assert!(aborted);
        assert!(report.lines[0].contains("cleanup=restored"));
    }

    #[test]
    fn unavailable_action_requires_explicit_capability_rejection() {
        let model = providers_fake::snapshot();
        let report = run_probe("unavailable", |request| match request {
            ProviderRequest::Snapshot { request_id } => {
                Ok(snapshot_response(request_id, model.clone()))
            }
            ProviderRequest::Action {
                request_id,
                provider,
                action,
                ..
            } => {
                assert_eq!((provider.as_str(), action.as_str()), ("apps", "launch"));
                Ok(ProviderResponse {
                    error: Some("provider capability is not granted: apps.launch".into()),
                    ..response(request_id, false)
                })
            }
            _ => panic!("unexpected request"),
        });
        assert_eq!(report.status, ProbeStatus::Pass);
    }

    #[test]
    fn audio_actions_restore_original_state() {
        let mut initial = providers_fake::snapshot();
        initial.providers.audio.volume_percent = Some(50);
        initial.providers.audio.muted = Some(false);
        initial.providers.capabilities = vec![
            SystemCapability::AudioSetVolume,
            SystemCapability::AudioSetMuted,
        ];
        let mut volume_changed = initial.clone();
        volume_changed.providers.audio.volume_percent = Some(60);
        let mut mute_changed = initial.clone();
        mute_changed.providers.audio.muted = Some(true);
        let mut snapshots = VecDeque::from([
            initial.clone(),
            volume_changed,
            initial.clone(),
            initial.clone(),
            mute_changed,
            initial,
        ]);
        let mut actions = Vec::new();
        let report = run_probe("audio-restore", |request| match request {
            ProviderRequest::Snapshot { request_id } => Ok(snapshot_response(
                request_id,
                snapshots.pop_front().expect("snapshot fixture"),
            )),
            ProviderRequest::Action {
                request_id,
                action,
                payload,
                ..
            } => {
                actions.push((action, payload));
                Ok(response(request_id, true))
            }
            _ => panic!("unexpected request"),
        });
        assert_eq!(report.status, ProbeStatus::Pass);
        assert!(snapshots.is_empty());
        assert!(report
            .lines
            .iter()
            .all(|line| line.contains("restored=true")));
        assert_eq!(
            actions,
            vec![
                ("set_volume".into(), json!({ "percent": 60 })),
                ("set_volume".into(), json!({ "percent": 50 })),
                ("set_muted".into(), json!({ "muted": true })),
                ("set_muted".into(), json!({ "muted": false })),
            ]
        );
    }

    #[test]
    fn wifi_disconnect_reconnect_restores_without_printing_identifiers() {
        let mut connected = providers_fake::snapshot();
        connected.providers.connectivity.connected = true;
        connected.providers.connectivity.wifi_networks = vec![SystemWifiNetwork {
            id: "secret-network-id".into(),
            label: "SECRET_WIFI".into(),
            signal_level: 3,
            saved: true,
            connected: true,
        }];
        connected.providers.capabilities = vec![SystemCapability::WifiDisconnect];
        let mut disconnected = connected.clone();
        disconnected.providers.connectivity.connected = false;
        disconnected.providers.connectivity.wifi_networks[0].connected = false;
        disconnected.providers.capabilities = vec![SystemCapability::WifiConnect];
        let mut snapshots = VecDeque::from([connected.clone(), disconnected, connected]);
        let mut actions = Vec::new();
        let report = run_probe("wifi-restore", |request| match request {
            ProviderRequest::Snapshot { request_id } => Ok(snapshot_response(
                request_id,
                snapshots.pop_front().expect("snapshot fixture"),
            )),
            ProviderRequest::Action {
                request_id,
                action,
                payload,
                ..
            } => {
                actions.push((action, payload));
                Ok(response(request_id, true))
            }
            _ => panic!("unexpected request"),
        });
        assert_eq!(report.status, ProbeStatus::Pass);
        let output = report.lines.join("\n");
        assert!(!output.contains("secret-network-id"));
        assert!(!output.contains("SECRET_WIFI"));
        assert!(output.contains("restored=true"));
        assert_eq!(
            actions,
            vec![
                ("disconnect".into(), json!({})),
                (
                    "connect".into(),
                    json!({ "network_id": "secret-network-id" }),
                ),
            ]
        );
    }
}
