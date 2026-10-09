//! Integration tests for scanning a vault folder into a tree, using real temporary folders.

use std::fs;
use std::path::Path;

use kunotes_core::VaultNode;
use tempfile::tempdir;

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, "").unwrap();
}

fn child_names(node: &VaultNode) -> Vec<&str> {
    node.children.iter().map(|c| c.name.as_str()).collect()
}

#[test]
fn scan_keeps_folders_and_markdown_only() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    touch(&root.join("Welcome.md"));
    touch(&root.join("Shout.MD"));
    touch(&root.join("notes.txt"));
    touch(&root.join(".hidden.md"));
    touch(&root.join(".git").join("config"));
    touch(&root.join(".obsidian").join("app.json"));
    touch(&root.join("Projects").join("a10.md"));
    touch(&root.join("Projects").join("a2.md"));
    fs::create_dir(root.join("Empty")).unwrap();

    let tree = VaultNode::scan(root);
    assert!(tree.is_dir);
    assert_eq!(
        child_names(&tree),
        ["Empty", "Projects", "Shout.MD", "Welcome.md"]
    );

    let projects = &tree.children[1];
    assert_eq!(child_names(projects), ["a2.md", "a10.md"]);
    assert!(
        projects
            .children
            .iter()
            .all(|c| !c.is_dir && c.children.is_empty())
    );
}

#[test]
fn scan_of_sample_vault_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-vault");
    let tree = VaultNode::scan(&root);
    assert_eq!(
        child_names(&tree),
        ["Journal", "Projects", "Example.md", "Welcome.md"]
    );
}

#[test]
fn scan_of_missing_folder_is_empty() {
    let dir = tempdir().unwrap();
    let tree = VaultNode::scan(&dir.path().join("does-not-exist"));
    assert!(tree.children.is_empty());
}

#[cfg(unix)]
#[test]
fn scan_does_not_follow_folder_symlinks() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    touch(&root.join("Real").join("note.md"));
    // A link back to the root would loop forever if followed.
    std::os::unix::fs::symlink(root, root.join("Real").join("loop")).unwrap();
    std::os::unix::fs::symlink(root.join("Real").join("note.md"), root.join("linked.md")).unwrap();

    let tree = VaultNode::scan(root);
    assert_eq!(child_names(&tree), ["Real", "linked.md"]);
    assert_eq!(child_names(&tree.children[0]), ["note.md"]);
}
