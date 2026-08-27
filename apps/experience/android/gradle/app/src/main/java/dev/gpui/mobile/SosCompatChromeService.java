package dev.gpui.mobile;

import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.graphics.Canvas;
import android.graphics.PixelFormat;
import android.graphics.RectF;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.os.BatteryManager;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.util.Log;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;

import java.lang.reflect.Method;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;

import dev.sos.experience.BuildConfig;

/** Fixed SOS-rendered task controls above Android compatibility applications. */
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
    private static final long APP_TRANSITION_REVEAL_MS = 750;
    private static final long OWNER_FOCUS_REVEAL_MS = 250;
    private static volatile boolean ready;
    private static volatile boolean experienceOwnerVisible;
    private static volatile SosCompatChromeService instance;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private WindowManager windowManager;
    private ChromeView chrome;
    private final Runnable transitionReveal = () -> {
        if (chrome != null) {
            chrome.setVisibility(View.VISIBLE);
            chrome.invalidate();
        }
    };
    private final Runnable statusRefresh = new Runnable() {
        @Override
        public void run() {
            updateStatus();
            handler.postDelayed(this, STATUS_REFRESH_MS);
        }
    };
    static void start(Context context) {
        if (!BuildConfig.SOS_COMPAT_ENABLED) return;
        context.startService(new Intent(context, SosCompatChromeService.class));
    }

    static boolean isReady() {
        return ready;
    }

    static void experienceOwnerFocused(Context context) {
        experienceOwnerVisible = true;
        SosCompatChromeService service = instance;
        if (service == null) {
            start(context);
        } else {
            service.handler.post(service::hideForExperienceOwner);
        }
    }

    static void trustedSurfaceFocused(Context context) {
        experienceOwnerVisible = false;
        SosCompatChromeService service = instance;
        if (service == null) {
            start(context);
        } else {
            service.handler.post(
                    () -> service.redrawAfterTransition(OWNER_FOCUS_REVEAL_MS));
        }
    }

    static void beginTransition(Context context) {
        experienceOwnerVisible = false;
        SosCompatChromeService service = instance;
        if (service == null) {
            start(context);
        } else {
            service.redrawAfterTransition(APP_TRANSITION_REVEAL_MS);
        }
    }

    @Override
    public void onCreate() {
        super.onCreate();
        instance = this;
        windowManager = getSystemService(WindowManager.class);
        installChrome();
        setAndroidNavigationDisabled(true);
        handler.post(statusRefresh);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (chrome == null) installChrome();
        setAndroidNavigationDisabled(true);
        redrawAfterTransition(APP_TRANSITION_REVEAL_MS);
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        ready = false;
        if (instance == this) instance = null;
        handler.removeCallbacks(statusRefresh);
        handler.removeCallbacks(transitionReveal);
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
        ChromeView view = new ChromeView(this);
        WindowManager.LayoutParams params = new WindowManager.LayoutParams(
                Math.round(SosFixedUi.dp(this, 76)), Math.round(SosFixedUi.dp(this, 344)),
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                        | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                        | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
                PixelFormat.TRANSLUCENT);
        params.gravity = Gravity.END | Gravity.CENTER_VERTICAL;
        params.setTitle("SOS Trusted App Controls");
        try {
            markAsSystemApplicationOverlay(params);
            windowManager.addView(view, params);
            chrome = view;
            ready = true;
            if (experienceOwnerVisible) {
                hideForExperienceOwner();
            } else {
                redrawAfterTransition(APP_TRANSITION_REVEAL_MS);
            }
            Log.i(TAG, "compat_chrome_ready renderer=sos-fixed-software-text"
                    + " transition_reveal=atomic controls=back,apps,attention,exit");
        } catch (ReflectiveOperationException | RuntimeException error) {
            ready = false;
            Log.e(TAG, "compat_chrome_failed", error);
        }
    }

    private void redrawAfterTransition(long revealDelayMs) {
        if (chrome == null) return;
        handler.removeCallbacks(transitionReveal);
        chrome.setVisibility(View.INVISIBLE);
        handler.postDelayed(transitionReveal, revealDelayMs);
    }

    private void hideForExperienceOwner() {
        if (chrome == null) return;
        handler.removeCallbacks(transitionReveal);
        chrome.setVisibility(View.INVISIBLE);
        Log.i(TAG, "compat_chrome_visibility=hidden owner=stock-mobile");
    }

    private void markAsSystemApplicationOverlay(WindowManager.LayoutParams params)
            throws ReflectiveOperationException {
        Method mark = WindowManager.LayoutParams.class.getMethod(
                "setSystemApplicationOverlay", boolean.class);
        mark.invoke(params, true);
        Log.i(TAG, "compat_chrome_trust=system_application_overlay");
    }

    private void updateStatus() {
        if (chrome == null) return;
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
        chrome.setStatus(time, network, percent < 0 ? "--" : percent + "%");
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
            Log.i(TAG, "android_navigation_owner=" + (disabled ? "sos" : "none"));
        } catch (ReflectiveOperationException | RuntimeException error) {
            Log.e(TAG, "status_bar_policy_failed", error);
        }
    }

    private final class ChromeView extends View {
        private final SosFixedUi.Renderer renderer = new SosFixedUi.Renderer();
        private final String[] labels = {"BACK", "APPS", "ATTN", "EXIT"};
        private String time = "--:--";
        private String network = "OFFLINE";
        private String battery = "--";
        private int pressed = -1;

        ChromeView(Context context) {
            super(context);
            setLayerType(View.LAYER_TYPE_SOFTWARE, null);
            setContentDescription("SOS trusted Back, Apps, Attention and Exit controls");
        }

        void setStatus(String time, String network, String battery) {
            this.time = time;
            this.network = network;
            this.battery = battery;
            invalidate();
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            renderer.fill(canvas, SosFixedUi.PANEL);
            float inset = SosFixedUi.dp(getContext(), 6);
            renderer.text(canvas, time, inset, SosFixedUi.dp(getContext(), 19),
                    SosFixedUi.dp(getContext(), 12), SosFixedUi.TEXT, true);
            renderer.text(canvas, network, inset, SosFixedUi.dp(getContext(), 37),
                    SosFixedUi.dp(getContext(), 9), SosFixedUi.MUTED, false);
            renderer.text(canvas, battery, inset, SosFixedUi.dp(getContext(), 53),
                    SosFixedUi.dp(getContext(), 9), SosFixedUi.MUTED, false);
            for (int index = 0; index < labels.length; index++) {
                RectF bounds = buttonBounds(index);
                renderer.button(canvas, bounds, labels[index], pressed == index,
                        SosFixedUi.dp(getContext(), 8), SosFixedUi.dp(getContext(), 10));
            }
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            int index = buttonAt(event.getX(), event.getY());
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
                pressed = index;
                invalidate();
                return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_CANCEL) {
                pressed = -1;
                invalidate();
                return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_UP) {
                int selected = pressed == index ? index : -1;
                pressed = -1;
                invalidate();
                if (selected >= 0) {
                    beginTransition(SosCompatChromeService.this);
                }
                if (selected == 0) SosAndroidAppAdapter.back();
                else if (selected == 1) SosAndroidAppAdapter.home(
                        SosCompatChromeService.this, "apps");
                else if (selected == 2) SosAndroidAppAdapter.home(
                        SosCompatChromeService.this, "controls");
                else if (selected == 3) SosAndroidAppAdapter.home(SosCompatChromeService.this);
                performClick();
                return true;
            }
            return true;
        }

        @Override
        public boolean performClick() {
            super.performClick();
            return true;
        }

        private RectF buttonBounds(int index) {
            float inset = SosFixedUi.dp(getContext(), 6);
            float top = SosFixedUi.dp(getContext(), 64 + index * 68);
            return new RectF(inset, top, getWidth() - inset,
                    top + SosFixedUi.dp(getContext(), 58));
        }

        private int buttonAt(float x, float y) {
            for (int index = 0; index < labels.length; index++) {
                if (buttonBounds(index).contains(x, y)) return index;
            }
            return -1;
        }
    }
}
