//! Numbers for the status bar: cursor line/column and character count.

use unicode_segmentation::UnicodeSegmentation;

/// The 1-based (line, column) of a byte `offset` in `text`.
///
/// Columns count characters, not bytes, so "é" or "日" move the column by one.
/// An offset past the end (or inside a character) is clamped safely.
pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }

    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

/// The number of characters as a person would count them.
///
/// Uses grapheme clusters, so an emoji like "👩‍💻" (several code points) counts as one.
pub fn char_count(text: &str) -> usize {
    text.graphemes(true).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_starts_at_one() {
        assert_eq!(line_col("", 0), (1, 1));
        assert_eq!(line_col("abc", 0), (1, 1));
        assert_eq!(line_col("abc", 3), (1, 4));
    }

    #[test]
    fn line_col_counts_lines() {
        let text = "one\ntwo\nthree";
        assert_eq!(line_col(text, 4), (2, 1));
        assert_eq!(line_col(text, 6), (2, 3));
        assert_eq!(line_col(text, text.len()), (3, 6));
    }

    #[test]
    fn line_col_handles_crlf() {
        let text = "a\r\nb";
        assert_eq!(line_col(text, 3), (2, 1));
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let text = "日本語";
        assert_eq!(line_col(text, text.len()), (1, 4));
        // Offset 1 is inside '日'; it is clamped back to the character start.
        assert_eq!(line_col(text, 1), (1, 1));
        assert_eq!(line_col(text, 999), (1, 4));
    }

    #[test]
    fn char_count_uses_graphemes() {
        assert_eq!(char_count(""), 0);
        assert_eq!(char_count("abc"), 3);
        assert_eq!(char_count("café"), 4);
        assert_eq!(char_count("👩‍💻🇮🇩"), 2);
    }
}
