//! The quick switcher (`secondary-k`): a searchable list of every note in the vault.
//! Type to filter by name, use the arrow keys to choose, Enter to open, Escape to close.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex};
use gpui_kit::{App, AppContext as _, Entity, ParentElement as _, Styled as _, Window, div, px};
use kunotes_core::paths::{note_title, relative_path};
use kunotes_core::search::flatten_files;

use crate::vault_store::VaultStore;

/// One note in the list.
#[derive(Clone)]
struct Note {
    path: PathBuf,
    title: String,
    /// Parent folder name, shown on the right (empty for notes in the vault root).
    folder: String,
    /// Path inside the vault, also searchable (e.g. "Projects/Plan.md").
    relative: String,
}

/// Opens the quick switcher. Does nothing if no vault is open.
pub fn open(vault: Entity<VaultStore>, window: &mut Window, cx: &mut App) {
    let notes: Rc<Vec<Note>> = {
        let store = vault.read(cx);
        let (Some(root), Some(tree)) = (store.root(), store.tree()) else {
            return;
        };
        Rc::new(
            flatten_files(tree)
                .into_iter()
                .map(|node| Note {
                    title: note_title(&node.path),
                    folder: node
                        .path
                        .parent()
                        .filter(|parent| *parent != root)
                        .map(note_title)
                        .unwrap_or_default(),
                    relative: relative_path(root, &node.path).unwrap_or_default(),
                    path: node.path.clone(),
                })
                .collect(),
        )
    };

    let state = cx.new(|cx| CommandState::new(window, cx));
    let palette_state = state.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let (notes, vault) = (notes.clone(), vault.clone());
        // With the dialog's padding removed, the palette reaches the dialog's edge,
        // so it needs the same rounded corners or its square corners poke out.
        let radius = cx.theme().radius_lg;
        dialog.close_button(false).p_0().w(px(480.)).child(
            Command::new(&palette_state)
                .bordered(false)
                .rounded(radius)
                .overflow_hidden()
                .placeholder("Search notes…")
                .max_h(px(360.))
                // The items keep their order; Command filters them by label and keywords.
                .items(notes.iter().cloned().map(note_item))
                .empty(|_, _, cx| {
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No matches")
                })
                .footer(|_, _, cx| {
                    h_flex()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child("↑↓ move")
                        .child("↵ open")
                        .child("esc close")
                })
                // `index.row` is the position in `notes` (before filtering).
                .on_confirm(move |index, window, cx| {
                    if let Some(note) = notes.get(index.row) {
                        let path = note.path.clone();
                        vault.update(cx, |vault, cx| vault.open_note(path, cx));
                    }
                    window.close_dialog(cx);
                }),
        )
    });

    // Put the cursor in the search field.
    state.update(cx, |state, cx| state.focus(window, cx));
}

/// A list row: note title on the left, folder name on the right.
fn note_item(note: Note) -> CommandItem {
    let (title, folder) = (note.title.clone(), note.folder.to_uppercase());
    CommandItem::new()
        .label(note.title)
        .keywords([note.relative])
        .child(move |_, cx| {
            h_flex()
                .w_full()
                .justify_between()
                .gap_3()
                // A long title shrinks and ends with "…"; the folder label keeps its size.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(folder.clone()),
                )
        })
}
