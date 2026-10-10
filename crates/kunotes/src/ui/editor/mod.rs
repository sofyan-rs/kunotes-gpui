//! `EditorPane`: one open note. Loads it, shows it in the current view mode
//! (Live / Source / Split / Preview), and autosaves it.
//!
//! Live mode uses our own editor (`live/`); Source and Split use gpui-kit's
//! code editor. Only one of them is in charge of the text at a time: when the
//! mode switches, the text (and cursor) is handed over to the other.
//!
//! A new pane is created for every note that gets opened, so nothing from the
//! previous note (cursor, unsaved state) can leak into the next one.

mod drawing;
mod edit_menu;
#[cfg(test)]
mod editor_tests;
mod formatter_bar;
mod formatting;
mod live;
mod locked;
mod markdown_style;
mod preview;
mod saving;
mod status_bar;
mod view_mode_switch;

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::component::text::TextViewState;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, Focusable as _, SharedString,
    Subscription, Task, Window,
};
use kunotes_core::cursor;
use kunotes_core::line_ending::{self, LineEnding};
use kunotes_core::lock::{self, VaultKey};
use kunotes_core::settings::ViewMode;

use live::{LiveEditor, LiveEditorEvent};
use locked::Secret;
use markdown_style::highlighter_factory;

use crate::settings_store::SettingsStore;
use crate::vault_lock::VaultLock;

/// Save this long after the last keystroke.
const SAVE_DELAY: Duration = Duration::from_millis(500);
/// Refresh the split-view preview at most this often while typing.
const PREVIEW_DELAY: Duration = Duration::from_millis(150);

/// Something the workspace should show to the user.
pub enum EditorEvent {
    Error(String),
    /// The text changed (a preview tab becomes permanent when edited).
    Edited,
}

pub struct EditorPane {
    path: PathBuf,
    vault_root: PathBuf,
    /// The Source/Split editor.
    editor: Entity<EditorState>,
    /// The Live editor.
    live: Entity<LiveEditor>,
    /// True while the Live editor is in charge of the text (Live mode, or
    /// Preview entered from Live).
    live_active: bool,
    /// The file's original line endings, restored when saving.
    line_ending: LineEnding,
    /// False if the file wasn't valid UTF-8: saving would damage it, so we never write.
    can_save: bool,
    /// True when the editor has changes that aren't on disk yet.
    dirty: bool,
    save_task: Option<Task<()>>,
    /// The rendered preview, updated a little behind the editor while typing.
    /// Kept here (not inside the view) so its right-click menu can read and
    /// change the selection.
    preview: Entity<TextViewState>,
    /// Updates the preview and character count; `None` when idle.
    refresh_task: Option<Task<()>>,
    /// The text changed while `refresh_task` was running, so run it once more.
    refresh_again: bool,
    /// Counted in the background: counting a 1 MB note takes ~30 ms, too slow per keystroke.
    char_count: usize,
    /// Is this a locked note, and is its text shown (see `locked.rs`)?
    secret: Secret,
    /// The vault key while a locked note is shown (to save it encrypted).
    key: Option<Arc<VaultKey>>,
    vault_lock: Entity<VaultLock>,
    /// Listeners on the two editors; replaced when the editors are rebuilt.
    editor_subscriptions: Vec<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for EditorPane {}

/// A note read from disk, ready to be shown in an `EditorPane`.
pub struct LoadedNote {
    path: PathBuf,
    /// The text with `\n` line endings (what the editor works with).
    text: String,
    line_ending: LineEnding,
    can_save: bool,
    secret: Secret,
}

/// Reads a note from disk. Notes are small, so this runs on the UI thread.
/// A locked note is decrypted with `key`; without one it opens hidden.
pub fn load_note(path: PathBuf, key: Option<&VaultKey>) -> Result<LoadedNote, String> {
    if lock::is_locked_note(&path) {
        let (text, secret) = match key {
            Some(key) => (
                lock::read_note(key, &path).map_err(|error| error.to_string())?,
                Secret::Shown,
            ),
            None => (String::new(), Secret::Hidden),
        };
        return Ok(LoadedNote {
            line_ending: LineEnding::detect(&text),
            text: line_ending::normalize(&text),
            path,
            can_save: true,
            secret,
        });
    }
    let bytes = fs::read(&path).map_err(|error| format!("Couldn't open note: {error}"))?;
    let (text, can_save) = match String::from_utf8(bytes) {
        Ok(text) => (text, true),
        // Not valid UTF-8: show it with replacement characters, but never save over it.
        Err(error) => (
            String::from_utf8_lossy(error.as_bytes()).into_owned(),
            false,
        ),
    };
    Ok(LoadedNote {
        line_ending: LineEnding::detect(&text),
        text: line_ending::normalize(&text),
        path,
        can_save,
        secret: Secret::No,
    })
}

impl EditorPane {
    pub fn new(
        note: LoadedNote,
        vault_root: PathBuf,
        vault_lock: Entity<VaultLock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let LoadedNote {
            path,
            text,
            line_ending,
            can_save,
            secret,
        } = note;

        let live_active = uses_live(SettingsStore::get(cx).view_mode, true);
        let note_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let (editor, live, editor_subscriptions) =
            Self::build_editors(&text, can_save, note_dir, window, cx);

        let subscriptions = vec![
            // Switching between Live and Source hands the text to the other editor.
            cx.observe_global_in::<SettingsStore>(window, |pane, window, cx| {
                pane.sync_active_editor(window, cx)
            }),
            // The vault locked or unlocked: show or hide a locked note's text.
            cx.observe_in(&vault_lock, window, |pane, _, window, cx| {
                pane.sync_lock(window, cx)
            }),
        ];
        let key = match secret {
            Secret::Shown => vault_lock.read(cx).key(),
            _ => None,
        };

        EditorPane {
            path,
            vault_root,
            line_ending,
            can_save,
            dirty: false,
            live_active,
            save_task: None,
            preview: cx.new(|cx| TextViewState::markdown(&text, cx)),
            refresh_task: None,
            refresh_again: false,
            char_count: cursor::char_count(&text),
            editor,
            live,
            secret,
            key,
            vault_lock,
            editor_subscriptions,
            _subscriptions: subscriptions,
        }
    }

    /// Makes the Live and Source editors holding `text`, and their listeners.
    fn build_editors(
        text: &str,
        can_save: bool,
        note_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<EditorState>, Entity<LiveEditor>, Vec<Subscription>) {
        let live = cx.new(|cx| LiveEditor::new(text, !can_save, note_dir, cx));
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .folding(false) // code folding doesn't suit prose
                .soft_wrap(true)
                .placeholder("Start writing…")
                .default_value(text.to_string());
            // Our own markdown styling (see `markdown_style.rs`) instead of the built-in one.
            state.set_highlighter_factory(highlighter_factory(), cx);
            // Our own right-click menu replaces the built-in one (see `edit_menu.rs`).
            state.set_context_menu_enabled(false);
            state
        });

        let subscriptions = vec![
            cx.subscribe(&editor, |pane, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    pane.on_text_changed(cx);
                }
            }),
            cx.subscribe(&live, |pane, _, event: &LiveEditorEvent, cx| match event {
                LiveEditorEvent::Changed => pane.on_text_changed(cx),
            }),
            // Cursor moves don't send an event, but they do notify; redraw the status bar.
            cx.observe(&editor, |_, _, cx| cx.notify()),
            cx.observe(&live, |_, _, cx| cx.notify()),
        ];
        (editor, live, subscriptions)
    }

    /// The folder the note is in (relative image links start here).
    fn note_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// The note's current text (from whichever editor is in charge).
    pub fn text(&self, cx: &App) -> SharedString {
        if self.live_active {
            self.live.read(cx).text().to_string().into()
        } else {
            self.editor.read(cx).value()
        }
    }

    /// The selection in the editor that's in charge.
    fn selection(&self, cx: &App) -> Range<usize> {
        if self.live_active {
            self.live.read(cx).selection()
        } else {
            self.editor.read(cx).selected_range()
        }
    }

    fn cursor(&self, cx: &App) -> usize {
        if self.live_active {
            self.live.read(cx).cursor()
        } else {
            self.editor.read(cx).cursor()
        }
    }

    /// Selects `range` and focuses the editor. For tests.
    #[cfg(test)]
    pub fn select_and_focus(&self, range: Range<usize>, window: &mut Window, cx: &mut App) {
        if self.live_active {
            self.live.update(cx, |live, cx| live.select(range, cx));
        } else {
            self.editor
                .update(cx, |editor, cx| editor.set_selected_range(range, cx));
        }
        self.focus(window, cx);
    }

    /// Where the preview's menu sends its actions. For tests.
    #[cfg(test)]
    pub fn preview_focus(&self, cx: &App) -> gpui_kit::FocusHandle {
        self.preview.read(cx).focus_handle().clone()
    }

    /// The selected text. For tests.
    #[cfg(test)]
    pub fn selected_text(&self, cx: &App) -> String {
        self.text(cx)[self.selection(cx)].to_string()
    }

    /// The character count shown in the status bar. For tests.
    #[cfg(test)]
    pub fn char_count(&self) -> usize {
        self.char_count
    }

    /// True if there are changes not yet written to disk (shown as a dot on the tab).
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// True if the note opened read-only (not valid UTF-8).
    pub fn is_read_only(&self) -> bool {
        !self.can_save
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Puts the text cursor in the editor.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if self.live_active {
            let handle = self.live.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        } else {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        }
    }

    /// The file changed on disk (e.g. a git sync brought in another computer's
    /// edit): show the new text. Does nothing while there is unsaved typing, so
    /// nothing typed here is lost (it gets saved and synced as usual).
    pub fn reload_from_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dirty {
            return;
        }
        if self.secret == Secret::Hidden {
            return; // nothing shown; it's decrypted fresh when unlocked
        }
        let note = match load_note(self.path.clone(), self.key.as_deref()) {
            Ok(note) => note,
            Err(error) => {
                log::warn!("couldn't reload {}: {error}", self.path.display());
                return;
            }
        };
        if note.text == self.text(cx).as_ref() {
            return;
        }
        self.line_ending = note.line_ending;
        self.can_save = note.can_save;
        let cursor = self.cursor(cx).min(note.text.len());
        if self.live_active {
            self.live
                .update(cx, |live, cx| live.set_text(&note.text, cursor, cx));
        } else {
            // `set_value` doesn't count as an edit, so the note isn't saved back.
            self.editor.update(cx, |editor, cx| {
                editor.set_value(note.text.clone(), window, cx)
            });
        }
        self.set_preview_text(note.text.into(), cx);
        self.schedule_refresh(cx);
        cx.notify();
    }

    /// The note was renamed or moved on disk; keep editing it at its new path.
    pub fn set_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.path = path;
        self.follow_lock_change(cx);
        let note_dir = self.note_dir();
        self.live
            .update(cx, |live, cx| live.set_note_dir(note_dir, cx));
        if self.dirty {
            self.schedule_save(cx);
        }
        cx.notify();
    }

    // ----- Live / Source switching -----

    /// When the mode switches between Live and Source/Split, hands the text and
    /// cursor to the other editor. (Preview shows neither, so nothing changes.)
    fn sync_active_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live_active = uses_live(SettingsStore::get(cx).view_mode, self.live_active);
        if live_active == self.live_active {
            return;
        }
        let text = self.text(cx);
        let cursor = self.cursor(cx);
        let was_focused = if self.live_active {
            self.live.read(cx).focus_handle(cx).is_focused(window)
        } else {
            self.editor.read(cx).focus_handle(cx).is_focused(window)
        };
        self.live_active = live_active;
        if live_active {
            self.live
                .update(cx, |live, cx| live.set_text(&text, cursor, cx));
        } else {
            self.editor.update(cx, |editor, cx| {
                // Only replace when different: replacing counts as an edit.
                if editor.value() != text {
                    editor.replace_all(text.clone(), window, cx);
                }
                editor.set_selected_range(cursor..cursor, cx);
            });
        }
        if was_focused {
            self.focus(window, cx);
        }
        cx.notify();
    }
}

/// True if the Live editor should be in charge in `mode`. Preview shows
/// neither editor, so it keeps `current`.
fn uses_live(mode: ViewMode, current: bool) -> bool {
    match mode {
        ViewMode::Live => true,
        ViewMode::Source | ViewMode::Split => false,
        ViewMode::Preview => current,
    }
}
