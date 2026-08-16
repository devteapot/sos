package dev.gpui.mobile;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.graphics.Color;
import android.os.Bundle;
import android.util.Log;
import android.view.Gravity;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** Explicit allow-by-selection launch surface for Android compatibility applications. */
public final class SosCompatWorkspaceActivity extends Activity {
    private static final String TAG = "SosCompatWorkspace";

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        SosCompatChromeService.start(this);
        render();
    }

    private void render() {
        int padding = dp(24);
        LinearLayout list = new LinearLayout(this);
        list.setOrientation(LinearLayout.VERTICAL);
        list.setPadding(padding, padding, dp(96), padding);
        list.setBackgroundColor(0xfff3f1e8);

        TextView title = new TextView(this);
        title.setText("SOS COMPATIBILITY SPACE");
        title.setTextColor(0xff17211b);
        title.setTextSize(22);
        title.setGravity(Gravity.START);
        list.addView(title);

        TextView boundary = new TextView(this);
        boundary.setText("Only an explicitly selected launchable application is opened. "
                + "SOS chrome remains above the task for Back, Apps, Attention and Exit.");
        boundary.setTextColor(0xff637069);
        boundary.setTextSize(14);
        boundary.setPadding(0, dp(8), 0, dp(16));
        list.addView(boundary);

        Intent launcher = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
        List<ResolveInfo> candidates = new ArrayList<>(
                getPackageManager().queryIntentActivities(launcher, PackageManager.MATCH_ALL));
        candidates.removeIf(info -> info.activityInfo == null || !info.activityInfo.exported
                || getPackageName().equals(info.activityInfo.packageName));
        candidates.sort(Comparator.comparing(
                info -> String.valueOf(info.loadLabel(getPackageManager())),
                String.CASE_INSENSITIVE_ORDER));

        for (ResolveInfo candidate : candidates) {
            String label = String.valueOf(candidate.loadLabel(getPackageManager()));
            String packageName = candidate.activityInfo.packageName;
            Button app = new Button(this);
            app.setAllCaps(false);
            app.setGravity(Gravity.START | Gravity.CENTER_VERTICAL);
            app.setText(label + "\n" + packageName);
            app.setContentDescription("Open " + label + " in SOS compatibility space");
            app.setOnClickListener(view -> launch(candidate));
            list.addView(app, new LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT, dp(64)));
        }

        ScrollView scroll = new ScrollView(this);
        scroll.addView(list);
        setContentView(scroll);
        Log.i(TAG, "compat_workspace_ready candidates=" + candidates.size());
    }

    private void launch(ResolveInfo selected) {
        Intent launch = new Intent(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_LAUNCHER)
                .setClassName(selected.activityInfo.packageName, selected.activityInfo.name)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                        | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        SosCompatChromeService.start(this);
        startActivity(launch);
        Log.i(TAG, "compat_workspace_launch package=" + selected.activityInfo.packageName);
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }
}
