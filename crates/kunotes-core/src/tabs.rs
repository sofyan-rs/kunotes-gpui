//! The list of open tabs, like VS Code's: preview tabs, pinned tabs, closing,
//! and reordering. Pure data, no UI; the workspace keeps one editor per tab.
//!
//! Rules:
//! - Pinned tabs always come first. Dragging can't mix pinned and unpinned tabs.
//! - At most one *preview* tab exists (shown in italics). Opening another note
//!   in preview replaces it. Editing it, or opening it "for real", makes it permanent.
//! - "Close others / to the right / all" never close pinned tabs.

use std::path::{Path, PathBuf};

use crate::paths::remap_path;

/// One open note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub path: PathBuf,
    pub pinned: bool,
    /// A preview tab is temporary: the next note opened in preview replaces it.
    pub preview: bool,
}

/// The open tabs and which one is active.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabList {
    tabs: Vec<Tab>,
    active: Option<usize>,
}

impl TabList {
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|index| self.tabs.get(index))
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn position(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.path == path)
    }

    /// Opens `path` and makes it active. Returns the paths of tabs that were
    /// replaced (so their editors can be closed).
    ///
    /// - Already open: just activate it (and make it permanent if `preview` is false).
    /// - `preview`: replace the current preview tab, or add one after the active tab.
    /// - Otherwise: add a permanent tab after the active tab.
    pub fn open(&mut self, path: PathBuf, preview: bool) -> Vec<PathBuf> {
        if let Some(index) = self.position(&path) {
            if !preview {
                self.tabs[index].preview = false;
            }
            self.active = Some(index);
            return Vec::new();
        }

        let tab = Tab {
            path,
            pinned: false,
            preview,
        };
        if preview && let Some(index) = self.tabs.iter().position(|tab| tab.preview) {
            let replaced = std::mem::replace(&mut self.tabs[index], tab);
            self.active = Some(index);
            return vec![replaced.path];
        }

        // New tabs go right after the active one (but never among pinned tabs).
        let after_active = self.active.map_or(self.tabs.len(), |index| index + 1);
        let index = after_active.max(self.pinned_count());
        self.tabs.insert(index, tab);
        self.active = Some(index);
        Vec::new()
    }

    pub fn activate(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = Some(index);
        }
    }

    /// Keeps the tab when another note is opened (it was edited, or double-clicked).
    pub fn make_permanent(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.preview = false;
        }
    }

    /// Pins or unpins a tab, moving it to the edge of the pinned group.
    pub fn toggle_pin(&mut self, index: usize) {
        let Some(mut tab) = self.tabs.get(index).cloned() else {
            return;
        };
        let active_path = self.active().map(|tab| tab.path.clone());
        self.tabs.remove(index);
        tab.pinned = !tab.pinned;
        tab.preview = false;
        // Pinning adds to the end of the pinned group; unpinning puts it first after it.
        let target = self.pinned_count();
        self.tabs.insert(target, tab);
        self.restore_active(active_path);
    }

    /// Moves a tab from `from` to position `to`, staying within its group
    /// (pinned tabs among pinned, others among others).
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() {
            return;
        }
        let active_path = self.active().map(|tab| tab.path.clone());
        let tab = self.tabs.remove(from);
        let pinned = self.pinned_count();
        let to = if tab.pinned {
            to.min(pinned)
        } else {
            to.clamp(pinned, self.tabs.len())
        };
        self.tabs.insert(to, tab);
        self.restore_active(active_path);
    }

    /// Closes one tab. Returns its path.
    pub fn close(&mut self, index: usize) -> Option<PathBuf> {
        if index >= self.tabs.len() {
            return None;
        }
        let closed = self.tabs.remove(index);
        self.active = match self.active {
            _ if self.tabs.is_empty() => None,
            // Closing the active tab activates its right neighbor (or the new last tab).
            Some(active) if active == index => Some(index.min(self.tabs.len() - 1)),
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
        Some(closed.path)
    }

    /// Closes every tab for which `should_close` is true. Returns their paths.
    pub fn close_where(&mut self, should_close: impl Fn(usize, &Tab) -> bool) -> Vec<PathBuf> {
        let active_path = self.active().map(|tab| tab.path.clone());
        let mut closed = Vec::new();
        let mut index = 0;
        self.tabs.retain(|tab| {
            let close = should_close(index, tab);
            index += 1;
            if close {
                closed.push(tab.path.clone());
            }
            !close
        });
        self.restore_active(active_path);
        if self.active.is_none() && !self.tabs.is_empty() {
            self.active = Some(self.tabs.len() - 1);
        }
        closed
    }

    /// Closes all unpinned tabs except `keep`, which becomes active.
    pub fn close_others(&mut self, keep: usize) -> Vec<PathBuf> {
        let kept = self.tabs.get(keep).map(|tab| tab.path.clone());
        let closed = self.close_where(|index, tab| index != keep && !tab.pinned);
        if let Some(kept) = kept {
            self.restore_active(Some(kept));
        }
        closed
    }

    /// Closes the unpinned tabs to the right of `index`.
    pub fn close_to_the_right(&mut self, index: usize) -> Vec<PathBuf> {
        self.close_where(|i, tab| i > index && !tab.pinned)
    }

    /// Closes every unpinned tab.
    pub fn close_unpinned(&mut self) -> Vec<PathBuf> {
        self.close_where(|_, tab| !tab.pinned)
    }

    /// After `old` was renamed or moved to `new`, updates every tab under it.
    /// Returns the (old, new) pairs that changed.
    pub fn remap(&mut self, old: &Path, new: &Path) -> Vec<(PathBuf, PathBuf)> {
        let mut changed = Vec::new();
        for tab in &mut self.tabs {
            if let Some(moved) = remap_path(&tab.path, old, new)
                && moved != tab.path
            {
                changed.push((tab.path.clone(), moved.clone()));
                tab.path = moved;
            }
        }
        changed
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|tab| tab.pinned).count()
    }

    fn restore_active(&mut self, active_path: Option<PathBuf>) {
        self.active = active_path.and_then(|path| self.position(&path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("vault/{name}.md"))
    }

    fn names(list: &TabList) -> Vec<String> {
        list.tabs()
            .iter()
            .map(|tab| {
                let name = tab.path.file_stem().unwrap().to_string_lossy().into_owned();
                let mut label = name;
                if tab.pinned {
                    label.push('*');
                }
                if tab.preview {
                    label = format!("({label})");
                }
                label
            })
            .collect()
    }

    fn active_name(list: &TabList) -> Option<String> {
        list.active()
            .map(|tab| tab.path.file_stem().unwrap().to_string_lossy().into_owned())
    }

    #[test]
    fn preview_tabs_replace_each_other() {
        let mut list = TabList::default();
        list.open(path("a"), true);
        let replaced = list.open(path("b"), true);
        assert_eq!(names(&list), ["(b)"]);
        assert_eq!(replaced, [path("a")]);
        assert_eq!(active_name(&list).as_deref(), Some("b"));
    }

    #[test]
    fn permanent_tabs_are_kept_and_new_tabs_open_after_the_active_one() {
        let mut list = TabList::default();
        list.open(path("a"), false);
        list.open(path("b"), false);
        list.activate(0);
        list.open(path("c"), false);
        assert_eq!(names(&list), ["a", "c", "b"]);
        assert_eq!(active_name(&list).as_deref(), Some("c"));
    }

    #[test]
    fn opening_an_open_note_activates_it_and_can_make_it_permanent() {
        let mut list = TabList::default();
        list.open(path("a"), false);
        list.open(path("b"), true);
        list.open(path("a"), true);
        assert_eq!(active_name(&list).as_deref(), Some("a"));
        list.open(path("b"), false); // e.g. double-click in the tree
        assert_eq!(names(&list), ["a", "b"]);
    }

    #[test]
    fn closing_the_active_tab_activates_its_right_neighbor() {
        let mut list = TabList::default();
        for name in ["a", "b", "c"] {
            list.open(path(name), false);
        }
        list.activate(1);
        assert_eq!(list.close(1), Some(path("b")));
        assert_eq!(active_name(&list).as_deref(), Some("c"));
        list.close(1);
        assert_eq!(active_name(&list).as_deref(), Some("a"));
        list.close(0);
        assert!(list.is_empty());
        assert_eq!(list.active_index(), None);
    }

    #[test]
    fn closing_a_tab_left_of_the_active_one_keeps_the_same_tab_active() {
        let mut list = TabList::default();
        for name in ["a", "b", "c"] {
            list.open(path(name), false);
        }
        list.close(0);
        assert_eq!(active_name(&list).as_deref(), Some("c"));
    }

    #[test]
    fn pinned_tabs_move_to_the_front_and_survive_bulk_closes() {
        let mut list = TabList::default();
        for name in ["a", "b", "c", "d"] {
            list.open(path(name), false);
        }
        list.toggle_pin(2); // pin "c"
        assert_eq!(names(&list), ["c*", "a", "b", "d"]);

        list.close_others(1); // keep "a"
        assert_eq!(names(&list), ["c*", "a"]);

        list.open(path("e"), false);
        list.close_unpinned();
        assert_eq!(names(&list), ["c*"]);
        assert_eq!(active_name(&list).as_deref(), Some("c"));
    }

    #[test]
    fn close_to_the_right_keeps_pinned_and_left_tabs() {
        let mut list = TabList::default();
        for name in ["a", "b", "c", "d"] {
            list.open(path(name), false);
        }
        list.activate(3);
        let closed = list.close_to_the_right(1);
        assert_eq!(names(&list), ["a", "b"]);
        assert_eq!(closed, [path("c"), path("d")]);
        assert_eq!(
            active_name(&list).as_deref(),
            Some("b"),
            "active fell back to a remaining tab"
        );
    }

    #[test]
    fn unpinning_puts_the_tab_first_after_the_pinned_group() {
        let mut list = TabList::default();
        for name in ["a", "b", "c"] {
            list.open(path(name), false);
        }
        list.toggle_pin(1); // b*
        list.toggle_pin(2); // c*
        assert_eq!(names(&list), ["b*", "c*", "a"]);
        list.toggle_pin(0); // unpin b
        assert_eq!(names(&list), ["c*", "b", "a"]);
    }

    #[test]
    fn dragging_stays_within_the_pinned_or_unpinned_group() {
        let mut list = TabList::default();
        for name in ["a", "b", "c", "d"] {
            list.open(path(name), false);
        }
        list.toggle_pin(0); // a*
        list.move_tab(3, 0); // try to drag "d" before the pinned tab
        assert_eq!(names(&list), ["a*", "d", "b", "c"]);
        list.move_tab(0, 3); // try to drag the pinned tab among the others
        assert_eq!(names(&list), ["a*", "d", "b", "c"]);
        list.move_tab(1, 3);
        assert_eq!(names(&list), ["a*", "b", "c", "d"]);
    }

    #[test]
    fn new_tabs_never_open_among_pinned_tabs() {
        let mut list = TabList::default();
        list.open(path("a"), false);
        list.toggle_pin(0);
        list.open(path("b"), false);
        list.activate(0);
        list.open(path("c"), false);
        assert_eq!(names(&list), ["a*", "c", "b"]);
    }

    #[test]
    fn pinning_makes_a_preview_tab_permanent() {
        let mut list = TabList::default();
        list.open(path("a"), true);
        list.toggle_pin(0);
        list.open(path("b"), true);
        assert_eq!(names(&list), ["a*", "(b)"]);
    }

    #[test]
    fn remap_follows_renamed_notes_and_folders() {
        let mut list = TabList::default();
        list.open(PathBuf::from("vault/Projects/Plan.md"), false);
        list.open(PathBuf::from("vault/Welcome.md"), false);
        let changed = list.remap(Path::new("vault/Projects"), Path::new("vault/Work"));
        assert_eq!(
            changed,
            [(
                PathBuf::from("vault/Projects/Plan.md"),
                PathBuf::from("vault/Work/Plan.md")
            )]
        );
        assert_eq!(list.tabs()[0].path, PathBuf::from("vault/Work/Plan.md"));
        assert_eq!(list.tabs()[1].path, PathBuf::from("vault/Welcome.md"));
    }

    #[test]
    fn close_where_picks_a_new_active_tab_when_the_active_one_closes() {
        let mut list = TabList::default();
        for name in ["a", "b", "c"] {
            list.open(path(name), false);
        }
        list.activate(2);
        list.close_where(|_, tab| tab.path == path("c"));
        assert_eq!(active_name(&list).as_deref(), Some("b"));
    }
}
