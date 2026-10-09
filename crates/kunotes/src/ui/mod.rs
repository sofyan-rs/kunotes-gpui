//! All views, grouped by feature. `workspace.rs` is the root view of the window.

pub mod dialogs;
pub mod editor;
pub mod editor_area;
pub mod empty_state;
pub mod quick_switcher;
#[cfg(test)]
mod quick_switcher_tests;
pub mod sidebar;
#[cfg(test)]
pub mod test_helpers;
pub mod title_bar;
pub mod workspace;
