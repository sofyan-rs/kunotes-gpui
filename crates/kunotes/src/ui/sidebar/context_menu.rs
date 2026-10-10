//! The right-click menus of the file tree: one for a row, one for the empty
//! space below the rows (which makes things at the vault root).

use std::path::{Path, PathBuf};

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, ClipboardItem, Context, Entity, Window};
use kunotes_core::paths::relative_path;

use super::file_tree::FileTree;
use crate::actions::ToggleNoteLock;
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
                let p = p.clone();
                after_menu_closes(&t, window, cx, move |tree, window, cx| {
                    tree.start_new(p, false, window, cx)
                });
            }),
        );
        let (t, p) = (tree.clone(), path.clone());
        menu = menu
            .item(
                PopupMenuItem::new("New Folder").on_click(move |_, window, cx| {
                    let p = p.clone();
                    after_menu_closes(&t, window, cx, move |tree, window, cx| {
                        tree.start_new(p, true, window, cx)
                    });
                }),
            )
            .separator();
    }

    if !is_dir {
        // Lock (encrypt) or unlock the note; the workspace handles it.
        let locked = kunotes_core::lock::is_locked_note(&path);
        let (t, p) = (tree.clone(), path.clone());
        menu = menu
            .item(
                PopupMenuItem::new(if locked { "Remove Lock" } else { "Lock Note" }).on_click(
                    move |_, window, cx| {
                        let action = ToggleNoteLock { path: p.clone() };
                        after_menu_closes(&t, window, cx, move |_, window, cx| {
                            window.dispatch_action(Box::new(action), cx);
                        });
                    },
                ),
            )
            .separator();
    }

    let (t, p) = (tree.clone(), path.clone());
    menu = menu.item(PopupMenuItem::new("Rename").on_click(move |_, window, cx| {
        let p = p.clone();
        after_menu_closes(&t, window, cx, move |tree, window, cx| {
            tree.start_rename(p, window, cx)
        });
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

/// Builds the menu for empty space in the tree: new items go in the vault root.
pub fn build_for_root(
    menu: PopupMenu,
    tree: Entity<FileTree>,
    root: PathBuf,
    _cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let (t, r) = (tree.clone(), root.clone());
    let menu = menu.item(
        PopupMenuItem::new("New Note").on_click(move |_, window, cx| {
            let r = r.clone();
            after_menu_closes(&t, window, cx, move |tree, window, cx| {
                tree.start_new(r, false, window, cx)
            });
        }),
    );
    let (t, r) = (tree, root.clone());
    menu.item(
        PopupMenuItem::new("New Folder").on_click(move |_, window, cx| {
            let r = r.clone();
            after_menu_closes(&t, window, cx, move |tree, window, cx| {
                tree.start_new(r, true, window, cx)
            });
        }),
    )
    .separator()
    .item(
        PopupMenuItem::new(platform::reveal_label()).on_click(move |_, _, _| {
            if let Err(error) = opener::reveal(&root) {
                log::warn!("couldn't reveal {}: {error}", root.display());
            }
        }),
    )
}

/// Runs `f` on the tree once the menu has closed. The menu gives focus back
/// to where it was when it closes, which would take it away from the name
/// field `f` opens.
fn after_menu_closes(
    tree: &Entity<FileTree>,
    window: &mut Window,
    cx: &mut App,
    f: impl FnOnce(&mut FileTree, &mut Window, &mut Context<FileTree>) + 'static,
) {
    let tree = tree.clone();
    window.defer(cx, move |window, cx| {
        tree.update(cx, |tree, cx| f(tree, window, cx))
    });
}
