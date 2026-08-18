package dev.gpui.mobile;

import android.app.Instrumentation;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.os.Build;
import android.util.Log;
import android.view.KeyEvent;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** Headless Android task adapter. It exposes no platform UI to SOS. */
final class SosAndroidAppAdapter {
    private static final String TAG = "SosAndroidAppAdapter";

    static final class Entry {
        final String label;
        final String packageName;
        final String activityName;

        Entry(String label, String packageName, String activityName) {
            this.label = label;
            this.packageName = packageName;
            this.activityName = activityName;
        }
    }

    static List<Entry> launchable(Context context) {
        PackageManager packages = context.getPackageManager();
        Intent launcher = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
        List<ResolveInfo> candidates = new ArrayList<>(
                packages.queryIntentActivities(launcher, PackageManager.MATCH_ALL));
        candidates.removeIf(info -> info.activityInfo == null || !info.activityInfo.exported
                || info.activityInfo.applicationInfo == null
                || (info.activityInfo.applicationInfo.flags
                    & (ApplicationInfo.FLAG_SYSTEM | ApplicationInfo.FLAG_UPDATED_SYSTEM_APP)) != 0
                // Legacy apps that require Android's permission-review Activity
                // cannot be presented without leaking Android UI. They are not
                // compatible with the fixed SOS ceremony boundary.
                || info.activityInfo.applicationInfo.targetSdkVersion < Build.VERSION_CODES.M
                || context.getPackageName().equals(info.activityInfo.packageName));
        candidates.sort(Comparator.comparing(
                info -> String.valueOf(info.loadLabel(packages)),
                String.CASE_INSENSITIVE_ORDER));
        List<Entry> result = new ArrayList<>();
        for (ResolveInfo candidate : candidates) {
            result.add(new Entry(
                    String.valueOf(candidate.loadLabel(packages)),
                    candidate.activityInfo.packageName,
                    candidate.activityInfo.name));
        }
        return result;
    }

    static void launch(Context context, Entry selected) {
        Intent launch = new Intent(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_LAUNCHER)
                .setClassName(selected.packageName, selected.activityName)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                        | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        SosCompatChromeService.beginTransition(context);
        context.startActivity(launch);
        Log.i(TAG, "compat_app_launch package=" + selected.packageName);
    }

    static void back() {
        new Thread(() -> {
            try {
                new Instrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);
                Log.i(TAG, "compat_app_action action=back ime_precedence=platform_key_dispatch");
            } catch (RuntimeException error) {
                Log.e(TAG, "compat_app_back_failed", error);
            }
        }, "sos-compat-back").start();
    }

    static void home(Context context) {
        Intent home = new Intent(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_HOME)
                .setPackage(context.getPackageName())
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        context.startActivity(home);
    }

    private SosAndroidAppAdapter() {}
}
