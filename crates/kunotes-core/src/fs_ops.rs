//! File operations on the vault: create, rename, move, trash, and safe saving.
//!
//! Safety rules:
//! - Every write to a note goes through `atomic_write`, so a crash can't leave
//!   a half-written file.
//! - Deleting always moves to the OS trash. Nothing here deletes permanently.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::names::validate_name;
use crate::paths::note_title;

/// A free path in `dir`: `base.ext`, then `base 2.ext`, `base 3.ext`, ...
pub fn unique_path(dir: &Path, base: &str, extension: Option<&str>) -> PathBuf {
    let with_extension = |name: String| match extension {
        Some(ext) => format!("{name}.{ext}"),
        None => name,
    };

    let mut candidate = dir.join(with_extension(base.to_string()));
    let mut suffix = 2;
    while exists(&candidate) {
        candidate = dir.join(with_extension(format!("{base} {suffix}")));
        suffix += 1;
    }
    candidate
}

/// Creates a new note in `dir` named `base.md` (or `base 2.md`, ...),
/// starting with a heading that matches its name. Returns the new path.
pub fn create_file(dir: &Path, base: &str) -> Result<PathBuf> {
    validate_name(base)?;
    let path = unique_path(dir, base, Some("md"));
    let seed = format!("# {}\n", note_title(&path));

    // `create_new` fails instead of overwriting if the file appeared in the meantime.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(seed.as_bytes())?;
    Ok(path)
}

/// Creates a note with exactly the name the user typed (`.md` is added unless it's
/// already there), starting with a matching heading. Fails if the name is taken.
pub fn create_file_named(dir: &Path, name: &str) -> Result<PathBuf> {
    let mut name = name.trim().to_string();
    validate_name(&name)?;
    if !name.to_lowercase().ends_with(".md") {
        name.push_str(".md");
    }
    let path = dir.join(&name);
    if exists(&path) {
        return Err(CoreError::AlreadyExists(path));
    }
    let seed = format!("# {}\n", note_title(&path));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(seed.as_bytes())?;
    Ok(path)
}

/// Creates a folder with exactly the name the user typed. Fails if the name is taken.
pub fn create_folder_named(dir: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    validate_name(name)?;
    let path = dir.join(name);
    if exists(&path) {
        return Err(CoreError::AlreadyExists(path));
    }
    fs::create_dir(&path)?;
    Ok(path)
}

/// Creates a new folder in `dir` named `base` (or `base 2`, ...). Returns the new path.
pub fn create_folder(dir: &Path, base: &str) -> Result<PathBuf> {
    validate_name(base)?;
    let path = unique_path(dir, base, None);
    fs::create_dir(&path)?;
    Ok(path)
}

/// Renames a file or folder in place. Returns the new path.
///
/// - Surrounding whitespace in `new_name` is trimmed.
/// - For files, `.md` is added unless the name already ends with `.md`.
/// - Renaming only the letter case (`note.md` -> `Note.md`) works even on
///   case-insensitive filesystems (the macOS and Windows default).
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf> {
    let mut name = new_name.trim().to_string();
    validate_name(&name)?;
    if !path.is_dir() && !name.to_lowercase().ends_with(".md") {
        name.push_str(".md");
    }

    let parent = path.parent().unwrap_or(Path::new(""));
    let destination = parent.join(&name);
    if destination == path {
        return Ok(destination);
    }

    if is_case_only_change(path, &destination) {
        // On a case-sensitive disk (Linux) "Note.md" may be a different file.
        if exact_name_exists(parent, &name) {
            return Err(CoreError::AlreadyExists(destination));
        }
        // Go through a temporary name: on a case-insensitive disk the direct
        // rename target "already exists" (it is the same file).
        let temp = parent.join(format!(".kunotes-rename-{}", std::process::id()));
        fs::rename(path, &temp)?;
        fs::rename(&temp, &destination)?;
        return Ok(destination);
    }

    if exists(&destination) {
        return Err(CoreError::AlreadyExists(destination));
    }
    fs::rename(path, &destination)?;
    Ok(destination)
}

/// Moves a file or folder into `folder`, keeping its name. Returns the new path.
///
/// Moving a folder into itself or one of its own subfolders is refused.
/// Moving something into the folder it is already in does nothing.
pub fn move_into(path: &Path, folder: &Path) -> Result<PathBuf> {
    if folder.starts_with(path) {
        return Err(CoreError::MoveIntoSelf);
    }
    if path.parent() == Some(folder) {
        return Ok(path.to_path_buf());
    }

    let Some(name) = path.file_name() else {
        return Err(CoreError::MoveIntoSelf);
    };
    let destination = folder.join(name);
    if exists(&destination) {
        return Err(CoreError::AlreadyExists(destination));
    }
    fs::rename(path, &destination)?;
    Ok(destination)
}

/// Moves a file or folder to the OS trash (Trash / Recycle Bin). Never deletes permanently.
pub fn trash(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|error| CoreError::Trash(error.to_string()))
}

/// Writes `contents` to `path` without ever leaving a half-written file.
///
/// It writes a hidden temporary file next to the target, flushes it to disk,
/// then renames it over the target in one step.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new(""));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Starts with "." so the tree scanner never shows it.
    let temp = parent.join(format!(".{file_name}.kunotes.tmp"));

    let result = write_and_sync(&temp, contents).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        // Best effort cleanup; the original error is what matters.
        let _ = fs::remove_file(&temp);
    }
    Ok(result?)
}

fn write_and_sync(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

/// True if something (file, folder, or even a broken symlink) is at `path`.
fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// True if `dir` contains an entry named exactly `name` (same letter case).
fn exact_name_exists(dir: &Path, name: &str) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.file_name() == name)
        })
        .unwrap_or(false)
}

/// True if `from` and `to` only differ in the letter case of the file name.
fn is_case_only_change(from: &Path, to: &Path) -> bool {
    let (Some(a), Some(b)) = (from.file_name(), to.file_name()) else {
        return false;
    };
    let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
    a != b && a.to_lowercase() == b.to_lowercase()
}
