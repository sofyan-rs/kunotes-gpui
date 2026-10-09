//! `VaultStore`: the shared state of the open vault (folder tree, selection).
//!
//! Views read it and listen to its `VaultEvent`s. All changes to files go
//! through it, so every view stays in sync with the disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, Context, EventEmitter, Task};
use kunotes_core::{VaultNode, fs_ops};

use crate::settings_store::SettingsStore;

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
    /// Increased on every scan so a slow, outdated scan can't overwrite a newer one.
    scan_generation: u64,
    scan_task: Option<Task<()>>,
}

impl EventEmitter<VaultEvent> for VaultStore {}

impl VaultStore {
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn tree(&self) -> Option<&VaultNode> {
        self.tree.as_ref()
    }

    pub fn expanded(&self) -> &HashSet<PathBuf> {
        &self.expanded
    }

    /// The vault folder's name, for titles.
    pub fn name(&self) -> Option<String> {
        self.root.as_deref().map(kunotes_core::paths::note_title)
    }

    /// Opens `path` as the vault and remembers it for the next launch.
    pub fn open_vault(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        *self = VaultStore {
            root: Some(path.clone()),
            ..VaultStore::default()
        };
        SettingsStore::update(cx, |settings| settings.last_vault = Some(path));
        self.refresh(cx);
        cx.notify();
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
                cx.emit(VaultEvent::TreeChanged);
                cx.notify();
            });
        }));
    }

    /// Creates a new note in the vault root.
    pub fn create_file(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        match fs_ops::create_file(&root, "Untitled") {
            Ok(path) => {
                self.selected_file = Some(path.clone());
                self.selected_path = Some(path);
                self.refresh(cx);
            }
            Err(error) => cx.emit(VaultEvent::Error(error.to_string())),
        }
    }

    /// Creates a new folder in the vault root.
    pub fn create_folder(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        match fs_ops::create_folder(&root, "New Folder") {
            Ok(path) => {
                self.selected_path = Some(path);
                self.refresh(cx);
            }
            Err(error) => cx.emit(VaultEvent::Error(error.to_string())),
        }
    }
}
