//! The bar under the editor: cursor position on the left, character count on the right.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, div, px};

pub fn render(line: usize, column: usize, chars: usize, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    StatusBar::new()
        .left(
            div()
                .text_size(px(11.))
                .text_color(muted)
                .child(format!("Ln {line}, Col {column}")),
        )
        .right(
            div()
                .text_size(px(11.))
                .text_color(muted)
                .child(format!("{chars} characters")),
        )
}
