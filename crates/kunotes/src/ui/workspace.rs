//! The root view of the main window: title bar on top, sidebar and editor area below.

use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{ActiveTheme as _, Theme, WindowExt as _, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, Styled as _, Subscription, Window, px,
};

use kunotes_core::settings::ViewMode;

use crate::actions::{
    CloseAllTabs, CloseOtherTabs, CloseSavedTabs, CloseTab, CloseTabsToTheRight, CloseVault,
    CycleViewMode, DeleteSelection, NewFile, NewFolder, NextTab, OpenVault, PreviousTab,
    QuickSwitcher, RenameSelection, TogglePinTab, ToggleSidebar, ViewLive, ViewPreview, ViewSource,
    ViewSplit, WORKSPACE,
};
use crate::git_sync::{GitSync, GitSyncEvent};
use crate::settings_store::SettingsStore;
use crate::ui::editor_area::EditorArea;
use crate::ui::{dialogs, quick_switcher, sidebar::Sidebar, title_bar};
use crate::vault_store::{VaultEvent, VaultStore};

const SIDEBAR_DEFAULT_WIDTH: f32 = 250.;
const SIDEBAR_MIN_WIDTH: f32 = 200.;
const SIDEBAR_MAX_WIDTH: f32 = 400.;

pub struct Workspace {
    vault: Entity<VaultStore>,
    sidebar: Entity<Sidebar>,
    /// The tabs and the open notes' editors.
    editor_area: Entity<EditorArea>,
    /// Keeps the vault in step with a git remote, if set up.
    git_sync: Entity<GitSync>,
    focus_handle: FocusHandle,
    /// Kept alive so our listeners keep working; dropping one unsubscribes it.
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let vault = cx.new(|_| {
            #[cfg_attr(not(test), allow(unused_mut))]
            let mut vault = VaultStore::default();
            // UI tests: the OS watcher's thread would break the test scheduler.
            #[cfg(test)]
            vault.disable_live_sync();
            vault
        });
        let editor_area = cx.new(|cx| EditorArea::new(vault.clone(), window, cx));
        let git_sync = cx.new(|cx| GitSync::new(vault.clone(), editor_area.clone(), cx));
        let sidebar = cx.new(|cx| Sidebar::new(vault.clone(), git_sync.clone(), cx));

        let subscriptions = vec![
            // Keep the window title in step with the vault and the active tab.
            cx.observe_in(&vault, window, |this, _, window, cx| {
                this.update_window_title(window, cx);
                cx.notify();
            }),
            cx.observe_in(&editor_area, window, |this, _, window, cx| {
                this.update_window_title(window, cx);
                cx.notify();
            }),
            // Save every open note before the app quits (and commit/push them).
            cx.on_app_quit(|this, cx| {
                this.save_open_notes(cx);
                this.git_sync.update(cx, |sync, cx| sync.finish_on_quit(cx));
                async {}
            }),
            // After a git sync: show notes that changed, and mention conflict copies.
            cx.subscribe_in(
                &git_sync,
                window,
                |this, _, event, window, cx| match event {
                    GitSyncEvent::Synced(report) => {
                        this.editor_area.update(cx, |area, cx| {
                            area.reload_from_disk(&report.changed, window, cx)
                        });
                        for copy in &report.conflicts {
                            let name = copy
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned());
                            window.push_notification(
                            format!(
                                "A note changed on two computers. Both versions were kept: “{}”.",
                                name.unwrap_or_default()
                            ),
                            cx,
                        );
                        }
                    }
                    GitSyncEvent::Message(message) => window.push_notification(message.clone(), cx),
                },
            ),
            // Show file operation errors.
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

        // Also save when the window is closed (the app quits right after).
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = this.update(cx, |workspace, cx| workspace.save_open_notes(cx));
            true
        });

        vault.update(cx, |vault, cx| vault.restore_last_vault(cx));

        Workspace {
            vault,
            sidebar,
            editor_area,
            git_sync,
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    /// Writes every open note's unsaved changes now.
    fn save_open_notes(&mut self, cx: &mut Context<Self>) {
        self.editor_area.update(cx, |area, cx| area.save_all(cx));
    }

    /// Window title: the open note, else the vault, else the app name.
    fn update_window_title(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .editor_area
            .read(cx)
            .active_title()
            .or_else(|| self.vault.read(cx).name());
        let title = match name {
            Some(name) => format!("{name} — KuNotes"),
            None => "KuNotes".to_string(),
        };
        window.set_window_title(&title);
    }

    fn set_view_mode(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        SettingsStore::update(cx, |settings| settings.view_mode = mode);
        cx.notify();
        self.editor_area.update(cx, |area, cx| {
            if let Some(pane) = area.active_pane() {
                pane.update(cx, |_, cx| cx.notify());
            }
            cx.notify();
        });
    }

    fn view_live(&mut self, _: &ViewLive, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view_mode(ViewMode::Live, cx);
    }

    fn view_source(&mut self, _: &ViewSource, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view_mode(ViewMode::Source, cx);
    }

    fn view_split(&mut self, _: &ViewSplit, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view_mode(ViewMode::Split, cx);
    }

    fn view_preview(&mut self, _: &ViewPreview, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view_mode(ViewMode::Preview, cx);
    }

    fn cycle_view_mode(&mut self, _: &CycleViewMode, _: &mut Window, cx: &mut Context<Self>) {
        let next = match SettingsStore::get(cx).view_mode {
            ViewMode::Live => ViewMode::Source,
            ViewMode::Source => ViewMode::Split,
            ViewMode::Split => ViewMode::Preview,
            ViewMode::Preview => ViewMode::Live,
        };
        self.set_view_mode(next, cx);
    }

    /// The shared vault state. For tests.
    #[cfg(test)]
    pub fn vault(&self) -> Entity<VaultStore> {
        self.vault.clone()
    }

    /// The active tab's editor. For tests.
    #[cfg(test)]
    pub fn editor(&self, cx: &gpui_kit::App) -> Option<Entity<crate::ui::editor::EditorPane>> {
        self.editor_area.read(cx).active_pane().cloned()
    }

    /// The tabs view. For tests.
    #[cfg(test)]
    pub fn editor_area(&self) -> Entity<EditorArea> {
        self.editor_area.clone()
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

    fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        self.start_new_item(false, window, cx);
    }

    fn new_folder(&mut self, _: &NewFolder, window: &mut Window, cx: &mut Context<Self>) {
        self.start_new_item(true, window, cx);
    }

    /// New items are named inline in the tree, so the sidebar must be visible.
    fn start_new_item(&mut self, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.show_sidebar(cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.start_new_item(is_dir, window, cx));
    }

    fn rename_selection(
        &mut self,
        _: &RenameSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.vault.read(cx).selected_path().map(|p| p.to_path_buf()) {
            self.show_sidebar(cx);
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.start_rename(path, window, cx));
        }
    }

    fn show_sidebar(&mut self, cx: &mut Context<Self>) {
        if !SettingsStore::get(cx).sidebar_visible {
            SettingsStore::update(cx, |settings| settings.sidebar_visible = true);
            cx.notify();
        }
    }

    fn delete_selection(
        &mut self,
        _: &DeleteSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.vault.read(cx).selected_path().map(|p| p.to_path_buf()) {
            dialogs::confirm_trash(self.vault.clone(), path, window, cx);
        }
    }

    fn quick_switcher(&mut self, _: &QuickSwitcher, window: &mut Window, cx: &mut Context<Self>) {
        quick_switcher::open(self.vault.clone(), window, cx);
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        SettingsStore::update(cx, |settings| {
            settings.sidebar_visible = !settings.sidebar_visible;
        });
        cx.notify();
    }
}

impl Workspace {
    /// Wraps an editor-area method as an action handler for the workspace.
    fn tab_action<A: gpui_kit::Action>(
        &self,
        handler: impl Fn(&mut EditorArea, &A, &mut Window, &mut Context<EditorArea>) + 'static,
    ) -> impl Fn(&A, &mut Window, &mut gpui_kit::App) + 'static {
        let area = self.editor_area.clone();
        move |action, window, cx| area.update(cx, |area, cx| handler(area, action, window, cx))
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
        let title = self
            .editor_area
            .read(cx)
            .active_title()
            .or_else(|| self.vault.read(cx).name());

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
            .on_action(cx.listener(Self::rename_selection))
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::view_live))
            .on_action(cx.listener(Self::view_source))
            .on_action(cx.listener(Self::view_split))
            .on_action(cx.listener(Self::view_preview))
            .on_action(cx.listener(Self::cycle_view_mode))
            .on_action(cx.listener(Self::quick_switcher))
            // Tab commands go to the editor area, wherever the focus is.
            .on_action(
                self.tab_action(|area, _: &CloseTab, window, cx| area.close_active(window, cx)),
            )
            .on_action(
                self.tab_action(|area, _: &CloseOtherTabs, window, cx| {
                    area.close_others(window, cx)
                }),
            )
            .on_action(
                self.tab_action(|area, _: &CloseTabsToTheRight, window, cx| {
                    area.close_to_the_right(window, cx)
                }),
            )
            .on_action(
                self.tab_action(|area, _: &CloseSavedTabs, window, cx| {
                    area.close_saved(window, cx)
                }),
            )
            .on_action(
                self.tab_action(|area, _: &CloseAllTabs, window, cx| area.close_all(window, cx)),
            )
            .on_action(
                self.tab_action(|area, _: &TogglePinTab, window, cx| area.toggle_pin(window, cx)),
            )
            .on_action(self.tab_action(|area, _: &NextTab, window, cx| area.cycle(1, window, cx)))
            .on_action(
                self.tab_action(|area, _: &PreviousTab, window, cx| area.cycle(-1, window, cx)),
            )
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
                    .child(resizable_panel().child(self.editor_area.clone())),
            )
    }
}
