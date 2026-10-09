# Implementation Plan

This plan builds KuNotes in phases. Each phase ends with something runnable and testable on **macOS, Windows, and Linux (Fedora)**. Design details are in [ARCHITECTURE.md](../ARCHITECTURE.md). Section numbers below (§) refer to that file.

Legend: `[ ]` todo · `[~]` in progress · `[x]` done

---

## Phase 0: Scaffold and API spike

**Goal:** a hello-world gpui-kit window that builds on all three OSes in CI, plus confirmed answers to every open question in §11.

- [x] `cargo new` workspace with `crates/kunotes-core` (lib) and `crates/kunotes` (bin). Edition 2024, `rust-version = "1.92"`.
- [x] Workspace `Cargo.toml`: shared `[workspace.dependencies]`, and `[profile.dev.package]` opt-level 3 for `gpui-pre`, `gpui-component`, `gpui-kit`, `gpui-kit-assets`, `gpui-pre-macros`, `gpui-pre-platform`, `rustybuzz`, `taffy`, `ttf-parser` (from the gpui-kit install guide).
- [x] `fixtures/sample-vault/`: nested folders, hidden files, non-md files, and an `Example.md` that uses every markdown element in §6.5 (headings, emphasis, inline code, code block, nested lists, task list, quote, link, rule). Used for manual testing and screenshots.
- [x] `rust-toolchain.toml` pinned to 1.99.0 (≥ 1.92), with components `rustfmt` and `clippy`.
- [x] Hello window: `application().with_assets(assets::Assets).run(|cx| { init(cx); open_window(..) })`.
- [~] CI (GitHub Actions) matrix on `macos-latest`, `windows-latest`, and Linux as a `fedora:latest` container job on `ubuntu-latest`, running `fmt --check`, `clippy -D warnings`, `test --workspace`, and `build --release`. Install the dnf deps in the Fedora container (see CONTRIBUTION.md).
- [x] Answer §11 from crate source (runtime spikes only where needed):
  - [x] Editor: soft wrap, cursor offset, selection get/set, undoable range replace, markdown highlighting.
  - [x] TextView: task lists, styling hooks.
  - [x] Live stage A: per-capture highlight styles (weight, italic, background, size), proportional font (§11.10).
  - [~] Live stage B: API confirmed (`EntityInputHandler`, UTF-16 ranges). IME runtime check on Windows/Fedora moves to the start of Phase 10.
  - [x] `secondary-` keybinding.
  - [x] Double-click via `ClickEvent` on `ListItem`.
  - [~] `on_drag`/`on_drop` inside `uniform_list`: API confirmed; runtime check during Phase 3.
  - [x] ToggleGroup re-click behavior.
  - [x] App menu on Windows/Linux.
  - [x] System theme sync.
  - [~] `prompt_for_paths(directories)`: API confirmed; runtime check per OS during Phase 2.
- [x] Record findings in [`spike-notes.md`](./spike-notes.md) and update ARCHITECTURE §6 and §11 wherever a decision changes.

**Done when:** CI is green on all three OSes, the window shows a gpui-kit `Button` on each, and every §11 item has a written answer.

---

## Phase 1: `kunotes-core` (pure logic with tests)

**Goal:** all non-UI behavior, covered by unit tests. No GPUI dependency.

- [ ] `node.rs`: `VaultNode { path, name, is_dir, children: Option<Vec<VaultNode>> }` and `scan(root)`.
  - Directories and `*.md` (case-insensitive) only. Skip dot-prefixed names. Don't follow dir symlinks. Unreadable dirs → empty.
  - Sort folders first, then `natord::compare_ignore_case`.
- [ ] `fs_ops.rs`:
  - [ ] `unique_path(dir, base, ext)` → `Untitled.md`, `Untitled 2.md`, `Untitled 3.md`, …
  - [ ] `create_file(dir, base)` seeds `# {stem}\n`. `create_folder(dir, base)`.
  - [ ] `rename(path, new_name)`: validate, append `.md` to files typed without an extension, no-op if unchanged, handle case-only renames via a temp name, error on collision.
  - [ ] `move_into(path, folder)`: reject self/descendant moves and collisions, no-op if already a child.
  - [ ] `trash(path)` via the `trash` crate.
  - [ ] `atomic_write(path, contents)`: temp file + fsync + rename.
  - [ ] `remap_path(p, old, new)` helper for selection/expansion remapping.
- [ ] `names.rs`: `validate_name(&str) -> Result<(), NameError>` (§8.2 rules).
- [ ] `paths.rs`: `relative_path(root, path)`, `breadcrumb(root, file) -> Vec<String>` (vault name omitted, `.md` stripped, falls back to the file stem).
- [ ] `format.rs`: `wrap`, `line_prefix`, `link`, `code_block`, `horizontal_rule` → `Edit { range, replacement, new_selection }` (§6.4). Byte offsets.
- [ ] `cursor.rs`: `line_col(text, offset) -> (usize, usize)` (1-based), `char_count(text)` (graphemes).
- [ ] `search.rs`: `flatten_files(&VaultNode)`, `filter(files, query)` (case-insensitive substring), `visible_rows(root, expanded)` for the tree.
- [ ] `settings.rs`: `Settings` with serde, `load()` (default on missing or corrupt file), `save()` (atomic) at `dirs::config_dir()/kunotes/settings.json`.
- [ ] `error.rs`: `CoreError` (`thiserror`).

**Tests** (filesystem tests use `tempfile` dirs):
- [ ] Scan: hidden files, `.git`, non-md files, nested folders, sort order (`a2` before `a10`, case-insensitive), symlink loop doesn't hang (unix only, `#[cfg(unix)]`).
- [ ] Unique names, rename with and without extension, case-only rename, collision error.
- [ ] Move into self, into a descendant, into the current parent (no-op), collision.
- [ ] `validate_name`: every rule in §8.2.
- [ ] Format transforms: empty selection, non-empty selection, start/middle/end of document, multi-byte text (emoji, CJK), multi-line selection for `line_prefix`.
- [ ] `line_col` with `\n` and `\r\n`. `char_count` with emoji ZWJ sequences.
- [ ] Settings round-trip and corrupt-file fallback.

**Done when:** `cargo test -p kunotes-core` passes on all three OSes in CI.

---

## Phase 2: App shell and vault lifecycle

**Goal:** the window opens, a vault can be opened and is remembered, and the layout matches the mockup in §5 (without the tree contents yet).

- [ ] `actions.rs`: `actions!(kunotes, [OpenVault, NewFile, NewFolder, DeleteSelection, RenameSelection, QuickSwitcher, ToggleSidebar, ViewLive, ViewSource, ViewSplit, ViewPreview, CycleViewMode, SaveNow, Quit])`.
- [ ] `app.rs`: keybindings (§8.1), `cx.set_menus` (macOS), quit handling, theme follows system.
- [ ] `vault.rs`: `VaultStore` entity with `open_vault`, `restore_last_vault`, `close_vault`, `refresh` (background scan + generation guard), and events.
- [ ] `ui/workspace.rs`: `TitleBar` (sidebar toggle, centered title) + `h_resizable` (sidebar 250, range 200–400, persisted width/visibility) + detail area.
- [ ] `ui/sidebar.rs`: header icon bar with tooltips and disabled states; "No Vault Open" + button.
- [ ] `ui/empty_state.rs`: both variants.
- [ ] Open vault via `cx.prompt_for_paths` (directories only, prompt "Open Vault").
- [ ] Restore the last vault on launch. Clear the setting if the folder is gone.

**Done when:** on all three OSes, opening a folder, quitting, and relaunching reopens it, and the empty states and sidebar resize/toggle work.

---

## Phase 3: File tree

**Goal:** a fully interactive file tree (§6.2).

- [ ] `ui/file_tree.rs`: `uniform_list` over `visible_rows(root, expanded)`, rows rendered with `ListItem`, depth indent, chevron, and folder/file icons (selected row → white icon).
- [ ] Single-click select. Double-click a folder to toggle. Chevron click to toggle.
- [ ] Keyboard navigation (`up`/`down`/`left`/`right`/`enter`) within the `FileTree` key context. Scroll the selection into view.
- [ ] Expansion state lives in `VaultStore.expanded` and survives `refresh`. A newly created file's parent auto-expands.
- [ ] Selection sync: tree selection → `selected_path`; files → `selected_file`. External selection changes (quick switcher, create) are reflected in the tree.
- [ ] Context menu (folder items, rename, delete, reveal with the per-OS label, copy path, copy relative path).
- [ ] `ui/dialogs.rs`: rename dialog (pre-filled, validated, inline error) and delete confirm (`AlertDialog`, per-OS trash wording).
- [ ] Header actions are wired: new file/folder in the vault root, delete selection.
- [ ] Drag and drop: drag rows onto folders or empty space (vault root), with a drop-target highlight. Invalid drops are rejected visually.
- [ ] Error notifications for failed ops.

**Done when:** every interaction in §6.2 works on all three OSes, and renaming a folder that contains the open file keeps that file open.

---

## Phase 4: Source editor, preview, autosave

**Goal:** edit (Source mode), preview, and save (§6.3–§6.5, §7.1). Live mode comes in Phases 9–10; until then Source is the default.

- [ ] `ui/editor_pane.rs`: recreated per file, loads content (lossy UTF-8 fallback with a warning), preserves line endings.
- [ ] Source editor: monospace 15px, soft wrap, padding, no line numbers, markdown highlighting.
- [ ] `save.rs`: `SaveDebouncer` (500ms). Flush on file switch, vault switch/close, window close, app quit, and `secondary-s`. Uses `atomic_write` in the background.
- [ ] Breadcrumb (relative path, chevrons, last segment emphasized).
- [ ] View-mode segmented toggle (Source/Split/Preview for now; Live segment hidden until Phase 9) + `secondary-2/3/4`, `secondary-e` to cycle, persisted in settings.
- [ ] `ui/preview.rs`: `TextView` markdown, scrollable, selectable, links open in the browser, ~150ms throttle in Split mode. Implement the pulldown-cmark fallback **only if** the Phase 0 spike found TextView insufficient.
- [ ] Split: `h_resizable` 50/50.
- [ ] `ui/formatter_bar.rs`: 11 buttons in 5 groups with tooltips, wired to `kunotes_core::format` through one undoable replace each. Restores the selection/cursor and editor focus.
- [ ] Editor shortcuts: `secondary-b`, `secondary-i`, `secondary-shift-k`.
- [ ] `ui/status_bar.rs`: `Ln, Col` + character count, updated on cursor move and edits.
- [ ] Window/title-bar title = file stem.

**Done when:** typing autosaves within ~0.5s, switching files or quitting mid-typing loses nothing (verified manually on each OS), the preview renders every element in `fixtures/sample-vault/Example.md` as specified in §6.5, and all formatter buttons behave as specified.

---

## Phase 5: Quick switcher

- [ ] `ui/quick_switcher.rs`: `Command` inside `open_dialog`, width 480, all `.md` files, file stem label + uppercase parent-folder hint, relative path as a keyword.
- [ ] Up/down/enter/escape. Footer hints. "No matches" empty state.
- [ ] Opening a file selects it in the tree (expanding its ancestors) and closes the dialog.
- [ ] Bound to `secondary-k` and `secondary-shift-o`, plus the sidebar search button.

**Done when:** `secondary-k`, typing part of a name, then Enter opens the file on all three OSes.

---

## Phase 6: Live external sync

- [ ] `watcher.rs`: `notify-debouncer-full` recursive watch (500ms) → async channel → `cx.spawn` loop → `VaultStore::refresh`.
- [ ] The watcher restarts on vault switch and is dropped on close.
- [ ] Watch failures (e.g. the Linux inotify limit) are logged and shown in one notification. The app keeps working.
- [ ] Refresh keeps selection and expansion. If the open file was deleted externally, clear the editor and show the empty state.

**Done when:** creating, renaming, or deleting files from Finder, Explorer, a file manager, or a terminal appears in the tree within ~1s on every OS.

---

## Phase 7: Polish and feature audit

- [ ] Walk the full feature checklist below on each OS.
- [ ] Menus: macOS native menu bar (App, File, Edit, View, Window). Windows/Linux title-bar menu with the same commands and shortcut hints.
- [ ] Light and dark themes look right. Accent color is consistent.
- [ ] Large-vault check (~10k files): launch, scan, quick-switcher filter, and tree scroll all stay smooth.
- [ ] Large-note check (~1 MB markdown): typing latency is OK and preview throttling works.
- [ ] HiDPI / fractional scaling check on Windows and Linux.
- [ ] `env_logger` default level `warn` and `RUST_LOG` respected.

---

## Phase 8: Packaging and release

- [ ] App icon: design it and export PNG/ICNS/ICO into `packaging/`.
- [ ] macOS: `.app` bundle (`cargo-bundle` or a script), bundle ID `id.sofyan.KuNotes`, `.dmg`. Codesign + notarize (optional at first).
- [ ] Windows: embed the icon + manifest (`winresource`), produce a `.msi` (`cargo-wix`) or zip.
- [ ] Linux (Fedora): `.desktop` file + icon, `.rpm` (`cargo-generate-rpm`). Flatpak optional later.
- [ ] GitHub Actions release workflow on tag `v*` that uploads artifacts for all three OSes.
- [ ] Root `README.md` with screenshots from each OS.

---

## Phase 9: Live mode stage A, styled source (v1.5)

**Goal:** a formatted writing view on the same buffer (§6.9 stage A). Becomes the default mode.

- [ ] Live segment in the view-mode toggle + `secondary-1`. Default `view_mode` becomes Live for new settings files.
- [ ] Second editor configuration: proportional UI font, custom markdown highlight theme (headings, bold, italic, inline code, links, dimmed markers).
- [ ] Cursor/selection carries over when switching Live ⇄ Source.
- [ ] Formatter bar and editor shortcuts work in Live.
- [ ] Light and dark theme variants of the highlight styles.

**Done when:** writing in Live on `Example.md` shows emphasis and headings styled, switching modes never changes the file bytes, and this holds on all three OSes.

---

## Phase 10: Live mode stage B, custom live editor (v2)

**Goal:** Obsidian-style live preview (§6.9 stage B), replacing stage A as the Live implementation.

- [ ] `kunotes-core::live`: block parsing with byte ranges (pulldown-cmark offset iter), marker ranges per block/inline span, "revealed" ranges for a cursor/selection, cursor movement over hidden ranges, list continuation. Unit tests for each.
- [ ] `ui/live_editor/buffer.rs`: `ropey` buffer, undo/redo with grouping, incremental re-parse of edited blocks.
- [ ] `layout.rs`: per-block shaped lines with styled runs, heading sizes, soft wrap, visible-blocks-only layout.
- [ ] `element.rs`: paint text, cursor, selection, checkbox, hr, code-block background, quote bar.
- [ ] `input.rs`: `EntityInputHandler` (typing, IME marked text), mouse (click, drag, double/triple click, shift-click).
- [ ] `actions.rs`: movement (char, word, line, visual up/down, home/end, page), delete, clipboard, select all, Enter list continuation, Tab/Shift-Tab list indent.
- [ ] Checkbox click toggles `[ ]` ⇄ `[x]` as one undoable edit. Cmd/Ctrl+click opens links.
- [ ] Formatter bar + shortcuts reuse `kunotes_core::format`.
- [ ] Performance: typing stays smooth on a ~1 MB note.
- [ ] Setting to fall back to stage A.
- [ ] Manual IME test: Japanese/Chinese input on macOS, Windows (MS IME), and Fedora (ibus + fcitx5, Wayland).

**Done when:** the Live checklist rows pass on all three OSes and a full writing session in Live on `Example.md` produces a byte-identical file to doing the same edits in Source.

---

## Feature checklist

Check off per OS during Phase 7 (Live rows after Phases 9 and 10).

| Feature | macOS | Windows | Fedora |
|---|:-:|:-:|:-:|
| Open folder as vault via native picker | [ ] | [ ] | [ ] |
| Reopen last vault on launch | [ ] | [ ] | [ ] |
| Tree: folders + `.md` only, hidden files skipped, folders first, natural sort | [ ] | [ ] | [ ] |
| Tree: single click selects, double-click / chevron toggles folder | [ ] | [ ] | [ ] |
| Tree: drag-and-drop move into folder / vault root | [ ] | [ ] | [ ] |
| Context menu: New File/Folder, Rename, Delete, Reveal, Copy Path, Copy Relative Path | [ ] | [ ] | [ ] |
| New file (`Untitled.md` with `# Untitled` seed, unique suffix) | [ ] | [ ] | [ ] |
| New folder (`New Folder`, unique suffix) | [ ] | [ ] | [ ] |
| Rename (auto `.md`), with validation | [ ] | [ ] | [ ] |
| Delete with confirmation → OS trash | [ ] | [ ] | [ ] |
| Source / Split / Preview modes | [ ] | [ ] | [ ] |
| Live mode stage A (styled source) | [ ] | [ ] | [ ] |
| Live mode stage B (markers hidden off-cursor, checkboxes, IME) | [ ] | [ ] | [ ] |
| Formatter bar (B, I, H1, H2, link, inline code, code block, quote, bullet, numbered, hr) | [ ] | [ ] | [ ] |
| Preview: headings, emphasis, inline code, code blocks, lists (nested), task lists, quotes, links, hr | [ ] | [ ] | [ ] |
| Preview text selectable/copyable | [ ] | [ ] | [ ] |
| Debounced autosave + flush on switch/quit | [ ] | [ ] | [ ] |
| Breadcrumb (vault-relative) | [ ] | [ ] | [ ] |
| Status bar: Ln/Col + character count | [ ] | [ ] | [ ] |
| Quick switcher with keyboard nav | [ ] | [ ] | [ ] |
| Live external sync | [ ] | [ ] | [ ] |
| Sidebar resizable + toggleable | [ ] | [ ] | [ ] |

---

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| gpui-kit is pre-1.0 and pins a GPUI snapshot (`gpui-pre`), so APIs change between minors | Breakage on upgrade | Pin the exact version in `Cargo.lock`. Upgrade deliberately in its own PR. Keep UI glue thin and logic in `kunotes-core`. |
| Editor component lacks a selection/replace API | Formatter bar blocked | Found in the Phase 0 spike. Fallback: `set_value` + cursor restore (loses undo granularity), or contribute upstream. |
| TextView doesn't render task lists | Preview feature gap | pulldown-cmark custom renderer (§6.5). |
| Linux portal/Vulkan variability | Picker or window fails on some setups | Target Fedora Workstation. Document requirements. Show a clear error when the picker fails. Test on Fedora Workstation (GNOME, Wayland) and the Fedora KDE spin. |
| Live stage B editor is a large custom component (cursor, selection, IME, undo, wrap) | Delays v2; subtle editing bugs | Ship stage A first. Keep pure logic in `kunotes-core::live` with heavy tests. Spike IME in Phase 0. Fall back to stage A behind a setting if a platform lags. |
| Watcher noise from our own saves | Extra rescans | Rescans run in the background and are debounced. Optimize only if profiling shows a problem. |
| Windows path quirks (case-insensitivity, reserved names, long paths) | Rename/move bugs | `names.rs` validation, case-only rename handling, CI tests on Windows. |
