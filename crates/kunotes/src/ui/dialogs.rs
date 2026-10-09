//! Small dialogs: rename a file or folder, and confirm moving it to the trash.

use std::path::{Path, PathBuf};

use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{WindowExt as _, v_flex};
use gpui_kit::{App, AppContext as _, Entity, ParentElement as _, Styled as _, Window};
use kunotes_core::paths::note_title;

use crate::platform;
use crate::vault_store::VaultStore;

/// Asks for a new name, then renames. On an invalid name the dialog stays open
/// and the reason is shown as a notification.
pub fn rename(vault: Entity<VaultStore>, path: PathBuf, window: &mut Window, cx: &mut App) {
    // Files are shown without ".md"; it is added back automatically.
    let current_name = note_title(&path);
    let input = cx.new(|cx| InputState::new(window, cx).default_value(current_name));

    let field = input.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (vault, path, field) = (vault.clone(), path.clone(), field.clone());
        alert
            .confirm()
            .title("Rename")
            .ok_text("Rename")
            .child(v_flex().pt_2().child(Input::new(&field)))
            .on_ok(move |_, window, cx| {
                let new_name = field.read(cx).value().to_string();
                match vault.update(cx, |vault, cx| vault.rename(&path, &new_name, cx)) {
                    Ok(()) => true,
                    Err(message) => {
                        window.push_notification((NotificationType::Error, message), cx);
                        false // keep the dialog open so the user can fix the name
                    }
                }
            })
    });

    // Focus the field with the whole name selected, ready to type over.
    input.update(cx, |state, cx| {
        state.focus(window, cx);
        state.select_all(window, cx);
    });
}

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
