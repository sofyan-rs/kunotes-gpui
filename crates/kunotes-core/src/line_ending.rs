//! Keeping a file's line endings (`\n` or `\r\n`) unchanged across load and save.
//!
//! The editor always works with `\n`. When a note is opened we remember its
//! original style, and convert back when saving, so Windows-made files stay
//! `\r\n` and git doesn't see every line as changed.

/// The line ending style of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    /// Detects the style from the first line break. Files without any line break count as `\n`.
    pub fn detect(text: &str) -> LineEnding {
        match text.find('\n') {
            Some(index) if index > 0 && text.as_bytes()[index - 1] == b'\r' => LineEnding::CrLf,
            _ => LineEnding::Lf,
        }
    }

    /// Converts editor text (`\n`) back to this style for saving.
    pub fn apply(self, text: &str) -> String {
        match self {
            LineEnding::Lf => text.to_string(),
            LineEnding::CrLf => text.replace('\n', "\r\n"),
        }
    }
}

/// Converts any `\r\n` to `\n`, for loading into the editor.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_style_from_first_line_break() {
        assert_eq!(LineEnding::detect("a\nb"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\r\nb"), LineEnding::CrLf);
        assert_eq!(LineEnding::detect("no break"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("\nstarts with break"), LineEnding::Lf);
    }

    #[test]
    fn round_trip_keeps_crlf_files_identical() {
        let original = "# Title\r\n\r\nBody\r\n";
        let style = LineEnding::detect(original);
        let in_editor = normalize(original);
        assert_eq!(in_editor, "# Title\n\nBody\n");
        assert_eq!(style.apply(&in_editor), original);
    }

    #[test]
    fn lf_files_are_unchanged() {
        let original = "a\nb\n";
        assert_eq!(
            LineEnding::detect(original).apply(&normalize(original)),
            original
        );
    }
}
