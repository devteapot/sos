package dev.gpui.mobile;

import android.app.Service;
import android.app.Instrumentation;
import android.content.Context;
import android.content.Intent;
import android.graphics.Color;
import android.graphics.PixelFormat;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.os.BatteryManager;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.util.Log;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.View;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

import java.lang.reflect.Method;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;

import dev.sos.experience.BuildConfig;

/** Trusted Back/Apps/Attention/Exit controls displayed above compatibility tasks. */
public final class SosCompatChromeService extends Service {
    private static final String TAG = "SosCompatChrome";
    private static final int DISABLE_EXPAND = 0x00010000;
    private static final int DISABLE_NOTIFICATION_ICONS = 0x00020000;
    private static final int DISABLE_NOTIFICATION_ALERTS = 0x00040000;
    private static final int DISABLE_CLOCK = 0x00800000;
    private static final int DISABLE_SYSTEM_INFO = 0x00100000;
    private static final int DISABLE_HOME = 0x00200000;
    private static final int DISABLE_BACK = 0x00400000;
    private static final int DISABLE_RECENT = 0x01000000;
    private static final long STATUS_REFRESH_MS = 30_000;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable statusRefresh = new Runnable() {
        @Override
        public void run() {
            updateStatus();
            handler.postDelayed(this, STATUS_REFRESH_MS);
        }
    };
    private WindowManager windowManager;
    private View chrome;
    private TextView status;

    static void start(Context context) {
        if (!BuildConfig.SOS_COMPAT_ENABLED) return;
        context.startService(new Intent(context, SosCompatChromeService.class));
    }

    @Override
    public void onCreate() {
        super.onCreate();
        windowManager = getSystemService(WindowManager.class);
        installChrome();
        setAndroidNavigationDisabled(true);
        handler.post(statusRefresh);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (chrome == null) installChrome();
        setAndroidNavigationDisabled(true);
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        handler.removeCallbacks(statusRefresh);
        setAndroidNavigationDisabled(false);
        if (chrome != null && windowManager != null) {
            windowManager.removeView(chrome);
            chrome = null;
        }
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    private void installChrome() {
        if (chrome != null || windowManager == null) return;
        LinearLayout bar = new LinearLayout(this);
        bar.setOrientation(LinearLayout.VERTICAL);
        bar.setPadding(dp(6), dp(8), dp(6), dp(8));
        bar.setBackgroundColor(0xee17211b);
        status = new TextView(this);
        status.setTextColor(0xffd7e2db);
        status.setTextSize(10);
        status.setGravity(Gravity.CENTER);
        status.setContentDescription("SOS trusted time, network and battery status");
        status.setPadding(0, 0, 0, dp(8));
        bar.addView(status);
        bar.addView(button("BACK", view -> injectBack()));
        bar.addView(button("APPS", view -> open(SosCompatWorkspaceActivity.class)));
        bar.addView(button("ATTN", view -> open(SosAttentionActivity.class)));
        bar.addView(button("EXIT", view -> showSosHome()));

        WindowManager.LayoutParams params = new WindowManager.LayoutParams(
                dp(76), WindowManager.LayoutParams.WRAP_CONTENT,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                        | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                        | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
                PixelFormat.TRANSLUCENT);
        params.gravity = Gravity.END | Gravity.CENTER_VERTICAL;
        params.setTitle("SOS Compat Chrome");
        try {
            markAsSystemApplicationOverlay(params);
            windowManager.addView(bar, params);
            chrome = bar;
            Log.i(TAG, "compat_chrome_ready controls=back,apps,attention,exit");
        } catch (ReflectiveOperationException | RuntimeException error) {
            Log.e(TAG, "compat_chrome_failed", error);
        }
    }

    private void markAsSystemApplicationOverlay(WindowManager.LayoutParams params)
            throws ReflectiveOperationException {
        Method mark = WindowManager.LayoutParams.class.getMethod(
                "setSystemApplicationOverlay", boolean.class);
        mark.invoke(params, true);
        Log.i(TAG, "compat_chrome_trust=system_application_overlay");
    }

    private Button button(String label, View.OnClickListener listener) {
        Button button = new Button(this);
        button.setText(label);
        button.setTextColor(Color.WHITE);
        button.setTextSize(10);
        button.setAllCaps(false);
        button.setOnClickListener(listener);
        button.setMinHeight(dp(48));
        button.setContentDescription("SOS " + label.toLowerCase());
        return button;
    }

    private void open(Class<?> activity) {
        Intent intent = new Intent(this, activity)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        startActivity(intent);
    }

    private void showSosHome() {
        Intent home = new Intent(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_HOME)
                .setPackage(getPackageName())
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        startActivity(home);
    }

    private void injectBack() {
        new Thread(() -> {
            try {
                new Instrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);
                Log.i(TAG, "compat_chrome_action action=back");
            } catch (RuntimeException error) {
                Log.e(TAG, "compat_back_failed", error);
            }
        }, "sos-compat-back").start();
    }

    private void updateStatus() {
        if (status == null) return;
        String time = new SimpleDateFormat("HH:mm", Locale.getDefault()).format(new Date());
        BatteryManager battery = getSystemService(BatteryManager.class);
        int percent = battery == null ? -1
                : battery.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY);
        ConnectivityManager connectivity = getSystemService(ConnectivityManager.class);
        Network active = connectivity == null ? null : connectivity.getActiveNetwork();
        NetworkCapabilities capabilities = active == null || connectivity == null
                ? null : connectivity.getNetworkCapabilities(active);
        String network = "OFFLINE";
        if (capabilities != null) {
            if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) network = "VPN";
            else if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) network = "WIFI";
            else if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)) network = "CELL";
            else if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)) network = "LAN";
            else network = "NET";
        }
        status.setText(time + "\n" + network + "\n" + (percent < 0 ? "--" : percent + "%"));
    }

    private void setAndroidNavigationDisabled(boolean disabled) {
        Object statusBar = getSystemService("statusbar");
        if (statusBar == null) return;
        int flags = disabled
                ? DISABLE_EXPAND | DISABLE_NOTIFICATION_ICONS | DISABLE_NOTIFICATION_ALERTS
                        | DISABLE_CLOCK | DISABLE_SYSTEM_INFO | DISABLE_HOME | DISABLE_BACK
                        | DISABLE_RECENT
                : 0;
        try {
            Method disable = statusBar.getClass().getMethod("disable", int.class);
            disable.invoke(statusBar, flags);
            Log.i(TAG, "android_navigation_owner=" + (disabled ? "sos" : "android"));
        } catch (ReflectiveOperationException | RuntimeException error) {
            Log.e(TAG, "status_bar_policy_failed", error);
        }
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }
}
