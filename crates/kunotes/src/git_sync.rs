//! `GitSync`: keeps the open vault in step with a git remote, using
//! `kunotes_core::git` on a background thread.
//!
//! When sync is on for a vault (Settings → `synced_vaults`), it syncs:
//! - when the vault opens (to bring in other computers' changes),
//! - a minute after the last edit, and every five minutes while open,
//! - when the user clicks the sync status in the sidebar,
//! - and when the app quits (commit, plus a push that gives up after a few seconds).
//!
//! Open notes are saved before every sync. Afterwards the workspace reloads
//! open notes that changed and shows any conflict copies (`GitSyncEvent`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task};
use kunotes_core::git::{self, SyncReport};

use crate::settings_store::SettingsStore;
use crate::ui::editor_area::{EditorArea, EditorAreaEvent};
use crate::vault_store::VaultStore;

/// Sync this long after the last edit.
const AFTER_EDIT: Duration = Duration::from_secs(60);
/// Sync this often while the vault is open, to bring in other computers' changes.
const EVERY: Duration = Duration::from_secs(5 * 60);
/// When quitting, wait at most this long for the push.
const QUIT_PUSH_TIMEOUT: Duration = Duration::from_secs(5);

/// What the sidebar shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    /// Sync isn't set up for this vault.
    Off,
    /// Set up; `last` is when the last sync finished.
    Idle {
        last: Option<Instant>,
    },
    Syncing,
    /// The last sync failed; the text says why.
    Failed(String),
}

pub enum GitSyncEvent {
    /// A sync finished; reload these notes and mention any conflict copies.
    Synced(SyncReport),
    /// Something to tell the user (a failed sync, or sync turned on/off).
    Message(String),
}

pub struct GitSync {
    vault: Entity<VaultStore>,
    editor_area: Entity<EditorArea>,
    /// The vault this is set up for.
    root: Option<PathBuf>,
    /// The remote's address, while sync is on.
    remote: Option<String>,
    status: SyncStatus,
    /// The sync running now; `None` when idle.
    running: Option<Task<()>>,
    /// Another sync was asked for while one was running.
    again: bool,
    /// Waits `AFTER_EDIT` after the last edit (replaced, i.e. restarted, by each edit).
    after_edit: Option<Task<()>>,
    /// Syncs every `EVERY`, and redraws "synced N min ago".
    ticker: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<GitSyncEvent> for GitSync {}

impl GitSync {
    pub fn new(
        vault: Entity<VaultStore>,
        editor_area: Entity<EditorArea>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&vault, |sync, _, cx| sync.follow_vault(cx)),
            cx.subscribe(&editor_area, |sync, _, event, cx| match event {
                EditorAreaEvent::NoteEdited => sync.schedule_after_edit(cx),
            }),
        ];
        let mut sync = GitSync {
            vault,
            editor_area,
            root: None,
            remote: None,
            status: SyncStatus::Off,
            running: None,
            again: false,
            after_edit: None,
            ticker: None,
            _subscriptions: subscriptions,
        };
        sync.follow_vault(cx);
        sync
    }

    pub fn status(&self) -> &SyncStatus {
        &self.status
    }

    /// The remote's address, while sync is on.
    pub fn remote(&self) -> Option<&str> {
        self.remote.as_deref()
    }

    /// Starts syncing the open vault with `url`.
    pub fn connect(&mut self, url: String, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.save_open_notes(cx);
        self.status = SyncStatus::Syncing;
        cx.notify();
        let task_root = root.clone();
        let task_url = url.clone();
        self.running = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { git::connect(&task_root, &task_url) })
                .await;
            let _ = this.update(cx, |sync, cx| {
                sync.running = None;
                match result {
                    Ok(report) => {
                        SettingsStore::update(cx, |settings| {
                            if !settings.synced_vaults.contains(&root) {
                                settings.synced_vaults.push(root.clone());
                            }
                        });
                        sync.remote = Some(url);
                        sync.finished(Ok(report), cx);
                        sync.start_ticker(cx);
                        cx.emit(GitSyncEvent::Message("Git sync is on.".into()));
                    }
                    Err(error) => {
                        sync.status = SyncStatus::Off;
                        cx.emit(GitSyncEvent::Message(format!(
                            "Couldn't set up git sync: {error}"
                        )));
                        cx.notify();
                    }
                }
            });
        }));
    }

    /// Stops syncing the open vault. Its `.git` history stays.
    pub fn disconnect(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        SettingsStore::update(cx, |settings| {
            settings.synced_vaults.retain(|vault| *vault != root)
        });
        self.turn_off(cx);
        cx.background_spawn(async move {
            if let Err(error) = git::disconnect(&root) {
                log::warn!("couldn't remove the git remote: {error}");
            }
        })
        .detach();
        cx.emit(GitSyncEvent::Message("Git sync is off.".into()));
    }

    /// Syncs now (or right after the sync that's running).
    pub fn sync_now(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.remote.is_none() {
            return;
        }
        if self.running.is_some() {
            self.again = true;
            return;
        }
        self.after_edit = None;
        // Unsaved typing must be on disk before it's committed.
        self.save_open_notes(cx);
        self.status = SyncStatus::Syncing;
        cx.notify();
        self.running = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { git::sync(&root) }).await;
            let _ = this.update(cx, |sync, cx| {
                sync.running = None;
                sync.finished(result, cx);
                if std::mem::take(&mut sync.again) {
                    sync.sync_now(cx);
                }
            });
        }));
    }

    /// When the app quits: commit, and try to push for a few seconds.
    pub fn finish_on_quit(&mut self, cx: &mut Context<Self>) {
        let (Some(root), Some(_)) = (&self.root, &self.remote) else {
            return;
        };
        self.save_open_notes(cx);
        if let Err(error) = git::commit_and_push_quickly(root, QUIT_PUSH_TIMEOUT) {
            log::warn!("git sync on quit failed: {error}");
        }
    }

    // ----- Internals -----

    fn finished(&mut self, result: kunotes_core::Result<SyncReport>, cx: &mut Context<Self>) {
        match result {
            Ok(report) => {
                self.status = SyncStatus::Idle {
                    last: Some(Instant::now()),
                };
                if !report.changed.is_empty() || !report.conflicts.is_empty() {
                    self.vault.update(cx, |vault, cx| vault.refresh(cx));
                }
                cx.emit(GitSyncEvent::Synced(report));
            }
            Err(error) => {
                let message = error.to_string();
                self.status = SyncStatus::Failed(message.clone());
                cx.emit(GitSyncEvent::Message(format!("Git sync failed: {message}")));
            }
        }
        cx.notify();
    }

    /// The vault opened or closed: set up (or stop) sync for it.
    fn follow_vault(&mut self, cx: &mut Context<Self>) {
        let root = self.vault.read(cx).root().map(|root| root.to_path_buf());
        if root == self.root {
            return;
        }
        self.root = root.clone();
        self.turn_off(cx);
        let Some(root) = root else {
            return;
        };
        if !SettingsStore::get(cx).synced_vaults.contains(&root) {
            return;
        }
        // Read the remote's address in the background, then sync.
        cx.spawn(async move |this, cx| {
            let lookup_root = root.clone();
            let remote = cx
                .background_spawn(async move { git::remote_url(&lookup_root) })
                .await;
            let _ = this.update(cx, |sync, cx| {
                if sync.root.as_ref() != Some(&root) {
                    return; // another vault opened meanwhile
                }
                match remote {
                    Some(remote) => {
                        sync.remote = Some(remote);
                        sync.status = SyncStatus::Idle { last: None };
                        sync.start_ticker(cx);
                        sync.sync_now(cx);
                    }
                    None => sync.status = SyncStatus::Failed("The vault has no git remote.".into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn turn_off(&mut self, cx: &mut Context<Self>) {
        self.remote = None;
        self.status = SyncStatus::Off;
        self.running = None;
        self.after_edit = None;
        self.ticker = None;
        cx.notify();
    }

    fn schedule_after_edit(&mut self, cx: &mut Context<Self>) {
        if self.remote.is_none() {
            return;
        }
        // Replacing the task cancels the previous wait, so this restarts the minute.
        self.after_edit = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AFTER_EDIT).await;
            let _ = this.update(cx, |sync, cx| sync.sync_now(cx));
        }));
    }

    /// Syncs every `EVERY`; also redraws every 30 s so "N min ago" stays current.
    fn start_ticker(&mut self, cx: &mut Context<Self>) {
        self.ticker = Some(cx.spawn(async move |this, cx| {
            let mut since_sync = Duration::ZERO;
            loop {
                let tick = Duration::from_secs(30);
                cx.background_executor().timer(tick).await;
                since_sync += tick;
                let keep_going = this.update(cx, |sync, cx| {
                    if since_sync >= EVERY {
                        since_sync = Duration::ZERO;
                        sync.sync_now(cx);
                    }
                    cx.notify();
                });
                if keep_going.is_err() {
                    break;
                }
            }
        }));
    }

    fn save_open_notes(&self, cx: &mut Context<Self>) {
        self.editor_area.update(cx, |area, cx| area.save_all(cx));
    }
}
