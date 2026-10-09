//! The root view of the main window: title bar on top, sidebar and editor area below.

use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{ActiveTheme as _, Theme, WindowExt as _, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, Styled as _, Subscription, Window, div, px,
};

use crate::actions::{CloseVault, NewFile, NewFolder, OpenVault, ToggleSidebar, WORKSPACE};
use crate::settings_store::SettingsStore;
use crate::ui::{empty_state, sidebar::Sidebar, title_bar};
use crate::vault_store::{VaultEvent, VaultStore};

const SIDEBAR_DEFAULT_WIDTH: f32 = 250.;
const SIDEBAR_MIN_WIDTH: f32 = 200.;
const SIDEBAR_MAX_WIDTH: f32 = 400.;

pub struct Workspace {
    vault: Entity<VaultStore>,
    sidebar: Entity<Sidebar>,
    focus_handle: FocusHandle,
    /// Kept alive so our listeners keep working; dropping one unsubscribes it.
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let vault = cx.new(|_| VaultStore::default());
        let sidebar = cx.new(|cx| Sidebar::new(vault.clone(), cx));

        let subscriptions = vec![
            // Redraw and update the window title whenever the vault changes.
            cx.observe_in(&vault, window, |_, vault, window, cx| {
                let title = match vault.read(cx).name() {
                    Some(name) => format!("{name} — KuNotes"),
                    None => "KuNotes".to_string(),
                };
                window.set_window_title(&title);
                cx.notify();
            }),
            // Show file operation errors as a notification.
            cx.subscribe_in(&vault, window, |_, _, event, window, cx| {
                if let VaultEvent::Error(message) = event {
                    window.push_notification(message.clone(), cx);
                }
            }),
            // Follow the system light/dark setting while the app is running.
            cx.observe_window_appearance(window, |_, window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
            }),
        ];
        Theme::sync_system_appearance(Some(window), cx);

        // Focus the workspace so its keyboard shortcuts work right away.
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        vault.update(cx, |vault, cx| vault.restore_last_vault(cx));

        Workspace {
            vault,
            sidebar,
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    /// Asks the user for a folder and opens it as the vault.
    fn open_vault(&mut self, _: &OpenVault, window: &mut Window, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Vault".into()),
        });
        let vault = self.vault.clone();
        cx.spawn_in(window, async move |_, cx| {
            match answer.await {
                // The user picked a folder.
                Ok(Ok(Some(mut paths))) => {
                    if let Some(path) = paths.pop() {
                        vault.update(cx, |vault, cx| vault.open_vault(path, cx));
                    }
                }
                // The system dialog failed (e.g. no portal on Linux).
                Ok(Err(error)) => {
                    let _ = cx.update(|window, cx| {
                        window.push_notification(
                            format!("Couldn't open the folder picker: {error}"),
                            cx,
                        );
                    });
                }
                // Cancelled.
                _ => {}
            }
        })
        .detach();
    }

    fn close_vault(&mut self, _: &CloseVault, _: &mut Window, cx: &mut Context<Self>) {
        self.vault.update(cx, |vault, cx| vault.close_vault(cx));
    }

    fn new_file(&mut self, _: &NewFile, _: &mut Window, cx: &mut Context<Self>) {
        self.vault.update(cx, |vault, cx| vault.create_file(cx));
    }

    fn new_folder(&mut self, _: &NewFolder, _: &mut Window, cx: &mut Context<Self>) {
        self.vault.update(cx, |vault, cx| vault.create_folder(cx));
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        SettingsStore::update(cx, |settings| {
            settings.sidebar_visible = !settings.sidebar_visible;
        });
        cx.notify();
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_vault = self.vault.read(cx).root().is_some();
        div()
            .size_full()
            .bg(cx.theme().background)
            .child(if has_vault {
                empty_state::no_file_selected(cx).into_any_element()
            } else {
                empty_state::no_vault(cx).into_any_element()
            })
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = SettingsStore::get(cx);
        let sidebar_visible = settings.sidebar_visible;
        let sidebar_width = settings
            .sidebar_width
            .unwrap_or(SIDEBAR_DEFAULT_WIDTH)
            .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        let title = self.vault.read(cx).name();

        v_flex()
            .id("workspace")
            .key_context(WORKSPACE)
            .track_focus(&self.focus_handle)
            // `cx.listener` turns a method into a callback that gets `&mut self`.
            .on_action(cx.listener(Self::open_vault))
            .on_action(cx.listener(Self::close_vault))
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::new_folder))
            .on_action(cx.listener(Self::toggle_sidebar))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(title_bar::render(title, window, cx))
            .child(
                h_resizable("workspace-split")
                    .on_resize(|state, _, cx| {
                        if let Some(width) = state.read(cx).sizes().first().copied() {
                            SettingsStore::update(cx, |settings| {
                                settings.sidebar_width = Some(f32::from(width));
                            });
                        }
                    })
                    .child(
                        resizable_panel()
                            .size(px(sidebar_width))
                            .size_range(px(SIDEBAR_MIN_WIDTH)..px(SIDEBAR_MAX_WIDTH))
                            .visible(sidebar_visible)
                            .child(self.sidebar.clone()),
                    )
                    .child(resizable_panel().child(self.render_detail(cx))),
            )
    }
}
