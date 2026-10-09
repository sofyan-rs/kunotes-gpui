//! The error type returned by every fallible `kunotes-core` operation.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::names::NameError;

/// Something went wrong while working with the vault on disk.
///
/// The `#[error(..)]` text is shown to the user, so keep it readable.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Invalid name: {0}")]
    InvalidName(#[from] NameError),

    #[error("\"{}\" already exists", .0.display())]
    AlreadyExists(PathBuf),

    #[error("A folder can't be moved into itself or one of its subfolders")]
    MoveIntoSelf,

    #[error("Couldn't move to trash: {0}")]
    Trash(String),

    #[error("Settings file is invalid: {0}")]
    Settings(#[from] serde_json::Error),

    #[error("\"{0}\" isn't an image (use PNG, JPEG, GIF, WebP, SVG or BMP)")]
    NotAnImage(String),

    /// A git sync step failed; the text says why.
    #[error("{0}")]
    Git(String),

    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Shorthand for `std::result::Result<T, CoreError>`.
pub type Result<T> = std::result::Result<T, CoreError>;
