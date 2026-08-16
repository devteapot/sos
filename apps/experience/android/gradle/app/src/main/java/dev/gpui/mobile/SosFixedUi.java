package dev.gpui.mobile;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;

/** Fixed SOS drawing primitives shared by trusted compatibility surfaces. */
final class SosFixedUi {
    static final int BACKGROUND = 0xff0c1411;
    static final int PANEL = 0xff17211d;
    static final int ACTION = 0xff245b43;
    static final int ACTION_PRESSED = 0xff34765a;
    static final int TEXT = 0xfff0f3ed;
    static final int MUTED = 0xff87a094;
    static final int URGENT = 0xffdf6f62;

    static float dp(Context context, float value) {
        return value * context.getResources().getDisplayMetrics().density;
    }

    static final class Renderer {
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);

        void fill(Canvas canvas, int color) {
            canvas.drawColor(color);
        }

        void rect(Canvas canvas, RectF bounds, int color, float radius) {
            paint.setColor(color);
            paint.setStyle(Paint.Style.FILL);
            canvas.drawRoundRect(bounds, radius, radius, paint);
        }

        void text(Canvas canvas, String value, float x, float baseline, float size,
                int color, boolean strong) {
            paint.setColor(color);
            paint.setTextSize(size);
            paint.setTypeface(Typeface.create("sans-serif",
                    strong ? Typeface.BOLD : Typeface.NORMAL));
            paint.setStyle(Paint.Style.FILL);
            canvas.drawText(value, x, baseline, paint);
        }

        void centered(Canvas canvas, String value, RectF bounds, float size,
                int color, boolean strong) {
            paint.setTextSize(size);
            paint.setTypeface(Typeface.create("sans-serif",
                    strong ? Typeface.BOLD : Typeface.NORMAL));
            Paint.FontMetrics metrics = paint.getFontMetrics();
            float x = bounds.centerX() - paint.measureText(value) / 2f;
            float y = bounds.centerY() - (metrics.ascent + metrics.descent) / 2f;
            text(canvas, value, x, y, size, color, strong);
        }

        void button(Canvas canvas, RectF bounds, String label, boolean pressed, float radius,
                float textSize) {
            rect(canvas, bounds, pressed ? ACTION_PRESSED : ACTION, radius);
            centered(canvas, label, bounds, textSize, TEXT, true);
        }
    }

    private SosFixedUi() {}
}
