//! `EditorPane`: one open note. Loads it, shows it in the current view mode
//! (Source / Split / Preview), and autosaves it.
//!
//! A new pane is created for every note that gets opened, so nothing from the
//! previous note (cursor, unsaved state) can leak into the next one.

#[cfg(test)]
mod editor_tests;
mod formatter_bar;
mod preview;
mod status_bar;

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Toggle, ToggleGroup};
use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription, Task, Window,
    div, prelude::FluentBuilder as _, px,
};
use kunotes_core::format::Edit;
use kunotes_core::fs_ops::atomic_write;
use kunotes_core::line_ending::{self, LineEnding};
use kunotes_core::settings::ViewMode;
use kunotes_core::{cursor, paths};

use crate::actions::{EDITOR, FormatBold, FormatItalic, FormatLink, SaveNow};
use crate::settings_store::SettingsStore;

/// Save this long after the last keystroke.
const SAVE_DELAY: Duration = Duration::from_millis(500);
/// Refresh the split-view preview at most this often while typing.
const PREVIEW_DELAY: Duration = Duration::from_millis(150);

/// Something the workspace should show to the user.
pub enum EditorEvent {
    Error(String),
}

pub struct EditorPane {
    path: PathBuf,
    vault_root: PathBuf,
    editor: Entity<EditorState>,
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

        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .folding(false) // code folding doesn't suit prose
                .soft_wrap(true)
                .placeholder("Start writing…")
                .default_value(text.clone())
        });

        let subscriptions = vec![
            cx.subscribe(&editor, |pane, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    pane.on_text_changed(cx);
                }
            }),
            // Cursor moves don't send an event, but they do notify; redraw the status bar.
            cx.observe(&editor, |_, _, cx| cx.notify()),
        ];

        EditorPane {
            path,
            vault_root,
            line_ending,
            can_save,
            dirty: false,
            save_task: None,
            preview_text: text.clone().into(),
            refresh_task: None,
            refresh_again: false,
            char_count: cursor::char_count(&text),
            editor,
            _subscriptions: subscriptions,
        }
    }

    /// The text editor's state. For tests.
    #[cfg(test)]
    pub fn editor_state(&self) -> Entity<EditorState> {
        self.editor.clone()
    }

    /// The character count shown in the status bar. For tests.
    #[cfg(test)]
    pub fn char_count(&self) -> usize {
        self.char_count
    }

    /// True if the note opened read-only (not valid UTF-8).
    pub fn is_read_only(&self) -> bool {
        !self.can_save
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The note's display name, e.g. "Plan" for `Plan.md`.
    pub fn title(&self) -> String {
        paths::note_title(&self.path)
    }

    /// The note was renamed or moved on disk; keep editing it at its new path.
    pub fn set_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.path = path;
        if self.dirty {
            self.schedule_save(cx);
        }
        cx.notify();
    }

    // ----- Saving -----

    fn on_text_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.schedule_save(cx);
        self.schedule_refresh(cx);
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
        let text = self.editor.read(cx).value();
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
                let text = pane.editor.read(cx).value();
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
        let (text, selection) = {
            let editor = self.editor.read(cx);
            (editor.value(), editor.selected_range())
        };
        let edit = transform(&text, selection);
        self.editor.update(cx, |editor, cx| {
            // The editor can only replace the selection, so select the range first.
            editor.set_selected_range(edit.range, cx);
            editor.replace(edit.replacement, window, cx);
            editor.set_selected_range(edit.new_selection, cx);
            editor.focus(window, cx);
        });
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
            .px_3()
            .py_1()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(breadcrumb)
            .child(view_mode_toggle(mode))
    }

    fn render_editor(&self) -> AnyElement {
        Editor::new(&self.editor)
            .bordered(false)
            .readonly(!self.can_save)
            .size_full()
            .px_6()
            .py_4()
            .text_size(px(15.))
            .into_any_element()
    }
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = match SettingsStore::get(cx).view_mode {
            ViewMode::Live => ViewMode::Source, // Live mode arrives in Phase 9
            mode => mode,
        };
        let (line, column) = {
            let editor = self.editor.read(cx);
            cursor::line_col(&editor.value(), editor.cursor())
        };

        let body: AnyElement = match mode {
            ViewMode::Preview => preview::render(self.preview_text.clone()).into_any_element(),
            ViewMode::Split => h_resizable("editor-split")
                .child(resizable_panel().child(self.render_editor()))
                .child(resizable_panel().child(preview::render(self.preview_text.clone())))
                .into_any_element(),
            _ => self.render_editor(),
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

/// The Source / Split / Preview switch. It sends the matching action, which the
/// workspace handles (so the same code runs for the buttons, shortcuts, and menu).
fn view_mode_toggle(mode: ViewMode) -> impl IntoElement {
    const MODES: [(ViewMode, &str); 3] = [
        (ViewMode::Source, "Source"),
        (ViewMode::Split, "Split"),
        (ViewMode::Preview, "Preview"),
    ];
    ToggleGroup::new("view-mode")
        .segmented()
        .small()
        .children(
            MODES
                .iter()
                .enumerate()
                .map(|(index, (m, label))| Toggle::new(index).label(*label).checked(*m == mode)),
        )
        .on_click(move |states, window, cx| {
            // The group reports every toggle's new state; find the one that turned on.
            // Clicking the already-active mode turns it "off", which we ignore.
            let clicked = MODES
                .iter()
                .zip(states)
                .find(|((m, _), on)| **on && *m != mode)
                .map(|((m, _), _)| *m);
            let action: Box<dyn gpui_kit::Action> = match clicked {
                Some(ViewMode::Split) => Box::new(crate::actions::ViewSplit),
                Some(ViewMode::Preview) => Box::new(crate::actions::ViewPreview),
                Some(_) => Box::new(crate::actions::ViewSource),
                None => return,
            };
            window.dispatch_action(action, cx);
        })
}
