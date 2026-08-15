package dev.gpui.mobile;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Context;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.wifi.ScanResult;
import android.net.wifi.WifiConfiguration;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiManager;
import android.text.InputType;
import android.widget.EditText;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** Trusted Wi-Fi bridge. Passwords remain in this Java process and never cross JNI. */
public final class GpuiWifi {
    private static final int MAX_NETWORKS = 20;
    private static volatile String sActivity = "";
    private static volatile String sError;

    public static String snapshot(Activity activity) {
        JSONObject root = new JSONObject();
        JSONArray networks = new JSONArray();
        try {
            WifiManager wifi = activity.getSystemService(WifiManager.class);
            ConnectivityManager connectivity =
                    activity.getSystemService(ConnectivityManager.class);
            boolean enabled = wifi != null && wifi.isWifiEnabled();
            boolean connected = false;
            boolean validated = false;
            String connectedSsid = null;
            int connectedLevel = -1;

            if (connectivity != null) {
                Network active = connectivity.getActiveNetwork();
                NetworkCapabilities capabilities =
                        active == null ? null : connectivity.getNetworkCapabilities(active);
                connected = capabilities != null
                        && capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI);
                validated = connected
                        && capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED);
            }
            if (wifi != null && connected) {
                WifiInfo info = wifi.getConnectionInfo();
                if (info != null) {
                    connectedSsid = unquote(info.getSSID());
                    if (connectedSsid.isEmpty()
                            || WifiManager.UNKNOWN_SSID.equals(connectedSsid)) {
                        connectedSsid = null;
                    }
                    connectedLevel = WifiManager.calculateSignalLevel(info.getRssi(), 5);
                }
            }

            Set<String> saved = new HashSet<>();
            if (wifi != null) {
                List<WifiConfiguration> configured = wifi.getConfiguredNetworks();
                if (configured != null) {
                    for (WifiConfiguration config : configured) {
                        String ssid = unquote(config.SSID);
                        if (!ssid.isEmpty()) saved.add(ssid);
                    }
                }
            }

            Map<String, ScanResult> strongest = new HashMap<>();
            if (wifi != null) {
                List<ScanResult> results = wifi.getScanResults();
                if (results != null) {
                    for (ScanResult result : results) {
                        String ssid = result.SSID == null ? "" : result.SSID.trim();
                        if (ssid.isEmpty()) continue;
                        ScanResult previous = strongest.get(ssid);
                        if (previous == null || result.level > previous.level) {
                            strongest.put(ssid, result);
                        }
                    }
                }
            }
            List<ScanResult> ordered = new ArrayList<>(strongest.values());
            Collections.sort(ordered, Comparator.comparingInt((ScanResult value) -> value.level)
                    .reversed());
            int count = 0;
            for (ScanResult result : ordered) {
                if (count++ >= MAX_NETWORKS) break;
                String ssid = result.SSID.trim();
                JSONObject item = new JSONObject();
                item.put("ssid", ssid);
                item.put("signal_level", WifiManager.calculateSignalLevel(result.level, 5));
                item.put("security", security(result.capabilities));
                item.put("saved", saved.contains(ssid));
                networks.put(item);
            }

            root.put("wifi_enabled", enabled);
            root.put("connected", connected);
            if (connectedSsid != null) root.put("connected_ssid", connectedSsid);
            root.put("validated", validated);
            if (connectedLevel >= 0) root.put("signal_level", connectedLevel);
            root.put("networks", networks);
            root.put("activity", sActivity == null ? "" : sActivity);
            if (sError != null && !sError.isEmpty()) root.put("error", sError);
        } catch (Exception error) {
            try {
                root.put("wifi_enabled", false);
                root.put("connected", false);
                root.put("validated", false);
                root.put("networks", networks);
                root.put("activity", "Unavailable");
                root.put("error", safeError(error));
            } catch (Exception ignored) {
                return "{\"wifi_enabled\":false,\"connected\":false,\"validated\":false,"
                        + "\"networks\":[],\"activity\":\"Unavailable\","
                        + "\"error\":\"Wi-Fi snapshot failed\"}";
            }
        }
        return root.toString();
    }

    public static boolean refresh(Activity activity) {
        try {
            WifiManager wifi = activity.getSystemService(WifiManager.class);
            if (wifi == null) return fail("Wi-Fi service unavailable");
            sError = null;
            sActivity = wifi.startScan() ? "Refreshing networks" : "Using recent scan";
            return true;
        } catch (Exception error) {
            return fail(safeError(error));
        }
    }

    public static boolean connect(Activity activity, String ssid, String security) {
        if (!validSsid(ssid)) return fail("Invalid network name");
        if (!("open".equals(security) || "personal".equals(security))) {
            return fail("Enterprise networks are not supported yet");
        }
        try {
            WifiManager wifi = activity.getSystemService(WifiManager.class);
            if (wifi == null) return fail("Wi-Fi service unavailable");
            for (WifiConfiguration configured : wifi.getConfiguredNetworks()) {
                if (ssid.equals(unquote(configured.SSID))) {
                    sError = null;
                    sActivity = "Connecting";
                    if (!wifi.enableNetwork(configured.networkId, true)) {
                        return fail("Saved network could not be enabled");
                    }
                    return true;
                }
            }
            if ("open".equals(security)) {
                return addAndConnect(wifi, ssid, null);
            }
            activity.runOnUiThread(() -> showPasswordDialog(activity, wifi, ssid));
            sError = null;
            sActivity = "Password required";
            return true;
        } catch (Exception error) {
            return fail(safeError(error));
        }
    }

    public static boolean disconnect(Activity activity) {
        try {
            WifiManager wifi = activity.getSystemService(WifiManager.class);
            if (wifi == null) return fail("Wi-Fi service unavailable");
            activity.runOnUiThread(() -> new AlertDialog.Builder(activity)
                    .setTitle("Disconnect Wi-Fi?")
                    .setMessage("The resident agent will be offline until another network connects.")
                    .setNegativeButton("Cancel", null)
                    .setPositiveButton("Disconnect", (dialog, which) -> {
                        if (wifi.disconnect()) {
                            sError = null;
                            sActivity = "Disconnected";
                        } else {
                            fail("Wi-Fi did not disconnect");
                        }
                    })
                    .show());
            return true;
        } catch (Exception error) {
            return fail(safeError(error));
        }
    }

    private static void showPasswordDialog(Activity activity, WifiManager wifi, String ssid) {
        EditText password = new EditText(activity);
        password.setSingleLine(true);
        password.setHint("Wi-Fi password");
        password.setSaveEnabled(false);
        password.setImportantForAutofill(EditText.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        password.setInputType(InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_VARIATION_PASSWORD);
        AlertDialog dialog = new AlertDialog.Builder(activity)
                .setTitle("Connect to " + ssid)
                .setView(password)
                .setNegativeButton("Cancel", (ignored, which) -> {
                    password.getText().clear();
                    sActivity = "Connection cancelled";
                })
                .setPositiveButton("Connect", (ignored, which) -> {
                    String secret = password.getText().toString();
                    password.getText().clear();
                    if (secret.length() < 8 || secret.length() > 63) {
                        fail("Wi-Fi password must contain 8 to 63 characters");
                        return;
                    }
                    addAndConnect(wifi, ssid, secret);
                })
                .create();
        dialog.setOnDismissListener(ignored -> password.getText().clear());
        dialog.show();
    }

    private static boolean addAndConnect(WifiManager wifi, String ssid, String password) {
        try {
            WifiConfiguration config = new WifiConfiguration();
            config.SSID = quote(ssid);
            if (password == null) {
                config.allowedKeyManagement.set(WifiConfiguration.KeyMgmt.NONE);
            } else {
                config.preSharedKey = quote(password);
                config.allowedKeyManagement.set(WifiConfiguration.KeyMgmt.WPA_PSK);
            }
            WifiManager.AddNetworkResult result = wifi.addNetworkPrivileged(config);
            if (result.statusCode != WifiManager.AddNetworkResult.STATUS_SUCCESS
                    || result.networkId < 0) {
                return fail("Wi-Fi rejected the network configuration ("
                        + result.statusCode + ")");
            }
            if (!wifi.enableNetwork(result.networkId, true)) {
                return fail("Wi-Fi accepted but could not enable the network");
            }
            sError = null;
            sActivity = "Connecting";
            return true;
        } catch (Exception error) {
            return fail(safeError(error));
        }
    }

    private static String security(String capabilities) {
        String value = capabilities == null ? "" : capabilities;
        if (value.contains("EAP") || value.contains("WAPI-CERT")) return "enterprise";
        if (value.contains("PSK") || value.contains("SAE") || value.contains("WEP")
                || value.contains("WAPI-PSK")) return "personal";
        return "open";
    }

    private static boolean validSsid(String ssid) {
        if (ssid == null || ssid.isEmpty() || ssid.length() > 64) return false;
        return ssid.indexOf('\n') < 0 && ssid.indexOf('\r') < 0 && ssid.indexOf('\0') < 0;
    }

    private static String quote(String value) {
        return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    private static String unquote(String value) {
        if (value == null) return "";
        if (value.length() >= 2 && value.startsWith("\"") && value.endsWith("\"")) {
            return value.substring(1, value.length() - 1);
        }
        return value;
    }

    private static boolean fail(String message) {
        sActivity = "Wi-Fi action failed";
        sError = message;
        return false;
    }

    private static String safeError(Exception error) {
        if (error instanceof SecurityException) return "Wi-Fi permission denied";
        return "Wi-Fi operation failed";
    }

    private GpuiWifi() {}
}
