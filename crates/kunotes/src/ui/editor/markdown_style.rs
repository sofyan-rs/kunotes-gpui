//! Styles markdown inside the Source/Split editor (gpui-kit's code editor).
//!
//! The parts of the note come from `kunotes_core::live::spans`. This file turns
//! them into colors and font styles, using the theme's syntax colors (so light
//! and dark themes both work). Markers stay as visible as the text: Source
//! shows the markdown as it is. (Live mode draws itself, see `live/`.)

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::input::{
    EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter,
    InputHighlighterFactory, Rope,
};
use gpui_kit::{
    Context, FontStyle, FontWeight, HighlightStyle, SharedString, StrikethroughStyle,
    UnderlineStyle, Window, px,
};
use kunotes_core::live::{Span, SpanKind, spans};

/// Makes a highlighter for the editor. The editor calls it when it needs one.
pub fn highlighter_factory() -> InputHighlighterFactory {
    Rc::new(move |_language: &str| {
        let highlighter: Box<dyn InputHighlighter> =
            Box::new(MarkdownHighlighter { spans: Vec::new() });
        Some(highlighter)
    })
}

struct MarkdownHighlighter {
    /// The styled parts of the current text, in order.
    spans: Vec<Span>,
}

impl InputHighlighter for MarkdownHighlighter {
    fn language(&self) -> SharedString {
        "markdown".into()
    }

    /// Called after every edit. Re-scanning the whole note is simple and fast
    /// enough here: the scan is a single pass over the text.
    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        self.spans = spans(&text.to_string());
    }

    /// Style runs that cover all of `range`; text without a part gets the default style.
    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut runs = Vec::new();
        let mut position = range.start;
        // Skip parts that end before the visible range.
        let first = self
            .spans
            .partition_point(|span| span.range.end <= range.start);
        for span in &self.spans[first..] {
            if span.range.start >= range.end {
                break;
            }
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            if position < start {
                runs.push((position..start, HighlightStyle::default()));
            }
            runs.push((start..end, self.style_for(span.kind, resolver)));
            position = end;
        }
        if position < range.end {
            runs.push((position..range.end, HighlightStyle::default()));
        }
        runs
    }

    fn fold_ranges(&self, _text: &Rope) -> Vec<FoldRange> {
        Vec::new() // no code folding in notes
    }
}

impl MarkdownHighlighter {
    fn style_for(&self, kind: SpanKind, resolver: &dyn HighlightStyleResolver) -> HighlightStyle {
        let theme = |name: &str| resolver.style(name).unwrap_or_default();
        let bold = |style: HighlightStyle| HighlightStyle {
            font_weight: Some(FontWeight::BOLD),
            ..style
        };
        let italic = |style: HighlightStyle| HighlightStyle {
            font_style: Some(FontStyle::Italic),
            ..style
        };

        match kind {
            SpanKind::Marker => HighlightStyle::default(),
            SpanKind::Heading(_) => bold(theme("title")),
            SpanKind::Bold => bold(theme("emphasis.strong")),
            SpanKind::Italic => italic(theme("emphasis")),
            SpanKind::BoldItalic => italic(bold(theme("emphasis.strong"))),
            SpanKind::Strike => HighlightStyle {
                strikethrough: Some(StrikethroughStyle {
                    thickness: px(1.),
                    ..Default::default()
                }),
                ..HighlightStyle::default()
            },
            SpanKind::Code | SpanKind::CodeBlock => theme("text.literal"),
            SpanKind::LinkText => HighlightStyle {
                underline: Some(UnderlineStyle {
                    thickness: px(1.),
                    ..Default::default()
                }),
                ..theme("link_text")
            },
            SpanKind::LinkUrl => theme("link_uri"),
            SpanKind::ListMarker => theme("keyword"),
            SpanKind::Quote => italic(theme("comment")),
            SpanKind::Rule => theme("comment"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A theme stand-in that gives every name the same color.
    struct OneColor;
    impl HighlightStyleResolver for OneColor {
        fn style(&self, _name: &str) -> Option<HighlightStyle> {
            Some(HighlightStyle {
                color: Some(gpui_kit::red()),
                ..Default::default()
            })
        }
    }

    fn highlighter(text: &str) -> MarkdownHighlighter {
        MarkdownHighlighter { spans: spans(text) }
    }

    #[test]
    fn runs_cover_the_whole_range_in_order() {
        let text = "a **b** c `d` e";
        let runs = highlighter(text).styles(&(0..text.len()), &OneColor);
        assert_eq!(runs.first().unwrap().0.start, 0);
        assert_eq!(runs.last().unwrap().0.end, text.len());
        for pair in runs.windows(2) {
            assert_eq!(pair[0].0.end, pair[1].0.start, "no gaps or overlaps");
        }
    }

    #[test]
    fn runs_are_clipped_to_the_requested_range() {
        let text = "**bold text**";
        let runs = highlighter(text).styles(&(4..8), &OneColor);
        assert_eq!(
            runs,
            [(4..8, highlighter(text).style_for(SpanKind::Bold, &OneColor))]
        );
    }

    #[test]
    fn heading_text_is_bold_and_markers_are_not_faded() {
        let text = "# Title";
        let runs = highlighter(text).styles(&(0..text.len()), &OneColor);
        assert!(
            runs[0].1.fade_out.is_none(),
            "Source shows markers as they are"
        );
        assert_eq!(runs[1].1.font_weight, Some(FontWeight::BOLD));
    }
}
