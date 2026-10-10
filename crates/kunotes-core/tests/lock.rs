//! Locked notes on a real disk: password, lock, unlock, wrong password.

use std::fs;

use kunotes_core::lock;

#[test]
fn a_locked_note_is_unreadable_on_disk_and_opens_with_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let note = root.join("Keys.md");
    fs::write(&note, "api_key = sk-日本-🔑\n").unwrap();

    assert!(!lock::has_password(root));
    assert!(lock::create_password(root, "short").is_err(), "too short");
    let key = lock::create_password(root, "correct horse").unwrap();
    assert!(lock::has_password(root));
    assert!(
        lock::create_password(root, "another password").is_err(),
        "only one"
    );

    let locked = lock::lock_note(&key, &note).unwrap();
    assert_eq!(locked, root.join("Keys.md.age"));
    assert!(
        note.exists(),
        "the app moves the readable copy to the trash"
    );
    fs::remove_file(&note).unwrap(); // (the app trashes it)
    let on_disk = fs::read_to_string(&locked).unwrap();
    assert!(on_disk.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));
    assert!(!on_disk.contains("sk-"), "the secret isn't on disk");

    // Later (e.g. after a restart): unlock with the password.
    assert!(lock::unlock(root, "wrong password").is_err());
    let key = lock::unlock(root, "correct horse").unwrap();
    assert_eq!(
        lock::read_note(&key, &locked).unwrap(),
        "api_key = sk-日本-🔑\n"
    );

    lock::write_note(&key, &locked, "changed\n").unwrap();
    assert_eq!(lock::read_note(&key, &locked).unwrap(), "changed\n");

    let back = lock::unlock_note(&key, &locked).unwrap();
    assert_eq!(fs::read_to_string(&back).unwrap(), "changed\n");
}

#[test]
fn a_note_from_another_vault_key_is_refused() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let key_a = lock::create_password(a.path(), "password a").unwrap();
    let key_b = lock::create_password(b.path(), "password b").unwrap();
    let note = a.path().join("N.md");
    fs::write(&note, "x").unwrap();
    let locked = lock::lock_note(&key_a, &note).unwrap();
    assert!(lock::read_note(&key_b, &locked).is_err());
}
