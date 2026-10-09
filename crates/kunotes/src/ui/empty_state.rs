//! Placeholder screens shown when no vault or no note is open.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, v_flex};
use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, div, px};

use crate::actions::OpenVault;

/// "No Vault Selected" with an Open Vault button.
pub fn no_vault(cx: &App) -> impl IntoElement {
    message(
        IconName::FolderPlus,
        "No Vault Selected",
        "Open a folder to use it as your vault.",
        cx,
    )
    .child(
        Button::new("open-vault")
            .primary()
            .label("Open Vault…")
            .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenVault), cx)),
    )
}

/// "No File Selected", shown while a vault is open but no note is.
pub fn no_file_selected(cx: &App) -> impl IntoElement {
    message(
        IconName::NotebookPen,
        "No File Selected",
        "Select a file from the sidebar to start writing.",
        cx,
    )
}

/// The shared layout: big muted icon, title, and a short hint, centered.
fn message(icon: IconName, title: &str, hint: &str, cx: &App) -> gpui_kit::Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            Icon::new(icon)
                .size(px(48.))
                .text_color(cx.theme().muted_foreground.opacity(0.5)),
        )
        .child(
            div()
                .text_lg()
                .text_color(cx.theme().muted_foreground)
                .child(title.to_string()),
        )
        .child(
            div()
                .max_w(px(260.))
                .text_sm()
                .text_center()
                .text_color(cx.theme().muted_foreground.opacity(0.8))
                .child(hint.to_string()),
        )
}
