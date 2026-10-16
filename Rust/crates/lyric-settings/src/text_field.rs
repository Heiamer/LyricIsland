//! A compact single-line text field.
//!
//! gpui ships the input plumbing but no ready-made field, so this is the small one the settings
//! window needs: focus on click, type, move the caret, select, copy/cut/paste, and commit on Enter.
//!
//! Text does **not** arrive through key events. The field implements [`EntityInputHandler`] and
//! registers an [`ElementInputHandler`] while it paints, which is what makes an input method work:
//! Windows hands `WM_IME_COMPOSITION` to the registered handler, which calls
//! [`EntityInputHandler::replace_and_mark_text_in_range`] with the composing string (painted with
//! an underline) and [`EntityInputHandler::replace_text_in_range`] with the committed one.
//! Ordinary characters (including every non-ASCII character a keyboard layout can produce) take
//! the same route through `WM_CHAR`, so the field never guesses a character from a keystroke and
//! nothing is inserted twice.
//!
//! Deliberately out of scope: double-click word selection, triple-click line selection, and
//! undo/redo. The field holds one line of a few dozen characters; click-to-place, drag-to-select,
//! shift-arrow selection and clipboard editing cover what the settings window needs.
use std::{cell::RefCell, ops::Range, rc::Rc};

use gpui::{prelude::*, *};

use crate::theme as t;
use crate::value::{next_boundary, prev_boundary};

/// Called with the trimmed contents when the user presses Enter.
pub type OnCommit = Rc<dyn Fn(&str, &mut App)>;

/// What the paint pass leaves behind for the platform callbacks.
///
/// `bounds_for_range` (where to put the IME candidate window) and `character_index_for_point`
/// (pointer position to text index) are called from platform events, i.e. outside `Render`, so the
/// shaped line and the rectangle it was painted in live behind a shared cell rather than on the
/// entity — touching the entity from its own paint would re-enter the borrow that `render` holds.
#[derive(Default)]
struct Painted {
    line: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
}

pub struct TextField {
    focus: FocusHandle,
    id: SharedString,
    text: String,
    /// Byte range of the selection in `text`; empty when it is just an insertion point.
    selected: Range<usize>,
    /// Whether the selection was dragged backwards, so the caret sits at `selected.start`.
    reversed: bool,
    /// Byte range the IME is composing into; `None` when no composition is in progress.
    marked: Option<Range<usize>>,
    placeholder: SharedString,
    on_commit: OnCommit,
    /// Self-check hook: publish this field's rectangle once so an automated test can click it.
    watched: bool,
    /// Written during paint, read from the platform input callbacks.
    painted: Rc<RefCell<Painted>>,
    /// True while a pointer drag is extending the selection.
    dragging: bool,
}

/// Self-check hook: name one field so its painted rectangle is published for an automated click.
fn watched(id: &str) -> bool {
    std::env::var("LYRICISLAND_SMOKE_FIELD").is_ok_and(|want| want == id)
}

fn log(message: &str) {
    lyric_backend::io::log(&lyric_backend::io::data_directory().join("Logs"), "FIELD", message);
}

/// One line has no room for line breaks, and control characters would shape as nothing.
fn sanitize(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// The UTF-8 byte offset of a UTF-16 offset inside `text` (never past the end or into a character).
fn byte_offset(text: &str, utf16_offset: usize) -> usize {
    let mut utf16 = 0;
    let mut bytes = 0;
    for character in text.chars() {
        if utf16 >= utf16_offset {
            break;
        }
        utf16 += character.len_utf16();
        bytes += character.len_utf8();
    }
    bytes
}

/// Runs for the shaped line: the composing span is underlined so the IME's uncommitted text is
/// visibly distinct from what the field already holds.
fn runs_for(run: &TextRun, len: usize, marked: Option<&Range<usize>>) -> Vec<TextRun> {
    let Some(marked) = marked else {
        return vec![run.clone()];
    };
    let start = marked.start.min(len);
    let end = marked.end.clamp(start, len);
    let mut runs = Vec::with_capacity(3);
    if start > 0 {
        runs.push(TextRun { len: start, ..run.clone() });
    }
    if end > start {
        runs.push(TextRun {
            len: end - start,
            underline: Some(UnderlineStyle { color: Some(run.color), thickness: px(1.0), wavy: false }),
            ..run.clone()
        });
    }
    if end < len {
        runs.push(TextRun { len: len - end, ..run.clone() });
    }
    if runs.is_empty() {
        runs.push(TextRun { len, ..run.clone() });
    }
    runs
}

impl TextField {
    pub fn new(
        id: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        initial: impl Into<String>,
        on_commit: OnCommit,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial: String = initial.into();
        let text = sanitize(&initial);
        let selected = text.len()..text.len();
        let id: SharedString = id.into();
        let watched = watched(id.as_ref());
        Self {
            focus: cx.focus_handle(),
            id,
            text,
            selected,
            reversed: false,
            marked: None,
            placeholder: placeholder.into(),
            on_commit,
            watched,
            painted: Rc::new(RefCell::new(Painted::default())),
            dragging: false,
        }
    }

    pub fn set_text(&mut self,text:&str,cx:&mut Context<Self>){self.text=text.to_owned();self.selected=self.text.len()..self.text.len();self.marked=None;cx.notify();}

    /// Current contents. A form reads its values back through this rather than keeping a second
    /// copy in sync, so what is submitted is always exactly what the field shows.
    pub fn text(&self)->&str{&self.text}

    /// Where the caret is: the moving end of the selection.
    fn caret(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.text.len());
        self.selected = offset..offset;
        self.reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.text.len());
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected = 0..self.text.len();
        self.reversed = false;
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.text[self.selected.clone()].to_string()));
        }
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        byte_offset(&self.text, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for character in self.text.chars() {
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

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    /// The range a replacement applies to: an explicit platform range, else the composition in
    /// progress, else the current selection. Always a valid, ascending span of `text`.
    fn replacement_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        let candidate = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected.clone());
        let len = self.text.len();
        let start = candidate.start.min(len);
        let end = candidate.end.max(start).min(len);
        start..end
    }

    /// Replace one byte range, keeping the text on character boundaries.
    fn splice(&mut self, range: &Range<usize>, text: &str) {
        let mut next = String::with_capacity(self.text.len() + text.len());
        next.push_str(&self.text[..range.start]);
        next.push_str(text);
        next.push_str(&self.text[range.end..]);
        self.text = next;
    }

    /// Map a window position onto a byte index in `text`, using the line painted last frame.
    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        if self.text.is_empty() {
            return 0;
        }
        let painted = self.painted.borrow();
        let (Some(bounds), Some(line)) = (painted.bounds.as_ref(), painted.line.as_ref()) else {
            return self.caret();
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.text.len();
        }
        line.closest_index_for_x(position.x - bounds.left()).min(self.text.len())
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Focus on the press, not on the release: the IME must already know where the caret is.
        window.focus(&self.focus);
        self.dragging = true;
        let offset = self.index_for_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.dragging = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.dragging {
            let offset = self.index_for_position(event.position);
            self.select_to(offset, cx);
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let command = keystroke.modifiers.control || keystroke.modifiers.platform;
        let shift = keystroke.modifiers.shift;
        let caret = self.caret();

        // While an IME is composing, the keys belong to it: Windows reports the new candidate
        // through `replace_and_mark_text_in_range`, and editing here would fight that.
        if self.marked.is_some() && !matches!(keystroke.key.as_str(), "enter" | "escape") {
            return;
        }

        match keystroke.key.as_str() {
            "backspace" => {
                if self.selected.is_empty() {
                    let previous = self.previous_boundary(caret);
                    self.select_to(previous, cx);
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "delete" => {
                if self.selected.is_empty() {
                    let next = self.next_boundary(caret);
                    self.select_to(next, cx);
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "left" if shift => self.select_to(self.previous_boundary(caret), cx),
            "left" => {
                let target = if self.selected.is_empty() { self.previous_boundary(caret) } else { self.selected.start };
                self.move_to(target, cx)
            }
            "right" if shift => self.select_to(self.next_boundary(caret), cx),
            "right" => {
                let target = if self.selected.is_empty() { self.next_boundary(caret) } else { self.selected.end };
                self.move_to(target, cx)
            }
            "home" if shift => self.select_to(0, cx),
            "home" => self.move_to(0, cx),
            "end" if shift => self.select_to(self.text.len(), cx),
            "end" => self.move_to(self.text.len(), cx),
            // Enter submits. During a composition it belongs to the IME instead, which is what
            // makes "pick a candidate, then submit" behave the way every other field does.
            "enter" => {
                if self.marked.is_none() {
                    (self.on_commit)(self.text.trim(), cx);
                }
            }
            "a" if command => self.select_all(cx),
            "c" if command => self.copy(cx),
            "x" if command => {
                self.copy(cx);
                self.replace_text_in_range(None, "", window, cx);
            }
            "v" if command => {
                if let Some(pasted) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace_text_in_range(None, &pasted, window, cx);
                }
            }
            // Everything else — character keys included — is left to the platform: it turns them
            // into `replace_text_in_range`, which is also the only path an IME can take.
            _ => {}
        }
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        prev_boundary(&self.text, offset)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        next_boundary(&self.text, offset)
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        let len = self.text.len();
        let start = range.start.min(len);
        let end = range.end.max(start).min(len);
        let range = start..end;
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.range_to_utf16(&self.selected), reversed: self.reversed })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.marked.take().is_some() {
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.replacement_range(range_utf16);
        let text = sanitize(new_text);
        self.splice(&range, &text);
        let caret = range.start + text.len();
        self.selected = caret..caret;
        self.reversed = false;
        self.marked = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.replacement_range(range_utf16);
        let text = sanitize(new_text);
        self.splice(&range, &text);
        let start = range.start;
        let len = text.len();
        self.marked = if len == 0 { None } else { Some(start..start + len) };
        // The composition's own selection is measured inside `text`, not inside the document.
        self.selected = new_selected_range_utf16
            .as_ref()
            .map(|range| {
                let from = (start + byte_offset(&text, range.start)).min(start + len);
                let to = (start + byte_offset(&text, range.end)).max(from).min(start + len);
                from..to
            })
            .unwrap_or(start + len..start + len);
        self.reversed = false;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let painted = self.painted.borrow();
        let line = painted.line.as_ref()?;
        let len = self.text.len();
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                element_bounds.left() + line.x_for_index(range.start.min(len)),
                element_bounds.top(),
            ),
            point(
                element_bounds.left() + line.x_for_index(range.end.min(len)),
                element_bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let painted = self.painted.borrow();
        let (Some(bounds), Some(line)) = (painted.bounds.as_ref(), painted.line.as_ref()) else {
            return None;
        };
        if self.text.is_empty() {
            return Some(0);
        }
        let x = position.x - bounds.left();
        let index = line.index_for_x(x).unwrap_or_else(|| line.closest_index_for_x(x)).min(self.text.len());
        Some(self.offset_to_utf16(index))
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let text = self.text.clone();
        let text_len = text.len();
        let selected = self.selected.clone();
        let caret = self.caret();
        let marked = self.marked.clone();
        let placeholder = self.placeholder.clone();
        let empty = text.is_empty();
        let watched = self.watched;
        let focus = self.focus.clone();
        let painted = Rc::clone(&self.painted);
        let entity = cx.entity();

        let body = canvas(
            move |bounds, _, _| {
                // Log while the bounds keep changing: the first frame happens before the page is
                // scrolled, and the test needs the settled rectangle.
                if watched && bounds.size.width > px(0.0) {
                    static LAST: std::sync::Mutex<Option<(i32, i32, i32, i32)>> = std::sync::Mutex::new(None);
                    let current = (
                        f32::from(bounds.origin.x) as i32,
                        f32::from(bounds.origin.y) as i32,
                        f32::from(bounds.size.width) as i32,
                        f32::from(bounds.size.height) as i32,
                    );
                    if let Ok(mut last) = LAST.lock() {
                        if *last != Some(current) {
                            *last = Some(current);
                            log(&format!(
                                "bounds x={} y={} w={} h={}",
                                f32::from(bounds.origin.x),
                                f32::from(bounds.origin.y),
                                f32::from(bounds.size.width),
                                f32::from(bounds.size.height)
                            ));
                        }
                    }
                }
            },
            move |bounds, _, window, cx| {
                let font_size = px(12.5);
                let line_height = px(18.0);
                let style = window.text_style();
                let display: SharedString = if empty { placeholder.clone() } else { text.clone().into() };
                let color: Hsla = if empty { t::text_faint().into() } else { t::text().into() };
                let run = TextRun {
                    len: display.len(),
                    font: style.font(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let runs = runs_for(&run, display.len(), marked.as_ref());
                let shaped = window.text_system().shape_line(display, font_size, &runs, None);
                let origin = point(bounds.origin.x + px(2.0), bounds.origin.y + (bounds.size.height - line_height) / 2.0);

                if selected.start < selected.end {
                    window.paint_quad(fill(
                        Bounds::from_corners(
                            point(origin.x + shaped.x_for_index(selected.start), origin.y),
                            point(origin.x + shaped.x_for_index(selected.end), origin.y + line_height),
                        ),
                        t::accent_dim(),
                    ));
                }
                let _ = shaped.paint(origin, line_height, window, cx);
                if focus.is_focused(window) {
                    let x = if empty { px(0.0) } else { shaped.x_for_index(caret.min(text_len)) };
                    window.paint_quad(fill(
                        Bounds::new(point(origin.x + x, origin.y + px(2.0)), size(px(1.5), line_height - px(5.0))),
                        t::accent(),
                    ));
                }

                // Registering the handler is what routes committed characters and IME compositions
                // into this entity. Without it the platform has nowhere to send them.
                window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
                // Remember where the line landed: the platform asks for the caret rectangle while
                // positioning the IME candidate window.
                *painted.borrow_mut() = Painted { line: Some(shaped), bounds: Some(bounds) };
            },
        )
        .size_full();

        div()
            .id(self.id.clone())
            .flex()
            .flex_row()
            .items_center()
            .h(px(26.0))
            .px_2()
            .rounded(px(7.0))
            .bg(t::field())
            .border_1()
            .border_color(if focused { t::accent() } else { t::hairline() })
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus);
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| this.key_down(event, window, cx)))
            .child(body)
    }
}
