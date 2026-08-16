package dev.gpui.mobile;

import android.content.Context;
import android.util.AtomicFile;
import android.util.Log;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Bounded credential-protected journal for typed SOS attention events. */
final class SosAttentionStore {
    static final int MAX_EVENTS = 256;
    private static final int MAX_FIELD = 512;
    private static final String TAG = "SosAttentionStore";
    private static final Object LOCK = new Object();

    static final class Event {
        final long timestamp;
        final String key;
        final String packageName;
        final String kind;
        final boolean urgent;
        final String title;
        final String detail;

        Event(long timestamp, String key, String packageName, String kind, boolean urgent,
                String title, String detail) {
            this.timestamp = timestamp;
            this.key = bounded(key);
            this.packageName = bounded(packageName);
            this.kind = bounded(kind);
            this.urgent = urgent;
            this.title = bounded(title);
            this.detail = bounded(detail);
        }

        JSONObject toJson() throws JSONException {
            return new JSONObject()
                    .put("timestamp", timestamp)
                    .put("key", key)
                    .put("package", packageName)
                    .put("kind", kind)
                    .put("urgent", urgent)
                    .put("title", title)
                    .put("detail", detail);
        }

        static Event fromJson(JSONObject object) {
            return new Event(object.optLong("timestamp"), object.optString("key"),
                    object.optString("package"), object.optString("kind", "general"),
                    object.optBoolean("urgent"), object.optString("title"),
                    object.optString("detail"));
        }
    }

    static void append(Context context, Event event) {
        synchronized (LOCK) {
            List<Event> events = readLocked(context);
            events.removeIf(existing -> existing.key.equals(event.key));
            events.add(0, event);
            if (events.size() > MAX_EVENTS) {
                events = new ArrayList<>(events.subList(0, MAX_EVENTS));
            }
            writeLocked(context, events);
        }
    }

    static List<Event> read(Context context) {
        synchronized (LOCK) {
            return Collections.unmodifiableList(readLocked(context));
        }
    }

    static void clear(Context context) {
        synchronized (LOCK) {
            writeLocked(context, Collections.emptyList());
        }
    }

    private static List<Event> readLocked(Context context) {
        ArrayList<Event> events = new ArrayList<>();
        AtomicFile file = file(context);
        if (!file.getBaseFile().isFile()) return events;
        try (FileInputStream input = file.openRead();
                BufferedReader reader = new BufferedReader(
                        new InputStreamReader(input, StandardCharsets.UTF_8))) {
            String line;
            while ((line = reader.readLine()) != null && events.size() < MAX_EVENTS) {
                if (!line.isEmpty()) events.add(Event.fromJson(new JSONObject(line)));
            }
        } catch (Exception error) {
            Log.e(TAG, "attention_journal_read_failed", error);
        }
        return events;
    }

    private static void writeLocked(Context context, List<Event> events) {
        AtomicFile file = file(context);
        FileOutputStream output = null;
        try {
            output = file.startWrite();
            OutputStreamWriter writer = new OutputStreamWriter(output, StandardCharsets.UTF_8);
            for (Event event : events) {
                writer.write(event.toJson().toString());
                writer.write('\n');
            }
            writer.flush();
            file.finishWrite(output);
        } catch (Exception error) {
            if (output != null) file.failWrite(output);
            Log.e(TAG, "attention_journal_write_failed", error);
        }
    }

    private static AtomicFile file(Context context) {
        File directory = new File(context.getFilesDir(), "attention");
        if (!directory.isDirectory() && !directory.mkdirs()) {
            Log.w(TAG, "attention_directory_unavailable path=" + directory);
        }
        return new AtomicFile(new File(directory, "events.jsonl"));
    }

    private static String bounded(String value) {
        if (value == null) return "";
        String clean = value.replace('\n', ' ').replace('\r', ' ').trim();
        return clean.length() <= MAX_FIELD ? clean : clean.substring(0, MAX_FIELD);
    }

    private SosAttentionStore() {}
}
