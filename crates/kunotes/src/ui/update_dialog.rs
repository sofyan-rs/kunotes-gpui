//! App update dialogs: "a new version is available" and "restart to finish".
//! The work itself happens in `updater.rs`.

use std::path::PathBuf;

use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{
    App, Entity, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, Window, div,
};
use kunotes_core::update::Release;

use crate::updater::Updater;

/// Offers to install `release`, with a link to its release notes.
pub fn offer_update(release: Release, updater: Entity<Updater>, window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let (release, updater) = (release.clone(), updater.clone());
        let page_url = release.page_url.clone();
        let muted = cx.theme().muted_foreground;
        let link_color = cx.theme().link;
        alert
            .confirm()
            .title(format!("KuNotes {} is available", release.version))
            .description(
                v_flex()
                    .gap_3()
                    .child(div().text_color(muted).child(format!(
                        "You have version {}. The update downloads from GitHub and \
                         replaces this copy of KuNotes.",
                        env!("CARGO_PKG_VERSION")
                    )))
                    .child(
                        div()
                            .id("update-release-notes")
                            .text_color(link_color)
                            .cursor_pointer()
                            .hover(|style| style.underline())
                            .child("What's new")
                            .on_click(move |_, _, cx| cx.open_url(&page_url)),
                    ),
            )
            .ok_text("Install Update")
            .cancel_text("Later")
            .on_ok(move |_, _, cx| {
                let release = release.clone();
                updater.update(cx, |updater, cx| updater.install(release, cx));
                true
            })
    });
}

/// The update is installed: restart now (open notes are saved on the way out)
/// or later (the new version starts next time).
pub fn offer_restart(version: String, path: PathBuf, window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, move |alert, _, _| {
        let path = path.clone();
        alert
            .confirm()
            .title(format!("KuNotes {version} is installed"))
            .description("Restart KuNotes to start using it. Your notes are saved first.")
            .ok_text("Restart Now")
            .cancel_text("Later")
            .on_ok(move |_, _, cx| {
                // GPUI quits (running the save-on-quit handlers), waits for
                // this process to end, then starts `path`.
                cx.set_restart_path(path.clone());
                cx.restart();
                true
            })
    });
}
