package dev.gpui.mobile;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** Boot/package hook for the product HOME invariant. */
public final class SosBootReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context context, Intent intent) {
        SosHomePolicy.enforce(context, intent == null ? "broadcast" : intent.getAction());
    }
}
