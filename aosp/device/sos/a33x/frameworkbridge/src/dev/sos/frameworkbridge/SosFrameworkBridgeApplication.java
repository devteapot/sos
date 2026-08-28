package dev.sos.frameworkbridge;

import android.app.Application;
import android.app.NotificationManager;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.net.Credentials;
import android.net.LocalServerSocket;
import android.net.LocalSocket;
import android.net.LocalSocketAddress;
import android.os.Process;
import android.os.SystemClock;
import android.os.SystemProperties;
import android.os.UserHandle;
import android.os.UserManager;
import android.util.Slog;

import com.android.internal.widget.LockPatternUtils;
import com.android.internal.widget.LockscreenCredential;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.CharBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/** Direct-boot bridge for credentials, typed providers, and fixed trusted administration. */
public final class SosFrameworkBridgeApplication extends Application {
    private static final String TAG = "SosFrameworkBridge";
    private static final String SOCKET_NAME = "sos_framework_bridge";
    private static final int MAGIC = 0x534f5331; // SOS1
    private static final int COMMAND_STATUS = 1;
    private static final int COMMAND_VERIFY_PIN = 2;
    private static final int COMMAND_START_HOME = 3;
    private static final int COMMAND_PROVIDER_SNAPSHOT = 4;
    private static final int COMMAND_PROVIDER_ACTION = 5;
    private static final int RESPONSE_OK = 1;
    private static final int RESPONSE_REJECTED = 2;
    private static final int RESPONSE_RETRY = 3;
    private static final int RESPONSE_ERROR = 4;
    private static final int MIN_PIN = 4;
    private static final int MAX_PIN = 64;
    private static final int MAX_PROVIDER_ACTION_BYTES = 64 * 1024;
    private static final String CONTROL_SOCKET_NAME = "sos_native_shell_control";
    private static final int CONTROL_MAGIC = 0x534f5332; // SOS2
    private static final int CONTROL_LOCK = 1;
    private static final int CONTROL_HOME_FAILED = 2;
    private static final int CONTROL_ACK_TIMEOUT_MS = 10_000;
    private static final String SOS_PACKAGE = "dev.sos.experience";
    private static final String HOME_HEARTBEAT_ACTION = "dev.sos.action.HOME_HEARTBEAT";
    private static final String HOME_HEARTBEAT_PERMISSION =
            "dev.sos.permission.REPORT_HOME_HEARTBEAT";
    private static final long HOME_MONITOR_INITIAL_DELAY_MS = 30_000;
    private static final long HOME_MONITOR_INTERVAL_MS = 2_000;
    private static final long HOME_HEARTBEAT_TIMEOUT_MS = 16_000;

    private volatile long lastHomeHeartbeatMs;
    private volatile boolean homeFailureReported;

    private final BroadcastReceiver screenOffReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (intent != null && Intent.ACTION_SCREEN_OFF.equals(intent.getAction())) {
                notifyControlAsync(CONTROL_LOCK, "screen-off", goAsync());
            }
        }
    };
    private final BroadcastReceiver homeHeartbeatReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (intent != null && HOME_HEARTBEAT_ACTION.equals(intent.getAction())) {
                lastHomeHeartbeatMs = SystemClock.elapsedRealtime();
                homeFailureReported = false;
            }
        }
    };

    @Override
    public void onCreate() {
        super.onCreate();
        Thread server = new Thread(this::serve, "sos-framework-bridge");
        server.setDaemon(true);
        server.start();
        grantProviderListenerAccess();
        if (!isCompatStage()) {
            Slog.i(TAG, "framework_bridge_mode=credential-only");
            return;
        }
        registerReceiver(screenOffReceiver, new IntentFilter(Intent.ACTION_SCREEN_OFF),
                Context.RECEIVER_NOT_EXPORTED);
        registerReceiver(homeHeartbeatReceiver, new IntentFilter(HOME_HEARTBEAT_ACTION),
                HOME_HEARTBEAT_PERMISSION, null, Context.RECEIVER_EXPORTED);
        Thread monitor = new Thread(this::monitorHome, "sos-home-monitor");
        monitor.setDaemon(true);
        monitor.start();
    }

    private void serve() {
        try (LocalServerSocket server = new LocalServerSocket(SOCKET_NAME)) {
            Slog.i(TAG, "framework_bridge_ready transport=local_socket rendered_ui=false");
            for (;;) {
                try (LocalSocket client = server.accept()) {
                    Credentials peer = client.getPeerCredentials();
                    if (peer == null || peer.getUid() != Process.SYSTEM_UID) {
                        Slog.w(TAG, "framework_bridge_peer_rejected");
                        continue;
                    }
                    handle(client);
                } catch (IOException | RuntimeException error) {
                    Slog.e(TAG, "framework_bridge_client_failed", error);
                }
            }
        } catch (IOException | RuntimeException error) {
            Slog.wtf(TAG, "framework_bridge_stopped", error);
        }
    }

    private void handle(LocalSocket client) throws IOException {
        DataInputStream input = new DataInputStream(client.getInputStream());
        DataOutputStream output = new DataOutputStream(client.getOutputStream());
        if (input.readInt() != MAGIC) throw new IOException("bad protocol magic");
        int command = input.readUnsignedByte();
        if (command == COMMAND_PROVIDER_SNAPSHOT) {
            try {
                writeProviderResponse(output, RESPONSE_OK,
                        SosSystemProviders.snapshot(this));
            } catch (Exception error) {
                Slog.e(TAG, "framework_provider_snapshot_failed", error);
                writeProviderResponse(output, RESPONSE_ERROR,
                        "{\"error\":\"provider snapshot failed\"}");
            }
            return;
        }
        if (command == COMMAND_PROVIDER_ACTION) {
            int length = input.readInt();
            if (length <= 0 || length > MAX_PROVIDER_ACTION_BYTES) {
                writeProviderResponse(output, RESPONSE_ERROR,
                        "{\"error\":\"invalid provider action length\"}");
                return;
            }
            byte[] encoded = new byte[length];
            input.readFully(encoded);
            try {
                String result = SosSystemProviders.execute(
                        this, new String(encoded, StandardCharsets.UTF_8));
                writeProviderResponse(output, RESPONSE_OK, result);
            } catch (Exception error) {
                Slog.e(TAG, "framework_provider_action_failed", error);
                writeProviderResponse(output, RESPONSE_ERROR,
                        "{\"error\":\"provider action rejected\"}");
            } finally {
                Arrays.fill(encoded, (byte) 0);
            }
            return;
        }
        if (command == COMMAND_STATUS) {
            writeHeader(output, RESPONSE_OK);
            LockPatternUtils locks = new LockPatternUtils(this);
            output.writeInt(locks.getCredentialTypeForUser(UserHandle.USER_SYSTEM));
            UserManager users = getSystemService(UserManager.class);
            output.writeBoolean(users != null && users.isUserUnlocked(UserHandle.SYSTEM));
            output.flush();
            return;
        }
        if (command == COMMAND_START_HOME) {
            boolean started = startSosHome();
            writeHeader(output, started ? RESPONSE_OK : RESPONSE_ERROR);
            output.writeInt(0);
            output.flush();
            return;
        }
        if (command != COMMAND_VERIFY_PIN) throw new IOException("unknown command");
        int length = input.readUnsignedShort();
        if (length < MIN_PIN || length > MAX_PIN) {
            writeHeader(output, RESPONSE_ERROR);
            output.writeInt(0);
            output.flush();
            return;
        }
        byte[] encoded = new byte[length];
        char[] pin = new char[length];
        try {
            input.readFully(encoded);
            for (int index = 0; index < length; index++) {
                int value = encoded[index] & 0xff;
                if (value < '0' || value > '9') {
                    writeHeader(output, RESPONSE_ERROR);
                    output.writeInt(0);
                    output.flush();
                    return;
                }
                pin[index] = (char) value;
            }
            LockPatternUtils locks = new LockPatternUtils(this);
            try (LockscreenCredential credential =
                    LockscreenCredential.createPin(CharBuffer.wrap(pin))) {
                boolean matched = locks.checkCredential(
                        credential, UserHandle.USER_SYSTEM, null);
                writeHeader(output, matched ? RESPONSE_OK : RESPONSE_REJECTED);
                output.writeInt(0);
                output.flush();
                Slog.i(TAG, "credential_result matched=" + matched);
            } catch (LockPatternUtils.RequestThrottledException throttled) {
                writeHeader(output, RESPONSE_RETRY);
                output.writeInt(throttled.getTimeoutMs());
                output.flush();
                Slog.w(TAG, "credential_throttled timeout_ms=" + throttled.getTimeoutMs());
            }
        } catch (EOFException error) {
            throw error;
        } finally {
            Arrays.fill(encoded, (byte) 0);
            Arrays.fill(pin, '\0');
        }
    }

    private static void writeHeader(DataOutputStream output, int response) throws IOException {
        output.writeInt(MAGIC);
        output.writeByte(response);
    }

    private static void writeProviderResponse(DataOutputStream output, int response,
            String document) throws IOException {
        byte[] encoded = document.getBytes(StandardCharsets.UTF_8);
        writeHeader(output, response);
        output.writeInt(encoded.length);
        output.write(encoded);
        output.flush();
    }

    private void grantProviderListenerAccess() {
        NotificationManager notifications = getSystemService(NotificationManager.class);
        if (notifications == null) return;
        ComponentName component = new ComponentName(
                this, SosProviderNotificationListenerService.class);
        try {
            Method grant = NotificationManager.class.getMethod(
                    "setNotificationListenerAccessGranted", ComponentName.class, boolean.class);
            grant.invoke(notifications, component, true);
            Slog.i(TAG, "framework_provider_attention_access granted=true");
        } catch (ReflectiveOperationException | RuntimeException error) {
            Slog.w(TAG, "framework_provider_attention_access granted=false", error);
        }
    }

    private boolean startSosHome() {
        if (!isCompatStage()) {
            Slog.w(TAG, "framework_bridge_home_start result=wrong-stage");
            return false;
        }
        UserManager users = getSystemService(UserManager.class);
        if (users == null || !users.isUserUnlocked(UserHandle.SYSTEM)) {
            // NativeActivity is intentionally credential-encrypted and must
            // never be initialized in the persistent direct-boot adapter.
            Slog.w(TAG, "framework_bridge_home_start result=user-locked");
            return false;
        }
        Intent home = new Intent(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_HOME)
                .setPackage(SOS_PACKAGE)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        try {
            startActivity(home);
            // Allow a bounded startup interval. A real heartbeat must arrive
            // before the next failure is considered recovered.
            lastHomeHeartbeatMs = SystemClock.elapsedRealtime();
            homeFailureReported = false;
            Slog.i(TAG, "framework_bridge_home_start result=accepted");
            return true;
        } catch (RuntimeException error) {
            Slog.e(TAG, "framework_bridge_home_start result=failed", error);
            return false;
        }
    }

    private void monitorHome() {
        SystemClock.sleep(HOME_MONITOR_INITIAL_DELAY_MS);
        for (;;) {
            UserManager users = getSystemService(UserManager.class);
            if (users == null || !users.isUserUnlocked(UserHandle.SYSTEM)) {
                homeFailureReported = false;
                SystemClock.sleep(HOME_MONITOR_INTERVAL_MS);
                continue;
            }
            long heartbeatAge = SystemClock.elapsedRealtime() - lastHomeHeartbeatMs;
            if (lastHomeHeartbeatMs == 0 || heartbeatAge > HOME_HEARTBEAT_TIMEOUT_MS) {
                if (!homeFailureReported) {
                    homeFailureReported = true;
                    notifyControl(CONTROL_HOME_FAILED, "home-heartbeat-missing");
                }
            } else {
                homeFailureReported = false;
            }
            SystemClock.sleep(HOME_MONITOR_INTERVAL_MS);
        }
    }

    private void notifyControlAsync(int command, String reason,
            BroadcastReceiver.PendingResult pendingResult) {
        Thread sender = new Thread(() -> {
            try {
                notifyControl(command, reason);
            } finally {
                pendingResult.finish();
            }
        },
                "sos-native-control-" + command);
        sender.setDaemon(true);
        sender.start();
    }

    private void notifyControl(int command, String reason) {
        try (LocalSocket socket = new LocalSocket()) {
            socket.connect(new LocalSocketAddress(
                    CONTROL_SOCKET_NAME, LocalSocketAddress.Namespace.ABSTRACT));
            socket.setSoTimeout(CONTROL_ACK_TIMEOUT_MS);
            DataOutputStream output = new DataOutputStream(socket.getOutputStream());
            output.writeInt(CONTROL_MAGIC);
            output.writeByte(command);
            output.flush();
            DataInputStream input = new DataInputStream(socket.getInputStream());
            if (input.readUnsignedByte() != RESPONSE_OK) {
                throw new IOException("native control did not become ready");
            }
            Slog.i(TAG, "native_control_acknowledged command=" + command
                    + " reason=" + reason);
        } catch (IOException | RuntimeException error) {
            Slog.w(TAG, "native_control_unavailable command=" + command
                    + " reason=" + reason, error);
        }
    }

    private static boolean isCompatStage() {
        return "compat".equals(SystemProperties.get("ro.sos.core.stage", ""));
    }
}
