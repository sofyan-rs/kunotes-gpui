//! The row of tabs above the editor.
//!
//! - Click activates, double-click keeps a preview tab, middle-click closes.
//! - Drag a tab onto another to reorder (pinned and unpinned tabs stay separate).
//! - Right-click: Close, Close Others, Close to the Right, Close Saved, Close All,
//!   Pin/Unpin, Copy Path, Copy Relative Path, Reveal in Finder/Explorer.
//! - A dot replaces the close button while a note has unsaved changes; pinned
//!   tabs show a pin instead.

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, AppContext as _, ClipboardItem, Context, ElementId, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use kunotes_core::paths::{note_title, relative_path};

use super::EditorArea;
use crate::actions::{
    CloseAllTabs, CloseOtherTabs, CloseSavedTabs, CloseTab, CloseTabsToTheRight, TogglePinTab,
};
use crate::platform;

const TAB_HEIGHT: f32 = 34.;

/// What is carried while dragging a tab.
#[derive(Clone)]
struct DraggedTab {
    index: usize,
    title: String,
}

/// The chip that follows the mouse while dragging a tab.
struct TabDragPreview(String);

impl Render for TabDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .gap_1p5()
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .child(
                Icon::new(IconName::FileText)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(self.0.clone())
    }
}

/// ID of a tab, used by UI tests: `tab:<path>`.
pub fn tab_id(path: &std::path::Path) -> ElementId {
    ElementId::Name(format!("tab:{}", path.display()).into())
}

pub fn render(area: &EditorArea, cx: &mut Context<EditorArea>) -> impl IntoElement {
    let tabs: Vec<AnyElement> = area
        .tabs
        .tabs()
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let is_active = area.tabs.active_index() == Some(index);
            let is_dirty = area.is_dirty(&tab.path, cx);
            render_tab(
                index,
                tab.path.clone(),
                tab.pinned,
                tab.preview,
                is_active,
                is_dirty,
                area.root.clone(),
                cx,
            )
        })
        .collect();

    let theme = cx.theme();
    h_flex()
        .id("tab-bar")
        .w_full()
        .h(px(TAB_HEIGHT))
        .flex_none()
        .bg(theme.muted)
        .border_b_1()
        .border_color(theme.border)
        .overflow_x_scroll()
        .children(tabs)
}

#[allow(clippy::too_many_arguments)] // one tab's state, passed explicitly for clarity
fn render_tab(
    index: usize,
    path: PathBuf,
    pinned: bool,
    preview: bool,
    is_active: bool,
    is_dirty: bool,
    root: Option<PathBuf>,
    cx: &mut Context<EditorArea>,
) -> AnyElement {
    let theme = cx.theme();
    let title = note_title(&path);
    let group: SharedString = format!("tab-{index}").into();
    let text_color = if is_active {
        theme.foreground
    } else {
        theme.muted_foreground
    };

    // Right side of the tab: pin (pinned), dot (unsaved), or a close button
    // that appears on hover and on the active tab.
    let trailing: AnyElement = if pinned {
        div()
            .id("tab-pin")
            .flex_none()
            .child(
                Icon::new(IconName::Pin)
                    .xsmall()
                    .text_color(theme.muted_foreground),
            )
            .on_click(cx.listener(move |area, _, window, cx| {
                cx.stop_propagation();
                area.tabs.activate(index);
                area.toggle_pin(window, cx);
            }))
            .into_any_element()
    } else {
        let close_hover = theme.secondary_hover;
        let close = div()
            .id("tab-close")
            .flex_none()
            .size(px(16.))
            .rounded_sm()
            .flex()
            .items_center()
            .justify_center()
            .hover(move |style| style.bg(close_hover))
            .on_click(cx.listener(move |area, _, window, cx| {
                cx.stop_propagation();
                area.close(index, window, cx);
            }));
        if is_dirty {
            // Unsaved: a dot, which turns into the close button on hover.
            close
                .child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .bg(theme.muted_foreground)
                        .group_hover(group.clone(), |style| style.invisible()),
                )
                .into_any_element()
        } else {
            close
                .child(
                    Icon::new(IconName::Close)
                        .xsmall()
                        .text_color(theme.muted_foreground),
                )
                .when(!is_active, |close| {
                    close
                        .invisible()
                        .group_hover(group.clone(), |style| style.visible())
                })
                .into_any_element()
        }
    };

    let drag = DraggedTab {
        index,
        title: title.clone(),
    };
    let drop_border = theme.primary;
    let active_bg = theme.background;
    let hover_bg = theme.secondary_hover;
    let menu_path = path.clone();

    h_flex()
        .id(tab_id(&path))
        .test_support()
        .group(group)
        .h_full()
        .flex_none()
        .min_w(px(90.))
        .max_w(px(220.))
        .pl_3()
        .pr_2()
        .gap_1p5()
        .text_sm()
        .text_color(text_color)
        .border_r_1()
        .border_color(theme.border)
        .cursor_pointer()
        .when(is_active, |tab| tab.bg(active_bg))
        .when(!is_active, |tab| tab.hover(move |style| style.bg(hover_bg)))
        .child(
            // A locked note's tab shows a lock, like its row in the tree.
            Icon::new(if kunotes_core::lock::is_locked_note(&path) {
                IconName::Lock
            } else {
                IconName::FileText
            })
            .xsmall()
            .text_color(theme.muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(preview, |label| label.italic())
                .child(title.clone()),
        )
        .child(trailing)
        .on_click(
            cx.listener(move |area, event: &gpui_kit::ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    area.make_permanent(index, window, cx);
                } else {
                    area.activate(index, window, cx);
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Middle,
            cx.listener(move |area, _, window, cx| area.close(index, window, cx)),
        )
        // Right-click selects the tab first, so the menu's commands apply to it.
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |area, _, window, cx| area.activate(index, window, cx)),
        )
        .on_drag(drag, |dragged, _, _, cx| {
            cx.new(|_| TabDragPreview(dragged.title.clone()))
        })
        .drag_over::<DraggedTab>(move |style, _, _, _| style.border_l_2().border_color(drop_border))
        .on_drop(cx.listener(move |area, dragged: &DraggedTab, window, cx| {
            area.move_tab(dragged.index, index, window, cx);
        }))
        .context_menu(move |menu, _, _| {
            let path = menu_path.clone();
            let relative = root
                .as_deref()
                .and_then(|root| relative_path(root, &path))
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let absolute = path.to_string_lossy().into_owned();
            let reveal_path = path.clone();
            menu.menu("Close", Box::new(CloseTab))
                .menu("Close Others", Box::new(CloseOtherTabs))
                .menu("Close to the Right", Box::new(CloseTabsToTheRight))
                .menu("Close Saved", Box::new(CloseSavedTabs))
                .menu("Close All", Box::new(CloseAllTabs))
                .separator()
                .menu(if pinned { "Unpin" } else { "Pin" }, Box::new(TogglePinTab))
                .separator()
                .item(PopupMenuItem::new("Copy Path").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(absolute.clone()));
                }))
                .item(
                    PopupMenuItem::new("Copy Relative Path").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(relative.clone()));
                    }),
                )
                .item(
                    PopupMenuItem::new(platform::reveal_label()).on_click(move |_, _, _| {
                        if let Err(error) = opener::reveal(&reveal_path) {
                            log::warn!("couldn't reveal {}: {error}", reveal_path.display());
                        }
                    }),
                )
        })
        .into_any_element()
}
