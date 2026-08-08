package dev.gpui.mobile;

import android.app.Activity;
import android.graphics.Rect;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.View;
import android.view.ViewGroup;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.webkit.WebChromeClient;
import android.widget.FrameLayout;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * Helper class for managing native platform views embedded in the GPUI render tree.
 *
 * Platform views are native Android Views positioned absolutely over the
 * NativeActivity's content area. GPUI controls their position and visibility
 * via JNI calls from Rust.
 *
 * Supports view types:
 * - "container": Generic empty FrameLayout (for custom content)
 * - Additional types can be registered via registerViewType()
 */
public class GpuiPlatformView {
    private static final String TAG = "GpuiPlatformView";
    private static final Handler mainHandler = new Handler(Looper.getMainLooper());

    /** Map of view ID -> native View */
    private static final Map<Long, View> views = new HashMap<>();

    /** Map of view ID -> FrameLayout container */
    private static final Map<Long, FrameLayout> containers = new HashMap<>();

    /** The root FrameLayout that hosts all platform views */
    private static FrameLayout rootContainer;
    private static AccessibilityBridge accessibilityBridge;

    /** Publish the host-owned SOS semantic tree as Android virtual nodes. */
    public static void updateAccessibilityTree(Activity activity, String payload) {
        mainHandler.post(() -> {
            View decor = activity.getWindow().getDecorView();
            decor.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_YES);
            if (accessibilityBridge == null || accessibilityBridge.host != decor) {
                accessibilityBridge = new AccessibilityBridge(decor);
                AccessibilityBridge bridge = accessibilityBridge;
                decor.setAccessibilityDelegate(new View.AccessibilityDelegate() {
                    @Override
                    public AccessibilityNodeProvider getAccessibilityNodeProvider(View host) {
                        return bridge;
                    }
                });
            }
            accessibilityBridge.update(payload);
        });
    }

    private static final class SemanticNode {
        String id;
        String parent;
        String role;
        String label;
        String value;
        String hint;
        String clickAction;
        boolean editable;
        boolean scrollable;
        int selectionStart = -1;
        int selectionEnd = -1;
        int markedStart = -1;
        int markedEnd = -1;
        float scrollOffsetY;
        float scrollMaxY;
        float x;
        float y;
        float width;
        float height;
        int virtualId;
    }

    /** Android is one adapter over the platform-neutral SOS semantic tree. */
    private static final class AccessibilityBridge extends AccessibilityNodeProvider {
        final View host;
        final Map<String, Integer> ids = new HashMap<>();
        final Map<Integer, SemanticNode> nodes = new LinkedHashMap<>();
        int nextId = 1;
        int accessibilityFocus = View.NO_ID;
        String summary = "";

        AccessibilityBridge(View host) {
            this.host = host;
        }

        void update(String payload) {
            try {
                JSONObject root = new JSONObject(payload);
                summary = root.optString("summary", "");
                JSONArray incoming = root.optJSONArray("nodes");
                Map<Integer, SemanticNode> next = new LinkedHashMap<>();
                if (incoming != null) {
                    for (int index = 0; index < incoming.length(); index++) {
                        JSONObject value = incoming.getJSONObject(index);
                        SemanticNode node = new SemanticNode();
                        node.id = value.getString("id");
                        node.parent = value.isNull("parent") ? null : value.optString("parent", null);
                        node.role = value.optString("role", "status");
                        node.label = value.optString("label", "");
                        node.value = value.isNull("value") ? null : value.optString("value", null);
                        node.hint = value.isNull("hint") ? null : value.optString("hint", null);
                        node.clickAction = value.isNull("click_action")
                                ? null : value.optString("click_action", null);
                        node.editable = value.optBoolean("editable", false);
                        node.scrollable = value.optBoolean("scrollable", false);
                        node.selectionStart = value.optInt("selection_start", -1);
                        node.selectionEnd = value.optInt("selection_end", -1);
                        node.markedStart = value.optInt("marked_start", -1);
                        node.markedEnd = value.optInt("marked_end", -1);
                        node.scrollOffsetY = (float)value.optDouble("scroll_offset_y", 0);
                        node.scrollMaxY = (float)value.optDouble("scroll_max_y", 0);
                        JSONArray bounds = value.optJSONArray("bounds");
                        if (bounds != null && bounds.length() == 4) {
                            node.x = (float)bounds.optDouble(0, 0);
                            node.y = (float)bounds.optDouble(1, 0);
                            node.width = (float)bounds.optDouble(2, 0);
                            node.height = (float)bounds.optDouble(3, 0);
                        }
                        Integer existing = ids.get(node.id);
                        if (existing == null) {
                            existing = nextId++;
                            ids.put(node.id, existing);
                        }
                        node.virtualId = existing;
                        next.put(existing, node);
                    }
                }
                nodes.clear();
                nodes.putAll(next);
                ids.entrySet().removeIf(entry -> !nodes.containsKey(entry.getValue()));
                if (!nodes.containsKey(accessibilityFocus)) accessibilityFocus = View.NO_ID;
                host.setContentDescription(nodes.isEmpty() ? summary : null);
                host.sendAccessibilityEvent(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
                Log.i(TAG, "Accessibility tree updated: nodes=" + nodes.size());
            } catch (Exception error) {
                Log.e(TAG, "Invalid accessibility tree", error);
            }
        }

        @Override
        public AccessibilityNodeInfo createAccessibilityNodeInfo(int virtualViewId) {
            if (virtualViewId == AccessibilityNodeProvider.HOST_VIEW_ID) {
                AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain(host);
                info.setPackageName(host.getContext().getPackageName());
                info.setClassName("android.view.ViewGroup");
                info.setSource(host);
                info.setContentDescription(nodes.isEmpty() ? summary : null);
                info.setEnabled(true);
                info.setVisibleToUser(host.isShown());
                info.setBoundsInParent(new Rect(0, 0, host.getWidth(), host.getHeight()));
                int[] location = new int[2];
                host.getLocationOnScreen(location);
                info.setBoundsInScreen(new Rect(
                        location[0], location[1],
                        location[0] + host.getWidth(), location[1] + host.getHeight()));
                for (SemanticNode node : nodes.values()) {
                    if (node.parent == null) info.addChild(host, node.virtualId);
                }
                return info;
            }
            SemanticNode node = nodes.get(virtualViewId);
            if (node == null) return null;

            AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain();
            info.setPackageName(host.getContext().getPackageName());
            info.setClassName(className(node));
            info.setSource(host, node.virtualId);
            Integer parentId = node.parent == null ? null : ids.get(node.parent);
            if (parentId == null) info.setParent(host);
            else info.setParent(host, parentId);
            for (SemanticNode child : nodes.values()) {
                if (node.id.equals(child.parent)) info.addChild(host, child.virtualId);
            }
            info.setContentDescription(node.label);
            info.setText(node.value == null || node.value.isEmpty() ? node.label : node.value);
            if (node.hint != null) info.setHintText(node.hint);
            info.setEnabled(true);
            float density = host.getResources().getDisplayMetrics().density;
            info.setVisibleToUser(
                    node.width > 0 && node.height > 0 && host.isShown()
                    && node.x + node.width > 0 && node.y + node.height > 0
                    && node.x * density < host.getWidth() && node.y * density < host.getHeight());
            info.setFocusable(true);
            info.setAccessibilityFocused(accessibilityFocus == node.virtualId);
            if ("header".equals(node.role) && android.os.Build.VERSION.SDK_INT >= 28) {
                info.setHeading(true);
            }
            if ("status".equals(node.role)) {
                info.setLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);
            }
            if (node.clickAction != null && !node.clickAction.isEmpty()) {
                info.setClickable(true);
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK);
            }
            if (node.editable) {
                info.setEditable(true);
                if (node.selectionStart >= 0 && node.selectionEnd >= 0) {
                    info.setTextSelection(node.selectionStart, node.selectionEnd);
                }
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_TEXT);
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_SELECTION);
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_COPY);
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CUT);
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_PASTE);
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_FOCUS);
            }
            if (node.scrollable) {
                info.setScrollable(true);
                if (node.scrollOffsetY < node.scrollMaxY) {
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
                }
                if (node.scrollOffsetY > 0) {
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
                }
            }
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS);
            setBounds(info, node, parentId == null ? null : nodes.get(parentId));
            return info;
        }

        @Override
        public List<AccessibilityNodeInfo> findAccessibilityNodeInfosByText(
                String searched, int virtualViewId) {
            List<AccessibilityNodeInfo> result = new ArrayList<>();
            String needle = searched == null ? "" : searched.toLowerCase();
            for (SemanticNode node : nodes.values()) {
                String haystack = (node.label + " " + (node.value == null ? "" : node.value)).toLowerCase();
                if (haystack.contains(needle)) result.add(createAccessibilityNodeInfo(node.virtualId));
            }
            return result;
        }

        @Override
        public boolean performAction(int virtualViewId, int action, Bundle arguments) {
            SemanticNode node = nodes.get(virtualViewId);
            if (node == null) return false;
            if (action == AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS) {
                accessibilityFocus = virtualViewId;
                sendEvent(node, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED);
                GpuiActivity.dispatchAccessibilityAction("focus", node.id, "");
                return true;
            }
            if (action == AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS) {
                if (accessibilityFocus == virtualViewId) accessibilityFocus = View.NO_ID;
                sendEvent(node, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED);
                return true;
            }
            if (action == AccessibilityNodeInfo.ACTION_FOCUS && node.editable) {
                GpuiActivity.dispatchAccessibilityAction("focus", node.id, "");
                return true;
            }
            if (action == AccessibilityNodeInfo.ACTION_CLICK && node.clickAction != null) {
                GpuiActivity.dispatchAccessibilityAction("click", node.id, node.clickAction);
                sendEvent(node, AccessibilityEvent.TYPE_VIEW_CLICKED);
                return true;
            }
            if (action == AccessibilityNodeInfo.ACTION_SET_TEXT && node.editable && arguments != null) {
                CharSequence replacement = arguments.getCharSequence(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE);
                if (replacement == null) return false;
                node.value = replacement.toString();
                GpuiActivity.dispatchAccessibilityAction("set_text", node.id, node.value);
                sendEvent(node, AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED);
                return true;
            }
            if (action == AccessibilityNodeInfo.ACTION_SET_SELECTION
                    && node.editable && arguments != null) {
                int start = arguments.getInt(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, -1);
                int end = arguments.getInt(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, -1);
                if (start < 0 || end < 0) return false;
                node.selectionStart = start;
                node.selectionEnd = end;
                GpuiActivity.dispatchAccessibilityAction(
                        "set_selection", node.id, start + ":" + end);
                sendEvent(node, AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED);
                return true;
            }
            if (node.editable && (action == AccessibilityNodeInfo.ACTION_COPY
                    || action == AccessibilityNodeInfo.ACTION_CUT
                    || action == AccessibilityNodeInfo.ACTION_PASTE)) {
                String command = action == AccessibilityNodeInfo.ACTION_COPY
                        ? "copy" : action == AccessibilityNodeInfo.ACTION_CUT ? "cut" : "paste";
                GpuiActivity.dispatchAccessibilityAction(command, node.id, "");
                return true;
            }
            if (node.scrollable && (action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
                    || action == AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD)) {
                String command = action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
                        ? "scroll_forward" : "scroll_backward";
                GpuiActivity.dispatchAccessibilityAction(command, node.id, "");
                sendEvent(node, AccessibilityEvent.TYPE_VIEW_SCROLLED);
                return true;
            }
            return false;
        }

        private void setBounds(AccessibilityNodeInfo info, SemanticNode node, SemanticNode parent) {
            float density = host.getResources().getDisplayMetrics().density;
            int left = Math.round(node.x * density);
            int top = Math.round(node.y * density);
            int right = Math.round((node.x + node.width) * density);
            int bottom = Math.round((node.y + node.height) * density);
            int parentLeft = parent == null ? 0 : Math.round(parent.x * density);
            int parentTop = parent == null ? 0 : Math.round(parent.y * density);
            info.setBoundsInParent(new Rect(
                    left - parentLeft, top - parentTop, right - parentLeft, bottom - parentTop));
            int[] location = new int[2];
            host.getLocationOnScreen(location);
            info.setBoundsInScreen(new Rect(
                    location[0] + left, location[1] + top,
                    location[0] + right, location[1] + bottom));
        }

        private void sendEvent(SemanticNode node, int type) {
            AccessibilityEvent event = AccessibilityEvent.obtain(type);
            event.setPackageName(host.getContext().getPackageName());
            event.setClassName(className(node));
            event.setSource(host, node.virtualId);
            event.getText().add(node.value == null ? node.label : node.value);
            if (host.getParent() != null) host.getParent().requestSendAccessibilityEvent(host, event);
        }

        private String className(SemanticNode node) {
            switch (node.role) {
                case "button": return "android.widget.Button";
                case "image": return "android.widget.ImageView";
                case "text_field": return "android.widget.EditText";
                case "scroll_area": return "android.widget.ScrollView";
                default: return "android.widget.TextView";
            }
        }
    }

    /**
     * Ensure the root container exists in the activity's view hierarchy.
     * Platform views are added as children of this container.
     */
    private static void ensureRootContainer(Activity activity) {
        if (rootContainer != null) {
            return;
        }

        mainHandler.post(() -> {
            if (rootContainer != null) return;

            rootContainer = new FrameLayout(activity);
            rootContainer.setLayoutParams(new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            ));

            // Add to the activity's content view on top of the NativeActivity surface.
            // NativeActivity uses an internal SurfaceView for native rendering.
            // addContentView adds to the end of the window's DecorView, which
            // renders ON TOP of the NativeActivity's SurfaceView.
            activity.addContentView(rootContainer, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            ));

            // Log the view hierarchy for debugging
            try {
                View decorView = activity.getWindow().getDecorView();
                logViewHierarchy(decorView, 0);
            } catch (Exception e) {
                Log.w(TAG, "Could not log view hierarchy: " + e.getMessage());
            }

            Log.i(TAG, "Root container created and added to activity");
        });
    }

    /**
     * Create a new platform view and add it to the view hierarchy.
     *
     * @param activity       The hosting Activity
     * @param viewType       The type of view to create (e.g., "container", "video_player", "webview")
     * @param viewId         Unique ID for this view instance
     * @param x              Left position in logical pixels
     * @param y              Top position in logical pixels
     * @param width          Width in logical pixels
     * @param height         Height in logical pixels
     * @param creationParams Pipe-delimited key=value pairs (e.g., "player_id=1|url=https://...")
     * @return true if the view was created successfully
     */
    public static boolean createView(
            Activity activity,
            String viewType,
            long viewId,
            float x, float y,
            float width, float height,
            String creationParams) {

        Log.i(TAG, "createView: type=" + viewType + " id=" + viewId
                + " bounds=(" + x + ", " + y + ", " + width + ", " + height + ")"
                + " params=" + creationParams);

        ensureRootContainer(activity);

        // Parse creation params
        Map<String, String> params = parseCreationParams(creationParams);

        mainHandler.post(() -> {
            try {
                float density = activity.getResources().getDisplayMetrics().density;

                // Create a container FrameLayout for this platform view
                FrameLayout container = new FrameLayout(activity);
                FrameLayout.LayoutParams layoutParams = new FrameLayout.LayoutParams(
                    (int)(width * density),
                    (int)(height * density)
                );
                layoutParams.leftMargin = (int)(x * density);
                layoutParams.topMargin = (int)(y * density);
                container.setLayoutParams(layoutParams);

                // Create the actual view based on type
                View view = createViewForType(activity, viewType, params);
                if (view != null) {
                    container.addView(view, new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT
                    ));
                }

                // Store references
                views.put(viewId, view != null ? view : container);
                containers.put(viewId, container);

                // Add to root container
                if (rootContainer != null) {
                    rootContainer.addView(container);
                }

                Log.i(TAG, "View created: id=" + viewId + " type=" + viewType);
            } catch (Exception e) {
                Log.e(TAG, "Failed to create view: " + e.getMessage(), e);
            }
        });

        return true;
    }

    /**
     * Parse a pipe-delimited creation params string into a map.
     * Format: "key1=value1|key2=value2"
     */
    private static Map<String, String> parseCreationParams(String params) {
        Map<String, String> map = new HashMap<>();
        if (params == null || params.isEmpty()) return map;
        for (String pair : params.split("\\|")) {
            int eq = pair.indexOf('=');
            if (eq > 0) {
                map.put(pair.substring(0, eq), pair.substring(eq + 1));
            }
        }
        return map;
    }

    /**
     * Create a View instance based on the view type string and creation params.
     */
    private static View createViewForType(Activity activity, String viewType, Map<String, String> params) {
        switch (viewType) {
            case "container":
                FrameLayout frame = new FrameLayout(activity);
                frame.setBackgroundColor(0x00000000);
                return frame;

            case "video_player":
                return createVideoPlayerView(activity, params);

            case "webview":
                return createWebViewView(activity, params);

            case "camera_preview":
                return createCameraPreviewView(activity, params);

            case "map":
                // Placeholder — MapView requires Google Play Services SDK
                FrameLayout mapContainer = new FrameLayout(activity);
                mapContainer.setBackgroundColor(0xFFE0E0E0);
                return mapContainer;

            default:
                Log.w(TAG, "Unknown view type: " + viewType + ", creating empty container");
                return new FrameLayout(activity);
        }
    }

    /**
     * Create a TextureView for video playback and wire it to the MediaPlayer.
     */
    private static View createVideoPlayerView(Activity activity, Map<String, String> params) {
        int playerId = 0;
        try {
            playerId = Integer.parseInt(params.getOrDefault("player_id", "0"));
        } catch (NumberFormatException e) {
            Log.w(TAG, "Invalid player_id in creation params");
        }

        return GpuiVideoPlayer.createVideoSurface(activity, playerId);
    }

    /**
     * Create a WebView with settings from creation params.
     */
    private static View createWebViewView(Activity activity, Map<String, String> params) {
        boolean jsEnabled = Boolean.parseBoolean(params.getOrDefault("javascript_enabled", "true"));
        boolean domStorage = Boolean.parseBoolean(params.getOrDefault("dom_storage_enabled", "true"));
        boolean zoom = Boolean.parseBoolean(params.getOrDefault("zoom_enabled", "true"));
        String url = params.getOrDefault("url", "");
        String html = params.getOrDefault("html", "");

        WebView wv = new WebView(activity);
        WebSettings settings = wv.getSettings();
        settings.setJavaScriptEnabled(jsEnabled);
        settings.setDomStorageEnabled(domStorage);
        settings.setBuiltInZoomControls(zoom);
        if (zoom) settings.setDisplayZoomControls(false);
        settings.setLoadWithOverviewMode(true);
        settings.setUseWideViewPort(true);

        wv.setWebViewClient(new WebViewClient());
        wv.setWebChromeClient(new WebChromeClient());

        if (!html.isEmpty()) {
            wv.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null);
        } else if (!url.isEmpty()) {
            wv.loadUrl(url);
        }

        return wv;
    }

    /**
     * Create a TextureView for camera preview and wire it to the camera session.
     */
    private static View createCameraPreviewView(Activity activity, Map<String, String> params) {
        int sessionId = 0;
        try {
            sessionId = Integer.parseInt(params.getOrDefault("session_id", "0"));
        } catch (NumberFormatException e) {
            Log.w(TAG, "Invalid session_id in creation params");
        }

        return GpuiCamera.createPreviewSurface(activity, sessionId);
    }

    /**
     * Update a view's position and size.
     */
    public static void setBounds(long viewId, float x, float y, float width, float height) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container == null) {
                Log.w(TAG, "setBounds: no container for id=" + viewId);
                return;
            }

            Activity activity = (Activity) container.getContext();
            float density = activity.getResources().getDisplayMetrics().density;

            FrameLayout.LayoutParams params = (FrameLayout.LayoutParams) container.getLayoutParams();
            params.leftMargin = (int)(x * density);
            params.topMargin = (int)(y * density);
            params.width = (int)(width * density);
            params.height = (int)(height * density);
            container.setLayoutParams(params);
        });
    }

    /**
     * Show or hide a platform view.
     */
    public static void setVisible(long viewId, boolean visible) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container == null) return;
            container.setVisibility(visible ? View.VISIBLE : View.GONE);
        });
    }

    /**
     * Set the z-order of a platform view.
     * Uses View.setZ() (API 21+) for elevation-based ordering.
     */
    public static void setZIndex(long viewId, int zIndex) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container == null) return;
            container.setZ(zIndex);
        });
    }

    /**
     * Remove and dispose a platform view.
     */
    public static void disposeView(long viewId) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container != null) {
                if (rootContainer != null) {
                    rootContainer.removeView(container);
                }
                container.removeAllViews();
                containers.remove(viewId);
            }
            views.remove(viewId);
            Log.i(TAG, "View disposed: id=" + viewId);
        });
    }

    /**
     * Get the native View for a given platform view ID.
     * Useful for packages that need direct access to the Android View.
     */
    public static View getView(long viewId) {
        return views.get(viewId);
    }

    /**
     * Get the container FrameLayout for a given platform view ID.
     */
    public static FrameLayout getContainer(long viewId) {
        return containers.get(viewId);
    }

    /**
     * Pause all platform views. Called when the app goes to background.
     * Views are hidden to release rendering resources.
     */
    public static void pauseAll() {
        mainHandler.post(() -> {
            for (FrameLayout container : containers.values()) {
                container.setVisibility(View.INVISIBLE);
            }
            Log.i(TAG, "All platform views paused");
        });
    }

    /**
     * Resume all platform views. Called when the app returns to foreground.
     * Views are made visible again.
     */
    public static void resumeAll() {
        mainHandler.post(() -> {
            for (FrameLayout container : containers.values()) {
                container.setVisibility(View.VISIBLE);
            }
            Log.i(TAG, "All platform views resumed");
        });
    }

    /**
     * Dispose all platform views. Called during activity cleanup.
     */
    public static void disposeAll() {
        mainHandler.post(() -> {
            for (FrameLayout container : containers.values()) {
                if (rootContainer != null) {
                    rootContainer.removeView(container);
                }
                container.removeAllViews();
            }
            views.clear();
            containers.clear();
            Log.i(TAG, "All platform views disposed");
        });
    }

    /**
     * Check if a touch point hits any visible platform view.
     *
     * Coordinates are in physical pixels relative to the window.
     * This can be called from any thread (reads only from containers map
     * on the calling thread, but note that container positions are set
     * asynchronously on the UI thread, so there is a small race window).
     *
     * For the primary hit-test path, the Rust side uses its own
     * PlatformViewRegistry.hit_test() which is synchronous and lock-free
     * on the native thread. This Java method is provided as a convenience
     * for Java-side callers.
     *
     * @param x Physical pixel x coordinate
     * @param y Physical pixel y coordinate
     * @return true if the point falls within a visible platform view
     */
    public static boolean hitTest(float x, float y) {
        for (Map.Entry<Long, FrameLayout> entry : containers.entrySet()) {
            FrameLayout container = entry.getValue();
            if (container.getVisibility() != View.VISIBLE) continue;

            int[] location = new int[2];
            container.getLocationOnScreen(location);
            int left = location[0];
            int top = location[1];
            int right = left + container.getWidth();
            int bottom = top + container.getHeight();

            if (x >= left && x <= right && y >= top && y <= bottom) {
                return true;
            }
        }
        return false;
    }

    /**
     * Dispatch a touch event to the platform view hierarchy.
     *
     * Used to forward NativeActivity input events to platform views
     * when the Rust-side hit-test determines the touch lands on a
     * platform view. The event is posted to the UI thread for dispatch.
     *
     * @param x      Physical pixel x coordinate
     * @param y      Physical pixel y coordinate
     * @param action MotionEvent action constant (0=DOWN, 1=UP, 2=MOVE)
     */
    public static void dispatchTouch(float x, float y, int action) {
        mainHandler.post(() -> {
            if (rootContainer == null) return;

            long now = android.os.SystemClock.uptimeMillis();
            android.view.MotionEvent event = android.view.MotionEvent.obtain(
                now, now, action, x, y, 0
            );
            rootContainer.dispatchTouchEvent(event);
            event.recycle();
        });
    }

    /**
     * Log the view hierarchy from a root view for debugging.
     */
    private static void logViewHierarchy(View view, int depth) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < depth; i++) sb.append("  ");
        sb.append(view.getClass().getSimpleName());
        sb.append(" [").append(view.getWidth()).append("x").append(view.getHeight()).append("]");
        sb.append(" vis=").append(view.getVisibility() == View.VISIBLE ? "V" : view.getVisibility() == View.GONE ? "G" : "I");
        if (view instanceof android.view.SurfaceView) {
            sb.append(" (SurfaceView)");
        }
        Log.i(TAG, sb.toString());
        if (view instanceof ViewGroup) {
            ViewGroup vg = (ViewGroup) view;
            for (int i = 0; i < vg.getChildCount(); i++) {
                logViewHierarchy(vg.getChildAt(i), depth + 1);
            }
        }
    }
}
