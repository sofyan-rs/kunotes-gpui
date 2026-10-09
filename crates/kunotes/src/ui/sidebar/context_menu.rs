//! The right-click menu of a file tree row.

use std::path::{Path, PathBuf};

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{ClipboardItem, Context, Entity};
use kunotes_core::paths::relative_path;

use super::file_tree::FileTree;
use crate::platform;
use crate::ui::dialogs;
use crate::vault_store::VaultStore;

/// Builds the menu for the row at `path`.
pub fn build(
    menu: PopupMenu,
    tree: Entity<FileTree>,
    vault: Entity<VaultStore>,
    path: PathBuf,
    is_dir: bool,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let root = vault.read(cx).root().map(Path::to_path_buf);
    let mut menu = menu;

    if is_dir {
        let (t, p) = (tree.clone(), path.clone());
        menu = menu.item(
            PopupMenuItem::new("New Note").on_click(move |_, window, cx| {
                t.update(cx, |tree, cx| tree.start_new(p.clone(), false, window, cx));
            }),
        );
        let (t, p) = (tree.clone(), path.clone());
        menu = menu
            .item(
                PopupMenuItem::new("New Folder").on_click(move |_, window, cx| {
                    t.update(cx, |tree, cx| tree.start_new(p.clone(), true, window, cx));
                }),
            )
            .separator();
    }

    let (t, p) = (tree.clone(), path.clone());
    menu = menu.item(PopupMenuItem::new("Rename").on_click(move |_, window, cx| {
        t.update(cx, |tree, cx| tree.start_rename(p.clone(), window, cx));
    }));
    let (v, p) = (vault.clone(), path.clone());
    menu = menu
        .item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
            dialogs::confirm_trash(v.clone(), p.clone(), window, cx);
        }))
        .separator();

    let p = path.clone();
    menu = menu.item(
        PopupMenuItem::new(platform::reveal_label()).on_click(move |_, _, _| {
            if let Err(error) = opener::reveal(&p) {
                log::warn!("couldn't reveal {}: {error}", p.display());
            }
        }),
    );
    let absolute = path.to_string_lossy().into_owned();
    menu = menu.item(PopupMenuItem::new("Copy Path").on_click(move |_, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(absolute.clone()));
    }));
    let relative = root
        .as_deref()
        .and_then(|root| relative_path(root, &path))
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    menu.item(
        PopupMenuItem::new("Copy Relative Path").on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(relative.clone()));
        }),
    )
}
