//! The right-click menu of the Live editor: clipboard, formatting, select all.
//!
//! Items are actions, so the menu shows each one's shortcut and runs it on the
//! editor (through `action_context`), exactly like pressing the keys.

use gpui_kit::base::input::{Copy, Cut, Paste, SelectAll};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{Context, Entity};

use super::LiveEditor;
use crate::actions::{FormatBold, FormatItalic, FormatLink};

pub fn build(
    menu: PopupMenu,
    editor: &Entity<LiveEditor>,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let editor = editor.read(cx);
    let nothing_selected = editor.buffer.selection().is_empty();
    let read_only = editor.read_only;

    menu.action_context(editor.focus_handle.clone())
        .menu_with_disabled("Cut", Box::new(Cut), nothing_selected || read_only)
        .menu_with_disabled("Copy", Box::new(Copy), nothing_selected)
        .menu_with_disabled("Paste", Box::new(Paste), read_only)
        .separator()
        .menu_with_disabled("Bold", Box::new(FormatBold), read_only)
        .menu_with_disabled("Italic", Box::new(FormatItalic), read_only)
        .menu_with_disabled("Link", Box::new(FormatLink), read_only)
        .separator()
        .menu("Select All", Box::new(SelectAll))
}
