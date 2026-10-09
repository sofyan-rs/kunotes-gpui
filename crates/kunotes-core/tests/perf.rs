//! Rough performance checks for big vaults and notes. Ignored by default because
//! timings depend on the machine; run with:
//! `cargo test -p kunotes-core --release --test perf -- --ignored --nocapture`

use std::collections::HashSet;
use std::fs;
use std::time::Instant;

use kunotes_core::VaultNode;
use kunotes_core::cursor::{char_count, line_col};
use kunotes_core::search::{filter_files, flatten_files, visible_rows};
use tempfile::tempdir;

#[test]
#[ignore]
fn ten_thousand_notes() {
    let dir = tempdir().unwrap();
    // 100 folders × 100 notes.
    for folder in 0..100 {
        let folder_path = dir.path().join(format!("Folder {folder}"));
        fs::create_dir(&folder_path).unwrap();
        for note in 0..100 {
            fs::write(folder_path.join(format!("Note {note}.md")), "").unwrap();
        }
    }

    let start = Instant::now();
    let tree = VaultNode::scan(dir.path());
    println!("scan 10k notes:            {:?}", start.elapsed());

    let expanded: HashSet<_> = tree.children.iter().map(|c| c.path.clone()).collect();
    let start = Instant::now();
    let rows = visible_rows(&tree, &expanded);
    println!(
        "visible_rows (all open):   {:?} ({} rows)",
        start.elapsed(),
        rows.len()
    );

    let start = Instant::now();
    let files = flatten_files(&tree);
    let matches = filter_files(&files, "note 42");
    println!(
        "flatten + filter:          {:?} ({} matches)",
        start.elapsed(),
        matches.len()
    );
}

#[test]
#[ignore]
fn one_megabyte_note() {
    let line = "Some **markdown** text with `code`, an emoji 👩‍💻 and 日本語.\n";
    let text = line.repeat(1_000_000 / line.len());
    println!("note size: {} bytes", text.len());

    let start = Instant::now();
    let count = char_count(&text);
    println!(
        "char_count:                {:?} ({count} chars)",
        start.elapsed()
    );

    let start = Instant::now();
    let position = line_col(&text, text.len());
    println!(
        "line_col at end:           {:?} ({position:?})",
        start.elapsed()
    );
}
