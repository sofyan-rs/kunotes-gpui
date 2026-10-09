//! UI tests for the file tree, run in a headless window with real clicks,
//! key presses, and drags. The test vault is described in `ui/test_helpers.rs`.

use std::fs;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px};

use super::file_tree::row_id;
use crate::ui::test_helpers::{in_window, is_expanded, selected, setup};

#[gpui_kit::test]
fn click_selects_and_double_click_expands(cx: &mut TestAppContext) {
    let s = setup(cx);
    let projects = s.path("Projects");

    in_window(&s, cx, |window, cx| window.click(row_id(&projects), cx));
    assert_eq!(selected(&s, cx), Some(projects.clone()));
    assert!(
        !is_expanded(&s, cx, &projects),
        "a single click only selects"
    );

    in_window(&s, cx, |window, cx| {
        window.double_click(row_id(&projects), cx)
    });
    assert!(is_expanded(&s, cx, &projects));
    in_window(&s, cx, |window, _| {
        assert!(
            window
                .try_find(row_id(&s.path("Projects/Plan.md")))
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn arrow_keys_navigate_expand_and_collapse(cx: &mut TestAppContext) {
    let s = setup(cx);
    let projects = s.path("Projects");

    // Rows: Projects, Welcome.md. Clicking also focuses the tree.
    in_window(&s, cx, |window, cx| {
        window.click(row_id(&s.path("Welcome.md")), cx)
    });
    in_window(&s, cx, |window, cx| window.press("up", cx));
    assert_eq!(selected(&s, cx), Some(projects.clone()));

    in_window(&s, cx, |window, cx| window.press("right", cx));
    assert!(is_expanded(&s, cx, &projects));

    // Expanded: Projects, Archive, Plan.md, Welcome.md (folders first).
    in_window(&s, cx, |window, cx| window.press("down", cx));
    assert_eq!(selected(&s, cx), Some(s.path("Projects/Archive")));

    in_window(&s, cx, |window, cx| window.press("left", cx));
    assert_eq!(
        selected(&s, cx),
        Some(projects.clone()),
        "left jumps to the parent"
    );

    in_window(&s, cx, |window, cx| window.press("left", cx));
    assert!(
        !is_expanded(&s, cx, &projects),
        "left on an open folder collapses it"
    );
}

#[gpui_kit::test]
fn dragging_a_note_onto_a_folder_moves_it(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");
    let projects = s.path("Projects");

    in_window(&s, cx, |window, cx| {
        window.drag_to(row_id(&welcome), row_id(&projects), cx)
    });

    assert!(!welcome.exists());
    assert!(s.path("Projects/Welcome.md").exists());
}

#[gpui_kit::test]
fn f2_renames_the_selected_note(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");

    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    in_window(&s, cx, |window, cx| window.press("f2", cx));
    in_window(&s, cx, |window, cx| {
        assert!(window.has_active_dialog(cx));
        window.input("Hello", cx); // replaces the pre-selected name
    });
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    assert!(!welcome.exists());
    let renamed = s.path("Hello.md");
    assert_eq!(fs::read_to_string(&renamed).unwrap(), "# Welcome\n");
    assert_eq!(
        selected(&s, cx),
        Some(renamed),
        "selection follows the rename"
    );
}

#[gpui_kit::test]
fn backspace_asks_before_trashing_and_escape_cancels(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");

    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    in_window(&s, cx, |window, cx| window.press("backspace", cx));
    in_window(&s, cx, |window, cx| {
        assert!(window.has_active_dialog(cx), "delete must ask first");
        window.press("escape", cx);
    });
    in_window(&s, cx, |window, cx| assert!(!window.has_active_dialog(cx)));

    assert!(welcome.exists(), "cancel keeps the file");
}

#[gpui_kit::test]
fn invalid_rename_keeps_the_dialog_open(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");

    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    in_window(&s, cx, |window, cx| window.press("f2", cx));
    in_window(&s, cx, |window, cx| window.input("a/b", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    in_window(&s, cx, |window, cx| {
        assert!(window.has_active_dialog(cx), "the user can fix the name");
    });
    assert!(welcome.exists());
}

#[gpui_kit::test]
fn dropping_on_empty_space_moves_to_the_vault_root(cx: &mut TestAppContext) {
    let s = setup(cx);
    let plan = s.path("Projects/Plan.md");
    let projects = s.path("Projects");

    in_window(&s, cx, |window, cx| {
        window.double_click(row_id(&projects), cx)
    });
    in_window(&s, cx, |window, cx| {
        let from = window.find(row_id(&plan)).bounds().center();
        // Well below the last row, still inside the tree.
        let to = gpui_kit::point(from.x, px(500.));
        window.drag(from, to, cx);
    });

    assert!(!plan.exists());
    assert!(s.path("Plan.md").exists());
}
