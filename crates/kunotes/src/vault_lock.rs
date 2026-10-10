//! `VaultLock`: whether the open vault's locked notes are unlocked, holding
//! the unlocked vault key in memory while they are (see `kunotes_core::lock`).
//!
//! - Unlocking takes the vault password (or sets it the first time).
//!   The password can be changed later (File → Change Notes Password…).
//! - The key is forgotten 5 minutes after locked notes were last used, when
//!   the user locks them (`secondary-shift-l`), when another vault opens, and
//!   when the app quits (memory is gone with the process).
//! - Open locked notes watch this entity: they show their text while it has a
//!   key, and save, then clear their editors, when it forgets it.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, Context, Entity, Subscription, Task};
use kunotes_core::lock::{self, VaultKey};

use crate::vault_store::VaultStore;

/// Lock again after this long without using a locked note.
const IDLE_LOCK: Duration = Duration::from_secs(5 * 60);

pub struct VaultLock {
    vault: Entity<VaultStore>,
    root: Option<PathBuf>,
    key: Option<Arc<VaultKey>>,
    last_used: Instant,
    /// Checks every few seconds whether it's time to lock again.
    idle_check: Option<Task<()>>,
    _subscription: Subscription,
}

impl VaultLock {
    pub fn new(vault: Entity<VaultStore>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&vault, |lock, _, cx| lock.follow_vault(cx));
        let root = vault.read(cx).root().map(|root| root.to_path_buf());
        VaultLock {
            vault,
            root,
            key: None,
            last_used: Instant::now(),
            idle_check: None,
            _subscription: subscription,
        }
    }

    /// The unlocked vault key, if locked notes are unlocked.
    pub fn key(&self) -> Option<Arc<VaultKey>> {
        self.key.clone()
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }

    /// True if the vault has a password already (else unlocking sets one).
    pub fn has_password(&self) -> bool {
        self.root.as_deref().is_some_and(lock::has_password)
    }

    /// A locked note was used (opened, edited): restart the idle countdown.
    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Unlocks with `password`, or sets it as the vault password if there is
    /// none yet. The slow part runs in the background; the task's result is
    /// the error to show, if any.
    pub fn unlock(&mut self, password: String, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        let Some(root) = self.root.clone() else {
            return Task::ready(Err("No vault is open.".into()));
        };
        let create = !lock::has_password(&root);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if create {
                        lock::create_password(&root, &password)
                    } else {
                        lock::unlock(&root, &password)
                    }
                })
                .await;
            let key = result.map_err(|error| error.to_string())?;
            this.update(cx, |lock, cx| lock.set_key(key, cx))
                .map_err(|_| "The window was closed.".to_string())
        })
    }

    /// Changes the vault password. Locked notes don't change (they use the
    /// vault key, which stays the same), and they're unlocked afterwards.
    /// The task's result is the error to show, if any.
    pub fn change_password(
        &mut self,
        current: String,
        new: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let Some(root) = self.root.clone() else {
            return Task::ready(Err("No vault is open.".into()));
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { lock::change_password(&root, &current, &new) })
                .await;
            let key = result.map_err(|error| error.to_string())?;
            this.update(cx, |lock, cx| lock.set_key(key, cx))
                .map_err(|_| "The window was closed.".to_string())
        })
    }

    fn set_key(&mut self, key: VaultKey, cx: &mut Context<Self>) {
        self.key = Some(Arc::new(key));
        self.touch();
        self.start_idle_check(cx);
        cx.notify();
    }

    /// Locks all locked notes now (open ones save first, see `EditorPane`).
    pub fn lock_now(&mut self, cx: &mut Context<Self>) {
        if self.key.take().is_some() {
            self.idle_check = None;
            cx.notify();
        }
    }

    fn follow_vault(&mut self, cx: &mut Context<Self>) {
        let root = self.vault.read(cx).root().map(|root| root.to_path_buf());
        if root != self.root {
            self.root = root;
            self.lock_now(cx);
        }
    }

    fn start_idle_check(&mut self, cx: &mut Context<Self>) {
        self.idle_check = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(5)).await;
                let done = this.update(cx, |lock, cx| {
                    if lock.last_used.elapsed() >= IDLE_LOCK {
                        lock.lock_now(cx);
                        true
                    } else {
                        false
                    }
                });
                if done.unwrap_or(true) {
                    break;
                }
            }
        }));
    }
}
