package dev.gpui.mobile;

import android.app.Activity;
import android.os.Bundle;

/** Lifecycle adapter that keeps fixed SOS Java surfaces on the shared frame policy. */
abstract class SosFixedActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        SosWindowPolicy.apply(this, getClass().getSimpleName());
        super.onCreate(state);
    }

    @Override
    protected void onResume() {
        super.onResume();
        SosWindowPolicy.apply(this, getClass().getSimpleName());
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            SosWindowPolicy.apply(this, getClass().getSimpleName());
            SosCompatChromeService.trustedSurfaceFocused(this);
        }
    }
}
