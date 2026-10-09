//! The row of formatting buttons above the editor (bold, headings, link, lists, ...).
//! Each button runs a pure transform from `kunotes_core::format`, except Image,
//! which asks for files first (`EditorPane::insert_images`).

use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{App, Div, Entity, IntoElement, ParentElement as _, Styled as _, div, px};
use kunotes_core::format::{self, Edit};

use super::EditorPane;

/// A formatting transform: (text, selection) -> edit.
type Transform = fn(&str, Range<usize>) -> Edit;

/// What a button does.
#[derive(Clone, Copy)]
enum Command {
    /// Runs a transform on the selection.
    Format(Transform),
    /// Asks for image files, copies them next to the note, and links them.
    InsertImage,
}

use Command::{Format, InsertImage};

fn heading_1(text: &str, selection: Range<usize>) -> Edit {
    format::heading(text, selection, 1)
}

fn heading_2(text: &str, selection: Range<usize>) -> Edit {
    format::heading(text, selection, 2)
}

fn heading_3(text: &str, selection: Range<usize>) -> Edit {
    format::heading(text, selection, 3)
}

/// Buttons in groups; a divider is drawn between groups.
/// Each button: (element id, icon, tooltip, command).
const GROUPS: &[&[(&str, IconName, &str, Command)]] = &[
    &[
        ("format-bold", IconName::Bold, "Bold", Format(format::bold)),
        (
            "format-italic",
            IconName::Italic,
            "Italic",
            Format(format::italic),
        ),
        (
            "format-strike",
            IconName::Strikethrough,
            "Strikethrough",
            Format(format::strikethrough),
        ),
    ],
    &[
        (
            "format-h1",
            IconName::Heading1,
            "Heading 1",
            Format(heading_1),
        ),
        (
            "format-h2",
            IconName::Heading2,
            "Heading 2",
            Format(heading_2),
        ),
        (
            "format-h3",
            IconName::Heading3,
            "Heading 3",
            Format(heading_3),
        ),
    ],
    &[
        ("format-link", IconName::Link, "Link", Format(format::link)),
        ("format-image", IconName::ImagePlus, "Image", InsertImage),
        (
            "format-code",
            IconName::Code,
            "Inline Code",
            Format(format::inline_code),
        ),
        (
            "format-code-block",
            IconName::SquareCode,
            "Code Block",
            Format(format::code_block),
        ),
        (
            "format-quote",
            IconName::Quote,
            "Quote",
            Format(format::quote),
        ),
    ],
    &[
        (
            "format-bullets",
            IconName::List,
            "Bullet List",
            Format(format::bullet_list),
        ),
        (
            "format-numbers",
            IconName::ListOrdered,
            "Numbered List",
            Format(format::numbered_list),
        ),
        (
            "format-tasks",
            IconName::ListTodo,
            "Task List",
            Format(format::task_list),
        ),
    ],
    &[
        (
            "format-table",
            IconName::Table,
            "Table",
            Format(format::table),
        ),
        (
            "format-rule",
            IconName::Minus,
            "Horizontal Rule",
            Format(format::horizontal_rule),
        ),
    ],
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
        for &(id, icon, tooltip, command) in group.iter() {
            let pane = pane.clone();
            bar = bar.child(
                Button::new(id)
                    .ghost()
                    .small()
                    .icon(icon)
                    .tooltip(tooltip)
                    .on_click(move |_, window, cx| {
                        pane.update(cx, |pane, cx| match command {
                            Format(transform) => pane.apply_format(transform, window, cx),
                            InsertImage => pane.insert_images(window, cx),
                        });
                    }),
            );
        }
    }
    bar
}
