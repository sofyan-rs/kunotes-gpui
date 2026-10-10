//! Locking a note (encrypting it as `Name.md.age`) and removing a lock.
//!
//! Both need the vault unlocked first, so they go through the password dialog
//! when it isn't. The open note keeps its tab and its text: the tab follows the
//! new file name, and the old file goes to the trash.

use std::path::PathBuf;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, AppContext as _, Entity, Window};
use kunotes_core::lock;

use crate::ui::editor_area::EditorArea;
use crate::ui::unlock_dialog;
use crate::vault_lock::VaultLock;
use crate::vault_store::VaultStore;

/// Locks `path` if it's a normal note, or removes the lock if it's locked.
pub fn toggle(
    path: PathBuf,
    vault: Entity<VaultStore>,
    vault_lock: Entity<VaultLock>,
    editor_area: Entity<EditorArea>,
    window: &mut Window,
    cx: &mut App,
) {
    let lock_for_later = vault_lock.clone();
    let then = Box::new(move |window: &mut Window, cx: &mut App| {
        run(path, vault, lock_for_later, editor_area, window, cx);
    });
    unlock_dialog::open(vault_lock, Some(then), window, cx);
}

/// The vault is unlocked: do it.
fn run(
    path: PathBuf,
    vault: Entity<VaultStore>,
    vault_lock: Entity<VaultLock>,
    editor_area: Entity<EditorArea>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(key) = vault_lock.read(cx).key() else {
        return;
    };
    // The note on disk must have the latest typing before it's converted.
    editor_area.update(cx, |area, cx| area.save_all(cx));
    let locking = !lock::is_locked_note(&path);
    let task_path = path.clone();
    let task = cx.background_spawn(async move {
        if locking {
            lock::lock_note(&key, &task_path)
        } else {
            lock::unlock_note(&key, &task_path)
        }
    });
    window
        .spawn(cx, async move |cx| {
            let result = task.await;
            let _ = cx.update(|window, cx| match result {
                Ok(new_path) => {
                    vault.update(cx, |vault, cx| vault.replace_note(&path, &new_path, cx));
                    let message = if locking {
                        "Note locked. The unlocked copy is in the trash; older versions \
                         may still be in the git history."
                    } else {
                        "Lock removed. The note is plain text again."
                    };
                    window.push_notification(message, cx);
                }
                Err(error) => window.push_notification(error.to_string(), cx),
            });
        })
        .detach();
}
