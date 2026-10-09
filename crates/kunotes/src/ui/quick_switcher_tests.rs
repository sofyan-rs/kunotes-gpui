//! UI tests for the quick switcher. The test vault is described in `ui/test_helpers.rs`.

use std::path::Path;

use gpui_kit::TestAppContext;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;

use crate::ui::test_helpers::{in_window, is_expanded, setup};

#[gpui_kit::test]
fn typing_filters_and_enter_opens_the_note(cx: &mut TestAppContext) {
    let s = setup(cx);

    in_window(&s, cx, |window, cx| window.press("secondary-k", cx));
    in_window(&s, cx, |window, cx| {
        assert!(window.has_active_dialog(cx));
        window.input("plan", cx);
    });
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    in_window(&s, cx, |window, cx| assert!(!window.has_active_dialog(cx)));
    let opened = s
        .vault
        .read_with(cx, |vault, _| vault.selected_file().map(Path::to_path_buf));
    assert_eq!(opened, Some(s.path("Projects/Plan.md")));
    assert!(
        is_expanded(&s, cx, &s.path("Projects")),
        "the note's folder opens so it's visible in the tree"
    );
}

#[gpui_kit::test]
fn escape_closes_without_opening(cx: &mut TestAppContext) {
    let s = setup(cx);

    in_window(&s, cx, |window, cx| window.press("secondary-shift-o", cx));
    in_window(&s, cx, |window, cx| assert!(window.has_active_dialog(cx)));
    in_window(&s, cx, |window, cx| window.press("escape", cx));

    in_window(&s, cx, |window, cx| assert!(!window.has_active_dialog(cx)));
    let opened = s
        .vault
        .read_with(cx, |vault, _| vault.selected_file().is_some());
    assert!(!opened);
}

#[gpui_kit::test]
fn arrow_keys_choose_which_note_opens(cx: &mut TestAppContext) {
    let s = setup(cx);

    // "md" matches every note's path; the list is in tree order:
    // Archive/old.md, Projects/Plan.md, Welcome.md.
    in_window(&s, cx, |window, cx| window.press("secondary-k", cx));
    in_window(&s, cx, |window, cx| window.input("md", cx));
    in_window(&s, cx, |window, cx| window.press("down", cx));
    in_window(&s, cx, |window, cx| window.press("down", cx));
    in_window(&s, cx, |window, cx| window.press("down", cx)); // stays on the last row
    in_window(&s, cx, |window, cx| window.press("up", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    let opened = s
        .vault
        .read_with(cx, |vault, _| vault.selected_file().map(Path::to_path_buf));
    assert_eq!(opened, Some(s.path("Projects/Plan.md")));
}
