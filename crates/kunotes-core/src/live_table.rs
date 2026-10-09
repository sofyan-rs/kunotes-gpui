//! Markdown tables for Live mode: finding them and splitting rows into cells.
//!
//! A table is a header row, a delimiter row (`| --- | :---: |`), then body
//! rows, every one containing `|`:
//!
//! ```text
//! | Name | Age |
//! | ---  | --- |
//! | Ann  | 31  |
//! ```
//!
//! Ranges are UTF-8 byte offsets.

use std::ops::Range;

use crate::live_view::Line;

/// Which lines (by index into `lines`) form tables in `text`.
pub fn tables(text: &str, lines: &[Line]) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut index = 0;
    while index + 1 < lines.len() {
        let header = &text[lines[index].range.clone()];
        let delimiter = &text[lines[index + 1].range.clone()];
        let columns = cells(header).len();
        if header.contains('|') && is_delimiter_row(delimiter) && cells(delimiter).len() == columns
        {
            let mut end = index + 2;
            while end < lines.len() && is_body_row(&text[lines[end].range.clone()]) {
                end += 1;
            }
            found.push(index..end);
            index = end;
        } else {
            index += 1;
        }
    }
    found
}

/// The cells of a table row: each cell's text without the `|`s and the spaces
/// around it. A `\|` inside a cell is part of the text.
pub fn cells(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut pipes = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2, // skip the escaped character
            b'|' => {
                pipes.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
    // Cell boundaries: the start, every pipe, the end. A leading or trailing
    // pipe doesn't start an extra empty cell.
    let mut bounds = vec![0];
    for &pipe in &pipes {
        bounds.push(pipe);
    }
    bounds.push(line.len());
    let trimmed_start = line.len() - line.trim_start().len();
    let trimmed_end = line.trim_end().len();
    let mut result = Vec::new();
    for pair in bounds.windows(2) {
        let start = if pair[0] == 0 && !pipes.contains(&0) {
            0
        } else {
            pair[0] + 1
        };
        let end = pair[1];
        let is_outer_edge = (pipes.first() == Some(&pair[1]) && pair[1] == trimmed_start)
            || (pipes.last() == Some(&pair[0]) && pair[0] + 1 == trimmed_end);
        if start > end || is_outer_edge {
            continue;
        }
        let cell = &line[start..end];
        let lead = cell.len() - cell.trim_start().len();
        let content = cell.trim();
        result.push(start + lead..start + lead + content.len());
    }
    result
}

/// `| --- | :---: |`: every cell is dashes, optionally with `:` at either end.
pub fn is_delimiter_row(line: &str) -> bool {
    if !line.contains('-') || !line.contains('|') {
        return false;
    }
    let row = cells(line);
    !row.is_empty()
        && row.iter().all(|cell| {
            let cell = line[cell.clone()]
                .trim_start_matches(':')
                .trim_end_matches(':');
            !cell.is_empty() && cell.bytes().all(|b| b == b'-')
        })
}

fn is_body_row(line: &str) -> bool {
    line.contains('|') && !line.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_view::lines;

    fn texts(line: &str) -> Vec<&str> {
        cells(line).into_iter().map(|range| &line[range]).collect()
    }

    #[test]
    fn cells_drop_outer_pipes_and_spaces() {
        assert_eq!(texts("| Name | Age |"), ["Name", "Age"]);
        assert_eq!(texts("Name | Age"), ["Name", "Age"]);
        assert_eq!(texts("|  |  |"), ["", ""]);
        assert_eq!(texts("| a \\| b | 日本 |"), ["a \\| b", "日本"]);
    }

    #[test]
    fn delimiter_rows() {
        assert!(is_delimiter_row("| --- | :---: |"));
        assert!(is_delimiter_row("---|---"));
        assert!(!is_delimiter_row("| a | b |"));
        assert!(!is_delimiter_row("---"), "a rule isn't a table delimiter");
    }

    #[test]
    fn tables_are_found_with_their_body_rows() {
        let text = "intro\n| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n\nafter";
        let all = lines(text);
        assert_eq!(tables(text, &all), vec![1..5]);
    }

    #[test]
    fn a_header_without_a_matching_delimiter_is_not_a_table() {
        let text = "| A | B |\n| --- |\n";
        assert!(tables(text, &lines(text)).is_empty());
        let text = "a | b\nplain";
        assert!(tables(text, &lines(text)).is_empty());
    }
}
