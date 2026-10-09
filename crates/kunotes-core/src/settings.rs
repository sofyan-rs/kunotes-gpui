//! User settings saved between launches, stored as JSON in the OS config folder
//! (e.g. `~/Library/Application Support/kunotes/settings.json` on macOS).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::fs_ops::atomic_write;

/// How the editor pane shows the current note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// Formatted while typing (Phase 9+).
    Live,
    /// Raw markdown with syntax highlighting.
    #[default]
    Source,
    /// Source and preview side by side.
    Split,
    /// Read-only rendered document.
    Preview,
}

/// Everything KuNotes remembers between launches.
///
/// `#[serde(default)]` means a missing field (e.g. from an older version)
/// gets its default value instead of failing to load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub last_vault: Option<PathBuf>,
    pub view_mode: ViewMode,
    pub sidebar_width: Option<f32>,
    pub sidebar_visible: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            last_vault: None,
            view_mode: ViewMode::default(),
            sidebar_width: None,
            sidebar_visible: true,
        }
    }
}

impl Settings {
    /// Where settings are stored: `$KUNOTES_CONFIG_DIR/settings.json` if that
    /// variable is set (handy for a separate dev/test instance), otherwise the OS
    /// config folder. `None` if neither exists.
    pub fn default_path() -> Option<PathBuf> {
        let dir = match std::env::var_os("KUNOTES_CONFIG_DIR") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => dirs::config_dir()?.join("kunotes"),
        };
        Some(dir.join("settings.json"))
    }

    /// Loads settings from `path`. A missing or broken file gives the defaults,
    /// so a bad settings file can never stop the app from starting.
    pub fn load_from(path: &Path) -> Settings {
        fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    /// Saves settings to `path`, creating the folder if needed.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        atomic_write(path, json.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_show_sidebar_in_source_mode() {
        let settings = Settings::default();
        assert!(settings.sidebar_visible);
        assert_eq!(settings.view_mode, ViewMode::Source);
        assert_eq!(settings.last_vault, None);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let settings: Settings = serde_json::from_str(r#"{ "view_mode": "split" }"#).unwrap();
        assert_eq!(settings.view_mode, ViewMode::Split);
        assert!(settings.sidebar_visible);
    }
}
