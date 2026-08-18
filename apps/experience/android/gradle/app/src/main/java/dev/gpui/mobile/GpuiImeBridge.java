package dev.gpui.mobile;

import android.app.Activity;
import android.content.Context;
import android.graphics.Color;
import android.graphics.Rect;
import android.os.Handler;
import android.os.Looper;
import android.os.Build;
import android.text.Editable;
import android.text.InputType;
import android.text.Selection;
import android.text.SpannableStringBuilder;
import android.text.Spanned;
import android.util.Log;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewTreeObserver;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;

/** A host-owned Android text editor that forwards the complete composing state to GPUI. */
public final class GpuiImeBridge extends View {
    private static final String TAG = "GpuiImeBridge";
    private static final Handler mainHandler = new Handler(Looper.getMainLooper());
    private static final int MAX_UTF16 = 16 * 1024;
    private static GpuiImeBridge instance;
    private static Activity insetObserverActivity;
    private static View insetObserverDecor;
    private static ViewTreeObserver.OnGlobalLayoutListener insetObserver;
    private static int lastImeBottom = -1;

    private final SpannableStringBuilder editable = new SpannableStringBuilder();
    private String nodeId = "";
    private int activationGeneration;
    private int inputConnectionGeneration = -1;
    private Runnable pendingShow;

    private GpuiImeBridge(Activity activity) {
        super(activity);
        setFocusable(true);
        setFocusableInTouchMode(true);
        setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        setVisibility(View.VISIBLE);
        setBackgroundColor(Color.TRANSPARENT);
        // Keep framework/OEM visibility predicates true without drawing a pixel.
        setAlpha(1.0f);
        setClickable(false);
        setLongClickable(false);
        setContextClickable(false);
        setHapticFeedbackEnabled(false);
        setSoundEffectsEnabled(false);
        setOnApplyWindowInsetsListener((view, insets) -> {
            int bottom = Build.VERSION.SDK_INT >= 30
                    ? insets.getInsets(WindowInsets.Type.ime()).bottom
                    : Math.max(0, insets.getSystemWindowInsetBottom()
                            - insets.getStableInsetBottom());
            float density = getResources().getDisplayMetrics().density;
            GpuiActivity.dispatchImeInset(bottom / density);
            return insets;
        });
    }

    public static void activate(
            Activity activity, String nodeId, String text,
            int selectionStart, int selectionEnd, int markedStart, int markedEnd) {
        mainHandler.post(() -> {
            if (activity.isDestroyed() || activity.isFinishing()) return;
            GpuiImeBridge bridge = ensure(activity);
            int generation = ++bridge.activationGeneration;
            bridge.inputConnectionGeneration = -1;
            bridge.nodeId = nodeId;
            bridge.replaceState(text, selectionStart, selectionEnd, markedStart, markedEnd);
            boolean focused = bridge.requestFocus();
            InputMethodManager manager = (InputMethodManager)
                    activity.getSystemService(Context.INPUT_METHOD_SERVICE);
            manager.restartInput(bridge);
            // Attachment, layout, window focus, and input-connection handoff may complete after
            // activation. Their callbacks converge on this generation-bounded request.
            bridge.scheduleShowIme(activity, nodeId, generation, "activate");
            WindowManager.LayoutParams window = activity.getWindow().getAttributes();
            Log.i(TAG, "ime_activate focus_requested=" + focused
                    + " window_flags=0x" + Integer.toHexString(window.flags)
                    + " soft_input_mode=0x" + Integer.toHexString(window.softInputMode)
                    + " " + bridge.stateSummary(manager));
        });
    }

    public static void deactivate(Activity activity, String nodeId) {
        mainHandler.post(() -> {
            if (instance == null
                    || instance.getContext() != activity
                    || !instance.nodeId.equals(nodeId)) return;
            ++instance.activationGeneration;
            instance.inputConnectionGeneration = -1;
            instance.nodeId = "";
            instance.cancelPendingShow();
            InputMethodManager manager = (InputMethodManager)
                    activity.getSystemService(Context.INPUT_METHOD_SERVICE);
            manager.hideSoftInputFromWindow(instance.getWindowToken(), 0);
            GpuiActivity.dispatchImeInset(0.0f);
            instance.clearFocus();
        });
    }

    static void activityDestroyed(Activity activity) {
        if (insetObserverActivity == activity) removeInsetObserver();
        if (instance == null || instance.getContext() != activity) return;
        ++instance.activationGeneration;
        instance.inputConnectionGeneration = -1;
        instance.nodeId = "";
        instance.cancelPendingShow();
        InputMethodManager manager = (InputMethodManager)
                activity.getSystemService(Context.INPUT_METHOD_SERVICE);
        manager.hideSoftInputFromWindow(instance.getWindowToken(), 0);
        instance.clearFocus();
        instance = null;
        lastImeBottom = -1;
    }

    private static void removeInsetObserver() {
        if (insetObserverDecor != null && insetObserver != null) {
            ViewTreeObserver observer = insetObserverDecor.getViewTreeObserver();
            if (observer.isAlive()) observer.removeOnGlobalLayoutListener(insetObserver);
        }
        insetObserverActivity = null;
        insetObserverDecor = null;
        insetObserver = null;
    }

    private void scheduleShowIme(
            Activity activity, String requestedNodeId, int generation, String reason) {
        cancelPendingShow();
        pendingShow = () -> {
            pendingShow = null;
            showIme(activity, requestedNodeId, generation, reason);
        };
        post(pendingShow);
    }

    private void cancelPendingShow() {
        if (pendingShow == null) return;
        removeCallbacks(pendingShow);
        pendingShow = null;
    }

    private boolean isCurrentActivation(
            Activity activity, String requestedNodeId, int generation) {
        return instance == this
                && getContext() == activity
                && !activity.isDestroyed()
                && !activity.isFinishing()
                && activationGeneration == generation
                && !nodeId.isEmpty()
                && nodeId.equals(requestedNodeId)
                && isFocused();
    }

    private void showIme(
            Activity activity, String requestedNodeId, int generation, String reason) {
        if (!isCurrentActivation(activity, requestedNodeId, generation)) return;
        InputMethodManager manager = (InputMethodManager)
                activity.getSystemService(Context.INPUT_METHOD_SERVICE);
        if (!hasViableEditorAnchor(manager, generation)) {
            Log.i(TAG, "ime_show_deferred reason=" + reason + " " + stateSummary(manager));
            return;
        }

        // This follows an explicit user interaction. Zero flags avoids the weaker implicit mode
        // and the lifecycle-sticky SHOW_FORCED mode.
        boolean accepted = manager.showSoftInput(this, 0);
        boolean controllerRequested = false;
        if (Build.VERSION.SDK_INT >= 30) {
            // Target the Activity window whose IME insets the native surface consumes.
            WindowInsetsController controller = activity.getWindow().getInsetsController();
            if (controller != null) {
                controller.show(WindowInsets.Type.ime());
                controllerRequested = true;
            }
        }
        requestApplyInsets();
        activity.getWindow().getDecorView().requestApplyInsets();
        Log.i(TAG, "ime_show_requested reason=" + reason
                + " legacy_accepted=" + accepted
                + " controller_requested=" + controllerRequested
                + " " + stateSummary(manager));
    }

    private boolean hasViableEditorAnchor(InputMethodManager manager, int generation) {
        Rect visible = new Rect();
        return isAttachedToWindow()
                && getWindowToken() != null
                && isShown()
                && isLaidOut()
                && getWidth() > 0
                && getHeight() > 0
                && getGlobalVisibleRect(visible)
                && !visible.isEmpty()
                && isFocused()
                && hasWindowFocus()
                && inputConnectionGeneration == generation
                && manager.isActive(this);
    }

    private String stateSummary(InputMethodManager manager) {
        Rect visible = new Rect();
        boolean globallyVisible = getGlobalVisibleRect(visible) && !visible.isEmpty();
        return "anchor_contract=in_window_nonzero_alpha_noninteractive"
                + " attached=" + isAttachedToWindow()
                + " token=" + (getWindowToken() != null)
                + " shown=" + isShown()
                + " alpha=" + getAlpha()
                + " laid_out=" + isLaidOut()
                + " view_focused=" + isFocused()
                + " window_focused=" + hasWindowFocus()
                + " size=" + getWidth() + "x" + getHeight()
                + " visible_bounds=" + visible.toShortString()
                + " globally_visible=" + globallyVisible
                + " imm_active=" + manager.isActive(this);
    }

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        if (nodeId.isEmpty()) return;
        requestFocus();
        int generation = activationGeneration;
        String requestedNodeId = nodeId;
        InputMethodManager manager = (InputMethodManager)
                getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
        manager.restartInput(this);
        scheduleShowIme((Activity) getContext(), requestedNodeId, generation, "attached");
    }

    @Override
    public void onWindowFocusChanged(boolean hasWindowFocus) {
        super.onWindowFocusChanged(hasWindowFocus);
        if (!hasWindowFocus || nodeId.isEmpty() || !isFocused()) return;
        int generation = activationGeneration;
        String requestedNodeId = nodeId;
        if (inputConnectionGeneration != generation) {
            InputMethodManager manager = (InputMethodManager)
                    getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
            manager.restartInput(this);
        }
        scheduleShowIme((Activity) getContext(), requestedNodeId, generation, "window_focus");
    }

    @Override
    protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
        super.onLayout(changed, left, top, right, bottom);
        if (!changed || nodeId.isEmpty() || !isFocused()) return;
        scheduleShowIme(
                (Activity) getContext(), nodeId, activationGeneration, "in_window_layout");
    }

    @Override
    protected void onDetachedFromWindow() {
        cancelPendingShow();
        super.onDetachedFromWindow();
    }

    @Override
    public boolean dispatchTouchEvent(MotionEvent event) {
        // The transparent anchor must never consume native-surface pointer input.
        return false;
    }

    private static GpuiImeBridge ensure(Activity activity) {
        if (instance != null && instance.getContext() == activity) return instance;
        if (instance != null) {
            ++instance.activationGeneration;
            instance.inputConnectionGeneration = -1;
            instance.nodeId = "";
            instance.cancelPendingShow();
            instance.clearFocus();
        }
        if (insetObserverActivity != activity) removeInsetObserver();
        instance = new GpuiImeBridge(activity);
        // A transparent one-pixel child at the visible origin is a valid framework editor anchor
        // without obstructing or handling native-surface input.
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(
                1, 1, Gravity.TOP | Gravity.START);
        params.leftMargin = 0;
        params.topMargin = 0;
        activity.addContentView(instance, params);
        if (insetObserver == null) {
            insetObserverActivity = activity;
            View decor = activity.getWindow().getDecorView();
            insetObserverDecor = decor;
            insetObserver = () -> {
                WindowInsets insets = decor.getRootWindowInsets();
                int bottom = Build.VERSION.SDK_INT >= 30 && insets != null
                        ? insets.getInsets(WindowInsets.Type.ime()).bottom : 0;
                if (bottom != lastImeBottom) {
                    lastImeBottom = bottom;
                    float density = decor.getResources().getDisplayMetrics().density;
                    GpuiActivity.dispatchImeInset(bottom / density);
                }
            };
            decor.getViewTreeObserver().addOnGlobalLayoutListener(insetObserver);
            decor.requestApplyInsets();
        }
        return instance;
    }

    private void replaceState(
            String text, int selectionStart, int selectionEnd, int markedStart, int markedEnd) {
        editable.replace(0, editable.length(), bounded(text));
        int length = editable.length();
        Selection.setSelection(
                editable,
                clamp(selectionStart, 0, length),
                clamp(selectionEnd, 0, length));
        BaseInputConnection.removeComposingSpans(editable);
        if (markedStart >= 0 && markedEnd >= markedStart) {
            editable.setSpan(
                    new Object(), clamp(markedStart, 0, length), clamp(markedEnd, 0, length),
                    Spanned.SPAN_EXCLUSIVE_EXCLUSIVE | Spanned.SPAN_COMPOSING);
        }
    }

    private static String bounded(String text) {
        if (text == null) return "";
        return text.length() <= MAX_UTF16 ? text : text.substring(0, MAX_UTF16);
    }

    private static int clamp(int value, int min, int max) {
        return Math.max(min, Math.min(value, max));
    }

    @Override
    public boolean onCheckIsTextEditor() {
        return true;
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES;
        outAttrs.imeOptions = EditorInfo.IME_ACTION_DONE | EditorInfo.IME_FLAG_NO_EXTRACT_UI;
        outAttrs.initialSelStart = Selection.getSelectionStart(editable);
        outAttrs.initialSelEnd = Selection.getSelectionEnd(editable);
        BridgeConnection connection = new BridgeConnection(this);
        boolean active = instance == this && !nodeId.isEmpty() && isFocused();
        if (active) {
            inputConnectionGeneration = activationGeneration;
            scheduleShowIme(
                    (Activity) getContext(), nodeId, activationGeneration, "input_connection");
        }
        Log.i(TAG, "ime_input_connection_created active=" + active
                + " selection_length="
                + Math.abs(outAttrs.initialSelEnd - outAttrs.initialSelStart)
                + " " + stateSummary((InputMethodManager)
                        getContext().getSystemService(Context.INPUT_METHOD_SERVICE)));
        return connection;
    }

    private static final class BridgeConnection extends BaseInputConnection {
        private final GpuiImeBridge bridge;
        private int batchDepth;
        private boolean dirty;
        private String pendingKind = "state";

        BridgeConnection(GpuiImeBridge bridge) {
            super(bridge, true);
            this.bridge = bridge;
        }

        @Override
        public Editable getEditable() {
            return bridge.editable;
        }

        @Override
        public boolean beginBatchEdit() {
            batchDepth++;
            return super.beginBatchEdit();
        }

        @Override
        public boolean endBatchEdit() {
            boolean result = super.endBatchEdit();
            batchDepth = Math.max(0, batchDepth - 1);
            if (batchDepth == 0 && dirty) dispatch(pendingKind);
            return result;
        }

        @Override
        public boolean commitText(CharSequence text, int newCursorPosition) {
            boolean result = super.commitText(bounded(text == null ? "" : text.toString()), newCursorPosition);
            changed("commit");
            return result;
        }

        @Override
        public boolean setComposingText(CharSequence text, int newCursorPosition) {
            boolean result = super.setComposingText(
                    bounded(text == null ? "" : text.toString()), newCursorPosition);
            changed("compose");
            return result;
        }

        @Override
        public boolean setComposingRegion(int start, int end) {
            boolean result = super.setComposingRegion(start, end);
            changed("compose");
            return result;
        }

        @Override
        public boolean finishComposingText() {
            boolean result = super.finishComposingText();
            changed("finish_composing");
            return result;
        }

        @Override
        public boolean deleteSurroundingText(int beforeLength, int afterLength) {
            boolean result = super.deleteSurroundingText(beforeLength, afterLength);
            changed("delete");
            return result;
        }

        @Override
        public boolean deleteSurroundingTextInCodePoints(int beforeLength, int afterLength) {
            boolean result = super.deleteSurroundingTextInCodePoints(beforeLength, afterLength);
            changed("delete");
            return result;
        }

        @Override
        public boolean setSelection(int start, int end) {
            boolean result = super.setSelection(start, end);
            changed("selection");
            return result;
        }

        @Override
        public boolean sendKeyEvent(KeyEvent event) {
            if (event.getAction() != KeyEvent.ACTION_DOWN) return true;
            if (event.getKeyCode() == KeyEvent.KEYCODE_ENTER) {
                dispatch("submit");
                return true;
            }
            if (event.getKeyCode() == KeyEvent.KEYCODE_DEL) {
                deleteSurroundingText(1, 0);
                return true;
            }
            if (event.getKeyCode() == KeyEvent.KEYCODE_FORWARD_DEL) {
                deleteSurroundingText(0, 1);
                return true;
            }
            int unicode = event.getUnicodeChar(event.getMetaState());
            if (unicode != 0 && !Character.isISOControl(unicode)) {
                commitText(new String(Character.toChars(unicode)), 1);
                return true;
            }
            String characters = event.getCharacters();
            if (characters != null && !characters.isEmpty()) {
                commitText(characters, 1);
                return true;
            }
            return super.sendKeyEvent(event);
        }

        @Override
        public boolean performEditorAction(int actionCode) {
            changed("submit");
            return true;
        }

        private void changed(String kind) {
            pendingKind = kind;
            dirty = true;
            if (batchDepth == 0) dispatch(kind);
        }

        private void dispatch(String kind) {
            dirty = false;
            Editable value = bridge.editable;
            int selectionStart = Math.max(0, Selection.getSelectionStart(value));
            int selectionEnd = Math.max(0, Selection.getSelectionEnd(value));
            int markedStart = BaseInputConnection.getComposingSpanStart(value);
            int markedEnd = BaseInputConnection.getComposingSpanEnd(value);
            GpuiActivity.dispatchImeState(
                    bridge.nodeId, value.toString(), selectionStart, selectionEnd,
                    markedStart, markedEnd, kind);
        }
    }
}
