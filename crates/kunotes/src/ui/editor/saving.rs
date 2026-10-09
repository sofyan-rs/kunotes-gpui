//! Autosave and the delayed preview/character-count refresh of `EditorPane`.
//!
//! Saving waits `SAVE_DELAY` after the last keystroke, then writes with
//! `atomic_write`, restoring the file's line endings.

use gpui_kit::{AppContext as _, Context, SharedString, Window};
use kunotes_core::cursor;
use kunotes_core::fs_ops::atomic_write;

use super::{EditorEvent, EditorPane, PREVIEW_DELAY, SAVE_DELAY};
use crate::actions::SaveNow;

impl EditorPane {
    // ----- Saving -----

    pub(super) fn on_text_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.schedule_save(cx);
        self.schedule_refresh(cx);
        cx.emit(EditorEvent::Edited);
        cx.notify();
    }

    /// Saves `SAVE_DELAY` after the last change. Each new change restarts the wait,
    /// because replacing `save_task` drops (cancels) the previous one.
    pub(super) fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let _ = this.update(cx, |pane, cx| pane.save_now(cx));
        }));
    }

    /// Writes unsaved changes right away. Called by the timer, `secondary-s`,
    /// and the workspace before switching notes or quitting.
    pub fn save_now(&mut self, cx: &mut Context<Self>) {
        self.save_task = None;
        if !self.dirty || !self.can_save {
            return;
        }
        let text = self.text(cx);
        let contents = self.line_ending.apply(&text);
        match atomic_write(&self.path, contents.as_bytes()) {
            Ok(()) => self.dirty = false,
            Err(error) => cx.emit(EditorEvent::Error(format!("Couldn't save: {error}"))),
        }
    }

    pub(super) fn save_action(&mut self, _: &SaveNow, _: &mut Window, cx: &mut Context<Self>) {
        self.save_now(cx);
    }

    // ----- Preview and character count -----

    /// Updates the preview and character count at most every `PREVIEW_DELAY`
    /// while typing. The count runs on a background thread.
    pub(super) fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_task.is_some() {
            self.refresh_again = true; // the running update will start one more
            return;
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PREVIEW_DELAY).await;
            let Ok(text) = this.update(cx, |pane, cx| {
                let text = pane.text(cx);
                pane.set_preview_text(text.clone(), cx);
                cx.notify();
                text
            }) else {
                return; // the pane was closed
            };
            // `SharedString` clones are cheap (shared, not copied).
            let count = cx
                .background_spawn(async move { cursor::char_count(&text) })
                .await;
            let _ = this.update(cx, |pane, cx| {
                pane.char_count = count;
                pane.refresh_task = None;
                if std::mem::take(&mut pane.refresh_again) {
                    pane.schedule_refresh(cx);
                }
                cx.notify();
            });
        }));
    }

    /// Shows `text` in the preview.
    pub(super) fn set_preview_text(&mut self, text: SharedString, cx: &mut Context<Self>) {
        self.preview
            .update(cx, |preview, cx| preview.set_text(&text, cx));
    }
}
