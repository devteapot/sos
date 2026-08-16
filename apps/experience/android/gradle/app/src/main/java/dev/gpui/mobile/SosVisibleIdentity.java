package dev.gpui.mobile;

import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;

/** Maps runtime package identities to bounded SOS-owned presentation labels. */
final class SosVisibleIdentity {
    private static final String COMPATIBILITY_APP = "COMPATIBILITY APP";
    private static final String SOS_RUNTIME = "SOS RUNTIME";

    static String source(Context context, String packageName) {
        String raw = clean(packageName);
        if (raw.isEmpty() || "android".equals(raw)) return SOS_RUNTIME;
        if (context.getPackageName().equals(raw)) return "SOS";
        try {
            PackageManager packages = context.getPackageManager();
            ApplicationInfo info = packages.getApplicationInfo(raw, 0);
            String label = clean(packages.getApplicationLabel(info).toString());
            if (!label.isEmpty() && !looksLikePackageName(label)) return label;
        } catch (PackageManager.NameNotFoundException ignored) {
            // Framework facts can outlive the application that produced them.
        }
        return COMPATIBILITY_APP;
    }

    static String content(Context context, String packageName, String value) {
        String cleanValue = clean(value);
        String cleanPackage = clean(packageName);
        if (cleanValue.equals(cleanPackage) || "android".equals(cleanValue)
                || looksLikePackageName(cleanValue)) {
            return source(context, cleanPackage);
        }
        return cleanValue;
    }

    private static boolean looksLikePackageName(String value) {
        if (value.indexOf(' ') >= 0 || value.indexOf('.') <= 0) return false;
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            if (!(Character.isLetterOrDigit(character) || character == '.'
                    || character == '_')) return false;
        }
        return true;
    }

    private static String clean(String value) {
        if (value == null) return "";
        String clean = value.replace('\n', ' ').replace('\r', ' ').trim();
        return clean.length() <= 256 ? clean : clean.substring(0, 256);
    }

    private SosVisibleIdentity() {}
}
