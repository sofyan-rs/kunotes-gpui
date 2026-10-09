//! The sidebar: a row of action buttons on top, the vault's file tree below.

mod file_tree;
#[cfg(test)]
mod file_tree_tests;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Action, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div,
};

use crate::actions::{DeleteSelection, NewFile, NewFolder, OpenVault, QuickSwitcher, WORKSPACE};
use crate::vault_store::VaultStore;
use file_tree::FileTree;

pub struct Sidebar {
    vault: Entity<VaultStore>,
    file_tree: Entity<FileTree>,
}

impl Sidebar {
    pub fn new(vault: Entity<VaultStore>, cx: &mut Context<Self>) -> Self {
        let file_tree = cx.new(|cx| FileTree::new(vault.clone(), cx));
        // Redraw when the vault changes (e.g. buttons become enabled).
        cx.observe(&vault, |_, _, cx| cx.notify()).detach();
        Sidebar { vault, file_tree }
    }

    fn render_header(&self, has_vault: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selection = self.vault.read(cx).selected_path().is_some();
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
                "delete",
                IconName::Trash,
                "Move to Trash",
                &DeleteSelection,
                has_selection,
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
                    .child(self.file_tree.clone())
                    .into_any_element()
            } else {
                self.render_no_vault(cx).into_any_element()
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
