package dev.sos.frameworkbridge;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.debug.IAdbManager;
import android.os.IBinder;
import android.os.ServiceManager;
import android.os.SystemProperties;
import android.util.Slog;

import java.nio.charset.StandardCharsets;

/** Privileged half of the signature-bound ADB consent result path. */
public final class SosAdbConsentReceiver extends BroadcastReceiver {
    private static final String TAG = "SosAdbConsent";
    private static final String ACTION = "dev.sos.action.ADB_CONSENT_RESULT";
    private static final String DECISION_ALLOW_ONCE = "allow-once";
    private static final String DECISION_ALWAYS_ALLOW = "always-allow";
    private static final String DECISION_DENY = "deny";
    private static final int MAX_KEY_BYTES = 16 * 1024;

    @Override
    public void onReceive(Context context, Intent intent) {
        if (intent == null || !ACTION.equals(intent.getAction())
                || !"compat".equals(SystemProperties.get("ro.sos.core.stage", ""))) {
            deny("invalid-context");
            return;
        }
        String decision = intent.getStringExtra("decision");
        String key = intent.getStringExtra("key");
        if (DECISION_DENY.equals(decision)) {
            deny("user");
            return;
        }
        if ((!DECISION_ALLOW_ONCE.equals(decision)
                && !DECISION_ALWAYS_ALLOW.equals(decision)) || !validKey(key)) {
            deny("invalid-result");
            return;
        }
        try {
            adbManager().allowDebugging(DECISION_ALWAYS_ALLOW.equals(decision), key);
            Slog.i(TAG, "framework_adb_consent action=" + decision + " result=accepted");
        } catch (Exception error) {
            Slog.e(TAG, "framework_adb_consent action=" + decision + " result=failed", error);
            deny("allow-failed");
        }
    }

    private static void deny(String reason) {
        try {
            adbManager().denyDebugging();
            Slog.i(TAG, "framework_adb_consent action=deny result=accepted reason=" + reason);
        } catch (Exception error) {
            Slog.e(TAG, "framework_adb_consent action=deny result=failed reason=" + reason,
                    error);
        }
    }

    private static IAdbManager adbManager() {
        IBinder binder = ServiceManager.getService(Context.ADB_SERVICE);
        if (binder == null) throw new IllegalStateException("ADB service unavailable");
        return IAdbManager.Stub.asInterface(binder);
    }

    private static boolean validKey(String value) {
        return value != null && !value.isEmpty()
                && value.getBytes(StandardCharsets.UTF_8).length <= MAX_KEY_BYTES;
    }
}
