package dev.gpui.mobile;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.SharedPreferences;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.text.InputType;
import android.util.Base64;
import android.view.WindowManager;
import android.widget.EditText;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import java.util.Arrays;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** Trusted on-device agent bridge. API-key plaintext never crosses JNI. */
public final class GpuiAgent {
    private static final String PREFS = "sos_agent_credentials";
    private static final String KEY_ALIAS = "sos.openai.api-key.v1";
    private static final String PREF_CIPHERTEXT = "openai_ciphertext";
    private static final String PREF_IV = "openai_iv";
    private static final String PREF_LIVE = "use_openai";
    private static final byte[] AAD = "dev.sos.experience/openai/v1"
            .getBytes(StandardCharsets.UTF_8);
    private static final int MAX_HTTP_BYTES = 1024 * 1024;
    private static final int MAX_SOURCE_BYTES = 256 * 1024;
    private static final int MAX_PROMPT_BYTES = 32 * 1024;
    private static final String MODEL = "gpt-5.6-luna";
    private static volatile String sActivity = "Deterministic fake provider ready";

    public static String status(Activity activity) {
        JSONObject result = new JSONObject();
        try {
            boolean configured = hasCredential(activity);
            boolean live = configured && preferences(activity).getBoolean(PREF_LIVE, false);
            result.put("provider", live ? "openai" : "fake");
            result.put("configured", configured);
            result.put("activity", live ? "OpenAI ready · " + MODEL : sActivity);
        } catch (Exception ignored) {
            return "{\"provider\":\"fake\",\"configured\":false,"
                    + "\"activity\":\"Deterministic fake provider ready\"}";
        }
        return result.toString();
    }

    public static boolean configureOpenAi(Activity activity) {
        activity.runOnUiThread(() -> showCredentialDialog(activity));
        sActivity = "Waiting for API key";
        return true;
    }

    public static boolean useFake(Activity activity) {
        if (!preferences(activity).edit().putBoolean(PREF_LIVE, false).commit()) return false;
        sActivity = "Deterministic fake provider ready";
        return true;
    }

    public static boolean clearCredential(Activity activity) {
        activity.runOnUiThread(() -> new AlertDialog.Builder(activity)
                .setTitle("Remove OpenAI credential?")
                .setMessage("The encrypted API key will be deleted. The deterministic fake provider remains available.")
                .setNegativeButton("Cancel", null)
                .setPositiveButton("Remove", (dialog, which) -> {
                    try {
                        preferences(activity).edit().clear().commit();
                        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
                        store.load(null);
                        if (store.containsAlias(KEY_ALIAS)) store.deleteEntry(KEY_ALIAS);
                        sActivity = "Deterministic fake provider ready";
                    } catch (Exception ignored) {
                        sActivity = "Credential removal failed";
                    }
                })
                .show());
        return true;
    }

    public static String run(Activity activity, String prompt, String currentSource) {
        try {
            if (prompt == null || prompt.trim().isEmpty()
                    || prompt.getBytes(StandardCharsets.UTF_8).length > MAX_PROMPT_BYTES) {
                return failure("The agent prompt is outside its bounded size");
            }
            if (currentSource == null || currentSource.isEmpty()
                    || currentSource.getBytes(StandardCharsets.UTF_8).length > MAX_SOURCE_BYTES) {
                return failure("The active experience is outside its bounded size");
            }
            byte[] keyBytes = decryptCredential(activity);
            if (keyBytes == null) return failure("OpenAI is not configured");
            try {
                sActivity = "OpenAI is proposing an experience";
                return requestCandidate(new String(keyBytes, StandardCharsets.UTF_8), prompt,
                        currentSource);
            } finally {
                Arrays.fill(keyBytes, (byte) 0);
                sActivity = "OpenAI ready · " + MODEL;
            }
        } catch (Exception ignored) {
            sActivity = "OpenAI request failed";
            return failure("The trusted OpenAI request failed");
        }
    }

    private static void showCredentialDialog(Activity activity) {
        EditText key = new EditText(activity);
        key.setSingleLine(true);
        key.setHint("OpenAI project API key");
        key.setSaveEnabled(false);
        key.setImportantForAutofill(EditText.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        key.setFilterTouchesWhenObscured(true);
        key.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        activity.getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        AlertDialog dialog = new AlertDialog.Builder(activity)
                .setTitle("Configure OpenAI")
                .setMessage("Stored with Android Keystore. The key is never exposed to Luau or the agent conversation.")
                .setView(key)
                .setNegativeButton("Cancel", (ignored, which) -> {
                    key.getText().clear();
                    sActivity = "Deterministic fake provider ready";
                })
                .setPositiveButton("Save", null)
                .create();
        dialog.setOnShowListener(ignored -> dialog.getButton(AlertDialog.BUTTON_POSITIVE)
                .setOnClickListener(button -> {
                    String secret = key.getText().toString();
                    if (!validCredential(secret)) {
                        key.setError("Enter a valid API key without spaces");
                        return;
                    }
                    try {
                        storeCredential(activity, secret);
                        key.getText().clear();
                        sActivity = "OpenAI ready · " + MODEL;
                        dialog.dismiss();
                    } catch (Exception error) {
                        key.getText().clear();
                        key.setError("Android Keystore could not store the credential");
                        sActivity = "Credential storage failed";
                    }
                }));
        dialog.setOnDismissListener(ignored -> {
            key.getText().clear();
            activity.getWindow().clearFlags(WindowManager.LayoutParams.FLAG_SECURE);
        });
        dialog.show();
    }

    private static boolean validCredential(String secret) {
        if (secret == null || secret.length() < 20 || secret.length() > 512) return false;
        for (int index = 0; index < secret.length(); index++) {
            if (Character.isWhitespace(secret.charAt(index))
                    || Character.isISOControl(secret.charAt(index))) return false;
        }
        return true;
    }

    private static void storeCredential(Activity activity, String secret) throws Exception {
        SecretKey key = getOrCreateKey();
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.ENCRYPT_MODE, key);
        cipher.updateAAD(AAD);
        byte[] plaintext = secret.getBytes(StandardCharsets.UTF_8);
        byte[] ciphertext;
        try {
            ciphertext = cipher.doFinal(plaintext);
        } finally {
            Arrays.fill(plaintext, (byte) 0);
        }
        boolean committed = preferences(activity).edit()
                .putString(PREF_CIPHERTEXT, Base64.encodeToString(ciphertext, Base64.NO_WRAP))
                .putString(PREF_IV, Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP))
                .putBoolean(PREF_LIVE, true)
                .commit();
        Arrays.fill(ciphertext, (byte) 0);
        if (!committed) throw new IllegalStateException("credential preferences commit failed");
    }

    private static byte[] decryptCredential(Activity activity) throws Exception {
        SharedPreferences prefs = preferences(activity);
        String encodedCiphertext = prefs.getString(PREF_CIPHERTEXT, null);
        String encodedIv = prefs.getString(PREF_IV, null);
        if (encodedCiphertext == null || encodedIv == null) return null;
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        SecretKey key = (SecretKey) store.getKey(KEY_ALIAS, null);
        if (key == null) return null;
        byte[] ciphertext = Base64.decode(encodedCiphertext, Base64.NO_WRAP);
        byte[] iv = Base64.decode(encodedIv, Base64.NO_WRAP);
        try {
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(128, iv));
            cipher.updateAAD(AAD);
            return cipher.doFinal(ciphertext);
        } finally {
            Arrays.fill(ciphertext, (byte) 0);
            Arrays.fill(iv, (byte) 0);
        }
    }

    private static SecretKey getOrCreateKey() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        SecretKey existing = (SecretKey) store.getKey(KEY_ALIAS, null);
        if (existing != null) return existing;
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES,
                "AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .setUnlockedDeviceRequired(true)
                .build());
        return generator.generateKey();
    }

    private static boolean hasCredential(Activity activity) throws Exception {
        SharedPreferences prefs = preferences(activity);
        if (!prefs.contains(PREF_CIPHERTEXT) || !prefs.contains(PREF_IV)) return false;
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        return store.containsAlias(KEY_ALIAS);
    }

    private static SharedPreferences preferences(Activity activity) {
        return activity.getSharedPreferences(PREFS, Activity.MODE_PRIVATE);
    }

    private static String requestCandidate(String apiKey, String prompt, String source)
            throws Exception {
        JSONObject parameters = new JSONObject()
                .put("type", "object")
                .put("properties", new JSONObject()
                        .put("source", new JSONObject().put("type", "string")
                                .put("description", "Complete API-v3 Luau experience source"))
                        .put("summary", new JSONObject().put("type", "string")
                                .put("description", "Short user-facing description")))
                .put("required", new JSONArray().put("source").put("summary"))
                .put("additionalProperties", false);
        JSONObject tool = new JSONObject()
                .put("type", "function")
                .put("name", "propose_experience")
                .put("description", "Propose one complete replacement SOS Luau experience. The phone validates and activates it transactionally.")
                .put("parameters", parameters)
                .put("strict", true);
        String input = "<user_request>\n" + prompt + "\n</user_request>\n"
                + "<active_experience>\n" + source + "\n</active_experience>";
        JSONObject request = new JSONObject()
                .put("model", MODEL)
                .put("instructions", "You are the resident SOS experience author. Preserve API version 3, return a complete self-contained Luau source, keep all provider effects within the existing bounded capabilities, and satisfy the user's requested experiential change. Call propose_experience exactly once.")
                .put("input", input)
                .put("tools", new JSONArray().put(tool))
                .put("tool_choice", new JSONObject().put("type", "function")
                        .put("name", "propose_experience"))
                .put("parallel_tool_calls", false)
                .put("max_output_tokens", 24000)
                .put("store", false);

        byte[] body = request.toString().getBytes(StandardCharsets.UTF_8);
        if (body.length > 512 * 1024) return failure("The OpenAI request is too large");
        HttpURLConnection connection = (HttpURLConnection) new URL(
                "https://api.openai.com/v1/responses").openConnection();
        try {
            connection.setConnectTimeout(15_000);
            connection.setReadTimeout(180_000);
            connection.setInstanceFollowRedirects(false);
            connection.setRequestMethod("POST");
            connection.setDoOutput(true);
            connection.setRequestProperty("Authorization", "Bearer " + apiKey);
            connection.setRequestProperty("Content-Type", "application/json");
            connection.setRequestProperty("User-Agent", "SOS-Android-Agent/1");
            connection.setFixedLengthStreamingMode(body.length);
            try (OutputStream output = connection.getOutputStream()) {
                output.write(body);
            } finally {
                Arrays.fill(body, (byte) 0);
            }
            int code = connection.getResponseCode();
            InputStream stream = code >= 200 && code < 300
                    ? connection.getInputStream() : connection.getErrorStream();
            String response = stream == null ? "" : readBounded(stream);
            if (code < 200 || code >= 300) {
                if (code == 401) return failure("OpenAI rejected the configured credential");
                if (code == 429) return failure("OpenAI rate or spend limit reached");
                return failure("OpenAI request failed with HTTP " + code);
            }
            return decodeCandidate(response);
        } finally {
            connection.disconnect();
        }
    }

    private static String readBounded(InputStream stream) throws Exception {
        try (InputStream input = stream; ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[8192];
            int total = 0;
            for (;;) {
                int read = input.read(buffer);
                if (read < 0) break;
                total += read;
                if (total > MAX_HTTP_BYTES) throw new IllegalStateException("response too large");
                output.write(buffer, 0, read);
            }
            return output.toString(StandardCharsets.UTF_8.name());
        }
    }

    private static String decodeCandidate(String response) throws Exception {
        JSONArray output = new JSONObject(response).getJSONArray("output");
        for (int index = 0; index < output.length(); index++) {
            JSONObject item = output.getJSONObject(index);
            if (!"function_call".equals(item.optString("type"))
                    || !"propose_experience".equals(item.optString("name"))) continue;
            JSONObject arguments = new JSONObject(item.getString("arguments"));
            String source = arguments.getString("source");
            String summary = arguments.getString("summary");
            if (source.isEmpty() || source.getBytes(StandardCharsets.UTF_8).length > MAX_SOURCE_BYTES)
                return failure("OpenAI proposed an invalid source size");
            if (summary.length() > 2048) summary = summary.substring(0, 2048);
            return new JSONObject().put("ok", true).put("source", source)
                    .put("summary", summary).toString();
        }
        return failure("OpenAI did not propose an experience");
    }

    private static String failure(String error) {
        try {
            return new JSONObject().put("ok", false).put("error", error).toString();
        } catch (Exception ignored) {
            return "{\"ok\":false,\"error\":\"Agent request failed\"}";
        }
    }

    private GpuiAgent() {}
}
