//! Every user command (action), its keyboard shortcut, and the app menus, in one place.
//!
//! An action is a small struct that GPUI sends to the focused view when a shortcut
//! is pressed or a menu item is clicked. Views handle them with `.on_action(..)`.

use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
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
        DeleteSelection,
        RenameSelection,
        ViewLive,
        ViewSource,
        ViewSplit,
        ViewPreview,
        CycleViewMode,
        QuickSwitcher,
        LockNotes,
        ChangeNotesPassword,
        CollapseFolders,
        About,
        CheckForUpdates,
        Quit,
    ]
);

/// Locks a note (encrypts it) or removes its lock. Sent by the file tree's menu.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = kunotes, no_json)]
pub struct ToggleNoteLock {
    pub path: std::path::PathBuf,
}

// Tabs. Handled by the workspace, so they work wherever the focus is.
actions!(
    tabs,
    [
        CloseTab,
        CloseOtherTabs,
        CloseTabsToTheRight,
        CloseSavedTabs,
        CloseAllTabs,
        TogglePinTab,
        NextTab,
        PreviousTab,
    ]
);

// Quick switcher list navigation. Only active while the switcher is open.
actions!(quick_switcher, [SwitcherUp, SwitcherDown, SwitcherOpen]);

// Enter in our dialogs' forms (password, git sync). gpui-kit's dialog handles
// Enter itself before a text field sees it, so forms bind it in their own context.
actions!(form, [SubmitForm]);

// Editor commands. Only active while the editor pane has focus.
actions!(editor, [SaveNow, FormatBold, FormatItalic, FormatLink]);

// File tree keyboard navigation. Only active while the tree has focus.
actions!(
    file_tree,
    [
        SelectPrevious,
        SelectNext,
        ExpandFolder,
        CollapseFolder,
        OpenSelected
    ]
);

/// Key context of the main window. Bindings with this context work anywhere in it.
pub const WORKSPACE: &str = "Workspace";

/// Key context of the editor pane (editor, formatter bar, preview).
pub const EDITOR: &str = "EditorPane";

/// Key context of the quick switcher dialog.
pub const QUICK_SWITCHER: &str = "QuickSwitcher";

/// Key context of a form inside a dialog (Enter submits it).
pub const FORM: &str = "Form";

/// Key context of the file tree. Bindings here only work while the tree has focus,
/// so e.g. Backspace deletes a file only when you're in the tree, never while typing.
pub const FILE_TREE: &str = "FileTree";

/// Registers all keyboard shortcuts. `secondary` means Cmd on macOS and Ctrl elsewhere.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-o", OpenVault, Some(WORKSPACE)),
        KeyBinding::new("secondary-n", NewFile, Some(WORKSPACE)),
        KeyBinding::new("secondary-shift-n", NewFolder, Some(WORKSPACE)),
        KeyBinding::new("secondary-\\", ToggleSidebar, Some(WORKSPACE)),
        KeyBinding::new("secondary-k", QuickSwitcher, Some(WORKSPACE)),
        KeyBinding::new("secondary-w", CloseTab, Some(WORKSPACE)),
        KeyBinding::new("secondary-alt-t", CloseOtherTabs, Some(WORKSPACE)),
        KeyBinding::new("secondary-shift-w", CloseAllTabs, Some(WORKSPACE)),
        KeyBinding::new("ctrl-tab", NextTab, Some(WORKSPACE)),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, Some(WORKSPACE)),
        KeyBinding::new("secondary-shift-o", QuickSwitcher, Some(WORKSPACE)),
        KeyBinding::new("secondary-shift-l", LockNotes, Some(WORKSPACE)),
        KeyBinding::new("secondary-1", ViewLive, Some(WORKSPACE)),
        KeyBinding::new("secondary-2", ViewSource, Some(WORKSPACE)),
        KeyBinding::new("secondary-3", ViewSplit, Some(WORKSPACE)),
        KeyBinding::new("secondary-4", ViewPreview, Some(WORKSPACE)),
        KeyBinding::new("secondary-e", CycleViewMode, Some(WORKSPACE)),
        KeyBinding::new("secondary-s", SaveNow, Some(EDITOR)),
        KeyBinding::new("secondary-b", FormatBold, Some(EDITOR)),
        KeyBinding::new("secondary-i", FormatItalic, Some(EDITOR)),
        KeyBinding::new("secondary-shift-k", FormatLink, Some(EDITOR)),
        KeyBinding::new("up", SwitcherUp, Some(QUICK_SWITCHER)),
        KeyBinding::new("down", SwitcherDown, Some(QUICK_SWITCHER)),
        // Caught here, before the dialog's own Enter (which would just close it).
        KeyBinding::new("enter", SwitcherOpen, Some(QUICK_SWITCHER)),
        KeyBinding::new("enter", SubmitForm, Some(FORM)),
        KeyBinding::new("up", SelectPrevious, Some(FILE_TREE)),
        KeyBinding::new("down", SelectNext, Some(FILE_TREE)),
        KeyBinding::new("right", ExpandFolder, Some(FILE_TREE)),
        KeyBinding::new("left", CollapseFolder, Some(FILE_TREE)),
        KeyBinding::new("enter", OpenSelected, Some(FILE_TREE)),
        KeyBinding::new("f2", RenameSelection, Some(FILE_TREE)),
        KeyBinding::new("backspace", DeleteSelection, Some(FILE_TREE)),
        KeyBinding::new("delete", DeleteSelection, Some(FILE_TREE)),
        KeyBinding::new("secondary-backspace", DeleteSelection, Some(FILE_TREE)),
    ]);
    if let Some(keys) = platform::quit_keybinding() {
        cx.bind_keys([KeyBinding::new(keys, Quit, None)]);
    }
}

/// The app menus. On macOS these appear in the system menu bar; on Windows and
/// Linux the same list is drawn inside the title bar.
pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu::new("KuNotes").items([
            MenuItem::action("About KuNotes", About),
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::action("Quit KuNotes", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Open Vault…", OpenVault),
            MenuItem::action("Quick Open…", QuickSwitcher),
            MenuItem::separator(),
            MenuItem::action("New Note", NewFile),
            MenuItem::action("New Folder", NewFolder),
            MenuItem::separator(),
            MenuItem::action("Rename…", RenameSelection),
            MenuItem::action("Move to Trash", DeleteSelection),
            MenuItem::separator(),
            MenuItem::action("Lock Notes Now", LockNotes),
            MenuItem::action("Change Notes Password…", ChangeNotesPassword),
            MenuItem::separator(),
            MenuItem::action("Close Tab", CloseTab),
            MenuItem::action("Close All Tabs", CloseAllTabs),
            MenuItem::separator(),
            MenuItem::action("Close Vault", CloseVault),
        ]),
        // These are the text editor's own actions; its shortcuts show next to them.
        Menu::new("Edit").items([
            MenuItem::action("Undo", Undo),
            MenuItem::action("Redo", Redo),
            MenuItem::separator(),
            MenuItem::action("Cut", Cut),
            MenuItem::action("Copy", Copy),
            MenuItem::action("Paste", Paste),
            MenuItem::action("Select All", SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::separator(),
            MenuItem::action("Live", ViewLive),
            MenuItem::action("Source", ViewSource),
            MenuItem::action("Split", ViewSplit),
            MenuItem::action("Preview", ViewPreview),
        ]),
    ]
}
