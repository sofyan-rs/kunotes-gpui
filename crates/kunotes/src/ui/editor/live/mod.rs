//! `LiveEditor`: the Live mode editor. Markdown markers are hidden except on
//! the line the cursor is on, headings are drawn bigger, tasks get checkboxes.
//!
//! The text is plain markdown (a `LiveBuffer` from `kunotes-core`); nothing is
//! converted, so switching modes never changes the file.
//!
//! - this file: the editor's state and the mouse
//! - `keys.rs`: keyboard actions (arrows, delete, undo, clipboard)
//! - `input.rs`: the OS text input (typing, IME)
//! - `layout.rs`: places each line; `style.rs`: its fonts and colors
//! - `element.rs`: draws everything

mod element;
mod input;
mod keys;
mod layout;
mod style;

use gpui_kit::{
    Bounds, Context, CursorStyle, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
    Pixels, Point, Render, ScrollHandle, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use kunotes_core::live_buffer::LiveBuffer;

use element::LiveElement;
use layout::DocLayout;

/// The key context for the editor. It's the same one gpui-kit's own text
/// inputs use, so all their shortcuts (arrows, Home/End, undo, copy, ...) work
/// here too, with the right keys on every OS.
const KEY_CONTEXT: &str = "Input";

/// Keep this much space between the caret and the edge when scrolling to it.
const SCROLL_MARGIN: f32 = 24.;

pub enum LiveEditorEvent {
    /// The text changed.
    Changed,
}

pub struct LiveEditor {
    focus_handle: FocusHandle,
    buffer: LiveBuffer,
    read_only: bool,
    /// Text being composed with an IME (e.g. Japanese input), not yet committed.
    marked_range: Option<std::ops::Range<usize>>,
    /// The layout from the last frame, for clicks and Up/Down.
    last_layout: Option<DocLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    /// True while the mouse button is held after a click in the text.
    selecting: bool,
    /// Up/Down keep the caret at this x, even across short lines.
    preferred_x: Option<Pixels>,
    scroll_handle: ScrollHandle,
    /// Scroll the caret into view on the next frame (after typing or moving).
    scroll_to_cursor: bool,
}

impl EventEmitter<LiveEditorEvent> for LiveEditor {}

impl Focusable for LiveEditor {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl LiveEditor {
    pub fn new(text: &str, read_only: bool, cx: &mut Context<Self>) -> Self {
        LiveEditor {
            focus_handle: cx.focus_handle(),
            buffer: LiveBuffer::new(text),
            read_only,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            selecting: false,
            preferred_x: None,
            scroll_handle: ScrollHandle::new(),
            scroll_to_cursor: false,
        }
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    pub fn selection(&self) -> std::ops::Range<usize> {
        self.buffer.selection()
    }

    pub fn cursor(&self) -> usize {
        self.buffer.cursor()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut gpui_kit::App) {
        window.focus(&self.focus_handle, cx);
    }

    /// Replaces the whole text (after editing in another mode). Can be undone.
    pub fn set_text(&mut self, text: &str, cursor: usize, cx: &mut Context<Self>) {
        if self.buffer.text() != text {
            let all = 0..self.buffer.text().len();
            self.buffer.replace(all, text, false);
        }
        self.buffer.move_to(cursor, false);
        cx.notify();
    }

    #[cfg(test)]
    pub fn select(&mut self, range: std::ops::Range<usize>, cx: &mut Context<Self>) {
        self.buffer.select(range);
        self.moved(cx);
    }

    /// Replaces `range` and selects `new_selection` afterwards (formatter bar).
    pub fn apply_edit(
        &mut self,
        range: std::ops::Range<usize>,
        replacement: &str,
        new_selection: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        self.buffer.replace(range, replacement, false);
        self.buffer.select(new_selection);
        self.changed(cx);
    }

    // ----- Shared steps -----

    /// Call after every edit.
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.scroll_to_cursor = true;
        cx.emit(LiveEditorEvent::Changed);
        cx.notify();
    }

    /// Call after the cursor or selection moved without an edit.
    fn moved(&mut self, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.scroll_to_cursor = true;
        cx.notify();
    }

    /// Runs an edit on the buffer, unless the note is read-only.
    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut LiveBuffer)) {
        if self.read_only {
            return;
        }
        let before = self.buffer.text().len();
        let selection = self.buffer.selection();
        edit(&mut self.buffer);
        self.marked_range = None;
        if self.buffer.text().len() != before || self.buffer.selection() != selection {
            self.changed(cx);
        }
    }

    /// Up/Down: moves a row, using the layout from the last frame.
    fn move_vertically(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(layout) = &self.last_layout else {
            return;
        };
        let cursor = self.buffer.cursor();
        let x = match self.preferred_x {
            Some(x) => x,
            None => layout.caret(cursor).map_or(px(0.), |(caret, _)| caret.x),
        };
        let target = layout.offset_above_or_below(cursor, x, down);
        self.buffer.move_to(target, extend);
        self.preferred_x = Some(x); // keep it for the next Up/Down
        self.scroll_to_cursor = true;
        cx.notify();
    }

    /// Called while drawing: scrolls so the caret is visible, if asked to.
    fn reveal_cursor(&mut self, layout: &DocLayout, bounds: Bounds<Pixels>) {
        if !std::mem::take(&mut self.scroll_to_cursor) {
            return;
        }
        let Some((caret, height)) = layout.caret(self.buffer.cursor()) else {
            return;
        };
        let viewport = self.scroll_handle.bounds();
        let top = bounds.origin.y + caret.y - px(SCROLL_MARGIN);
        let bottom = bounds.origin.y + caret.y + height + px(SCROLL_MARGIN);
        let mut offset = self.scroll_handle.offset();
        // The offset is negative when scrolled down.
        if top < viewport.top() {
            offset.y += viewport.top() - top;
        } else if bottom > viewport.bottom() {
            offset.y -= bottom - viewport.bottom();
        } else {
            return;
        }
        offset.y = offset.y.min(px(0.));
        self.scroll_handle.set_offset(offset);
    }

    /// The text offset under a window position.
    fn offset_at(&self, position: Point<Pixels>) -> Option<usize> {
        let (layout, bounds) = (self.last_layout.as_ref()?, self.last_bounds?);
        Some(layout.offset_for_point(position - bounds.origin))
    }

    // ----- Mouse -----

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus(window, cx);
        let (Some(layout), Some(bounds)) = (&self.last_layout, self.last_bounds) else {
            return;
        };
        // Clicking a checkbox ticks it.
        if let Some(line_start) = layout.checkbox_at(event.position - bounds.origin) {
            if !self.read_only && self.buffer.toggle_task(line_start) {
                self.changed(cx);
            }
            return;
        }
        let Some(offset) = self.offset_at(event.position) else {
            return;
        };
        match event.click_count {
            2 => self.buffer.select_word(offset),
            3.. => self.buffer.select_line(offset),
            _ => {
                self.buffer.move_to(offset, event.modifiers.shift);
                self.selecting = true;
            }
        }
        self.moved(cx);
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || !event.dragging() {
            return;
        }
        if let Some(offset) = self.offset_at(event.position) {
            self.buffer.move_to(offset, true);
            self.moved(cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }
}

impl Render for LiveEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("live-editor")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            // Keyboard actions, see `keys.rs`.
            .map(|root| keys::bind(root, cx))
            .child(
                div()
                    .px_6()
                    .py_4()
                    // Room below the last line, so it isn't stuck to the bottom edge.
                    .pb(px(120.))
                    .child(LiveElement::new(cx.entity())),
            )
    }
}
