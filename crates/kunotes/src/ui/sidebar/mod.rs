//! The sidebar: a row of action buttons on top, the vault's file tree below,
//! and the git sync status at the bottom (`sync_footer.rs`).

mod context_menu;
mod file_tree;
#[cfg(test)]
pub use file_tree::row_id as row_id_for_tests;
#[cfg(test)]
mod file_tree_tests;
mod inline_edit;
mod keyboard;
mod sync_footer;

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Action, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div, prelude::FluentBuilder as _,
};

use crate::actions::{CollapseFolders, NewFile, NewFolder, OpenVault, QuickSwitcher, WORKSPACE};
use crate::git_sync::GitSync;
use crate::vault_store::VaultStore;
use file_tree::FileTree;

pub struct Sidebar {
    vault: Entity<VaultStore>,
    file_tree: Entity<FileTree>,
    git_sync: Entity<GitSync>,
}

impl Sidebar {
    pub fn new(
        vault: Entity<VaultStore>,
        git_sync: Entity<GitSync>,
        cx: &mut Context<Self>,
    ) -> Self {
        let file_tree = cx.new(|cx| FileTree::new(vault.clone(), cx));
        // Redraw when the vault changes (e.g. buttons become enabled).
        cx.observe(&vault, |_, _, cx| cx.notify()).detach();
        // Redraw the sync status when it changes.
        cx.observe(&git_sync, |_, _, cx| cx.notify()).detach();
        Sidebar {
            vault,
            file_tree,
            git_sync,
        }
    }

    /// Starts typing the name of a new note or folder in the tree, inside the
    /// selected folder (or the selected note's folder, or the vault root).
    pub fn start_new_item(&mut self, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(folder) = self.vault.read(cx).target_folder() else {
            return;
        };
        self.file_tree
            .update(cx, |tree, cx| tree.start_new(folder, is_dir, window, cx));
    }

    /// Starts renaming `path` inline in the tree.
    pub fn start_rename(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.file_tree
            .update(cx, |tree, cx| tree.start_rename(path, window, cx));
    }

    fn render_header(&self, has_vault: bool, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .px_2()
            .py_1()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().sidebar_border)
            .child(header_button(
                "open-vault",
                IconName::Folder,
                "Open Vault…",
                &OpenVault,
                true,
            ))
            .child(header_button(
                "new-file",
                IconName::SquarePen,
                "New Note",
                &NewFile,
                has_vault,
            ))
            .child(header_button(
                "new-folder",
                IconName::FolderPlus,
                "New Folder",
                &NewFolder,
                has_vault,
            ))
            .child(header_button(
                "collapse-folders",
                IconName::CopyMinus,
                "Collapse Folders",
                &CollapseFolders,
                has_vault,
            ))
            .child(div().flex_1())
            .child(header_button(
                "search",
                IconName::Search,
                "Search Notes",
                &QuickSwitcher,
                has_vault,
            ))
    }

    fn render_no_vault(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No Vault Open"),
            )
            .child(
                Button::new("sidebar-open-vault")
                    .small()
                    .label("Open Vault…")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenVault), cx)),
            )
    }
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_vault = self.vault.read(cx).root().is_some();
        v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(self.render_header(has_vault, cx))
            .child(if has_vault {
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.file_tree.clone())
                    .into_any_element()
            } else {
                self.render_no_vault(cx).into_any_element()
            })
            .when(has_vault, |sidebar| {
                sidebar.child(sync_footer::render(&self.git_sync, cx))
            })
    }
}

/// An icon-only button that dispatches `action`, with a tooltip showing its shortcut.
fn header_button(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    action: &dyn Action,
    enabled: bool,
) -> Button {
    let action = action.boxed_clone();
    Button::new(id)
        .ghost()
        .small()
        .icon(icon)
        .tooltip_with_action(tooltip, action.as_ref(), Some(WORKSPACE))
        .disabled(!enabled)
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}
