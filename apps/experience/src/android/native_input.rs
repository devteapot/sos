use std::{
    cell::RefCell,
    collections::VecDeque,
    ops::Range,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex, OnceLock,
    },
};

use experience_ir::MAX_TEXT_BYTES;
use gpui::{
    actions, div, fill, point, prelude::*, px, relative, rgba, size, App, Bounds, ClipboardItem,
    ContentMask, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, Subscription, TextRun, UTF16Selection, UnderlineStyle, WeakEntity, Window,
};
use unicode_segmentation::UnicodeSegmentation as _;

use super::{accessibility, ExperienceHost};
#[cfg(not(feature = "core-native"))]
use gpui_mobile::android::jni::{activity, find_app_class, get_string, with_env};
#[cfg(not(feature = "core-native"))]
use jni::objects::{JObject, JValue};

thread_local! {
    static ACTIVE_INPUT: RefCell<Option<String>> = const { RefCell::new(None) };
    static PENDING_TEXT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Debug)]
pub struct ImeState {
    pub node_id: String,
    pub text: String,
    pub selection_start: usize,
    pub selection_end: usize,
    pub marked: Option<Range<usize>>,
    pub kind: String,
}

pub struct ImeApplyOutcome {
    pub changed: bool,
    pub state_key: String,
    pub value: String,
    pub submit_action: Option<String>,
}

static IME_STATES: OnceLock<Mutex<VecDeque<ImeState>>> = OnceLock::new();
static IME_INSET_BITS: AtomicU32 = AtomicU32::new(0);
static IME_INSET_CHANGED: AtomicBool = AtomicBool::new(false);

pub fn ime_inset() -> f32 {
    f32::from_bits(IME_INSET_BITS.load(Ordering::Acquire))
}

pub fn take_ime_inset_changed() -> bool {
    IME_INSET_CHANGED.swap(false, Ordering::AcqRel)
}

pub fn take_ime_states() -> Vec<ImeState> {
    IME_STATES
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .expect("IME state lock")
        .drain(..)
        .collect()
}

#[cfg(not(feature = "core-native"))]
#[no_mangle]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnImeState(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    target: *mut std::ffi::c_void,
    text: *mut std::ffi::c_void,
    selection_start: i32,
    selection_end: i32,
    marked_start: i32,
    marked_end: i32,
    kind: *mut std::ffi::c_void,
) {
    let _ = with_env(|env| {
        let target = JObject::from_raw(env, target as jni::sys::jobject);
        let text = JObject::from_raw(env, text as jni::sys::jobject);
        let kind = JObject::from_raw(env, kind as jni::sys::jobject);
        let node_id = get_string(env, &target);
        if node_id.is_empty() {
            return Ok(());
        }
        let marked = (marked_start >= 0 && marked_end >= marked_start)
            .then_some(marked_start as usize..marked_end as usize);
        let state = ImeState {
            node_id,
            text: get_string(env, &text),
            selection_start: selection_start.max(0) as usize,
            selection_end: selection_end.max(0) as usize,
            marked,
            kind: get_string(env, &kind),
        };
        let mut states = IME_STATES
            .get_or_init(|| Mutex::new(VecDeque::new()))
            .lock()
            .expect("IME state lock");
        if states.len() >= 64 {
            states.pop_front();
        }
        states.push_back(state);
        drop(states);
        super::request_host_frame();
        Ok(())
    });
}

#[cfg(not(feature = "core-native"))]
#[no_mangle]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnImeInset(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    logical_bottom: f32,
) {
    let inset = logical_bottom.clamp(0.0, 2_048.0);
    IME_INSET_BITS.store(inset.to_bits(), Ordering::Release);
    IME_INSET_CHANGED.store(true, Ordering::Release);
    log::info!("ime_inset_changed logical_bottom={inset:.1}");
    super::request_host_frame();
}

#[cfg(not(feature = "core-native"))]
fn install_mobile_keyboard_callback(node_id: &str) {
    ACTIVE_INPUT.with(|active| *active.borrow_mut() = Some(node_id.to_owned()));
    gpui_mobile::set_text_input_callback(Some(Box::new(|text| {
        PENDING_TEXT.with(|pending| pending.borrow_mut().push(text.to_owned()));
    })));
    gpui_mobile::TEXT_INPUT_DIRTY.store(true, std::sync::atomic::Ordering::Release);
}

fn clear_mobile_keyboard_callback(node_id: &str) {
    let should_clear = ACTIVE_INPUT.with(|active| {
        if active.borrow().as_deref() == Some(node_id) {
            *active.borrow_mut() = None;
            true
        } else {
            false
        }
    });
    if should_clear {
        gpui_mobile::set_text_input_callback(None);
    }
}

pub fn active_input_id() -> Option<String> {
    ACTIVE_INPUT.with(|active| active.borrow().clone())
}

actions!(
    sos_text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("SosTextInput")),
        KeyBinding::new("delete", Delete, Some("SosTextInput")),
        KeyBinding::new("left", Left, Some("SosTextInput")),
        KeyBinding::new("right", Right, Some("SosTextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("SosTextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("SosTextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("SosTextInput")),
        KeyBinding::new("cmd-v", Paste, Some("SosTextInput")),
        KeyBinding::new("cmd-c", Copy, Some("SosTextInput")),
        KeyBinding::new("cmd-x", Cut, Some("SosTextInput")),
        KeyBinding::new("home", Home, Some("SosTextInput")),
        KeyBinding::new("end", End, Some("SosTextInput")),
    ]);
}

pub struct NativeTextInput {
    node_id: String,
    state_key: String,
    submit_action: Option<String>,
    host: WeakEntity<ExperienceHost>,
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    scroll_offset: Pixels,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    _focus_subscriptions: Vec<Subscription>,
}

#[derive(Clone, Debug)]
pub struct AccessibilityTextState {
    pub value: String,
    pub selection: Range<usize>,
    pub marked: Option<Range<usize>>,
}

impl NativeTextInput {
    // These values are the complete immutable configuration for one keyed GPUI
    // text-input entity; grouping them would only move the same boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_id: String,
        state_key: String,
        content: String,
        placeholder: String,
        submit_action: Option<String>,
        host: WeakEntity<ExperienceHost>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let focused = cx.on_focus(&focus_handle, window, |this, _, cx| {
            this.activate_mobile_ime();
            this.notify_focus(true, cx);
        });
        let blurred = cx.on_blur(&focus_handle, window, |this, _, cx| {
            this.deactivate_mobile_ime();
            clear_mobile_keyboard_callback(&this.node_id);
            gpui_mobile::hide_keyboard();
            this.marked_range = None;
            accessibility::mark_state_changed();
            this.notify_focus(false, cx);
        });
        let cursor = content.len();
        Self {
            node_id,
            state_key,
            submit_action,
            host,
            focus_handle,
            content: content.into(),
            placeholder: placeholder.into(),
            selected_range: cursor..cursor,
            selection_reversed: false,
            marked_range: None,
            scroll_offset: px(0.),
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            _focus_subscriptions: vec![focused, blurred],
        }
    }

    pub fn sync(
        &mut self,
        state_key: &str,
        value: &str,
        placeholder: &str,
        submit_action: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state_key.clear();
        self.state_key.push_str(state_key);
        self.placeholder = placeholder.to_owned().into();
        self.submit_action = submit_action.map(str::to_owned);
        if !self.focus_handle.is_focused(window) && self.content.as_ref() != value {
            self.content = value.to_owned().into();
            let cursor = self.content.len();
            self.selected_range = cursor..cursor;
            self.marked_range = None;
            cx.notify();
        }
    }

    pub fn activate(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            window.focus(&self.focus_handle, cx);
        } else {
            self.activate_mobile_ime();
        }
    }

    pub fn replace_from_accessibility(&mut self, value: String, cx: &mut Context<Self>) {
        self.content = value.into();
        let cursor = self.content.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        accessibility::mark_state_changed();
        self.activate_mobile_ime();
        cx.notify();
    }

    pub fn accessibility_state(&self) -> AccessibilityTextState {
        AccessibilityTextState {
            value: self.content.to_string(),
            selection: self.range_to_utf16(&self.selected_range),
            marked: self
                .marked_range
                .as_ref()
                .map(|range| self.range_to_utf16(range)),
        }
    }

    pub fn set_selection_from_accessibility(
        &mut self,
        start_utf16: usize,
        end_utf16: usize,
        cx: &mut Context<Self>,
    ) {
        let start = self.offset_from_utf16(start_utf16);
        let end = self.offset_from_utf16(end_utf16);
        self.selected_range = start.min(end)..start.max(end);
        self.selection_reversed = start > end;
        self.marked_range = None;
        accessibility::mark_state_changed();
        self.activate_mobile_ime();
        cx.notify();
    }

    pub fn accessibility_clipboard_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            "copy" => self.copy(&Copy, window, cx),
            "cut" => self.cut(&Cut, window, cx),
            "paste" => self.paste(&Paste, window, cx),
            _ => {}
        }
        self.activate_mobile_ime();
    }

    pub fn apply_ime_state(&mut self, state: ImeState, cx: &mut Context<Self>) -> ImeApplyOutcome {
        log::info!(
            "ime_state_applied node_id={} kind={} selection={}:{} marked={}",
            self.node_id,
            state.kind,
            state.selection_start,
            state.selection_end,
            state
                .marked
                .as_ref()
                .map(|range| format!("{}:{}", range.start, range.end))
                .unwrap_or_else(|| "none".into())
        );
        let mut text = state.text.replace(['\r', '\n'], "");
        if text.len() > MAX_TEXT_BYTES {
            let mut end = MAX_TEXT_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        let changed = self.content.as_ref() != text;
        self.content = text.into();
        let start = self.offset_from_utf16(state.selection_start);
        let end = self.offset_from_utf16(state.selection_end);
        self.selected_range = start.min(end)..start.max(end);
        self.selection_reversed = start > end;
        self.marked_range = state.marked.map(|range| self.range_from_utf16(&range));
        accessibility::mark_state_changed();
        let submit_action = (state.kind == "submit")
            .then(|| self.submit_action.clone())
            .flatten();
        cx.notify();
        ImeApplyOutcome {
            changed,
            state_key: self.state_key.clone(),
            value: self.content.to_string(),
            submit_action,
        }
    }

    fn activate_mobile_ime(&self) {
        #[cfg(feature = "core-native")]
        {
            log::warn!(
                "core_native_ime_unavailable node={}; trusted keyboard gate remains open",
                self.node_id
            );
            return;
        }
        #[cfg(not(feature = "core-native"))]
        {
            gpui_mobile::set_text_input_callback(None);
            ACTIVE_INPUT.with(|active| *active.borrow_mut() = Some(self.node_id.clone()));
            let state = self.accessibility_state();
            let result = with_env(|env| {
                let helper = find_app_class(env, "dev.gpui.mobile.GpuiImeBridge")?;
                let activity = activity(env)?;
                let node_id = env
                    .new_string(&self.node_id)
                    .map_err(|error| error.to_string())?;
                let text = env
                    .new_string(&state.value)
                    .map_err(|error| error.to_string())?;
                let (marked_start, marked_end) = state
                    .marked
                    .map(|range| (range.start as i32, range.end as i32))
                    .unwrap_or((-1, -1));
                env.call_static_method(
                    &helper,
                    jni::jni_str!("activate"),
                    jni::jni_sig!(
                        "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;IIII)V"
                    ),
                    &[
                        JValue::Object(&activity),
                        JValue::Object(&node_id),
                        JValue::Object(&text),
                        JValue::Int(state.selection.start as i32),
                        JValue::Int(state.selection.end as i32),
                        JValue::Int(marked_start),
                        JValue::Int(marked_end),
                    ],
                )
                .map_err(|error| {
                    env.exception_clear();
                    error.to_string()
                })?;
                Ok(())
            });
            if let Err(error) = result {
                log::warn!(
                    "composition_ime_unavailable error={error}; using committed-text fallback"
                );
                install_mobile_keyboard_callback(&self.node_id);
                gpui_mobile::show_keyboard();
            }
        }
    }

    fn deactivate_mobile_ime(&self) {
        #[cfg(feature = "core-native")]
        {
            return;
        }
        #[cfg(not(feature = "core-native"))]
        {
            let _ = with_env(|env| {
                let helper = find_app_class(env, "dev.gpui.mobile.GpuiImeBridge")?;
                let activity = activity(env)?;
                let node_id = env
                    .new_string(&self.node_id)
                    .map_err(|error| error.to_string())?;
                env.call_static_method(
                    &helper,
                    jni::jni_str!("deactivate"),
                    jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)V"),
                    &[JValue::Object(&activity), JValue::Object(&node_id)],
                )
                .map_err(|error| {
                    env.exception_clear();
                    error.to_string()
                })?;
                Ok(())
            });
        }
    }

    fn notify_change(&self, cx: &mut Context<Self>) {
        accessibility::mark_state_changed();
        let node_id = self.node_id.clone();
        let state_key = self.state_key.clone();
        let value = self.content.to_string();
        let _ = self.host.update(cx, |host, cx| {
            host.native_input_changed(node_id, state_key, value, cx);
        });
    }

    fn notify_focus(&self, focused: bool, cx: &mut Context<Self>) {
        let node_id = self.node_id.clone();
        let _ = self.host.update(cx, |host, cx| {
            host.native_input_focus_changed(node_id, focused, cx);
        });
    }

    fn submit(&self, cx: &mut Context<Self>) {
        let Some(action) = self.submit_action.clone() else {
            return;
        };
        let node_id = self.node_id.clone();
        let value = self.content.to_string();
        let _ = self.host.update(cx, |host, cx| {
            host.native_input_submitted(node_id, action, value, cx);
        });
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            if previous == self.cursor_offset() {
                return;
            }
            self.select_to(previous, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if next == self.cursor_offset() {
                return;
            }
            self.select_to(next, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace(['\r', '\n'], " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        if !self.selected_range.is_empty() {
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus_handle.is_focused(window) {
            self.activate_mobile_ime();
        } else {
            window.focus(&self.focus_handle, cx);
        }
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        accessibility::mark_state_changed();
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        accessibility::mark_state_changed();
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        Self::editable_index_from_layout(
            &self.content,
            line.closest_index_for_x(position.x - bounds.left() + self.scroll_offset),
        )
    }

    fn editable_index_from_layout(content: &str, layout_index: usize) -> usize {
        // Empty inputs shape their placeholder for display. That layout must not
        // contribute an offset to the editable (empty) content. Clamp generally
        // as a defensive boundary between shaped display text and stored text.
        let mut index = layout_index.min(content.len());
        while !content.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for character in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += character.len_utf16();
            utf8_offset += character.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for character in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += character.len_utf8();
            utf16_offset += character.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn bounded_replacement(&self, range: &Range<usize>, text: &str) -> String {
        let available = MAX_TEXT_BYTES.saturating_sub(self.content.len() - range.len());
        let mut end = text.len().min(available);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_owned()
    }

    fn adjusted_scroll_offset(
        current: Pixels,
        cursor_x: Pixels,
        line_width: Pixels,
        viewport_width: Pixels,
    ) -> Pixels {
        let caret_width = px(2.);
        if line_width <= viewport_width {
            return px(0.);
        }
        let max_offset = (line_width + caret_width - viewport_width).max(px(0.));
        let visible_cursor_x = cursor_x - current;
        if visible_cursor_x < px(0.) {
            cursor_x.min(max_offset)
        } else if visible_cursor_x + caret_width > viewport_width {
            (cursor_x + caret_width - viewport_width).min(max_offset)
        } else {
            current.min(max_offset)
        }
    }

    fn replacement_range(&self, range_utf16: Option<&Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone())
    }

    fn drain_mobile_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = ACTIVE_INPUT.with(|active| active.borrow().as_deref() == Some(&self.node_id));
        if !active {
            return;
        }
        let pending =
            PENDING_TEXT.with(|pending| pending.borrow_mut().drain(..).collect::<Vec<_>>());
        for text in pending {
            match text.as_str() {
                "\x08" => self.backspace(&Backspace, window, cx),
                "\x1b[D" => self.left(&Left, window, cx),
                "\x1b[C" => self.right(&Right, window, cx),
                "\x1b[H" => self.home(&Home, window, cx),
                "\x1b[F" => self.end(&End, window, cx),
                other => self.replace_text_in_range(None, other, window, cx),
            }
        }
    }
}

impl EntityInputHandler for NativeTextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        self.content.get(range).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let submit = new_text.contains(['\r', '\n']);
        let normalized = new_text.replace(['\r', '\n'], "");
        let range = self.replacement_range(range_utf16.as_ref());
        let normalized = self.bounded_replacement(&range, &normalized);
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            normalized,
            &self.content[range.end..]
        )
        .into();
        let cursor = range.start + normalized.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.notify_change(cx);
        if submit {
            self.submit(cx);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        selected_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.replacement_range(range_utf16.as_ref());
        let normalized = new_text.replace(['\r', '\n'], "");
        let normalized = self.bounded_replacement(&range, &normalized);
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            normalized,
            &self.content[range.end..]
        )
        .into();
        self.marked_range =
            (!normalized.is_empty()).then_some(range.start..range.start + normalized.len());
        self.selected_range = selected_utf16
            .map(|selected| {
                let selected = Self::range_from_utf16_in(&normalized, &selected);
                range.start + selected.start..range.start + selected.end
            })
            .unwrap_or_else(|| {
                let cursor = range.start + normalized.len();
                cursor..cursor
            });
        self.selection_reversed = false;
        self.notify_change(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(range.start) - self.scroll_offset,
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(range.end) - self.scroll_offset,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(point.x - bounds.left() + self.scroll_offset)?;
        Some(self.offset_to_utf16(index))
    }
}

impl NativeTextInput {
    fn range_from_utf16_in(text: &str, range: &Range<usize>) -> Range<usize> {
        fn offset(text: &str, utf16: usize) -> usize {
            let mut bytes = 0;
            let mut units = 0;
            for character in text.chars() {
                if units >= utf16 {
                    break;
                }
                units += character.len_utf16();
                bytes += character.len_utf8();
            }
            bytes
        }
        offset(text, range.start)..offset(text, range.end)
    }
}

struct TextElement {
    input: Entity<NativeTextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    scroll_offset: Pixels,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let (display, color) = if content.is_empty() {
            (input.placeholder.clone(), rgba(0x76807899).into())
        } else {
            (content, style.color)
        };
        let run = TextRun {
            len: display.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display, font_size, &runs, None);
        let cursor_x = line.x_for_index(cursor);
        let scroll_offset = if input.focus_handle.is_focused(window) {
            NativeTextInput::adjusted_scroll_offset(
                input.scroll_offset,
                cursor_x,
                line.width(),
                bounds.size.width,
            )
        } else {
            px(0.)
        };
        let text_left = bounds.left() - scroll_offset;
        let (selection, cursor) = if selected.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(text_left + cursor_x, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    gpui::blue(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(text_left + line.x_for_index(selected.start), bounds.top()),
                        point(text_left + line.x_for_index(selected.end), bounds.bottom()),
                    ),
                    rgba(0x4F8CFF44),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            scroll_offset,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let line = state.line.take().expect("text line shaped during prepaint");
        let text_origin = point(bounds.left() - state.scroll_offset, bounds.top());
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = state.selection.take() {
                window.paint_quad(selection);
            }
            line.paint(
                text_origin,
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .expect("native input text paint");
            if focus.is_focused(window) {
                if let Some(cursor) = state.cursor.take() {
                    window.paint_quad(cursor);
                }
            }
        });
        self.input.update(cx, |input, _| {
            input.scroll_offset = state.scroll_offset;
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for NativeTextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drain_mobile_input(window, cx);
        div()
            .flex()
            .w_full()
            .key_context("SosTextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .line_height(px(28.))
            .text_size(px(17.))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for NativeTextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
