package dev.gpui.mobile;

import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.res.ColorStateList;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Bundle;
import android.os.UserManager;
import android.util.Log;
import android.view.Gravity;
import android.view.View;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.nio.charset.StandardCharsets;

import dev.sos.experience.BuildConfig;

/** Fixed SOS HOME surface for a physically connected ADB host key. */
public final class SosAdbConfirmationActivity extends SosFixedActivity {
    private static final String TAG = "SosAdbConfirmation";
    private static final String ACTION_USB_STATE = "android.hardware.usb.action.USB_STATE";
    private static final String EXTRA_USB_CONNECTED = "connected";
    private static final String RESULT_ACTION = "dev.sos.action.ADB_CONSENT_RESULT";
    private static final ComponentName RESULT_RECEIVER = new ComponentName(
            "dev.sos.frameworkbridge",
            "dev.sos.frameworkbridge.SosAdbConsentReceiver");
    private static final int MAX_KEY_BYTES = 16 * 1024;
    private static final int MAX_FINGERPRINT_CHARS = 512;

    private String key;
    private boolean resultSent;
    private boolean receiverRegistered;

    private final BroadcastReceiver usbStateReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (intent != null && ACTION_USB_STATE.equals(intent.getAction())
                    && !intent.getBooleanExtra(EXTRA_USB_CONNECTED, false)) {
                sendResult("deny", "usb-disconnected");
                finish();
            }
        }
    };

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().setHideOverlayWindows(true);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        setTitle("SOS computer access approval");
        setFinishOnTouchOutside(false);

        if (!BuildConfig.SOS_COMPAT_ENABLED) {
            finish();
            return;
        }
        Intent request = getIntent();
        key = request == null ? null : request.getStringExtra("key");
        String fingerprints = request == null
                ? null : request.getStringExtra("fingerprints");
        if (!validKey(key) || !validFingerprints(fingerprints)) {
            sendResult("deny", "invalid-request");
            finish();
            return;
        }
        UserManager users = getSystemService(UserManager.class);
        if (users == null || !users.isAdminUser() || !users.isUserUnlocked()) {
            sendResult("deny", "owner-locked");
            finish();
            return;
        }

        registerReceiver(usbStateReceiver, new IntentFilter(ACTION_USB_STATE),
                Context.RECEIVER_EXPORTED);
        receiverRegistered = true;
        setContentView(content(fingerprints));
        Log.i(TAG, "sos_adb_confirmation state=shown owner=true renderer=sos-home");
    }

    @Override
    public void onBackPressed() {
        sendResult("deny", "back");
        super.onBackPressed();
    }

    @Override
    protected void onDestroy() {
        if (receiverRegistered) {
            unregisterReceiver(usbStateReceiver);
            receiverRegistered = false;
        }
        if (isFinishing() && !isChangingConfigurations() && !resultSent) {
            sendResult("deny", "surface-closed");
        }
        super.onDestroy();
    }

    private View content(String fingerprints) {
        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setGravity(Gravity.CENTER_VERTICAL);
        content.setPadding(dp(28), dp(42), dp(104), dp(28));
        content.setBackgroundColor(SosFixedUi.BACKGROUND);
        content.setMinimumHeight(getResources().getDisplayMetrics().heightPixels);

        TextView eyebrow = text("TRUSTED SYSTEM REQUEST", 12, SosFixedUi.ACTION_PRESSED);
        eyebrow.setTypeface(Typeface.DEFAULT_BOLD);
        eyebrow.setLetterSpacing(0.12f);
        content.addView(eyebrow, matchWrap(dp(14)));

        TextView title = text("Allow this computer to access SOS?", 29, SosFixedUi.TEXT);
        title.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        title.setAccessibilityHeading(true);
        content.addView(title, matchWrap(dp(14)));

        TextView message = text(
                "ADB grants deep administrative access. Compare this fingerprint with the "
                        + "computer you physically connected.", 16, SosFixedUi.MUTED);
        message.setLineSpacing(0, 1.25f);
        content.addView(message, matchWrap(dp(20)));

        TextView fingerprint = text(fingerprints, 17, SosFixedUi.TEXT);
        fingerprint.setTypeface(Typeface.MONOSPACE);
        fingerprint.setPadding(dp(18), dp(16), dp(18), dp(16));
        GradientDrawable panel = new GradientDrawable();
        panel.setColor(SosFixedUi.PANEL);
        panel.setCornerRadius(dp(14));
        panel.setStroke(dp(1), SosFixedUi.MUTED);
        fingerprint.setBackground(panel);
        content.addView(fingerprint, matchWrap(dp(18)));

        TextView persistence = text(
                "Allow once lasts until this ADB session ends. Always allow stores this "
                        + "computer’s public key on the device.", 14, SosFixedUi.MUTED);
        persistence.setLineSpacing(0, 1.22f);
        content.addView(persistence, matchWrap(dp(20)));

        Button once = button("Allow once", SosFixedUi.ACTION, SosFixedUi.TEXT);
        once.setOnClickListener(view -> approve("allow-once"));
        content.addView(once, matchExact(dp(54), dp(10)));

        Button always = button("Always allow this computer", SosFixedUi.PANEL, SosFixedUi.TEXT);
        always.setOnClickListener(view -> approve("always-allow"));
        content.addView(always, matchExact(dp(54), dp(10)));

        Button deny = button("Deny connection", SosFixedUi.PANEL, SosFixedUi.URGENT);
        deny.setOnClickListener(view -> {
            sendResult("deny", "user");
            finish();
        });
        content.addView(deny, matchExact(dp(54), 0));

        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        scroll.setBackgroundColor(SosFixedUi.BACKGROUND);
        scroll.addView(content, new ScrollView.LayoutParams(
                ScrollView.LayoutParams.MATCH_PARENT, ScrollView.LayoutParams.WRAP_CONTENT));
        return scroll;
    }

    private void approve(String decision) {
        sendResult(decision, "user");
        finish();
    }

    private void sendResult(String decision, String reason) {
        if (resultSent) return;
        Intent result = new Intent(RESULT_ACTION)
                .setComponent(RESULT_RECEIVER)
                .putExtra("decision", decision);
        if (!"deny".equals(decision)) result.putExtra("key", key);
        try {
            sendBroadcast(result);
            resultSent = true;
            Log.i(TAG, "sos_adb_confirmation action=" + decision
                    + " result=dispatched reason=" + reason);
        } catch (RuntimeException error) {
            Log.e(TAG, "sos_adb_confirmation action=" + decision
                    + " result=failed reason=" + reason, error);
        }
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

    private static boolean validKey(String value) {
        return value != null && !value.isEmpty()
                && value.getBytes(StandardCharsets.UTF_8).length <= MAX_KEY_BYTES;
    }

    private static boolean validFingerprints(String value) {
        return value != null && !value.isEmpty() && value.length() <= MAX_FINGERPRINT_CHARS;
    }
}
