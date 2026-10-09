//! The bottom line of the sidebar: git sync status. Click it to sync now;
//! the gear opens the Git Sync dialog. Without sync, it offers to set it up.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div,
};

use crate::git_sync::{GitSync, SyncStatus};
use crate::ui::git_sync_dialog;

pub fn render(git_sync: &Entity<GitSync>, cx: &App) -> impl IntoElement {
    let status = git_sync.read(cx).status().clone();
    let theme = cx.theme();
    let row = h_flex()
        .px_2()
        .py_1()
        .gap_1()
        .border_t_1()
        .border_color(theme.sidebar_border)
        .text_xs()
        .text_color(theme.muted_foreground);

    let dialog_sync = git_sync.clone();
    let open_dialog = move |_: &_, window: &mut _, cx: &mut App| {
        git_sync_dialog::open(dialog_sync.clone(), window, cx);
    };

    let (icon, text, color) = match &status {
        SyncStatus::Syncing => (
            IconName::RefreshCw,
            "Syncing…".to_string(),
            theme.muted_foreground,
        ),
        SyncStatus::Failed(_) => (
            IconName::CloudOff,
            "Sync failed · click to retry".to_string(),
            theme.danger,
        ),
        SyncStatus::Idle { last } => (
            IconName::CloudCheck,
            synced_text(*last),
            theme.muted_foreground,
        ),
        SyncStatus::Off => {
            return row
                .child(
                    Button::new("git-sync-setup")
                        .ghost()
                        .xsmall()
                        .icon(IconName::GitBranch)
                        .label("Set Up Git Sync…")
                        .on_click(open_dialog),
                )
                .into_any_element();
        }
    };
    let tooltip = match &status {
        SyncStatus::Failed(message) => message.clone(),
        _ => "Sync now".to_string(),
    };
    let sync = git_sync.clone();
    row.child(
        h_flex()
            .id("git-sync-status")
            .flex_1()
            .min_w_0()
            .gap_1p5()
            .px_1()
            .py_0p5()
            .rounded(theme.radius)
            .cursor_pointer()
            .hover(|style| style.bg(theme.sidebar_accent))
            .text_color(color)
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .on_click(move |_, _, cx| sync.update(cx, |sync, cx| sync.sync_now(cx)))
            .child(Icon::new(icon).xsmall().text_color(color))
            .child(div().truncate().child(text)),
    )
    .child(
        Button::new("git-sync-settings")
            .ghost()
            .xsmall()
            .icon(IconName::Settings)
            .tooltip("Git Sync…")
            .on_click(open_dialog),
    )
    .into_any_element()
}

/// "Synced just now", "Synced 5 min ago", ...
fn synced_text(last: Option<std::time::Instant>) -> String {
    let Some(last) = last else {
        return "Not synced yet".into();
    };
    match last.elapsed().as_secs() / 60 {
        0 => "Synced just now".into(),
        minutes if minutes < 60 => format!("Synced {minutes} min ago"),
        minutes => format!("Synced {} h ago", minutes / 60),
    }
}
