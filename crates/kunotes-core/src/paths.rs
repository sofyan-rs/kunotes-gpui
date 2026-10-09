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

/// The part of a name to pre-select when renaming: everything except a `.md`
/// extension for notes (so typing replaces "Plan" in "Plan.md"), the whole name
/// for folders. A byte range into `name`.
pub fn rename_selection(name: &str, is_dir: bool) -> std::ops::Range<usize> {
    let has_md = name.len() > 3 && name.to_lowercase().ends_with(".md");
    if !is_dir && has_md {
        0..name.len() - 3
    } else {
        0..name.len()
    }
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

/// True if `path` is hidden inside the vault: some part of it below `root`
/// starts with "." (like `.git/HEAD` or `.note.md.kunotes.tmp`).
/// Paths outside `root` count as hidden, since they aren't part of the vault.
pub fn is_hidden_within(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative
        .components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
}

/// True if moving `path` into `folder` would actually do something: the folder
/// isn't the item itself, isn't inside it, and isn't where the item already is.
/// (Only compares paths; `fs_ops::move_into` still checks the disk.)
pub fn can_move_into(path: &Path, folder: &Path) -> bool {
    !folder.starts_with(path) && path.parent() != Some(folder)
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
    fn hidden_paths_are_detected_anywhere_below_root() {
        assert!(is_hidden_within(&root(), &root().join(".git").join("HEAD")));
        assert!(is_hidden_within(
            &root(),
            &root().join("a").join(".b.md.kunotes.tmp")
        ));
        assert!(!is_hidden_within(&root(), &root().join("a").join("b.md")));
        assert!(
            !is_hidden_within(&root(), &root()),
            "the vault root itself is visible"
        );
        assert!(is_hidden_within(&root(), Path::new("elsewhere/a.md")));
    }

    #[test]
    fn can_move_into_rejects_self_descendants_and_current_folder() {
        let projects = root().join("Projects");
        let plan = projects.join("Plan.md");
        assert!(can_move_into(&plan, &root()));
        assert!(can_move_into(&plan, &root().join("Other")));
        assert!(!can_move_into(&plan, &projects), "already there");
        assert!(!can_move_into(&projects, &projects), "into itself");
        assert!(
            !can_move_into(&projects, &projects.join("Archive")),
            "into its own child"
        );
        // "Projects2" only shares a text prefix with "Projects".
        assert!(can_move_into(&projects, &root().join("Projects2")));
    }

    #[test]
    fn rename_selection_skips_only_the_md_extension_of_notes() {
        assert_eq!(rename_selection("PLAN.md", false), 0..4);
        assert_eq!(rename_selection("Shout.MD", false), 0..5);
        assert_eq!(rename_selection("v1.2.md", false), 0..4);
        assert_eq!(rename_selection("notes.txt", false), 0..9);
        assert_eq!(
            rename_selection("Folder.md", true),
            0..9,
            "folders: whole name"
        );
        assert_eq!(rename_selection(".md", false), 0..3);
        assert_eq!(rename_selection("日本.md", false), 0..6);
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
