//! The file tree: folders and notes of the vault.
//!
//! - Click selects; double-click (or the chevron) expands a folder.
//! - Arrow keys move and expand/collapse, Enter opens, F2 renames, Backspace/Delete trashes.
//! - Right-click shows a context menu (`context_menu.rs`); rows can be dragged onto folders.
//! - Renaming and new items are edited inline (`inline_edit.rs`).
//! - Keyboard navigation lives in `keyboard.rs`.
//!
//! It draws the flat list from `kunotes_core::search::visible_rows`, so only
//! expanded folders contribute rows.

use std::ops::Range;
use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, AppContext as _, ClickEvent, Context, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, UniformListScrollHandle,
    Window, div, px, uniform_list,
};
use kunotes_core::paths::can_move_into;
use kunotes_core::search::{VisibleRow, visible_rows};

use super::context_menu;
use super::inline_edit::InlineEdit;
use crate::actions::FILE_TREE;
use crate::vault_store::VaultStore;

const ROW_HEIGHT: f32 = 26.;
const ROW_INDENT: f32 = 14.;

/// What is carried while dragging a row.
#[derive(Clone)]
struct DraggedEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

/// The small chip that follows the mouse while dragging: icon + name.
struct DragPreview {
    name: String,
    is_dir: bool,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (icon, icon_color) = if self.is_dir {
            (IconName::Folder, theme.blue)
        } else {
            (IconName::FileText, theme.muted_foreground)
        };
        h_flex()
            .gap_1p5()
            .px_2()
            .py_1()
            .max_w(px(280.))
            .rounded_md()
            .text_sm()
            // Explicit colors: a drag preview doesn't inherit the tree's text color,
            // so without these the name is drawn dark-on-dark.
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .child(Icon::new(icon).small().text_color(icon_color))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.name.clone()),
            )
    }
}

/// Where the "new note/folder" name field is drawn.
#[derive(Clone, Copy)]
struct NewItemSlot {
    /// Position in the drawn list (the other rows shift down by one after it).
    index: usize,
    depth: usize,
    is_dir: bool,
}

pub struct FileTree {
    pub(super) vault: Entity<VaultStore>,
    pub(super) focus_handle: FocusHandle,
    pub(super) scroll_handle: UniformListScrollHandle,
    /// The rows drawn in the last render, used by keyboard navigation.
    pub(super) rows: Vec<VisibleRow>,
    /// The inline name field (rename or new item), if one is open.
    pub(super) editing: Option<InlineEdit>,
    new_item_slot: Option<NewItemSlot>,
}

impl FileTree {
    pub fn new(vault: Entity<VaultStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&vault, |_, _, cx| cx.notify()).detach();
        FileTree {
            vault,
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::new(),
            rows: Vec::new(),
            editing: None,
            new_item_slot: None,
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
            if event.click_count() == 2 {
                if row.is_dir {
                    vault.toggle_expanded(path, cx);
                } else {
                    // Double-clicking a note keeps it open in a permanent tab.
                    vault.keep_open(path, cx);
                }
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
        let slot = self.new_item_slot;
        range
            .filter_map(|index| match slot {
                Some(slot) if index == slot.index => Some(self.render_new_item_row(slot, cx)),
                _ => {
                    // Rows after the new-item field are shifted down by one.
                    let row_index = match slot {
                        Some(slot) if index > slot.index => index - 1,
                        _ => index,
                    };
                    let row = self.rows.get(row_index)?.clone();
                    let is_selected = selected.as_deref() == Some(row.path.as_path());
                    Some(self.render_row(row, is_selected, cx))
                }
            })
            .collect()
    }

    /// The row holding the name field for a new note or folder.
    fn render_new_item_row(&self, slot: NewItemSlot, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (icon, color) = if slot.is_dir {
            (IconName::Folder, theme.blue)
        } else {
            (IconName::FileText, theme.muted_foreground)
        };
        h_flex()
            .id("new-item-row")
            .w_full()
            .h(px(ROW_HEIGHT))
            .pl(px(6. + ROW_INDENT * slot.depth as f32))
            .pr_2()
            .gap_1()
            .text_sm()
            .child(div().flex_none().w(px(14.)))
            .child(
                div()
                    .flex_none()
                    .child(Icon::new(icon).small().text_color(color)),
            )
            .children(self.render_name_input(cx))
            .into_any_element()
    }

    /// Works out where the new-item field goes: the top of its folder's children.
    fn place_new_item(&mut self, root: Option<&Path>) {
        self.new_item_slot = self
            .editing
            .as_ref()
            .and_then(|edit| edit.new_item())
            .and_then(|(folder, is_dir)| {
                if Some(folder) == root {
                    return Some(NewItemSlot {
                        index: 0,
                        depth: 0,
                        is_dir,
                    });
                }
                let parent = self.rows.iter().position(|row| row.path == folder)?;
                Some(NewItemSlot {
                    index: parent + 1,
                    depth: self.rows[parent].depth + 1,
                    is_dir,
                })
            });
    }

    fn render_row(&self, row: VisibleRow, is_selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (icon, icon_color) = match (row.is_dir, row.is_expanded) {
            (true, true) => (IconName::FolderOpen, theme.blue),
            (true, false) => (IconName::Folder, theme.blue),
            // A locked note (encrypted, `.md.age`) gets a lock instead of a page.
            (false, _) if kunotes_core::lock::is_locked_note(&row.path) => {
                (IconName::Lock, theme.muted_foreground)
            }
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
        let renaming = self
            .editing
            .as_ref()
            .and_then(|edit| edit.renaming())
            .is_some_and(|path| path == row.path);
        // A long name shrinks and ends with "…" instead of pushing the icon aside.
        let plain_label = || {
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(display_name(&row))
                .into_any_element()
        };
        let label: AnyElement = if renaming {
            self.render_name_input(cx).unwrap_or_else(plain_label)
        } else {
            plain_label()
        };
        let drag = DraggedEntry {
            path: row.path.clone(),
            name: row.name.clone(),
            is_dir: row.is_dir,
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
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .text_sm()
                    // Files get an empty slot where folders have a chevron, so names line up.
                    // `flex_none`: the slot and icon keep their size however long the name is.
                    .child(div().flex_none().w(px(14.)).children(chevron))
                    .child(
                        div()
                            .id(icon_id(&row.path))
                            .test_support()
                            .flex_none()
                            .child(Icon::new(icon).small().text_color(icon_color)),
                    )
                    .child(label),
            )
            .on_click(cx.listener(move |this, event, window, cx| {
                this.on_row_click(&click_row, event, window, cx)
            }))
            .on_drag(drag, |entry, _, _, cx| {
                cx.new(|_| DragPreview {
                    name: entry.name.clone(),
                    is_dir: entry.is_dir,
                })
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

        let (menu_tree, menu_vault, menu_path) =
            (cx.entity(), self.vault.clone(), row.path.clone());
        let item = item.context_menu(move |menu, _, cx| {
            context_menu::build(
                menu,
                menu_tree.clone(),
                menu_vault.clone(),
                menu_path.clone(),
                is_dir,
                cx,
            )
        });

        // The wrapper gives the row a stable ID that UI tests can find and click.
        div()
            .id(row_id(&row.path))
            .test_support()
            // Clicks on a row are the row's: keep them from reaching the empty-space
            // layer behind the rows (its menu, and its "clear the selection").
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
        self.place_new_item(root.as_deref());
        let row_count = self.rows.len() + usize::from(self.new_item_slot.is_some());
        let highlight_root = root.clone();
        let menu_root = root.clone();
        let menu_tree = cx.entity();
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
            .relative()
            // Right-click on empty space: new note/folder in the vault root. The menu
            // sits on a layer behind the rows; rows stop the click from reaching it.
            .child(
                div().absolute().inset_0().child(
                    div()
                        .id("file-tree-background")
                        .test_support()
                        .size_full()
                        // A click on empty space clears the selection, like VS Code.
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|tree, _, window, cx| {
                                tree.vault
                                    .update(cx, |vault, cx| vault.clear_tree_selection(cx));
                                window.focus(&tree.focus_handle, cx);
                            }),
                        )
                        .context_menu(move |menu, _, cx| match &menu_root {
                            Some(root) => context_menu::build_for_root(
                                menu,
                                menu_tree.clone(),
                                root.clone(),
                                cx,
                            ),
                            None => menu,
                        }),
                ),
            )
            .child(
                // `uniform_list` only draws the rows on screen, so big vaults stay fast.
                uniform_list("file-tree", row_count, cx.processor(Self::render_rows))
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

/// ID of a row's icon, used by tests to check that rows line up.
pub fn icon_id(path: &Path) -> ElementId {
    ElementId::Name(format!("icon:{}", path.display()).into())
}

fn item_id(path: &Path) -> ElementId {
    ElementId::Name(format!("item:{}", path.display()).into())
}

/// The name shown for a row: locked notes hide the `.age` (the lock icon says it).
fn display_name(row: &VisibleRow) -> String {
    if kunotes_core::lock::is_locked_note(&row.path) {
        let name = &row.name;
        name[..name.len() - ".age".len()].to_string()
    } else {
        row.name.clone()
    }
}
