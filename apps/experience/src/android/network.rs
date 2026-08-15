use experience_ir::{NetworkState, WifiSecurity};
use gpui_mobile::android::jni::{activity, find_app_class, get_string, with_env};
use jni::objects::{JObject, JValue};
use jni::strings::JNIString;

const HELPER_CLASS: &str = "dev.gpui.mobile.GpuiWifi";

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

pub fn refresh() -> Result<(), String> {
    call_bool("refresh")
}

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

pub fn disconnect() -> Result<(), String> {
    call_bool("disconnect")
}

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
