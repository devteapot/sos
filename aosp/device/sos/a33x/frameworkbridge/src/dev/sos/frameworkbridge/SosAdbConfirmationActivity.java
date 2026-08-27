package dev.sos.frameworkbridge;

import android.app.Activity;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.res.ColorStateList;
import android.debug.IAdbManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.hardware.usb.UsbManager;
import android.os.Bundle;
import android.os.IBinder;
import android.os.ServiceManager;
import android.os.UserManager;
import android.util.Slog;
import android.view.Gravity;
import android.view.View;
import android.view.Window;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.Space;
import android.widget.TextView;

/** Fixed SOS-owned consent surface for a physically connected ADB host key. */
public final class SosAdbConfirmationActivity extends Activity {
    private static final String TAG = "SosAdbConfirmation";
    private static final int MAX_KEY_BYTES = 16 * 1024;
    private static final int MAX_FINGERPRINT_CHARS = 512;

    // Fixed administrative surfaces do not inherit experience appearance. These
    // named tokens keep the small trusted surface internally coherent.
    private static final int COLOR_BACKGROUND = Color.rgb(13, 15, 18);
    private static final int COLOR_SURFACE = Color.rgb(27, 31, 36);
    private static final int COLOR_TEXT = Color.rgb(244, 247, 248);
    private static final int COLOR_MUTED = Color.rgb(174, 184, 190);
    private static final int COLOR_ACCENT = Color.rgb(89, 214, 190);
    private static final int COLOR_DANGER = Color.rgb(255, 147, 147);

    private String key;
    private boolean serviceNotified;
    private boolean receiverRegistered;

    private final BroadcastReceiver usbStateReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (intent != null && UsbManager.ACTION_USB_STATE.equals(intent.getAction())
                    && !intent.getBooleanExtra(UsbManager.USB_CONNECTED, false)) {
                deny("usb-disconnected");
                finish();
            }
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        Window window = getWindow();
        window.addSystemFlags(
                WindowManager.LayoutParams.SYSTEM_FLAG_HIDE_NON_SYSTEM_OVERLAY_WINDOWS);
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        window.setStatusBarColor(COLOR_BACKGROUND);
        window.setNavigationBarColor(COLOR_BACKGROUND);
        super.onCreate(savedInstanceState);
        setTitle("SOS computer access approval");
        setFinishOnTouchOutside(false);

        Intent intent = getIntent();
        key = intent == null ? null : intent.getStringExtra("key");
        String fingerprints = intent == null ? null : intent.getStringExtra("fingerprints");
        if (!validKey(key) || !validFingerprints(fingerprints)) {
            Slog.w(TAG, "sos_adb_confirmation state=rejected reason=invalid-request");
            deny("invalid-request");
            finish();
            return;
        }

        IntentFilter usbState = new IntentFilter(UsbManager.ACTION_USB_STATE);
        registerReceiver(usbStateReceiver, usbState, Context.RECEIVER_EXPORTED);
        receiverRegistered = true;

        UserManager users = getSystemService(UserManager.class);
        if (users == null || !users.isAdminUser()) {
            showBlocked("Owner approval required",
                    "ADB access can only be approved from the SOS owner profile.");
            Slog.w(TAG, "sos_adb_confirmation state=blocked reason=non-owner");
            return;
        }
        if (!users.isUserUnlocked()) {
            showBlocked("Unlock SOS first",
                    "Return to SOS, unlock the owner profile, then reconnect this computer.");
            Slog.w(TAG, "sos_adb_confirmation state=blocked reason=user-locked");
            return;
        }

        showConsent(fingerprints);
        Slog.i(TAG, "sos_adb_confirmation state=shown owner=true");
    }

    @Override
    public void onBackPressed() {
        deny("back");
        super.onBackPressed();
    }

    @Override
    protected void onDestroy() {
        if (receiverRegistered) {
            unregisterReceiver(usbStateReceiver);
            receiverRegistered = false;
        }
        if (isFinishing() && !serviceNotified) deny("surface-closed");
        super.onDestroy();
    }

    private void showConsent(String fingerprints) {
        LinearLayout content = baseContent(
                "TRUSTED SYSTEM REQUEST",
                "Allow this computer to access SOS?",
                "ADB grants deep administrative access. Compare the fingerprint below "
                        + "with the computer you physically connected.");

        TextView fingerprint = text(fingerprints, 17, COLOR_TEXT);
        fingerprint.setTypeface(Typeface.MONOSPACE);
        fingerprint.setTextIsSelectable(false);
        fingerprint.setPadding(dp(18), dp(16), dp(18), dp(16));
        GradientDrawable panel = new GradientDrawable();
        panel.setColor(COLOR_SURFACE);
        panel.setCornerRadius(dp(14));
        panel.setStroke(dp(1), Color.rgb(56, 64, 70));
        fingerprint.setBackground(panel);
        content.addView(fingerprint, matchWrap(dp(18)));

        TextView persistence = text(
                "“Allow once” lasts until this ADB session ends. “Always allow” stores this "
                        + "computer’s public key on the device.", 14, COLOR_MUTED);
        persistence.setLineSpacing(0, 1.25f);
        content.addView(persistence, matchWrap(dp(22)));

        Button once = button("Allow once", COLOR_ACCENT, COLOR_BACKGROUND);
        once.setOnClickListener(view -> allow(false, "allow-once"));
        content.addView(once, matchExact(dp(54), dp(10)));

        Button always = button("Always allow this computer", COLOR_SURFACE, COLOR_TEXT);
        always.setOnClickListener(view -> allow(true, "always-allow"));
        content.addView(always, matchExact(dp(54), dp(10)));

        Button deny = button("Deny connection", COLOR_SURFACE, COLOR_DANGER);
        deny.setOnClickListener(view -> {
            deny("deny");
            finish();
        });
        content.addView(deny, matchExact(dp(54), dp(4)));

        setContentView(scroll(content));
    }

    private void showBlocked(String title, String message) {
        LinearLayout content = baseContent("TRUSTED SYSTEM REQUEST", title, message);
        Space space = new Space(this);
        content.addView(space, new LinearLayout.LayoutParams(1, 0, 1));
        Button close = button("Deny connection and return", COLOR_ACCENT, COLOR_BACKGROUND);
        close.setOnClickListener(view -> {
            deny("blocked");
            finish();
        });
        content.addView(close, matchExact(dp(54), dp(4)));
        setContentView(scroll(content));
    }

    private LinearLayout baseContent(String eyebrow, String title, String message) {
        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setGravity(Gravity.CENTER_VERTICAL);
        content.setPadding(dp(28), dp(44), dp(28), dp(32));
        content.setBackgroundColor(COLOR_BACKGROUND);
        content.setMinimumHeight(getResources().getDisplayMetrics().heightPixels);

        TextView eyebrowView = text(eyebrow, 12, COLOR_ACCENT);
        eyebrowView.setTypeface(Typeface.DEFAULT_BOLD);
        eyebrowView.setLetterSpacing(0.12f);
        content.addView(eyebrowView, matchWrap(dp(14)));

        TextView titleView = text(title, 29, COLOR_TEXT);
        titleView.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        titleView.setAccessibilityHeading(true);
        titleView.setLineSpacing(0, 1.08f);
        content.addView(titleView, matchWrap(dp(14)));

        TextView messageView = text(message, 16, COLOR_MUTED);
        messageView.setLineSpacing(0, 1.28f);
        content.addView(messageView, matchWrap(dp(22)));
        return content;
    }

    private ScrollView scroll(View child) {
        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        scroll.setBackgroundColor(COLOR_BACKGROUND);
        scroll.addView(child, new ScrollView.LayoutParams(
                ScrollView.LayoutParams.MATCH_PARENT, ScrollView.LayoutParams.WRAP_CONTENT));
        return scroll;
    }

    private TextView text(String value, int sizeSp, int color) {
        TextView view = new TextView(this);
        view.setText(value);
        view.setTextSize(sizeSp);
        view.setTextColor(color);
        return view;
    }

    private Button button(String label, int background, int foreground) {
        Button button = new Button(this);
        button.setText(label);
        button.setTextSize(16);
        button.setTextColor(foreground);
        button.setAllCaps(false);
        button.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        button.setBackgroundTintList(ColorStateList.valueOf(background));
        button.setStateListAnimator(null);
        return button;
    }

    private LinearLayout.LayoutParams matchWrap(int bottomMargin) {
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT);
        params.bottomMargin = bottomMargin;
        return params;
    }

    private LinearLayout.LayoutParams matchExact(int height, int bottomMargin) {
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, height);
        params.bottomMargin = bottomMargin;
        return params;
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    private void allow(boolean alwaysAllow, String action) {
        try {
            IAdbManager adb = adbManager();
            adb.allowDebugging(alwaysAllow, key);
            serviceNotified = true;
            Slog.i(TAG, "sos_adb_confirmation action=" + action + " result=accepted");
        } catch (Exception error) {
            Slog.e(TAG, "sos_adb_confirmation action=" + action + " result=failed", error);
        }
        finish();
    }

    private void deny(String reason) {
        if (serviceNotified) return;
        try {
            adbManager().denyDebugging();
            serviceNotified = true;
            Slog.i(TAG, "sos_adb_confirmation action=deny result=accepted reason=" + reason);
        } catch (Exception error) {
            Slog.e(TAG, "sos_adb_confirmation action=deny result=failed reason=" + reason,
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
                && value.getBytes(java.nio.charset.StandardCharsets.UTF_8).length <= MAX_KEY_BYTES;
    }

    private static boolean validFingerprints(String value) {
        return value != null && !value.isEmpty() && value.length() <= MAX_FINGERPRINT_CHARS;
    }
}
