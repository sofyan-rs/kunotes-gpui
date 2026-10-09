//! The rendered markdown preview (read-only, selectable, links open in the browser).
//!
//! Task lists are drawn by us instead of gpui-kit, so their checkboxes can be
//! clicked: a click tells the editor pane the task's position in the note,
//! and the pane flips `[ ]` ↔ `[x]` in the text.
//!
//! Right-click shows Copy and Select All.

use gpui_kit::assets::IconName;
use gpui_kit::base::markdown_ast::Node;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, TextView, TextViewState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, CursorStyle, Entity, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, SharedString, Styled as _, TestSupportExt as _,
    WeakEntity, Window, div, prelude::*, px,
};

use super::EditorPane;

/// Name of our custom block for task lists.
const TASK_LIST: &str = "kunotes-task-list";

pub fn render(state: &Entity<TextViewState>, pane: WeakEntity<EditorPane>) -> impl IntoElement {
    let menu_state = state.clone();
    div()
        .id("preview")
        .test_support() // lets UI tests find it
        .size_full()
        .child(
            TextView::new(state)
                .selectable(true)
                .scrollable(true)
                .on_link_click(|url, _, _, cx| cx.open_url(url))
                .markdown_block_parser(parse_task_list)
                // The renderer runs on every frame; gpui-kit only re-parses when the
                // text changes, so passing a fresh closure each time is fine.
                .markdown_block_renderer(TASK_LIST, move |node, window, cx| {
                    render_task_list(node, &pane, window, cx)
                })
                .size_full()
                .px_6()
                .py_4(),
        )
        .context_menu(move |menu, _, cx| context_menu(menu, &menu_state, cx))
}

/// The right-click menu: Copy (the selected text) and Select All.
fn context_menu(
    menu: PopupMenu,
    state: &Entity<TextViewState>,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let selected = state.read(cx).selected_text();
    let copy = PopupMenuItem::new("Copy")
        .disabled(selected.is_empty())
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(selected.clone()));
        });
    let state = state.clone();
    let select_all = PopupMenuItem::new("Select All").on_click(move |_, window, cx| {
        let focus = state.read(cx).focus_handle().clone();
        window.focus(&focus, cx); // so Cmd/Ctrl+C copies it afterwards
        state.update(cx, |state, cx| state.select_all(cx));
    });
    menu.item(copy).item(select_all)
}

/// A list that contains at least one task (`- [ ]`).
struct TaskList {
    ordered: bool,
    start: u32,
    items: Vec<TaskItem>,
}

struct TaskItem {
    /// Where the item starts in the note (sent with `ToggleTask`).
    offset: usize,
    /// `None` for a plain item in a list that also has tasks.
    checked: Option<bool>,
    /// The item's text as markdown, drawn with a small nested preview.
    content: SharedString,
}

/// Turns a markdown list that has tasks into our `TaskList` block.
fn parse_task_list(node: &Node, cx: &MarkdownParseContext) -> Option<MarkdownNode> {
    let Node::List(list) = node else {
        return None;
    };
    let has_tasks = list
        .children
        .iter()
        .any(|child| matches!(child, Node::ListItem(item) if item.checked.is_some()));
    if !has_tasks {
        return None;
    }
    let items = list
        .children
        .iter()
        .filter_map(|child| {
            let Node::ListItem(item) = child else {
                return None;
            };
            let start = item.position.as_ref()?.start.offset;
            let content = match (item.children.first(), item.children.last()) {
                (Some(first), Some(last)) => {
                    let range = first.position()?.start.offset..last.position()?.end.offset;
                    cx.source().get(range).unwrap_or_default()
                }
                _ => "",
            };
            Some(TaskItem {
                offset: cx.offset() + start,
                checked: item.checked,
                content: content.to_string().into(),
            })
        })
        .collect();
    let source = cx.node_source(node).unwrap_or_default().to_string();
    let task_list = TaskList {
        ordered: list.ordered,
        start: list.start.unwrap_or(1),
        items,
    };
    Some(
        MarkdownNode::new(TASK_LIST, task_list)
            .text(source.clone())
            .markdown(source),
    )
}

fn render_task_list(
    node: &MarkdownNode,
    pane: &WeakEntity<EditorPane>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Some(list) = node.data::<TaskList>() else {
        return div().into_any_element();
    };
    let line_height = window.line_height();
    let foreground = cx.theme().foreground;
    let background = cx.theme().background;

    let rows = list.items.iter().enumerate().map(|(index, item)| {
        let marker = match item.checked {
            Some(checked) => checkbox(item.offset, checked, pane.clone(), foreground, background)
                .into_any_element(),
            None if list.ordered => div()
                .child(format!("{}.", list.start as usize + index))
                .into_any_element(),
            None => div().child("•").into_any_element(),
        };
        h_flex()
            .items_start()
            .gap_1p5()
            .child(
                h_flex()
                    .flex_none()
                    .h(line_height)
                    .items_center()
                    .child(marker),
            )
            .child(
                div().flex_1().min_w_0().child(
                    TextView::markdown(("task", item.offset), item.content.clone())
                        .selectable(true)
                        .on_link_click(|url, _, _, cx| cx.open_url(url)),
                ),
            )
    });
    v_flex().children(rows).into_any_element()
}

/// A checkbox like gpui-kit's preview draws, but clickable.
fn checkbox(
    offset: usize,
    checked: bool,
    pane: WeakEntity<EditorPane>,
    foreground: gpui_kit::Hsla,
    background: gpui_kit::Hsla,
) -> impl IntoElement {
    div()
        .id(("task-checkbox", offset))
        .test_support() // lets UI tests click it
        .flex()
        .size(px(14.))
        .items_center()
        .justify_center()
        .border_1()
        .border_color(foreground)
        .cursor(CursorStyle::PointingHand)
        .when(checked, |this| {
            this.bg(foreground)
                .child(Icon::new(IconName::Check).xsmall().text_color(background))
        })
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation(); // don't start a text selection
            // `update` fails only if the pane was closed; then there's nothing to do.
            let _ = pane.update(cx, |pane, cx| pane.toggle_task(offset, window, cx));
        })
}
