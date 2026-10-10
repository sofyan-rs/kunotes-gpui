//! The Git Sync dialog: enter a repository address to start syncing the vault,
//! or turn sync off.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{App, AppContext as _, Entity, ParentElement as _, Styled as _, Window, div, px};

use crate::git_sync::GitSync;

/// Opens the dialog for the vault `git_sync` belongs to.
pub fn open(git_sync: Entity<GitSync>, window: &mut Window, cx: &mut App) {
    let current = git_sync.read(cx).remote().map(str::to_string);
    let is_on = current.is_some();
    let url = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("git@github.com:you/notes.git  or  https://github.com/you/notes.git")
            .default_value(current.unwrap_or_default())
    });
    // Enter in the field connects, like the button.
    let (enter_sync, enter_url) = (git_sync.clone(), url.clone());
    window
        .subscribe(&url, cx, move |_, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                connect(&enter_sync, &enter_url, window, cx);
            }
        })
        .detach();

    let content_url = url.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let muted = cx.theme().muted_foreground;
        let (connect_sync, connect_url) = (git_sync.clone(), content_url.clone());
        let off_sync = git_sync.clone();
        let mut buttons = h_flex().gap_2().justify_end();
        if is_on {
            buttons = buttons.child(Button::new("git-sync-off").label("Turn Off").on_click(
                move |_, window, cx| {
                    off_sync.update(cx, |sync, cx| sync.disconnect(cx));
                    window.close_dialog(cx);
                },
            ));
        }
        buttons = buttons.child(
            Button::new("git-sync-connect")
                .primary()
                .label(if is_on { "Save" } else { "Turn On" })
                .on_click(move |_, window, cx| connect(&connect_sync, &connect_url, window, cx)),
        );

        dialog.w(px(480.)).title("Git Sync").child(
            v_flex()
                .gap_3()
                .child(div().text_sm().text_color(muted).child(
                    "Keep this vault in a git repository (like a GitHub repo). \
                             KuNotes commits and syncs your notes automatically.",
                ))
                .child(Input::new(&content_url))
                .child(div().text_xs().text_color(muted).child(
                    "SSH address (git@github.com:…) if you use an SSH key; HTTPS address \
                     if you're signed in with the GitHub CLI (`gh auth login`). \
                     KuNotes uses the git installed on this computer.",
                ))
                .child(buttons),
        )
    });
    url.update(cx, |url, cx| url.focus(window, cx));
}

fn connect(
    git_sync: &Entity<GitSync>,
    url: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) {
    let address = url.read(cx).value().trim().to_string();
    if address.is_empty() {
        return;
    }
    git_sync.update(cx, |sync, cx| sync.connect(address, cx));
    window.close_dialog(cx);
}
