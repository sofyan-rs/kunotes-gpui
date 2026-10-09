//! The root view of the main window: title bar on top, sidebar and editor area below.

use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{ActiveTheme as _, Theme, WindowExt as _, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, Styled as _, Subscription, Window, div, px,
};

use kunotes_core::paths::remap_path;
use kunotes_core::settings::ViewMode;

use crate::actions::{
    CloseVault, CycleViewMode, DeleteSelection, NewFile, NewFolder, OpenVault, QuickSwitcher,
    RenameSelection, ToggleSidebar, ViewPreview, ViewSource, ViewSplit, WORKSPACE,
};
use crate::settings_store::SettingsStore;
use crate::ui::editor::{EditorEvent, EditorPane, load_note};
use crate::ui::{dialogs, empty_state, quick_switcher, sidebar::Sidebar, title_bar};
use crate::vault_store::{VaultEvent, VaultStore};

const SIDEBAR_DEFAULT_WIDTH: f32 = 250.;
const SIDEBAR_MIN_WIDTH: f32 = 200.;
const SIDEBAR_MAX_WIDTH: f32 = 400.;

pub struct Workspace {
    vault: Entity<VaultStore>,
    sidebar: Entity<Sidebar>,
    /// The open note, if any.
    editor: Option<Entity<EditorPane>>,
    /// Listens to the open note's errors; replaced when another note opens.
    editor_subscription: Option<Subscription>,
    focus_handle: FocusHandle,
    /// Kept alive so our listeners keep working; dropping one unsubscribes it.
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let vault = cx.new(|_| VaultStore::default());
        let sidebar = cx.new(|cx| Sidebar::new(vault.clone(), cx));

        let subscriptions = vec![
            // Whenever the vault changes: open/close the right note, update the title.
            cx.observe_in(&vault, window, |this, _, window, cx| {
                this.sync_editor(window, cx);
                this.update_window_title(window, cx);
                cx.notify();
            }),
            // Save the open note before the app quits.
            cx.on_app_quit(|this, cx| {
                this.save_open_note(cx);
                async {}
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

        // Also save when the window is closed (the app quits right after).
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = this.update(cx, |workspace, cx| workspace.save_open_note(cx));
            true
        });

        vault.update(cx, |vault, cx| vault.restore_last_vault(cx));

        Workspace {
            vault,
            sidebar,
            editor: None,
            editor_subscription: None,
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    /// Makes the editor show the vault's selected note.
    ///
    /// - Same note: nothing to do.
    /// - The open note was renamed/moved: keep the pane (and unsaved typing), update its path.
    /// - Another note: save the old one first (unless it was deleted), then open the new one.
    fn sync_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let vault = self.vault.read(cx);
        let selected = vault.selected_file().map(|p| p.to_path_buf());
        let root = vault.root().map(|p| p.to_path_buf());
        let moved = vault
            .last_move()
            .map(|(old, new)| (old.to_path_buf(), new.to_path_buf()));

        if let Some(pane) = &self.editor {
            let current = pane.read(cx).path().to_path_buf();
            if Some(&current) == selected.as_ref() {
                return;
            }
            let followed = moved
                .and_then(|(old, new)| remap_path(&current, &old, &new))
                .filter(|new_path| Some(new_path) == selected.as_ref());
            if let Some(new_path) = followed {
                pane.update(cx, |pane, cx| pane.set_path(new_path, cx));
                return;
            }
            // A deleted note must not be written back (that would bring it back).
            if current.exists() {
                pane.update(cx, |pane, cx| pane.save_now(cx));
            }
        }

        self.editor = None;
        let (Some(path), Some(root)) = (selected, root) else {
            return;
        };
        match load_note(path) {
            Ok(note) => {
                let pane = cx.new(|cx| EditorPane::new(note, root, window, cx));
                self.show_editor(pane, window, cx);
            }
            Err(message) => window.push_notification(message, cx),
        }
    }

    /// Starts showing `pane`: listens for its errors and warns about read-only notes.
    fn show_editor(
        &mut self,
        pane: Entity<EditorPane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_subscription =
            Some(cx.subscribe_in(&pane, window, |_, _, event, window, cx| {
                let EditorEvent::Error(message) = event;
                window.push_notification(message.clone(), cx);
            }));
        if pane.read(cx).is_read_only() {
            window.push_notification(
                "This file isn't valid UTF-8, so it opened read-only to avoid damaging it.",
                cx,
            );
        }
        self.editor = Some(pane);
    }

    /// Writes the open note's unsaved changes now.
    fn save_open_note(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = &self.editor {
            pane.update(cx, |pane, cx| pane.save_now(cx));
        }
    }

    /// Window title: the open note, else the vault, else the app name.
    fn update_window_title(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .editor
            .as_ref()
            .map(|pane| pane.read(cx).title())
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
        if let Some(pane) = &self.editor {
            pane.update(cx, |_, cx| cx.notify());
        }
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
            ViewMode::Source | ViewMode::Live => ViewMode::Split,
            ViewMode::Split => ViewMode::Preview,
            ViewMode::Preview => ViewMode::Source,
        };
        self.set_view_mode(next, cx);
    }

    /// The shared vault state. For tests.
    #[cfg(test)]
    pub fn vault(&self) -> Entity<VaultStore> {
        self.vault.clone()
    }

    /// The open note's pane. For tests.
    #[cfg(test)]
    pub fn editor(&self) -> Option<Entity<EditorPane>> {
        self.editor.clone()
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
        self.vault
            .update(cx, |vault, cx| vault.create_file(None, cx));
    }

    fn new_folder(&mut self, _: &NewFolder, _: &mut Window, cx: &mut Context<Self>) {
        self.vault
            .update(cx, |vault, cx| vault.create_folder(None, cx));
    }

    fn rename_selection(
        &mut self,
        _: &RenameSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.vault.read(cx).selected_path().map(|p| p.to_path_buf()) {
            dialogs::rename(self.vault.clone(), path, window, cx);
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

    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_vault = self.vault.read(cx).root().is_some();
        div()
            .size_full()
            .bg(cx.theme().background)
            .child(match &self.editor {
                Some(pane) => pane.clone().into_any_element(),
                None if has_vault => empty_state::no_file_selected(cx).into_any_element(),
                None => empty_state::no_vault(cx).into_any_element(),
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
        let title = self
            .editor
            .as_ref()
            .map(|pane| pane.read(cx).title())
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
            .on_action(cx.listener(Self::view_source))
            .on_action(cx.listener(Self::view_split))
            .on_action(cx.listener(Self::view_preview))
            .on_action(cx.listener(Self::cycle_view_mode))
            .on_action(cx.listener(Self::quick_switcher))
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
