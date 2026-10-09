# Implementation Plan

This plan builds KuNotes in phases. Each phase ends with something runnable and testable on **macOS, Windows, and Linux (Fedora)**. Design details are in [ARCHITECTURE.md](../ARCHITECTURE.md). Section numbers below (§) refer to that file.

Legend: `[ ]` todo · `[~]` in progress · `[x]` done

---

## Phase 0: Scaffold and API spike ✅

**Goal:** a hello-world gpui-kit window that builds on all three OSes in CI, plus confirmed answers to every open question in §11.

- [x] `cargo new` workspace with `crates/kunotes-core` (lib) and `crates/kunotes` (bin). Edition 2024, `rust-version = "1.92"`.
- [x] Workspace `Cargo.toml`: shared `[workspace.dependencies]`, and `[profile.dev.package]` opt-level 3 for `gpui-pre`, `gpui-component`, `gpui-kit`, `gpui-kit-assets`, `gpui-pre-macros`, `gpui-pre-platform`, `rustybuzz`, `taffy`, `ttf-parser` (from the gpui-kit install guide).
- [x] `fixtures/sample-vault/`: nested folders, hidden files, non-md files, and an `Example.md` that uses every markdown element in §6.5 (headings, emphasis, inline code, code block, nested lists, task list, quote, link, rule). Used for manual testing and screenshots.
- [x] `rust-toolchain.toml` pinned to 1.99.0 (≥ 1.92), with components `rustfmt` and `clippy`.
- [x] Hello window: `application().with_assets(assets::Assets).run(|cx| { init(cx); open_window(..) })`.
- [x] CI (GitHub Actions) matrix on `macos-latest`, `windows-latest`, and Linux as a `fedora:latest` container job on `ubuntu-latest`, running `fmt --check`, `clippy -D warnings`, `test --workspace`, and `build --release`. Install the dnf deps in the Fedora container (see CONTRIBUTION.md).
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

## Phase 1: `kunotes-core` (pure logic with tests) ✅

**Goal:** all non-UI behavior, covered by unit tests. No GPUI dependency.

- [x] `node.rs`: `VaultNode { path, name, is_dir, children: Vec<VaultNode> }` (files have no children), `VaultNode::scan(root)`, `is_markdown(path)`.
  - Directories and `*.md` (case-insensitive) only. Skip dot-prefixed names. Don't follow dir symlinks. Unreadable dirs → empty.
  - Sort folders first, then `natord::compare_ignore_case`.
- [x] `fs_ops.rs`:
  - [x] `unique_path(dir, base, ext)` → `Untitled.md`, `Untitled 2.md`, `Untitled 3.md`, …
  - [x] `create_file(dir, base)` seeds `# {title}\n` (never overwrites). `create_folder(dir, base)`.
  - [x] `rename(path, new_name)`: trim, validate, append `.md` to files unless the name already ends in `.md`, no-op if unchanged, case-only renames via a temp name (refused if a different file has that exact name on a case-sensitive disk), error on collision.
  - [x] `move_into(path, folder)`: reject self/descendant moves and collisions, no-op if already a child.
  - [x] `trash(path)` via the `trash` crate.
  - [x] `atomic_write(path, contents)`: hidden temp file + fsync + rename, temp removed on failure.
- [x] `names.rs`: `validate_name(&str) -> Result<(), NameError>` (§8.2 rules, plus a leading `.` is rejected because the file would be hidden).
- [x] `paths.rs`: `relative_path` (always `/`), `note_title`, `breadcrumb`, `remap_path` (selection/expansion after rename or move), `ancestors_within` (folders to expand to reveal a file).
- [x] `format.rs`: `bold`, `italic`, `inline_code`, `wrap`, `heading` (replaces an existing marker), `quote`, `bullet_list`, `numbered_list`, `link`, `code_block`, `horizontal_rule` → `Edit { range, replacement, new_selection }` (§6.4). Byte offsets; out-of-range or mid-character selections are clamped.
- [x] `cursor.rs`: `line_col(text, offset)` (1-based, column in characters), `char_count(text)` (graphemes).
- [x] `line_ending.rs`: `LineEnding::detect` / `apply`, `normalize`, so `\r\n` files are saved back unchanged.
- [x] `search.rs`: `flatten_files`, `filter_files` (case-insensitive substring), `visible_rows(root, expanded)` → `VisibleRow`.
- [x] `settings.rs`: `Settings` + `ViewMode` with serde, `default_path()`, `load_from()` (defaults on missing or corrupt file), `save_to()` (atomic).
- [x] `error.rs`: `CoreError` (`thiserror`) + `Result<T>` alias.

**Tests:** 65 passing (45 unit tests in `src/`, 20 integration tests in `tests/fs_ops.rs`, `tests/scan.rs`, `tests/settings.rs`). One real-trash test is `#[ignore]`d for CI; run it with `cargo test -- --ignored`.
- [x] Scan: hidden files, `.git`, non-md files, nested folders, sort order (`a2` before `a10`, case-insensitive), symlink loop doesn't hang (unix only), the sample-vault fixture.
- [x] Unique names, rename with and without extension, case-only rename, collision error, invalid names leave the file untouched.
- [x] Move into self, into a descendant, into the current parent (no-op), collision.
- [x] `validate_name`: every rule in §8.2.
- [x] Format transforms: empty selection, non-empty selection, start/end of document, multi-byte text (emoji, CJK), multi-line selection, selection ending at a line start.
- [x] `line_col` with `\n` and `\r\n`. `char_count` with emoji ZWJ sequences and flags.
- [x] Settings round-trip and corrupt-file fallback.

**Done when:** `cargo test -p kunotes-core` passes on all three OSes in CI.

---

## Phase 2: App shell and vault lifecycle

**Goal:** the window opens, a vault can be opened and is remembered, and the layout matches the mockup in §5 (without the tree contents yet).

- [x] `actions.rs`: actions added as each phase needs them (now `OpenVault`, `CloseVault`, `NewFile`, `NewFolder`, `ToggleSidebar`, `Quit`), their keybindings, and `app_menus()`.
- [x] `app.rs`: init order (gpui-kit → settings → keys → menus), `cx.set_menus` (macOS) + `GlobalState::set_app_menus` for the in-window `AppMenuBar` (Windows/Linux), quit handling, settings saved on quit.
- [x] `assets.rs`: `AppAssets` = gpui-kit default icons + `icon_assets!` extras (gpui-kit only embeds ~100 icons by default; others render blank).
- [x] `settings_store.rs`: `SettingsStore` GPUI global (`get` / `update` with a 300ms debounced background save / `save_now`).
- [x] `vault_store.rs`: `VaultStore` entity with `open_vault`, `restore_last_vault`, `close_vault`, `refresh` (background scan + generation guard), `create_file`/`create_folder` in the root, and `VaultEvent`.
- [x] `ui/workspace.rs`: `h_resizable` (sidebar 250, range 200–400, persisted width/visibility) + detail area, focused on start so shortcuts work; window title = vault name; theme follows system (`observe_window_appearance`).
- [x] `ui/title_bar.rs`: `TitleBar` with sidebar toggle, centered title, `AppMenuBar` on Windows/Linux.
- [x] `ui/sidebar/mod.rs`: header icon bar with tooltips (incl. shortcut) and disabled states; "No Vault Open" + button. Delete and Search stay disabled until Phases 3 and 5.
- [x] `ui/sidebar/file_tree.rs`: read-only `uniform_list` of top-level rows (Phase 3 makes it interactive).
- [x] `ui/empty_state.rs`: both variants.
- [x] Open vault via `cx.prompt_for_paths` (directories only, prompt "Open Vault"); picker failure shows a notification.
- [x] Restore the last vault on launch. Clear the setting if the folder is gone.

**Checked on macOS 15.8:** empty states render; restoring a vault from settings shows the tree (hidden and non-md files skipped, folders first), remembered sidebar width, and vault name as the title. 🧪 Still to check by hand: the folder picker, sidebar drag/toggle, and shortcuts on each OS (they need real mouse/keyboard input).

**Done when:** on all three OSes, opening a folder, quitting, and relaunching reopens it, and the empty states and sidebar resize/toggle work.

---

## Phase 3: File tree ✅

**Goal:** a fully interactive file tree (§6.2).

- [x] `ui/sidebar/file_tree.rs`: `uniform_list` over `visible_rows(root, expanded)`, rows rendered with `ListItem`, depth indent, chevron, folder (blue, open/closed) and file icons.
- [x] Single-click select. Double-click a folder to toggle. Chevron click to toggle.
- [x] Keyboard navigation (`up`/`down`/`left`/`right`/`enter`) within the `FileTree` key context. Scroll the selection into view.
- [x] Expansion state lives in `VaultStore.expanded` and survives `refresh`. New items expand their parents.
- [x] Selection: tree selection → `selected_path`; files → `selected_file`. Rename/move remap both, plus expanded folders (`VaultStore::remap`).
- [x] Context menu (New Note/Folder on folders, Rename…, Delete, per-OS reveal via `opener::reveal`, Copy Path, Copy Relative Path).
- [x] `ui/dialogs.rs`: rename (pre-filled without `.md`, text selected, Enter confirms; invalid name keeps the dialog open + error notification) and delete confirm (`AlertDialog`, danger button, per-OS trash wording).
- [x] Header actions wired: new note/folder in the vault root, delete selection (enabled only with a selection). File menu gains Rename… and Move to Trash.
- [x] Drag and drop: rows onto folders, or onto empty space for the vault root, with a drop-target highlight. Invalid moves (into itself, name collision) show an error notification.
- [x] Error notifications for failed ops.
- [ ] Later polish: white icon on the selected row; visually refusing invalid drops while dragging.

**Tests:** 7 headless UI tests in `ui/sidebar/file_tree_tests.rs` (gpui-kit `test-support`) drive the real `Workspace` on a temp vault with real clicks, keys, and drags: click/double-click, arrow-key navigation, drag onto folder, drop on empty space, F2 rename, invalid rename, Backspace → confirm → Escape. A deliberate break of the drop handler makes the two drag tests fail, so they test real behavior.

**Done when:** every interaction in §6.2 works on all three OSes, and renaming a folder that contains the open file keeps that file open.

---

## Phase 4: Source editor, preview, autosave ✅

**Goal:** edit (Source mode), preview, and save (§6.3–§6.5, §7.1). Live mode comes in Phases 9–10; until then Source is the default.

- [x] `ui/editor/mod.rs`: `load_note` + `EditorPane`, recreated per note. Lossy UTF-8 → read-only with a warning. Line endings preserved.
- [x] Source editor: `EditorState` markdown highlighting, soft wrap, no line numbers, no folding, 15px, padding.
- [x] Autosave 500ms after typing (`save_now` on the UI thread, no races). Immediate save on note switch, window close, quit, and `secondary-s`. Deleted notes are never written back. Renamed/moved open notes keep unsaved typing (`VaultStore::last_move` + `set_path`).
- [x] Breadcrumb (vault-relative, chevrons, last part emphasized).
- [x] View-mode segmented toggle (Source/Split/Preview) + `secondary-2/3/4`, `secondary-e` to cycle, View menu items, persisted in settings.
- [x] `ui/editor/preview.rs`: `TextView` markdown, scrollable, selectable, links open in the browser, ~150ms throttle while typing. No pulldown-cmark fallback was needed.
- [x] Split: `h_resizable` 50/50.
- [x] `ui/editor/formatter_bar.rs`: 11 buttons in 5 groups with tooltips, each one undoable edit through `apply_format`. Keeps the selection and editor focus.
- [x] Editor shortcuts: `secondary-b`, `secondary-i`, `secondary-shift-k`.
- [x] `ui/editor/status_bar.rs`: `Ln, Col` + character count (cached, recomputed on change).
- [x] Window/title-bar title = note title.

**Tests:** 8 headless UI tests in `ui/editor/editor_tests.rs` cover: note loads; autosave waits for the pause; switching saves immediately; renaming the open note keeps unsaved typing and doesn't recreate the old file; a deleted note isn't written back; the Bold button; view-mode shortcuts; CRLF round-trip. Deliberately breaking save-on-switch or rename-follow makes the matching test fail. Shared setup lives in `ui/test_helpers.rs`.

**Checked on macOS 15.8 (screenshot):** Split view of `Example.md` shows highlighted source, a rendered preview (headings, emphasis, inline code, task-list checkboxes, nested lists, numbered list, blockquote), all formatter icons, and the status bar.

**Done when:** typing autosaves within ~0.5s, switching files or quitting mid-typing loses nothing (verified manually on each OS), the preview renders every element in `fixtures/sample-vault/Example.md` as specified in §6.5, and all formatter buttons behave as specified.

---

## Phase 5: Quick switcher ✅

- [x] `ui/quick_switcher.rs`: `Command` inside `open_dialog`, width 480, every `.md` file. Title on the left, uppercase parent folder on the right. Relative path is searchable as a keyword.
- [x] Up/down/enter/escape. Footer hints. "No matches" empty state.
- [x] Opening a note selects it in the tree and expands its folders (`VaultStore::open_note`), then closes the dialog.
- [x] Bound to `secondary-k` and `secondary-shift-o`, plus the sidebar search button and File → Quick Open….

**Tests:** 2 UI tests in `ui/quick_switcher_tests.rs`. Typing "plan" + Enter opens `Projects/Plan.md` (not the first note in the list, which proves filtering and index mapping) and expands `Projects`. Escape closes without opening anything.

**Done when:** `secondary-k`, typing part of a name, then Enter opens the file on all three OSes.

---

## Phase 6: Live external sync ✅

- [x] `watcher.rs`: `notify-debouncer-full` recursive watch (500ms), then a `futures` channel, a `cx.spawn` loop, and `VaultStore::refresh`. Changes that only touch hidden paths (`.git`, our `.kunotes.tmp` files) are ignored.
- [x] Event paths are matched against both the vault path and its canonical path (macOS `/var` → `/private/var`, symlinked vaults).
- [x] The watcher restarts on vault switch and is dropped on close.
- [x] Watch failures (e.g. the Linux inotify limit) are logged and shown in one notification. The app keeps working.
- [x] Refresh keeps selection and expansion. Paths that no longer exist are forgotten (`forget_missing_paths`), so a note deleted outside the app closes the editor and is never written back.

**Tests:** `watcher.rs` has a real-filesystem test (plain `#[test]`, since GPUI's deterministic scheduler forbids the watcher's OS thread): hidden files are ignored and a new note is reported. `vault_store_tests.rs` has 2 UI tests: a note created outside appears after refresh, and a note deleted outside closes the editor without being written back, even with unsaved typing. UI tests turn the OS watcher off (`VaultStore::disable_live_sync`).

**Checked on macOS 15.8:** with the app running, `mkdir` and creating a `.md` file from the shell showed up in the tree within ~3s.

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
- [ ] `ui/editor/live/buffer.rs`: `ropey` buffer, undo/redo with grouping, incremental re-parse of edited blocks.
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
