//! Locked notes in the editor pane (`Name.md.age`, see `kunotes_core::lock`).
//!
//! A locked note is either *hidden* (the vault is locked: the pane shows a
//! lock screen and holds no text) or *shown* (decrypted in memory; saving
//! encrypts it again). When the vault locks, a shown note saves, then its
//! editors are rebuilt empty, so the text and its undo history are dropped.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, v_flex};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use kunotes_core::lock::{self, VaultKey};
use kunotes_core::{cursor, paths};

use super::EditorPane;
use crate::ui::unlock_dialog;

/// Whether this pane's note is a locked one, and if so, whether its text is out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    /// A normal note.
    No,
    /// A locked note while the vault is locked: no text in memory.
    Hidden,
    /// A locked note, decrypted.
    Shown,
}

impl EditorPane {
    /// The vault locked or unlocked: show or hide this note's text.
    pub(super) fn sync_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.vault_lock.read(cx).key();
        match (self.secret, key) {
            (Secret::Hidden, Some(key)) => self.reveal(key, window, cx),
            (Secret::Shown, None) => self.hide(window, cx),
            _ => {}
        }
    }

    /// Decrypts the note and shows it.
    fn reveal(&mut self, key: Arc<VaultKey>, window: &mut Window, cx: &mut Context<Self>) {
        match lock::read_note(&key, &self.path) {
            Ok(text) => {
                let text = kunotes_core::line_ending::normalize(&text);
                self.secret = Secret::Shown;
                self.key = Some(key);
                self.rebuild_editors(&text, window, cx);
            }
            Err(error) => cx.emit(super::EditorEvent::Error(error.to_string())),
        }
        cx.notify();
    }

    /// Saves, then forgets the text (the vault locked).
    fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_now(cx); // still has its own copy of the key for this
        self.key = None;
        self.secret = Secret::Hidden;
        self.dirty = false;
        self.rebuild_editors("", window, cx);
        cx.notify();
    }

    /// Replaces both editors and the preview with fresh ones holding `text`.
    /// Old editors (and anything secret in their undo history) are dropped.
    fn rebuild_editors(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (editor, live, subscriptions) =
            Self::build_editors(text, self.can_save, self.note_dir(), window, cx);
        self.editor = editor;
        self.live = live;
        self.editor_subscriptions = subscriptions;
        self.set_preview_text(text.to_string().into(), cx);
        self.char_count = cursor::char_count(text);
    }

    /// The note moved to `path`: it may have been locked or unlocked.
    pub(super) fn follow_lock_change(&mut self, cx: &mut Context<Self>) {
        let locked = lock::is_locked_note(&self.path);
        match (self.secret, locked) {
            (Secret::No, true) => {
                // Just locked, with the text already here: keep showing it.
                self.secret = Secret::Shown;
                self.key = self.vault_lock.read(cx).key();
            }
            (Secret::Shown | Secret::Hidden, false) => {
                self.secret = Secret::No;
                self.key = None;
            }
            _ => {}
        }
    }

    /// The lock screen shown instead of the editor while the vault is locked.
    pub(super) fn render_locked(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = paths::note_title(&self.path);
        let vault_lock = self.vault_lock.clone();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Icon::new(IconName::Lock)
                    .large()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().text_lg().child(format!("“{title}” is locked")))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Enter the vault password to show it."),
            )
            .child(
                Button::new("unlock-note")
                    .primary()
                    .label("Unlock…")
                    .on_click(move |_, window, cx| {
                        unlock_dialog::open(vault_lock.clone(), None, window, cx);
                    }),
            )
            .into_any_element()
    }
}
