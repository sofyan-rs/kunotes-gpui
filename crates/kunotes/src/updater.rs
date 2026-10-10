//! `Updater`: looks for a newer KuNotes on GitHub Releases and installs it
//! when the user agrees, using `kunotes_core::update` on a background thread.
//!
//! It checks once at launch (release builds, if Settings → `check_for_updates`)
//! and whenever the user picks "Check for Updates…". The workspace shows the
//! dialogs (`ui/update_dialog.rs`); after an install the user can restart now
//! or keep working (the new version starts next time).

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::{AppContext as _, Context, EventEmitter, Task};
use kunotes_core::update::{self, Release};

use crate::settings_store::SettingsStore;

/// Wait this long after launch, so checking doesn't slow down opening the vault.
const CHECK_AFTER_LAUNCH: Duration = Duration::from_secs(5);

pub enum UpdaterEvent {
    /// A newer version is out; ask whether to install it.
    Available(Release),
    /// The new version is installed; start it from this path to finish.
    Installed { version: String, path: PathBuf },
    /// Something to tell the user ("up to date", or what failed).
    Message(String),
}

pub struct Updater {
    /// The check or install running now; `None` when idle.
    running: Option<Task<()>>,
}

impl EventEmitter<UpdaterEvent> for Updater {}

impl Updater {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut updater = Updater { running: None };
        // Dev builds and tests never check on their own (they'd always be "older").
        let automatic = !cfg!(debug_assertions) && SettingsStore::get(cx).check_for_updates;
        if automatic {
            updater.running = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(CHECK_AFTER_LAUNCH).await;
                let _ = this.update(cx, |updater, cx| {
                    updater.running = None;
                    updater.check(false, cx);
                });
            }));
        }
        // Windows keeps the replaced exe until the next launch; remove it now.
        cx.background_spawn(async {
            if let Err(error) = std::env::current_exe()
                .map_err(Into::into)
                .and_then(|exe| update::clean_up_after_update(&exe))
            {
                log::warn!("couldn't remove the previous version: {error}");
            }
        })
        .detach();
        updater
    }

    /// True while checking or installing.
    pub fn is_busy(&self) -> bool {
        self.running.is_some()
    }

    /// Asks GitHub for the latest release. `manual` checks also say when
    /// there's nothing new or the check failed; automatic ones stay quiet.
    pub fn check(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.is_busy() {
            if manual {
                cx.emit(UpdaterEvent::Message(
                    "Already checking for updates.".into(),
                ));
            }
            return;
        }
        self.running = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async { update::fetch_latest_release() })
                .await;
            let _ = this.update(cx, |updater, cx| {
                updater.running = None;
                let current = env!("CARGO_PKG_VERSION");
                match result {
                    Ok(Some(release)) if update::is_newer(&release.version, current) => {
                        cx.emit(UpdaterEvent::Available(release));
                    }
                    Ok(_) if manual => cx.emit(UpdaterEvent::Message(format!(
                        "KuNotes {current} is the latest version."
                    ))),
                    Ok(_) => {}
                    Err(error) if manual => cx.emit(UpdaterEvent::Message(format!(
                        "Couldn't check for updates: {error}"
                    ))),
                    Err(error) => log::warn!("update check failed: {error}"),
                }
            });
        }));
    }

    /// Downloads, checks, and installs `release`.
    pub fn install(&mut self, release: Release, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        cx.emit(UpdaterEvent::Message(format!(
            "Downloading KuNotes {}…",
            release.version
        )));
        self.running = Some(cx.spawn(async move |this, cx| {
            let version = release.version.clone();
            let result = cx
                .background_spawn(async move { update::download_and_install(&release) })
                .await;
            let _ = this.update(cx, |updater, cx| {
                updater.running = None;
                cx.emit(match result {
                    Ok(path) => UpdaterEvent::Installed { version, path },
                    Err(error) => {
                        UpdaterEvent::Message(format!("Couldn't install the update: {error}"))
                    }
                });
            });
        }));
    }
}
