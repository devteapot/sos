package dev.gpui.mobile;

import android.app.NotificationManager;
import android.content.ComponentName;
import android.content.Context;
import android.util.Log;

import androidx.core.app.NotificationManagerCompat;

import java.lang.reflect.Method;

/** Keeps the product-owned notification adapter enabled across no-wipe upgrades. */
final class SosAttentionPolicy {
    private static final String TAG = "SosAttentionPolicy";
    private static final ComponentName LISTENER = new ComponentName(
            "dev.sos.experience", "dev.gpui.mobile.SosAttentionListenerService");
    private static volatile boolean requestInFlight;

    static void enforce(Context context, String reason) {
        if (requestInFlight
                || NotificationManagerCompat.getEnabledListenerPackages(context)
                        .contains(context.getPackageName())) {
            return;
        }
        NotificationManager notifications = context.getSystemService(NotificationManager.class);
        if (notifications == null) return;
        requestInFlight = true;
        try {
            Method grant = NotificationManager.class.getMethod(
                    "setNotificationListenerAccessGranted",
                    ComponentName.class, boolean.class, boolean.class);
            grant.invoke(notifications, LISTENER, true, false);
            Log.i(TAG, "attention_listener_enforced reason=" + reason);
        } catch (ReflectiveOperationException | RuntimeException error) {
            Log.e(TAG, "attention_listener_enforcement_failed reason=" + reason, error);
        } finally {
            requestInFlight = false;
        }
    }

    private SosAttentionPolicy() {}
}
