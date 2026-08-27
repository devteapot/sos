use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use chrono::Local;
use experience_ir::{
    AppsProviderState, AudioProviderState, ClockProviderState, ConnectivityProviderState,
    MediaProviderState, PowerProviderState, ProviderEffect, SystemApplication, SystemCapability,
    SystemProviders, SystemWifiNetwork, ThermalStatus, SYSTEM_PROVIDER_ABI_VERSION,
};
use sha2::{Digest, Sha256};
use zbus::{
    blocking::{fdo::DBusProxy, Connection, Proxy},
    zvariant::{OwnedObjectPath, OwnedValue},
};

use super::{require, Capability, ProviderContext, ProviderError};

const NETWORK_MANAGER_DESTINATION: &str = "org.freedesktop.NetworkManager";
const NETWORK_MANAGER_PATH: &str = "/org/freedesktop/NetworkManager";
const NETWORK_MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager";
const NETWORK_MANAGER_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NETWORK_MANAGER_SETTINGS_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings";
const NETWORK_MANAGER_CONNECTION_INTERFACE: &str =
    "org.freedesktop.NetworkManager.Settings.Connection";
const NETWORK_MANAGER_ACTIVE_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const NETWORK_MANAGER_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
const NETWORK_MANAGER_WIFI_DEVICE_INTERFACE: &str =
    "org.freedesktop.NetworkManager.Device.Wireless";
const NETWORK_MANAGER_ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const UPOWER_DESTINATION: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_INTERFACE: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
const TIMEDATE_DESTINATION: &str = "org.freedesktop.timedate1";
const TIMEDATE_PATH: &str = "/org/freedesktop/timedate1";
const TIMEDATE_INTERFACE: &str = "org.freedesktop.timedate1";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Clone)]
pub(super) struct SystemAdapter {
    system_bus: Option<Connection>,
    session_bus: Option<Connection>,
    applications: Arc<Vec<ApplicationSelection>>,
}

impl std::fmt::Debug for SystemAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SystemAdapter")
            .field("system_bus", &self.system_bus.is_some())
            .field("session_bus", &self.session_bus.is_some())
            .field("applications", &self.applications.len())
            .finish()
    }
}

#[derive(Clone, Debug)]
struct NetworkSelection {
    id: String,
    label: String,
    connection_path: OwnedObjectPath,
    connected: bool,
    signal_level: u8,
}

#[derive(Clone, Debug, Default)]
struct NetworkInventory {
    state: ConnectivityProviderState,
    selections: Vec<NetworkSelection>,
    active_connections: Vec<OwnedObjectPath>,
    wifi_device: Option<OwnedObjectPath>,
}

#[derive(Clone, Debug)]
struct MediaPlayer {
    bus_name: String,
    state: MediaProviderState,
    can_play_pause: bool,
    can_next: bool,
    can_previous: bool,
}

#[derive(Clone, Debug)]
struct ApplicationSelection {
    id: String,
    label: String,
    desktop_file: PathBuf,
}

pub(super) fn is_system_effect(effect: &ProviderEffect) -> bool {
    matches!(
        effect.provider.as_str(),
        "audio" | "media" | "network" | "apps" | "attention" | "power"
    )
}

impl SystemAdapter {
    pub(super) fn connect() -> Self {
        Self {
            system_bus: Connection::system().ok(),
            session_bus: Connection::session().ok(),
            applications: Arc::new(application_inventory()),
        }
    }

    pub(super) fn snapshot(
        &self,
        context: &ProviderContext,
    ) -> Result<SystemProviders, ProviderError> {
        context.cancellation.check()?;
        require(context, Capability::SystemRead)?;

        let network = self.network_inventory();
        let (volume_percent, muted) = super::read_audio_state();
        let media = self.media_player();
        let applications = &self.applications;
        let mut capabilities = Vec::new();
        if context.grants.contains(&Capability::AudioControl) && volume_percent.is_some() {
            capabilities.extend([
                SystemCapability::AudioSetVolume,
                SystemCapability::AudioSetMuted,
            ]);
        }
        if context.grants.contains(&Capability::MusicControl) {
            if media.as_ref().is_some_and(|player| player.can_play_pause) {
                capabilities.push(SystemCapability::MediaPlayPause);
            }
            if media.as_ref().is_some_and(|player| player.can_next) {
                capabilities.push(SystemCapability::MediaNext);
            }
            if media.as_ref().is_some_and(|player| player.can_previous) {
                capabilities.push(SystemCapability::MediaPrevious);
            }
        }
        if context.grants.contains(&Capability::NetworkControl) {
            if !network.selections.is_empty() && network.wifi_device.is_some() {
                capabilities.push(SystemCapability::WifiConnect);
            }
            if !network.active_connections.is_empty() {
                capabilities.push(SystemCapability::WifiDisconnect);
            }
        }
        if context.grants.contains(&Capability::ApplicationLaunch)
            && !applications.is_empty()
            && Path::new("/usr/bin/gio").is_file()
        {
            capabilities.push(SystemCapability::AppLaunch);
        }

        let now = Local::now();
        let unix_time_ms = now.timestamp_millis().max(0) as u64;
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
            .unwrap_or_else(|| "C".into());
        let power = self.power_state();
        let audio = AudioProviderState {
            volume_percent,
            muted,
            media: media.map_or_else(MediaProviderState::default, |player| player.state),
        };
        let apps = AppsProviderState {
            compatible: applications
                .iter()
                .map(|application| SystemApplication {
                    id: application.id.clone(),
                    label: application.label.clone(),
                })
                .collect(),
            status_widgets: Vec::new(),
        };

        Ok(SystemProviders {
            abi_version: SYSTEM_PROVIDER_ABI_VERSION,
            observed_at_ms: unix_time_ms,
            clock: ClockProviderState {
                unix_time_ms,
                locale: bounded_text(&normalize_locale(&locale), 64),
                timezone: self.timezone(),
                time_label: now.format("%H:%M").to_string(),
                date_label: now.format("%A, %B %-d").to_string(),
            },
            power,
            connectivity: network.state,
            audio,
            apps,
            attention: Default::default(),
            capabilities,
        })
    }

    pub(super) fn fingerprint(&self) -> Result<SystemProviders, ProviderError> {
        let mut snapshot = self.snapshot(&super::prototype_grants("system-fingerprint"))?;
        snapshot.observed_at_ms = 0;
        snapshot.clock.unix_time_ms /= 60_000;
        Ok(snapshot)
    }

    pub(super) fn execute(
        &self,
        context: &ProviderContext,
        effect: &ProviderEffect,
    ) -> Result<(), ProviderError> {
        context.cancellation.check()?;
        match (effect.provider.as_str(), effect.action.as_str()) {
            ("audio", "set_volume") => {
                require(context, Capability::AudioControl)?;
                let percent = effect
                    .payload
                    .get("percent")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|value| *value <= 100)
                    .ok_or_else(|| {
                        ProviderError::Unavailable(
                            "audio.set_volume requires an integer percent in 0..100".into(),
                        )
                    })?;
                run_command(
                    "/usr/bin/wpctl",
                    &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{percent}%")],
                )
            }
            ("audio", "adjust_volume") => {
                require(context, Capability::AudioControl)?;
                let delta = effect
                    .payload
                    .get("delta")
                    .and_then(serde_json::Value::as_i64)
                    .filter(|value| (-100..=100).contains(value) && *value != 0)
                    .ok_or_else(|| {
                        ProviderError::Unavailable(
                            "audio.adjust_volume requires a non-zero integer delta in -100..100"
                                .into(),
                        )
                    })?;
                let adjustment = if delta > 0 {
                    format!("{delta}%+")
                } else {
                    format!("{}%-", delta.unsigned_abs())
                };
                run_command(
                    "/usr/bin/wpctl",
                    &["set-volume", "@DEFAULT_AUDIO_SINK@", &adjustment],
                )
            }
            ("audio", "set_muted") => {
                require(context, Capability::AudioControl)?;
                let muted = effect
                    .payload
                    .get("muted")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| {
                        ProviderError::Unavailable(
                            "audio.set_muted requires a boolean muted value".into(),
                        )
                    })?;
                run_command(
                    "/usr/bin/wpctl",
                    &[
                        "set-mute",
                        "@DEFAULT_AUDIO_SINK@",
                        if muted { "1" } else { "0" },
                    ],
                )
            }
            ("media", "play_pause") => {
                require(context, Capability::MusicControl)?;
                self.media_command("PlayPause")
            }
            ("media", "next") => {
                require(context, Capability::MusicControl)?;
                self.media_command("Next")
            }
            ("media", "previous") => {
                require(context, Capability::MusicControl)?;
                self.media_command("Previous")
            }
            ("network", "connect") => {
                require(context, Capability::NetworkControl)?;
                let network_id = required_opaque_id(effect, "network_id", "network-")?;
                self.connect_network(network_id)
            }
            ("network", "disconnect") => {
                require(context, Capability::NetworkControl)?;
                self.disconnect_network()
            }
            ("apps", "launch") => {
                require(context, Capability::ApplicationLaunch)?;
                let app_id = required_opaque_id(effect, "app_id", "app-")?;
                self.launch_application(app_id)
            }
            ("attention", "acknowledge") => Err(ProviderError::Unavailable(
                "Linux notification attention collection is not active".into(),
            )),
            ("power", action) => Err(ProviderError::Unavailable(format!(
                "power.{action} requires a trusted native ceremony"
            ))),
            (provider, action) => Err(ProviderError::Unavailable(format!(
                "unsupported Linux system provider effect: {provider}.{action}"
            ))),
        }
    }

    fn power_state(&self) -> PowerProviderState {
        let fallback_percent = super::read_battery_capacity(Path::new("/sys/class/power_supply"));
        let fallback_ac = super::read_ac_online(Path::new("/sys/class/power_supply"));
        let Some(connection) = self.system_bus.as_ref() else {
            return fallback_power(fallback_percent, fallback_ac);
        };
        let Ok(upower) = Proxy::new(
            connection,
            UPOWER_DESTINATION,
            UPOWER_PATH,
            UPOWER_INTERFACE,
        ) else {
            return fallback_power(fallback_percent, fallback_ac);
        };
        let Ok(display_path) = upower.call::<_, _, OwnedObjectPath>("GetDisplayDevice", &()) else {
            return fallback_power(fallback_percent, fallback_ac);
        };
        let Ok(device) = Proxy::new(
            connection,
            UPOWER_DESTINATION,
            display_path.as_str(),
            UPOWER_DEVICE_INTERFACE,
        ) else {
            return fallback_power(fallback_percent, fallback_ac);
        };
        let percentage = device
            .get_property::<f64>("Percentage")
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| value.round().clamp(0.0, 100.0) as u8)
            .or(fallback_percent);
        let state = device.get_property::<u32>("State").unwrap_or_default();
        let charging = percentage.and(match state {
            1 => Some(true),
            2..=6 => Some(false),
            _ => fallback_ac,
        });
        let temperature = device
            .get_property::<f64>("Temperature")
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| (value * 10.0).round() as i32);
        PowerProviderState {
            battery_percent: percentage,
            charging,
            charging_source: if charging == Some(true) || fallback_ac == Some(true) {
                "ac".into()
            } else {
                "battery".into()
            },
            battery_temperature_deci_c: temperature,
            thermal_status: thermal_status(),
        }
    }

    fn timezone(&self) -> String {
        if let Some(timezone) = env::var("TZ").ok().filter(|value| !value.is_empty()) {
            return bounded_text(&timezone, 128);
        }
        if let Some(connection) = self.system_bus.as_ref() {
            if let Ok(proxy) = Proxy::new(
                connection,
                TIMEDATE_DESTINATION,
                TIMEDATE_PATH,
                TIMEDATE_INTERFACE,
            ) {
                if let Ok(timezone) = proxy.get_property::<String>("Timezone") {
                    if !timezone.is_empty() {
                        return bounded_text(&timezone, 128);
                    }
                }
            }
        }
        if let Ok(target) = fs::read_link("/etc/localtime") {
            if let Some(timezone) = target
                .to_string_lossy()
                .split("/zoneinfo/")
                .nth(1)
                .filter(|value| !value.is_empty())
            {
                return bounded_text(timezone, 128);
            }
        }
        bounded_text(&Local::now().format("%Z").to_string(), 128)
    }

    fn network_inventory(&self) -> NetworkInventory {
        let online_interfaces = super::online_interfaces(Path::new("/sys/class/net"));
        let fallback_connected = !online_interfaces.is_empty();
        let mut inventory = NetworkInventory {
            state: ConnectivityProviderState {
                wifi_enabled: false,
                connected: fallback_connected,
                validated: fallback_connected,
                transport: if online_interfaces.iter().any(|name| name.starts_with("wl")) {
                    "wifi"
                } else {
                    "ethernet"
                }
                .into(),
                network_label: String::new(),
                signal_level: None,
                online_interfaces,
                wifi_networks: Vec::new(),
            },
            ..Default::default()
        };
        let Some(connection) = self.system_bus.as_ref() else {
            return inventory;
        };
        let Ok(manager) = Proxy::new(
            connection,
            NETWORK_MANAGER_DESTINATION,
            NETWORK_MANAGER_PATH,
            NETWORK_MANAGER_INTERFACE,
        ) else {
            return inventory;
        };
        inventory.state.wifi_enabled = manager
            .get_property::<bool>("WirelessEnabled")
            .unwrap_or(false);
        let manager_state = manager.get_property::<u32>("State").unwrap_or_default();
        let connectivity = manager
            .get_property::<u32>("Connectivity")
            .unwrap_or_default();
        inventory.state.connected = manager_state >= 50;
        inventory.state.validated = connectivity == 4;

        let active_paths = manager
            .get_property::<Vec<OwnedObjectPath>>("ActiveConnections")
            .unwrap_or_default();
        let mut active_connection_paths = BTreeSet::new();
        let mut active_wifi_paths = Vec::new();
        for active_path in &active_paths {
            let Ok(active) = Proxy::new(
                connection,
                NETWORK_MANAGER_DESTINATION,
                active_path.as_str(),
                NETWORK_MANAGER_ACTIVE_INTERFACE,
            ) else {
                continue;
            };
            let id = bounded_text(
                &active.get_property::<String>("Id").unwrap_or_default(),
                128,
            );
            let kind = active.get_property::<String>("Type").unwrap_or_default();
            if kind == "802-11-wireless" {
                active_wifi_paths.push(active_path.clone());
            }
            if inventory.state.network_label.is_empty() && !id.is_empty() {
                inventory.state.network_label = id;
                inventory.state.transport = if kind == "802-11-wireless" {
                    "wifi".into()
                } else {
                    "ethernet".into()
                };
            }
            if let Ok(path) = active.get_property::<OwnedObjectPath>("Connection") {
                active_connection_paths.insert(path.to_string());
            }
        }
        inventory.active_connections = active_wifi_paths;

        let devices = manager
            .call::<_, _, Vec<OwnedObjectPath>>("GetDevices", &())
            .unwrap_or_default();
        for device_path in devices {
            let Ok(device) = Proxy::new(
                connection,
                NETWORK_MANAGER_DESTINATION,
                device_path.as_str(),
                NETWORK_MANAGER_DEVICE_INTERFACE,
            ) else {
                continue;
            };
            if device.get_property::<u32>("DeviceType").unwrap_or_default() != 2 {
                continue;
            }
            inventory.wifi_device = Some(device_path.clone());
            let Ok(wireless) = Proxy::new(
                connection,
                NETWORK_MANAGER_DESTINATION,
                device_path.as_str(),
                NETWORK_MANAGER_WIFI_DEVICE_INTERFACE,
            ) else {
                continue;
            };
            let Ok(access_point_path) =
                wireless.get_property::<OwnedObjectPath>("ActiveAccessPoint")
            else {
                continue;
            };
            if access_point_path.as_str() == "/" {
                continue;
            }
            if let Ok(access_point) = Proxy::new(
                connection,
                NETWORK_MANAGER_DESTINATION,
                access_point_path.as_str(),
                NETWORK_MANAGER_ACCESS_POINT_INTERFACE,
            ) {
                let strength = access_point
                    .get_property::<u8>("Strength")
                    .unwrap_or_default();
                inventory.state.signal_level = Some(signal_level(strength));
            };
        }

        let Ok(settings) = Proxy::new(
            connection,
            NETWORK_MANAGER_DESTINATION,
            NETWORK_MANAGER_SETTINGS_PATH,
            NETWORK_MANAGER_SETTINGS_INTERFACE,
        ) else {
            return inventory;
        };
        let connection_paths = settings
            .call::<_, _, Vec<OwnedObjectPath>>("ListConnections", &())
            .unwrap_or_default();
        for connection_path in connection_paths {
            let Ok(connection_proxy) = Proxy::new(
                connection,
                NETWORK_MANAGER_DESTINATION,
                connection_path.as_str(),
                NETWORK_MANAGER_CONNECTION_INTERFACE,
            ) else {
                continue;
            };
            let Ok(settings) = connection_proxy
                .call::<_, _, HashMap<String, HashMap<String, OwnedValue>>>("GetSettings", &())
            else {
                continue;
            };
            let Some(connection_settings) = settings.get("connection") else {
                continue;
            };
            let kind = owned_string(connection_settings.get("type"));
            if kind.as_deref() != Some("802-11-wireless") {
                continue;
            }
            let Some(label) = owned_string(connection_settings.get("id"))
                .map(|label| bounded_text(&label, 128))
                .filter(|label| !label.is_empty())
            else {
                continue;
            };
            let connected = active_connection_paths.contains(connection_path.as_str());
            let signal = if connected {
                inventory.state.signal_level.unwrap_or(1)
            } else {
                1
            };
            inventory.selections.push(NetworkSelection {
                id: opaque_id("network", connection_path.as_str()),
                label,
                connection_path: connection_path.clone(),
                connected,
                signal_level: signal,
            });
        }
        inventory
            .selections
            .sort_by(|left, right| left.label.cmp(&right.label));
        inventory.state.wifi_networks = inventory
            .selections
            .iter()
            .map(|selection| SystemWifiNetwork {
                id: selection.id.clone(),
                label: selection.label.clone(),
                signal_level: selection.signal_level,
                saved: true,
                connected: selection.connected,
            })
            .collect();
        inventory
    }

    fn connect_network(&self, network_id: &str) -> Result<(), ProviderError> {
        let inventory = self.network_inventory();
        let selection = inventory
            .selections
            .into_iter()
            .find(|selection| selection.id == network_id)
            .ok_or_else(|| ProviderError::Unavailable("unknown saved Wi-Fi selection".into()))?;
        let device = inventory.wifi_device.ok_or_else(|| {
            ProviderError::Unavailable("NetworkManager has no Wi-Fi device".into())
        })?;
        let connection = self
            .system_bus
            .as_ref()
            .ok_or_else(|| ProviderError::Unavailable("system D-Bus is unavailable".into()))?;
        let manager = Proxy::new(
            connection,
            NETWORK_MANAGER_DESTINATION,
            NETWORK_MANAGER_PATH,
            NETWORK_MANAGER_INTERFACE,
        )
        .map_err(dbus_error)?;
        let specific = OwnedObjectPath::try_from("/")
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        let _: OwnedObjectPath = manager
            .call(
                "ActivateConnection",
                &(selection.connection_path, device, specific),
            )
            .map_err(dbus_error)?;
        Ok(())
    }

    fn disconnect_network(&self) -> Result<(), ProviderError> {
        let inventory = self.network_inventory();
        let connection = self
            .system_bus
            .as_ref()
            .ok_or_else(|| ProviderError::Unavailable("system D-Bus is unavailable".into()))?;
        let manager = Proxy::new(
            connection,
            NETWORK_MANAGER_DESTINATION,
            NETWORK_MANAGER_PATH,
            NETWORK_MANAGER_INTERFACE,
        )
        .map_err(dbus_error)?;
        let active = inventory.active_connections.first().ok_or_else(|| {
            ProviderError::Unavailable("NetworkManager has no active connection".into())
        })?;
        let _: () = manager
            .call("DeactivateConnection", &(active.clone(),))
            .map_err(dbus_error)?;
        Ok(())
    }

    fn media_player(&self) -> Option<MediaPlayer> {
        let connection = self.session_bus.as_ref()?;
        let dbus = DBusProxy::new(connection).ok()?;
        let mut names = dbus
            .list_names()
            .ok()?
            .into_iter()
            .map(|name| name.to_string())
            .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
            .collect::<Vec<_>>();
        names.sort();
        let mut players = names
            .into_iter()
            .filter_map(|name| media_player(connection, name))
            .collect::<Vec<_>>();
        players.sort_by_key(|player| !player.state.playing);
        players.into_iter().next()
    }

    fn media_command(&self, method: &str) -> Result<(), ProviderError> {
        let player = self
            .media_player()
            .ok_or_else(|| ProviderError::Unavailable("no active MPRIS media player".into()))?;
        let connection = self
            .session_bus
            .as_ref()
            .ok_or_else(|| ProviderError::Unavailable("session D-Bus is unavailable".into()))?;
        let proxy = Proxy::new(
            connection,
            player.bus_name.as_str(),
            MPRIS_PATH,
            MPRIS_PLAYER_INTERFACE,
        )
        .map_err(dbus_error)?;
        let _: () = proxy.call(method, &()).map_err(dbus_error)?;
        Ok(())
    }

    fn launch_application(&self, app_id: &str) -> Result<(), ProviderError> {
        let application = self
            .applications
            .iter()
            .find(|application| application.id == app_id)
            .ok_or_else(|| ProviderError::Unavailable("unknown application selection".into()))?;
        let status = Command::new("/usr/bin/gio")
            .arg("launch")
            .arg(&application.desktop_file)
            .env("LC_ALL", "C")
            .status()?;
        status.success().then_some(()).ok_or_else(|| {
            ProviderError::Unavailable(format!("GIO application launch exited with {status}"))
        })
    }
}

fn fallback_power(percent: Option<u8>, on_ac: Option<bool>) -> PowerProviderState {
    PowerProviderState {
        battery_percent: percent,
        charging: percent.and(on_ac),
        charging_source: match on_ac {
            Some(true) => "ac".into(),
            Some(false) => "battery".into(),
            None => String::new(),
        },
        battery_temperature_deci_c: None,
        thermal_status: thermal_status(),
    }
}

fn thermal_status() -> Option<ThermalStatus> {
    let root = Path::new("/sys/class/thermal");
    let hottest = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path().join("temp")).ok())
        .filter_map(|value| value.trim().parse::<i64>().ok())
        .max()?;
    Some(match hottest {
        ..=59_999 => ThermalStatus::None,
        60_000..=69_999 => ThermalStatus::Light,
        70_000..=79_999 => ThermalStatus::Moderate,
        80_000..=89_999 => ThermalStatus::Severe,
        90_000..=99_999 => ThermalStatus::Critical,
        100_000..=109_999 => ThermalStatus::Emergency,
        _ => ThermalStatus::Shutdown,
    })
}

fn signal_level(strength: u8) -> u8 {
    match strength {
        0..=24 => 1,
        25..=49 => 2,
        50..=74 => 3,
        _ => 4,
    }
}

fn normalize_locale(value: &str) -> String {
    let locale = value
        .split(['.', '@'])
        .next()
        .unwrap_or(value)
        .replace('_', "-");
    if locale.eq_ignore_ascii_case("c") || locale.eq_ignore_ascii_case("posix") {
        "und".into()
    } else {
        locale
    }
}

fn owned_string(value: Option<&OwnedValue>) -> Option<String> {
    value.and_then(|value| <&str>::try_from(value).ok().map(str::to_owned))
}

fn media_player(connection: &Connection, bus_name: String) -> Option<MediaPlayer> {
    let proxy = Proxy::new(
        connection,
        bus_name.as_str(),
        MPRIS_PATH,
        MPRIS_PLAYER_INTERFACE,
    )
    .ok()?;
    let playback = proxy
        .get_property::<String>("PlaybackStatus")
        .unwrap_or_default();
    let metadata = proxy
        .get_property::<HashMap<String, OwnedValue>>("Metadata")
        .unwrap_or_default();
    let title = owned_string(metadata.get("xesam:title")).map(|value| bounded_text(&value, 256));
    let artist = metadata
        .get("xesam:artist")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .and_then(|artists| artists.into_iter().next())
        .map(|value| bounded_text(&value, 256));
    let can_play_pause = proxy.get_property::<bool>("CanPlay").unwrap_or(false)
        || proxy.get_property::<bool>("CanPause").unwrap_or(false);
    let can_next = proxy.get_property::<bool>("CanGoNext").unwrap_or(false);
    let can_previous = proxy.get_property::<bool>("CanGoPrevious").unwrap_or(false);
    drop(proxy);
    Some(MediaPlayer {
        bus_name,
        state: MediaProviderState {
            active: true,
            playing: playback == "Playing",
            title: title.unwrap_or_default(),
            artist: artist.unwrap_or_default(),
        },
        can_play_pause,
        can_next,
        can_previous,
    })
}

fn application_inventory() -> Vec<ApplicationSelection> {
    let mut roots = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        roots.push(data_home.join("applications"));
    } else if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".local/share/applications"));
    }
    let data_dirs =
        env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(env::split_paths(&data_dirs).map(|path| path.join("applications")));

    let mut files = Vec::new();
    for root in roots {
        collect_desktop_files(&root, &mut files, 0);
    }
    files.sort();
    files.dedup();
    let mut seen = BTreeSet::new();
    let mut applications = Vec::new();
    for path in files {
        let Some(application) = parse_desktop_file(&path) else {
            continue;
        };
        if seen.insert(application.id.clone()) {
            applications.push(application);
        }
        if applications.len() == 64 {
            break;
        }
    }
    applications.sort_by(|left, right| left.label.cmp(&right.label));
    applications
}

fn collect_desktop_files(root: &Path, output: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_files(&path, output, depth + 1);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("desktop") {
            output.push(path);
        }
    }
}

fn parse_desktop_file(path: &Path) -> Option<ApplicationSelection> {
    let text = fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    let mut name = None;
    let mut kind = None;
    let mut executable = None;
    let mut hidden = false;
    let mut no_display = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Name" => name = Some(value.trim().to_owned()),
            "Type" => kind = Some(value.trim().to_owned()),
            "Exec" => executable = Some(value.trim().to_owned()),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    let name = name
        .map(|name| bounded_text(&name, 128))
        .filter(|name| !name.is_empty())?;
    let executable = executable.filter(|value| !value.is_empty())?;
    if kind.as_deref() != Some("Application")
        || hidden
        || no_display
        || prohibited_application(path, &name, &executable)
    {
        return None;
    }
    let canonical = path.canonicalize().ok()?;
    Some(ApplicationSelection {
        id: opaque_id("app", canonical.to_string_lossy().as_ref()),
        label: name,
        desktop_file: canonical,
    })
}

fn prohibited_application(path: &Path, label: &str, executable: &str) -> bool {
    const PROHIBITED_DESKTOP_FILES: &[&str] = &[
        "anaconda.desktop",
        "liveinst.desktop",
        "org.fedoraproject.MediaWriter.desktop",
        "org.gnome.DiskUtility.desktop",
        "gparted.desktop",
    ];
    const PROHIBITED_EXECUTABLES: &[&str] = &[
        "anaconda",
        "blivet-gui",
        "calamares",
        "gparted",
        "gpartedbin",
        "gnome-disks",
        "liveinst",
        "mediawriter",
    ];
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| PROHIBITED_DESKTOP_FILES.contains(&name))
    {
        return true;
    }
    let command = executable
        .split_whitespace()
        .next()
        .and_then(|command| Path::new(command).file_name())
        .and_then(|command| command.to_str());
    command.is_some_and(|command| PROHIBITED_EXECUTABLES.contains(&command))
        || label.eq_ignore_ascii_case("Install to Hard Drive")
}

fn run_command(program: &str, arguments: &[&str]) -> Result<(), ProviderError> {
    if !Path::new(program).is_file() {
        return Err(ProviderError::Unavailable(format!(
            "required provider adapter is unavailable: {program}"
        )));
    }
    let status = Command::new(program)
        .args(arguments)
        .env("LC_ALL", "C")
        .status()?;
    status.success().then_some(()).ok_or_else(|| {
        ProviderError::Unavailable(format!("provider adapter exited with {status}: {program}"))
    })
}

fn required_opaque_id<'a>(
    effect: &'a ProviderEffect,
    field: &str,
    prefix: &str,
) -> Result<&'a str, ProviderError> {
    effect
        .payload
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            value.starts_with(prefix)
                && value.len() <= 256
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .ok_or_else(|| {
            ProviderError::Unavailable(format!(
                "{}.{} requires a bounded opaque {field}",
                effect.provider, effect.action
            ))
        })
}

fn opaque_id(namespace: &str, source: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    format!("{namespace}-{}", &digest[..24])
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    let mut output = String::with_capacity(value.len().min(max_bytes));
    for character in value.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if output.len() + character.len_utf8() > max_bytes {
            break;
        }
        output.push(character);
    }
    output
}

fn dbus_error(error: zbus::Error) -> ProviderError {
    ProviderError::Unavailable(format!("Linux service D-Bus request failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_snapshot_uses_the_shared_v1_contract_without_privileged_capabilities() {
        let adapter = SystemAdapter::connect();
        let snapshot = adapter
            .snapshot(&super::super::prototype_grants("system-v1"))
            .unwrap();
        assert_eq!(snapshot.abi_version, SYSTEM_PROVIDER_ABI_VERSION);
        assert!(snapshot.clock.unix_time_ms > 0);
        assert!(!snapshot.clock.locale.is_empty());
        assert!(!snapshot.clock.time_label.is_empty());
        assert!(snapshot
            .apps
            .compatible
            .iter()
            .all(|application| application.id.starts_with("app-")
                && !application.id.contains('/')
                && !application.label.is_empty()));
        assert!(!snapshot.capabilities.iter().any(|capability| matches!(
            capability,
            SystemCapability::RequestLock
                | SystemCapability::RequestRestart
                | SystemCapability::RequestShutdown
        )));
    }

    #[test]
    fn system_effects_fail_before_adapter_access_without_a_grant() {
        let adapter = SystemAdapter::connect();
        let context = ProviderContext {
            revision_id: "denied".into(),
            instance_id: None,
            grants: BTreeSet::new(),
            cancellation: Default::default(),
        };
        let effect = ProviderEffect {
            provider: "audio".into(),
            action: "set_volume".into(),
            payload: serde_json::json!({"percent": 50}),
        };
        assert!(matches!(
            adapter.execute(&context, &effect),
            Err(ProviderError::Denied(Capability::AudioControl))
        ));
    }

    #[test]
    fn relative_volume_rejects_zero_and_out_of_range_deltas() {
        let adapter = SystemAdapter::connect();
        let context = ProviderContext {
            revision_id: "audio-adjustment".into(),
            instance_id: None,
            grants: [Capability::AudioControl].into_iter().collect(),
            cancellation: Default::default(),
        };
        for delta in [0, -101, 101] {
            let effect = ProviderEffect {
                provider: "audio".into(),
                action: "adjust_volume".into(),
                payload: serde_json::json!({"delta": delta}),
            };
            assert!(matches!(
                adapter.execute(&context, &effect),
                Err(ProviderError::Unavailable(_))
            ));
        }
    }

    #[test]
    fn desktop_entries_become_opaque_bounded_applications() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("calculator.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Calculator\nExec=calculator\n",
        )
        .unwrap();
        let application = parse_desktop_file(&path).unwrap();
        assert_eq!(application.label, "Calculator");
        assert!(application.id.starts_with("app-"));
        assert!(!application.id.contains("calculator"));

        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nNoDisplay=true\n",
        )
        .unwrap();
        assert!(parse_desktop_file(&path).is_none());

        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Friendly name\nExec=liveinst\n",
        )
        .unwrap();
        assert!(parse_desktop_file(&path).is_none());
    }

    #[test]
    fn storage_writers_are_not_application_provider_selections() {
        for (desktop_file, label, executable) in [
            ("anaconda.desktop", "Installer", "liveinst"),
            (
                "org.fedoraproject.MediaWriter.desktop",
                "Writer",
                "mediawriter",
            ),
            ("org.gnome.DiskUtility.desktop", "Disks", "gnome-disks"),
            ("renamed.desktop", "Partitioner", "/usr/bin/gparted"),
        ] {
            assert!(prohibited_application(
                Path::new(desktop_file),
                label,
                executable
            ));
        }
        assert!(!prohibited_application(
            Path::new("org.gnome.TextEditor.desktop"),
            "Text Editor",
            "gnome-text-editor %U"
        ));
    }

    #[test]
    fn signal_strength_is_normalized_to_four_levels() {
        assert_eq!(signal_level(0), 1);
        assert_eq!(signal_level(25), 2);
        assert_eq!(signal_level(50), 3);
        assert_eq!(signal_level(100), 4);
    }

    #[test]
    fn posix_locales_are_normalized_to_language_tags() {
        assert_eq!(normalize_locale("en_US.UTF-8"), "en-US");
        assert_eq!(normalize_locale("de_CH@euro"), "de-CH");
        assert_eq!(normalize_locale("C.UTF-8"), "und");
    }

    #[test]
    fn opaque_ids_are_stable_and_namespaced() {
        let first = opaque_id("network", "/org/freedesktop/NetworkManager/Settings/1");
        let second = opaque_id("network", "/org/freedesktop/NetworkManager/Settings/1");
        assert_eq!(first, second);
        assert!(first.starts_with("network-"));
        assert_eq!(first.len(), "network-".len() + 24);
    }

    #[test]
    fn public_labels_are_control_free_and_utf8_bounded() {
        assert_eq!(bounded_text("hello\nworld", 64), "hello world");
        assert_eq!(bounded_text("ééé", 5), "éé");
        assert_eq!(bounded_text("long", 2), "lo");
    }
}
