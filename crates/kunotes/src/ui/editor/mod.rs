//! `EditorPane`: one open note. Loads it, shows it in the current view mode
//! (Live / Source / Split / Preview), and autosaves it.
//!
//! Live mode uses our own editor (`live/`); Source and Split use gpui-kit's
//! code editor. Only one of them is in charge of the text at a time: when the
//! mode switches, the text (and cursor) is handed over to the other.
//!
//! A new pane is created for every note that gets opened, so nothing from the
//! previous note (cursor, unsaved state) can leak into the next one.

#[cfg(test)]
mod editor_tests;
mod formatter_bar;
mod live;
mod markdown_style;
mod preview;
mod status_bar;
mod view_mode_switch;

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Task, Window, div, prelude::FluentBuilder as _, px,
};
use kunotes_core::format::Edit;
use kunotes_core::fs_ops::atomic_write;
use kunotes_core::line_ending::{self, LineEnding};
use kunotes_core::settings::ViewMode;
use kunotes_core::{cursor, live_buffer, paths};

use live::{LiveEditor, LiveEditorEvent};
use markdown_style::highlighter_factory;

use crate::actions::{EDITOR, FormatBold, FormatItalic, FormatLink, SaveNow};
use crate::settings_store::SettingsStore;

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
    /// The text the preview shows (updated a little behind the editor while typing).
    preview_text: SharedString,
    /// Updates the preview and character count; `None` when idle.
    refresh_task: Option<Task<()>>,
    /// The text changed while `refresh_task` was running, so run it once more.
    refresh_again: bool,
    /// Counted in the background: counting a 1 MB note takes ~30 ms, too slow per keystroke.
    char_count: usize,
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
}

/// Reads a note from disk. Notes are small, so this runs on the UI thread.
pub fn load_note(path: PathBuf) -> Result<LoadedNote, String> {
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
    })
}

impl EditorPane {
    pub fn new(
        note: LoadedNote,
        vault_root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let LoadedNote {
            path,
            text,
            line_ending,
            can_save,
        } = note;

        let live_active = uses_live(SettingsStore::get(cx).view_mode, true);
        let live = cx.new(|cx| LiveEditor::new(&text, !can_save, cx));
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .folding(false) // code folding doesn't suit prose
                .soft_wrap(true)
                .placeholder("Start writing…")
                .default_value(text.clone());
            // Our own markdown styling (see `markdown_style.rs`) instead of the built-in one.
            state.set_highlighter_factory(highlighter_factory(), cx);
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
            // Switching between Live and Source hands the text to the other editor.
            cx.observe_global_in::<SettingsStore>(window, |pane, window, cx| {
                pane.sync_active_editor(window, cx)
            }),
        ];

        EditorPane {
            path,
            vault_root,
            line_ending,
            can_save,
            dirty: false,
            live_active,
            save_task: None,
            preview_text: text.clone().into(),
            refresh_task: None,
            refresh_again: false,
            char_count: cursor::char_count(&text),
            editor,
            live,
            _subscriptions: subscriptions,
        }
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

    /// The note was renamed or moved on disk; keep editing it at its new path.
    pub fn set_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.path = path;
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

    // ----- Saving -----

    fn on_text_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.schedule_save(cx);
        self.schedule_refresh(cx);
        cx.emit(EditorEvent::Edited);
        cx.notify();
    }

    /// Saves `SAVE_DELAY` after the last change. Each new change restarts the wait,
    /// because replacing `save_task` drops (cancels) the previous one.
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
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

    fn save_action(&mut self, _: &SaveNow, _: &mut Window, cx: &mut Context<Self>) {
        self.save_now(cx);
    }

    // ----- Preview and character count -----

    /// Updates the preview and character count at most every `PREVIEW_DELAY`
    /// while typing. The count runs on a background thread.
    fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_task.is_some() {
            self.refresh_again = true; // the running update will start one more
            return;
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PREVIEW_DELAY).await;
            let Ok(text) = this.update(cx, |pane, cx| {
                let text = pane.text(cx);
                pane.preview_text = text.clone();
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

    // ----- Formatting -----

    /// Runs a `kunotes_core::format` transform on the current selection and applies it
    /// as one undoable edit.
    pub fn apply_format(
        &mut self,
        transform: impl FnOnce(&str, Range<usize>) -> Edit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_save {
            return;
        }
        let text = self.text(cx);
        let edit = transform(&text, self.selection(cx));
        if self.live_active {
            self.live.update(cx, |live, cx| {
                live.apply_edit(edit.range, &edit.replacement, edit.new_selection, cx);
                live.focus(window, cx);
            });
            return;
        }
        self.editor.update(cx, |editor, cx| {
            // The editor can only replace the selection, so select the range first.
            editor.set_selected_range(edit.range, cx);
            editor.replace(edit.replacement, window, cx);
            editor.set_selected_range(edit.new_selection, cx);
            editor.focus(window, cx);
        });
    }

    /// A checkbox was clicked in the preview: flip `[ ]` ↔ `[x]` on that line.
    /// The cursor and focus stay where they are.
    fn toggle_task(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_save {
            return;
        }
        let text = self.text(cx);
        let Some((range, replacement)) = live_buffer::task_toggle(&text, offset) else {
            return;
        };
        if self.live_active {
            let selection = self.selection(cx);
            self.live.update(cx, |live, cx| {
                live.apply_edit(range, replacement, selection, cx);
            });
        } else {
            self.editor.update(cx, |editor, cx| {
                let selection = editor.selected_range();
                editor.set_selected_range(range, cx);
                editor.replace(replacement, window, cx);
                editor.set_selected_range(selection, cx);
            });
        }
        // Show the change right away instead of after the preview delay.
        self.preview_text = self.text(cx);
        cx.notify();
    }

    fn format_bold(&mut self, _: &FormatBold, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_format(kunotes_core::format::bold, window, cx);
    }

    fn format_italic(&mut self, _: &FormatItalic, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_format(kunotes_core::format::italic, window, cx);
    }

    fn format_link(&mut self, _: &FormatLink, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_format(kunotes_core::format::link, window, cx);
    }

    // ----- Drawing -----

    fn render_header(&self, mode: ViewMode, cx: &mut Context<Self>) -> impl IntoElement {
        let parts = paths::breadcrumb(&self.vault_root, &self.path);
        let last = parts.len().saturating_sub(1);
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;

        let mut breadcrumb = h_flex().gap_1().text_sm().overflow_hidden();
        for (index, part) in parts.into_iter().enumerate() {
            if index > 0 {
                breadcrumb =
                    breadcrumb.child(Icon::new(IconName::ChevronRight).xsmall().text_color(muted));
            }
            let color = if index == last { foreground } else { muted };
            breadcrumb = breadcrumb.child(div().text_color(color).child(part));
        }

        h_flex()
            .justify_between()
            // The switch gets the same space on its right as above and below it.
            .pl_3()
            .pr_1p5()
            .py_1p5()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(breadcrumb)
            .child(view_mode_switch::render(mode, cx))
    }

    /// The Source/Split editor: monospace, like a code editor.
    fn render_source_editor(&self) -> AnyElement {
        Editor::new(&self.editor)
            .bordered(false)
            .readonly(!self.can_save)
            .size_full()
            // The editor adds 12px of its own on the left, so this lines the
            // text up with Live and Preview (24px from the edge in every mode).
            .pl_3()
            .pr_6()
            .py_4()
            .text_size(px(15.))
            .into_any_element()
    }
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = SettingsStore::get(cx).view_mode;
        let (line, column) = cursor::line_col(&self.text(cx), self.cursor(cx));

        let body: AnyElement = match mode {
            ViewMode::Preview => {
                preview::render(self.preview_text.clone(), cx.weak_entity()).into_any_element()
            }
            ViewMode::Split => h_resizable("editor-split")
                .child(resizable_panel().child(self.render_source_editor()))
                .child(
                    resizable_panel()
                        .child(preview::render(self.preview_text.clone(), cx.weak_entity())),
                )
                .into_any_element(),
            ViewMode::Live => self.live.clone().into_any_element(),
            ViewMode::Source => self.render_source_editor(),
        };

        v_flex()
            .id("editor-pane")
            .key_context(EDITOR)
            .on_action(cx.listener(Self::save_action))
            .on_action(cx.listener(Self::format_bold))
            .on_action(cx.listener(Self::format_italic))
            .on_action(cx.listener(Self::format_link))
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_header(mode, cx))
            .when(mode != ViewMode::Preview, |pane| {
                pane.child(formatter_bar::render(cx.entity(), cx))
            })
            .child(div().flex_1().min_h_0().child(body))
            .child(status_bar::render(line, column, self.char_count, cx))
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
