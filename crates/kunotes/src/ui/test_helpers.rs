//! Shared setup for headless UI tests (gpui-kit `test-support`).
//!
//! `setup` opens the real `Workspace` on a fresh temporary vault:
//!
//! ```text
//! vault/
//!   Projects/
//!     Archive/old.md
//!     Plan.md
//!   Welcome.md        "# Welcome\n"
//! ```
//!
//! Settings stay in memory, so tests never touch the user's real settings file.

use std::fs;
use std::path::{Path, PathBuf};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};
use tempfile::TempDir;

use crate::actions;
use crate::settings_store::SettingsStore;
use crate::ui::workspace::Workspace;
use crate::vault_store::VaultStore;

pub struct Setup {
    pub dir: TempDir,
    pub window: AnyWindowHandle,
    pub workspace: Entity<Workspace>,
    pub vault: Entity<VaultStore>,
}

impl Setup {
    /// `relative` uses `/`; each part is joined separately so the result uses the
    /// OS separator (on Windows, "a/b" would otherwise stay mixed: `C:\vault\a/b`).
    pub fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.dir.path().to_path_buf(), |path, part| path.join(part))
    }
}

pub fn setup(cx: &mut TestAppContext) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("Projects/Archive")).unwrap();
    fs::write(dir.path().join("Projects/Archive/old.md"), "").unwrap();
    fs::write(dir.path().join("Projects/Plan.md"), "").unwrap();
    fs::write(dir.path().join("Welcome.md"), "# Welcome\n").unwrap();

    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        SettingsStore::init_in_memory(cx); // never touches the real settings file
        actions::bind_keys(cx);
    });

    let (window, workspace) = cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(900.), px(600.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Workspace::new(window, cx))
        })
        .unwrap()
    });

    let vault = workspace.read_with(cx, |workspace, _| workspace.vault());
    let root = dir.path().to_path_buf();
    vault.update(cx, |vault, cx| vault.open_vault(root, cx));
    cx.run_until_parked(); // let the background scan finish

    Setup {
        dir,
        window,
        workspace,
        vault,
    }
}

/// Runs `f` with the test window, after drawing a fresh frame.
pub fn in_window(
    setup: &Setup,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App),
) {
    cx.update_window(setup.window, |_, window, cx| {
        window.render_frame(cx);
        f(window, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

pub fn selected(setup: &Setup, cx: &mut TestAppContext) -> Option<PathBuf> {
    setup
        .vault
        .read_with(cx, |vault, _| vault.selected_path().map(Path::to_path_buf))
}

pub fn is_expanded(setup: &Setup, cx: &mut TestAppContext, folder: &Path) -> bool {
    setup
        .vault
        .read_with(cx, |vault, _| vault.expanded().contains(folder))
}
