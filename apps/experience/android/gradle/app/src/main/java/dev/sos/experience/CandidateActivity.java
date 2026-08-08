package dev.sos.experience;

import android.app.Activity;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.Process;
import android.util.Log;
import android.view.View;

import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Disposable candidate-process surface used to prove the Android supervisor
 * boundary before bootstrapping a second GPUI runtime in this process.
 */
public final class CandidateActivity extends Activity {
    private static final String TAG = "sos-supervisor";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Thread.setDefaultUncaughtExceptionHandler((thread, error) -> {
            Log.e(TAG, "candidate_uncaught_exception pid=" + Process.myPid(), error);
            try {
                finishAndRemoveTask();
            } finally {
                Process.killProcess(Process.myPid());
            }
        });
        String revision = valueOr(getIntent().getStringExtra("revision"), "unknown");
        String mode = valueOr(getIntent().getStringExtra("mode"), "ready");
        Log.i(TAG, "candidate_created revision=" + revision + " mode=" + mode
                + " pid=" + Process.myPid());

        if ("crash-before".equals(mode)) {
            Log.i(TAG, "candidate_forced_crash phase=before_first_frame revision=" + revision);
            throw new IllegalStateException("injected candidate crash before first frame");
        }

        setContentView(new CandidateView(this, revision, mode));
    }

    private static String valueOr(String value, String fallback) {
        return value == null || value.isEmpty() ? fallback : value;
    }

    private static final class CandidateView extends View {
        private final String revision;
        private final String mode;
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final AtomicBoolean presented = new AtomicBoolean(false);

        CandidateView(Activity activity, String revision, String mode) {
            super(activity);
            this.revision = revision;
            this.mode = mode;
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            canvas.drawColor(Color.rgb(20, 25, 31));
            paint.setColor(Color.rgb(215, 235, 220));
            paint.setTextSize(52f);
            canvas.drawText("Candidate revision", 48f, 140f, paint);
            paint.setColor(Color.rgb(135, 207, 160));
            paint.setTextSize(34f);
            canvas.drawText(revision, 48f, 205f, paint);

            if (presented.compareAndSet(false, true)) {
                Log.i(TAG, "candidate_first_frame revision=" + revision
                        + " pid=" + Process.myPid());
                if ("crash-after".equals(mode)) {
                    new Handler(Looper.getMainLooper()).postDelayed(() -> {
                        Log.i(TAG, "candidate_forced_crash phase=after_first_frame revision="
                                + revision);
                        throw new IllegalStateException(
                                "injected candidate crash after first frame");
                    }, 250L);
                }
            }
        }
    }
}
