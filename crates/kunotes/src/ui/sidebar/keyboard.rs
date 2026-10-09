//! Keyboard navigation in the file tree: arrows move, expand, and collapse;
//! Enter opens a folder. (F2 and Backspace are handled by the workspace.)

use gpui_kit::{App, Context, ScrollStrategy, Window};

use super::file_tree::FileTree;
use crate::actions::{CollapseFolder, ExpandFolder, OpenSelected, SelectNext, SelectPrevious};

impl FileTree {
    fn selected_index(&self, cx: &App) -> Option<usize> {
        let selected = self.vault.read(cx).selected_path()?;
        self.rows.iter().position(|row| row.path == selected)
    }

    fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index).cloned() else {
            return;
        };
        self.vault
            .update(cx, |vault, cx| vault.select(row.path, row.is_dir, cx));
        self.scroll_handle
            .scroll_to_item(index, ScrollStrategy::Center);
    }

    pub(super) fn select_previous(
        &mut self,
        _: &SelectPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = match self.selected_index(cx) {
            Some(index) => index.saturating_sub(1),
            None => self.rows.len().saturating_sub(1),
        };
        self.select_index(index, cx);
    }

    pub(super) fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let index = match self.selected_index(cx) {
            Some(index) => (index + 1).min(self.rows.len().saturating_sub(1)),
            None => 0,
        };
        self.select_index(index, cx);
    }

    /// Right arrow: expand a collapsed folder, or move into an expanded one.
    pub(super) fn expand_folder(
        &mut self,
        _: &ExpandFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.selected_index(cx) else {
            return;
        };
        let row = self.rows[index].clone();
        if !row.is_dir {
            return;
        }
        if row.is_expanded {
            self.select_index(index + 1, cx);
        } else {
            self.vault
                .update(cx, |vault, cx| vault.set_expanded(row.path, true, cx));
        }
    }

    /// Left arrow: collapse an expanded folder, or jump to the parent folder.
    pub(super) fn collapse_folder(
        &mut self,
        _: &CollapseFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.selected_index(cx) else {
            return;
        };
        let row = self.rows[index].clone();
        if row.is_dir && row.is_expanded {
            self.vault
                .update(cx, |vault, cx| vault.set_expanded(row.path, false, cx));
        } else if let Some(parent) = row.path.parent()
            && let Some(parent_index) = self.rows.iter().position(|r| r.path == parent)
        {
            self.select_index(parent_index, cx);
        }
    }

    /// Enter: toggle a folder, or open a file (selecting it already opens it).
    pub(super) fn open_selected(
        &mut self,
        _: &OpenSelected,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.selected_index(cx) else {
            return;
        };
        let row = self.rows[index].clone();
        if row.is_dir {
            self.vault
                .update(cx, |vault, cx| vault.toggle_expanded(row.path, cx));
        }
    }
}
