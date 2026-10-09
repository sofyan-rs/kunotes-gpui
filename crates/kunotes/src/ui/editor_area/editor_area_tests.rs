//! UI tests for tabs: preview tabs, keeping tabs open, closing, pinning,
//! reordering, following renames, and restoring tabs after a relaunch.
//! The test vault is described in `ui/test_helpers.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use super::tab_bar::tab_id;
use crate::ui::sidebar::row_id_for_tests as row_id;
use crate::ui::test_helpers::{Setup, in_window, setup};

/// The open tabs as `name`, `name*` (pinned), or `(name)` (preview), plus the active one.
fn tabs(s: &Setup, cx: &mut TestAppContext) -> (Vec<String>, Option<String>) {
    s.workspace.read_with(cx, |workspace, cx| {
        let area = workspace.editor_area();
        let list = area.read(cx).tab_list();
        let label = |path: &Path| path.file_stem().unwrap().to_string_lossy().into_owned();
        let names = list
            .tabs()
            .iter()
            .map(|tab| {
                let mut name = label(&tab.path);
                if tab.pinned {
                    name.push('*');
                }
                if tab.preview {
                    name = format!("({name})");
                }
                name
            })
            .collect();
        (names, list.active().map(|tab| label(&tab.path)))
    })
}

fn select(s: &Setup, cx: &mut TestAppContext, relative: &str) {
    let path = s.path(relative);
    s.vault
        .update(cx, |vault, cx| vault.select(path, false, cx));
    cx.run_until_parked();
}

fn keep_open(s: &Setup, cx: &mut TestAppContext, relative: &str) {
    let path = s.path(relative);
    s.vault.update(cx, |vault, cx| vault.open_note(path, cx));
    cx.run_until_parked();
}

fn type_in_active_editor(s: &Setup, cx: &mut TestAppContext, text: &str) {
    let state = s.workspace.read_with(cx, |workspace, cx| {
        workspace.editor(cx).unwrap().read(cx).editor_state()
    });
    cx.update_window(s.window, |_, window, cx| {
        state.update(cx, |state, cx| {
            let end = state.value().len();
            state.set_selected_range(end..end, cx);
            state.focus(window, cx);
        });
    })
    .unwrap();
    in_window(s, cx, |window, cx| window.input(text, cx));
}

#[gpui_kit::test]
fn clicking_notes_reuses_one_preview_tab(cx: &mut TestAppContext) {
    let s = setup(cx);
    select(&s, cx, "Welcome.md");
    assert_eq!(
        tabs(&s, cx),
        (vec!["(Welcome)".into()], Some("Welcome".into()))
    );

    select(&s, cx, "Projects/Plan.md");
    assert_eq!(tabs(&s, cx), (vec!["(Plan)".into()], Some("Plan".into())));
}

#[gpui_kit::test]
fn editing_or_double_clicking_keeps_a_tab_open(cx: &mut TestAppContext) {
    let s = setup(cx);
    select(&s, cx, "Welcome.md");
    type_in_active_editor(&s, cx, "x");
    select(&s, cx, "Projects/Plan.md");
    assert_eq!(tabs(&s, cx).0, ["Welcome", "(Plan)"]);

    // Double-clicking a note in the tree keeps its tab too.
    in_window(&s, cx, |window, cx| {
        window.double_click(row_id(&s.path("Projects")), cx)
    });
    in_window(&s, cx, |window, cx| {
        window.double_click(row_id(&s.path("Projects/Plan.md")), cx)
    });
    assert_eq!(tabs(&s, cx).0, ["Welcome", "Plan"]);
}

#[gpui_kit::test]
fn switching_tabs_saves_the_one_you_leave(cx: &mut TestAppContext) {
    let s = setup(cx);
    keep_open(&s, cx, "Welcome.md");
    type_in_active_editor(&s, cx, "Draft");
    keep_open(&s, cx, "Projects/Plan.md"); // no waiting for autosave
    assert_eq!(
        fs::read_to_string(s.path("Welcome.md")).unwrap(),
        "# Welcome\nDraft"
    );
}

#[gpui_kit::test]
fn close_tab_shortcut_saves_and_closes(cx: &mut TestAppContext) {
    let s = setup(cx);
    keep_open(&s, cx, "Welcome.md");
    type_in_active_editor(&s, cx, "Bye");
    in_window(&s, cx, |window, cx| window.press("secondary-w", cx));

    assert_eq!(tabs(&s, cx), (vec![], None));
    assert_eq!(
        fs::read_to_string(s.path("Welcome.md")).unwrap(),
        "# Welcome\nBye"
    );
    let open = s
        .vault
        .read_with(cx, |vault, _| vault.selected_file().map(Path::to_path_buf));
    assert_eq!(open, None, "nothing is shown once the last tab closes");
}

#[gpui_kit::test]
fn clicking_a_tab_activates_it_and_selects_it_in_the_tree(cx: &mut TestAppContext) {
    let s = setup(cx);
    keep_open(&s, cx, "Welcome.md");
    keep_open(&s, cx, "Projects/Plan.md");
    in_window(&s, cx, |window, cx| {
        window.click(tab_id(&s.path("Welcome.md")), cx)
    });

    assert_eq!(tabs(&s, cx).1.as_deref(), Some("Welcome"));
    let selected = s
        .vault
        .read_with(cx, |vault, _| vault.selected_file().map(Path::to_path_buf));
    assert_eq!(selected, Some(s.path("Welcome.md")));
}

#[gpui_kit::test]
fn pinned_tabs_survive_close_others(cx: &mut TestAppContext) {
    let s = setup(cx);
    keep_open(&s, cx, "Welcome.md");
    keep_open(&s, cx, "Projects/Plan.md");
    keep_open(&s, cx, "Projects/Archive/old.md");

    // Pin "Welcome" (via its tab), then close the others from "old".
    in_window(&s, cx, |window, cx| {
        window.click(tab_id(&s.path("Welcome.md")), cx)
    });
    let area = s
        .workspace
        .read_with(cx, |workspace, _| workspace.editor_area());
    cx.update_window(s.window, |_, window, cx| {
        area.update(cx, |area, cx| area.toggle_pin(window, cx)); // the tab menu's "Pin"
    })
    .unwrap();
    cx.run_until_parked();
    in_window(&s, cx, |window, cx| {
        window.click(tab_id(&s.path("Projects/Archive/old.md")), cx)
    });
    in_window(&s, cx, |window, cx| window.press("secondary-alt-t", cx));

    assert_eq!(
        tabs(&s, cx),
        (vec!["Welcome*".into(), "old".into()], Some("old".into()))
    );
}

#[gpui_kit::test]
fn dragging_a_tab_reorders_it(cx: &mut TestAppContext) {
    let s = setup(cx);
    keep_open(&s, cx, "Welcome.md");
    keep_open(&s, cx, "Projects/Plan.md");
    keep_open(&s, cx, "Projects/Archive/old.md");
    assert_eq!(tabs(&s, cx).0, ["Welcome", "Plan", "old"]);

    in_window(&s, cx, |window, cx| {
        window.drag_to(
            tab_id(&s.path("Projects/Archive/old.md")),
            tab_id(&s.path("Welcome.md")),
            cx,
        )
    });
    assert_eq!(tabs(&s, cx).0, ["old", "Welcome", "Plan"]);
}

#[gpui_kit::test]
fn renaming_a_note_in_a_background_tab_updates_the_tab(cx: &mut TestAppContext) {
    let s = setup(cx);
    keep_open(&s, cx, "Welcome.md");
    keep_open(&s, cx, "Projects/Plan.md");
    let welcome = s.path("Welcome.md");
    s.vault
        .update(cx, |vault, cx| vault.rename(&welcome, "Hello", cx).unwrap());
    cx.run_until_parked();

    assert_eq!(tabs(&s, cx).0, ["Hello", "Plan"]);
}

#[gpui_kit::test]
fn tabs_come_back_after_a_relaunch(cx: &mut TestAppContext) {
    let s = setup(cx);
    keep_open(&s, cx, "Welcome.md");
    keep_open(&s, cx, "Projects/Plan.md");
    in_window(&s, cx, |window, cx| {
        window.click(tab_id(&s.path("Welcome.md")), cx)
    });

    // A second workspace reads the same (in-memory) settings, like a relaunch.
    let root: PathBuf = s.dir.path().to_path_buf();
    let (_, relaunched) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| crate::ui::workspace::Workspace::new(window, cx))
        })
        .unwrap()
    });
    cx.run_until_parked();

    let restored = relaunched.read_with(cx, |workspace, cx| {
        let area = workspace.editor_area();
        let list = area.read(cx).tab_list();
        (
            list.tabs()
                .iter()
                .map(|tab| tab.path.clone())
                .collect::<Vec<_>>(),
            list.active().map(|tab| tab.path.clone()),
        )
    });
    assert_eq!(
        restored.0,
        [
            root.join("Welcome.md"),
            root.join("Projects").join("Plan.md")
        ]
    );
    assert_eq!(restored.1, Some(root.join("Welcome.md")));
}
