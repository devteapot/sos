package dev.gpui.mobile;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.util.Log;

/** Receives bounded framework facts after Android's own error UI is suppressed. */
public final class SosSystemAttentionReceiver extends BroadcastReceiver {
    private static final String TAG = "SosSystemAttention";
    private static final String ACTION = "dev.sos.action.SYSTEM_ATTENTION";

    @Override
    public void onReceive(Context context, Intent intent) {
        if (intent == null || !ACTION.equals(intent.getAction())) return;
        String kind = bounded(intent.getStringExtra("kind"));
        String packageName = bounded(intent.getStringExtra("package"));
        String title = bounded(intent.getStringExtra("title"));
        String detail = bounded(intent.getStringExtra("detail"));
        long now = System.currentTimeMillis();
        SosAttentionStore.append(context, new SosAttentionStore.Event(
                now, "framework:" + kind + ":" + packageName + ":" + now,
                packageName, kind, true, title, detail));
        Log.i(TAG, "system_attention_persisted kind=" + kind
                + " package=" + packageName);
    }

    private static String bounded(String value) {
        if (value == null) return "unknown";
        String clean = value.replace('\n', ' ').replace('\r', ' ').trim();
        return clean.length() <= 256 ? clean : clean.substring(0, 256);
    }
}
