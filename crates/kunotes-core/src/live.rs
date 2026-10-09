//! Finds the markdown parts of a note so the editor can style them: headings,
//! bold/italic, inline code, links, list markers, quotes, code blocks, and rules,
//! plus the marker characters (`#`, `**`, `` ` ``, `>`, ...) around them.
//!
//! This is a light, line-by-line scan made for styling while typing, not a full
//! CommonMark parser. It never changes the text; it only reports byte ranges.

use std::ops::Range;

/// What a piece of text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// Markdown syntax itself: `#`, `**`, `` ` ``, `[`, `](`, `)`, `>`, fences.
    Marker,
    /// Heading text (level 1-6), without the `#` marker.
    Heading(u8),
    Bold,
    Italic,
    BoldItalic,
    /// Text inside `~~strikethrough~~`.
    Strike,
    /// Text inside `` `inline code` ``.
    Code,
    /// A line inside a fenced code block.
    CodeBlock,
    /// The visible text of a link `[this](...)`, or an image's alt text `![this](...)`.
    LinkText,
    /// The target of a link: `[...](this)`.
    LinkUrl,
    /// A list bullet or number: `-`, `*`, `+`, `1.`, `2)`, and a task box `[ ]`/`[x]`.
    ListMarker,
    /// Text of a blockquote line, after the `>` marker.
    Quote,
    /// A horizontal rule: `---`, `***`, `___`.
    Rule,
}

/// One styled piece of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// UTF-8 byte range in the note.
    pub range: Range<usize>,
    pub kind: SpanKind,
}

/// All styled pieces of `text`, in order and never overlapping.
/// Text that isn't listed is plain.
pub fn spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut fence: Option<(u8, usize)> = None; // (fence char, length) while inside a code block
    let mut line_start = 0;

    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let start = line_start;
        line_start += raw_line.len();
        if line.is_empty() {
            continue;
        }

        if let Some((fence_char, fence_len)) = fence {
            if is_fence(line, fence_char, fence_len) {
                push(&mut spans, start..start + line.len(), SpanKind::Marker);
                fence = None;
            } else {
                push(&mut spans, start..start + line.len(), SpanKind::CodeBlock);
            }
            continue;
        }
        if let Some(opened) = fence_opening(line) {
            push(&mut spans, start..start + line.len(), SpanKind::Marker);
            fence = Some(opened);
            continue;
        }

        line_spans(line, start, &mut spans);
    }
    spans
}

/// Block-level parts of one line (heading, rule, quote, list), then inline parts.
fn line_spans(line: &str, start: usize, spans: &mut Vec<Span>) {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];

    // # Heading
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if indent <= 3 && (1..=6).contains(&hashes) && rest[hashes..].is_empty_or_starts_with_space() {
        let marker_end = indent + hashes + usize::from(!rest[hashes..].is_empty());
        push(spans, start..start + marker_end, SpanKind::Marker);
        if marker_end < line.len() {
            push(
                spans,
                start + marker_end..start + line.len(),
                SpanKind::Heading(hashes as u8),
            );
        }
        return;
    }

    // --- rule
    if indent <= 3 && is_rule(rest) {
        push(spans, start..start + line.len(), SpanKind::Rule);
        return;
    }

    // > quote
    if indent <= 3 && rest.starts_with('>') {
        let marker_len = 1 + usize::from(rest[1..].starts_with(' '));
        push(
            spans,
            start + indent..start + indent + marker_len,
            SpanKind::Marker,
        );
        let quote_start = start + indent + marker_len;
        if quote_start < start + line.len() {
            push(spans, quote_start..start + line.len(), SpanKind::Quote);
        }
        return;
    }

    // - list item, 1. list item, optional [ ] task box
    if let Some(marker_len) = list_marker_len(rest) {
        let mut marker_end = indent + marker_len;
        let after = &line[marker_end..];
        if after.starts_with("[ ] ") || after.starts_with("[x] ") || after.starts_with("[X] ") {
            marker_end += 4;
        }
        push(
            spans,
            start + indent..start + marker_end,
            SpanKind::ListMarker,
        );
        inline_spans(&line[marker_end..], start + marker_end, spans);
        return;
    }

    inline_spans(line, start, spans);
}

/// Inline parts: `code`, [links](url), **bold**, *italic*, ***both***.
fn inline_spans(text: &str, offset: usize, spans: &mut Vec<Span>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = run_length(bytes, i, b'`');
                if let Some(close) = find_run(bytes, i + run, b'`', run) {
                    push(spans, offset + i..offset + i + run, SpanKind::Marker);
                    if close > i + run {
                        push(spans, offset + i + run..offset + close, SpanKind::Code);
                    }
                    push(
                        spans,
                        offset + close..offset + close + run,
                        SpanKind::Marker,
                    );
                    i = close + run;
                    continue;
                }
                i += run;
            }
            // An image `![alt](path)` is styled like a link, with `![` as its marker.
            b'[' | b'!' => {
                let open = usize::from(bytes[i] == b'!'); // length of the "!"
                if bytes[i] == b'!' && bytes.get(i + 1) != Some(&b'[') {
                    i += 1;
                    continue;
                }
                if let Some((text_end, url_end)) = link_at(bytes, i + open) {
                    push(spans, offset + i..offset + i + open + 1, SpanKind::Marker);
                    if text_end > i + open + 1 {
                        push(
                            spans,
                            offset + i + open + 1..offset + text_end,
                            SpanKind::LinkText,
                        );
                    }
                    push(
                        spans,
                        offset + text_end..offset + text_end + 2,
                        SpanKind::Marker,
                    );
                    if url_end > text_end + 2 {
                        push(
                            spans,
                            offset + text_end + 2..offset + url_end,
                            SpanKind::LinkUrl,
                        );
                    }
                    push(
                        spans,
                        offset + url_end..offset + url_end + 1,
                        SpanKind::Marker,
                    );
                    i = url_end + 1;
                    continue;
                }
                i += 1;
            }
            b'~' if run_length(bytes, i, b'~') == 2 => {
                if let Some(close) = emphasis_close(bytes, i, b'~', 2) {
                    push(spans, offset + i..offset + i + 2, SpanKind::Marker);
                    push(spans, offset + i + 2..offset + close, SpanKind::Strike);
                    push(spans, offset + close..offset + close + 2, SpanKind::Marker);
                    i = close + 2;
                    continue;
                }
                i += 2;
            }
            delimiter @ (b'*' | b'_') => {
                let run = run_length(bytes, i, delimiter).min(3);
                if let Some(close) = emphasis_close(bytes, i, delimiter, run) {
                    let kind = match run {
                        1 => SpanKind::Italic,
                        2 => SpanKind::Bold,
                        _ => SpanKind::BoldItalic,
                    };
                    push(spans, offset + i..offset + i + run, SpanKind::Marker);
                    push(spans, offset + i + run..offset + close, kind);
                    push(
                        spans,
                        offset + close..offset + close + run,
                        SpanKind::Marker,
                    );
                    i = close + run;
                    continue;
                }
                i += run_length(bytes, i, delimiter);
            }
            _ => i += 1,
        }
    }
}

/// For an opening delimiter run at `open`, the position of its matching closing run.
fn emphasis_close(bytes: &[u8], open: usize, delimiter: u8, run: usize) -> Option<usize> {
    let inner_start = open + run;
    // The text right after the opener must not be a space ("* item" isn't emphasis).
    if bytes
        .get(inner_start)
        .is_none_or(|b| b.is_ascii_whitespace())
    {
        return None;
    }
    // `_` only counts at word edges, so snake_case_names stay plain.
    if delimiter == b'_' && open > 0 && bytes[open - 1].is_ascii_alphanumeric() {
        return None;
    }
    let mut j = inner_start;
    while j < bytes.len() {
        if bytes[j] == delimiter && run_length(bytes, j, delimiter) == run {
            let before = bytes[j - 1];
            let after = bytes.get(j + run).copied();
            let word_ok = delimiter != b'_' || after.is_none_or(|b| !b.is_ascii_alphanumeric());
            if !before.is_ascii_whitespace() && word_ok {
                return Some(j);
            }
        }
        if bytes[j] == delimiter {
            j += run_length(bytes, j, delimiter);
        } else {
            j += 1;
        }
    }
    None
}

/// For `[` at `open`: the positions of `]` and of the closing `)` of `[text](url)`.
fn link_at(bytes: &[u8], open: usize) -> Option<(usize, usize)> {
    let text_end = open + 1 + bytes[open + 1..].iter().position(|&b| b == b']')?;
    if bytes.get(text_end + 1) != Some(&b'(') {
        return None;
    }
    let url_end = text_end + 2 + bytes[text_end + 2..].iter().position(|&b| b == b')')?;
    Some((text_end, url_end))
}

/// How many `byte`s in a row start at `start`.
fn run_length(bytes: &[u8], start: usize, byte: u8) -> usize {
    bytes[start..].iter().take_while(|&&b| b == byte).count()
}

/// The next run of exactly `len` `byte`s at or after `from`.
fn find_run(bytes: &[u8], from: usize, byte: u8, len: usize) -> Option<usize> {
    let mut j = from;
    while j < bytes.len() {
        if bytes[j] == byte {
            let run = run_length(bytes, j, byte);
            if run == len {
                return Some(j);
            }
            j += run;
        } else {
            j += 1;
        }
    }
    None
}

/// Length of a list marker plus its space (`- `, `* `, `+ `, `12. `, `3) `).
fn list_marker_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    match bytes.first()? {
        b'-' | b'*' | b'+' if bytes.get(1) == Some(&b' ') => Some(2),
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            let ok = digits <= 9
                && matches!(bytes.get(digits), Some(b'.' | b')'))
                && bytes.get(digits + 1) == Some(&b' ');
            ok.then_some(digits + 2)
        }
        _ => None,
    }
}

/// `---`, `***`, `___` (three or more, spaces allowed between).
fn is_rule(text: &str) -> bool {
    let compact: Vec<u8> = text.bytes().filter(|b| *b != b' ').collect();
    compact.len() >= 3
        && matches!(compact[0], b'-' | b'*' | b'_')
        && compact.iter().all(|&b| b == compact[0])
}

/// A code fence opening (```` ``` ```` or `~~~`, three or more), returning its char and length.
fn fence_opening(line: &str) -> Option<(u8, usize)> {
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return None;
    }
    let fence_char = *rest.as_bytes().first()?;
    if fence_char != b'`' && fence_char != b'~' {
        return None;
    }
    let len = run_length(rest.as_bytes(), 0, fence_char);
    (len >= 3).then_some((fence_char, len))
}

/// A line that closes a fence opened with `fence_len` × `fence_char`.
fn is_fence(line: &str, fence_char: u8, fence_len: usize) -> bool {
    let rest = line.trim();
    run_length(rest.as_bytes(), 0, fence_char) >= fence_len && rest.bytes().all(|b| b == fence_char)
}

fn push(spans: &mut Vec<Span>, range: Range<usize>, kind: SpanKind) {
    if !range.is_empty() {
        spans.push(Span { range, kind });
    }
}

/// `str` helper: empty, or starts with a space (what follows a heading's `#`s).
trait EmptyOrSpace {
    fn is_empty_or_starts_with_space(&self) -> bool;
}

impl EmptyOrSpace for str {
    fn is_empty_or_starts_with_space(&self) -> bool {
        self.is_empty() || self.starts_with(' ')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spans as (text, kind) pairs, which are easier to read than byte ranges.
    fn pieces(text: &str) -> Vec<(&str, SpanKind)> {
        spans(text)
            .into_iter()
            .map(|span| (&text[span.range], span.kind))
            .collect()
    }

    use SpanKind::*;

    #[test]
    fn headings_split_marker_and_text() {
        assert_eq!(pieces("# Title"), [("# ", Marker), ("Title", Heading(1))]);
        assert_eq!(pieces("### Deep"), [("### ", Marker), ("Deep", Heading(3))]);
        assert_eq!(pieces("#hashtag"), [], "no space after # is not a heading");
        assert_eq!(pieces("####### seven"), []);
    }

    #[test]
    fn bold_italic_and_both() {
        assert_eq!(
            pieces("a **b** c"),
            [("**", Marker), ("b", Bold), ("**", Marker)]
        );
        assert_eq!(pieces("*i*"), [("*", Marker), ("i", Italic), ("*", Marker)]);
        assert_eq!(
            pieces("***x***"),
            [("***", Marker), ("x", BoldItalic), ("***", Marker)]
        );
        assert_eq!(
            pieces("__b__"),
            [("__", Marker), ("b", Bold), ("__", Marker)]
        );
    }

    #[test]
    fn underscores_inside_words_are_plain() {
        assert_eq!(pieces("snake_case_name"), []);
        assert_eq!(pieces("2 * 3 * 4"), [], "spaced asterisks aren't emphasis");
    }

    #[test]
    fn strikethrough_and_images() {
        assert_eq!(
            pieces("a ~~gone~~ b"),
            [("~~", Marker), ("gone", Strike), ("~~", Marker)]
        );
        assert!(
            !pieces("a ~~~x~~~ b")
                .iter()
                .any(|(_, kind)| *kind == Strike),
            "three tildes aren't strikethrough"
        );
        assert_eq!(
            pieces("![cat](.img/cat.png)"),
            [
                ("![", Marker),
                ("cat", LinkText),
                ("](", Marker),
                (".img/cat.png", LinkUrl),
                (")", Marker)
            ]
        );
        assert!(pieces("wow!").is_empty(), "a lone ! is plain text");
    }

    #[test]
    fn inline_code_hides_formatting_inside() {
        assert_eq!(
            pieces("run `cargo **x**` now"),
            [("`", Marker), ("cargo **x**", Code), ("`", Marker)]
        );
        assert_eq!(
            pieces("``a ` b``"),
            [("``", Marker), ("a ` b", Code), ("``", Marker)]
        );
        assert_eq!(pieces("`unclosed"), []);
    }

    #[test]
    fn links_split_text_and_url() {
        assert_eq!(
            pieces("see [docs](https://x.dev)!"),
            [
                ("[", Marker),
                ("docs", LinkText),
                ("](", Marker),
                ("https://x.dev", LinkUrl),
                (")", Marker)
            ]
        );
        assert_eq!(pieces("[not a link] here"), []);
    }

    #[test]
    fn list_markers_and_task_boxes() {
        assert_eq!(pieces("- item"), [("- ", ListMarker)]);
        assert_eq!(pieces("  12. step"), [("12. ", ListMarker)]);
        assert_eq!(pieces("- [x] done"), [("- [x] ", ListMarker)]);
        assert_eq!(
            pieces("* **bold** item"),
            [
                ("* ", ListMarker),
                ("**", Marker),
                ("bold", Bold),
                ("**", Marker)
            ]
        );
    }

    #[test]
    fn quotes_rules_and_code_blocks() {
        assert_eq!(
            pieces("> wise words"),
            [("> ", Marker), ("wise words", Quote)]
        );
        assert_eq!(pieces("---"), [("---", Rule)]);
        assert_eq!(pieces("* * *"), [("* * *", Rule)]);
        assert_eq!(
            pieces("```rust\nlet x = **1**;\n```\nafter **b**"),
            [
                ("```rust", Marker),
                ("let x = **1**;", CodeBlock),
                ("```", Marker),
                ("**", Marker),
                ("b", Bold),
                ("**", Marker)
            ]
        );
    }

    #[test]
    fn ranges_are_correct_with_multibyte_text_and_crlf() {
        let text = "日本 **語** 👩‍💻\r\n# 見出し\r\n";
        assert_eq!(
            pieces(text),
            [
                ("**", Marker),
                ("語", Bold),
                ("**", Marker),
                ("# ", Marker),
                ("見出し", Heading(1))
            ]
        );
    }

    #[test]
    fn spans_are_ordered_and_never_overlap() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/sample-vault/Example.md"
        ))
        .unwrap();
        let spans = spans(&text);
        assert!(!spans.is_empty());
        for pair in spans.windows(2) {
            assert!(pair[0].range.end <= pair[1].range.start, "{pair:?}");
        }
        for span in &spans {
            assert!(
                text.is_char_boundary(span.range.start) && text.is_char_boundary(span.range.end)
            );
        }
    }
}
