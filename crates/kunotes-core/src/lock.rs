//! Locked notes: notes encrypted on disk, so a credential written in a note
//! isn't readable in the vault folder or in its git repository.
//!
//! - A locked note is `Name.md.age`, in the [age](https://age-encryption.org)
//!   format, so it can also be opened without KuNotes (see `docs`).
//! - Each vault has one password. It protects the vault key: a random age key
//!   stored, encrypted with the password, in `.kunotes-lock.age` at the vault
//!   root. Notes are encrypted with that key. Typing the password unlocks the
//!   key once (slow on purpose, so passwords are hard to guess); after that,
//!   opening and saving locked notes is fast.
//! - Forgetting the password means the locked notes can't be opened. Deleting
//!   `.kunotes-lock.age` has the same effect.

use std::fs;
use std::path::{Path, PathBuf};

use age::secrecy::{ExposeSecret as _, SecretString};
use age::x25519;

use crate::error::{CoreError, Result};
use crate::fs_ops::atomic_write;

/// The encrypted vault key, at the vault root (hidden, but synced with git).
pub const KEY_FILE: &str = ".kunotes-lock.age";

/// The extra extension of a locked note: `Name.md` → `Name.md.age`.
const LOCKED_SUFFIX: &str = ".age";

/// Passwords shorter than this are refused when creating the vault password.
pub const MIN_PASSWORD_LEN: usize = 8;

/// The unlocked vault key. Kept only in memory, only while notes are unlocked.
pub struct VaultKey {
    identity: x25519::Identity,
}

impl VaultKey {
    /// Encrypts note text (ASCII-armored, so git sees text, not binary).
    pub fn encrypt(&self, text: &str) -> Result<Vec<u8>> {
        let recipient = self.identity.to_public();
        let armored = age::encrypt_and_armor(&recipient, text.as_bytes())
            .map_err(|error| CoreError::Lock(format!("Couldn't encrypt the note: {error}")))?;
        Ok(armored.into_bytes())
    }

    /// Decrypts a locked note's bytes to its text.
    pub fn decrypt(&self, bytes: &[u8]) -> Result<String> {
        let plain = age::decrypt(&self.identity, bytes).map_err(|_| {
            CoreError::Lock("This note was locked with another vault password.".into())
        })?;
        String::from_utf8(plain)
            .map_err(|_| CoreError::Lock("The unlocked note isn't valid text.".into()))
    }
}

/// True if the vault already has a password (a key file).
pub fn has_password(root: &Path) -> bool {
    root.join(KEY_FILE).exists()
}

/// Sets the vault password: makes a new vault key and saves it encrypted with
/// `password`. Refuses if the vault already has one (that would make its
/// locked notes unreadable).
pub fn create_password(root: &Path, password: &str) -> Result<VaultKey> {
    if has_password(root) {
        return Err(CoreError::Lock("This vault already has a password.".into()));
    }
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(CoreError::Lock(format!(
            "Use at least {MIN_PASSWORD_LEN} characters."
        )));
    }
    let identity = x25519::Identity::generate();
    let recipient = age::scrypt::Recipient::new(SecretString::from(password.to_owned()));
    let key_text = identity.to_string();
    let encrypted = age::encrypt_and_armor(&recipient, key_text.expose_secret().as_bytes())
        .map_err(|error| CoreError::Lock(format!("Couldn't save the vault key: {error}")))?;
    atomic_write(&root.join(KEY_FILE), encrypted.as_bytes())?;
    Ok(VaultKey { identity })
}

/// Unlocks the vault key with `password`. Takes about a second (on purpose).
pub fn unlock(root: &Path, password: &str) -> Result<VaultKey> {
    let bytes = fs::read(root.join(KEY_FILE))?;
    let identity = age::scrypt::Identity::new(SecretString::from(password.to_owned()));
    let key_text =
        age::decrypt(&identity, &bytes).map_err(|_| CoreError::Lock("Wrong password.".into()))?;
    let key_text = String::from_utf8(key_text)
        .map_err(|_| CoreError::Lock("The vault key file is damaged.".into()))?;
    let identity = key_text
        .trim()
        .parse::<x25519::Identity>()
        .map_err(|_| CoreError::Lock("The vault key file is damaged.".into()))?;
    Ok(VaultKey { identity })
}

/// True if `path` is a locked note (`*.md.age`, any letter case).
pub fn is_locked_note(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase().ends_with(".md.age"))
        .unwrap_or(false)
}

/// `Name.md` → `Name.md.age`.
pub fn locked_path(note: &Path) -> PathBuf {
    let mut name = note.file_name().unwrap_or_default().to_os_string();
    name.push(LOCKED_SUFFIX);
    note.with_file_name(name)
}

/// `Name.md.age` → `Name.md`.
pub fn unlocked_path(locked: &Path) -> PathBuf {
    let name = locked.file_name().unwrap_or_default().to_string_lossy();
    let plain = &name[..name.len().saturating_sub(LOCKED_SUFFIX.len())];
    locked.with_file_name(plain)
}

/// Reads a locked note.
pub fn read_note(key: &VaultKey, path: &Path) -> Result<String> {
    key.decrypt(&fs::read(path)?)
}

/// Saves a locked note's text, encrypted.
pub fn write_note(key: &VaultKey, path: &Path, text: &str) -> Result<()> {
    atomic_write(path, &key.encrypt(text)?)
}

/// Locks a note: writes `Name.md.age` (and checks it opens again).
/// Returns its path. The caller then moves `Name.md` to the trash.
pub fn lock_note(key: &VaultKey, note: &Path) -> Result<PathBuf> {
    let locked = locked_path(note);
    if locked.exists() {
        return Err(CoreError::AlreadyExists(locked));
    }
    let text = fs::read_to_string(note)?;
    write_note(key, &locked, &text)?;
    // Check it opens again before giving up the readable copy.
    if read_note(key, &locked)? != text {
        return Err(CoreError::Lock(
            "Locking didn't round-trip; the note was left as it is.".into(),
        ));
    }
    Ok(locked)
}

/// Removes the lock: writes `Name.md`. Returns its path. The caller then
/// moves `Name.md.age` to the trash.
pub fn unlock_note(key: &VaultKey, locked: &Path) -> Result<PathBuf> {
    let note = unlocked_path(locked);
    if note.exists() {
        return Err(CoreError::AlreadyExists(note));
    }
    let text = read_note(key, locked)?;
    atomic_write(&note, text.as_bytes())?;
    Ok(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_names() {
        let note = Path::new("vault").join("Keys.md");
        let locked = locked_path(&note);
        assert_eq!(locked, Path::new("vault").join("Keys.md.age"));
        assert!(is_locked_note(&locked));
        assert!(is_locked_note(Path::new("A.MD.AGE")));
        assert!(!is_locked_note(&note));
        assert!(!is_locked_note(Path::new("photo.age")));
        assert_eq!(unlocked_path(&locked), note);
    }
}
