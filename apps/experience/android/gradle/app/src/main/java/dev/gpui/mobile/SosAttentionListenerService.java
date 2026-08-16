package dev.gpui.mobile;

import android.app.Notification;
import android.os.Bundle;
import android.service.notification.NotificationListenerService;
import android.service.notification.StatusBarNotification;
import android.util.Log;

/** Converts Android notifications into durable, typed SOS attention events. */
public final class SosAttentionListenerService extends NotificationListenerService {
    private static final String TAG = "SosAttentionBroker";

    @Override
    public void onListenerConnected() {
        super.onListenerConnected();
        Log.i(TAG, "attention_broker_ready durable=true max_events="
                + SosAttentionStore.MAX_EVENTS);
    }

    @Override
    public void onNotificationPosted(StatusBarNotification posted) {
        if (posted == null || getPackageName().equals(posted.getPackageName())) return;
        Notification notification = posted.getNotification();
        if (notification == null) return;
        String kind = classify(notification, posted.getPackageName());
        boolean urgent = "call".equals(kind) || "alarm".equals(kind)
                || "security".equals(kind);
        Bundle extras = notification.extras;
        String title = text(extras == null ? null : extras.getCharSequence(Notification.EXTRA_TITLE));
        String detail = text(extras == null ? null : extras.getCharSequence(Notification.EXTRA_TEXT));
        SosAttentionStore.append(this, new SosAttentionStore.Event(
                posted.getPostTime(), posted.getKey(), posted.getPackageName(), kind, urgent,
                title.isEmpty() ? posted.getPackageName() : title, detail));
        Log.i(TAG, "attention_event_persisted kind=" + kind + " urgent=" + urgent
                + " package=" + posted.getPackageName());
    }

    private static String classify(Notification notification, String packageName) {
        String category = notification.category;
        if (Notification.CATEGORY_CALL.equals(category)) return "call";
        if (Notification.CATEGORY_ALARM.equals(category)) return "alarm";
        if (Notification.CATEGORY_ERROR.equals(category)
                || Notification.CATEGORY_SYSTEM.equals(category)
                || "android".equals(packageName)) return "security";
        if (Notification.CATEGORY_TRANSPORT.equals(category)) return "media";
        if (Notification.CATEGORY_MESSAGE.equals(category)
                || Notification.CATEGORY_SOCIAL.equals(category)) return "message";
        if (Notification.CATEGORY_SERVICE.equals(category)
                || Notification.CATEGORY_PROGRESS.equals(category)) return "background";
        return "general";
    }

    private static String text(CharSequence value) {
        return value == null ? "" : value.toString();
    }
}
