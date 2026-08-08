package dev.gpui.mobile;

import android.app.ActivityManager;
import android.app.NativeActivity;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.media.AudioManager;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;
import android.view.KeyEvent;

import androidx.core.content.ContextCompat;
import androidx.core.splashscreen.SplashScreen;

import dev.sos.experience.CandidateGpuiActivity;

/** NativeActivity host plus the accepted-process candidate watchdog. */
public class GpuiActivity extends NativeActivity {
    private static final String CANDIDATE_FIRST_FRAME =
            "dev.sos.experience.CANDIDATE_FIRST_FRAME";
    private static volatile boolean sNativeLibLoaded = false;
    private BroadcastReceiver candidateReceiver;
    private volatile long candidateLaunchAtMs;
    private volatile int intentionallyStoppedCandidatePid = -1;

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

        if (getClass() == GpuiActivity.class) {
            candidateReceiver = new BroadcastReceiver() {
                @Override
                public void onReceive(Context context, Intent intent) {
                    String revision = intent.getStringExtra("revision");
                    Log.i("sos-supervisor", "candidate_gpui_first_frame revision="
                            + revision + " pid="
                            + intent.getIntExtra("pid", -1)
                            + " launch_to_first_frame_ms="
                            + (SystemClock.elapsedRealtime() - candidateLaunchAtMs));
                    try {
                        nativeOnDeepLink("sos://candidate-presented?revision=" + revision);
                    } catch (UnsatisfiedLinkError ignored) {
                        Log.e("sos-supervisor", "candidate presentation JNI unavailable");
                    }
                }
            };
            ContextCompat.registerReceiver(
                    this,
                    candidateReceiver,
                    new IntentFilter(CANDIDATE_FIRST_FRAME),
                    ContextCompat.RECEIVER_NOT_EXPORTED);
        }
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
        if (candidateReceiver != null) {
            unregisterReceiver(candidateReceiver);
            candidateReceiver = null;
        }
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
        if (getClass() == GpuiActivity.class
                && "sos".equals(data.getScheme())
                && "candidate".equals(data.getHost())) {
            launchCandidate(data);
        }
        try {
            nativeOnDeepLink(url);
        } catch (UnsatisfiedLinkError ignored) {
            Log.w("GpuiActivity", "nativeOnDeepLink not available yet");
        }
    }

    private void launchCandidate(Uri data) {
        String revision = valueOr(data.getQueryParameter("revision"), "unknown");
        String mode = valueOr(data.getQueryParameter("mode"), "ready");
        launchCandidate(revision, mode, "0", "0");
    }

    /** Called by the accepted Rust host after worker validation and state staging. */
    public void launchNativeCandidate(
            String revision, String stageId, String expectedRevision) {
        launchCandidate(revision, "ready", stageId, expectedRevision);
    }

    /** Test-only crash injection selected by an explicit source marker. */
    public void launchNativeCandidateMode(
            String revision, String stageId, String expectedRevision, String mode) {
        launchCandidate(revision, mode, stageId, expectedRevision);
    }

    private void launchCandidate(
            String revision, String mode, String stageId, String expectedRevision) {
        Intent candidate = new Intent(this, CandidateGpuiActivity.class);
        candidate.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
        candidate.putExtra("sos_process_role", "candidate");
        candidate.putExtra("revision", revision);
        candidate.putExtra("mode", mode);
        candidate.putExtra("stage_id", stageId);
        candidate.putExtra("expected_revision", expectedRevision);
        Context application = getApplicationContext();
        new Thread(() -> {
            stopExistingCandidate();
            new Handler(Looper.getMainLooper()).post(() -> {
                candidateLaunchAtMs = SystemClock.elapsedRealtime();
                startActivity(candidate);
                watchCandidate(revision);
                Log.i("sos-supervisor", "candidate_fresh_process_requested revision=" + revision);
            });
        }, "sos-candidate-replacer").start();
    }

    private static String valueOr(String value, String fallback) {
        return value == null || value.isEmpty() ? fallback : value;
    }

    private void watchCandidate(String revision) {
        Context application = getApplicationContext();
        String candidateProcess = getPackageName() + ":candidate";
        new Thread(() -> {
            int watchedPid = -1;
            for (int attempt = 0; attempt < 1200; attempt++) {
                ActivityManager manager =
                        (ActivityManager) application.getSystemService(Context.ACTIVITY_SERVICE);
                boolean alive = false;
                if (manager != null && manager.getRunningAppProcesses() != null) {
                    for (ActivityManager.RunningAppProcessInfo process
                            : manager.getRunningAppProcesses()) {
                        if (candidateProcess.equals(process.processName)) {
                            if (watchedPid < 0) watchedPid = process.pid;
                            alive = process.pid == watchedPid;
                            break;
                        }
                    }
                }
                if (alive) {
                    // Keep watching the exact process that rendered this revision.
                } else if (watchedPid >= 0) {
                    if (watchedPid == intentionallyStoppedCandidatePid) {
                        Log.i("sos-supervisor", "candidate_process_replaced revision=" + revision
                                + " pid=" + watchedPid);
                        return;
                    }
                    Log.w("sos-supervisor", "candidate_process_died revision=" + revision);
                    new Handler(Looper.getMainLooper()).post(() -> {
                        Intent accepted = new Intent(application, GpuiActivity.class);
                        accepted.setData(Uri.parse(
                                "sos://candidate-died?revision=" + revision));
                        accepted.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                                | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT
                                | Intent.FLAG_ACTIVITY_SINGLE_TOP);
                        application.startActivity(accepted);
                        Log.i("sos-supervisor", "accepted_surface_restored revision=" + revision);
                    });
                    return;
                }
                try {
                    Thread.sleep(50L);
                } catch (InterruptedException interrupted) {
                    Thread.currentThread().interrupt();
                    return;
                }
            }
            Log.w("sos-supervisor", "candidate_watch_timeout revision=" + revision);
        }, "sos-candidate-watchdog").start();
    }

    private void stopExistingCandidate() {
        String candidateProcess = getPackageName() + ":candidate";
        ActivityManager manager =
                (ActivityManager) getSystemService(Context.ACTIVITY_SERVICE);
        for (int attempt = 0; attempt < 40; attempt++) {
            int pid = -1;
            if (manager != null && manager.getRunningAppProcesses() != null) {
                for (ActivityManager.RunningAppProcessInfo process
                        : manager.getRunningAppProcesses()) {
                    if (candidateProcess.equals(process.processName)) {
                        pid = process.pid;
                        break;
                    }
                }
            }
            if (pid < 0) return;
            if (attempt == 0) {
                Log.i("sos-supervisor", "candidate_cached_process_terminated pid=" + pid);
                intentionallyStoppedCandidatePid = pid;
                android.os.Process.killProcess(pid);
            }
            try {
                Thread.sleep(25L);
            } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
                return;
            }
        }
        Log.w("sos-supervisor", "candidate_cached_process_exit_timeout");
    }

    /** Called by Rust after the candidate GPUI post-render callback. */
    public void onNativeCandidateFirstFrame(String revision) {
        Intent firstFrame = new Intent(CANDIDATE_FIRST_FRAME);
        firstFrame.setPackage(getPackageName());
        firstFrame.putExtra("revision", revision);
        firstFrame.putExtra("pid", android.os.Process.myPid());
        sendBroadcast(firstFrame);
    }

    private static native boolean nativeIsInitialized();
    private static native void nativeOnDeepLink(String url);
}
