//! Keyboard actions of the Live editor: moving the cursor, selecting,
//! deleting, undo, and the clipboard. The key bindings themselves come from
//! gpui-kit (the "Input" key context), so every OS gets its usual shortcuts.

use gpui_kit::base::actions::{SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::base::input::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, DeleteToPreviousWordStart, Enter,
    IndentInline, MoveDown, MoveEnd, MoveHome, MoveLeft, MoveRight, MoveToEnd, MoveToNextWord,
    MoveToPreviousWord, MoveToStart, MoveUp, Paste, Redo, SelectAll, SelectToEnd,
    SelectToEndOfLine, SelectToNextWordEnd, SelectToPreviousWordStart, SelectToStart,
    SelectToStartOfLine, Undo,
};
use gpui_kit::{ClipboardItem, Context, Div, InteractiveElement as _, Stateful, Window};
use kunotes_core::live_buffer::LiveBuffer;

use super::LiveEditor;

/// Connects every action below to the editor's root element.
pub fn bind(root: Stateful<Div>, cx: &mut Context<LiveEditor>) -> Stateful<Div> {
    root.on_action(cx.listener(LiveEditor::backspace))
        .on_action(cx.listener(LiveEditor::delete))
        .on_action(cx.listener(LiveEditor::delete_word))
        .on_action(cx.listener(LiveEditor::delete_line))
        .on_action(cx.listener(LiveEditor::enter))
        .on_action(cx.listener(LiveEditor::tab))
        .on_action(cx.listener(LiveEditor::undo))
        .on_action(cx.listener(LiveEditor::redo))
        .on_action(cx.listener(LiveEditor::copy))
        .on_action(cx.listener(LiveEditor::cut))
        .on_action(cx.listener(LiveEditor::paste))
        .on_action(cx.listener(LiveEditor::select_all))
        .on_action(cx.listener(LiveEditor::move_left))
        .on_action(cx.listener(LiveEditor::move_right))
        .on_action(cx.listener(LiveEditor::select_left))
        .on_action(cx.listener(LiveEditor::select_right))
        .on_action(cx.listener(LiveEditor::move_up))
        .on_action(cx.listener(LiveEditor::move_down))
        .on_action(cx.listener(LiveEditor::select_up))
        .on_action(cx.listener(LiveEditor::select_down))
        .on_action(cx.listener(LiveEditor::move_home))
        .on_action(cx.listener(LiveEditor::move_end))
        .on_action(cx.listener(LiveEditor::select_home))
        .on_action(cx.listener(LiveEditor::select_end))
        .on_action(cx.listener(LiveEditor::move_to_start))
        .on_action(cx.listener(LiveEditor::move_to_end))
        .on_action(cx.listener(LiveEditor::select_to_start))
        .on_action(cx.listener(LiveEditor::select_to_end))
        .on_action(cx.listener(LiveEditor::word_left))
        .on_action(cx.listener(LiveEditor::word_right))
        .on_action(cx.listener(LiveEditor::select_word_left))
        .on_action(cx.listener(LiveEditor::select_word_right))
}

impl LiveEditor {
    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, LiveBuffer::backspace);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, LiveBuffer::delete_forward);
    }

    fn delete_word(
        &mut self,
        _: &DeleteToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, LiveBuffer::delete_word_back);
    }

    fn delete_line(&mut self, _: &DeleteToBeginningOfLine, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, LiveBuffer::delete_to_line_start);
    }

    fn enter(&mut self, action: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if action.secondary {
            cx.propagate(); // secondary-enter is not ours
            return;
        }
        if action.shift {
            self.edit(cx, |buffer| buffer.insert("\n"));
        } else {
            self.edit(cx, LiveBuffer::new_line);
        }
    }

    fn tab(&mut self, _: &IndentInline, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |buffer| buffer.insert("    "));
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |buffer| {
            buffer.undo();
        });
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |buffer| {
            buffer.redo();
        });
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selection = self.buffer.selection();
        if !selection.is_empty() {
            let text = self.buffer.text()[selection].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        if !self.buffer.selection().is_empty() {
            self.edit(cx, LiveBuffer::backspace);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            // Pasted text may come with Windows line endings; the buffer uses `\n`.
            let text = text.replace("\r\n", "\n");
            self.edit(cx, |buffer| buffer.insert(&text));
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_all();
        cx.notify();
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_left(false);
        self.moved(cx);
    }

    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_right(false);
        self.moved(cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_left(true);
        self.moved(cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_right(true);
        self.moved(cx);
    }

    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(false, false, cx);
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(true, false, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(false, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(true, true, cx);
    }

    fn move_home(&mut self, _: &MoveHome, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to_line_start(false);
        self.moved(cx);
    }

    fn move_end(&mut self, _: &MoveEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to_line_end(false);
        self.moved(cx);
    }

    fn select_home(&mut self, _: &SelectToStartOfLine, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to_line_start(true);
        self.moved(cx);
    }

    fn select_end(&mut self, _: &SelectToEndOfLine, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to_line_end(true);
        self.moved(cx);
    }

    fn move_to_start(&mut self, _: &MoveToStart, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to(0, false);
        self.moved(cx);
    }

    fn move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to(self.buffer.text().len(), false);
        self.moved(cx);
    }

    fn select_to_start(&mut self, _: &SelectToStart, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to(0, true);
        self.moved(cx);
    }

    fn select_to_end(&mut self, _: &SelectToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to(self.buffer.text().len(), true);
        self.moved(cx);
    }

    fn word_left(&mut self, _: &MoveToPreviousWord, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_word_left(false);
        self.moved(cx);
    }

    fn word_right(&mut self, _: &MoveToNextWord, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_word_right(false);
        self.moved(cx);
    }

    fn select_word_left(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.move_word_left(true);
        self.moved(cx);
    }

    fn select_word_right(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.move_word_right(true);
        self.moved(cx);
    }
}
