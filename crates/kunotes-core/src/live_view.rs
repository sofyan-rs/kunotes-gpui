//! How each line of a note looks in Live mode (the custom editor):
//! which characters are drawn, in which style, what kind of line it is
//! (heading, task, bullet, rule, ...), and how cursor positions map between
//! the drawn text and the file.
//!
//! A line the cursor is on is *revealed*: every character is drawn, with the
//! markdown markers dimmed. Other lines hide their markers: `## Title` draws
//! "Title" as a heading, `- [x] done` draws "done" next to a checkbox, and
//! `- item` draws "• item".
//!
//! Positions are UTF-8 byte offsets relative to the start of the line.

use std::ops::Range;

use crate::live::{Span, SpanKind, spans};

/// What kind of line this is, which decides its font size and decorations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Paragraph,
    Heading(u8),
    Quote,
    /// A line inside a fenced code block.
    CodeBlock,
    /// The ```` ``` ```` line that opens or closes a code block.
    Fence,
    Rule,
    /// A line that is only an image: `![alt](path)`.
    Image,
    Bullet,
    Numbered,
    /// A task list item; `checked` is `[x]`.
    Task {
        checked: bool,
    },
}

/// One line of the note: its byte range (without the line break) and its styled parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub range: Range<usize>,
    /// Styled parts with ranges relative to the line start.
    pub spans: Vec<Span>,
}

/// Splits `text` into lines, each with its own styled parts.
/// A trailing line break gives a final empty line (where the cursor can go).
pub fn lines(text: &str) -> Vec<Line> {
    let all = spans(text);
    let mut result = Vec::new();
    let mut start = 0;
    let mut next_span = 0;
    loop {
        let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        let content_end = if text[start..end].ends_with('\r') {
            end - 1
        } else {
            end
        };
        let mut line_spans = Vec::new();
        while next_span < all.len() && all[next_span].range.start < content_end.max(start + 1) {
            let span = &all[next_span];
            if span.range.end <= content_end {
                line_spans.push(Span {
                    range: span.range.start - start..span.range.end - start,
                    kind: span.kind,
                });
            }
            next_span += 1;
        }
        result.push(Line {
            range: start..content_end,
            spans: line_spans,
        });
        if end == text.len() {
            break;
        }
        start = end + 1;
    }
    result
}

/// One piece of drawn text and where it came from in the line.
/// Segments are contiguous: together they cover the whole line and the whole drawn text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    /// Where the piece is in the drawn text (empty for hidden markers).
    display: Range<usize>,
    /// The part of the line it stands for.
    source: Range<usize>,
    /// True if the drawn text isn't the source text itself (hidden, or "• " for "- ").
    replaced: bool,
}

/// How one line is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineView {
    pub kind: LineKind,
    /// The text to draw.
    pub text: String,
    /// Style runs over `text`: byte length and the kind of part (None = plain).
    pub runs: Vec<(usize, Option<SpanKind>)>,
    segments: Vec<Segment>,
    source_len: usize,
}

/// Builds the drawn form of `line_text` (one line, without its line break).
pub fn line_view(line_text: &str, line_spans: &[Span], revealed: bool) -> LineView {
    let kind = line_kind(line_text, line_spans);
    let mut view = LineView {
        kind,
        text: String::new(),
        runs: Vec::new(),
        segments: Vec::new(),
        source_len: line_text.len(),
    };

    // A rule is drawn as a line, not text (unless the cursor is on it).
    if kind == LineKind::Rule && !revealed {
        return view;
    }

    let mut position = 0;
    for span in line_spans {
        if position < span.range.start {
            view.push(line_text, position..span.range.start, None, None);
        }
        let hidden = !revealed && is_hidden(span, kind);
        let replacement =
            (!revealed && span.kind == SpanKind::ListMarker && kind == LineKind::Bullet)
                .then_some("• ");
        if let Some(replacement) = replacement {
            view.push(
                line_text,
                span.range.clone(),
                Some(SpanKind::ListMarker),
                Some(replacement),
            );
        } else if !hidden {
            view.push(line_text, span.range.clone(), Some(span.kind), None);
        } else {
            // Hidden: keep an empty segment so positions still map.
            let at = view.text.len();
            view.segments.push(Segment {
                display: at..at,
                source: span.range.clone(),
                replaced: true,
            });
        }
        position = span.range.end;
    }
    if position < line_text.len() {
        view.push(line_text, position..line_text.len(), None, None);
    }
    view
}

impl LineView {
    fn push(
        &mut self,
        line_text: &str,
        source: Range<usize>,
        kind: Option<SpanKind>,
        replacement: Option<&str>,
    ) {
        let drawn = replacement.unwrap_or(&line_text[source.clone()]);
        let start = self.text.len();
        self.segments.push(Segment {
            display: start..start + drawn.len(),
            source,
            replaced: replacement.is_some(),
        });
        self.text.push_str(drawn);
        match self.runs.last_mut() {
            Some((len, last)) if *last == kind => *len += drawn.len(),
            _ => self.runs.push((drawn.len(), kind)),
        }
    }

    /// Where a position in the line ends up in the drawn text.
    /// A position inside hidden or replaced text maps to the end of that piece.
    pub fn to_display(&self, source: usize) -> usize {
        for segment in &self.segments {
            if source < segment.source.end {
                return if segment.replaced {
                    segment.display.end
                } else {
                    segment.display.start + (source - segment.source.start)
                };
            }
        }
        self.text.len()
    }

    /// Where a position in the drawn text is in the line. At the edge of a
    /// hidden marker it lands after the marker, at the start of the visible text.
    pub fn to_source(&self, display: usize) -> usize {
        for segment in self.segments.iter().filter(|s| !s.display.is_empty()) {
            if display <= segment.display.start {
                return segment.source.start;
            }
            if display < segment.display.end {
                return if segment.replaced {
                    segment.source.end
                } else {
                    segment.source.start + (display - segment.display.start)
                };
            }
        }
        self.source_len
    }
}

/// Whether a part is hidden on a line that isn't revealed.
fn is_hidden(span: &Span, kind: LineKind) -> bool {
    match span.kind {
        SpanKind::Marker => kind != LineKind::Fence,
        SpanKind::ListMarker => matches!(kind, LineKind::Task { .. }),
        SpanKind::LinkUrl => true,
        _ => false,
    }
}

/// For an image line (`![alt](path)` alone), the range of `path` in the line.
pub fn image_link(line_text: &str, line_spans: &[Span]) -> Option<Range<usize>> {
    let trimmed_start = line_text.len() - line_text.trim_start().len();
    let trimmed_end = line_text.trim_end().len();
    let first = line_spans.first()?;
    let last = line_spans.last()?;
    let is_image = first.kind == SpanKind::Marker
        && first.range.start == trimmed_start
        && &line_text[first.range.clone()] == "!["
        && last.kind == SpanKind::Marker
        && last.range.end == trimmed_end
        && line_spans.len() <= 5;
    if !is_image {
        return None;
    }
    line_spans
        .iter()
        .find(|span| span.kind == SpanKind::LinkUrl)
        .map(|span| span.range.clone())
}

/// A line that draws no text (an image shown as a picture instead).
/// Every position maps to the end of the line.
pub fn empty_view(kind: LineKind, source_len: usize) -> LineView {
    LineView {
        kind,
        text: String::new(),
        runs: Vec::new(),
        segments: Vec::new(),
        source_len,
    }
}

/// The kind of line, from its parts.
fn line_kind(line_text: &str, line_spans: &[Span]) -> LineKind {
    if image_link(line_text, line_spans).is_some() {
        return LineKind::Image;
    }
    let first = line_spans.first();
    for span in line_spans {
        match span.kind {
            SpanKind::Heading(level) => return LineKind::Heading(level),
            SpanKind::Rule => return LineKind::Rule,
            SpanKind::CodeBlock => return LineKind::CodeBlock,
            SpanKind::Quote => return LineKind::Quote,
            _ => {}
        }
    }
    if let Some(span) = first {
        let marker = &line_text[span.range.clone()];
        match span.kind {
            SpanKind::ListMarker if marker.contains("[x]") || marker.contains("[X]") => {
                return LineKind::Task { checked: true };
            }
            SpanKind::ListMarker if marker.contains("[ ]") => {
                return LineKind::Task { checked: false };
            }
            SpanKind::ListMarker
                if marker
                    .trim_start()
                    .starts_with(|c: char| c.is_ascii_digit()) =>
            {
                return LineKind::Numbered;
            }
            SpanKind::ListMarker => return LineKind::Bullet,
            SpanKind::Marker if line_spans.len() == 1 && span.range == (0..line_text.len()) => {
                let trimmed = line_text.trim_start();
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    return LineKind::Fence;
                }
            }
            SpanKind::Marker if line_text.trim_start().starts_with('>') => return LineKind::Quote,
            _ => {}
        }
    }
    LineKind::Paragraph
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(line: &str, revealed: bool) -> LineView {
        let all = lines(line);
        line_view(line, &all[0].spans, revealed)
    }

    #[test]
    fn lines_split_text_and_keep_relative_spans() {
        let text = "# A\nplain\r\n**b**";
        let all = lines(text);
        assert_eq!(all.len(), 3);
        assert_eq!(
            &text[all[1].range.clone()],
            "plain",
            "\\r is not part of the line"
        );
        assert_eq!(all[2].spans[1].range, 2..3, "relative to the line start");
        assert_eq!(
            lines("a\n").len(),
            2,
            "a trailing break gives an empty last line"
        );
    }

    #[test]
    fn hidden_markers_disappear_until_revealed() {
        let hidden = view("## Title", false);
        assert_eq!(hidden.text, "Title");
        assert_eq!(hidden.kind, LineKind::Heading(2));
        let shown = view("## Title", true);
        assert_eq!(shown.text, "## Title");
    }

    #[test]
    fn emphasis_and_code_markers_hide() {
        assert_eq!(view("a **b** `c`", false).text, "a b c");
        assert_eq!(view("see [docs](https://x.dev)", false).text, "see docs");
    }

    #[test]
    fn bullets_become_dots_and_tasks_become_checkboxes() {
        let bullet = view("- item", false);
        assert_eq!(bullet.text, "• item");
        assert_eq!(bullet.kind, LineKind::Bullet);
        let task = view("- [x] done", false);
        assert_eq!(task.text, "done");
        assert_eq!(task.kind, LineKind::Task { checked: true });
        assert_eq!(
            view("- [ ] todo", false).kind,
            LineKind::Task { checked: false }
        );
        assert_eq!(view("1. step", false).text, "1. step");
    }

    #[test]
    fn image_lines_are_recognised() {
        let line = "![cat](.img/cat.png)";
        let all = lines(line);
        assert_eq!(view(line, false).kind, LineKind::Image);
        assert_eq!(
            image_link(line, &all[0].spans).map(|range| &line[range]),
            Some(".img/cat.png")
        );
        let inline = "see ![cat](c.png) here";
        assert_eq!(
            image_link(inline, &lines(inline)[0].spans),
            None,
            "not alone on its line"
        );
        let empty = empty_view(LineKind::Image, line.len());
        assert_eq!((empty.to_display(3), empty.to_source(0)), (0, line.len()));
    }

    #[test]
    fn rules_draw_nothing_until_revealed() {
        assert_eq!(view("---", false).text, "");
        assert_eq!(view("---", false).kind, LineKind::Rule);
        assert_eq!(view("---", true).text, "---");
    }

    #[test]
    fn runs_cover_the_drawn_text() {
        let line = view("x **bold** y", true);
        let total: usize = line.runs.iter().map(|(len, _)| len).sum();
        assert_eq!(total, line.text.len());
        assert!(
            line.runs
                .iter()
                .any(|(_, kind)| *kind == Some(SpanKind::Bold))
        );
    }

    #[test]
    fn positions_map_both_ways_around_hidden_markers() {
        let line = view("## Title", false); // drawn: "Title"
        assert_eq!(line.to_display(0), 0, "inside the hidden marker");
        assert_eq!(line.to_display(3), 0, "start of the text");
        assert_eq!(line.to_display(8), 5);
        assert_eq!(
            line.to_source(0),
            3,
            "clicking before the text lands after the marker"
        );
        assert_eq!(line.to_source(5), 8);

        let bold = view("a **b** c", false); // drawn: "a b c"
        assert_eq!(bold.to_source(2), 4, "the start of 'b'");
        assert_eq!(bold.to_display(4), 2);
        assert_eq!(bold.to_source(5), 9);
    }

    #[test]
    fn revealed_lines_map_one_to_one() {
        let line = view("**b** c", true);
        for i in 0..=7 {
            assert_eq!(line.to_display(i), i);
            assert_eq!(line.to_source(i), i);
        }
    }

    #[test]
    fn multibyte_text_maps_on_character_boundaries() {
        let line = view("# 日本", false); // drawn: "日本"
        assert_eq!(line.text, "日本");
        assert_eq!(line.to_source(3), 5, "after '日'");
        assert_eq!(line.to_display(5), 3);
    }
}
