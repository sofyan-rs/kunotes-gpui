//! The "Change Notes Password" dialog: the current password, then the new one
//! twice. Only `.kunotes-lock.age` is rewritten; locked notes stay as they are.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestSupportExt as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use kunotes_core::lock::MIN_PASSWORD_LEN;

use crate::actions::{FORM, SubmitForm};
use crate::vault_lock::VaultLock;

/// Opens the dialog. If the vault has no password yet, says how to set one.
pub fn open(vault_lock: Entity<VaultLock>, window: &mut Window, cx: &mut App) {
    if !vault_lock.read(cx).has_password() {
        window.push_notification(
            "This vault has no notes password yet. Lock a note to set one.",
            cx,
        );
        return;
    }
    let form = cx.new(|cx| ChangePasswordForm::new(vault_lock, window, cx));
    let content = form.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .w(px(420.))
            .title("Change Notes Password")
            .child(content.clone())
    });
    form.update(cx, |form, cx| {
        form.current.update(cx, |input, cx| input.focus(window, cx))
    });
}

struct ChangePasswordForm {
    vault_lock: Entity<VaultLock>,
    current: Entity<InputState>,
    new: Entity<InputState>,
    /// The new password typed again.
    repeat: Entity<InputState>,
    error: Option<String>,
    busy: bool,
}

impl ChangePasswordForm {
    fn new(vault_lock: Entity<VaultLock>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut password_field = |placeholder: &'static str| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(placeholder)
            })
        };
        ChangePasswordForm {
            vault_lock,
            current: password_field("Current password"),
            new: password_field("New password"),
            repeat: password_field("Type the new one again"),
            error: None,
            busy: false,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let current = self.current.read(cx).value().to_string();
        let new = self.new.read(cx).value().to_string();
        let problem = if new.chars().count() < MIN_PASSWORD_LEN {
            Some(format!(
                "Use at least {MIN_PASSWORD_LEN} characters for the new password."
            ))
        } else if new != self.repeat.read(cx).value().as_ref() {
            Some("The new passwords don't match.".to_string())
        } else {
            None
        };
        if let Some(problem) = problem {
            self.error = Some(problem);
            cx.notify();
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let task = self
            .vault_lock
            .update(cx, |lock, cx| lock.change_password(current, new, cx));
        cx.spawn_in(window, async move |form, cx| {
            let result = task.await;
            let _ = form.update_in(cx, |form, window, cx| {
                form.busy = false;
                match result {
                    Ok(()) => {
                        window.close_dialog(cx);
                        window.push_notification("The notes password was changed.", cx);
                    }
                    Err(message) => {
                        form.error = Some(message);
                        form.current.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                            input.focus(window, cx);
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for ChangePasswordForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        v_flex()
            // Enter in any field submits (see `actions::FORM`).
            .key_context(FORM)
            .on_action(cx.listener(|form, _: &SubmitForm, window, cx| form.submit(window, cx)))
            .gap_3()
            .child(div().text_sm().text_color(muted).child(
                "Locked notes stay as they are. With git sync, other computers \
                 use the new password after their next sync.",
            ))
            // The ids let UI tests click a field to focus it.
            .child(
                div()
                    .id("current-password")
                    .test_support()
                    .child(Input::new(&self.current)),
            )
            .child(
                div()
                    .id("new-password")
                    .test_support()
                    .child(Input::new(&self.new)),
            )
            .child(
                div()
                    .id("repeat-password")
                    .test_support()
                    .child(Input::new(&self.repeat)),
            )
            .child(div().text_xs().text_color(danger).child(
                "If you forget the new password, locked notes can't be opened. \
                 Nobody can recover it.",
            ))
            .when_some(self.error.clone(), |form, error| {
                form.child(div().text_sm().text_color(danger).child(error))
            })
            .child(
                h_flex().justify_end().child(
                    Button::new("change-password-submit")
                        .primary()
                        .loading(self.busy)
                        .label("Change Password")
                        .on_click(cx.listener(|form, _, window, cx| form.submit(window, cx))),
                ),
            )
    }
}
