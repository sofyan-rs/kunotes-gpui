//! Inline editing in the file tree, like VS Code or Zed: renaming a row in place,
//! or typing the name of a new note/folder in a row at the top of its folder.
//!
//! Enter confirms, Escape cancels, clicking elsewhere confirms (or cancels if the
//! field is empty). An invalid or taken name keeps the field open and shows why.

use std::path::{Path, PathBuf};

use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{Sizable as _, WindowExt as _};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    Subscription, TestSupportExt as _, Window, div,
};
use kunotes_core::paths::rename_selection;

use super::file_tree::FileTree;

/// What the name field is for.
pub enum EditKind {
    Rename { path: PathBuf },
    NewNote { folder: PathBuf },
    NewFolder { folder: PathBuf },
}

/// The name field currently shown in the tree.
pub struct InlineEdit {
    pub kind: EditKind,
    pub input: Entity<InputState>,
    /// Listen for Enter and focus loss; dropped together with the field.
    _subscriptions: [Subscription; 2],
}

impl InlineEdit {
    /// For a new item: the folder it goes in, and whether it's a folder.
    pub fn new_item(&self) -> Option<(&Path, bool)> {
        match &self.kind {
            EditKind::NewNote { folder } => Some((folder, false)),
            EditKind::NewFolder { folder } => Some((folder, true)),
            EditKind::Rename { .. } => None,
        }
    }

    /// For a rename: the row being renamed.
    pub fn renaming(&self) -> Option<&Path> {
        match &self.kind {
            EditKind::Rename { path } => Some(path),
            _ => None,
        }
    }
}

impl FileTree {
    /// Turns a row's name into an editable field, with the name (minus `.md`) selected.
    pub fn start_rename(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let selection = rename_selection(&name, path.is_dir());
        self.begin_edit(EditKind::Rename { path }, name, selection, window, cx);
    }

    /// Shows an empty name field at the top of `folder` for a new note or folder.
    pub fn start_new(
        &mut self,
        folder: PathBuf,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Make sure the folder is open, so the new row is visible.
        self.vault
            .update(cx, |vault, cx| vault.expand_folder_and_parents(&folder, cx));
        let kind = if is_dir {
            EditKind::NewFolder { folder }
        } else {
            EditKind::NewNote { folder }
        };
        self.begin_edit(kind, String::new(), 0..0, window, cx);
    }

    fn begin_edit(
        &mut self,
        kind: EditKind,
        text: String,
        selection: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(text));
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.set_selected_range(selection, cx);
        });

        let on_enter = cx.subscribe_in(&input, window, |tree, input, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                tree.finish_edit(input, true, window, cx);
            }
        });
        // `on_focus_out` (rather than the input's own Blur event) also fires when
        // focus moves to an element that contains the field, like the tree itself.
        let focus = input.focus_handle(cx);
        let field = input.clone();
        let on_focus_lost = cx.on_focus_out(&focus, window, move |tree, _, window, cx| {
            tree.finish_edit(&field, false, window, cx);
        });

        self.editing = Some(InlineEdit {
            kind,
            input,
            _subscriptions: [on_enter, on_focus_lost],
        });
        cx.notify();
    }

    /// Applies the typed name. `pressed_enter` is false when focus moved away.
    fn finish_edit(
        &mut self,
        input: &Entity<InputState>,
        pressed_enter: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Ignore events from a field that's already gone (e.g. its blur after Enter).
        let Some(edit) = self.editing.as_ref().filter(|edit| &edit.input == input) else {
            return;
        };
        let name = input.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.cancel_edit(window, cx);
            return;
        }

        let result = match &edit.kind {
            EditKind::Rename { path } => {
                let path = path.clone();
                self.vault
                    .update(cx, |vault, cx| vault.rename(&path, &name, cx))
            }
            EditKind::NewNote { folder } => {
                let folder = folder.clone();
                self.vault
                    .update(cx, |vault, cx| vault.create_note(&folder, &name, cx))
            }
            EditKind::NewFolder { folder } => {
                let folder = folder.clone();
                self.vault
                    .update(cx, |vault, cx| vault.create_folder(&folder, &name, cx))
            }
        };

        match result {
            Ok(()) => self.close_edit(window, cx),
            Err(message) => {
                window.push_notification((NotificationType::Error, message), cx);
                // After Enter, keep the field so the name can be fixed. When focus
                // moved elsewhere, the user has moved on: drop the field.
                if !pressed_enter {
                    self.close_edit(window, cx);
                }
            }
        }
    }

    pub fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_edit(window, cx);
    }

    fn close_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Clear first, so the field's own blur event (from the focus change) is ignored.
        let was_editing = self.editing.take().is_some();
        if was_editing
            && window
                .focused(cx)
                .is_none_or(|focus| focus != self.focus_handle)
        {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    /// The name field, sized to sit inside a tree row.
    pub fn render_name_input(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.editing.as_ref()?;
        Some(
            div()
                .id("tree-name-input")
                .test_support()
                .flex_1()
                .min_w_0()
                // Clicks inside the field must not reach the row (which would take
                // focus away and confirm the edit).
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                // The field passes Escape on when it has nothing else to do with it.
                .on_action(cx.listener(|tree, _: &Escape, window, cx| tree.cancel_edit(window, cx)))
                .child(Input::new(&edit.input).xsmall())
                .into_any_element(),
        )
    }
}
