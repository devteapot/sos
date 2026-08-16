use experience_ir::{NetworkState, WifiSecurity};
#[cfg(not(feature = "core-native"))]
use gpui_mobile::android::jni::{activity, find_app_class, get_string, with_env};
#[cfg(not(feature = "core-native"))]
use jni::objects::{JObject, JValue};
#[cfg(not(feature = "core-native"))]
use jni::strings::JNIString;

#[cfg(not(feature = "core-native"))]
const HELPER_CLASS: &str = "dev.gpui.mobile.GpuiWifi";

#[cfg(feature = "core-native")]
pub fn snapshot() -> Result<NetworkState, String> {
    let interface = std::path::Path::new("/sys/class/net/wlan0");
    let wifi_enabled = interface.exists();
    let connected = std::fs::read_to_string(interface.join("operstate"))
        .map(|state| state.trim() == "up")
        .unwrap_or(false);
    Ok(NetworkState {
        wifi_enabled,
        connected,
        connected_ssid: None,
        // Link state is not Android's validated-network signal. Do not infer
        // Internet reachability until the native framework bridge supplies it.
        validated: false,
        signal_level: None,
        networks: Vec::new(),
        activity: "Native Wi-Fi link state".into(),
        error: Some("Wi-Fi scan and configuration await the Core framework bridge".into()),
    })
}

#[cfg(not(feature = "core-native"))]
pub fn snapshot() -> Result<NetworkState, String> {
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let result = env
            .call_static_method(
                &helper,
                jni::jni_str!("snapshot"),
                jni::jni_sig!("(Landroid/app/Activity;)Ljava/lang/String;"),
                &[JValue::Object(&activity)],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if result.is_null() {
            return Err("Wi-Fi snapshot returned no data".into());
        }
        serde_json::from_str(&get_string(env, &result))
            .map_err(|error| format!("decode Wi-Fi snapshot: {error}"))
    })
}

#[cfg(feature = "core-native")]
pub fn refresh() -> Result<(), String> {
    Err(core_action_unavailable())
}

#[cfg(not(feature = "core-native"))]
pub fn refresh() -> Result<(), String> {
    call_bool("refresh")
}

#[cfg(feature = "core-native")]
pub fn connect(_ssid: &str, _security: WifiSecurity) -> Result<(), String> {
    Err(core_action_unavailable())
}

#[cfg(not(feature = "core-native"))]
pub fn connect(ssid: &str, security: WifiSecurity) -> Result<(), String> {
    let security = match security {
        WifiSecurity::Open => "open",
        WifiSecurity::Personal => "personal",
        WifiSecurity::Enterprise => "enterprise",
    };
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let ssid = JObject::from(env.new_string(ssid).map_err(|error| error.to_string())?);
        let security = JObject::from(
            env.new_string(security)
                .map_err(|error| error.to_string())?,
        );
        let ok = env
            .call_static_method(
                &helper,
                jni::jni_str!("connect"),
                jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)Z"),
                &[
                    JValue::Object(&activity),
                    JValue::Object(&ssid),
                    JValue::Object(&security),
                ],
            )
            .and_then(|value| value.z())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        ok.then_some(())
            .ok_or_else(|| "trusted Wi-Fi helper rejected connection request".into())
    })
}

#[cfg(feature = "core-native")]
pub fn disconnect() -> Result<(), String> {
    Err(core_action_unavailable())
}

#[cfg(not(feature = "core-native"))]
pub fn disconnect() -> Result<(), String> {
    call_bool("disconnect")
}

#[cfg(feature = "core-native")]
fn core_action_unavailable() -> String {
    "Core network changes require the trusted native framework bridge".into()
}

#[cfg(not(feature = "core-native"))]
fn call_bool(method: &str) -> Result<(), String> {
    with_env(|env| {
        let helper = find_app_class(env, HELPER_CLASS)?;
        let activity = activity(env)?;
        let method_name = JNIString::new(method);
        let ok = env
            .call_static_method(
                &helper,
                &method_name,
                jni::jni_sig!("(Landroid/app/Activity;)Z"),
                &[JValue::Object(&activity)],
            )
            .and_then(|value| value.z())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        ok.then_some(())
            .ok_or_else(|| format!("trusted Wi-Fi helper rejected {method}"))
    })
}
