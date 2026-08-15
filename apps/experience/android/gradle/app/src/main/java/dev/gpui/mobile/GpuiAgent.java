package dev.gpui.mobile;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.text.InputType;
import android.util.Base64;
import android.view.WindowManager;
import android.widget.EditText;
import android.widget.Toast;

import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import java.util.Arrays;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** Trusted on-device bridge to Pi running in native Android/Bionic Node. */
public final class GpuiAgent {
    private static final String PREFS = "sos_agent_credentials";
    private static final String KEY_ALIAS = "sos.agent.credentials.v2";
    private static final String LEGACY_KEY_ALIAS = "sos.openai.api-key.v1";
    private static final String PREF_PROVIDER = "provider";
    private static final String PREF_CIPHERTEXT_PREFIX = "credential_ciphertext_";
    private static final String PREF_IV_PREFIX = "credential_iv_";
    private static final byte[] AAD_PREFIX = "dev.sos.experience/pi/v2/"
            .getBytes(StandardCharsets.UTF_8);

    private static final String PROVIDER_FAKE = "fake";
    private static final String PROVIDER_OPENAI = "openai";
    private static final String PROVIDER_OPENROUTER = "openrouter";
    private static final String PROVIDER_CODEX = "openai-codex";
    private static final String MODEL_OPENAI = "gpt-5.6-luna";
    private static final String MODEL_OPENROUTER = "openai/gpt-5.4-mini";
    private static final String MODEL_CODEX = "gpt-5.6-sol";

    private static final String NODE = "/system_ext/bin/sos-node";
    private static final String RUNNER = "/system_ext/etc/sos-agent/android-runner.cjs";
    private static final String API_DOC = "/system_ext/etc/sos-agent/experience-api.md";
    private static final String EXAMPLE_PRIMARY = "/system_ext/etc/sos-agent/example-primary.luau";
    private static final String EXAMPLE_SECONDARY = "/system_ext/etc/sos-agent/example-secondary.luau";

    private static final int MAX_PROCESS_BYTES = 2 * 1024 * 1024;
    private static final int MAX_SOURCE_BYTES = 256 * 1024;
    private static final int MAX_PROMPT_BYTES = 32 * 1024;
    private static final int MAX_PROMPT_DOCUMENT_BYTES = 1024 * 1024;
    private static final AtomicBoolean LOGIN_RUNNING = new AtomicBoolean(false);
    private static volatile String sActivity = "Deterministic fake provider ready";

    public static String status(Activity activity) {
        JSONObject result = new JSONObject();
        try {
            String provider = selectedProvider(activity);
            boolean configured = PROVIDER_FAKE.equals(provider) || hasCredential(activity, provider);
            result.put("provider", provider);
            result.put("configured", configured);
            result.put("activity", configured && !PROVIDER_FAKE.equals(provider)
                    ? providerLabel(provider) + " ready · " + model(provider) : sActivity);
        } catch (Exception ignored) {
            return "{\"provider\":\"fake\",\"configured\":true,"
                    + "\"activity\":\"Deterministic fake provider ready\"}";
        }
        return result.toString();
    }

    public static boolean configureOpenAi(Activity activity) {
        return configureApiKey(activity, PROVIDER_OPENAI, "OpenAI", "OpenAI project API key");
    }

    public static boolean configureOpenRouter(Activity activity) {
        return configureApiKey(activity, PROVIDER_OPENROUTER, "OpenRouter", "OpenRouter API key");
    }

    public static boolean configureCodex(Activity activity) {
        if (!LOGIN_RUNNING.compareAndSet(false, true)) {
            sActivity = "Codex sign-in is already running";
            return true;
        }
        try {
            GpuiAgentService.start(activity);
        } catch (Exception ignored) {
            LOGIN_RUNNING.set(false);
            sActivity = "Codex sign-in could not start its foreground session";
            return false;
        }
        sActivity = "Starting Codex subscription sign-in";
        Thread login = new Thread(() -> {
            try {
                runCodexLogin(activity);
            } catch (Exception ignored) {
                sActivity = "Codex sign-in failed";
            } finally {
                LOGIN_RUNNING.set(false);
                GpuiAgentService.stop(activity);
            }
        }, "sos-codex-login");
        login.start();
        return true;
    }

    public static boolean useFake(Activity activity) {
        if (!preferences(activity).edit().putString(PREF_PROVIDER, PROVIDER_FAKE).commit()) {
            return false;
        }
        sActivity = "Deterministic fake provider ready";
        return true;
    }

    public static boolean clearCredential(Activity activity) {
        activity.runOnUiThread(() -> new AlertDialog.Builder(activity)
                .setTitle("Remove agent credentials?")
                .setMessage("All encrypted OpenAI, OpenRouter, and Codex credentials will be deleted. The deterministic fake provider remains available.")
                .setNegativeButton("Cancel", null)
                .setPositiveButton("Remove", (dialog, which) -> {
                    try {
                        preferences(activity).edit().clear().commit();
                        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
                        store.load(null);
                        if (store.containsAlias(KEY_ALIAS)) store.deleteEntry(KEY_ALIAS);
                        if (store.containsAlias(LEGACY_KEY_ALIAS)) store.deleteEntry(LEGACY_KEY_ALIAS);
                        preferences(activity).edit().putString(PREF_PROVIDER, PROVIDER_FAKE).commit();
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
            String provider = selectedProvider(activity);
            if (PROVIDER_FAKE.equals(provider)) return failure("The fake provider runs in the trusted HOME");
            byte[] credential = decryptCredential(activity, provider);
            if (credential == null) return failure(providerLabel(provider) + " is not configured");
            try {
                GpuiAgentService.start(activity);
                sActivity = providerLabel(provider) + " · Pi is proposing an experience";
                return requestPi(activity, provider, prompt, currentSource, credential);
            } finally {
                GpuiAgentService.stop(activity);
                Arrays.fill(credential, (byte) 0);
                sActivity = providerLabel(provider) + " ready · " + model(provider);
            }
        } catch (Exception ignored) {
            sActivity = "Pi request failed";
            return failure("The trusted on-device Pi request failed");
        }
    }

    private static boolean configureApiKey(Activity activity, String provider, String label,
            String hint) {
        activity.runOnUiThread(() -> showCredentialDialog(activity, provider, label, hint));
        sActivity = "Waiting for " + label + " API key";
        return true;
    }

    private static void showCredentialDialog(Activity activity, String provider, String label,
            String hint) {
        EditText key = new EditText(activity);
        key.setSingleLine(true);
        key.setHint(hint);
        key.setSaveEnabled(false);
        key.setImportantForAutofill(EditText.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        key.setFilterTouchesWhenObscured(true);
        key.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        activity.getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        AlertDialog dialog = new AlertDialog.Builder(activity)
                .setTitle("Configure " + label)
                .setMessage("Stored with Android Keystore and passed only to native on-device Pi over an anonymous pipe. It is never exposed to Luau or logs.")
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
                        JSONObject credential = new JSONObject().put("type", "api_key")
                                .put("key", secret);
                        storeCredential(activity, provider, credential.toString());
                        key.getText().clear();
                        sActivity = label + " ready · " + model(provider);
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

    private static void runCodexLogin(Activity activity) throws Exception {
        Process process = startPi();
        JSONObject request = new JSONObject().put("action", "login")
                .put("provider", PROVIDER_CODEX);
        try (OutputStream output = process.getOutputStream()) {
            output.write(request.toString().getBytes(StandardCharsets.UTF_8));
        }
        drain(process.getErrorStream());
        AlertDialog[] progress = new AlertDialog[1];
        try (BufferedReader input = new BufferedReader(new InputStreamReader(
                process.getInputStream(), StandardCharsets.UTF_8))) {
            int bytes = 0;
            for (String line; (line = input.readLine()) != null;) {
                bytes += line.getBytes(StandardCharsets.UTF_8).length;
                if (bytes > MAX_PROCESS_BYTES) throw new IllegalStateException("login output too large");
                JSONObject event = new JSONObject(line);
                if ("auth_event".equals(event.optString("type"))) {
                    JSONObject auth = event.getJSONObject("event");
                    if ("device_code".equals(auth.optString("type"))) {
                        showDeviceCode(activity, process, auth.optString("verificationUri"),
                                auth.optString("userCode"), progress);
                    } else if ("progress".equals(auth.optString("type"))) {
                        sActivity = auth.optString("message", "Codex sign-in is in progress");
                    }
                } else if ("login_complete".equals(event.optString("type"))) {
                    storeCredential(activity, PROVIDER_CODEX,
                            event.getJSONObject("credential").toString());
                    sActivity = "Codex subscription ready · " + MODEL_CODEX;
                } else if ("error".equals(event.optString("type"))) {
                    throw new IllegalStateException(event.optString("error", "Codex sign-in failed"));
                }
            }
        } finally {
            activity.runOnUiThread(() -> {
                if (progress[0] != null) progress[0].dismiss();
                activity.getWindow().clearFlags(WindowManager.LayoutParams.FLAG_SECURE);
            });
        }
        if (!process.waitFor(10, TimeUnit.SECONDS) || process.exitValue() != 0
                || !hasCredential(activity, PROVIDER_CODEX)) {
            process.destroyForcibly();
            throw new IllegalStateException("Codex sign-in did not complete");
        }
    }

    private static void showDeviceCode(Activity activity, Process process, String url, String code,
            AlertDialog[] target) {
        activity.runOnUiThread(() -> {
            activity.getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
            ClipboardManager clipboard = (ClipboardManager) activity.getSystemService(
                    Context.CLIPBOARD_SERVICE);
            if (clipboard != null) clipboard.setPrimaryClip(ClipData.newPlainText("Codex code", code));
            AlertDialog dialog = new AlertDialog.Builder(activity)
                    .setTitle("Finish Codex sign-in")
                    .setMessage("The one-time code has been copied:\n\n" + code
                            + "\n\nComplete sign-in in the browser. This dialog closes automatically.")
                    .setNegativeButton("Cancel", (ignored, which) -> process.destroyForcibly())
                    .create();
            target[0] = dialog;
            dialog.show();
            try {
                activity.startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
                Toast.makeText(activity, "Codex code copied", Toast.LENGTH_LONG).show();
            } catch (Exception ignored) {
                sActivity = "Open " + url + " and enter the displayed code";
            }
        });
    }

    private static boolean validCredential(String secret) {
        if (secret == null || secret.length() < 20 || secret.length() > 512) return false;
        for (int index = 0; index < secret.length(); index++) {
            if (Character.isWhitespace(secret.charAt(index))
                    || Character.isISOControl(secret.charAt(index))) return false;
        }
        return true;
    }

    private static void storeCredential(Activity activity, String provider, String document)
            throws Exception {
        SecretKey key = getOrCreateKey();
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.ENCRYPT_MODE, key);
        cipher.updateAAD(aad(provider));
        byte[] plaintext = document.getBytes(StandardCharsets.UTF_8);
        byte[] ciphertext;
        try {
            ciphertext = cipher.doFinal(plaintext);
        } finally {
            Arrays.fill(plaintext, (byte) 0);
        }
        boolean committed = preferences(activity).edit()
                .putString(PREF_CIPHERTEXT_PREFIX + provider,
                        Base64.encodeToString(ciphertext, Base64.NO_WRAP))
                .putString(PREF_IV_PREFIX + provider,
                        Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP))
                .putString(PREF_PROVIDER, provider)
                .commit();
        Arrays.fill(ciphertext, (byte) 0);
        if (!committed) throw new IllegalStateException("credential preferences commit failed");
    }

    private static byte[] decryptCredential(Activity activity, String provider) throws Exception {
        SharedPreferences prefs = preferences(activity);
        String encodedCiphertext = prefs.getString(PREF_CIPHERTEXT_PREFIX + provider, null);
        String encodedIv = prefs.getString(PREF_IV_PREFIX + provider, null);
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
            cipher.updateAAD(aad(provider));
            return cipher.doFinal(ciphertext);
        } finally {
            Arrays.fill(ciphertext, (byte) 0);
            Arrays.fill(iv, (byte) 0);
        }
    }

    private static byte[] aad(String provider) {
        byte[] suffix = provider.getBytes(StandardCharsets.UTF_8);
        byte[] aad = Arrays.copyOf(AAD_PREFIX, AAD_PREFIX.length + suffix.length);
        System.arraycopy(suffix, 0, aad, AAD_PREFIX.length, suffix.length);
        Arrays.fill(suffix, (byte) 0);
        return aad;
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

    private static boolean hasCredential(Activity activity, String provider) throws Exception {
        if (PROVIDER_FAKE.equals(provider)) return true;
        SharedPreferences prefs = preferences(activity);
        if (!prefs.contains(PREF_CIPHERTEXT_PREFIX + provider)
                || !prefs.contains(PREF_IV_PREFIX + provider)) return false;
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        return store.containsAlias(KEY_ALIAS);
    }

    private static String selectedProvider(Activity activity) {
        String provider = preferences(activity).getString(PREF_PROVIDER, PROVIDER_FAKE);
        if (PROVIDER_OPENAI.equals(provider) || PROVIDER_OPENROUTER.equals(provider)
                || PROVIDER_CODEX.equals(provider)) return provider;
        return PROVIDER_FAKE;
    }

    private static SharedPreferences preferences(Activity activity) {
        return activity.getSharedPreferences(PREFS, Activity.MODE_PRIVATE);
    }

    private static String requestPi(Activity activity, String provider, String prompt,
            String source, byte[] credentialBytes) throws Exception {
        JSONObject request = new JSONObject()
                .put("action", "prompt")
                .put("provider", provider)
                .put("model", model(provider))
                .put("credential", new JSONObject(new String(credentialBytes, StandardCharsets.UTF_8)))
                .put("prompt", prompt)
                .put("currentSource", source)
                .put("systemPrompt", systemPrompt());
        byte[] body = request.toString().getBytes(StandardCharsets.UTF_8);
        if (body.length > MAX_PROMPT_DOCUMENT_BYTES) return failure("The Pi request is too large");
        Process process = startPi();
        AtomicReference<byte[]> stdout = new AtomicReference<>();
        AtomicReference<Exception> readFailure = new AtomicReference<>();
        Thread outputReader = new Thread(() -> {
            try {
                stdout.set(readBoundedBytes(process.getInputStream(), MAX_PROCESS_BYTES));
            } catch (Exception error) {
                readFailure.set(error);
            }
        }, "sos-pi-output");
        outputReader.start();
        drain(process.getErrorStream());
        try (OutputStream output = process.getOutputStream()) {
            output.write(body);
        } finally {
            Arrays.fill(body, (byte) 0);
        }
        if (!process.waitFor(240, TimeUnit.SECONDS)) {
            process.destroyForcibly();
            throw new IllegalStateException("Pi request timed out");
        }
        outputReader.join(5000);
        if (readFailure.get() != null) throw readFailure.get();
        byte[] responseBytes = stdout.get();
        if (responseBytes == null) throw new IllegalStateException("Pi returned no response");
        try {
            String[] lines = new String(responseBytes, StandardCharsets.UTF_8).trim().split("\\n");
            JSONObject response = new JSONObject(lines[lines.length - 1]);
            if (!"prompt_complete".equals(response.optString("type"))) {
                return failure(response.optString("error", "Pi did not produce an experience"));
            }
            String candidate = response.getString("source");
            if (candidate.isEmpty()
                    || candidate.getBytes(StandardCharsets.UTF_8).length > MAX_SOURCE_BYTES) {
                return failure("Pi proposed an invalid source size");
            }
            storeCredential(activity, provider, response.getJSONObject("credential").toString());
            String summary = response.optString("summary",
                    "Pi proposed a complete replacement experience.");
            if (summary.length() > 2048) summary = summary.substring(0, 2048);
            return new JSONObject().put("ok", true).put("source", candidate)
                    .put("summary", summary).toString();
        } finally {
            Arrays.fill(responseBytes, (byte) 0);
        }
    }

    private static Process startPi() throws Exception {
        return new ProcessBuilder(NODE, RUNNER).start();
    }

    private static void drain(InputStream stream) {
        Thread thread = new Thread(() -> {
            try {
                byte[] buffer = new byte[4096];
                int total = 0;
                for (int read; (read = stream.read(buffer)) >= 0;) {
                    total += read;
                    if (total > MAX_PROCESS_BYTES) break;
                }
            } catch (Exception ignored) {
                // Pi stderr is deliberately neither persisted nor surfaced.
            } finally {
                try { stream.close(); } catch (Exception ignored) {}
            }
        }, "sos-pi-stderr");
        thread.start();
    }

    private static byte[] readBoundedBytes(InputStream input, int maximum) throws Exception {
        try (InputStream stream = input; ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[8192];
            int total = 0;
            for (int read; (read = stream.read(buffer)) >= 0;) {
                total += read;
                if (total > maximum) throw new IllegalStateException("Pi response is too large");
                output.write(buffer, 0, read);
            }
            return output.toByteArray();
        }
    }

    private static String readDocument(String path) throws Exception {
        byte[] bytes = readBoundedBytes(new FileInputStream(path), MAX_PROMPT_DOCUMENT_BYTES);
        try {
            return new String(bytes, StandardCharsets.UTF_8);
        } finally {
            Arrays.fill(bytes, (byte) 0);
        }
    }

    private static String systemPrompt() throws Exception {
        return "You are the resident SOS experience author. You modify the currently running visual experience in response to the user's direct request.\n\n"
                + "Rules:\n- Always call get_experience_context first.\n"
                + "- Return complete Luau module source, never a patch.\n"
                + "- Call validate_experience before submit_experience.\n"
                + "- Submit only the exact source that validated.\n"
                + "- Android Pi stages the candidate; the trusted HOME independently compiles, renders, validates, and activates it after Pi exits. Never claim activation from the staging response.\n"
                + "- You have no shell, filesystem, process, or general network tools.\n"
                + "- Preserve the user's current intent and durable state unless they ask for a reset.\n"
                + "- Every revision must keep a visible Luau-authored agent conversation/composer that renders model.agent and emits agent.prompt.\n\n"
                + "SOS experience API:\n" + readDocument(API_DOC) + "\n\n"
                + "Reference experiences:\n" + readDocument(EXAMPLE_PRIMARY) + "\n\n---\n\n"
                + readDocument(EXAMPLE_SECONDARY);
    }

    private static String model(String provider) {
        if (PROVIDER_OPENAI.equals(provider)) return MODEL_OPENAI;
        if (PROVIDER_OPENROUTER.equals(provider)) return MODEL_OPENROUTER;
        if (PROVIDER_CODEX.equals(provider)) return MODEL_CODEX;
        return "faux";
    }

    private static String providerLabel(String provider) {
        if (PROVIDER_OPENAI.equals(provider)) return "OpenAI";
        if (PROVIDER_OPENROUTER.equals(provider)) return "OpenRouter";
        if (PROVIDER_CODEX.equals(provider)) return "Codex subscription";
        return "Deterministic fake provider";
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
