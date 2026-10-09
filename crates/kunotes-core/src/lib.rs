//! Pure logic for KuNotes: vault scanning, file operations, name validation,
//! path helpers, and markdown formatting transforms. No GPUI dependency.
//!
//! Start reading with `node.rs` (how a vault folder becomes a tree), then
//! `fs_ops.rs` (how files are created, renamed, moved, and saved).

pub mod cursor;
pub mod error;
pub mod format;
pub mod fs_ops;
pub mod line_ending;
pub mod names;
pub mod node;
pub mod paths;
pub mod search;
pub mod settings;

pub use error::{CoreError, Result};
pub use node::VaultNode;

/// Application name used for config directories and window titles.
pub const APP_NAME: &str = "KuNotes";
