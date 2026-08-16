package dev.sos.frameworkbridge;

import android.app.Application;
import android.net.Credentials;
import android.net.LocalServerSocket;
import android.net.LocalSocket;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import android.util.Slog;

import com.android.internal.widget.LockPatternUtils;
import com.android.internal.widget.LockscreenCredential;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.nio.CharBuffer;
import java.util.Arrays;

/** Direct-boot, non-rendering bridge from the native lock surface to LockSettingsService. */
public final class SosFrameworkBridgeApplication extends Application {
    private static final String TAG = "SosFrameworkBridge";
    private static final String SOCKET_NAME = "sos_framework_bridge";
    private static final int MAGIC = 0x534f5331; // SOS1
    private static final int COMMAND_STATUS = 1;
    private static final int COMMAND_VERIFY_PIN = 2;
    private static final int RESPONSE_OK = 1;
    private static final int RESPONSE_REJECTED = 2;
    private static final int RESPONSE_RETRY = 3;
    private static final int RESPONSE_ERROR = 4;
    private static final int MIN_PIN = 4;
    private static final int MAX_PIN = 64;

    @Override
    public void onCreate() {
        super.onCreate();
        Thread server = new Thread(this::serve, "sos-framework-bridge");
        server.setDaemon(true);
        server.start();
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
        if (command == COMMAND_STATUS) {
            writeHeader(output, RESPONSE_OK);
            LockPatternUtils locks = new LockPatternUtils(this);
            output.writeInt(locks.getCredentialTypeForUser(UserHandle.USER_SYSTEM));
            UserManager users = getSystemService(UserManager.class);
            output.writeBoolean(users != null && users.isUserUnlocked(UserHandle.SYSTEM));
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
}
