//! The row of formatting buttons above the editor (bold, headings, link, lists, ...).
//! Each button runs a pure transform from `kunotes_core::format`.

use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{App, Div, Entity, IntoElement, ParentElement as _, Styled as _, div, px};
use kunotes_core::format::{self, Edit};

use super::EditorPane;

/// A formatting transform: (text, selection) -> edit.
type Transform = fn(&str, Range<usize>) -> Edit;

fn heading_1(text: &str, selection: Range<usize>) -> Edit {
    format::heading(text, selection, 1)
}

fn heading_2(text: &str, selection: Range<usize>) -> Edit {
    format::heading(text, selection, 2)
}

/// Buttons in groups; a divider is drawn between groups.
const GROUPS: &[&[(&str, IconName, &str, Transform)]] = &[
    &[
        ("format-bold", IconName::Bold, "Bold", format::bold),
        ("format-italic", IconName::Italic, "Italic", format::italic),
    ],
    &[
        ("format-h1", IconName::Heading1, "Heading 1", heading_1),
        ("format-h2", IconName::Heading2, "Heading 2", heading_2),
    ],
    &[
        ("format-link", IconName::Link, "Link", format::link),
        (
            "format-code",
            IconName::Code,
            "Inline Code",
            format::inline_code,
        ),
        (
            "format-code-block",
            IconName::SquareCode,
            "Code Block",
            format::code_block,
        ),
        ("format-quote", IconName::Quote, "Quote", format::quote),
    ],
    &[
        (
            "format-bullets",
            IconName::List,
            "Bullet List",
            format::bullet_list,
        ),
        (
            "format-numbers",
            IconName::ListOrdered,
            "Numbered List",
            format::numbered_list,
        ),
    ],
    &[(
        "format-rule",
        IconName::Minus,
        "Horizontal Rule",
        format::horizontal_rule,
    )],
];

pub fn render(pane: Entity<EditorPane>, cx: &App) -> impl IntoElement {
    let mut bar: Div = h_flex()
        .px_3()
        .py_1()
        .gap_1()
        .border_b_1()
        .border_color(cx.theme().border);

    for (group_index, group) in GROUPS.iter().enumerate() {
        if group_index > 0 {
            bar = bar.child(div().w(px(1.)).h(px(16.)).mx_1().bg(cx.theme().border));
        }
        for &(id, icon, tooltip, transform) in group.iter() {
            let pane = pane.clone();
            bar = bar.child(
                Button::new(id)
                    .ghost()
                    .small()
                    .icon(icon)
                    .tooltip(tooltip)
                    .on_click(move |_, window, cx| {
                        pane.update(cx, |pane, cx| pane.apply_format(transform, window, cx));
                    }),
            );
        }
    }
    bar
}
