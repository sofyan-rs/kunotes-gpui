//! Pure logic for KuNotes: vault scanning, file operations, name validation,
//! path helpers, markdown formatting transforms. No GPUI dependency.

/// Application name used for config directories and window titles.
pub const APP_NAME: &str = "KuNotes";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_set() {
        assert_eq!(APP_NAME, "KuNotes");
    }
}
