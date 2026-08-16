package dev.gpui.mobile;

import android.graphics.Canvas;
import android.graphics.RectF;
import android.os.Bundle;
import android.text.format.DateFormat;
import android.view.MotionEvent;
import android.view.View;

import java.util.Date;
import java.util.List;

/** Fixed SOS presentation for the durable attention journal. */
public final class SosAttentionActivity extends SosFixedActivity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        setContentView(new AttentionView(SosAttentionStore.read(this)));
    }

    private final class AttentionView extends View {
        private final SosFixedUi.Renderer renderer = new SosFixedUi.Renderer();
        private List<SosAttentionStore.Event> events;
        private float scroll;
        private float downY;
        private float lastY;
        private boolean dragging;

        AttentionView(List<SosAttentionStore.Event> events) {
            super(SosAttentionActivity.this);
            this.events = events;
            setBackgroundColor(SosFixedUi.BACKGROUND);
            setContentDescription("SOS attention events");
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            renderer.fill(canvas, SosFixedUi.BACKGROUND);
            float margin = SosFixedUi.dp(getContext(), 24);
            float usableRight = getWidth() - SosFixedUi.dp(getContext(), 96);
            renderer.text(canvas, "SOS ATTENTION", margin, SosFixedUi.dp(getContext(), 52),
                    SosFixedUi.dp(getContext(), 22), SosFixedUi.TEXT, true);
            renderer.text(canvas, "Calls, alarms and security signals keep trusted priority.",
                    margin, SosFixedUi.dp(getContext(), 80),
                    SosFixedUi.dp(getContext(), 13), SosFixedUi.MUTED, false);

            RectF clear = clearBounds(usableRight);
            renderer.button(canvas, clear, "CLEAR ACKNOWLEDGED", false,
                    SosFixedUi.dp(getContext(), 10), SosFixedUi.dp(getContext(), 12));

            canvas.save();
            canvas.clipRect(0, SosFixedUi.dp(getContext(), 152), usableRight, getHeight());
            canvas.translate(0, -scroll);
            float top = SosFixedUi.dp(getContext(), 164);
            float rowHeight = SosFixedUi.dp(getContext(), 104);
            float gap = SosFixedUi.dp(getContext(), 10);
            for (SosAttentionStore.Event event : events) {
                String source = SosVisibleIdentity.source(getContext(), event.packageName);
                String title = SosVisibleIdentity.content(
                        getContext(), event.packageName, event.title);
                String detail = SosVisibleIdentity.content(
                        getContext(), event.packageName, event.detail);
                RectF row = new RectF(margin, top, usableRight - margin, top + rowHeight);
                renderer.rect(canvas, row,
                        event.urgent ? 0xff3d211e : SosFixedUi.PANEL,
                        SosFixedUi.dp(getContext(), 12));
                String time = DateFormat.getTimeFormat(SosAttentionActivity.this)
                        .format(new Date(event.timestamp));
                renderer.text(canvas,
                        (event.urgent ? "URGENT · " : "")
                                + event.kind.toUpperCase() + " · " + time,
                        row.left + margin / 2, row.top + SosFixedUi.dp(getContext(), 26),
                        SosFixedUi.dp(getContext(), 13),
                        event.urgent ? SosFixedUi.URGENT : SosFixedUi.MUTED, true);
                renderer.text(canvas, title, row.left + margin / 2,
                        row.top + SosFixedUi.dp(getContext(), 52),
                        SosFixedUi.dp(getContext(), 16), SosFixedUi.TEXT, true);
                renderer.text(canvas, detail, row.left + margin / 2,
                        row.top + SosFixedUi.dp(getContext(), 76),
                        SosFixedUi.dp(getContext(), 13), SosFixedUi.TEXT, false);
                renderer.text(canvas, source, row.left + margin / 2,
                        row.top + SosFixedUi.dp(getContext(), 94),
                        SosFixedUi.dp(getContext(), 10), SosFixedUi.MUTED, false);
                top += rowHeight + gap;
            }
            if (events.isEmpty()) {
                renderer.text(canvas, "NO ATTENTION EVENTS", margin, top + margin,
                        SosFixedUi.dp(getContext(), 15), SosFixedUi.MUTED, true);
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
                if (clearBounds(getWidth() - SosFixedUi.dp(getContext(), 96))
                        .contains(event.getX(), y)) {
                    SosAttentionStore.clear(SosAttentionActivity.this);
                    events = SosAttentionStore.read(SosAttentionActivity.this);
                    scroll = 0;
                    invalidate();
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

        private RectF clearBounds(float usableRight) {
            return new RectF(SosFixedUi.dp(getContext(), 24),
                    SosFixedUi.dp(getContext(), 96),
                    usableRight - SosFixedUi.dp(getContext(), 24),
                    SosFixedUi.dp(getContext(), 140));
        }

        private float clampScroll(float value) {
            float content = SosFixedUi.dp(getContext(), 164 + events.size() * 114 + 24);
            return Math.max(0, Math.min(value, Math.max(0, content - getHeight())));
        }
    }
}
