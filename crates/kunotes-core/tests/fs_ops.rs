//! Integration tests for file operations, using real temporary folders.

use std::fs;
use std::path::Path;

use kunotes_core::CoreError;
use kunotes_core::fs_ops::{
    atomic_write, create_file, create_file_named, create_folder, create_folder_named, move_into,
    rename, unique_path,
};
use kunotes_core::names::NameError;
use tempfile::tempdir;

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}

/// Names inside `dir`, sorted, so tests don't depend on directory order.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn unique_path_adds_number_suffix() {
    let dir = tempdir().unwrap();
    assert_eq!(
        unique_path(dir.path(), "Untitled", Some("md")),
        dir.path().join("Untitled.md")
    );

    write(&dir.path().join("Untitled.md"), "");
    write(&dir.path().join("Untitled 2.md"), "");
    assert_eq!(
        unique_path(dir.path(), "Untitled", Some("md")),
        dir.path().join("Untitled 3.md")
    );
}

#[test]
fn create_file_seeds_heading_and_never_overwrites() {
    let dir = tempdir().unwrap();
    let first = create_file(dir.path(), "Untitled").unwrap();
    let second = create_file(dir.path(), "Untitled").unwrap();

    assert_eq!(first.file_name().unwrap(), "Untitled.md");
    assert_eq!(second.file_name().unwrap(), "Untitled 2.md");
    assert_eq!(fs::read_to_string(&second).unwrap(), "# Untitled 2\n");
}

#[test]
fn create_folder_uses_unique_name() {
    let dir = tempdir().unwrap();
    create_folder(dir.path(), "New Folder").unwrap();
    create_folder(dir.path(), "New Folder").unwrap();
    assert_eq!(names_in(dir.path()), ["New Folder", "New Folder 2"]);
}

#[test]
fn rename_file_adds_md_unless_present() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("a.md");
    write(&note, "x");

    let renamed = rename(&note, "  Plan  ").unwrap();
    assert_eq!(renamed, dir.path().join("Plan.md"));

    let renamed = rename(&renamed, "Report.MD").unwrap();
    assert_eq!(renamed, dir.path().join("Report.MD"));

    let renamed = rename(&renamed, "v1.2").unwrap();
    assert_eq!(renamed, dir.path().join("v1.2.md"));
    assert_eq!(fs::read_to_string(&renamed).unwrap(), "x");
}

#[test]
fn rename_folder_keeps_name_as_typed() {
    let dir = tempdir().unwrap();
    let folder = dir.path().join("Old");
    fs::create_dir(&folder).unwrap();
    assert_eq!(rename(&folder, "New").unwrap(), dir.path().join("New"));
}

#[test]
fn rename_to_same_name_is_a_no_op() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("Same.md");
    write(&note, "");
    assert_eq!(rename(&note, "Same").unwrap(), note);
}

#[test]
fn rename_only_changing_case_works() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("note.md");
    write(&note, "keep me");

    let renamed = rename(&note, "Note").unwrap();
    assert_eq!(names_in(dir.path()), ["Note.md"]);
    assert_eq!(fs::read_to_string(renamed).unwrap(), "keep me");
}

#[test]
fn rename_refuses_to_overwrite() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a.md");
    write(&a, "a");
    write(&dir.path().join("b.md"), "b");

    assert!(matches!(rename(&a, "b"), Err(CoreError::AlreadyExists(_))));
    assert_eq!(fs::read_to_string(dir.path().join("b.md")).unwrap(), "b");
}

#[test]
fn rename_rejects_invalid_names() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a.md");
    write(&a, "");

    assert!(matches!(
        rename(&a, "sub/escape"),
        Err(CoreError::InvalidName(NameError::InvalidChar('/')))
    ));
    // Caught even earlier: a leading "." would make the file hidden.
    assert!(matches!(
        rename(&a, "../escape"),
        Err(CoreError::InvalidName(_))
    ));
    assert!(a.exists(), "a rejected rename must leave the file in place");
    assert!(matches!(
        rename(&a, "   "),
        Err(CoreError::InvalidName(NameError::Empty))
    ));
}

#[test]
fn move_into_folder() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("a.md");
    let folder = dir.path().join("Folder");
    write(&note, "");
    fs::create_dir(&folder).unwrap();

    let moved = move_into(&note, &folder).unwrap();
    assert_eq!(moved, folder.join("a.md"));
    assert!(moved.exists());
    assert!(!note.exists());
}

#[test]
fn move_into_current_parent_does_nothing() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("a.md");
    write(&note, "");
    assert_eq!(move_into(&note, dir.path()).unwrap(), note);
}

#[test]
fn move_folder_into_itself_or_descendant_is_refused() {
    let dir = tempdir().unwrap();
    let folder = dir.path().join("A");
    let child = folder.join("B");
    fs::create_dir_all(&child).unwrap();

    assert!(matches!(
        move_into(&folder, &folder),
        Err(CoreError::MoveIntoSelf)
    ));
    assert!(matches!(
        move_into(&folder, &child),
        Err(CoreError::MoveIntoSelf)
    ));
}

#[test]
fn move_refuses_to_overwrite() {
    let dir = tempdir().unwrap();
    let folder = dir.path().join("Folder");
    fs::create_dir(&folder).unwrap();
    write(&dir.path().join("a.md"), "outside");
    write(&folder.join("a.md"), "inside");

    assert!(matches!(
        move_into(&dir.path().join("a.md"), &folder),
        Err(CoreError::AlreadyExists(_))
    ));
    assert_eq!(fs::read_to_string(folder.join("a.md")).unwrap(), "inside");
}

#[test]
fn atomic_write_replaces_content_and_leaves_no_temp_file() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("a.md");
    write(&note, "old");

    atomic_write(&note, "new content 日本".as_bytes()).unwrap();
    assert_eq!(fs::read_to_string(&note).unwrap(), "new content 日本");
    assert_eq!(names_in(dir.path()), ["a.md"]);
}

#[test]
fn atomic_write_creates_missing_file() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("fresh.md");
    atomic_write(&note, b"hello").unwrap();
    assert_eq!(fs::read_to_string(note).unwrap(), "hello");
}

/// Moves a real file to the OS trash. Ignored by default because CI machines
/// may not have a trash folder; run with `cargo test -- --ignored`.
#[test]
#[ignore]
fn trash_moves_file_away() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("trash-me.md");
    write(&note, "");
    kunotes_core::fs_ops::trash(&note).unwrap();
    assert!(!note.exists());
}

#[test]
fn create_file_named_uses_the_typed_name_and_refuses_duplicates() {
    let dir = tempdir().unwrap();
    let note = create_file_named(dir.path(), "  Ideas ").unwrap();
    assert_eq!(note, dir.path().join("Ideas.md"));
    assert_eq!(fs::read_to_string(&note).unwrap(), "# Ideas\n");

    assert_eq!(
        create_file_named(dir.path(), "Report.md").unwrap(),
        dir.path().join("Report.md")
    );
    assert!(matches!(
        create_file_named(dir.path(), "Ideas"),
        Err(CoreError::AlreadyExists(_))
    ));
    assert!(matches!(
        create_file_named(dir.path(), "a/b"),
        Err(CoreError::InvalidName(_))
    ));
}

#[test]
fn create_folder_named_uses_the_typed_name_and_refuses_duplicates() {
    let dir = tempdir().unwrap();
    assert_eq!(
        create_folder_named(dir.path(), "Work").unwrap(),
        dir.path().join("Work")
    );
    assert!(dir.path().join("Work").is_dir());
    assert!(matches!(
        create_folder_named(dir.path(), "Work"),
        Err(CoreError::AlreadyExists(_))
    ));
}

#[test]
fn import_image_copies_next_to_the_note_and_avoids_name_clashes() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Projects");
    std::fs::create_dir(&folder).unwrap();
    let note = folder.join("Plan.md");
    std::fs::write(&note, "# Plan\n").unwrap();
    let picture = dir.path().join("my photo.PNG");
    std::fs::write(&picture, b"png bytes").unwrap();

    let first = kunotes_core::fs_ops::import_image(&note, &picture).unwrap();
    assert_eq!(first, ".img/my-photo.png");
    assert_eq!(
        std::fs::read(folder.join(".img").join("my-photo.png")).unwrap(),
        b"png bytes"
    );
    let second = kunotes_core::fs_ops::import_image(&note, &picture).unwrap();
    assert_eq!(second, ".img/my-photo-2.png");
}

#[test]
fn import_image_refuses_files_that_are_not_images() {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("Note.md");
    let text = dir.path().join("notes.txt");
    std::fs::write(&text, "hi").unwrap();
    assert!(kunotes_core::fs_ops::import_image(&note, &text).is_err());
    assert!(
        !dir.path().join(".img").exists(),
        "no folder is made for nothing"
    );
}
