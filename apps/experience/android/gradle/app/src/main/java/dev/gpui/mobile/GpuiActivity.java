package dev.gpui.mobile;

import android.app.NativeActivity;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.media.AudioManager;
import android.net.Uri;
import android.os.Bundle;
import android.util.Log;
import android.view.KeyEvent;

import dev.sos.experience.BuildConfig;

import androidx.core.splashscreen.SplashScreen;
/** Permanent NativeActivity host for Luau-authored SOS experience revisions. */
public class GpuiActivity extends NativeActivity {
    private static volatile boolean sNativeLibLoaded = false;
    private static volatile boolean sActivityCreated = false;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        configureSosWindow();
        SplashScreen splash = SplashScreen.installSplashScreen(this);
        if (!sNativeLibLoaded) {
            try {
                ActivityInfo info = getPackageManager().getActivityInfo(
                        getComponentName(), PackageManager.GET_META_DATA);
                Bundle metadata = info.metaData;
                if (metadata == null) {
                    info = getPackageManager().getActivityInfo(
                            new android.content.ComponentName(this, GpuiActivity.class),
                            PackageManager.GET_META_DATA);
                    metadata = info.metaData;
                }
                String library = metadata == null
                        ? null
                        : metadata.getString("android.app.lib_name");
                if (library != null) {
                    System.loadLibrary(library);
                    sNativeLibLoaded = true;
                } else {
                    Log.e("GpuiActivity", "android.app.lib_name metadata unavailable");
                }
            } catch (PackageManager.NameNotFoundException ignored) {
                Log.e("GpuiActivity", "activity metadata unavailable");
            } catch (UnsatisfiedLinkError ignored) {
                sNativeLibLoaded = true;
            }
        }
        splash.setKeepOnScreenCondition(() -> !isNativeReady());
        setVolumeControlStream(AudioManager.STREAM_MUSIC);
        super.onCreate(savedInstanceState);
        sActivityCreated = true;
        SosHomePolicy.enforce(this, "activity-create");
    }

    @Override
    protected void onResume() {
        super.onResume();
        configureSosWindow();
        SosHomePolicy.enforce(this, "activity-resume");
        if (BuildConfig.SOS_COMPAT_ENABLED) {
            SosCompatChromeService.start(this);
        }
    }

    private boolean isNativeReady() {
        return isSosHomeReady();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            configureSosWindow();
            SosCompatChromeService.ownerFocused(this);
        }
    }

    private void configureSosWindow() {
        SosWindowPolicy.apply(this, "gpui");
    }

    static boolean isSosHomeReady() {
        if (!sActivityCreated) return false;
        if (!sNativeLibLoaded) return false;
        try {
            return nativeIsInitialized();
        } catch (UnsatisfiedLinkError ignored) {
            return false;
        }
    }

    static void dispatchAccessibilityAction(String action, String target, String value) {
        if (!sNativeLibLoaded) return;
        try {
            nativeOnAccessibilityAction(action, target, value);
            // The GPUI Android loop drains the queued action on its next frame.
            nativeOnDeepLink("sos://accessibility");
        } catch (UnsatisfiedLinkError ignored) {
            Log.w("GpuiActivity", "native accessibility bridge unavailable");
        }
    }

    static void dispatchImeState(
            String target, String text, int selectionStart, int selectionEnd,
            int markedStart, int markedEnd, String kind) {
        if (!sNativeLibLoaded) return;
        try {
            nativeOnImeState(
                    target, text, selectionStart, selectionEnd,
                    markedStart, markedEnd, kind);
            nativeOnDeepLink("sos://ime");
        } catch (UnsatisfiedLinkError ignored) {
            Log.w("GpuiActivity", "native IME bridge unavailable");
        }
    }

    static void dispatchImeInset(float logicalBottom) {
        if (!sNativeLibLoaded) return;
        try {
            nativeOnImeInset(logicalBottom);
        } catch (UnsatisfiedLinkError ignored) {
            Log.w("GpuiActivity", "native IME inset bridge unavailable");
        }
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        return super.dispatchKeyEvent(event);
    }

    @Override
    protected void onDestroy() {
        sActivityCreated = false;
        GpuiMediaSession.release();
        super.onDestroy();
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        Uri data = intent.getData();
        if (data == null) return;
        String url = data.toString();
        Log.i("GpuiActivity", "onNewIntent deeplink: " + url);
        try {
            nativeOnDeepLink(url);
        } catch (UnsatisfiedLinkError ignored) {
            Log.w("GpuiActivity", "nativeOnDeepLink not available yet");
        }
    }

    private static native boolean nativeIsInitialized();
    private static native void nativeOnDeepLink(String url);
    private static native void nativeOnAccessibilityAction(
            String action, String target, String value);
    private static native void nativeOnImeState(
            String target, String text, int selectionStart, int selectionEnd,
            int markedStart, int markedEnd, String kind);
    private static native void nativeOnImeInset(float logicalBottom);
}
