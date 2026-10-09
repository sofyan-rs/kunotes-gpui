//! Markdown formatting for the formatter bar (bold, headings, links, ...).
//!
//! Every function takes the full text and the current selection, and returns an
//! [`Edit`] describing what to replace. The editor applies it as one undo step.
//!
//! All ranges are UTF-8 **byte** offsets, the same units the editor uses.

use std::ops::Range;

/// "Replace `range` with `replacement`, then select `new_selection`."
///
/// `new_selection` is given in the text *after* the edit. An empty range
/// (`start == end`) means "put the cursor there".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
    pub new_selection: Range<usize>,
}

impl Edit {
    /// Returns `text` with this edit applied. The app uses the editor's own
    /// replace instead; this is mainly for tests.
    pub fn apply(&self, text: &str) -> String {
        let mut result = text.to_string();
        result.replace_range(self.range.clone(), &self.replacement);
        result
    }
}

/// `**selection**`
pub fn bold(text: &str, selection: Range<usize>) -> Edit {
    wrap(text, selection, "**", "**")
}

/// `*selection*`
pub fn italic(text: &str, selection: Range<usize>) -> Edit {
    wrap(text, selection, "*", "*")
}

/// `` `selection` ``
pub fn inline_code(text: &str, selection: Range<usize>) -> Edit {
    wrap(text, selection, "`", "`")
}

/// Puts `prefix` before and `suffix` after the selection and keeps the
/// original text selected (or puts the cursor between them if nothing was selected).
pub fn wrap(text: &str, selection: Range<usize>, prefix: &str, suffix: &str) -> Edit {
    let selection = clamp(text, selection);
    let selected = &text[selection.clone()];
    let inner_start = selection.start + prefix.len();
    Edit {
        replacement: format!("{prefix}{selected}{suffix}"),
        new_selection: inner_start..inner_start + selected.len(),
        range: selection,
    }
}

/// Turns every selected line into a heading of `level` (1-6), replacing any
/// existing heading marker instead of stacking them.
pub fn heading(text: &str, selection: Range<usize>, level: usize) -> Edit {
    let marker = format!("{} ", "#".repeat(level.clamp(1, 6)));
    transform_lines(text, selection, |_, line| {
        format!("{marker}{}", strip_heading(line))
    })
}

/// Prefixes every selected line with `> `.
pub fn quote(text: &str, selection: Range<usize>) -> Edit {
    transform_lines(text, selection, |_, line| format!("> {line}"))
}

/// Prefixes every selected line with `- `.
pub fn bullet_list(text: &str, selection: Range<usize>) -> Edit {
    transform_lines(text, selection, |_, line| format!("- {line}"))
}

/// Prefixes the selected lines with `1. `, `2. `, `3. `, ...
pub fn numbered_list(text: &str, selection: Range<usize>) -> Edit {
    transform_lines(text, selection, |index, line| {
        format!("{}. {line}", index + 1)
    })
}

/// `[selection](url)` with "url" selected so the user can type the link target.
/// With nothing selected, the cursor goes between the brackets instead.
pub fn link(text: &str, selection: Range<usize>) -> Edit {
    let selection = clamp(text, selection);
    let selected = &text[selection.clone()];
    let new_selection = if selected.is_empty() {
        let inside_brackets = selection.start + 1;
        inside_brackets..inside_brackets
    } else {
        let url_start = selection.start + 1 + selected.len() + 2; // after "[" + text + "]("
        url_start..url_start + "url".len()
    };
    Edit {
        replacement: format!("[{selected}](url)"),
        new_selection,
        range: selection,
    }
}

/// Wraps the selection in a fenced code block on its own lines.
pub fn code_block(text: &str, selection: Range<usize>) -> Edit {
    let selection = clamp(text, selection);
    let selected = &text[selection.clone()];
    let before = if selection.start == line_start(text, selection.start) {
        ""
    } else {
        "\n"
    };
    let after = if selection.end == line_end(text, selection.end) {
        ""
    } else {
        "\n"
    };

    let inner_start = selection.start + before.len() + "```\n".len();
    Edit {
        replacement: format!("{before}```\n{selected}\n```{after}"),
        new_selection: inner_start..inner_start + selected.len(),
        range: selection,
    }
}

/// Replaces the selection with a `---` rule surrounded by blank lines,
/// and puts the cursor after it.
pub fn horizontal_rule(text: &str, selection: Range<usize>) -> Edit {
    let selection = clamp(text, selection);
    let text_before = &text[..selection.start];
    let text_after = &text[selection.end..];

    let before = if text_before.is_empty() || text_before.ends_with("\n\n") {
        ""
    } else if text_before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let after = if text_after.starts_with("\n\n") {
        ""
    } else if text_after.starts_with('\n') {
        "\n"
    } else {
        "\n\n"
    };

    let replacement = format!("{before}---{after}");
    let cursor = selection.start + replacement.len();
    Edit {
        replacement,
        new_selection: cursor..cursor,
        range: selection,
    }
}

/// Rewrites every line touched by `selection` with `transform(line_index, line)`.
fn transform_lines(
    text: &str,
    selection: Range<usize>,
    transform: impl Fn(usize, &str) -> String,
) -> Edit {
    let selection = clamp(text, selection);

    // A selection that ends exactly at the start of a line (e.g. after
    // selecting whole lines) does not include that next line.
    let mut last = selection.end;
    if selection.end > selection.start && selection.end == line_start(text, selection.end) {
        last -= 1;
    }
    let start = line_start(text, selection.start);
    let end = line_end(text, last);
    let block = &text[start..end];

    let new_lines: Vec<String> = block
        .split('\n')
        .enumerate()
        .map(|(index, line)| transform(index, line))
        .collect();
    let replacement = new_lines.join("\n");

    // Shift the selection by how much its first line and the whole block grew.
    let first_old_len = block.split('\n').next().unwrap_or("").len();
    let first_delta = new_lines[0].len() as isize - first_old_len as isize;
    let total_delta = replacement.len() as isize - block.len() as isize;

    let new_start = shift(selection.start, first_delta).max(start);
    let new_end = shift(selection.end, total_delta).max(new_start);

    Edit {
        range: start..end,
        replacement,
        new_selection: new_start..new_end,
    }
}

/// Removes a leading ATX heading marker (`#` to `######` followed by a space).
fn strip_heading(line: &str) -> &str {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        &line[hashes + 1..]
    } else {
        line
    }
}

/// Byte offset where the line containing `index` starts.
fn line_start(text: &str, index: usize) -> usize {
    text[..index].rfind('\n').map_or(0, |newline| newline + 1)
}

/// Byte offset where the line containing `index` ends (before its `\n`).
fn line_end(text: &str, index: usize) -> usize {
    text[index..]
        .find('\n')
        .map_or(text.len(), |newline| index + newline)
}

/// Keeps a range inside `text`, ordered, and on character boundaries,
/// so slicing it can never panic.
fn clamp(text: &str, range: Range<usize>) -> Range<usize> {
    let mut start = range.start.min(text.len());
    let mut end = range.end.clamp(start, text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    while !text.is_char_boundary(end) {
        end += 1;
    }
    start..end
}

fn shift(offset: usize, delta: isize) -> usize {
    offset.saturating_add_signed(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies `edit` and returns the new text plus the newly selected text.
    fn run(text: &str, edit: Edit) -> (String, String) {
        let result = edit.apply(text);
        let selected = result[edit.new_selection.clone()].to_string();
        (result, selected)
    }

    #[test]
    fn bold_wraps_selection_and_keeps_it_selected() {
        let text = "make this bold";
        let (result, selected) = run(text, bold(text, 10..14));
        assert_eq!(result, "make this **bold**");
        assert_eq!(selected, "bold");
    }

    #[test]
    fn wrap_without_selection_puts_cursor_between_markers() {
        let text = "ab";
        let edit = italic(text, 1..1);
        assert_eq!(edit.apply(text), "a**b");
        assert_eq!(edit.new_selection, 2..2);
    }

    #[test]
    fn inline_code_works_at_start_and_end_of_text() {
        assert_eq!(inline_code("x", 0..1).apply("x"), "`x`");
        assert_eq!(inline_code("", 0..0).apply(""), "``");
    }

    #[test]
    fn wrap_handles_multibyte_text() {
        let text = "hi 👩‍💻 日本";
        let start = text.find('日').unwrap();
        let (result, selected) = run(text, bold(text, start..text.len()));
        assert_eq!(result, "hi 👩‍💻 **日本**");
        assert_eq!(selected, "日本");
    }

    #[test]
    fn out_of_range_or_mid_char_selection_never_panics() {
        let text = "日本";
        let edit = bold(text, 1..100); // 1 is inside '日'
        assert_eq!(edit.apply(text), "**日本**");
    }

    #[test]
    fn heading_applies_to_current_line_from_anywhere_in_it() {
        let text = "first\nsecond line\nthird";
        let (result, _) = run(text, heading(text, 9..9, 1));
        assert_eq!(result, "first\n# second line\nthird");
    }

    #[test]
    fn heading_replaces_existing_marker() {
        let text = "## Title";
        assert_eq!(heading(text, 0..0, 1).apply(text), "# Title");
        assert_eq!(heading("#tag", 0..0, 2).apply("#tag"), "## #tag");
    }

    #[test]
    fn line_prefix_covers_every_selected_line() {
        let text = "a\nb\nc";
        let (result, selected) = run(text, quote(text, 0..3));
        assert_eq!(result, "> a\n> b\nc");
        assert_eq!(selected, "a\n> b");
    }

    #[test]
    fn selection_ending_at_line_start_excludes_that_line() {
        let text = "a\nb\nc";
        // "a\n" selected: the cursor sits at the start of "b".
        assert_eq!(bullet_list(text, 0..2).apply(text), "- a\nb\nc");
    }

    #[test]
    fn numbered_list_counts_up() {
        let text = "x\ny\nz";
        assert_eq!(
            numbered_list(text, 0..text.len()).apply(text),
            "1. x\n2. y\n3. z"
        );
    }

    #[test]
    fn line_prefix_on_empty_text() {
        let edit = bullet_list("", 0..0);
        assert_eq!(edit.apply(""), "- ");
        assert_eq!(edit.new_selection, 2..2);
    }

    #[test]
    fn link_selects_url_placeholder() {
        let text = "see docs";
        let (result, selected) = run(text, link(text, 4..8));
        assert_eq!(result, "see [docs](url)");
        assert_eq!(selected, "url");
    }

    #[test]
    fn link_without_selection_puts_cursor_in_brackets() {
        let edit = link("", 0..0);
        assert_eq!(edit.apply(""), "[](url)");
        assert_eq!(edit.new_selection, 1..1);
    }

    #[test]
    fn code_block_goes_on_its_own_lines() {
        let text = "before code after";
        let (result, selected) = run(text, code_block(text, 7..11));
        assert_eq!(result, "before \n```\ncode\n```\n after");
        assert_eq!(selected, "code");

        let whole_line = "code";
        assert_eq!(
            code_block(whole_line, 0..4).apply(whole_line),
            "```\ncode\n```"
        );
    }

    #[test]
    fn horizontal_rule_adds_blank_lines_only_when_needed() {
        assert_eq!(horizontal_rule("", 0..0).apply(""), "---\n\n");
        assert_eq!(
            horizontal_rule("text", 4..4).apply("text"),
            "text\n\n---\n\n"
        );
        assert_eq!(
            horizontal_rule("a\n\nb", 3..3).apply("a\n\nb"),
            "a\n\n---\n\nb"
        );

        let edit = horizontal_rule("text", 4..4);
        assert_eq!(edit.new_selection, 11..11);
    }
}
