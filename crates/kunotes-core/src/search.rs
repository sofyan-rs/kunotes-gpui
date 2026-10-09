//! Turning the vault tree into flat lists: all files (for the quick switcher)
//! and the currently visible rows (for the file tree view).

use std::collections::HashSet;
use std::path::PathBuf;

use crate::node::VaultNode;

/// Every markdown file under `root`, in tree order. Folders themselves are left out.
pub fn flatten_files(root: &VaultNode) -> Vec<&VaultNode> {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    files
}

fn collect_files<'a>(node: &'a VaultNode, files: &mut Vec<&'a VaultNode>) {
    for child in &node.children {
        if child.is_dir {
            collect_files(child, files);
        } else {
            files.push(child);
        }
    }
}

/// Files whose name contains `query`, ignoring letter case.
/// An empty (or whitespace-only) query matches everything.
pub fn filter_files<'a>(files: &[&'a VaultNode], query: &str) -> Vec<&'a VaultNode> {
    let query = query.trim().to_lowercase();
    files
        .iter()
        .copied()
        .filter(|file| query.is_empty() || file.name.to_lowercase().contains(&query))
        .collect()
}

/// One row the file tree should draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleRow {
    pub path: PathBuf,
    pub name: String,
    /// 0 for items directly inside the vault root.
    pub depth: usize,
    pub is_dir: bool,
    pub is_expanded: bool,
}

/// The rows to draw for `root`, given which folders are expanded.
/// The vault root itself is not a row; its children are at depth 0.
pub fn visible_rows(root: &VaultNode, expanded: &HashSet<PathBuf>) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    collect_rows(root, expanded, 0, &mut rows);
    rows
}

fn collect_rows(
    node: &VaultNode,
    expanded: &HashSet<PathBuf>,
    depth: usize,
    rows: &mut Vec<VisibleRow>,
) {
    for child in &node.children {
        let is_expanded = child.is_dir && expanded.contains(&child.path);
        rows.push(VisibleRow {
            path: child.path.clone(),
            name: child.name.clone(),
            depth,
            is_dir: child.is_dir,
            is_expanded,
        });
        if is_expanded {
            collect_rows(child, expanded, depth + 1, rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> VaultNode {
        VaultNode {
            path: PathBuf::from(path),
            name: path.rsplit('/').next().unwrap().to_string(),
            is_dir: false,
            children: Vec::new(),
        }
    }

    fn folder(path: &str, children: Vec<VaultNode>) -> VaultNode {
        VaultNode {
            is_dir: true,
            children,
            ..file(path)
        }
    }

    /// vault/
    ///   Projects/
    ///     Archive/old.md
    ///     Plan.md
    ///   Welcome.md
    fn sample() -> VaultNode {
        folder(
            "vault",
            vec![
                folder(
                    "vault/Projects",
                    vec![
                        folder(
                            "vault/Projects/Archive",
                            vec![file("vault/Projects/Archive/old.md")],
                        ),
                        file("vault/Projects/Plan.md"),
                    ],
                ),
                file("vault/Welcome.md"),
            ],
        )
    }

    fn names(nodes: &[&VaultNode]) -> Vec<String> {
        nodes.iter().map(|n| n.name.clone()).collect()
    }

    #[test]
    fn flatten_returns_only_files_in_tree_order() {
        let root = sample();
        assert_eq!(
            names(&flatten_files(&root)),
            ["old.md", "Plan.md", "Welcome.md"]
        );
    }

    #[test]
    fn filter_is_case_insensitive_and_empty_matches_all() {
        let root = sample();
        let files = flatten_files(&root);
        assert_eq!(names(&filter_files(&files, "PLAN")), ["Plan.md"]);
        assert_eq!(filter_files(&files, "  ").len(), 3);
        assert!(filter_files(&files, "nothing").is_empty());
    }

    #[test]
    fn collapsed_tree_shows_only_top_level() {
        let rows = visible_rows(&sample(), &HashSet::new());
        let shown: Vec<(&str, usize)> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(shown, [("Projects", 0), ("Welcome.md", 0)]);
        assert!(!rows[0].is_expanded);
    }

    #[test]
    fn expanded_folders_show_children_with_depth() {
        let expanded: HashSet<PathBuf> = [
            PathBuf::from("vault/Projects"),
            PathBuf::from("vault/Projects/Archive"),
        ]
        .into();
        let rows = visible_rows(&sample(), &expanded);
        let shown: Vec<(&str, usize)> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(
            shown,
            [
                ("Projects", 0),
                ("Archive", 1),
                ("old.md", 2),
                ("Plan.md", 1),
                ("Welcome.md", 0)
            ]
        );
    }

    #[test]
    fn expanded_child_of_collapsed_parent_stays_hidden() {
        let expanded: HashSet<PathBuf> = [PathBuf::from("vault/Projects/Archive")].into();
        let rows = visible_rows(&sample(), &expanded);
        assert_eq!(rows.len(), 2);
    }
}
