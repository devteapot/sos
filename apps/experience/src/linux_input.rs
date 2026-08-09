use std::ops::Range;

use experience_ir::MAX_TEXT_BYTES;
use gpui::{
    actions, div, fill, point, prelude::*, px, relative, rgba, size, App, Bounds, ClipboardItem,
    Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style,
    Subscription, TextRun, UTF16Selection, UnderlineStyle, WeakEntity, Window,
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::linux::LinuxExperienceHost;

actions!(
    sos_linux_text_input,
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
        Submit,
        Paste,
        Cut,
        Copy,
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("SosLinuxTextInput")),
        KeyBinding::new("delete", Delete, Some("SosLinuxTextInput")),
        KeyBinding::new("left", Left, Some("SosLinuxTextInput")),
        KeyBinding::new("right", Right, Some("SosLinuxTextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("SosLinuxTextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("SosLinuxTextInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("SosLinuxTextInput")),
        KeyBinding::new("ctrl-v", Paste, Some("SosLinuxTextInput")),
        KeyBinding::new("ctrl-c", Copy, Some("SosLinuxTextInput")),
        KeyBinding::new("ctrl-x", Cut, Some("SosLinuxTextInput")),
        KeyBinding::new("home", Home, Some("SosLinuxTextInput")),
        KeyBinding::new("end", End, Some("SosLinuxTextInput")),
        KeyBinding::new("enter", Submit, Some("SosLinuxTextInput")),
    ]);
}

pub struct NativeTextInput {
    node_id: String,
    state_key: String,
    submit_action: Option<String>,
    host: WeakEntity<LinuxExperienceHost>,
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    _focus_subscriptions: Vec<Subscription>,
}

impl NativeTextInput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_id: String,
        state_key: String,
        content: String,
        placeholder: String,
        submit_action: Option<String>,
        host: WeakEntity<LinuxExperienceHost>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let focused = cx.on_focus(&focus_handle, window, |this, window, cx| {
            this.notify_focus(true, cx);
            window.invalidate_character_coordinates();
        });
        let blurred = cx.on_blur(&focus_handle, window, |this, _, cx| {
            this.marked_range = None;
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
        }
    }

    pub fn accessibility_set_value(
        &mut self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = 0..self.content.encode_utf16().count();
        self.replace_text_in_range(Some(range), value, window, cx);
    }

    pub fn accessibility_set_selection(
        &mut self,
        start: usize,
        end: usize,
        cx: &mut Context<Self>,
    ) {
        let start = self.offset_from_utf16(start);
        let end = self.offset_from_utf16(end);
        self.selected_range = start.min(end)..start.max(end);
        self.selection_reversed = start > end;
        self.marked_range = None;
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
    }

    fn notify_change(&self, cx: &mut Context<Self>) {
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
        let offset = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor_offset())
        } else {
            self.selected_range.start
        };
        self.move_to(offset, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            self.next_boundary(self.cursor_offset())
        } else {
            self.selected_range.end
        };
        self.move_to(offset, cx);
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

    fn submit_key(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        self.submit(cx);
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
            eprintln!("sos_linux_clipboard action=paste bytes={}", text.len());
            self.replace_text_in_range(None, &text.replace(['\r', '\n'], " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            eprintln!(
                "sos_linux_clipboard action=copy bytes={}",
                self.selected_range.len()
            );
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        if !self.selected_range.is_empty() {
            eprintln!(
                "sos_linux_clipboard action=cut bytes={}",
                self.selected_range.len()
            );
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            window.focus(&self.focus_handle, cx);
        }
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
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
        self.content[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(self.content.len(), |(index, _)| offset + index)
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds, self.last_layout.as_ref()) else {
            return 0;
        };
        if position.y < bounds.top() {
            0
        } else if position.y > bounds.bottom() {
            self.content.len()
        } else {
            line.closest_index_for_x(position.x - bounds.left())
        }
    }

    fn offset_from_utf16_in(text: &str, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for character in text.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += character.len_utf16();
            utf8 += character.len_utf8();
        }
        utf8
    }

    fn range_from_utf16_in(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_from_utf16_in(text, range.start)..Self::offset_from_utf16_in(text, range.end)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        Self::offset_from_utf16_in(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].encode_utf16().count()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn replacement_range(&self, range: Option<&Range<usize>>) -> Range<usize> {
        range
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone())
    }

    fn bounded_replacement(&self, range: &Range<usize>, text: &str) -> String {
        let available = MAX_TEXT_BYTES.saturating_sub(self.content.len() - range.len());
        let mut end = text.len().min(available);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_owned()
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
            .map(|selected| Self::range_from_utf16_in(&normalized, &selected))
            .map(|selected| range.start + selected.start..range.start + selected.end)
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
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        line.index_for_x(position.x - bounds.left())
            .map(|index| self.offset_to_utf16(index))
    }
}

struct TextElement {
    input: Entity<NativeTextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
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
        let (selection, cursor) = if selected.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    gpui::blue(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x4F8CFF44),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
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
        if let Some(selection) = state.selection.take() {
            window.paint_quad(selection);
        }
        let line = state.line.take().expect("text line shaped during prepaint");
        line.paint(
            bounds.origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .expect("Linux native input text paint");
        if focus.is_focused(window) {
            if let Some(cursor) = state.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for NativeTextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w_full()
            .key_context("SosLinuxTextInput")
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
            .on_action(cx.listener(Self::submit_key))
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

#[cfg(test)]
mod tests {
    use super::NativeTextInput;

    #[test]
    fn utf16_offsets_cover_multilingual_text() {
        let text = "A😀☕️明";
        assert_eq!(NativeTextInput::offset_from_utf16_in(text, 0), 0);
        assert_eq!(NativeTextInput::offset_from_utf16_in(text, 1), 1);
        assert_eq!(NativeTextInput::offset_from_utf16_in(text, 2), 5);
        assert_eq!(NativeTextInput::offset_from_utf16_in(text, 3), 5);
        assert_eq!(NativeTextInput::offset_from_utf16_in(text, 4), 8);
        assert_eq!(NativeTextInput::offset_from_utf16_in(text, 5), 11);
        assert_eq!(NativeTextInput::offset_from_utf16_in(text, 6), 14);
        assert_eq!(NativeTextInput::range_from_utf16_in(text, &(1..3)), 1..5);
    }
}
