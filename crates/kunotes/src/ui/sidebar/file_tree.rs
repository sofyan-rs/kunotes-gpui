//! The file tree. For now it lists the vault's top-level items (read-only);
//! Phase 3 adds expanding, selection, the context menu, and drag and drop.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window, px, uniform_list,
};
use kunotes_core::search::{VisibleRow, visible_rows};

use crate::vault_store::VaultStore;

const ROW_INDENT: f32 = 16.;

pub struct FileTree {
    vault: Entity<VaultStore>,
}

impl FileTree {
    pub fn new(vault: Entity<VaultStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&vault, |_, _, cx| cx.notify()).detach();
        FileTree { vault }
    }
}

impl Render for FileTree {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let vault = self.vault.read(cx);
        let rows: Rc<Vec<VisibleRow>> = Rc::new(
            vault
                .tree()
                .map(|tree| visible_rows(tree, vault.expanded()))
                .unwrap_or_default(),
        );
        let folder_color = cx.theme().blue;
        let file_color = cx.theme().muted_foreground;

        // `uniform_list` only draws the rows that are on screen, so big vaults stay fast.
        uniform_list("file-tree", rows.len(), move |range, _, _| {
            range
                .map(|index| {
                    let row = &rows[index];
                    let (icon, color) = if row.is_dir {
                        (IconName::Folder, folder_color)
                    } else {
                        (IconName::FileText, file_color)
                    };
                    h_flex()
                        .h(px(26.))
                        .pl(px(8. + ROW_INDENT * row.depth as f32))
                        .gap_2()
                        .text_sm()
                        .child(Icon::new(icon).small().text_color(color))
                        .child(row.name.clone())
                })
                .collect()
        })
        .size_full()
        .py_1()
    }
}
