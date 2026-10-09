//! The only place for OS-specific behavior and wording.
//! Everything else in the app stays platform-neutral.

/// Label for the "show this file in the system file manager" menu item.
#[allow(dead_code)] // used by the file tree context menu (Phase 3)
pub fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in Explorer"
    } else {
        "Open Containing Folder"
    }
}

/// Label for the delete confirmation button.
#[allow(dead_code)] // used by the delete dialog (Phase 3)
pub fn trash_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "Move to Recycle Bin"
    } else {
        "Move to Trash"
    }
}

/// macOS shows menus in the system menu bar; Windows and Linux need one inside the window.
pub fn uses_native_menu_bar() -> bool {
    cfg!(target_os = "macos")
}

/// The quit shortcut, if the app should bind one itself.
/// (Windows already closes windows with Alt+F4.)
pub fn quit_keybinding() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("cmd-q")
    } else if cfg!(target_os = "linux") {
        Some("ctrl-q")
    } else {
        None
    }
}
