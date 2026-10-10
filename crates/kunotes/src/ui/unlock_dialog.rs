//! The vault password dialog: unlocks locked notes, or sets the vault password
//! the first time a note is locked.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use kunotes_core::lock::MIN_PASSWORD_LEN;

use crate::actions::{FORM, SubmitForm};
use crate::vault_lock::VaultLock;

/// Runs after a successful unlock (e.g. "now lock this note").
pub type AfterUnlock = Box<dyn FnOnce(&mut Window, &mut App)>;

/// Opens the dialog. `then` runs once the vault is unlocked.
pub fn open(
    vault_lock: Entity<VaultLock>,
    then: Option<AfterUnlock>,
    window: &mut Window,
    cx: &mut App,
) {
    if vault_lock.read(cx).is_unlocked() {
        if let Some(then) = then {
            then(window, cx);
        }
        return;
    }
    let form = cx.new(|cx| UnlockForm::new(vault_lock, then, window, cx));
    let content = form.clone();
    let creating = form.read(cx).creating;
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .w(px(420.))
            .title(if creating {
                "Set a Vault Password"
            } else {
                "Unlock Notes"
            })
            .child(content.clone())
    });
    form.update(cx, |form, cx| {
        form.password
            .update(cx, |input, cx| input.focus(window, cx))
    });
}

struct UnlockForm {
    vault_lock: Entity<VaultLock>,
    /// True the first time: there's no vault password yet.
    creating: bool,
    password: Entity<InputState>,
    /// Typed again, when creating.
    repeat: Entity<InputState>,
    error: Option<String>,
    busy: bool,
    then: Option<AfterUnlock>,
}

impl UnlockForm {
    fn new(
        vault_lock: Entity<VaultLock>,
        then: Option<AfterUnlock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let creating = !vault_lock.read(cx).has_password();
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Vault password")
        });
        let repeat = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Type it again")
        });
        UnlockForm {
            vault_lock,
            creating,
            password,
            repeat,
            error: None,
            busy: false,
            then,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let password = self.password.read(cx).value().to_string();
        if self.creating {
            if password.chars().count() < MIN_PASSWORD_LEN {
                self.error = Some(format!("Use at least {MIN_PASSWORD_LEN} characters."));
                cx.notify();
                return;
            }
            if password != self.repeat.read(cx).value().as_ref() {
                self.error = Some("The passwords don't match.".into());
                cx.notify();
                return;
            }
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let task = self
            .vault_lock
            .update(cx, |lock, cx| lock.unlock(password, cx));
        cx.spawn_in(window, async move |form, cx| {
            let result = task.await;
            let _ = form.update_in(cx, |form, window, cx| {
                form.busy = false;
                match result {
                    Ok(()) => {
                        window.close_dialog(cx);
                        if let Some(then) = form.then.take() {
                            then(window, cx);
                        }
                    }
                    Err(message) => {
                        form.error = Some(message);
                        form.password.update(cx, |input, cx| {
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

impl Render for UnlockForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let intro = if self.creating {
            "Locked notes are encrypted with this password. It's asked once, then \
             notes stay unlocked until 5 minutes without use or until KuNotes quits."
        } else {
            "Enter the vault password to unlock locked notes."
        };
        v_flex()
            // Enter in either field submits (see `actions::FORM`).
            .key_context(FORM)
            .on_action(cx.listener(|form, _: &SubmitForm, window, cx| form.submit(window, cx)))
            .gap_3()
            .child(div().text_sm().text_color(muted).child(intro))
            .child(Input::new(&self.password))
            .when(self.creating, |form| {
                form.child(Input::new(&self.repeat)).child(
                    div().text_xs().text_color(danger).child(
                        "If you forget this password, locked notes can't be opened. \
                         Nobody can recover it.",
                    ),
                )
            })
            .when_some(self.error.clone(), |form, error| {
                form.child(div().text_sm().text_color(danger).child(error))
            })
            .child(
                h_flex().justify_end().child(
                    Button::new("unlock-submit")
                        .primary()
                        .loading(self.busy)
                        .label(if self.creating {
                            "Set Password"
                        } else {
                            "Unlock"
                        })
                        .on_click(cx.listener(|form, _, window, cx| form.submit(window, cx))),
                ),
            )
    }
}
