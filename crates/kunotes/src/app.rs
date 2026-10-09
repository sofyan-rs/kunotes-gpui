//! App setup: themes, settings, shortcuts, menus, and the main window.

use gpui_kit::component::{GlobalState, TitleBar};
use gpui_kit::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};

use crate::actions::{self, Quit};
use crate::settings_store::SettingsStore;
use crate::ui::workspace::Workspace;

/// Called once by GPUI when the app starts.
pub fn run(cx: &mut App) {
    gpui_kit::init(cx);
    SettingsStore::init(cx);
    actions::bind_keys(cx);
    install_menus(cx);

    cx.on_action(|_: &Quit, cx| cx.quit());
    // KuNotes has one window; closing it quits (which also saves the open note).
    cx.on_window_closed(|cx, _| cx.quit()).detach();
    cx.on_app_quit(|cx| {
        SettingsStore::save_now(cx);
        async {}
    })
    .detach();

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1100.), px(720.)),
            cx,
        ))),
        window_min_size: Some(size(px(640.), px(400.))),
        ..TitleBar::window_options()
    };
    gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Workspace::new(window, cx))
    })
    .expect("failed to open the main window");

    cx.activate(true);
}

/// Native menu bar on macOS; the in-window menu bar (Windows, Linux) reads the same list.
fn install_menus(cx: &mut App) {
    // `Menu` can't be cloned, so build the list once for each consumer.
    let owned_menus = actions::app_menus()
        .into_iter()
        .map(|menu| menu.owned())
        .collect();
    GlobalState::global_mut(cx).set_app_menus(owned_menus);
    cx.set_menus(actions::app_menus());
}
