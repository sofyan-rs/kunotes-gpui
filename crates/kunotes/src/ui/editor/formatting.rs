//! Commands that change the note's text from outside the editors: the
//! formatter bar and its shortcuts, inserting images, and ticking a task from
//! the preview. They work on whichever editor is in charge (Live or Source).

use std::ops::Range;

use gpui_kit::{AppContext as _, Context, PathPromptOptions, Window};
use kunotes_core::format::{self, Edit};
use kunotes_core::{fs_ops, live_buffer};

use super::{EditorEvent, EditorPane};
use crate::actions::{FormatBold, FormatItalic, FormatLink};

impl EditorPane {
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
    pub(super) fn toggle_task(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let text = self.text(cx);
        self.set_preview_text(text, cx);
        cx.notify();
    }

    /// Asks for image files, copies them into the `.img` folder next to the
    /// note (in the background), and inserts `![name](.img/name.png)` links.
    pub fn insert_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_save {
            return;
        }
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Insert".into()),
        });
        let note = self.path.clone();
        cx.spawn_in(window, async move |pane, cx| {
            let paths = match answer.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) | Err(_) => return, // cancelled
                Ok(Err(error)) => {
                    let message = format!("Couldn't open the file picker: {error}");
                    let _ = pane.update(cx, |_, cx| cx.emit(EditorEvent::Error(message)));
                    return;
                }
            };
            // Copying files is disk work: keep it off the UI thread.
            let imported = cx
                .background_spawn(async move {
                    paths
                        .iter()
                        .map(|path| {
                            let alt = path
                                .file_stem()
                                .map(|stem| stem.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            fs_ops::import_image(&note, path).map(|link| (alt, link))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .await;
            let _ = pane.update_in(cx, |pane, window, cx| match imported {
                Ok(images) => pane.apply_format(
                    |text, selection| format::images(text, selection, &images),
                    window,
                    cx,
                ),
                Err(error) => cx.emit(EditorEvent::Error(format!(
                    "Couldn't insert the image: {error}"
                ))),
            });
        })
        .detach();
    }

    pub(super) fn format_bold(
        &mut self,
        _: &FormatBold,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_format(kunotes_core::format::bold, window, cx);
    }

    pub(super) fn format_italic(
        &mut self,
        _: &FormatItalic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_format(kunotes_core::format::italic, window, cx);
    }

    pub(super) fn format_link(
        &mut self,
        _: &FormatLink,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_format(kunotes_core::format::link, window, cx);
    }
}
