//! Drawing `EditorPane`: the header (breadcrumb, lock button, view-mode
//! switch), the editor for the current mode, the formatter bar and status bar.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Editor;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, TestSupportExt as _, Window, div, prelude::FluentBuilder as _, px,
};
use kunotes_core::settings::ViewMode;
use kunotes_core::{cursor, paths};

use super::locked::Secret;
use super::{EditorPane, edit_menu, formatter_bar, preview, status_bar, view_mode_switch};
use crate::actions::{EDITOR, LockNotes, WORKSPACE};
use crate::settings_store::SettingsStore;

impl EditorPane {
    fn render_header(&self, mode: ViewMode, cx: &mut Context<Self>) -> impl IntoElement {
        let parts = paths::breadcrumb(&self.vault_root, &self.path);
        let last = parts.len().saturating_sub(1);
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;

        let mut breadcrumb = h_flex().gap_1().text_sm().overflow_hidden();
        for (index, part) in parts.into_iter().enumerate() {
            if index > 0 {
                breadcrumb =
                    breadcrumb.child(Icon::new(IconName::ChevronRight).xsmall().text_color(muted));
            }
            let color = if index == last { foreground } else { muted };
            breadcrumb = breadcrumb.child(div().text_color(color).child(part));
        }
        // An unlocked locked note: a button to lock notes again right away.
        if self.secret == Secret::Shown {
            breadcrumb = breadcrumb.child(
                Button::new("lock-now")
                    .ghost()
                    .xsmall()
                    .icon(IconName::LockOpen)
                    .tooltip_with_action("Lock Notes Now", &LockNotes, Some(WORKSPACE))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(LockNotes), cx)),
            );
        }

        h_flex()
            .justify_between()
            // The switch gets the same space on its right as above and below it.
            .pl_3()
            .pr_1p5()
            .py_1p5()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(breadcrumb)
            .child(view_mode_switch::render(mode, cx))
    }

    /// The Source/Split editor: monospace, like a code editor.
    fn render_source_editor(&self) -> AnyElement {
        let read_only = !self.can_save;
        let editor = Editor::new(&self.editor)
            .bordered(false)
            .readonly(!self.can_save)
            .size_full()
            // The editor adds 12px of its own on the left, so this lines the
            // text up with Live and Preview (24px from the edge in every mode).
            .pl_3()
            .pr_6()
            .py_4()
            .text_size(px(15.));
        let menu_editor = self.editor.clone();
        div()
            .id("source-editor")
            .test_support() // lets UI tests find and click it
            .size_full()
            .child(editor)
            // The same right-click menu as Live mode, instead of gpui-kit's
            // native one (which looks different and offers "Go to Definition").
            .context_menu(move |menu, _, cx| {
                let editor = menu_editor.read(cx);
                edit_menu::build(
                    menu,
                    editor.focus_handle(cx),
                    !editor.selected_range().is_empty(),
                    read_only,
                )
            })
            .into_any_element()
    }
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = SettingsStore::get(cx).view_mode;
        let (line, column) = cursor::line_col(&self.text(cx), self.cursor(cx));

        if self.secret == Secret::Hidden {
            return v_flex()
                .id("editor-pane")
                .key_context(EDITOR)
                .size_full()
                .bg(cx.theme().background)
                .child(self.render_header(mode, cx))
                .child(div().flex_1().min_h_0().child(self.render_locked(cx)));
        }
        let body: AnyElement = match mode {
            ViewMode::Preview => {
                preview::render(&self.preview, self.note_dir(), cx.weak_entity()).into_any_element()
            }
            ViewMode::Split => h_resizable("editor-split")
                .child(resizable_panel().child(self.render_source_editor()))
                .child(resizable_panel().child(preview::render(
                    &self.preview,
                    self.note_dir(),
                    cx.weak_entity(),
                )))
                .into_any_element(),
            ViewMode::Live => self.live.clone().into_any_element(),
            ViewMode::Source => self.render_source_editor(),
        };

        v_flex()
            .id("editor-pane")
            .key_context(EDITOR)
            .on_action(cx.listener(Self::save_action))
            .on_action(cx.listener(Self::format_bold))
            .on_action(cx.listener(Self::format_italic))
            .on_action(cx.listener(Self::format_link))
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_header(mode, cx))
            .when(mode != ViewMode::Preview, |pane| {
                pane.child(formatter_bar::render(cx.entity(), cx))
            })
            .child(div().flex_1().min_h_0().child(body))
            .child(status_bar::render(line, column, self.char_count, cx))
    }
}
