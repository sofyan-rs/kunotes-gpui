//! The right-click menu of the text editors (Live and Source): clipboard,
//! formatting, select all.
//!
//! Items are actions, so the menu shows each one's shortcut and runs it on the
//! editor (through `action_context`), exactly like pressing the keys.

use gpui_kit::FocusHandle;
use gpui_kit::base::input::{Copy, Cut, Paste, SelectAll};
use gpui_kit::component::menu::PopupMenu;

use crate::actions::{FormatBold, FormatItalic, FormatLink};

/// `editor` is the focus handle the actions go to.
pub fn build(
    menu: PopupMenu,
    editor: FocusHandle,
    has_selection: bool,
    read_only: bool,
) -> PopupMenu {
    menu.action_context(editor)
        .menu_with_disabled("Cut", Box::new(Cut), !has_selection || read_only)
        .menu_with_disabled("Copy", Box::new(Copy), !has_selection)
        .menu_with_disabled("Paste", Box::new(Paste), read_only)
        .separator()
        .menu_with_disabled("Bold", Box::new(FormatBold), read_only)
        .menu_with_disabled("Italic", Box::new(FormatItalic), read_only)
        .menu_with_disabled("Link", Box::new(FormatLink), read_only)
        .separator()
        .menu("Select All", Box::new(SelectAll))
}
