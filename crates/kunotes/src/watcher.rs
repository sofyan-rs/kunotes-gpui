//! Watches the vault folder for changes made outside KuNotes (Finder, another
//! editor, git, ...) and asks `VaultStore` to rescan.
//!
//! Two parts:
//! - `watch_os` runs the OS file watcher (`notify`) on its own thread and reports
//!   visible changes. It has no GPUI code, so it's tested with real files below.
//! - `start` connects it to the app: signals go over a channel to a GPUI task,
//!   which calls `VaultStore::refresh` on the UI side.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::{Context, Task};
use kunotes_core::paths::is_hidden_within;
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::vault_store::VaultStore;

/// Group bursts of file events (e.g. a git checkout) into one rescan.
const DEBOUNCE: Duration = Duration::from_millis(500);

type OsWatcher = Debouncer<RecommendedWatcher, RecommendedCache>;

/// What the watcher thread reports.
#[derive(Debug, PartialEq)]
enum Signal {
    Changed,
    Failed(String),
}

/// Keeps watching while alive. Dropping it stops the OS watcher and the task.
pub struct VaultWatcher {
    _os_watcher: OsWatcher,
    _task: Task<()>,
}

/// Starts watching `root`. Changes trigger `VaultStore::refresh`.
pub fn start(root: &Path, cx: &mut Context<VaultStore>) -> Result<VaultWatcher, String> {
    let (sender, mut receiver) = mpsc::unbounded::<Signal>();
    let os_watcher = watch_os(root, move |signal| {
        // Sending fails only when the receiver is gone (vault closed), so there's
        // nothing left to notify.
        let _ = sender.unbounded_send(signal);
    })?;

    let task = cx.spawn(async move |store, cx| {
        while let Some(signal) = receiver.next().await {
            let updated = store.update(cx, |store, cx| match signal {
                Signal::Changed => store.refresh(cx),
                Signal::Failed(message) => store.watch_failed(message, cx),
            });
            if updated.is_err() {
                break; // the store is gone
            }
        }
    });

    Ok(VaultWatcher {
        _os_watcher: os_watcher,
        _task: task,
    })
}

/// Watches `root` recursively and calls `on_signal` (on the watcher's thread) when
/// something visible changes or the watcher fails.
fn watch_os(
    root: &Path,
    mut on_signal: impl FnMut(Signal) + Send + 'static,
) -> Result<OsWatcher, String> {
    // The OS may report events under the folder's real path rather than the one we
    // opened (macOS: /var/... is really /private/var/...; any symlinked vault).
    // So we compare event paths against both forms. Canonicalizing alone isn't
    // enough: on Windows it gives `\\?\C:\...`, which events don't use.
    let mut roots: Vec<PathBuf> = vec![root.to_path_buf()];
    if let Ok(real) = std::fs::canonicalize(root)
        && real != root
    {
        roots.push(real);
    }

    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        match result {
            Ok(events) => {
                // Ignore changes that only touch hidden paths: `.git`, `.obsidian`,
                // and our own `.name.kunotes.tmp` files from saving.
                let visible = events
                    .iter()
                    .flat_map(|event| event.paths.iter())
                    .any(|path| roots.iter().any(|root| !is_hidden_within(root, path)));
                if visible {
                    on_signal(Signal::Changed);
                }
            }
            Err(errors) => on_signal(Signal::Failed(
                errors
                    .first()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "unknown error".into()),
            )),
        }
    })
    .map_err(|error| error.to_string())?;

    debouncer
        .watch(root, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;
    Ok(debouncer)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// Uses the real OS watcher and wall-clock time (a few seconds at most).
    #[test]
    fn reports_visible_changes_and_ignores_hidden_ones() {
        let dir = tempfile::tempdir().unwrap();
        let (sender, signals) = mpsc::channel();
        let _watcher = watch_os(dir.path(), move |signal| {
            let _ = sender.send(signal);
        })
        .unwrap();
        // Some OS watchers need a moment before they report events.
        std::thread::sleep(Duration::from_millis(300));

        // Saving writes a hidden temp file first; that alone must not cause a rescan.
        fs::write(dir.path().join(".note.md.kunotes.tmp"), "x").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git").join("HEAD"), "x").unwrap();
        assert!(
            signals.recv_timeout(Duration::from_millis(1500)).is_err(),
            "hidden files alone must be ignored"
        );

        fs::write(dir.path().join("note.md"), "hello").unwrap();
        assert_eq!(
            signals.recv_timeout(Duration::from_secs(10)),
            Ok(Signal::Changed)
        );
    }
}
