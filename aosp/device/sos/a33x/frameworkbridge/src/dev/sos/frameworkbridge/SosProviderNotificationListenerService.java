package dev.sos.frameworkbridge;

import android.app.Notification;
import android.content.Context;
import android.os.Bundle;
import android.service.notification.NotificationListenerService;
import android.service.notification.StatusBarNotification;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** Headless, bounded notification source for the attention provider. */
public final class SosProviderNotificationListenerService extends NotificationListenerService {
    private static final int MAX_ITEMS = 64;
    private static volatile SosProviderNotificationListenerService sInstance;

    @Override
    public void onListenerConnected() {
        sInstance = this;
    }

    @Override
    public void onListenerDisconnected() {
        if (sInstance == this) sInstance = null;
    }

    static JSONObject snapshot(Context context) throws Exception {
        JSONArray items = new JSONArray();
        int urgentCount = 0;
        SosProviderNotificationListenerService instance = sInstance;
        StatusBarNotification[] active = instance == null ? null : instance.getActiveNotifications();
        List<StatusBarNotification> ordered = new ArrayList<>();
        if (active != null) {
            for (StatusBarNotification notification : active) ordered.add(notification);
            ordered.sort(Comparator.comparingLong(StatusBarNotification::getPostTime).reversed());
        }
        for (StatusBarNotification posted : ordered) {
            if (items.length() >= MAX_ITEMS) break;
            Notification notification = posted.getNotification();
            Bundle extras = notification == null ? null : notification.extras;
            String kind = kind(notification);
            boolean urgent = notification != null
                    && (notification.fullScreenIntent != null
                    || notification.priority >= Notification.PRIORITY_HIGH
                    || "call".equals(kind) || "alarm".equals(kind));
            if (urgent) urgentCount++;
            items.put(new JSONObject()
                    .put("id", identifier(posted))
                    .put("occurred_at_ms", Math.max(0, posted.getPostTime()))
                    .put("source", SosSystemProviders.visibleSource(
                            context, posted.getPackageName()))
                    .put("kind", kind)
                    .put("urgent", urgent)
                    .put("title", visibleText(extras == null ? null
                            : extras.getCharSequence(Notification.EXTRA_TITLE)))
                    .put("detail", visibleText(extras == null ? null
                            : extras.getCharSequence(Notification.EXTRA_TEXT))));
        }
        return new JSONObject().put("items", items).put("urgent_count", urgentCount);
    }

    static boolean acknowledge(String requestedId) {
        SosProviderNotificationListenerService instance = sInstance;
        if (instance == null) return false;
        StatusBarNotification[] active = instance.getActiveNotifications();
        if (active == null) return false;
        for (StatusBarNotification posted : active) {
            if (!identifier(posted).equals(requestedId)) continue;
            instance.cancelNotification(posted.getKey());
            return true;
        }
        return false;
    }

    private static String identifier(StatusBarNotification posted) {
        return SosSystemProviders.opaque("attention", posted.getKey());
    }

    private static String kind(Notification notification) {
        String category = notification == null ? null : notification.category;
        if (Notification.CATEGORY_CALL.equals(category)) return "call";
        if (Notification.CATEGORY_ALARM.equals(category)) return "alarm";
        if (Notification.CATEGORY_MESSAGE.equals(category)
                || Notification.CATEGORY_SOCIAL.equals(category)) return "message";
        if (Notification.CATEGORY_TRANSPORT.equals(category)) return "media";
        if (Notification.CATEGORY_ERROR.equals(category)
                || Notification.CATEGORY_SYSTEM.equals(category)) return "system";
        if (Notification.CATEGORY_SERVICE.equals(category)
                || Notification.CATEGORY_PROGRESS.equals(category)) return "background";
        return "general";
    }

    private static String visibleText(CharSequence value) {
        if (value == null) return "";
        String clean = value.toString().replace('\n', ' ').replace('\r', ' ').trim();
        return clean.length() <= 512 ? clean : clean.substring(0, 512);
    }
}
