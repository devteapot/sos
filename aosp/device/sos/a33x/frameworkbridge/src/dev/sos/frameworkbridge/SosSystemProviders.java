package dev.sos.frameworkbridge;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.media.AudioManager;
import android.media.MediaMetadata;
import android.media.session.MediaController;
import android.media.session.MediaSessionManager;
import android.media.session.PlaybackState;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.wifi.ScanResult;
import android.net.wifi.WifiConfiguration;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiManager;
import android.os.BatteryManager;
import android.os.Build;
import android.os.PowerManager;
import android.text.format.DateFormat;

import org.json.JSONArray;
import org.json.JSONObject;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.text.Collator;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Date;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TimeZone;

/** Narrow framework-only half of System Providers v1. */
final class SosSystemProviders {
    private static final int ABI_VERSION = 1;
    private static final int MAX_APPS = 256;
    private static final int MAX_NETWORKS = 64;
    private static final int MAX_LABEL = 256;
    private static final ComponentName ATTENTION_COMPONENT = new ComponentName(
            "dev.sos.frameworkbridge",
            "dev.sos.frameworkbridge.SosProviderNotificationListenerService");

    static String snapshot(Context context) throws Exception {
        long now = System.currentTimeMillis();
        JSONObject root = new JSONObject();
        root.put("abi_version", ABI_VERSION);
        root.put("observed_at_ms", now);
        root.put("clock", clock(context, now));
        root.put("power", power(context));
        JSONObject connectivity = connectivity(context);
        root.put("connectivity", connectivity);
        JSONObject audio = audio(context);
        root.put("audio", audio);
        JSONArray applications = applications(context);
        root.put("apps", new JSONObject().put("compatible", applications));
        JSONObject attention = SosProviderNotificationListenerService.snapshot(context);
        root.put("attention", attention);

        JSONArray capabilities = new JSONArray();
        capabilities.put("audio_set_volume");
        capabilities.put("audio_set_muted");
        if (audio.getJSONObject("media").getBoolean("active")) {
            capabilities.put("media_play_pause");
            capabilities.put("media_next");
            capabilities.put("media_previous");
        }
        if (connectivity.getBoolean("connected")) {
            capabilities.put("wifi_disconnect");
        }
        if (connectivity.getJSONArray("wifi_networks").length() > 0) {
            capabilities.put("wifi_connect");
        }
        if (applications.length() > 0) capabilities.put("app_launch");
        if (attention.getJSONArray("items").length() > 0) {
            capabilities.put("attention_acknowledge");
        }
        root.put("capabilities", capabilities);
        return root.toString();
    }

    static String execute(Context context, String encoded) throws Exception {
        JSONObject request = new JSONObject(encoded);
        String provider = request.optString("provider");
        String action = request.optString("action");
        JSONObject payload = request.optJSONObject("payload");
        if (payload == null) payload = new JSONObject();
        boolean accepted;
        if ("audio".equals(provider) && "set_volume".equals(action)) {
            int percent = payload.getInt("percent");
            if (percent < 0 || percent > 100) throw new IllegalArgumentException("bad percent");
            AudioManager audio = context.getSystemService(AudioManager.class);
            if (audio == null) throw new IllegalStateException("audio service unavailable");
            int maximum = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC);
            int volume = Math.round(maximum * percent / 100.0f);
            audio.setStreamVolume(AudioManager.STREAM_MUSIC, volume, 0);
            accepted = true;
        } else if ("audio".equals(provider) && "set_muted".equals(action)) {
            AudioManager audio = context.getSystemService(AudioManager.class);
            if (audio == null) throw new IllegalStateException("audio service unavailable");
            audio.adjustStreamVolume(AudioManager.STREAM_MUSIC,
                    payload.getBoolean("muted")
                            ? AudioManager.ADJUST_MUTE : AudioManager.ADJUST_UNMUTE, 0);
            accepted = true;
        } else if ("media".equals(provider)) {
            accepted = controlMedia(context, action);
        } else if ("network".equals(provider) && "disconnect".equals(action)) {
            WifiManager wifi = context.getSystemService(WifiManager.class);
            accepted = wifi != null && wifi.disconnect();
        } else if ("network".equals(provider) && "connect".equals(action)) {
            accepted = connectWifi(context, payload.getString("network_id"));
        } else if ("apps".equals(provider) && "launch".equals(action)) {
            accepted = launchApplication(context, payload.getString("app_id"));
        } else if ("attention".equals(provider) && "acknowledge".equals(action)) {
            accepted = SosProviderNotificationListenerService.acknowledge(
                    payload.getString("attention_id"));
        } else {
            throw new IllegalArgumentException("unsupported provider action");
        }
        if (!accepted) throw new IllegalStateException("provider action was not accepted");
        return new JSONObject().put("accepted", true).toString();
    }

    private static JSONObject clock(Context context, long now) throws Exception {
        java.text.DateFormat time = DateFormat.getTimeFormat(context);
        java.text.DateFormat date = DateFormat.getLongDateFormat(context);
        return new JSONObject()
                .put("unix_time_ms", now)
                .put("locale", Locale.getDefault().toLanguageTag())
                .put("timezone", TimeZone.getDefault().getID())
                .put("time_label", bounded(time.format(new Date(now))))
                .put("date_label", bounded(date.format(new Date(now))));
    }

    private static JSONObject power(Context context) throws Exception {
        Integer percent = null;
        Boolean charging = null;
        String source = "";
        Integer temperature = null;
        Intent battery = context.registerReceiver(null,
                new IntentFilter(Intent.ACTION_BATTERY_CHANGED));
        if (battery != null) {
            int level = battery.getIntExtra(BatteryManager.EXTRA_LEVEL, -1);
            int scale = battery.getIntExtra(BatteryManager.EXTRA_SCALE, -1);
            if (level >= 0 && scale > 0) percent = Math.min(100, level * 100 / scale);
            int status = battery.getIntExtra(BatteryManager.EXTRA_STATUS, -1);
            charging = status == BatteryManager.BATTERY_STATUS_CHARGING
                    || status == BatteryManager.BATTERY_STATUS_FULL;
            int plugged = battery.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0);
            if (plugged == BatteryManager.BATTERY_PLUGGED_AC) source = "ac";
            else if (plugged == BatteryManager.BATTERY_PLUGGED_USB) source = "usb";
            else if (plugged == BatteryManager.BATTERY_PLUGGED_WIRELESS) source = "wireless";
            int rawTemperature = battery.getIntExtra(BatteryManager.EXTRA_TEMPERATURE,
                    Integer.MIN_VALUE);
            if (rawTemperature != Integer.MIN_VALUE) temperature = rawTemperature;
        }
        String thermal = null;
        PowerManager power = context.getSystemService(PowerManager.class);
        if (power != null) thermal = thermalStatus(power.getCurrentThermalStatus());
        return new JSONObject()
                .put("battery_percent", nullable(percent))
                .put("charging", nullable(charging))
                .put("charging_source", source)
                .put("battery_temperature_deci_c", nullable(temperature))
                .put("thermal_status", nullable(thermal));
    }

    private static JSONObject connectivity(Context context) throws Exception {
        ConnectivityManager connectivity = context.getSystemService(ConnectivityManager.class);
        WifiManager wifi = context.getSystemService(WifiManager.class);
        Network active = connectivity == null ? null : connectivity.getActiveNetwork();
        NetworkCapabilities capabilities = active == null || connectivity == null
                ? null : connectivity.getNetworkCapabilities(active);
        boolean connected = capabilities != null;
        boolean validated = capabilities != null
                && capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED);
        String transport = "";
        if (capabilities != null) {
            if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) transport = "wifi";
            else if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)) {
                transport = "cellular";
            } else if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)) {
                transport = "ethernet";
            } else transport = "network";
        }
        WifiInfo info = wifi == null ? null : wifi.getConnectionInfo();
        String connectedSsid = info == null ? "" : cleanSsid(info.getSSID());
        Integer signal = info == null || info.getNetworkId() < 0 ? null : signalLevel(info.getRssi());
        JSONArray networks = wifiNetworks(wifi, info);
        JSONArray interfaces = new JSONArray();
        return new JSONObject()
                .put("wifi_enabled", wifi != null && wifi.isWifiEnabled())
                .put("connected", connected)
                .put("validated", validated)
                .put("transport", transport)
                .put("network_label", bounded(connectedSsid))
                .put("signal_level", nullable(signal))
                .put("online_interfaces", interfaces)
                .put("wifi_networks", networks);
    }

    private static JSONArray wifiNetworks(WifiManager wifi, WifiInfo connected) throws Exception {
        JSONArray result = new JSONArray();
        if (wifi == null) return result;
        List<WifiConfiguration> configured = wifi.getConfiguredNetworks();
        if (configured == null) return result;
        Map<String, Integer> levels = new HashMap<>();
        List<ScanResult> scans = wifi.getScanResults();
        if (scans != null) {
            for (ScanResult scan : scans) {
                String label = cleanSsid(scan.SSID);
                if (!label.isEmpty()) {
                    levels.put(label, Math.max(levels.getOrDefault(label, 0),
                            signalLevel(scan.level)));
                }
            }
        }
        configured.sort(Comparator.comparing(
                configuration -> cleanSsid(configuration.SSID), String.CASE_INSENSITIVE_ORDER));
        Set<String> seenIds = new HashSet<>();
        for (WifiConfiguration configuration : configured) {
            if (result.length() >= MAX_NETWORKS) break;
            String label = cleanSsid(configuration.SSID);
            if (label.isEmpty()) continue;
            String id = networkId(configuration);
            // Some framework builds return the same saved configuration more
            // than once. The provider ABI promises stable, unique opaque IDs.
            if (!seenIds.add(id)) continue;
            boolean selected = connected != null
                    && connected.getNetworkId() == configuration.networkId;
            result.put(new JSONObject()
                    .put("id", id)
                    .put("label", bounded(label))
                    .put("signal_level", levels.getOrDefault(label, selected ? 3 : 1))
                    .put("saved", true)
                    .put("connected", selected));
        }
        return result;
    }

    private static JSONObject audio(Context context) throws Exception {
        AudioManager audio = context.getSystemService(AudioManager.class);
        Integer volume = null;
        Boolean muted = null;
        if (audio != null) {
            int maximum = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC);
            if (maximum > 0) {
                volume = Math.min(100, Math.max(0,
                        audio.getStreamVolume(AudioManager.STREAM_MUSIC) * 100 / maximum));
            }
            muted = audio.isStreamMute(AudioManager.STREAM_MUSIC);
        }
        JSONObject media = new JSONObject()
                .put("active", false)
                .put("playing", false)
                .put("title", "")
                .put("artist", "");
        MediaController controller = activeMediaController(context);
        if (controller != null) {
            PlaybackState playback = controller.getPlaybackState();
            MediaMetadata metadata = controller.getMetadata();
            media.put("active", true);
            media.put("playing", playback != null
                    && playback.getState() == PlaybackState.STATE_PLAYING);
            if (metadata != null) {
                media.put("title", bounded(metadata.getString(MediaMetadata.METADATA_KEY_TITLE)));
                media.put("artist", bounded(metadata.getString(MediaMetadata.METADATA_KEY_ARTIST)));
            }
        }
        return new JSONObject()
                .put("volume_percent", nullable(volume))
                .put("muted", nullable(muted))
                .put("media", media);
    }

    private static JSONArray applications(Context context) throws Exception {
        JSONArray result = new JSONArray();
        for (ApplicationEntry entry : applicationEntries(context)) {
            if (result.length() >= MAX_APPS) break;
            result.put(new JSONObject().put("id", entry.id).put("label", entry.label));
        }
        return result;
    }

    private static List<ApplicationEntry> applicationEntries(Context context) throws Exception {
        PackageManager packages = context.getPackageManager();
        Intent launcher = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
        List<ResolveInfo> candidates = new ArrayList<>(
                packages.queryIntentActivities(launcher, PackageManager.MATCH_ALL));
        List<ApplicationEntry> result = new ArrayList<>();
        for (ResolveInfo candidate : candidates) {
            if (candidate.activityInfo == null || !candidate.activityInfo.exported
                    || candidate.activityInfo.applicationInfo == null) continue;
            ApplicationInfo application = candidate.activityInfo.applicationInfo;
            if ((application.flags
                    & (ApplicationInfo.FLAG_SYSTEM | ApplicationInfo.FLAG_UPDATED_SYSTEM_APP)) != 0
                    || application.targetSdkVersion < Build.VERSION_CODES.M
                    || "dev.sos.experience".equals(candidate.activityInfo.packageName)
                    || context.getPackageName().equals(candidate.activityInfo.packageName)) {
                continue;
            }
            String label = bounded(String.valueOf(candidate.loadLabel(packages)));
            if (label.isEmpty()) label = "Compatible application";
            String identity = candidate.activityInfo.packageName + "/" + candidate.activityInfo.name;
            result.add(new ApplicationEntry(opaque("app", identity), label,
                    candidate.activityInfo.packageName, candidate.activityInfo.name));
        }
        Collator collator = Collator.getInstance();
        result.sort((left, right) -> collator.compare(left.label, right.label));
        return result;
    }

    private static boolean launchApplication(Context context, String requestedId) throws Exception {
        for (ApplicationEntry entry : applicationEntries(context)) {
            if (!entry.id.equals(requestedId)) continue;
            Intent launch = new Intent(Intent.ACTION_MAIN)
                    .addCategory(Intent.CATEGORY_LAUNCHER)
                    .setClassName(entry.packageName, entry.activityName)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                            | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
            context.startActivity(launch);
            return true;
        }
        return false;
    }

    private static boolean connectWifi(Context context, String requestedId) throws Exception {
        WifiManager wifi = context.getSystemService(WifiManager.class);
        if (wifi == null) return false;
        List<WifiConfiguration> configured = wifi.getConfiguredNetworks();
        if (configured == null) return false;
        for (WifiConfiguration configuration : configured) {
            if (!networkId(configuration).equals(requestedId)) continue;
            return wifi.enableNetwork(configuration.networkId, true) && wifi.reconnect();
        }
        return false;
    }

    private static boolean controlMedia(Context context, String action) {
        MediaController controller = activeMediaController(context);
        if (controller == null) return false;
        MediaController.TransportControls controls = controller.getTransportControls();
        if ("play_pause".equals(action)) {
            PlaybackState playback = controller.getPlaybackState();
            if (playback != null && playback.getState() == PlaybackState.STATE_PLAYING) {
                controls.pause();
            } else controls.play();
        } else if ("next".equals(action)) controls.skipToNext();
        else if ("previous".equals(action)) controls.skipToPrevious();
        else return false;
        return true;
    }

    private static MediaController activeMediaController(Context context) {
        MediaSessionManager sessions = context.getSystemService(MediaSessionManager.class);
        if (sessions == null) return null;
        try {
            List<MediaController> controllers = sessions.getActiveSessions(ATTENTION_COMPONENT);
            if (controllers == null || controllers.isEmpty()) return null;
            for (MediaController controller : controllers) {
                PlaybackState playback = controller.getPlaybackState();
                if (playback != null && playback.getState() == PlaybackState.STATE_PLAYING) {
                    return controller;
                }
            }
            return controllers.get(0);
        } catch (SecurityException error) {
            return null;
        }
    }

    static String visibleSource(Context context, String packageName) {
        if (packageName == null || packageName.isEmpty() || "android".equals(packageName)) {
            return "SOS RUNTIME";
        }
        try {
            PackageManager packages = context.getPackageManager();
            ApplicationInfo application = packages.getApplicationInfo(packageName, 0);
            String label = bounded(String.valueOf(packages.getApplicationLabel(application)));
            if (!label.isEmpty() && !looksLikePackage(label)) return label;
        } catch (PackageManager.NameNotFoundException ignored) {
        }
        return "COMPATIBILITY APP";
    }

    static String opaque(String kind, String value) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            byte[] hash = digest.digest(value.getBytes(StandardCharsets.UTF_8));
            StringBuilder encoded = new StringBuilder(kind).append('-');
            for (int index = 0; index < 12; index++) {
                encoded.append(String.format(Locale.US, "%02x", hash[index] & 0xff));
            }
            return encoded.toString();
        } catch (Exception error) {
            throw new IllegalStateException(error);
        }
    }

    private static String networkId(WifiConfiguration configuration) {
        return opaque("network", configuration.networkId + ":" + cleanSsid(configuration.SSID));
    }

    private static String cleanSsid(String value) {
        if (value == null || WifiManager.UNKNOWN_SSID.equals(value)) return "";
        String clean = value.trim();
        if (clean.length() >= 2 && clean.startsWith("\"") && clean.endsWith("\"")) {
            clean = clean.substring(1, clean.length() - 1);
        }
        return bounded(clean);
    }

    private static int signalLevel(int rssi) {
        if (rssi >= -55) return 4;
        if (rssi >= -65) return 3;
        if (rssi >= -75) return 2;
        return 1;
    }

    private static String thermalStatus(int status) {
        switch (status) {
            case PowerManager.THERMAL_STATUS_NONE: return "none";
            case PowerManager.THERMAL_STATUS_LIGHT: return "light";
            case PowerManager.THERMAL_STATUS_MODERATE: return "moderate";
            case PowerManager.THERMAL_STATUS_SEVERE: return "severe";
            case PowerManager.THERMAL_STATUS_CRITICAL: return "critical";
            case PowerManager.THERMAL_STATUS_EMERGENCY: return "emergency";
            case PowerManager.THERMAL_STATUS_SHUTDOWN: return "shutdown";
            default: return null;
        }
    }

    private static Object nullable(Object value) {
        return value == null ? JSONObject.NULL : value;
    }

    private static String bounded(String value) {
        if (value == null) return "";
        String clean = value.replace('\n', ' ').replace('\r', ' ').trim();
        return clean.length() <= MAX_LABEL ? clean : clean.substring(0, MAX_LABEL);
    }

    private static boolean looksLikePackage(String value) {
        if (value.indexOf(' ') >= 0 || value.indexOf('.') <= 0) return false;
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            if (!(Character.isLetterOrDigit(character) || character == '.'
                    || character == '_')) return false;
        }
        return true;
    }

    private static final class ApplicationEntry {
        final String id;
        final String label;
        final String packageName;
        final String activityName;

        ApplicationEntry(String id, String label, String packageName, String activityName) {
            this.id = id;
            this.label = label;
            this.packageName = packageName;
            this.activityName = activityName;
        }
    }

    private SosSystemProviders() {}
}
