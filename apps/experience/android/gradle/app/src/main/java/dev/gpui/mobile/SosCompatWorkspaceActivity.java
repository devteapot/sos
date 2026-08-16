package dev.gpui.mobile;

import android.graphics.Canvas;
import android.graphics.RectF;
import android.os.Bundle;
import android.util.Log;
import android.view.MotionEvent;
import android.view.View;

import java.util.List;

/** SOS-rendered allow-by-selection surface for compatibility applications. */
public final class SosCompatWorkspaceActivity extends SosFixedActivity {
    private static final String TAG = "SosCompatWorkspace";

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        List<SosAndroidAppAdapter.Entry> entries = SosAndroidAppAdapter.launchable(this);
        setContentView(new WorkspaceView(entries));
        Log.i(TAG, "compat_workspace_ready renderer=sos-fixed candidates=" + entries.size());
    }

    private final class WorkspaceView extends View {
        private final SosFixedUi.Renderer renderer = new SosFixedUi.Renderer();
        private final List<SosAndroidAppAdapter.Entry> entries;
        private float scroll;
        private float downY;
        private float lastY;
        private boolean dragging;

        WorkspaceView(List<SosAndroidAppAdapter.Entry> entries) {
            super(SosCompatWorkspaceActivity.this);
            this.entries = entries;
            setBackgroundColor(SosFixedUi.BACKGROUND);
            setContentDescription("SOS compatibility applications");
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            renderer.fill(canvas, SosFixedUi.BACKGROUND);
            float margin = SosFixedUi.dp(this.getContext(), 24);
            float usableRight = getWidth() - SosFixedUi.dp(this.getContext(), 96);
            renderer.text(canvas, "SOS APPLICATIONS", margin,
                    SosFixedUi.dp(this.getContext(), 52),
                    SosFixedUi.dp(this.getContext(), 22), SosFixedUi.TEXT, true);
            renderer.text(canvas, "Open a compatible application. SOS remains in control.",
                    margin, SosFixedUi.dp(this.getContext(), 80),
                    SosFixedUi.dp(this.getContext(), 13), SosFixedUi.MUTED, false);

            canvas.save();
            canvas.clipRect(0, SosFixedUi.dp(this.getContext(), 96), usableRight, getHeight());
            canvas.translate(0, -scroll);
            float top = SosFixedUi.dp(this.getContext(), 108);
            float rowHeight = SosFixedUi.dp(this.getContext(), 72);
            float gap = SosFixedUi.dp(this.getContext(), 10);
            for (SosAndroidAppAdapter.Entry entry : entries) {
                RectF row = new RectF(margin, top, usableRight - margin, top + rowHeight);
                renderer.rect(canvas, row, SosFixedUi.PANEL,
                        SosFixedUi.dp(this.getContext(), 12));
                renderer.text(canvas, entry.label, row.left + margin / 2,
                        row.top + SosFixedUi.dp(this.getContext(), 29),
                        SosFixedUi.dp(this.getContext(), 17), SosFixedUi.TEXT, true);
                top += rowHeight + gap;
            }
            if (entries.isEmpty()) {
                renderer.text(canvas, "NO COMPATIBLE APPLICATIONS", margin, top + margin,
                        SosFixedUi.dp(this.getContext(), 15), SosFixedUi.MUTED, true);
            }
            canvas.restore();
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            float y = event.getY();
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
                downY = y;
                lastY = y;
                dragging = false;
                return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_MOVE) {
                float delta = lastY - y;
                if (Math.abs(y - downY) > SosFixedUi.dp(getContext(), 8)) dragging = true;
                scroll = clampScroll(scroll + delta);
                lastY = y;
                invalidate();
                return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_UP && !dragging) {
                int index = entryAt(y + scroll);
                if (index >= 0) {
                    SosAndroidAppAdapter.launch(
                            SosCompatWorkspaceActivity.this, entries.get(index));
                }
                performClick();
                return true;
            }
            return true;
        }

        @Override
        public boolean performClick() {
            super.performClick();
            return true;
        }

        private int entryAt(float y) {
            float top = SosFixedUi.dp(getContext(), 108);
            float stride = SosFixedUi.dp(getContext(), 82);
            float rowHeight = SosFixedUi.dp(getContext(), 72);
            if (y < top) return -1;
            int index = (int) ((y - top) / stride);
            float within = (y - top) - index * stride;
            return index >= 0 && index < entries.size() && within <= rowHeight ? index : -1;
        }

        private float clampScroll(float value) {
            float content = SosFixedUi.dp(getContext(), 108 + entries.size() * 82 + 24);
            return Math.max(0, Math.min(value, Math.max(0, content - getHeight())));
        }
    }
}
