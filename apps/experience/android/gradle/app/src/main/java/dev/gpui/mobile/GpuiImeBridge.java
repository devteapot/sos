package dev.gpui.mobile;

import android.app.Activity;
import android.content.Context;
import android.os.Handler;
import android.os.Looper;
import android.os.Build;
import android.text.Editable;
import android.text.InputType;
import android.text.Selection;
import android.text.SpannableStringBuilder;
import android.text.Spanned;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;

/** A host-owned Android text editor that forwards the complete composing state to GPUI. */
public final class GpuiImeBridge extends View {
    private static final Handler mainHandler = new Handler(Looper.getMainLooper());
    private static final int MAX_UTF16 = 16 * 1024;
    private static GpuiImeBridge instance;
    private static boolean insetObserverInstalled;
    private static int lastImeBottom = -1;

    private final SpannableStringBuilder editable = new SpannableStringBuilder();
    private String nodeId = "";

    private GpuiImeBridge(Activity activity) {
        super(activity);
        setFocusable(true);
        setFocusableInTouchMode(true);
        setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        setAlpha(0.0f);
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
            GpuiImeBridge bridge = ensure(activity);
            bridge.nodeId = nodeId;
            bridge.replaceState(text, selectionStart, selectionEnd, markedStart, markedEnd);
            bridge.requestFocus();
            InputMethodManager manager = (InputMethodManager)
                    activity.getSystemService(Context.INPUT_METHOD_SERVICE);
            manager.restartInput(bridge);
            manager.showSoftInput(bridge, InputMethodManager.SHOW_IMPLICIT);
            bridge.requestApplyInsets();
        });
    }

    public static void deactivate(Activity activity, String nodeId) {
        mainHandler.post(() -> {
            if (instance == null || !instance.nodeId.equals(nodeId)) return;
            InputMethodManager manager = (InputMethodManager)
                    activity.getSystemService(Context.INPUT_METHOD_SERVICE);
            manager.hideSoftInputFromWindow(instance.getWindowToken(), 0);
            GpuiActivity.dispatchImeInset(0.0f);
            instance.clearFocus();
            instance.nodeId = "";
        });
    }

    private static GpuiImeBridge ensure(Activity activity) {
        if (instance != null && instance.getContext() == activity) return instance;
        instance = new GpuiImeBridge(activity);
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(1, 1);
        params.leftMargin = -2;
        params.topMargin = -2;
        activity.addContentView(instance, params);
        if (!insetObserverInstalled) {
            insetObserverInstalled = true;
            View decor = activity.getWindow().getDecorView();
            decor.getViewTreeObserver().addOnGlobalLayoutListener(() -> {
                WindowInsets insets = decor.getRootWindowInsets();
                int bottom = Build.VERSION.SDK_INT >= 30 && insets != null
                        ? insets.getInsets(WindowInsets.Type.ime()).bottom : 0;
                if (bottom != lastImeBottom) {
                    lastImeBottom = bottom;
                    float density = decor.getResources().getDisplayMetrics().density;
                    GpuiActivity.dispatchImeInset(bottom / density);
                }
            });
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
        return new BridgeConnection(this);
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
