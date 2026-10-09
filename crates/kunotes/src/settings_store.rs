//! App-wide access to the user's settings, stored as a GPUI global.
//!
//! Read with `SettingsStore::get(cx)`, change with `SettingsStore::update(cx, |s| ..)`.
//! Changes are saved to disk shortly after (debounced), and once more on quit.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::{App, AppContext as _, Global, Task};
use kunotes_core::settings::Settings;

/// Wait this long after the last change before writing the file,
/// so dragging the sidebar doesn't write on every pixel.
const SAVE_DELAY: Duration = Duration::from_millis(300);

pub struct SettingsStore {
    settings: Settings,
    path: Option<PathBuf>,
    /// The pending save. Replacing it cancels the previous one.
    save_task: Option<Task<()>>,
}

impl Global for SettingsStore {}

impl SettingsStore {
    /// Loads settings from disk and makes them available app-wide. Call once at startup.
    pub fn init(cx: &mut App) {
        let path = Settings::default_path();
        let settings = path.as_deref().map(Settings::load_from).unwrap_or_default();
        cx.set_global(SettingsStore {
            settings,
            path,
            save_task: None,
        });
    }

    /// Default settings that are never written to disk. For tests.
    #[cfg(test)]
    pub fn init_in_memory(cx: &mut App) {
        cx.set_global(SettingsStore {
            settings: Settings::default(),
            path: None,
            save_task: None,
        });
    }

    pub fn get(cx: &App) -> &Settings {
        &cx.global::<SettingsStore>().settings
    }

    /// Changes settings and schedules a save.
    pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
        let store = cx.global_mut::<SettingsStore>();
        let before = store.settings.clone();
        change(&mut store.settings);
        if store.settings == before {
            return;
        }

        let (settings, path) = (store.settings.clone(), store.path.clone());
        let Some(path) = path else { return };
        let task = cx.spawn(async move |cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            cx.background_spawn(async move {
                if let Err(error) = settings.save_to(&path) {
                    log::warn!("couldn't save settings: {error}");
                }
            })
            .await;
        });
        cx.global_mut::<SettingsStore>().save_task = Some(task);
    }

    /// Saves right now, skipping the delay. Used when the app quits.
    pub fn save_now(cx: &mut App) {
        let store = cx.global_mut::<SettingsStore>();
        store.save_task = None;
        if let Some(path) = &store.path
            && let Err(error) = store.settings.save_to(path)
        {
            log::warn!("couldn't save settings: {error}");
        }
    }
}
