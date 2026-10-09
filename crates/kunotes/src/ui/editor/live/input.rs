//! Connects `LiveEditor` to the operating system's text input. Typed
//! characters, IME composition (e.g. Japanese or Chinese input), and the
//! emoji picker all arrive through these methods.
//!
//! The OS counts text in UTF-16 units, while our buffer uses UTF-8 bytes, so
//! every range is converted on the way in and out.

use std::ops::Range;

use gpui_kit::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, size};

use super::LiveEditor;

impl LiveEditor {
    fn utf16_from_byte(&self, offset: usize) -> usize {
        let text = self.buffer.text();
        text[..offset.min(text.len())].encode_utf16().count()
    }

    fn byte_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (index, ch) in self.buffer.text().char_indices() {
            if utf16 >= offset {
                return index;
            }
            utf16 += ch.len_utf16();
        }
        self.buffer.text().len()
    }

    fn range_utf16_from_byte(&self, range: &Range<usize>) -> Range<usize> {
        self.utf16_from_byte(range.start)..self.utf16_from_byte(range.end)
    }

    fn range_byte_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.byte_from_utf16(range.start)..self.byte_from_utf16(range.end)
    }

    /// The range the OS wants to replace: the given one, else the text being
    /// composed, else the selection.
    fn target_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| self.range_byte_from_utf16(&range))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.buffer.selection())
    }
}

impl EntityInputHandler for LiveEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_byte_from_utf16(&range_utf16);
        *actual_range = Some(self.range_utf16_from_byte(&range));
        Some(self.buffer.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.buffer.selection();
        Some(UTF16Selection {
            range: self.range_utf16_from_byte(&selection),
            reversed: self.buffer.cursor() < selection.end,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_utf16_from_byte(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    /// Typing: insert (or replace) committed text.
    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = self.target_range(range_utf16);
        // A single typed character can be undone together with its neighbours.
        let is_typing = new_text.chars().count() == 1 && new_text != "\n";
        self.buffer.replace(range, new_text, is_typing);
        self.marked_range = None;
        self.changed(cx);
    }

    /// IME composition: insert text that is still being composed, and mark it.
    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = self.target_range(range_utf16);
        self.buffer.replace(range.clone(), new_text, false);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        // The selection inside the composed text is relative to its start, in UTF-16.
        if let Some(selected) = new_selected_range_utf16 {
            let to_bytes = |utf16: usize| {
                let mut count = 0;
                for (index, ch) in new_text.char_indices() {
                    if count >= utf16 {
                        return index;
                    }
                    count += ch.len_utf16();
                }
                new_text.len()
            };
            self.buffer.select(
                range.start + to_bytes(selected.start)..range.start + to_bytes(selected.end),
            );
        }
        self.changed(cx);
    }

    /// Where the IME should show its candidate window.
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let start = self.byte_from_utf16(range_utf16.start);
        let (caret, height) = layout.caret(start)?;
        Some(Bounds::new(
            element_bounds.origin + caret,
            size(Pixels::from(2.), height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_at(position)?;
        Some(self.utf16_from_byte(offset))
    }
}
