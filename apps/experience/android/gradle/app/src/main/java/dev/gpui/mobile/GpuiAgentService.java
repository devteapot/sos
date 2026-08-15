package dev.gpui.mobile;

import android.app.Activity;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.os.IBinder;

/** Keeps Android from reaping the bounded native Pi child during provider I/O. */
public final class GpuiAgentService extends Service {
    private static final String CHANNEL_ID = "sos_resident_agent";
    private static final int NOTIFICATION_ID = 0x534f53;

    public static void start(Activity activity) {
        activity.startForegroundService(new Intent(activity, GpuiAgentService.class));
    }

    public static void stop(Activity activity) {
        activity.stopService(new Intent(activity, GpuiAgentService.class));
    }

    @Override
    public void onCreate() {
        super.onCreate();
        NotificationManager manager = (NotificationManager) getSystemService(
                Context.NOTIFICATION_SERVICE);
        if (manager != null) {
            NotificationChannel channel = new NotificationChannel(CHANNEL_ID,
                    "SOS resident agent", NotificationManager.IMPORTANCE_LOW);
            channel.setDescription("Shown only while native on-device Pi is working");
            manager.createNotificationChannel(channel);
        }
        Notification notification = new Notification.Builder(this, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.stat_notify_sync)
                .setContentTitle("SOS resident agent")
                .setContentText("Native on-device Pi is working")
                .setCategory(Notification.CATEGORY_SERVICE)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .build();
        startForeground(NOTIFICATION_ID, notification);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        return START_NOT_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
