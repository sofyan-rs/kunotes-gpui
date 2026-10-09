//! Integration tests for saving and loading settings, using a real temporary folder.

use std::fs;

use kunotes_core::settings::{Settings, ViewMode};
use tempfile::tempdir;

#[test]
fn settings_round_trip_and_fallback() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("settings.json");

    // Missing file -> defaults.
    assert_eq!(Settings::load_from(&path), Settings::default());

    let settings = Settings {
        last_vault: Some(dir.path().to_path_buf()),
        view_mode: ViewMode::Preview,
        sidebar_width: Some(280.0),
        sidebar_visible: false,
    };
    settings.save_to(&path).unwrap();
    assert_eq!(Settings::load_from(&path), settings);

    // Broken file -> defaults instead of an error.
    fs::write(&path, "{ not json").unwrap();
    assert_eq!(Settings::load_from(&path), Settings::default());
}
