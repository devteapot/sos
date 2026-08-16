package dev.gpui.mobile;

import android.app.Application;
import android.os.Handler;
import android.os.Looper;

import dev.sos.experience.BuildConfig;

/** Permanent policy process for an SOS system HOME. */
public final class SosApplication extends Application {
    private static final long HOME_AUDIT_INTERVAL_MS = 5000;
    private final Handler handler = new Handler(Looper.getMainLooper());

    private final Runnable homeAudit = new Runnable() {
        @Override
        public void run() {
            SosHomePolicy.enforce(SosApplication.this, "periodic-audit");
            if (BuildConfig.SOS_COMPAT_ENABLED) {
                SosAttentionPolicy.enforce(SosApplication.this, "periodic-audit");
            }
            handler.postDelayed(this, HOME_AUDIT_INTERVAL_MS);
        }
    };

    @Override
    public void onCreate() {
        super.onCreate();
        if (BuildConfig.SOS_HOME_ENABLED) {
            SosHomePolicy.enforce(this, "application-create");
            if (BuildConfig.SOS_COMPAT_ENABLED) {
                SosAttentionPolicy.enforce(this, "application-create");
            }
            handler.postDelayed(homeAudit, HOME_AUDIT_INTERVAL_MS);
        }
    }
}
