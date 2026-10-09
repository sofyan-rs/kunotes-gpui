//! Filename validation that works for macOS, Windows, and Linux at once.
//!
//! Vaults are often synced between computers (git, Dropbox, ...), so we apply
//! the strictest rules (Windows) everywhere instead of only on Windows.

use thiserror::Error;

/// Why a file or folder name was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NameError {
    #[error("the name can't be empty")]
    Empty,

    #[error("\".\" and \"..\" can't be used as names")]
    DotName,

    #[error("names starting with \".\" are hidden")]
    Hidden,

    #[error("the name can't contain '{0}'")]
    InvalidChar(char),

    #[error("the name can't contain control characters")]
    ControlChar,

    #[error("\"{0}\" is a reserved name on Windows")]
    Reserved(String),

    #[error("the name can't end with a space or a dot")]
    TrailingSpaceOrDot,
}

/// Characters that are not allowed in names on Windows (plus both path separators).
const INVALID_CHARS: [char; 9] = ['/', '\\', '<', '>', ':', '"', '|', '?', '*'];

/// Device names that Windows reserves, with or without an extension.
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Checks that `name` is safe to use as a single file or folder name.
///
/// The caller should trim whitespace first; this function checks the name as given.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameError::DotName);
    }
    if name.starts_with('.') {
        return Err(NameError::Hidden);
    }
    for c in name.chars() {
        if INVALID_CHARS.contains(&c) {
            return Err(NameError::InvalidChar(c));
        }
        if c.is_control() {
            return Err(NameError::ControlChar);
        }
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Err(NameError::TrailingSpaceOrDot);
    }

    // "CON.md" is just as reserved as "CON", so only look at the part before the first dot.
    let stem = name.split('.').next().unwrap_or(name);
    if RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        return Err(NameError::Reserved(stem.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_names() {
        for name in [
            "Note",
            "My Note.md",
            "2026-10-09",
            "日本語",
            "café notes",
            "a.b.c",
        ] {
            assert_eq!(validate_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(validate_name("   "), Err(NameError::Empty));
    }

    #[test]
    fn rejects_dot_names_and_hidden_names() {
        assert_eq!(validate_name("."), Err(NameError::DotName));
        assert_eq!(validate_name(".."), Err(NameError::DotName));
        assert_eq!(validate_name(".secret"), Err(NameError::Hidden));
    }

    #[test]
    fn rejects_path_separators_and_windows_chars() {
        for c in INVALID_CHARS {
            let name = format!("a{c}b");
            assert_eq!(validate_name(&name), Err(NameError::InvalidChar(c)));
        }
    }

    #[test]
    fn rejects_control_characters() {
        assert_eq!(validate_name("a\tb"), Err(NameError::ControlChar));
        assert_eq!(validate_name("a\nb"), Err(NameError::ControlChar));
    }

    #[test]
    fn rejects_trailing_space_or_dot() {
        assert_eq!(validate_name("note "), Err(NameError::TrailingSpaceOrDot));
        assert_eq!(validate_name("note."), Err(NameError::TrailingSpaceOrDot));
    }

    #[test]
    fn rejects_reserved_windows_names_in_any_case() {
        assert_eq!(validate_name("CON"), Err(NameError::Reserved("CON".into())));
        assert_eq!(
            validate_name("nul.md"),
            Err(NameError::Reserved("nul".into()))
        );
        assert_eq!(
            validate_name("Com1.txt"),
            Err(NameError::Reserved("Com1".into()))
        );
        assert_eq!(validate_name("CONSOLE"), Ok(()));
    }
}
