//! `EditorArea`: the right side of the window. A row of tabs (one per open note)
//! above the active note's editor, like VS Code.
//!
//! - Clicking a note in the tree opens it in a *preview* tab (italic), which the
//!   next clicked note replaces. Editing it, double-clicking it, or opening it from
//!   the quick switcher keeps it open.
//! - Each tab has its own `EditorPane`, created when the tab is first shown, so
//!   cursor position and unsaved changes survive switching tabs.
//! - The tab list itself (preview, pin, close, reorder rules) is
//!   `kunotes_core::tabs::TabList`; this file connects it to editors and the tree.

#[cfg(test)]
mod editor_area_tests;
mod tab_bar;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Window, div,
};
use kunotes_core::settings::SavedTab;
use kunotes_core::tabs::TabList;

use crate::settings_store::SettingsStore;
use crate::ui::editor::{EditorEvent, EditorPane, load_note};
use crate::ui::empty_state;
use crate::vault_lock::VaultLock;
use crate::vault_store::{VaultEvent, VaultStore};

/// Something other parts of the window react to.
pub enum EditorAreaEvent {
    /// The text of an open note changed (git sync waits a bit, then syncs).
    NoteEdited,
}

pub struct EditorArea {
    vault: Entity<VaultStore>,
    /// Unlocks locked notes; each pane watches it.
    vault_lock: Entity<VaultLock>,
    tabs: TabList,
    /// The editor of each open tab that has been shown at least once, by path.
    panes: HashMap<PathBuf, Entity<EditorPane>>,
    /// Listeners for each pane (errors, edits, redraws); dropped with the pane.
    pane_subscriptions: HashMap<PathBuf, [Subscription; 2]>,
    /// The vault the tabs belong to. Tabs are closed when another vault opens.
    root: Option<PathBuf>,
    /// The tab that was active last time, so leaving it can save it right away.
    last_active: Option<PathBuf>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorAreaEvent> for EditorArea {}

impl EditorArea {
    pub fn new(
        vault: Entity<VaultStore>,
        vault_lock: Entity<VaultLock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe_in(&vault, window, |area, _, window, cx| {
                area.sync_with_vault(window, cx);
            }),
            cx.subscribe_in(&vault, window, |area, _, event, window, cx| match event {
                VaultEvent::KeepOpen(path) => area.open(path.clone(), false, window, cx),
                VaultEvent::NoteCreated(path) => {
                    area.open(path.clone(), false, window, cx);
                    area.focus_active_editor(window, cx);
                }
                VaultEvent::TreeChanged | VaultEvent::Error(_) => {}
            }),
        ];
        EditorArea {
            vault,
            vault_lock,
            tabs: TabList::default(),
            panes: HashMap::new(),
            pane_subscriptions: HashMap::new(),
            root: None,
            last_active: None,
            _subscriptions: subscriptions,
        }
    }

    // ----- Reading state -----

    /// The tab list. For tests.
    #[cfg(test)]
    pub fn tab_list(&self) -> &TabList {
        &self.tabs
    }

    pub fn active_pane(&self) -> Option<&Entity<EditorPane>> {
        self.panes.get(&self.tabs.active()?.path)
    }

    /// The active note's title, for the window title.
    pub fn active_title(&self) -> Option<String> {
        self.tabs
            .active()
            .map(|tab| kunotes_core::paths::note_title(&tab.path))
    }

    fn is_dirty(&self, path: &Path, cx: &App) -> bool {
        self.panes
            .get(path)
            .is_some_and(|pane| pane.read(cx).is_dirty())
    }

    // ----- Keeping in sync with the vault -----

    /// Runs whenever the vault changes: follows renames, closes tabs of deleted
    /// notes, and opens the note selected in the tree.
    fn sync_with_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let vault = self.vault.read(cx);
        let root = vault.root().map(Path::to_path_buf);
        let selected = vault.selected_file().map(Path::to_path_buf);
        let moved = vault
            .last_move()
            .map(|(old, new)| (old.to_path_buf(), new.to_path_buf()));

        if root != self.root {
            self.switch_vault(root, window, cx);
            return;
        }

        // A renamed or moved note keeps its tab and editor (and unsaved typing).
        if let Some((old, new)) = moved {
            for (from, to) in self.tabs.remap(&old, &new) {
                if let Some(pane) = self.panes.remove(&from) {
                    pane.update(cx, |pane, cx| pane.set_path(to.clone(), cx));
                    self.panes.insert(to.clone(), pane);
                }
                if let Some(subscriptions) = self.pane_subscriptions.remove(&from) {
                    self.pane_subscriptions.insert(to, subscriptions);
                }
            }
        }

        // Notes deleted (in the app or elsewhere) lose their tab, without saving:
        // writing them would bring them back.
        for path in self.tabs.close_where(|_, tab| !tab.path.exists()) {
            self.forget_pane(&path);
        }

        if let Some(path) = selected
            && self.tabs.active().map(|tab| &tab.path) != Some(&path)
        {
            self.open(path, true, window, cx);
        }
        self.after_tabs_changed(window, cx);
    }

    /// Another vault was opened (or closed): save and close the old tabs, then
    /// bring back the tabs remembered for the new vault.
    fn switch_vault(&mut self, root: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let had_tabs = !self.tabs.is_empty();
        for path in self.tabs.close_where(|_, _| true) {
            self.close_pane(&path, cx);
        }
        self.root = root.clone();
        if had_tabs {
            self.save_tabs(cx); // the old vault's tabs don't belong to the new one
        } else if let Some(root) = &root {
            self.restore_tabs(root, window, cx);
        }
        cx.notify();
    }

    /// Reopens the tabs saved in settings for this vault (skipping notes that are gone).
    fn restore_tabs(&mut self, root: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let settings = SettingsStore::get(cx);
        let saved: Vec<SavedTab> = settings
            .open_tabs
            .iter()
            .filter(|tab| tab.path.starts_with(root) && tab.path.is_file())
            .cloned()
            .collect();
        let active = settings.active_tab;
        for tab in &saved {
            self.tabs.open(tab.path.clone(), tab.preview);
            if tab.pinned {
                let index = self.tabs.position(&tab.path).unwrap_or_default();
                self.tabs.toggle_pin(index);
            }
        }
        if let Some(index) = active {
            self.tabs.activate(index.min(saved.len().saturating_sub(1)));
        }
        self.after_tabs_changed(window, cx);
    }

    /// After any change to the tabs: create the active tab's editor if needed,
    /// show the active note in the tree, remember the tabs, and redraw.
    fn after_tabs_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Leaving a tab saves it immediately, instead of waiting for autosave.
        let active = self.tabs.active().map(|tab| tab.path.clone());
        if self.last_active != active {
            if let Some(previous) = self.last_active.take()
                && previous.exists()
                && let Some(pane) = self.panes.get(&previous)
            {
                pane.update(cx, |pane, cx| pane.save_now(cx));
            }
            self.last_active = active;
        }

        if let Some(path) = self.tabs.active().map(|tab| tab.path.clone()) {
            self.ensure_pane(&path, window, cx);
            let shown_in_tree = self.vault.read(cx).selected_file() == Some(path.as_path());
            if !shown_in_tree {
                self.vault
                    .update(cx, |vault, cx| vault.select(path, false, cx));
            }
        } else {
            self.vault
                .update(cx, |vault, cx| vault.clear_selected_file(cx));
        }
        self.save_tabs(cx);
        cx.notify();
    }

    fn save_tabs(&self, cx: &mut App) {
        let open_tabs: Vec<SavedTab> = self
            .tabs
            .tabs()
            .iter()
            .map(|tab| SavedTab {
                path: tab.path.clone(),
                pinned: tab.pinned,
                preview: tab.preview,
            })
            .collect();
        let active_tab = self.tabs.active_index();
        SettingsStore::update(cx, |settings| {
            settings.open_tabs = open_tabs;
            settings.active_tab = active_tab;
        });
    }

    // ----- Editors -----

    /// Loads the note into a new editor, unless it already has one.
    fn ensure_pane(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.contains_key(path) {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let key = self.vault_lock.read(cx).key();
        let note = match load_note(path.to_path_buf(), key.as_deref()) {
            Ok(note) => note,
            Err(message) => {
                window.push_notification(message, cx);
                if let Some(index) = self.tabs.position(path) {
                    self.tabs.close(index);
                }
                return;
            }
        };
        let vault_lock = self.vault_lock.clone();
        let pane = cx.new(|cx| EditorPane::new(note, root, vault_lock, window, cx));
        if pane.read(cx).is_read_only() {
            window.push_notification(
                "This file isn't valid UTF-8, so it opened read-only to avoid damaging it.",
                cx,
            );
        }
        let subscriptions = [
            cx.subscribe_in(&pane, window, |area, pane, event, window, cx| match event {
                EditorEvent::Error(message) => window.push_notification(message.clone(), cx),
                EditorEvent::Edited => {
                    cx.emit(EditorAreaEvent::NoteEdited);
                    // Typing in a preview tab keeps it open.
                    let path = pane.read(cx).path().to_path_buf();
                    if let Some(index) = area.tabs.position(&path)
                        && area.tabs.tabs()[index].preview
                    {
                        area.tabs.make_permanent(index);
                        area.save_tabs(cx);
                        cx.notify();
                    }
                }
            }),
            // Redraw the tab bar when a note's unsaved state changes.
            cx.observe(&pane, |_, _, cx| cx.notify()),
        ];
        self.panes.insert(path.to_path_buf(), pane);
        self.pane_subscriptions
            .insert(path.to_path_buf(), subscriptions);
    }

    /// Saves (if the note still exists) and drops a tab's editor.
    fn close_pane(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(path)
            && path.exists()
        {
            pane.update(cx, |pane, cx| pane.save_now(cx));
        }
        self.forget_pane(path);
    }

    /// Drops a tab's editor without saving.
    fn forget_pane(&mut self, path: &Path) {
        self.panes.remove(path);
        self.pane_subscriptions.remove(path);
    }

    /// Shows the new contents of open notes that changed on disk (after a git
    /// sync brought in changes). Notes with unsaved typing keep it.
    pub fn reload_from_disk(
        &mut self,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for path in paths {
            if let Some(pane) = self.panes.get(path) {
                pane.update(cx, |pane, cx| pane.reload_from_disk(window, cx));
            }
        }
    }

    /// Writes every open note's unsaved changes (before quitting).
    pub fn save_all(&mut self, cx: &mut Context<Self>) {
        for pane in self.panes.values() {
            pane.update(cx, |pane, cx| pane.save_now(cx));
        }
    }

    pub fn focus_active_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Deferred: a just-opened tab's editor is created in the same update.
        cx.defer_in(window, |area, window, cx| {
            if let Some(pane) = area.active_pane() {
                pane.update(cx, |pane, cx| pane.focus(window, cx));
            }
        });
    }

    // ----- Tab operations (from clicks, the tab menu, and shortcuts) -----

    pub fn open(
        &mut self,
        path: PathBuf,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for replaced in self.tabs.open(path, preview) {
            self.close_pane(&replaced, cx);
        }
        self.after_tabs_changed(window, cx);
    }

    pub fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.activate(index);
        self.after_tabs_changed(window, cx);
    }

    pub fn make_permanent(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.make_permanent(index);
        self.after_tabs_changed(window, cx);
    }

    pub fn close(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.tabs.close(index) {
            self.close_pane(&path, cx);
        }
        self.after_tabs_changed(window, cx);
    }

    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.active_index() {
            self.close(index, window, cx);
        }
    }

    pub fn close_others(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.active_index() {
            for path in self.tabs.close_others(index) {
                self.close_pane(&path, cx);
            }
            self.after_tabs_changed(window, cx);
        }
    }

    pub fn close_to_the_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.active_index() {
            for path in self.tabs.close_to_the_right(index) {
                self.close_pane(&path, cx);
            }
            self.after_tabs_changed(window, cx);
        }
    }

    /// Closes unpinned tabs that have nothing waiting to be saved.
    pub fn close_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dirty: Vec<PathBuf> = self
            .tabs
            .tabs()
            .iter()
            .filter(|tab| self.is_dirty(&tab.path, cx))
            .map(|tab| tab.path.clone())
            .collect();
        for path in self
            .tabs
            .close_where(|_, tab| !tab.pinned && !dirty.contains(&tab.path))
        {
            self.close_pane(&path, cx);
        }
        self.after_tabs_changed(window, cx);
    }

    pub fn close_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for path in self.tabs.close_unpinned() {
            self.close_pane(&path, cx);
        }
        self.after_tabs_changed(window, cx);
    }

    pub fn toggle_pin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.active_index() {
            self.tabs.toggle_pin(index);
            self.after_tabs_changed(window, cx);
        }
    }

    /// Moves to the next (`step = 1`) or previous (`step = -1`) tab, wrapping around.
    pub fn cycle(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.tabs.tabs().len();
        if let Some(index) = self.tabs.active_index()
            && count > 1
        {
            let next = (index as isize + step).rem_euclid(count as isize) as usize;
            self.activate(next, window, cx);
        }
    }

    pub fn move_tab(
        &mut self,
        from: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs.move_tab(from, to);
        self.after_tabs_changed(window, cx);
    }
}

impl Render for EditorArea {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.active_pane() {
            Some(pane) => pane.clone().into_any_element(),
            None if self.root.is_some() => empty_state::no_file_selected(cx).into_any_element(),
            None => empty_state::no_vault(cx).into_any_element(),
        };
        let background = cx.theme().background;
        let tab_bar = (!self.tabs.is_empty()).then(|| tab_bar::render(self, cx));
        v_flex()
            .size_full()
            .bg(background)
            .children(tab_bar)
            .child(div().flex_1().min_h_0().child(body))
    }
}
