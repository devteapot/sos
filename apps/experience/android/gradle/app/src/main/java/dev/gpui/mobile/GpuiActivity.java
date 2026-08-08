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

import androidx.core.splashscreen.SplashScreen;

/** Permanent NativeActivity host for Luau-authored SOS experience revisions. */
public class GpuiActivity extends NativeActivity {
    private static volatile boolean sNativeLibLoaded = false;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        SplashScreen splash = SplashScreen.installSplashScreen(this);
        if (!sNativeLibLoaded) {
            try {
                ActivityInfo info = getPackageManager().getActivityInfo(
                        getComponentName(), PackageManager.GET_META_DATA);
                String library = info.metaData.getString("android.app.lib_name");
                if (library != null) {
                    System.loadLibrary(library);
                    sNativeLibLoaded = true;
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

    }

    private boolean isNativeReady() {
        if (!sNativeLibLoaded) return false;
        try {
            return nativeIsInitialized();
        } catch (UnsatisfiedLinkError ignored) {
            return false;
        }
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        return super.dispatchKeyEvent(event);
    }

    @Override
    protected void onDestroy() {
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
}
