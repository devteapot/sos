package dev.gpui.mobile;

import android.app.role.RoleManager;
import android.content.Context;
import android.os.UserHandle;
import android.os.Process;
import android.util.Log;

import java.lang.reflect.Method;
import java.util.concurrent.Executor;
import java.util.function.Consumer;

import dev.sos.experience.BuildConfig;

/** Reasserts the immutable product HOME role without depending on Launcher state. */
final class SosHomePolicy {
    private static final String TAG = "SosHomePolicy";
    private static final String PACKAGE_NAME = "dev.sos.experience";
    private static volatile boolean requestInFlight;

    static void enforce(Context context, String reason) {
        if (!BuildConfig.SOS_HOME_ENABLED || requestInFlight) return;
        RoleManager roles = context.getSystemService(RoleManager.class);
        if (roles == null || !roles.isRoleAvailable(RoleManager.ROLE_HOME)
                || roles.isRoleHeld(RoleManager.ROLE_HOME)) {
            return;
        }
        try {
            Method add = RoleManager.class.getMethod(
                    "addRoleHolderAsUser", String.class, String.class, int.class,
                    UserHandle.class, Executor.class, Consumer.class);
            requestInFlight = true;
            Executor direct = Runnable::run;
            Consumer<Boolean> callback = accepted -> {
                requestInFlight = false;
                Log.i(TAG, "home_role_enforced accepted=" + accepted + " reason=" + reason);
            };
            add.invoke(roles, RoleManager.ROLE_HOME, PACKAGE_NAME, 0,
                    Process.myUserHandle(), direct, callback);
        } catch (ReflectiveOperationException | RuntimeException error) {
            requestInFlight = false;
            Log.e(TAG, "home_role_enforcement_failed reason=" + reason, error);
        }
    }

    private SosHomePolicy() {}
}
