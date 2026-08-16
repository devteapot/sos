package dev.gpui.mobile;

import android.app.Activity;
import android.graphics.Color;
import android.os.Bundle;
import android.text.format.DateFormat;
import android.view.Gravity;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.util.Date;
import java.util.List;

/** Fixed signed presentation for the durable attention journal. */
public final class SosAttentionActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        SosCompatChromeService.start(this);
        render();
    }

    private void render() {
        List<SosAttentionStore.Event> events = SosAttentionStore.read(this);
        LinearLayout list = new LinearLayout(this);
        list.setOrientation(LinearLayout.VERTICAL);
        list.setPadding(dp(24), dp(24), dp(96), dp(24));
        list.setBackgroundColor(0xfff3f1e8);

        TextView title = new TextView(this);
        title.setText("SOS ATTENTION");
        title.setTextSize(22);
        title.setTextColor(0xff17211b);
        list.addView(title);

        TextView durable = new TextView(this);
        durable.setText("Durable typed events · urgent calls, alarms and security signals remain "
                + "fixed trusted presentation.");
        durable.setTextColor(0xff637069);
        durable.setPadding(0, dp(8), 0, dp(12));
        list.addView(durable);

        Button clear = new Button(this);
        clear.setText("Clear acknowledged events");
        clear.setAllCaps(false);
        clear.setOnClickListener(view -> {
            SosAttentionStore.clear(this);
            render();
        });
        list.addView(clear);

        if (events.isEmpty()) {
            TextView empty = new TextView(this);
            empty.setText("No attention events");
            empty.setTextSize(16);
            empty.setGravity(Gravity.CENTER);
            empty.setPadding(0, dp(48), 0, 0);
            list.addView(empty);
        }

        for (SosAttentionStore.Event event : events) {
            TextView row = new TextView(this);
            String time = DateFormat.getTimeFormat(this).format(new Date(event.timestamp));
            row.setText((event.urgent ? "URGENT · " : "") + event.kind.toUpperCase()
                    + " · " + time + "\n" + event.title + "\n" + event.detail
                    + "\n" + event.packageName);
            row.setTextColor(event.urgent ? 0xff7a241f : 0xff17211b);
            row.setBackgroundColor(event.urgent ? 0xffffe2dd : Color.WHITE);
            row.setPadding(dp(14), dp(12), dp(14), dp(12));
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT);
            params.topMargin = dp(8);
            list.addView(row, params);
        }

        ScrollView scroll = new ScrollView(this);
        scroll.addView(list);
        setContentView(scroll);
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }
}
