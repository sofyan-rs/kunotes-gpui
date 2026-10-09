//! The rendered markdown preview (read-only, selectable, links open in the browser).

use gpui_kit::component::text::TextView;
use gpui_kit::{IntoElement, SharedString, Styled as _, div, prelude::*};

pub fn render(text: SharedString) -> impl IntoElement {
    div().size_full().px_6().py_4().child(
        TextView::markdown("preview", text)
            .selectable(true)
            .scrollable(true)
            .on_link_click(|url, _, _, cx| cx.open_url(url))
            .size_full(),
    )
}
