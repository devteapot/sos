use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use experience_ir::{
    AppsProviderState, AttentionProviderState, AudioProviderState, CalendarEvent,
    ClockProviderState, ConnectivityProviderState, ExperienceModel, Music, NetworkState, Note,
    PowerProviderState, ProviderEffect, SystemCapability, SystemProviders, SystemState,
    ThermalStatus, Weather, SYSTEM_PROVIDER_ABI_VERSION,
};
#[cfg(any(target_os = "android", test))]
use serde_json::json;
use serde_json::Value;

const MAX_OPAQUE_ID_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SystemAction {
    SetVolume(u8),
    AdjustVolume(i8),
    SetMuted(bool),
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    WifiConnect(String),
    WifiDisconnect,
    LaunchApplication(String),
    AcknowledgeAttention(String),
    RequestLock,
    RequestRestart,
    RequestShutdown,
}

impl SystemAction {
    fn capability(&self) -> SystemCapability {
        match self {
            Self::SetVolume(_) | Self::AdjustVolume(_) => SystemCapability::AudioSetVolume,
            Self::SetMuted(_) => SystemCapability::AudioSetMuted,
            Self::MediaPlayPause => SystemCapability::MediaPlayPause,
            Self::MediaNext => SystemCapability::MediaNext,
            Self::MediaPrevious => SystemCapability::MediaPrevious,
            Self::WifiConnect(_) => SystemCapability::WifiConnect,
            Self::WifiDisconnect => SystemCapability::WifiDisconnect,
            Self::LaunchApplication(_) => SystemCapability::AppLaunch,
            Self::AcknowledgeAttention(_) => SystemCapability::AttentionAcknowledge,
            Self::RequestLock => SystemCapability::RequestLock,
            Self::RequestRestart => SystemCapability::RequestRestart,
            Self::RequestShutdown => SystemCapability::RequestShutdown,
        }
    }

    #[cfg(target_os = "android")]
    fn wire_value(&self) -> Value {
        match self {
            Self::SetVolume(percent) => {
                json!({"provider":"audio","action":"set_volume","payload":{"percent":percent}})
            }
            Self::AdjustVolume(delta) => {
                json!({"provider":"audio","action":"adjust_volume","payload":{"delta":delta}})
            }
            Self::SetMuted(muted) => {
                json!({"provider":"audio","action":"set_muted","payload":{"muted":muted}})
            }
            Self::MediaPlayPause => json!({"provider":"media","action":"play_pause"}),
            Self::MediaNext => json!({"provider":"media","action":"next"}),
            Self::MediaPrevious => json!({"provider":"media","action":"previous"}),
            Self::WifiConnect(id) => {
                json!({"provider":"network","action":"connect","payload":{"network_id":id}})
            }
            Self::WifiDisconnect => json!({"provider":"network","action":"disconnect"}),
            Self::LaunchApplication(id) => {
                json!({"provider":"apps","action":"launch","payload":{"app_id":id}})
            }
            Self::AcknowledgeAttention(id) => json!({
                "provider":"attention","action":"acknowledge","payload":{"attention_id":id}
            }),
            Self::RequestLock => json!({"provider":"power","action":"request_lock"}),
            Self::RequestRestart => json!({"provider":"power","action":"request_restart"}),
            Self::RequestShutdown => json!({"provider":"power","action":"request_shutdown"}),
        }
    }
}

/// Typed provider adapter below the canonical authority registry. Compat and
/// Core 0B use the non-rendering framework adapter; Core 1 uses a native
/// platform daemon. Neither adapter can pass platform handles through this
/// boundary.
pub(crate) trait ProviderAdapter: Send + Sync {
    fn snapshot(&self) -> Result<SystemProviders, String>;
    fn execute(&self, action: &SystemAction) -> Result<Value, String>;
}

#[cfg(not(target_os = "android"))]
struct UnavailableProviderAdapter;

#[cfg(not(target_os = "android"))]
impl ProviderAdapter for UnavailableProviderAdapter {
    fn snapshot(&self) -> Result<SystemProviders, String> {
        Err("Android provider adapter is unavailable".into())
    }

    fn execute(&self, _action: &SystemAction) -> Result<Value, String> {
        Err("Android provider adapter is unavailable".into())
    }
}

pub(crate) struct SystemProviderRegistry {
    sysfs_root: PathBuf,
    adapter: Box<dyn ProviderAdapter>,
}

impl SystemProviderRegistry {
    pub(crate) fn android() -> Self {
        Self {
            sysfs_root: PathBuf::from("/sys"),
            #[cfg(target_os = "android")]
            adapter: android_provider_adapter(),
            #[cfg(not(target_os = "android"))]
            adapter: Box::new(UnavailableProviderAdapter),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_adapter(
        sysfs_root: impl Into<PathBuf>,
        adapter: Box<dyn ProviderAdapter>,
    ) -> Self {
        Self {
            sysfs_root: sysfs_root.into(),
            adapter,
        }
    }

    pub(crate) fn snapshot_model(&self) -> ExperienceModel {
        let mut providers = self.native_snapshot();
        match self.adapter.snapshot() {
            Ok(platform) => merge_platform_snapshot(&mut providers, platform),
            Err(error) => eprintln!("platform_provider_snapshot_unavailable error={error}"),
        }
        providers.capabilities.sort_by_key(capability_order);
        providers.capabilities.dedup();
        compatibility_model(providers)
    }

    pub(crate) fn parse_and_authorize(
        &self,
        effect: &ProviderEffect,
    ) -> Result<SystemAction, String> {
        let action = parse_action(effect)?;
        let capability = action.capability();
        let granted = self
            .adapter
            .snapshot()
            .map(|snapshot| {
                snapshot.abi_version == SYSTEM_PROVIDER_ABI_VERSION
                    && platform_capability_allowed(capability)
                    && snapshot.capabilities.contains(&capability)
            })
            .unwrap_or(false);
        if !granted {
            return Err(format!(
                "provider capability is not granted: {}.{}",
                effect.provider, effect.action
            ));
        }
        Ok(action)
    }

    pub(crate) fn execute(&self, action: &SystemAction) -> Result<Value, String> {
        self.adapter.execute(action)
    }

    fn native_snapshot(&self) -> SystemProviders {
        let observed_at_ms = now_ms();
        let clock = native_clock(observed_at_ms);
        #[cfg(not(target_os = "android"))]
        let power = native_power(&self.sysfs_root);
        // Android deliberately keeps battery sysfs labels private to the
        // vendor health HAL. Compat receives health facts from its narrow
        // framework adapter; Core 1 receives them from the native health-HAL
        // client. Public thermal zones remain a native fallback for both.
        #[cfg(target_os = "android")]
        let power = native_thermal_power(&self.sysfs_root);
        let connectivity = native_connectivity(&self.sysfs_root);
        SystemProviders {
            abi_version: SYSTEM_PROVIDER_ABI_VERSION,
            observed_at_ms,
            clock,
            power,
            connectivity,
            audio: AudioProviderState::default(),
            apps: AppsProviderState::default(),
            attention: AttentionProviderState::default(),
            capabilities: Vec::new(),
        }
    }
}

fn parse_action(effect: &ProviderEffect) -> Result<SystemAction, String> {
    if serde_json::to_vec(&effect.payload)
        .map_err(|error| error.to_string())?
        .len()
        > experience_ir::MAX_EFFECT_PAYLOAD_BYTES
    {
        return Err("provider action payload is larger than the ABI limit".into());
    }
    let required_id = |field: &str| {
        let value = effect
            .payload
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{field} is required"))?;
        if value.is_empty()
            || value.len() > MAX_OPAQUE_ID_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(format!("{field} is not a bounded opaque identifier"));
        }
        Ok(value.to_owned())
    };
    match (effect.provider.as_str(), effect.action.as_str()) {
        ("audio", "set_volume") => effect
            .payload
            .get("percent")
            .and_then(Value::as_u64)
            .filter(|percent| *percent <= 100)
            .map(|percent| SystemAction::SetVolume(percent as u8))
            .ok_or_else(|| "audio.set_volume requires percent in 0..100".into()),
        ("audio", "adjust_volume") => effect
            .payload
            .get("delta")
            .and_then(Value::as_i64)
            .filter(|delta| (-100..=100).contains(delta) && *delta != 0)
            .map(|delta| SystemAction::AdjustVolume(delta as i8))
            .ok_or_else(|| "audio.adjust_volume requires a non-zero delta in -100..100".into()),
        ("audio", "set_muted") => effect
            .payload
            .get("muted")
            .and_then(Value::as_bool)
            .map(SystemAction::SetMuted)
            .ok_or_else(|| "audio.set_muted requires a boolean muted value".into()),
        ("media", "play_pause") => Ok(SystemAction::MediaPlayPause),
        ("media", "next") => Ok(SystemAction::MediaNext),
        ("media", "previous") => Ok(SystemAction::MediaPrevious),
        ("network", "connect") => required_id("network_id").map(SystemAction::WifiConnect),
        ("network", "disconnect") => Ok(SystemAction::WifiDisconnect),
        ("apps", "launch") => required_id("app_id").map(SystemAction::LaunchApplication),
        ("attention", "acknowledge") => {
            required_id("attention_id").map(SystemAction::AcknowledgeAttention)
        }
        ("power", "request_lock") => Ok(SystemAction::RequestLock),
        ("power", "request_restart") => Ok(SystemAction::RequestRestart),
        ("power", "request_shutdown") => Ok(SystemAction::RequestShutdown),
        (provider, action) => Err(format!("unsupported provider action: {provider}.{action}")),
    }
}

fn merge_platform_snapshot(native: &mut SystemProviders, platform: SystemProviders) {
    // A version mismatch is a closed boundary: retain native facts and grant
    // no framework-only action capabilities.
    if platform.abi_version != SYSTEM_PROVIDER_ABI_VERSION {
        eprintln!(
            "platform_provider_abi_rejected expected={} supplied={}",
            SYSTEM_PROVIDER_ABI_VERSION, platform.abi_version
        );
        return;
    }
    native.observed_at_ms = native.observed_at_ms.max(platform.observed_at_ms);
    if platform.clock.unix_time_ms != 0 {
        native.clock = platform.clock;
    }
    merge_power(&mut native.power, platform.power);
    let native_interfaces = native.connectivity.online_interfaces.clone();
    native.connectivity = platform.connectivity;
    if native.connectivity.online_interfaces.is_empty() {
        native.connectivity.online_interfaces = native_interfaces;
    }
    native.audio = platform.audio;
    native.apps = platform.apps;
    native.attention = platform.attention;
    native.capabilities = platform
        .capabilities
        .into_iter()
        .filter(|capability| platform_capability_allowed(*capability))
        .collect();
}

fn merge_power(native: &mut PowerProviderState, framework: PowerProviderState) {
    if framework.battery_percent.is_some() {
        native.battery_percent = framework.battery_percent;
    }
    if framework.charging.is_some() {
        native.charging = framework.charging;
        native.charging_source = framework.charging_source;
    }
    if framework.battery_temperature_deci_c.is_some() {
        native.battery_temperature_deci_c = framework.battery_temperature_deci_c;
    }
    if framework.thermal_status.is_some() {
        native.thermal_status = framework.thermal_status;
    }
}

fn platform_capability_allowed(capability: SystemCapability) -> bool {
    matches!(
        capability,
        SystemCapability::AudioSetVolume
            | SystemCapability::AudioSetMuted
            | SystemCapability::MediaPlayPause
            | SystemCapability::MediaNext
            | SystemCapability::MediaPrevious
            | SystemCapability::WifiConnect
            | SystemCapability::WifiDisconnect
            | SystemCapability::AppLaunch
            | SystemCapability::AttentionAcknowledge
    )
}

fn capability_order(capability: &SystemCapability) -> u8 {
    match capability {
        SystemCapability::AudioSetVolume => 0,
        SystemCapability::AudioSetMuted => 1,
        SystemCapability::MediaPlayPause => 2,
        SystemCapability::MediaNext => 3,
        SystemCapability::MediaPrevious => 4,
        SystemCapability::WifiConnect => 5,
        SystemCapability::WifiDisconnect => 6,
        SystemCapability::AppLaunch => 7,
        SystemCapability::AttentionAcknowledge => 8,
        SystemCapability::RequestLock => 9,
        SystemCapability::RequestRestart => 10,
        SystemCapability::RequestShutdown => 11,
    }
}

fn compatibility_model(providers: SystemProviders) -> ExperienceModel {
    let connectivity = &providers.connectivity;
    let media = &providers.audio.media;
    ExperienceModel {
        greeting: "SOS".into(),
        date: providers.clock.date_label.clone(),
        weather: Weather {
            summary: "Weather unavailable".into(),
            temperature_c: 0,
            high_c: 0,
            low_c: 0,
        },
        calendar: Vec::<CalendarEvent>::new(),
        notes: Vec::<Note>::new(),
        music: Music {
            title: media.title.clone(),
            artist: media.artist.clone(),
            playing: media.playing,
        },
        system: SystemState {
            unix_time_ms: providers.clock.unix_time_ms,
            timezone: providers.clock.timezone.clone(),
            online_interfaces: connectivity.online_interfaces.clone(),
            battery_percent: providers.power.battery_percent,
            on_ac_power: providers.power.charging,
            audio_volume_percent: providers.audio.volume_percent,
            audio_muted: providers.audio.muted,
            connected_displays: Vec::new(),
            input_devices: Vec::new(),
        },
        surfaces: Vec::new(),
        agent: Default::default(),
        network: NetworkState {
            wifi_enabled: connectivity.wifi_enabled,
            connected: connectivity.connected,
            connected_ssid: (!connectivity.network_label.is_empty())
                .then(|| connectivity.network_label.clone()),
            validated: connectivity.validated,
            signal_level: connectivity.signal_level,
            networks: Vec::new(),
            activity: connectivity.transport.clone(),
            error: None,
        },
        providers,
    }
}

fn native_clock(unix_time_ms: u64) -> ClockProviderState {
    let timezone = system_property("persist.sys.timezone")
        .or_else(|| std::env::var("TZ").ok())
        .unwrap_or_else(|| "UTC".into());
    let locale = system_property("persist.sys.locale")
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "und".into());
    ClockProviderState {
        unix_time_ms,
        locale,
        timezone,
        time_label: format_local_time("%H:%M"),
        date_label: format_local_time("%A, %e %B"),
    }
}

#[cfg(any(not(target_os = "android"), test))]
fn native_power(sysfs_root: &Path) -> PowerProviderState {
    let mut result = PowerProviderState::default();
    if let Ok(entries) = fs::read_dir(sysfs_root.join("class/power_supply")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let supply_type = read_trimmed(path.join("type")).unwrap_or_default();
            if supply_type.eq_ignore_ascii_case("battery") {
                result.battery_percent =
                    read_number::<u8>(path.join("capacity")).map(|value| value.min(100));
                result.battery_temperature_deci_c = read_number(path.join("temp"));
                if let Some(status) = read_trimmed(path.join("status")) {
                    result.charging = Some(matches!(
                        status.to_ascii_lowercase().as_str(),
                        "charging" | "full"
                    ));
                }
            } else if read_number::<u8>(path.join("online")) == Some(1) {
                result.charging = Some(true);
                if result.charging_source.is_empty() {
                    result.charging_source = supply_type;
                }
            }
        }
    }
    result.thermal_status = hottest_thermal_status(sysfs_root);
    result
}

#[cfg(target_os = "android")]
fn native_thermal_power(sysfs_root: &Path) -> PowerProviderState {
    PowerProviderState {
        thermal_status: hottest_thermal_status(sysfs_root),
        ..PowerProviderState::default()
    }
}

fn hottest_thermal_status(sysfs_root: &Path) -> Option<ThermalStatus> {
    let entries = fs::read_dir(sysfs_root.join("class/thermal")).ok()?;
    let hottest = entries
        .flatten()
        .filter_map(|entry| read_number::<i32>(entry.path().join("temp")))
        .max()?;
    Some(match hottest {
        value if value >= 90_000 => ThermalStatus::Emergency,
        value if value >= 80_000 => ThermalStatus::Critical,
        value if value >= 70_000 => ThermalStatus::Severe,
        value if value >= 60_000 => ThermalStatus::Moderate,
        value if value >= 50_000 => ThermalStatus::Light,
        _ => ThermalStatus::None,
    })
}

fn native_connectivity(sysfs_root: &Path) -> ConnectivityProviderState {
    let network_root = sysfs_root.join("class/net");
    let mut online_interfaces = Vec::new();
    let mut wifi_enabled = false;
    if let Ok(entries) = fs::read_dir(&network_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "lo" {
                continue;
            }
            if name.starts_with("wlan") || name.starts_with("wifi") {
                wifi_enabled = true;
            }
            if read_trimmed(entry.path().join("operstate")).as_deref() == Some("up") {
                online_interfaces.push(name);
            }
        }
    }
    online_interfaces.sort();
    let wifi_connected = online_interfaces
        .iter()
        .any(|name| name.starts_with("wlan") || name.starts_with("wifi"));
    let connected = !online_interfaces.is_empty();
    ConnectivityProviderState {
        wifi_enabled,
        connected,
        // Link state is not Android's validated-network signal.
        validated: false,
        transport: if wifi_connected {
            "wifi".into()
        } else if connected {
            "network".into()
        } else {
            String::new()
        },
        network_label: if wifi_connected {
            "Wi-Fi".into()
        } else {
            String::new()
        },
        signal_level: None,
        online_interfaces,
        wifi_networks: Vec::new(),
    }
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_number<T: std::str::FromStr>(path: impl AsRef<Path>) -> Option<T> {
    read_trimmed(path)?.parse().ok()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn format_local_time(format: &str) -> String {
    let seconds = (now_ms() / 1000) as libc::time_t;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: localtime_r initializes `local` when it returns non-null and
    // strftime receives a valid NUL-terminated format and output buffer.
    unsafe {
        if libc::localtime_r(&seconds, local.as_mut_ptr()).is_null() {
            return String::new();
        }
        let local = local.assume_init();
        let Ok(format) = std::ffi::CString::new(format) else {
            return String::new();
        };
        let mut output = [0 as libc::c_char; 128];
        let length = libc::strftime(output.as_mut_ptr(), output.len(), format.as_ptr(), &local);
        if length == 0 {
            return String::new();
        }
        std::ffi::CStr::from_ptr(output.as_ptr())
            .to_string_lossy()
            .trim()
            .to_owned()
    }
}

#[cfg(target_os = "android")]
fn system_property(name: &str) -> Option<String> {
    use std::ffi::{c_char, c_int, CStr, CString};
    unsafe extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }
    let name = CString::new(name).ok()?;
    let mut value = [0 as c_char; 92];
    // SAFETY: bionic writes at most PROP_VALUE_MAX bytes to the supplied
    // buffer and the property name is NUL terminated.
    let length = unsafe { __system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    (length > 0).then(|| {
        unsafe { CStr::from_ptr(value.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

#[cfg(not(target_os = "android"))]
fn system_property(_name: &str) -> Option<String> {
    None
}

#[cfg(target_os = "android")]
struct AndroidFrameworkBridge;

#[cfg(target_os = "android")]
impl ProviderAdapter for AndroidFrameworkBridge {
    fn snapshot(&self) -> Result<SystemProviders, String> {
        let bytes = provider_adapter_request("sos_framework_bridge", 4, None)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("decode framework snapshot: {error}"))
    }

    fn execute(&self, action: &SystemAction) -> Result<Value, String> {
        let payload =
            serde_json::to_vec(&action.wire_value()).map_err(|error| error.to_string())?;
        let bytes = provider_adapter_request("sos_framework_bridge", 5, Some(&payload))?;
        serde_json::from_slice(&bytes).map_err(|error| format!("decode framework action: {error}"))
    }
}

#[cfg(target_os = "android")]
struct CoreNativeAdapter;

#[cfg(target_os = "android")]
impl ProviderAdapter for CoreNativeAdapter {
    fn snapshot(&self) -> Result<SystemProviders, String> {
        let bytes = provider_adapter_request("sos_core_platform", 4, None)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("decode Core native snapshot: {error}"))
    }

    fn execute(&self, action: &SystemAction) -> Result<Value, String> {
        let payload =
            serde_json::to_vec(&action.wire_value()).map_err(|error| error.to_string())?;
        let bytes = provider_adapter_request("sos_core_platform", 5, Some(&payload))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("decode Core native action: {error}"))
    }
}

#[cfg(target_os = "android")]
fn android_provider_adapter() -> Box<dyn ProviderAdapter> {
    if system_property("ro.sos.providers").as_deref() == Some("core-native") {
        Box::new(CoreNativeAdapter)
    } else {
        Box::new(AndroidFrameworkBridge)
    }
}

#[cfg(target_os = "android")]
fn provider_adapter_request(
    socket_name: &str,
    command: u8,
    payload: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use std::{
        io::{Read, Write},
        mem::size_of,
        os::fd::{FromRawFd, OwnedFd},
    };

    const MAGIC: u32 = 0x534f5331;
    const RESPONSE_OK: u8 = 1;
    const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: `raw` is a newly owned descriptor and is transferred exactly
    // once into OwnedFd.
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut address = unsafe { std::mem::zeroed::<libc::sockaddr_un>() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let name = socket_name.as_bytes();
    if name.len() + 1 > address.sun_path.len() {
        return Err("provider adapter socket name is too long".into());
    }
    address.sun_path[0] = 0;
    for (index, byte) in name.iter().enumerate() {
        address.sun_path[index + 1] = *byte as libc::c_char;
    }
    let address_length = (size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;
    let connected = unsafe {
        libc::connect(
            raw,
            &address as *const libc::sockaddr_un as *const libc::sockaddr,
            address_length,
        )
    };
    if connected != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut stream = std::os::unix::net::UnixStream::from(owned);
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&MAGIC.to_be_bytes())
        .and_then(|()| stream.write_all(&[command]))
        .map_err(|error| error.to_string())?;
    if let Some(payload) = payload {
        let length = u32::try_from(payload.len()).map_err(|_| "provider action is too large")?;
        stream
            .write_all(&length.to_be_bytes())
            .and_then(|()| stream.write_all(payload))
            .map_err(|error| error.to_string())?;
    }
    stream.flush().map_err(|error| error.to_string())?;
    let mut header = [0_u8; 5];
    stream
        .read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    if u32::from_be_bytes(header[..4].try_into().expect("four-byte magic")) != MAGIC {
        return Err("provider adapter returned bad protocol magic".into());
    }
    if header[4] != RESPONSE_OK {
        return Err("provider adapter rejected provider request".into());
    }
    let mut length = [0_u8; 4];
    stream
        .read_exact(&mut length)
        .map_err(|error| error.to_string())?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_RESPONSE_BYTES {
        return Err("provider adapter response exceeded its size limit".into());
    }
    let mut response = vec![0; length];
    stream
        .read_exact(&mut response)
        .map_err(|error| error.to_string())?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use experience_ir::{
        AttentionItem, AttentionKind, MediaProviderState, SystemApplication, SystemWifiNetwork,
    };
    use std::sync::Mutex;

    struct FixtureAdapter {
        snapshot: SystemProviders,
        actions: Mutex<Vec<SystemAction>>,
    }

    impl ProviderAdapter for FixtureAdapter {
        fn snapshot(&self) -> Result<SystemProviders, String> {
            Ok(self.snapshot.clone())
        }

        fn execute(&self, action: &SystemAction) -> Result<Value, String> {
            self.actions.lock().unwrap().push(action.clone());
            Ok(json!({"accepted": true}))
        }
    }

    fn fixture_snapshot() -> SystemProviders {
        SystemProviders {
            abi_version: SYSTEM_PROVIDER_ABI_VERSION,
            observed_at_ms: 42,
            clock: ClockProviderState {
                unix_time_ms: 42,
                locale: "en-CH".into(),
                timezone: "Europe/Zurich".into(),
                time_label: "12:34".into(),
                date_label: "Sunday, 16 August".into(),
            },
            power: PowerProviderState::default(),
            connectivity: ConnectivityProviderState {
                wifi_enabled: true,
                connected: true,
                validated: true,
                transport: "wifi".into(),
                network_label: "Studio".into(),
                signal_level: Some(4),
                online_interfaces: vec!["wlan0".into()],
                wifi_networks: vec![SystemWifiNetwork {
                    id: "network-1".into(),
                    label: "Studio".into(),
                    signal_level: 4,
                    saved: true,
                    connected: true,
                }],
            },
            audio: AudioProviderState {
                volume_percent: Some(50),
                muted: Some(false),
                media: MediaProviderState::default(),
            },
            apps: AppsProviderState {
                compatible: vec![SystemApplication {
                    id: "app-1".into(),
                    label: "Calculator".into(),
                }],
            },
            attention: AttentionProviderState::default(),
            capabilities: vec![SystemCapability::AudioSetVolume],
        }
    }

    fn parity_snapshot() -> SystemProviders {
        let mut snapshot = fixture_snapshot();
        snapshot.power = PowerProviderState {
            battery_percent: Some(72),
            charging: Some(true),
            charging_source: "usb".into(),
            battery_temperature_deci_c: Some(301),
            thermal_status: Some(ThermalStatus::Light),
        };
        snapshot.audio.media = MediaProviderState {
            active: true,
            playing: true,
            title: "Native track".into(),
            artist: "SOS media".into(),
        };
        snapshot.attention = AttentionProviderState {
            items: vec![AttentionItem {
                id: "attention-1".into(),
                occurred_at_ms: 42,
                source: "SOS runtime".into(),
                kind: AttentionKind::System,
                urgent: false,
                title: "Native attention".into(),
                detail: "Typed journal entry".into(),
            }],
            urgent_count: 0,
        };
        snapshot.capabilities = vec![
            SystemCapability::AudioSetVolume,
            SystemCapability::AudioSetMuted,
            SystemCapability::MediaPlayPause,
            SystemCapability::MediaNext,
            SystemCapability::MediaPrevious,
            SystemCapability::WifiConnect,
            SystemCapability::WifiDisconnect,
            SystemCapability::AppLaunch,
            SystemCapability::AttentionAcknowledge,
            // A platform adapter must not grant a trusted-confirmation action.
            SystemCapability::RequestRestart,
        ];
        snapshot
    }

    #[test]
    fn framework_facts_replace_native_placeholders_without_exposing_internals() {
        let temporary = tempfile::tempdir().unwrap();
        let adapter = FixtureAdapter {
            snapshot: fixture_snapshot(),
            actions: Mutex::new(Vec::new()),
        };
        let registry = SystemProviderRegistry::with_adapter(temporary.path(), Box::new(adapter));
        let model = registry.snapshot_model();
        assert_eq!(model.providers.abi_version, SYSTEM_PROVIDER_ABI_VERSION);
        assert_eq!(model.date, "Sunday, 16 August");
        assert_eq!(model.providers.apps.compatible[0].id, "app-1");
        assert_eq!(model.network.connected_ssid.as_deref(), Some("Studio"));
        let encoded = serde_json::to_string(&model.providers).unwrap();
        assert!(!encoded.contains("package"));
        assert!(!encoded.contains("activity"));
    }

    #[test]
    fn native_adapters_report_live_sysfs_facts_without_android() {
        let temporary = tempfile::tempdir().unwrap();
        let battery = temporary.path().join("class/power_supply/battery");
        let wifi = temporary.path().join("class/net/wlan0");
        let thermal = temporary.path().join("class/thermal/thermal_zone0");
        fs::create_dir_all(&battery).unwrap();
        fs::create_dir_all(&wifi).unwrap();
        fs::create_dir_all(&thermal).unwrap();
        fs::write(battery.join("type"), "Battery\n").unwrap();
        fs::write(battery.join("capacity"), "73\n").unwrap();
        fs::write(battery.join("status"), "Charging\n").unwrap();
        fs::write(battery.join("temp"), "298\n").unwrap();
        fs::write(wifi.join("operstate"), "up\n").unwrap();
        fs::write(thermal.join("temp"), "61000\n").unwrap();

        let registry = SystemProviderRegistry::with_adapter(
            temporary.path(),
            Box::new(UnavailableProviderAdapter),
        );
        let providers = registry.snapshot_model().providers;
        assert_eq!(providers.abi_version, SYSTEM_PROVIDER_ABI_VERSION);
        assert!(providers.clock.unix_time_ms > 0);
        assert_eq!(providers.power.battery_percent, Some(73));
        assert_eq!(providers.power.charging, Some(true));
        assert_eq!(
            providers.power.thermal_status,
            Some(ThermalStatus::Moderate)
        );
        assert_eq!(providers.connectivity.online_interfaces, vec!["wlan0"]);
        assert!(providers.connectivity.wifi_enabled);
        assert!(providers.capabilities.is_empty());
    }

    #[test]
    fn actions_are_typed_bounded_and_capability_controlled() {
        let temporary = tempfile::tempdir().unwrap();
        let registry = SystemProviderRegistry::with_adapter(
            temporary.path(),
            Box::new(FixtureAdapter {
                snapshot: fixture_snapshot(),
                actions: Mutex::new(Vec::new()),
            }),
        );
        assert_eq!(
            registry
                .parse_and_authorize(&ProviderEffect {
                    provider: "audio".into(),
                    action: "set_volume".into(),
                    payload: json!({"percent": 75}),
                })
                .unwrap(),
            SystemAction::SetVolume(75)
        );
        assert_eq!(
            registry
                .parse_and_authorize(&ProviderEffect {
                    provider: "audio".into(),
                    action: "adjust_volume".into(),
                    payload: json!({"delta": -10}),
                })
                .unwrap(),
            SystemAction::AdjustVolume(-10)
        );
        for delta in [0, -101, 101] {
            assert!(registry
                .parse_and_authorize(&ProviderEffect {
                    provider: "audio".into(),
                    action: "adjust_volume".into(),
                    payload: json!({"delta": delta}),
                })
                .is_err());
        }
        assert!(registry
            .parse_and_authorize(&ProviderEffect {
                provider: "audio".into(),
                action: "set_volume".into(),
                payload: json!({"percent": 101}),
            })
            .is_err());
        assert!(registry
            .parse_and_authorize(&ProviderEffect {
                provider: "apps".into(),
                action: "launch".into(),
                payload: json!({"app_id": "app-1"}),
            })
            .is_err());
    }

    #[test]
    fn native_or_framework_adapter_can_supply_the_same_full_provider_abi() {
        let temporary = tempfile::tempdir().unwrap();
        let registry = SystemProviderRegistry::with_adapter(
            temporary.path(),
            Box::new(FixtureAdapter {
                snapshot: parity_snapshot(),
                actions: Mutex::new(Vec::new()),
            }),
        );
        let providers = registry.snapshot_model().providers;
        assert_eq!(providers.power.battery_percent, Some(72));
        assert!(providers.audio.media.active);
        assert_eq!(providers.apps.compatible[0].label, "Calculator");
        assert_eq!(providers.attention.items[0].id, "attention-1");
        assert!(providers
            .capabilities
            .contains(&SystemCapability::AttentionAcknowledge));
        assert!(!providers
            .capabilities
            .contains(&SystemCapability::RequestRestart));

        for (provider, action, payload) in [
            ("audio", "set_muted", json!({"muted": true})),
            ("media", "play_pause", json!({})),
            ("network", "connect", json!({"network_id": "network-1"})),
            ("apps", "launch", json!({"app_id": "app-1"})),
            (
                "attention",
                "acknowledge",
                json!({"attention_id": "attention-1"}),
            ),
        ] {
            registry
                .parse_and_authorize(&ProviderEffect {
                    provider: provider.into(),
                    action: action.into(),
                    payload,
                })
                .unwrap();
        }
        assert!(registry
            .parse_and_authorize(&ProviderEffect {
                provider: "power".into(),
                action: "request_restart".into(),
                payload: json!({}),
            })
            .is_err());
    }

    #[test]
    fn adapter_abi_mismatch_fails_closed_to_native_facts() {
        let temporary = tempfile::tempdir().unwrap();
        let mut incompatible = parity_snapshot();
        incompatible.abi_version = SYSTEM_PROVIDER_ABI_VERSION + 1;
        let registry = SystemProviderRegistry::with_adapter(
            temporary.path(),
            Box::new(FixtureAdapter {
                snapshot: incompatible,
                actions: Mutex::new(Vec::new()),
            }),
        );
        let providers = registry.snapshot_model().providers;
        assert!(providers.clock.unix_time_ms > 0);
        assert!(providers.apps.compatible.is_empty());
        assert!(providers.capabilities.is_empty());
        assert!(registry
            .parse_and_authorize(&ProviderEffect {
                provider: "audio".into(),
                action: "set_volume".into(),
                payload: json!({"percent": 50}),
            })
            .is_err());
    }
}
