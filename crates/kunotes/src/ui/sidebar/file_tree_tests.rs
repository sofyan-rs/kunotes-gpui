//! UI tests for the file tree, run in a headless window with real clicks,
//! key presses, and drags. The test vault is described in `ui/test_helpers.rs`.

use std::fs;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, point, px,
};

use super::file_tree::{icon_id, row_id};
use std::path::Path;

use crate::ui::test_helpers::{Setup, in_window, is_expanded, selected, setup};

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

#[gpui_kit::test]
fn dropping_a_note_on_its_own_folder_keeps_it_there(cx: &mut TestAppContext) {
    let s = setup(cx);
    let projects = s.path("Projects");
    let plan = s.path("Projects/Plan.md");

    in_window(&s, cx, |window, cx| {
        window.double_click(row_id(&projects), cx)
    });
    in_window(&s, cx, |window, cx| {
        window.drag_to(row_id(&plan), row_id(&projects), cx)
    });

    assert!(plan.exists(), "must not fall through to the vault root");
    assert!(!s.path("Plan.md").exists());
}

#[gpui_kit::test]
fn dropping_on_a_note_does_nothing(cx: &mut TestAppContext) {
    let s = setup(cx);
    let projects = s.path("Projects");
    let plan = s.path("Projects/Plan.md");

    in_window(&s, cx, |window, cx| {
        window.double_click(row_id(&projects), cx)
    });
    in_window(&s, cx, |window, cx| {
        window.drag_to(row_id(&plan), row_id(&s.path("Welcome.md")), cx)
    });

    assert!(plan.exists());
    assert!(!s.path("Plan.md").exists());
}

/// True while the inline name field is shown somewhere in the tree.
fn name_field_open(s: &Setup, cx: &mut TestAppContext) -> bool {
    let mut open = false;
    in_window(s, cx, |window, _| {
        open = window.try_find("tree-name-input").is_some()
    });
    open
}

#[gpui_kit::test]
fn f2_renames_inline_keeping_the_extension(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");

    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    in_window(&s, cx, |window, cx| window.press("f2", cx));
    assert!(name_field_open(&s, cx));
    in_window(&s, cx, |window, cx| window.input("Hello", cx)); // replaces "Welcome", keeps ".md"
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    assert!(!name_field_open(&s, cx));
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
fn invalid_name_keeps_the_field_open(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");

    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    in_window(&s, cx, |window, cx| window.press("f2", cx));
    in_window(&s, cx, |window, cx| window.input("a/b", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    assert!(name_field_open(&s, cx), "the user can fix the name");
    assert!(welcome.exists());
}

#[gpui_kit::test]
fn escape_cancels_a_rename(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");

    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    in_window(&s, cx, |window, cx| window.press("f2", cx));
    in_window(&s, cx, |window, cx| window.input("Changed", cx));
    in_window(&s, cx, |window, cx| window.press("escape", cx));

    assert!(!name_field_open(&s, cx));
    assert!(welcome.exists());
    assert!(!s.path("Changed.md").exists());
}

#[gpui_kit::test]
fn new_note_is_named_inline_in_the_selected_folder(cx: &mut TestAppContext) {
    let s = setup(cx);
    let projects = s.path("Projects");

    in_window(&s, cx, |window, cx| window.click(row_id(&projects), cx));
    in_window(&s, cx, |window, cx| window.press("secondary-n", cx));
    assert!(name_field_open(&s, cx));
    assert!(is_expanded(&s, cx, &projects), "the target folder opens");
    in_window(&s, cx, |window, cx| window.input("Ideas", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    let note = s.path("Projects/Ideas.md");
    assert_eq!(fs::read_to_string(&note).unwrap(), "# Ideas\n");
    let opened = s
        .vault
        .read_with(cx, |vault, _| vault.selected_file().map(Path::to_path_buf));
    assert_eq!(opened, Some(note), "the new note opens in the editor");
}

#[gpui_kit::test]
fn new_folder_goes_next_to_the_selected_note(cx: &mut TestAppContext) {
    let s = setup(cx);
    let projects = s.path("Projects");

    in_window(&s, cx, |window, cx| {
        window.double_click(row_id(&projects), cx)
    });
    in_window(&s, cx, |window, cx| {
        window.click(row_id(&s.path("Projects/Plan.md")), cx)
    });
    in_window(&s, cx, |window, cx| window.press("secondary-shift-n", cx));
    in_window(&s, cx, |window, cx| window.input("Drafts", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    assert!(s.path("Projects/Drafts").is_dir());
}

#[gpui_kit::test]
fn a_taken_name_is_refused(cx: &mut TestAppContext) {
    let s = setup(cx);

    in_window(&s, cx, |window, cx| window.press("secondary-n", cx));
    in_window(&s, cx, |window, cx| window.input("Welcome", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    assert!(name_field_open(&s, cx));
    assert_eq!(
        fs::read_to_string(s.path("Welcome.md")).unwrap(),
        "# Welcome\n",
        "the existing note is untouched"
    );
}

#[gpui_kit::test]
fn clicking_elsewhere_confirms_and_an_empty_name_cancels(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");

    in_window(&s, cx, |window, cx| window.press("secondary-n", cx));
    in_window(&s, cx, |window, cx| window.input("Later", cx));
    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    // GPUI reports focus loss when the next frame is drawn; draw it.
    in_window(&s, cx, |_, _| {});
    assert!(
        s.path("Later.md").exists(),
        "focus leaving the field confirms it"
    );

    in_window(&s, cx, |window, cx| window.press("secondary-n", cx));
    in_window(&s, cx, |window, cx| window.click(row_id(&welcome), cx));
    in_window(&s, cx, |_, _| {});
    assert!(!name_field_open(&s, cx), "an empty field just closes");
}

#[gpui_kit::test]
fn a_long_name_does_not_shift_its_row(cx: &mut TestAppContext) {
    let s = setup(cx);
    let long = s.path(&format!("{}.md", "Very long note name ".repeat(8).trim()));
    fs::write(&long, "").unwrap();
    s.vault.update(cx, |vault, cx| vault.refresh(cx));
    cx.run_until_parked();

    in_window(&s, cx, |window, _| {
        let long_icon = window.find(icon_id(&long)).bounds();
        let short_icon = window.find(icon_id(&s.path("Welcome.md"))).bounds();
        assert_eq!(long_icon.origin.x, short_icon.origin.x, "icons line up");
        assert_eq!(long_icon.size, short_icon.size, "the icon keeps its size");
    });
}

/// Right-clicks the empty space at the bottom of the tree.
fn right_click_empty_space(s: &Setup, cx: &mut TestAppContext) {
    in_window(s, cx, |window, cx| {
        let bounds = window.find("file-tree-background").bounds();
        let position = point(bounds.left() + px(20.), bounds.bottom() - px(20.));
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Right,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Right,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    });
    cx.run_until_parked();
}

/// True if the empty-space (vault root) menu is open.
fn root_menu_open(s: &Setup, cx: &mut TestAppContext) -> bool {
    let mut open = false;
    in_window(s, cx, |window, _| {
        open = window
            .within("file-tree-background")
            .try_find("popup-menu")
            .is_some();
    });
    open
}

#[gpui_kit::test]
fn right_click_on_empty_space_makes_a_note_in_the_root(cx: &mut TestAppContext) {
    let s = setup(cx);
    // Something inside a folder is selected; the new note still goes in the root.
    in_window(&s, cx, |window, cx| {
        window.click(row_id(&s.path("Projects")), cx)
    });

    right_click_empty_space(&s, cx);
    assert!(root_menu_open(&s, cx));
    // "New Note" is the first item: pick it with the keyboard.
    in_window(&s, cx, |window, cx| window.press("down", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));
    cx.run_until_parked();
    assert!(name_field_open(&s, cx));
    in_window(&s, cx, |window, cx| window.input("Root Note", cx));
    in_window(&s, cx, |window, cx| window.press("enter", cx));

    assert!(s.path("Root Note.md").exists());
}

#[gpui_kit::test]
fn right_click_on_a_row_opens_only_the_row_menu(cx: &mut TestAppContext) {
    let s = setup(cx);
    let welcome = s.path("Welcome.md");
    in_window(&s, cx, |window, cx| {
        window.right_click(row_id(&welcome), cx)
    });
    cx.run_until_parked();

    let mut row_menu = false;
    in_window(&s, cx, |window, _| {
        row_menu = window
            .within(row_id(&welcome))
            .try_find("popup-menu")
            .is_some();
    });
    assert!(row_menu, "the row's menu opens");
    assert!(!root_menu_open(&s, cx), "the empty-space menu stays closed");
}

#[gpui_kit::test]
fn clicking_empty_space_clears_the_selection(cx: &mut TestAppContext) {
    let s = setup(cx);
    let projects = s.path("Projects");
    in_window(&s, cx, |window, cx| window.click(row_id(&projects), cx));
    assert_eq!(selected(&s, cx), Some(projects));

    in_window(&s, cx, |window, cx| {
        let bounds = window.find("file-tree-background").bounds();
        let empty = point(bounds.left() + px(20.), bounds.bottom() - px(20.));
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: empty,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
    });
    assert_eq!(selected(&s, cx), None);
}
