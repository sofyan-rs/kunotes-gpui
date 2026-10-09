//! UI tests for live sync: changes made outside the app (another editor,
//! Finder, git) show up, and a note deleted outside the app is not written back.
//! The test vault is described in `ui/test_helpers.rs`. The real OS watcher is
//! tested separately in `watcher.rs` (GPUI's test scheduler forbids its thread).

use std::fs;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use crate::ui::test_helpers::{Setup, in_window, setup};

/// Names of the vault's top-level items, as the tree shows them.
fn top_level_names(s: &Setup, cx: &mut TestAppContext) -> Vec<String> {
    s.vault.read_with(cx, |vault, _| {
        vault
            .tree()
            .map(|tree| tree.children.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn a_note_created_outside_appears_after_refresh(cx: &mut TestAppContext) {
    let s = setup(cx);
    fs::write(s.path("From Elsewhere.md"), "hi").unwrap();

    s.vault.update(cx, |vault, cx| vault.refresh(cx)); // what the watcher triggers
    cx.run_until_parked();

    assert!(top_level_names(&s, cx).contains(&"From Elsewhere.md".to_string()));
}

#[gpui_kit::test]
fn a_note_deleted_outside_closes_the_editor_without_writing_it_back(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");
    s.vault
        .update(cx, |vault, cx| vault.select(welcome.clone(), false, cx));
    cx.run_until_parked();

    // Type something, so there are unsaved changes.
    let state = s.workspace.read_with(cx, |workspace, cx| {
        workspace.editor(cx).unwrap().read(cx).editor_state()
    });
    cx.update_window(s.window, |_, window, cx| {
        state.update(cx, |state, cx| state.focus(window, cx));
    })
    .unwrap();
    in_window(&s, cx, |window, cx| window.input("typing", cx));

    fs::remove_file(&welcome).unwrap(); // deleted outside the app
    s.vault.update(cx, |vault, cx| vault.refresh(cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1)); // past the autosave delay
    cx.run_until_parked();

    assert!(
        s.workspace
            .read_with(cx, |workspace, cx| workspace.editor(cx).is_none())
    );
    assert!(!welcome.exists(), "the deleted note must not come back");
}
