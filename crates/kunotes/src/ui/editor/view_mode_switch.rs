//! The Source / Split / Preview switch in the editor header: a segmented control
//! with a muted track and the active mode raised on top of it.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    Action, App, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, prelude::FluentBuilder as _,
    px,
};
use kunotes_core::settings::ViewMode;

use crate::actions::{ViewPreview, ViewSource, ViewSplit, WORKSPACE};

/// One segment: which mode it shows, its label and icon, and the action it sends.
fn segments() -> [(ViewMode, &'static str, IconName, Box<dyn Action>); 3] {
    [
        (
            ViewMode::Source,
            "Source",
            IconName::Code,
            Box::new(ViewSource),
        ),
        (
            ViewMode::Split,
            "Split",
            IconName::Columns2,
            Box::new(ViewSplit),
        ),
        (
            ViewMode::Preview,
            "Preview",
            IconName::Eye,
            Box::new(ViewPreview),
        ),
    ]
}

/// Clicking a segment sends the same action as its shortcut (⌘/Ctrl+2/3/4),
/// so the workspace handles all three ways of switching in one place.
pub fn render(active: ViewMode, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (track, raised, border) = (theme.muted, theme.background, theme.border);
    let (active_text, idle_text) = (theme.foreground, theme.muted_foreground);

    h_flex()
        .p(px(2.))
        .gap(px(2.))
        .rounded_md()
        .bg(track)
        .border_1()
        .border_color(border)
        .children(segments().into_iter().map(|(mode, label, icon, action)| {
            let is_active = mode == active;
            let text = if is_active { active_text } else { idle_text };
            let tooltip_action = action.boxed_clone();
            h_flex()
                .id(label)
                .test_support()
                .h(px(24.))
                .px_2p5()
                .gap_1p5()
                .rounded(px(5.))
                .text_xs()
                .text_color(text)
                .cursor_pointer()
                .when(is_active, |segment| segment.bg(raised).shadow_sm())
                .when(!is_active, |segment| {
                    segment.hover(move |segment| segment.text_color(active_text))
                })
                .child(Icon::new(icon).xsmall().text_color(text))
                .child(label)
                .tooltip(move |window, cx| {
                    Tooltip::new(label)
                        .action(tooltip_action.as_ref(), Some(WORKSPACE))
                        .build(window, cx)
                })
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
        }))
}
