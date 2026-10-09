//! Git sync against a real (local, bare) remote. Skipped if git isn't installed.

use std::fs;
use std::path::Path;
use std::process::Command;

use kunotes_core::git;

/// A bare repository standing in for GitHub, and its address.
fn remote(dir: &Path) -> String {
    let remote = dir.join("remote.git");
    let status = Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .arg(&remote)
        .output()
        .unwrap()
        .status;
    assert!(status.success());
    remote.to_string_lossy().into_owned()
}

fn vault(dir: &Path, name: &str) -> std::path::PathBuf {
    let vault = dir.join(name);
    fs::create_dir(&vault).unwrap();
    vault
}

#[test]
fn notes_travel_between_two_computers() {
    if !git::is_installed() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let url = remote(dir.path());

    let laptop = vault(dir.path(), "laptop");
    fs::write(laptop.join("Hello.md"), "# Hello\n").unwrap();
    git::connect(&laptop, &url).unwrap();
    assert_eq!(git::remote_url(&laptop).as_deref(), Some(url.as_str()));
    assert!(laptop.join(".gitignore").exists());

    // A second computer with its own note joins and gets both.
    let desktop = vault(dir.path(), "desktop");
    fs::write(desktop.join("Other.md"), "other\n").unwrap();
    let report = git::connect(&desktop, &url).unwrap();
    assert!(desktop.join("Hello.md").exists());
    assert!(report.changed.contains(&desktop.join("Hello.md")));

    // An edit on the desktop reaches the laptop.
    fs::write(desktop.join("Hello.md"), "# Hello\nfrom desktop\n").unwrap();
    git::sync(&desktop).unwrap();
    let report = git::sync(&laptop).unwrap();
    assert_eq!(
        fs::read_to_string(laptop.join("Hello.md")).unwrap(),
        "# Hello\nfrom desktop\n"
    );
    assert!(laptop.join("Other.md").exists());
    assert!(report.changed.contains(&laptop.join("Hello.md")));
}

#[test]
fn a_file_changed_on_both_sides_keeps_both_versions() {
    if !git::is_installed() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let url = remote(dir.path());
    let a = vault(dir.path(), "a");
    fs::write(a.join("Plan.md"), "start\n").unwrap();
    git::connect(&a, &url).unwrap();
    let b = vault(dir.path(), "b");
    git::connect(&b, &url).unwrap();

    fs::write(a.join("Plan.md"), "from a\n").unwrap();
    git::sync(&a).unwrap();
    fs::write(b.join("Plan.md"), "from b\n").unwrap();
    let report = git::sync(&b).unwrap();

    assert_eq!(
        fs::read_to_string(b.join("Plan.md")).unwrap(),
        "from b\n",
        "ours stays"
    );
    assert_eq!(report.conflicts.len(), 1);
    let copy = &report.conflicts[0];
    assert!(
        copy.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("Plan (conflict ")
    );
    assert_eq!(
        fs::read_to_string(copy).unwrap(),
        "from a\n",
        "theirs is kept as a copy"
    );

    // The resolution reaches the other side too.
    git::sync(&a).unwrap();
    assert!(a.join(copy.file_name().unwrap()).exists());
}

#[test]
fn a_bad_address_gives_a_readable_error() {
    if !git::is_installed() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let v = vault(dir.path(), "v");
    let missing = dir.path().join("nope.git").to_string_lossy().into_owned();
    let error = git::connect(&v, &missing).unwrap_err().to_string();
    assert!(
        error.contains("wasn't found") || error.contains("repository"),
        "{error}"
    );
}

#[test]
fn quitting_commits_and_pushes() {
    if !git::is_installed() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let url = remote(dir.path());
    let a = vault(dir.path(), "a");
    git::connect(&a, &url).unwrap();
    fs::write(a.join("Late.md"), "typed just before quitting\n").unwrap();
    git::commit_and_push_quickly(&a, std::time::Duration::from_secs(10)).unwrap();

    let b = vault(dir.path(), "b");
    git::connect(&b, &url).unwrap();
    assert!(b.join("Late.md").exists());
}
