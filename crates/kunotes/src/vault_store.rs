//! `VaultStore`: the shared state of the open vault (folder tree, selection,
//! expanded folders).
//!
//! Views read it and listen to its `VaultEvent`s. All changes to files go
//! through it, so every view stays in sync with the disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, Context, EventEmitter, Task};
use kunotes_core::paths::{ancestors_within, remap_path};
use kunotes_core::{VaultNode, fs_ops};

use crate::settings_store::SettingsStore;
use crate::watcher::{self, VaultWatcher};

/// Something changed that views may want to react to.
#[derive(Debug, Clone)]
pub enum VaultEvent {
    /// The folder tree was rescanned.
    TreeChanged,
    /// A file operation failed; the message is meant for the user.
    Error(String),
}

#[derive(Default)]
pub struct VaultStore {
    root: Option<PathBuf>,
    tree: Option<VaultNode>,
    /// The note shown in the editor.
    selected_file: Option<PathBuf>,
    /// What is selected in the tree (file or folder).
    selected_path: Option<PathBuf>,
    /// Folders the user expanded in the tree. Kept across rescans.
    expanded: HashSet<PathBuf>,
    /// The most recent rename/move (`old`, `new`). The editor uses it to follow
    /// the open note to its new path instead of losing unsaved typing.
    last_move: Option<(PathBuf, PathBuf)>,
    /// Increased on every scan so a slow, outdated scan can't overwrite a newer one.
    scan_generation: u64,
    scan_task: Option<Task<()>>,
    /// Rescans when files change outside the app. `None` if watching failed.
    watcher: Option<VaultWatcher>,
    /// So a broken watcher shows one notification, not one per event.
    watch_error_shown: bool,
    /// Tests turn the OS watcher off: its background thread would break GPUI's
    /// deterministic test scheduler. (Default `false` = live sync on.)
    live_sync_disabled: bool,
}

impl EventEmitter<VaultEvent> for VaultStore {}

impl VaultStore {
    // ----- Reading state -----

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn tree(&self) -> Option<&VaultNode> {
        self.tree.as_ref()
    }

    /// The note that should be open in the editor.
    pub fn selected_file(&self) -> Option<&Path> {
        self.selected_file.as_deref()
    }

    pub fn last_move(&self) -> Option<(&Path, &Path)> {
        self.last_move
            .as_ref()
            .map(|(old, new)| (old.as_path(), new.as_path()))
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.selected_path.as_deref()
    }

    pub fn expanded(&self) -> &HashSet<PathBuf> {
        &self.expanded
    }

    /// The vault folder's name, for titles.
    pub fn name(&self) -> Option<String> {
        self.root.as_deref().map(kunotes_core::paths::note_title)
    }

    // ----- Opening and scanning -----

    /// Opens `path` as the vault and remembers it for the next launch.
    pub fn open_vault(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        *self = VaultStore {
            root: Some(path.clone()),
            live_sync_disabled: self.live_sync_disabled,
            ..VaultStore::default()
        };
        SettingsStore::update(cx, |settings| settings.last_vault = Some(path.clone()));
        self.refresh(cx);
        if !self.live_sync_disabled {
            match watcher::start(&path, cx) {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(message) => self.watch_failed(message, cx),
            }
        }
        cx.notify();
    }

    /// Turns off the OS file watcher for vaults opened after this call. For tests.
    #[cfg(test)]
    pub fn disable_live_sync(&mut self) {
        self.live_sync_disabled = true;
    }

    /// Live sync broke (e.g. Linux's inotify watch limit). The app keeps working;
    /// changes made outside it just won't show up until the vault is reopened.
    pub fn watch_failed(&mut self, message: String, cx: &mut Context<Self>) {
        log::warn!("file watcher: {message}");
        if !self.watch_error_shown {
            self.watch_error_shown = true;
            cx.emit(VaultEvent::Error(format!(
                "Live sync is unavailable, so outside changes won't appear automatically: {message}"
            )));
        }
    }

    /// Reopens the vault from the last session, if its folder still exists.
    pub fn restore_last_vault(&mut self, cx: &mut Context<Self>) {
        let Some(path) = SettingsStore::get(cx).last_vault.clone() else {
            return;
        };
        if path.is_dir() {
            self.open_vault(path, cx);
        } else {
            log::info!("last vault {} is gone; forgetting it", path.display());
            SettingsStore::update(cx, |settings| settings.last_vault = None);
        }
    }

    /// Closes the vault and forgets it.
    pub fn close_vault(&mut self, cx: &mut Context<Self>) {
        *self = VaultStore::default();
        SettingsStore::update(cx, |settings| settings.last_vault = None);
        cx.emit(VaultEvent::TreeChanged);
        cx.notify();
    }

    /// Rescans the vault folder in the background, then updates the tree.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.scan_generation += 1;
        let generation = self.scan_generation;

        // Scanning reads the disk, so it runs off the UI thread.
        let scan = cx.background_spawn(async move { VaultNode::scan(&root) });
        self.scan_task = Some(cx.spawn(async move |this, cx| {
            let tree = scan.await;
            // `this` is a weak handle: the store may be gone by now, so ignore errors.
            let _ = this.update(cx, |store, cx| {
                if store.scan_generation != generation {
                    return; // a newer scan was started; drop this result
                }
                store.tree = Some(tree);
                store.forget_missing_paths();
                cx.emit(VaultEvent::TreeChanged);
                cx.notify();
            });
        }));
    }

    /// After a rescan: drop selections and expanded folders that no longer exist
    /// (e.g. deleted outside the app). Clearing `selected_file` closes the editor,
    /// and the workspace won't write a missing note back.
    /// Only a handful of paths are checked, so this is cheap on the UI thread.
    fn forget_missing_paths(&mut self) {
        let missing = |slot: &Option<PathBuf>| slot.as_deref().is_some_and(|p| !p.exists());
        if missing(&self.selected_file) {
            self.selected_file = None;
        }
        if missing(&self.selected_path) {
            self.selected_path = None;
        }
        self.expanded.retain(|folder| folder.exists());
    }

    // ----- Selection and expansion -----

    /// Selects a tree item. Selecting a file also opens it in the editor.
    pub fn select(&mut self, path: PathBuf, is_dir: bool, cx: &mut Context<Self>) {
        if !is_dir {
            self.selected_file = Some(path.clone());
        }
        self.selected_path = Some(path);
        cx.notify();
    }

    /// Opens a note from outside the tree (e.g. the quick switcher): selects it
    /// and expands its folders so it's visible in the tree.
    pub fn open_note(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.reveal(&path);
        self.select(path, false, cx);
    }

    pub fn set_expanded(&mut self, folder: PathBuf, expanded: bool, cx: &mut Context<Self>) {
        if expanded {
            self.expanded.insert(folder);
        } else {
            self.expanded.remove(&folder);
        }
        cx.notify();
    }

    pub fn toggle_expanded(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        let expanded = !self.expanded.contains(&folder);
        self.set_expanded(folder, expanded, cx);
    }

    /// Expands every folder above `path`, so it becomes visible in the tree.
    fn reveal(&mut self, path: &Path) {
        if let Some(root) = &self.root {
            self.expanded.extend(ancestors_within(root, path));
        }
    }

    // ----- File operations -----
    // Each one changes the disk through `kunotes_core::fs_ops`, then rescans.
    // Failures are sent as `VaultEvent::Error` so the window can show them.

    /// Creates a new note in `folder` (or the vault root) and selects it.
    pub fn create_file(&mut self, folder: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(folder) = folder.or_else(|| self.root.clone()) else {
            return;
        };
        match fs_ops::create_file(&folder, "Untitled") {
            Ok(path) => {
                self.reveal(&path);
                self.select(path, false, cx);
                self.refresh(cx);
            }
            Err(error) => cx.emit(VaultEvent::Error(error.to_string())),
        }
    }

    /// Creates a new folder in `folder` (or the vault root) and selects it.
    pub fn create_folder(&mut self, folder: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(folder) = folder.or_else(|| self.root.clone()) else {
            return;
        };
        match fs_ops::create_folder(&folder, "New Folder") {
            Ok(path) => {
                self.reveal(&path);
                self.select(path, true, cx);
                self.refresh(cx);
            }
            Err(error) => cx.emit(VaultEvent::Error(error.to_string())),
        }
    }

    /// Renames a file or folder. Returns the error message if it failed,
    /// so the rename dialog can stay open and show it.
    pub fn rename(
        &mut self,
        path: &Path,
        new_name: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let new_path = fs_ops::rename(path, new_name).map_err(|error| error.to_string())?;
        self.remap(path, &new_path);
        self.refresh(cx);
        cx.notify();
        Ok(())
    }

    /// Moves a file or folder into `folder`.
    pub fn move_into(&mut self, path: &Path, folder: &Path, cx: &mut Context<Self>) {
        match fs_ops::move_into(path, folder) {
            Ok(new_path) => {
                self.remap(path, &new_path);
                self.expanded.insert(folder.to_path_buf());
                self.refresh(cx);
                cx.notify();
            }
            Err(error) => cx.emit(VaultEvent::Error(error.to_string())),
        }
    }

    /// Moves a file or folder to the OS trash.
    pub fn trash(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Err(error) = fs_ops::trash(path) {
            cx.emit(VaultEvent::Error(error.to_string()));
            return;
        }
        // Forget anything that was inside the trashed item.
        let inside = |p: &Option<PathBuf>| p.as_deref().is_some_and(|p| p.starts_with(path));
        if inside(&self.selected_file) {
            self.selected_file = None;
        }
        if inside(&self.selected_path) {
            self.selected_path = None;
        }
        self.expanded.retain(|folder| !folder.starts_with(path));
        self.refresh(cx);
        cx.notify();
    }

    /// After `old` became `new`, points the selection and expanded folders at the new paths.
    fn remap(&mut self, old: &Path, new: &Path) {
        self.last_move = Some((old.to_path_buf(), new.to_path_buf()));
        let update = |slot: &mut Option<PathBuf>| {
            if let Some(path) = slot.as_deref()
                && let Some(moved) = remap_path(path, old, new)
            {
                *slot = Some(moved);
            }
        };
        update(&mut self.selected_file);
        update(&mut self.selected_path);
        self.expanded = self
            .expanded
            .drain()
            .map(|folder| remap_path(&folder, old, new).unwrap_or(folder))
            .collect();
    }
}
