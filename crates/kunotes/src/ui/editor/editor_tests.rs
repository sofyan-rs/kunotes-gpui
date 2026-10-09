//! UI tests for the editor pane: loading, autosave, saving on switch, following
//! renames, formatting, and view modes. The test vault is described in
//! `ui/test_helpers.rs`.

use std::fs;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use kunotes_core::settings::ViewMode;

use crate::settings_store::SettingsStore;
use crate::ui::editor::EditorPane;
use crate::ui::test_helpers::{Setup, in_window, setup};

/// Selects the note at `relative` in the vault, which opens it in the editor.
fn open_note(s: &Setup, cx: &mut TestAppContext, relative: &str) {
    let path = s.path(relative);
    s.vault
        .update(cx, |vault, cx| vault.select(path, false, cx));
    cx.run_until_parked();
}

/// The open note's editor pane (panics if no note is open).
fn pane(s: &Setup, cx: &mut TestAppContext) -> Entity<EditorPane> {
    s.workspace.read_with(cx, |workspace, cx| {
        workspace.editor(cx).expect("a note should be open")
    })
}

fn editor_text(s: &Setup, cx: &mut TestAppContext) -> String {
    pane(s, cx).read_with(cx, |pane, cx| pane.text(cx).to_string())
}

/// Selects `range` in the open note and focuses its editor.
fn select(s: &Setup, cx: &mut TestAppContext, range: std::ops::Range<usize>) {
    let pane = pane(s, cx);
    cx.update_window(s.window, |_, window, cx| {
        pane.update(cx, |pane, cx| pane.select_and_focus(range, window, cx));
    })
    .unwrap();
}

/// Focuses the editor and types `text` at the end of the note.
fn type_at_end(s: &Setup, cx: &mut TestAppContext, text: &str) {
    let end = editor_text(s, cx).len();
    select(s, cx, end..end);
    in_window(s, cx, |window, cx| window.input(text, cx));
}

fn wait(cx: &mut TestAppContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn selecting_a_note_shows_its_text(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    assert_eq!(editor_text(&s, cx), "# Welcome\n");
}

#[gpui_kit::test]
fn typing_saves_after_a_short_pause(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    type_at_end(&s, cx, "Hello");

    wait(cx, 100);
    assert_eq!(
        fs::read_to_string(s.path("Welcome.md")).unwrap(),
        "# Welcome\n"
    );

    wait(cx, 600);
    assert_eq!(
        fs::read_to_string(s.path("Welcome.md")).unwrap(),
        "# Welcome\nHello"
    );
}

#[gpui_kit::test]
fn switching_notes_saves_immediately(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    type_at_end(&s, cx, "Draft");

    // No waiting: switching must not lose the last keystrokes.
    open_note(&s, cx, "Projects/Plan.md");
    assert_eq!(
        fs::read_to_string(s.path("Welcome.md")).unwrap(),
        "# Welcome\nDraft"
    );
    assert_eq!(editor_text(&s, cx), "");
}

#[gpui_kit::test]
fn renaming_the_open_note_keeps_unsaved_typing(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    type_at_end(&s, cx, "Unsaved");

    let old = s.path("Welcome.md");
    s.vault.update(cx, |vault, cx| {
        vault.select(old.clone(), false, cx);
        vault.rename(&old, "Hello", cx).unwrap();
    });
    cx.run_until_parked();
    wait(cx, 600);

    assert!(!old.exists(), "the old file must not be written back");
    assert_eq!(
        fs::read_to_string(s.path("Hello.md")).unwrap(),
        "# Welcome\nUnsaved"
    );
    assert_eq!(editor_text(&s, cx), "# Welcome\nUnsaved");
}

#[gpui_kit::test]
fn a_deleted_note_is_not_written_back(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    type_at_end(&s, cx, "Gone");

    fs::remove_file(s.path("Welcome.md")).unwrap();
    open_note(&s, cx, "Projects/Plan.md");
    wait(cx, 600);

    assert!(!s.path("Welcome.md").exists());
}

#[gpui_kit::test]
fn bold_button_wraps_the_selection(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    select(&s, cx, 2..9); // "Welcome"

    in_window(&s, cx, |window, cx| window.click("format-bold", cx));

    assert_eq!(editor_text(&s, cx), "# **Welcome**\n");
    let selected = pane(&s, cx).read_with(cx, |pane, cx| pane.selected_text(cx));
    assert_eq!(selected, "Welcome", "the original text stays selected");
}

#[gpui_kit::test]
fn shortcuts_switch_view_modes(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    let mode = |cx: &mut TestAppContext| cx.read(|cx| SettingsStore::get(cx).view_mode);

    in_window(&s, cx, |window, cx| window.press("secondary-3", cx));
    assert_eq!(mode(cx), ViewMode::Split);
    in_window(&s, cx, |window, cx| window.press("secondary-4", cx));
    assert_eq!(mode(cx), ViewMode::Preview);
    in_window(&s, cx, |window, cx| window.press("secondary-e", cx));
    assert_eq!(mode(cx), ViewMode::Live, "cycling wraps around to Live");
    in_window(&s, cx, |window, cx| window.press("secondary-2", cx));
    assert_eq!(mode(cx), ViewMode::Source);
    in_window(&s, cx, |window, cx| window.press("secondary-1", cx));
    assert_eq!(mode(cx), ViewMode::Live);
}

#[gpui_kit::test]
fn live_is_the_default_and_switching_modes_never_changes_the_file(cx: &mut TestAppContext) {
    let s = setup(cx);
    let mode = |cx: &mut TestAppContext| cx.read(|cx| SettingsStore::get(cx).view_mode);
    assert_eq!(mode(cx), ViewMode::Live);

    let note = s.path("Rich.md");
    let original = "# Title\n\nSome **bold**, *italic*, `code` and [a link](https://x.dev).\n\n- [ ] task\n> quote\n";
    fs::write(&note, original).unwrap();
    open_note(&s, cx, "Rich.md");
    for key in [
        "secondary-2",
        "secondary-1",
        "secondary-3",
        "secondary-4",
        "secondary-1",
    ] {
        in_window(&s, cx, |window, cx| window.press(key, cx));
    }
    wait(cx, 1000);

    assert_eq!(editor_text(&s, cx), original);
    assert_eq!(
        fs::read_to_string(&note).unwrap(),
        original,
        "styling never writes"
    );
}

#[gpui_kit::test]
fn windows_line_endings_survive_editing(cx: &mut TestAppContext) {
    let s = setup(cx);
    let note = s.path("Crlf.md");
    fs::write(&note, "# Title\r\nBody\r\n").unwrap();
    open_note(&s, cx, "Crlf.md");
    assert_eq!(editor_text(&s, cx), "# Title\nBody\n");

    type_at_end(&s, cx, "More");
    wait(cx, 600);
    assert_eq!(
        fs::read_to_string(&note).unwrap(),
        "# Title\r\nBody\r\nMore"
    );
}

#[gpui_kit::test]
fn character_count_follows_typing(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    let count = |cx: &mut TestAppContext| {
        s.workspace.read_with(cx, |workspace, cx| {
            workspace.editor(cx).unwrap().read(cx).char_count()
        })
    };
    assert_eq!(count(cx), 10); // "# Welcome\n"

    type_at_end(&s, cx, "Hi");
    wait(cx, 50); // the count is still waiting to run...
    type_at_end(&s, cx, "!"); // ...and more typing arrives meanwhile
    wait(cx, 1000);
    assert_eq!(
        count(cx),
        13,
        "the count includes typing that arrived mid-update"
    );
}

#[gpui_kit::test]
fn clicking_a_view_mode_segment_switches_mode(cx: &mut TestAppContext) {
    let s = setup(cx);
    open_note(&s, cx, "Welcome.md");
    let mode = |cx: &mut TestAppContext| cx.read(|cx| SettingsStore::get(cx).view_mode);

    in_window(&s, cx, |window, cx| window.click("Split", cx));
    assert_eq!(mode(cx), ViewMode::Split);
    in_window(&s, cx, |window, cx| window.click("Preview", cx));
    assert_eq!(mode(cx), ViewMode::Preview);
    in_window(&s, cx, |window, cx| window.click("Source", cx));
    assert_eq!(mode(cx), ViewMode::Source);
}

#[gpui_kit::test]
fn live_enter_continues_a_list_and_backspace_and_undo_work(cx: &mut TestAppContext) {
    let s = setup(cx);
    let note = s.path("List.md");
    fs::write(&note, "- one").unwrap();
    open_note(&s, cx, "List.md");

    type_at_end(&s, cx, "");
    in_window(&s, cx, |window, cx| window.press("enter", cx));
    in_window(&s, cx, |window, cx| window.input("two", cx));
    assert_eq!(editor_text(&s, cx), "- one\n- two");

    in_window(&s, cx, |window, cx| window.press("backspace", cx));
    assert_eq!(editor_text(&s, cx), "- one\n- tw");

    in_window(&s, cx, |window, cx| window.press("secondary-z", cx));
    assert_eq!(editor_text(&s, cx), "- one\n- two");
}

#[gpui_kit::test]
fn clicking_a_task_in_the_preview_ticks_it(cx: &mut TestAppContext) {
    let s = setup(cx);
    let note = s.path("Tasks.md");
    fs::write(&note, "# Todo\n- [ ] first\n- [x] second\n").unwrap();
    open_note(&s, cx, "Tasks.md");
    in_window(&s, cx, |window, cx| window.press("secondary-4", cx)); // Preview

    // Each checkbox is named after where its task starts in the note.
    in_window(&s, cx, |window, cx| {
        window.click(("task-checkbox", 7usize), cx)
    });
    assert_eq!(editor_text(&s, cx), "# Todo\n- [x] first\n- [x] second\n");
    wait(cx, 600);
    assert_eq!(
        fs::read_to_string(&note).unwrap(),
        "# Todo\n- [x] first\n- [x] second\n"
    );
}
