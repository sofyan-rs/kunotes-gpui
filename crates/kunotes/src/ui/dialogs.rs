//! Dialogs: confirm moving a file or folder to the trash, and About KuNotes.
//! (Renaming happens inline in the file tree, see `sidebar/inline_edit.rs`.)

use std::path::{Path, PathBuf};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    App, Entity, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div,
};

use crate::platform;
use crate::vault_store::VaultStore;

/// Asks for confirmation, then moves the item to the OS trash.
pub fn confirm_trash(vault: Entity<VaultStore>, path: PathBuf, window: &mut Window, cx: &mut App) {
    let title = format!("Delete “{}”?", display_name(&path));
    let description = if path.is_dir() {
        "The folder and everything in it will be moved to the trash."
    } else {
        "The note will be moved to the trash."
    };

    window.open_alert_dialog(cx, move |alert, _, _| {
        let (vault, path) = (vault.clone(), path.clone());
        alert
            .confirm()
            .title(title.clone())
            .description(description)
            .ok_text(platform::trash_label())
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, _, cx| {
                vault.update(cx, |vault, cx| vault.trash(&path, cx));
                true
            })
    });
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Where the code lives (shown, and opened, by the About box).
const REPO_URL: &str = "https://github.com/sofyan-rs/kunotes-gpui";

/// The About box: name, version, what it is, and a link to the code.
pub fn about(window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, |alert, _, cx| {
        let link_color = cx.theme().link;
        let muted = cx.theme().muted_foreground;
        alert
            .title(format!("KuNotes {}", env!("CARGO_PKG_VERSION")))
            .description(
                v_flex()
                    .gap_3()
                    .child(div().text_color(muted).child(
                        "A minimal markdown vault: open a folder, browse it, edit notes. \
                         Files on disk are the only source of truth.",
                    ))
                    .child(
                        div()
                            .id("about-repo-link")
                            .test_support() // lets UI tests find it
                            .text_color(link_color)
                            .cursor_pointer()
                            .hover(|style| style.underline())
                            .child(REPO_URL.trim_start_matches("https://"))
                            .on_click(|_, _, cx| cx.open_url(REPO_URL)),
                    ),
            )
            .ok_text("OK")
    });
}
