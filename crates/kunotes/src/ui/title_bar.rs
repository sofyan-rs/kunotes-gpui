//! The window title bar: sidebar toggle, app menu (Windows/Linux), and centered title.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, TitleBar, h_flex};
use gpui_kit::{App, Entity, Global, IntoElement, ParentElement as _, Styled as _, Window, div};

use crate::actions::{ToggleSidebar, WORKSPACE};
use crate::platform;

/// The in-window menu bar, created once and shared (Windows and Linux only).
struct MenuBar(Entity<AppMenuBar>);
impl Global for MenuBar {}

pub fn render(title: Option<String>, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let menu_bar = (!platform::uses_native_menu_bar()).then(|| menu_bar(cx));

    TitleBar::new().child(
        h_flex()
            .w_full()
            .pr_2()
            .gap_1()
            .children(menu_bar)
            .child(
                Button::new("toggle-sidebar")
                    .ghost()
                    .small()
                    .icon(IconName::PanelLeft)
                    .tooltip_with_action("Toggle Sidebar", &ToggleSidebar, Some(WORKSPACE))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleSidebar), cx)),
            )
            .child(
                div()
                    .flex_1()
                    .text_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(title.unwrap_or_else(|| "KuNotes".to_string())),
            ),
    )
}

fn menu_bar(cx: &mut App) -> Entity<AppMenuBar> {
    if let Some(MenuBar(bar)) = cx.try_global::<MenuBar>() {
        return bar.clone();
    }
    let bar = AppMenuBar::new(cx);
    cx.set_global(MenuBar(bar.clone()));
    bar
}
