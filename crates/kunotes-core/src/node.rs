//! `VaultNode`: the in-memory tree of a vault folder, built by scanning the disk.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

/// One folder or markdown file in the vault.
///
/// Files have an empty `children` list. Folders list their contents sorted
/// folders-first, then by natural name order ("a2" before "a10").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<VaultNode>,
}

impl VaultNode {
    /// Scans `root` and everything below it into a tree.
    ///
    /// Only folders and `.md` files are kept. Hidden entries (names starting with
    /// `.`) are skipped, and symlinked folders are not followed, to avoid loops.
    /// Folders that can't be read show up as empty instead of failing the scan.
    pub fn scan(root: &Path) -> VaultNode {
        VaultNode {
            path: root.to_path_buf(),
            name: file_name_of(root),
            is_dir: true,
            children: scan_children(root),
        }
    }
}

/// Returns true if `path` is a note: a `.md` file, or a locked note
/// (`.md.age`, see `lock.rs`). Any letter case.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        || crate::lock::is_locked_note(path)
}

fn scan_children(dir: &Path) -> Vec<VaultNode> {
    // An unreadable folder (permissions, deleted mid-scan) is shown as empty.
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut nodes: Vec<VaultNode> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| node_for_entry(&entry))
        .collect();

    nodes.sort_by(compare_nodes);
    nodes
}

/// Turns one directory entry into a node, or `None` if it should be hidden.
fn node_for_entry(entry: &fs::DirEntry) -> Option<VaultNode> {
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    if name.starts_with('.') {
        return None;
    }

    // `file_type()` does not follow symlinks, so a symlinked folder reports as a symlink.
    let file_type = entry.file_type().ok()?;
    if file_type.is_dir() {
        return Some(VaultNode {
            children: scan_children(&path),
            path,
            name,
            is_dir: true,
        });
    }

    // Plain files and symlinks to files: keep markdown only.
    // `path.is_file()` follows the symlink, so a link to a folder is skipped here.
    if is_markdown(&path) && path.is_file() {
        return Some(VaultNode {
            path,
            name,
            is_dir: false,
            children: Vec::new(),
        });
    }

    None
}

/// Folders first, then natural, case-insensitive name order.
fn compare_nodes(a: &VaultNode, b: &VaultNode) -> Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| natord::compare_ignore_case(&a.name, &b.name))
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_extension_is_case_insensitive() {
        assert!(is_markdown(Path::new("a.md")));
        assert!(is_markdown(Path::new("a.MD")));
        assert!(!is_markdown(Path::new("a.txt")));
        assert!(!is_markdown(Path::new("md")));
    }

    #[test]
    fn sorts_folders_first_then_natural_order() {
        let node = |name: &str, is_dir: bool| VaultNode {
            path: PathBuf::from(name),
            name: name.to_string(),
            is_dir,
            children: Vec::new(),
        };
        let mut nodes = [
            node("b.md", false),
            node("a10.md", false),
            node("Zeta", true),
            node("a2.md", false),
            node("alpha", true),
        ];
        nodes.sort_by(compare_nodes);

        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Zeta", "a2.md", "a10.md", "b.md"]);
    }
}
