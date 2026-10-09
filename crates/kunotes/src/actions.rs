//! Every user command (action), its keyboard shortcut, and the app menus, in one place.
//!
//! An action is a small struct that GPUI sends to the focused view when a shortcut
//! is pressed or a menu item is clicked. Views handle them with `.on_action(..)`.

use gpui_kit::{App, KeyBinding, Menu, MenuItem, actions};

use crate::platform;

actions!(
    kunotes,
    [
        OpenVault,
        CloseVault,
        NewFile,
        NewFolder,
        ToggleSidebar,
        Quit,
    ]
);

/// Key context of the main window. Bindings with this context work anywhere in it.
pub const WORKSPACE: &str = "Workspace";

/// Registers all keyboard shortcuts. `secondary` means Cmd on macOS and Ctrl elsewhere.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-o", OpenVault, Some(WORKSPACE)),
        KeyBinding::new("secondary-n", NewFile, Some(WORKSPACE)),
        KeyBinding::new("secondary-shift-n", NewFolder, Some(WORKSPACE)),
        KeyBinding::new("secondary-\\", ToggleSidebar, Some(WORKSPACE)),
    ]);
    if let Some(keys) = platform::quit_keybinding() {
        cx.bind_keys([KeyBinding::new(keys, Quit, None)]);
    }
}

/// The app menus. On macOS these appear in the system menu bar; on Windows and
/// Linux the same list is drawn inside the title bar.
pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu::new("KuNotes").items([MenuItem::action("Quit KuNotes", Quit)]),
        Menu::new("File").items([
            MenuItem::action("Open Vault…", OpenVault),
            MenuItem::separator(),
            MenuItem::action("New Note", NewFile),
            MenuItem::action("New Folder", NewFolder),
            MenuItem::separator(),
            MenuItem::action("Close Vault", CloseVault),
        ]),
        Menu::new("View").items([MenuItem::action("Toggle Sidebar", ToggleSidebar)]),
    ]
}
