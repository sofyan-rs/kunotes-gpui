//! Path helpers: vault-relative paths, breadcrumbs, note titles, and remapping
//! paths after a rename or move.

use std::path::{Path, PathBuf};

use crate::node::is_markdown;

/// The path of `path` relative to `root`, joined with `/` on every OS
/// (handy for markdown links). Returns `None` if `path` is outside `root`.
pub fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(parts.join("/"))
}

/// The display title of a note: the file name without a `.md` extension.
/// Folders and other files keep their full name.
pub fn note_title(path: &Path) -> String {
    let name = if is_markdown(path) {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Breadcrumb parts for a file, relative to the vault root.
///
/// `root/Projects/Plan.md` gives `["Projects", "Plan"]`. If the file is not
/// inside the vault, only its title is returned.
pub fn breadcrumb(root: &Path, file: &Path) -> Vec<String> {
    let Ok(relative) = file.strip_prefix(root) else {
        return vec![note_title(file)];
    };

    let mut parts: Vec<String> = relative
        .parent()
        .into_iter()
        .flat_map(|parent| parent.components())
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.push(note_title(file));
    parts
}

/// After `old` was renamed or moved to `new`, returns where `path` lives now.
///
/// Returns `None` if `path` was not `old` or inside it. Used to keep the
/// selection and expanded folders correct after a rename or move.
pub fn remap_path(path: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(old).ok()?;
    if rest.as_os_str().is_empty() {
        Some(new.to_path_buf())
    } else {
        Some(new.join(rest))
    }
}

/// All folders between `root` (exclusive) and `path` (exclusive), outermost first.
/// Used to expand the tree so a file becomes visible.
pub fn ancestors_within(root: &Path, path: &Path) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = path
        .ancestors()
        .skip(1)
        .take_while(|folder| *folder != root && folder.starts_with(root))
        .map(Path::to_path_buf)
        .collect();
    folders.reverse();
    folders
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("vault")
    }

    #[test]
    fn relative_path_uses_forward_slashes() {
        let file = root().join("Projects").join("Plan.md");
        assert_eq!(
            relative_path(&root(), &file).as_deref(),
            Some("Projects/Plan.md")
        );
        assert_eq!(relative_path(&root(), Path::new("elsewhere/a.md")), None);
    }

    #[test]
    fn note_title_strips_only_markdown_extension() {
        assert_eq!(note_title(Path::new("a/Plan.md")), "Plan");
        assert_eq!(note_title(Path::new("a/Shout.MD")), "Shout");
        assert_eq!(note_title(Path::new("a/v1.2")), "v1.2");
        assert_eq!(note_title(Path::new("a/Folder")), "Folder");
    }

    #[test]
    fn breadcrumb_omits_vault_name_and_extension() {
        let file = root().join("Projects").join("Archive").join("old.md");
        assert_eq!(breadcrumb(&root(), &file), ["Projects", "Archive", "old"]);
        assert_eq!(breadcrumb(&root(), &root().join("Top.md")), ["Top"]);
    }

    #[test]
    fn breadcrumb_outside_vault_falls_back_to_title() {
        assert_eq!(breadcrumb(&root(), Path::new("other/Note.md")), ["Note"]);
    }

    #[test]
    fn remap_handles_exact_match_and_descendants() {
        let old = root().join("Projects");
        let new = root().join("Work");
        assert_eq!(remap_path(&old, &old, &new), Some(new.clone()));
        assert_eq!(
            remap_path(&old.join("Plan.md"), &old, &new),
            Some(new.join("Plan.md"))
        );
        assert_eq!(remap_path(&root().join("Other.md"), &old, &new), None);
        // "Projects2" only shares a text prefix with "Projects"; it is not inside it.
        assert_eq!(remap_path(&root().join("Projects2"), &old, &new), None);
    }

    #[test]
    fn ancestors_within_lists_folders_outermost_first() {
        let file = root().join("a").join("b").join("c.md");
        assert_eq!(
            ancestors_within(&root(), &file),
            [root().join("a"), root().join("a").join("b")]
        );
        assert!(ancestors_within(&root(), &root().join("top.md")).is_empty());
    }
}
