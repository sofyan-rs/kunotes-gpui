# CLAUDE.md

This file guides Claude Code (claude.ai/code) when working in this repository.

## Overview

KuNotes is a minimal, cross-platform (**macOS, Windows, Linux — Fedora is the Linux target**) markdown vault app in Rust. It opens a folder on disk as a "vault". Files on disk are the only source of truth, with no database. UI is built with [gpui-kit](https://gpui-kit.com/) (`gpui-kit = "0.7.1"`), which re-exports GPUI. Don't add `gpui` as a direct dependency.

Scope is intentionally minimal: open a folder, browse it, edit markdown. There are no wikilinks, backlinks, tags, graph, plugins, or sync. Push back on scope creep.

**Status:** docs-first. Check `docs/implementation/PLAN.md` for the current phase before writing code, and tick checkboxes there as items land.

## Docs (read before non-trivial changes)

- `docs/ARCHITECTURE.md`: stack, workspace layout, `VaultStore` state model, per-component specs (§6), save/watcher pipeline (§7), cross-platform rules (§8), feature summary (§10), open questions (§11).
- `docs/implementation/PLAN.md`: phased roadmap, per-OS feature checklist, risks.
- `docs/implementation/spike-notes.md`: Phase 0 answers about the gpui-kit API (written during Phase 0).
- `docs/CONTRIBUTION.md`: setup per OS, conventions, PR checklist.
- `fixtures/sample-vault/`: manual-test vault; `Example.md` covers every preview element.

If the design changes, update ARCHITECTURE.md in the same change.

## Code discovery (codebase-memory MCP)

The repo is indexed in codebase-memory as project `Users-sofyan-Dev-Project-Personal-kunotes-gpui`.
- Explore code with codebase-memory tools first: `search_graph` (find functions, structs, modules), `trace_path` (call chains), `get_code_snippet` (exact source), `get_architecture`, and `search_code`. Use Grep/Read for docs, configs, and before editing.
- Re-run `index_repository` after adding or moving modules, or when results look stale.
- Architecture decisions are stored as an ADR (`manage_adr`, mode `get`). Update it in the same change as ARCHITECTURE.md when a decision changes.

## Build / Test

```bash
cargo run -p kunotes                                   # app (debug)
cargo test -p kunotes-core                             # fast pure-logic tests
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings  # must be clean
```

Rust ≥ 1.92 (required by gpui-kit). Windows needs the MSVC toolchain. Fedora needs the dnf packages listed in CONTRIBUTION.md, plus a Vulkan driver.

## Layout

- `crates/kunotes-core/`: **pure logic, no gpui.** Covers scan (`node.rs`), file ops and `atomic_write` (`fs_ops.rs`), filename validation (`names.rs`), relative path and breadcrumb (`paths.rs`), formatter transforms (`format.rs`), line/col and char count (`cursor.rs`), file flatten/filter and tree `visible_rows` (`search.rs`), `Settings` (`settings.rs`), and Live-mode logic (`live.rs`: blocks, marker ranges, cursor movement over hidden ranges). Anything that can be written without gpui goes here, with tests.
- `crates/kunotes/`: the GPUI app.
  - `vault.rs`: the `VaultStore` entity, the single source of truth, which emits `VaultEvent`.
  - `watcher.rs`: `notify` events go through a channel to `refresh`.
  - `save.rs`: `SaveDebouncer`, 500ms.
  - `platform.rs`: the **only** place for `#[cfg(target_os)]` and OS wording.
  - `ui/*`: views. `ui/live_editor/` is the custom Live-mode editor (stage B).

## Editor modes

Live (realtime formatter, default once Phase 9 ships), Source (highlighted raw markdown), Split, Preview. All modes share **one markdown string buffer**. Live is not rich-text WYSIWYG: never introduce a separate document model or an HTML round-trip, and switching modes must never change file bytes. Live ships in two stages: A = styled gpui-kit editor (Phase 9), B = custom `LiveEditor` with markers hidden off-cursor (Phase 10). See ARCHITECTURE §6.3 and §6.9.

## Rules

- **Data safety:**
  - User files are written only through `kunotes_core::fs_ops::atomic_write`.
  - Deletes go only through the `trash` crate. Never `remove_file` or `remove_dir_all` vault content.
  - Flush pending saves on file switch, vault switch, window close, and app quit.
- **Main thread:** no blocking FS scans on the UI thread. Use `cx.background_spawn` and bring results back with `cx.spawn` / `entity.update`.
- **Cross-platform:**
  - Keybindings use `secondary-` (cmd on macOS, ctrl elsewhere). Never bare `cmd-`.
  - Use `Path`/`PathBuf`, never string-joined paths.
  - Don't assume a case-sensitive filesystem or `\n` endings. Preserve the line endings on disk.
  - Names must pass `names::validate_name`.
- **Text ranges** are UTF-8 byte offsets on char boundaries. Test multi-byte text (emoji, CJK).
- **GPUI patterns:**
  - Views hold `Entity` handles and `cx.subscribe`.
  - Call `cx.notify()` after state changes.
  - Store `Subscription`s and `Task`s; dropping one cancels it.
  - Scope keybindings with `key_context` (`Workspace`, `FileTree`, `EditorPane`).
  - The delete key is bound only in `FileTree`.
- **Errors:**
  - `kunotes-core` returns `CoreError` (`thiserror`).
  - The app surfaces user-facing failures with `VaultEvent::Error` → `push_notification`.
  - Don't silently `let _ =` or `.ok()` failed I/O. Don't call `unwrap()` in app code paths.
  - Use `log`, not `println!`.
- **gpui-kit is pre-1.0** and its docs are incomplete. Before using an API whose signature you haven't seen, verify it on docs.rs/gpui-kit or in the crate source (`~/.cargo/registry/src/*/gpui-kit-*`). Don't guess. Record findings in `spike-notes.md`.
- Commits use Conventional Commits (`feat(tree): …`, `fix(save): …`). Only commit when asked.
