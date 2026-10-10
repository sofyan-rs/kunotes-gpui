//! UI tests for locked notes: locking an open note, saving it encrypted,
//! locking the vault (the text goes away) and unlocking it again.

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use kunotes_core::lock;

use crate::actions::ToggleNoteLock;
use crate::ui::editor::EditorPane;
use crate::ui::test_helpers::{Setup, in_window, setup};
use crate::vault_lock::VaultLock;

fn vault_lock(s: &Setup, cx: &mut TestAppContext) -> Entity<VaultLock> {
    s.workspace
        .read_with(cx, |workspace, _| workspace.vault_lock())
}

fn pane(s: &Setup, cx: &mut TestAppContext) -> Entity<EditorPane> {
    s.workspace.read_with(cx, |workspace, cx| {
        workspace.editor(cx).expect("a note is open")
    })
}

fn text(s: &Setup, cx: &mut TestAppContext) -> String {
    pane(s, cx).read_with(cx, |pane, cx| pane.text(cx).to_string())
}

/// Unlocks (or sets the password) and returns the result once it's done.
fn try_unlock(s: &Setup, cx: &mut TestAppContext, password: &str) -> Result<(), String> {
    let task = vault_lock(s, cx).update(cx, |lock, cx| lock.unlock(password.into(), cx));
    let result = Rc::new(RefCell::new(None));
    let slot = result.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| {
            *slot.borrow_mut() = Some(task.await);
        })
        .detach()
    });
    cx.run_until_parked();
    result.take().expect("unlocking finished")
}

fn unlock(s: &Setup, cx: &mut TestAppContext, password: &str) {
    try_unlock(s, cx, password).unwrap();
}

#[gpui_kit::test]
fn a_locked_note_is_encrypted_on_disk_and_hidden_when_the_vault_locks(cx: &mut TestAppContext) {
    let s = setup(cx);
    let plain = s.path("Welcome.md");
    s.vault
        .update(cx, |vault, cx| vault.select(plain.clone(), false, cx));
    cx.run_until_parked();

    // First lock: set the vault password, then lock the open note.
    unlock(&s, cx, "password one");
    in_window(&s, cx, |window, cx| {
        window.dispatch_action(
            Box::new(ToggleNoteLock {
                path: plain.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();

    let locked = s.path("Welcome.md.age");
    assert!(locked.exists() && !plain.exists());
    assert!(
        !fs::read_to_string(&locked).unwrap().contains("Welcome"),
        "encrypted"
    );
    assert_eq!(text(&s, cx), "# Welcome\n", "the open note keeps its text");
    let tab_path = pane(&s, cx).read_with(cx, |pane, _| pane.path().to_path_buf());
    assert_eq!(tab_path, locked, "the tab follows the locked file");

    // Typing saves encrypted.
    let pane_entity = pane(&s, cx);
    cx.update_window(s.window, |_, window, cx| {
        pane_entity.update(cx, |pane, cx| pane.select_and_focus(10..10, window, cx));
    })
    .unwrap();
    in_window(&s, cx, |window, cx| window.input("api_key=xyz", cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let on_disk = fs::read_to_string(&locked).unwrap();
    assert!(!on_disk.contains("xyz"));
    let key = lock::unlock(s.dir.path(), "password one").unwrap();
    assert_eq!(
        lock::read_note(&key, &locked).unwrap(),
        "# Welcome\napi_key=xyz"
    );

    // Lock the vault: the note's text is gone from the editor.
    in_window(&s, cx, |window, cx| window.press("secondary-shift-l", cx));
    cx.run_until_parked();
    assert_eq!(text(&s, cx), "");
    in_window(&s, cx, |window, _| {
        assert!(
            window.try_find("unlock-note").is_some(),
            "the lock screen shows"
        );
    });

    // A wrong password is refused; the right one brings the text back.
    assert!(try_unlock(&s, cx, "nope nope").is_err());
    unlock(&s, cx, "password one");
    assert_eq!(text(&s, cx), "# Welcome\napi_key=xyz");
}

#[gpui_kit::test]
fn enter_in_the_password_dialog_unlocks(cx: &mut TestAppContext) {
    let s = setup(cx);
    lock::create_password(s.dir.path(), "password one").unwrap();
    let vault_lock = vault_lock(&s, cx);
    let dialog_lock = vault_lock.clone();
    in_window(&s, cx, |window, cx| {
        crate::ui::unlock_dialog::open(dialog_lock, None, window, cx)
    });
    cx.run_until_parked();
    in_window(&s, cx, |window, cx| window.input("password one", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));
    cx.run_until_parked();
    assert!(vault_lock.read_with(cx, |lock, _| lock.is_unlocked()));
}
