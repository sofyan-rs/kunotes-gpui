//! The file tree: folders and notes of the vault.
//!
//! - Click selects; double-click (or the chevron) expands a folder.
//! - Arrow keys move and expand/collapse, Enter opens, F2 renames, Backspace/Delete trashes.
//! - Right-click shows a context menu; rows can be dragged onto folders.
//!
//! It draws the flat list from `kunotes_core::search::visible_rows`, so only
//! expanded folders contribute rows.

use std::ops::Range;
use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, ClipboardItem, Context, ElementId, Entity,
    FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollStrategy,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, UniformListScrollHandle,
    Window, div, px, uniform_list,
};
use kunotes_core::paths::{can_move_into, relative_path};
use kunotes_core::search::{VisibleRow, visible_rows};

use crate::actions::{
    CollapseFolder, ExpandFolder, FILE_TREE, OpenSelected, SelectNext, SelectPrevious,
};
use crate::platform;
use crate::ui::dialogs;
use crate::vault_store::VaultStore;

const ROW_HEIGHT: f32 = 26.;
const ROW_INDENT: f32 = 14.;

/// What is carried while dragging a row.
#[derive(Clone)]
struct DraggedEntry {
    path: PathBuf,
    name: String,
}

/// The small label that follows the mouse while dragging.
struct DragPreview(String);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .child(self.0.clone())
    }
}

pub struct FileTree {
    vault: Entity<VaultStore>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    /// The rows drawn in the last render, used by keyboard navigation.
    rows: Vec<VisibleRow>,
}

impl FileTree {
    pub fn new(vault: Entity<VaultStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&vault, |_, _, cx| cx.notify()).detach();
        FileTree {
            vault,
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::new(),
            rows: Vec::new(),
        }
    }

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

    // ----- Keyboard actions -----

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        let index = match self.selected_index(cx) {
            Some(index) => index.saturating_sub(1),
            None => self.rows.len().saturating_sub(1),
        };
        self.select_index(index, cx);
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let index = match self.selected_index(cx) {
            Some(index) => (index + 1).min(self.rows.len().saturating_sub(1)),
            None => 0,
        };
        self.select_index(index, cx);
    }

    /// Right arrow: expand a collapsed folder, or move into an expanded one.
    fn expand_folder(&mut self, _: &ExpandFolder, _: &mut Window, cx: &mut Context<Self>) {
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
    fn collapse_folder(&mut self, _: &CollapseFolder, _: &mut Window, cx: &mut Context<Self>) {
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
    fn open_selected(&mut self, _: &OpenSelected, _: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index(cx) else {
            return;
        };
        let row = self.rows[index].clone();
        if row.is_dir {
            self.vault
                .update(cx, |vault, cx| vault.toggle_expanded(row.path, cx));
        }
    }

    // ----- Mouse -----

    fn on_row_click(
        &mut self,
        row: &VisibleRow,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        let path = row.path.clone();
        self.vault.update(cx, |vault, cx| {
            vault.select(path.clone(), row.is_dir, cx);
            if row.is_dir && event.click_count() == 2 {
                vault.toggle_expanded(path, cx);
            }
        });
    }

    /// Dropping onto a folder row moves the dragged item into that folder.
    fn on_drop_into(&mut self, entry: &DraggedEntry, folder: &Path, cx: &mut Context<Self>) {
        let source = entry.path.clone();
        self.vault
            .update(cx, |vault, cx| vault.move_into(&source, folder, cx));
    }

    // ----- Drawing -----

    fn render_rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let selected = self.vault.read(cx).selected_path().map(Path::to_path_buf);
        range
            .filter_map(|index| self.rows.get(index).cloned())
            .map(|row| {
                let is_selected = selected.as_deref() == Some(row.path.as_path());
                self.render_row(row, is_selected, cx)
            })
            .collect()
    }

    fn render_row(&self, row: VisibleRow, is_selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (icon, icon_color) = match (row.is_dir, row.is_expanded) {
            (true, true) => (IconName::FolderOpen, theme.blue),
            (true, false) => (IconName::Folder, theme.blue),
            (false, _) => (IconName::FileText, theme.muted_foreground),
        };
        let chevron = row.is_dir.then(|| {
            let folder = row.path.clone();
            let vault = self.vault.clone();
            div()
                .id("chevron")
                .child(
                    Icon::new(if row.is_expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .xsmall()
                    .text_color(theme.muted_foreground),
                )
                .on_click(move |_, _, cx| {
                    // Don't let the row's own click handler run as well.
                    cx.stop_propagation();
                    vault.update(cx, |vault, cx| vault.toggle_expanded(folder.clone(), cx));
                })
        });

        let drop_color = theme.drop_target;
        let label = row.name.clone();
        let drag = DraggedEntry {
            path: row.path.clone(),
            name: row.name.clone(),
        };
        let click_row = row.clone();
        let drop_folder = row.path.clone();
        let is_dir = row.is_dir;

        let item = ListItem::new(item_id(&row.path))
            .selected(is_selected)
            .h(px(ROW_HEIGHT))
            .pl(px(6. + ROW_INDENT * row.depth as f32))
            .child(
                h_flex()
                    .gap_1()
                    .text_sm()
                    // Files get an empty slot where folders have a chevron, so names line up.
                    .child(div().w(px(14.)).children(chevron))
                    .child(Icon::new(icon).small().text_color(icon_color))
                    .child(label),
            )
            .on_click(cx.listener(move |this, event, window, cx| {
                this.on_row_click(&click_row, event, window, cx)
            }))
            .on_drag(drag, |entry, _, _, cx| {
                cx.new(|_| DragPreview(entry.name.clone()))
            });

        // Every row catches drops, so a refused drop never falls through to the
        // tree behind it (which would move the item to the vault root).
        // Folders highlight and accept only moves that make sense; notes ignore drops.
        let highlight_folder = drop_folder.clone();
        let item = item
            .drag_over::<DraggedEntry>(move |style, entry, _, _| {
                if is_dir && can_move_into(&entry.path, &highlight_folder) {
                    style.bg(drop_color)
                } else {
                    style
                }
            })
            .on_drop(cx.listener(move |this, entry: &DraggedEntry, _, cx| {
                cx.stop_propagation();
                if is_dir && can_move_into(&entry.path, &drop_folder) {
                    this.on_drop_into(entry, &drop_folder, cx);
                }
            }));

        let menu_vault = self.vault.clone();
        let menu_path = row.path.clone();
        let item = item.context_menu(move |menu, _, cx| {
            context_menu(menu, menu_vault.clone(), menu_path.clone(), is_dir, cx)
        });

        // The wrapper gives the row a stable ID that UI tests can find and click.
        div()
            .id(row_id(&row.path))
            .test_support()
            .child(item)
            .into_any_element()
    }
}

impl Render for FileTree {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let vault = self.vault.read(cx);
        self.rows = vault
            .tree()
            .map(|tree| visible_rows(tree, vault.expanded()))
            .unwrap_or_default();
        let root = vault.root().map(Path::to_path_buf);
        let highlight_root = root.clone();
        let drop_color = cx.theme().drop_target;

        div()
            .id("file-tree-container")
            // Must come before `track_focus` so tests can send keys to the tree.
            .test_support()
            .key_context(FILE_TREE)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::expand_folder))
            .on_action(cx.listener(Self::collapse_folder))
            .on_action(cx.listener(Self::open_selected))
            .size_full()
            // Dropping on empty space below the rows moves the item to the vault root.
            .drag_over::<DraggedEntry>(move |style, entry, _, _| match &highlight_root {
                Some(root) if can_move_into(&entry.path, root) => style.bg(drop_color.opacity(0.3)),
                _ => style,
            })
            .on_drop(cx.listener(move |this, entry: &DraggedEntry, _, cx| {
                if let Some(root) = &root
                    && can_move_into(&entry.path, root)
                {
                    this.on_drop_into(entry, root, cx);
                }
            }))
            .child(
                // `uniform_list` only draws the rows on screen, so big vaults stay fast.
                uniform_list(
                    "file-tree",
                    self.rows.len(),
                    cx.processor(Self::render_rows),
                )
                .track_scroll(&self.scroll_handle)
                .size_full()
                .py_1(),
            )
    }
}

/// ID of a row's outer wrapper, e.g. `row:/vault/Projects`.
pub fn row_id(path: &Path) -> ElementId {
    ElementId::Name(format!("row:{}", path.display()).into())
}

fn item_id(path: &Path) -> ElementId {
    ElementId::Name(format!("item:{}", path.display()).into())
}

/// The right-click menu for one row.
fn context_menu(
    menu: PopupMenu,
    vault: Entity<VaultStore>,
    path: PathBuf,
    is_dir: bool,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let root = vault.read(cx).root().map(Path::to_path_buf);
    let mut menu = menu;

    if is_dir {
        let (v, p) = (vault.clone(), path.clone());
        menu = menu.item(PopupMenuItem::new("New Note").on_click(move |_, _, cx| {
            v.update(cx, |vault, cx| vault.create_file(Some(p.clone()), cx));
        }));
        let (v, p) = (vault.clone(), path.clone());
        menu = menu
            .item(PopupMenuItem::new("New Folder").on_click(move |_, _, cx| {
                v.update(cx, |vault, cx| vault.create_folder(Some(p.clone()), cx));
            }))
            .separator();
    }

    let (v, p) = (vault.clone(), path.clone());
    menu = menu.item(
        PopupMenuItem::new("Rename…").on_click(move |_, window, cx| {
            dialogs::rename(v.clone(), p.clone(), window, cx);
        }),
    );
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
