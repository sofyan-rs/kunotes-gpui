//! The text and cursor of the Live editor, with undo/redo. Pure data: the UI
//! turns key presses and clicks into calls here, and draws the result.
//!
//! Offsets are UTF-8 bytes. Moving left/right steps over whole characters as a
//! person sees them (an emoji like 👩‍💻 is one step).

use std::ops::Range;

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation as _};

/// One change, remembered so it can be undone.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Change {
    /// Where the change happened and the text that was there before.
    range_before: Range<usize>,
    text_before: String,
    /// The text that replaced it.
    text_after: String,
    selection_before: Range<usize>,
    selection_after: Range<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveBuffer {
    text: String,
    /// Selected range; empty means just a cursor. `anchor` is the fixed end.
    selection: Range<usize>,
    anchor: usize,
    undo: Vec<Change>,
    redo: Vec<Change>,
    /// True while typing plain characters, so a word typed in a row undoes at once.
    typing_run: bool,
}

impl LiveBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        LiveBuffer {
            text: text.into(),
            ..LiveBuffer::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// The moving end of the selection (where the caret is drawn).
    pub fn cursor(&self) -> usize {
        if self.anchor == self.selection.start {
            self.selection.end
        } else {
            self.selection.start
        }
    }

    // ----- Selection -----

    /// Puts the cursor at `offset` (or extends the selection to it).
    pub fn move_to(&mut self, offset: usize, extend: bool) {
        let offset = self.clamp(offset);
        if extend {
            self.selection = self.anchor.min(offset)..self.anchor.max(offset);
        } else {
            self.selection = offset..offset;
            self.anchor = offset;
        }
        self.typing_run = false;
    }

    pub fn select(&mut self, range: Range<usize>) {
        let range = self.clamp(range.start)..self.clamp(range.end);
        self.anchor = range.start;
        self.selection = range;
        self.typing_run = false;
    }

    pub fn select_all(&mut self) {
        self.select(0..self.text.len());
    }

    /// Left arrow: collapses a selection, or moves one character left.
    pub fn move_left(&mut self, extend: bool) {
        if !extend && !self.selection.is_empty() {
            self.move_to(self.selection.start, false);
        } else {
            self.move_to(self.previous_boundary(self.cursor()), extend);
        }
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend && !self.selection.is_empty() {
            self.move_to(self.selection.end, false);
        } else {
            self.move_to(self.next_boundary(self.cursor()), extend);
        }
    }

    /// Alt/Ctrl + Left: to the start of the previous word.
    pub fn move_word_left(&mut self, extend: bool) {
        self.move_to(self.previous_word_start(self.cursor()), extend);
    }

    /// Alt/Ctrl + Right: to the end of the next word.
    pub fn move_word_right(&mut self, extend: bool) {
        self.move_to(self.next_word_end(self.cursor()), extend);
    }

    /// Double-click: selects the word (or run of spaces/punctuation) at `offset`.
    pub fn select_word(&mut self, offset: usize) {
        let offset = self.clamp(offset);
        let range = self
            .text
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|range| range.contains(&offset) && !self.text[range.clone()].contains('\n'))
            .unwrap_or(offset..offset);
        self.select(range);
    }

    /// Triple-click: selects the whole line at `offset`, including its line break.
    pub fn select_line(&mut self, offset: usize) {
        let start = self.line_start(offset);
        let end = self.line_end(offset);
        let end = if end < self.text.len() { end + 1 } else { end };
        self.select(start..end);
    }

    pub fn move_to_line_start(&mut self, extend: bool) {
        self.move_to(self.line_start(self.cursor()), extend);
    }

    pub fn move_to_line_end(&mut self, extend: bool) {
        self.move_to(self.line_end(self.cursor()), extend);
    }

    // ----- Editing -----

    /// Replaces the selection with `new_text` (typing, paste, IME commit).
    pub fn insert(&mut self, new_text: &str) {
        let range = self.selection.clone();
        let is_typing = new_text.chars().count() == 1 && !new_text.contains('\n');
        self.replace(range, new_text, is_typing);
    }

    /// Replaces `range` with `new_text`, putting the cursor after it.
    /// `merge_with_typing` lets single typed characters undo together.
    pub fn replace(&mut self, range: Range<usize>, new_text: &str, merge_with_typing: bool) {
        let range = self.clamp(range.start)..self.clamp(range.end.max(range.start));
        let selection_before = self.selection.clone();
        let text_before = self.text[range.clone()].to_string();
        self.text.replace_range(range.clone(), new_text);
        let cursor = range.start + new_text.len();
        self.selection = cursor..cursor;
        self.anchor = cursor;
        self.redo.clear();

        let merged = merge_with_typing
            && self.typing_run
            && self.undo.last().is_some_and(|last| {
                last.range_before.start + last.text_after.len() == range.start
                    && range.is_empty()
                    && last.text_before.is_empty()
            });
        if merged && let Some(last) = self.undo.last_mut() {
            last.text_after.push_str(new_text);
            last.selection_after = self.selection.clone();
        } else {
            self.undo.push(Change {
                range_before: range,
                text_before,
                text_after: new_text.to_string(),
                selection_before,
                selection_after: self.selection.clone(),
            });
        }
        self.typing_run = merge_with_typing;
    }

    /// Backspace: deletes the selection, or the character before the cursor.
    pub fn backspace(&mut self) {
        let range = if self.selection.is_empty() {
            self.previous_boundary(self.cursor())..self.cursor()
        } else {
            self.selection.clone()
        };
        if !range.is_empty() {
            self.replace(range, "", false);
        }
    }

    /// Delete key: deletes the selection, or the character after the cursor.
    pub fn delete_forward(&mut self) {
        let range = if self.selection.is_empty() {
            self.cursor()..self.next_boundary(self.cursor())
        } else {
            self.selection.clone()
        };
        if !range.is_empty() {
            self.replace(range, "", false);
        }
    }

    /// Alt/Ctrl + Backspace: deletes back to the start of the word.
    pub fn delete_word_back(&mut self) {
        if self.selection.is_empty() {
            let start = self.previous_word_start(self.cursor());
            self.select(start..self.cursor());
        }
        self.backspace();
    }

    /// Cmd + Backspace: deletes back to the start of the line.
    pub fn delete_to_line_start(&mut self) {
        if self.selection.is_empty() {
            let start = self.line_start(self.cursor());
            let start = if start == self.cursor() {
                self.previous_boundary(start)
            } else {
                start
            };
            self.select(start..self.cursor());
        }
        self.backspace();
    }

    /// Enter: a new line that continues a list (`- `, `1. ` → `2. `, `- [ ] `).
    /// On an empty list item, ends the list instead (removes the marker).
    pub fn new_line(&mut self) {
        let line_start = self.line_start(self.selection.start);
        let line = &self.text[line_start..self.line_end(self.selection.start)];
        if let Some((marker_len, next_marker)) = list_continuation(line) {
            if line[marker_len..].trim().is_empty() {
                // Empty item: end the list.
                self.replace(line_start..line_start + line.len(), "", false);
            } else {
                self.insert(&format!("\n{next_marker}"));
            }
            return;
        }
        self.insert("\n");
    }

    /// Ticks or unticks the task box on the line containing `offset`.
    /// Returns true if the line had one.
    pub fn toggle_task(&mut self, offset: usize) -> bool {
        let Some((range, replacement)) = task_toggle(&self.text, self.clamp(offset)) else {
            return false;
        };
        let selection = self.selection.clone();
        self.replace(range, replacement, false);
        self.selection = selection.clone(); // ticking doesn't move the cursor
        self.anchor = selection.start;
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(change) = self.undo.pop() else {
            return false;
        };
        let after = change.range_before.start..change.range_before.start + change.text_after.len();
        self.text.replace_range(after, &change.text_before);
        self.selection = change.selection_before.clone();
        self.anchor = self.selection.start;
        self.redo.push(change);
        self.typing_run = false;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(change) = self.redo.pop() else {
            return false;
        };
        self.text
            .replace_range(change.range_before.clone(), &change.text_after);
        self.selection = change.selection_after.clone();
        self.anchor = self.selection.start;
        self.undo.push(change);
        self.typing_run = false;
        true
    }

    // ----- Positions -----

    pub fn line_start(&self, offset: usize) -> usize {
        self.text[..self.clamp(offset)]
            .rfind('\n')
            .map_or(0, |i| i + 1)
    }

    pub fn line_end(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |i| offset + i)
    }

    /// The start of the character before `offset` (whole emoji / accented letters).
    pub fn previous_boundary(&self, offset: usize) -> usize {
        let mut cursor = GraphemeCursor::new(self.clamp(offset), self.text.len(), true);
        cursor
            .prev_boundary(&self.text, 0)
            .ok()
            .flatten()
            .unwrap_or(0)
    }

    /// The end of the character after `offset`.
    pub fn next_boundary(&self, offset: usize) -> usize {
        let mut cursor = GraphemeCursor::new(self.clamp(offset), self.text.len(), true);
        cursor
            .next_boundary(&self.text, 0)
            .ok()
            .flatten()
            .unwrap_or(self.text.len())
    }

    /// The start of the word before `offset`, skipping spaces and punctuation.
    pub fn previous_word_start(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        self.text[..offset]
            .split_word_bound_indices()
            .rev()
            .find(|(_, word)| word.chars().any(char::is_alphanumeric))
            .map_or(0, |(start, _)| start)
    }

    /// The end of the word after `offset`, skipping spaces and punctuation.
    pub fn next_word_end(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        self.text[offset..]
            .split_word_bound_indices()
            .find(|(_, word)| word.chars().any(char::is_alphanumeric))
            .map_or(self.text.len(), |(start, word)| offset + start + word.len())
    }

    /// Keeps an offset inside the text and on a character boundary.
    fn clamp(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }
}

/// The `[ ]` / `[x]` box on the task line containing `offset`, and what to
/// replace it with to flip it. `None` if that line isn't a task.
pub fn task_toggle(text: &str, offset: usize) -> Option<(Range<usize>, &'static str)> {
    let offset = offset.min(text.len());
    let start = text.get(..offset)?.rfind('\n').map_or(0, |i| i + 1);
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let line = &text[start..end];
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let marker_len = match rest.as_bytes().first() {
        Some(b'-' | b'*' | b'+') if rest.as_bytes().get(1) == Some(&b' ') => 2,
        _ => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || !matches!(rest.as_bytes().get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 2
        }
    };
    let box_start = start + indent + marker_len;
    let replacement = match text.get(box_start..box_start + 3)? {
        "[ ]" => "[x]",
        "[x]" | "[X]" => "[ ]",
        _ => return None,
    };
    Some((box_start..box_start + 3, replacement))
}

/// For a list line: the marker's length and the marker for the next line.
fn list_continuation(line: &str) -> Option<(usize, String)> {
    let indent = &line[..line.len() - line.trim_start().len()];
    let rest = &line[indent.len()..];
    let bytes = rest.as_bytes();
    let (marker, next) = match bytes.first()? {
        b'-' | b'*' | b'+' if bytes.get(1) == Some(&b' ') => {
            let bullet = &rest[..2];
            if rest[2..].starts_with("[ ] ")
                || rest[2..].starts_with("[x] ")
                || rest[2..].starts_with("[X] ")
            {
                (6, format!("{bullet}[ ] "))
            } else {
                (2, bullet.to_string())
            }
        }
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            let separator = *bytes.get(digits)? as char;
            if !matches!(separator, '.' | ')') || bytes.get(digits + 1) != Some(&b' ') {
                return None;
            }
            let number: u64 = rest[..digits].parse().ok()?;
            (digits + 2, format!("{}{separator} ", number + 1))
        }
        _ => return None,
    };
    Some((indent.len() + marker, format!("{indent}{next}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str, cursor: usize) -> LiveBuffer {
        let mut buffer = LiveBuffer::new(text);
        buffer.move_to(cursor, false);
        buffer
    }

    #[test]
    fn typing_inserts_at_the_cursor_and_replaces_a_selection() {
        let mut b = buffer("ac", 1);
        b.insert("b");
        assert_eq!((b.text(), b.cursor()), ("abc", 2));
        b.select(0..3);
        b.insert("x");
        assert_eq!(b.text(), "x");
    }

    #[test]
    fn backspace_and_delete_step_over_whole_characters() {
        let mut b = buffer("a👩‍💻b", "a👩‍💻".len());
        b.backspace();
        assert_eq!(b.text(), "ab");
        let mut b = buffer("é!", 0);
        b.delete_forward();
        assert_eq!(b.text(), "!");
        let mut b = buffer("", 0);
        b.backspace(); // nothing to delete, no panic
        assert_eq!(b.text(), "");
    }

    #[test]
    fn arrows_move_and_extend() {
        let mut b = buffer("ab日", 0);
        b.move_right(false);
        b.move_right(true);
        assert_eq!(b.selection(), 1..2);
        b.move_right(true);
        assert_eq!(b.selection(), 1..5, "the CJK character is one step");
        b.move_left(false);
        assert_eq!(b.selection(), 1..1, "left collapses to the start");
    }

    #[test]
    fn home_and_end_stay_on_the_line() {
        let mut b = buffer("one\ntwo\nthree", 5);
        b.move_to_line_start(false);
        assert_eq!(b.cursor(), 4);
        b.move_to_line_end(true);
        assert_eq!(b.selection(), 4..7);
    }

    #[test]
    fn enter_continues_lists_and_ends_them_on_an_empty_item() {
        let mut b = buffer("- item", 6);
        b.new_line();
        assert_eq!(b.text(), "- item\n- ");
        b.new_line(); // empty item: end the list
        assert_eq!(b.text(), "- item\n");

        let mut b = buffer("  9. nine", 9);
        b.new_line();
        assert_eq!(b.text(), "  9. nine\n  10. ");

        let mut b = buffer("- [x] done", 10);
        b.new_line();
        assert_eq!(b.text(), "- [x] done\n- [ ] ", "a new task starts unticked");

        let mut b = buffer("plain", 5);
        b.new_line();
        assert_eq!(b.text(), "plain\n");
    }

    #[test]
    fn task_boxes_toggle_without_moving_the_cursor() {
        let mut b = buffer("- [ ] a\n* [x] b\nplain", 0);
        assert!(b.toggle_task(3));
        assert!(b.toggle_task(10));
        assert!(!b.toggle_task(17));
        assert_eq!(b.text(), "- [x] a\n* [ ] b\nplain");
        assert_eq!(b.cursor(), 0);
        let mut b = buffer("1. [ ] numbered", 0);
        assert!(b.toggle_task(0));
        assert_eq!(b.text(), "1. [x] numbered");
        assert_eq!(task_toggle("[ ] no marker", 0), None);
        assert_eq!(task_toggle("日本\n- [ ] x", 8), Some((9..12, "[x]")));
    }

    #[test]
    fn undo_groups_typing_and_redo_restores() {
        let mut b = buffer("", 0);
        for c in ["h", "i"] {
            b.insert(c);
        }
        b.insert("\n");
        b.insert("x");
        assert_eq!(b.text(), "hi\nx");
        assert!(b.undo());
        assert_eq!(b.text(), "hi\n");
        assert!(b.undo());
        assert_eq!(b.text(), "hi");
        assert!(b.undo());
        assert_eq!(b.text(), "", "typed characters in a row undo together");
        assert!(!b.undo());
        assert!(b.redo());
        assert_eq!(b.text(), "hi");
        b.backspace();
        assert!(!b.redo(), "a new change clears redo");
    }

    #[test]
    fn moving_the_cursor_ends_a_typing_run() {
        let mut b = buffer("", 0);
        b.insert("a");
        b.move_to(0, false);
        b.insert("b");
        b.undo();
        assert_eq!(b.text(), "a", "the second insert undoes separately");
    }

    #[test]
    fn word_movement_skips_spaces_and_punctuation() {
        let mut b = buffer("one, two  three", 15);
        b.move_word_left(false);
        assert_eq!(b.cursor(), 10);
        b.move_word_left(false);
        assert_eq!(b.cursor(), 5);
        b.move_word_right(false);
        assert_eq!(b.cursor(), 8);
        b.delete_word_back();
        assert_eq!(b.text(), "one,   three");
    }

    #[test]
    fn double_and_triple_click_select_word_and_line() {
        let mut b = buffer("hello wörld\nnext", 0);
        b.select_word(8);
        assert_eq!(&b.text()[b.selection()], "wörld");
        b.select_line(2);
        assert_eq!(&b.text()[b.selection()], "hello wörld\n");
    }

    #[test]
    fn delete_to_line_start_joins_lines_at_the_start() {
        let mut b = buffer("ab\ncd", 5);
        b.delete_to_line_start();
        assert_eq!(b.text(), "ab\n");
        b.delete_to_line_start();
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn offsets_are_clamped_to_character_boundaries() {
        let mut b = buffer("日本", 0);
        b.move_to(1, false); // inside '日'
        assert_eq!(b.cursor(), 0);
        b.move_to(100, false);
        assert_eq!(b.cursor(), 6);
    }
}
