//! Pure logic for KuNotes: vault scanning, file operations, name validation,
//! path helpers, and markdown formatting transforms. No GPUI dependency.
//!
//! Start reading with `node.rs` (how a vault folder becomes a tree), then
//! `fs_ops.rs` (how files are created, renamed, moved, and saved).

pub mod cursor;
pub mod error;
pub mod format;
pub mod fs_ops;
pub mod git;
pub mod line_ending;
pub mod live;
pub mod live_buffer;
pub mod live_table;
pub mod live_view;
pub mod lock;
pub mod names;
pub mod node;
pub mod paths;
pub mod search;
pub mod settings;
pub mod tabs;

pub use error::{CoreError, Result};
pub use node::VaultNode;

/// Application name used for config directories and window titles.
pub const APP_NAME: &str = "KuNotes";
