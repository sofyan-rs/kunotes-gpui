//! Dialogs: confirm moving a file or folder to the trash.
//! (Renaming happens inline in the file tree, see `sidebar/inline_edit.rs`.)

use std::path::{Path, PathBuf};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::{App, Entity, Window};

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
