//! Integration tests for app updates: swapping the app's files and checksums,
//! using real temporary folders. (No network, except the ignored test.)

use std::fs;

use kunotes_core::update::{
    clean_up_after_update, fetch_latest_release, old_exe_path, replace_with, sha256_of,
};
use tempfile::tempdir;

#[test]
fn replace_moves_the_old_version_aside() {
    let dir = tempdir().unwrap();
    let (target, new, old) = (
        dir.path().join("KuNotes.exe"),
        dir.path().join("new.exe"),
        dir.path().join("KuNotes.exe.old"),
    );
    fs::write(&target, "v1").unwrap();
    fs::write(&new, "v2").unwrap();

    replace_with(&target, &new, &old).unwrap();

    assert_eq!(fs::read_to_string(&target).unwrap(), "v2");
    assert_eq!(fs::read_to_string(&old).unwrap(), "v1");
    assert!(!new.exists());
}

#[test]
fn replace_puts_the_original_back_when_it_fails() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("KuNotes.app");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("Info.plist"), "v1").unwrap();
    let missing = dir.path().join("missing.app");

    assert!(replace_with(&target, &missing, &dir.path().join("old.app")).is_err());
    assert_eq!(fs::read_to_string(target.join("Info.plist")).unwrap(), "v1");
}

#[test]
fn checksum_is_sha256_hex() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("package.zip");
    fs::write(&file, "abc").unwrap();
    assert_eq!(
        sha256_of(&file).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn clean_up_removes_the_old_exe_only() {
    let dir = tempdir().unwrap();
    let exe = dir.path().join("KuNotes.exe");
    fs::write(&exe, "v2").unwrap();
    fs::write(old_exe_path(&exe), "v1").unwrap();

    clean_up_after_update(&exe).unwrap();
    clean_up_after_update(&exe).unwrap(); // nothing left: still fine

    assert!(exe.exists());
    assert!(!old_exe_path(&exe).exists());
}

/// Talks to GitHub: `cargo test -p kunotes-core --test update -- --ignored`.
#[test]
#[ignore]
fn latest_release_from_github() {
    match fetch_latest_release().unwrap() {
        Some(release) => println!("latest: {} {}", release.version, release.page_url),
        None => println!("no releases yet"),
    }
}
