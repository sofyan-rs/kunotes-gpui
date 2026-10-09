# Architecture

KuNotes is a minimal, cross-platform (macOS, Windows, Linux) markdown vault app. The Linux target is **Fedora** (Workstation, GNOME/Wayland first; KDE spin second). It uses Rust, [GPUI](https://www.gpui.rs/), and [GPUI Kit](https://gpui-kit.com/) (`gpui-kit` crate, which bundles GPUI, GPUI Base, the component library, and default icons).

Scope: **open a folder, browse it, edit markdown.** There are no wikilinks, backlinks, graph view, tags, or plugins. Files on disk are the only source of truth, so there is no database, sync service, or lock-in.

---

## 1. Goals and non-goals

### Goals
- Ship the feature set in [§10 Feature summary](#10-feature-summary).
- One codebase that builds and behaves the same on macOS 15+, Windows 10+, and Fedora Linux (Wayland/X11 with Vulkan).
- Never lose user data: debounced autosave, flush on file switch or quit, atomic writes, delete moves to the OS trash.
- Stay responsive on large vaults: no blocking filesystem I/O on the UI thread.
- Keep logic testable: pure logic lives in a crate with no GPUI dependency and has unit tests.

### Non-goals (for now)
- Wikilinks, backlinks, tags, graph, plugins, sync.
- Multiple windows or multiple vaults open at once.
- Tabs or multiple open files.
- Reloading the open editor's content when the file changes on disk (see [§7.3](#73-external-changes-to-the-open-file)).

---

## 2. Tech stack

| Concern | Choice | Notes |
|---|---|---|
| UI framework | `gpui-kit = "0.7.1"` | Re-exports GPUI. Do not add `gpui` separately. |
| Toolchain | Rust ≥ 1.92, edition 2024 | 1.92 is required by gpui-kit's locked dependency graph. Windows needs the MSVC toolchain. |
| Markdown preview | `gpui_kit::component::text::TextView::markdown` | Renders task lists; style hooks via `TextViewStyle` (§6.5). |
| Editor (Source mode, Live stage A) | gpui-kit `EditorState` (`.language("markdown")`, soft wrap, no line numbers) | Live stage A uses the unstyled `gpui_base::input::Editor` with its own highlight styles (§6.9). |
| Editor (Live stage B) | Custom `LiveEditor` element on GPUI primitives (`EntityInputHandler`, `StyledText`/text layout) + `ropey` buffer + `pulldown-cmark` offsets | See §6.9. |
| File watching | `notify` + `notify-debouncer-full` | FSEvents on macOS, ReadDirectoryChangesW on Windows, inotify on Linux. |
| Trash | `trash` crate | macOS Trash, Windows Recycle Bin, freedesktop trash on Linux. |
| Reveal in file manager | `opener` crate (`reveal` feature) | Finder, Explorer, or the default Linux file manager. |
| Config dir | `dirs` crate | `dirs::config_dir()/kunotes/` |
| Settings serialization | `serde` + `serde_json` | |
| Natural sort | `natord` | Case-insensitive, `a2` sorts before `a10`. |
| Character count | `unicode-segmentation` | Counts grapheme clusters, so an emoji counts as one character. |
| Errors and logging | `thiserror` (core), `anyhow` (app), `log` + `env_logger` | |
| Folder picker | GPUI `cx.prompt_for_paths(PathPromptOptions { directories: true, .. })` | Native dialog on each OS; uses the XDG portal on Linux. |
| Clipboard | GPUI `cx.write_to_clipboard(ClipboardItem::new_string(..))` | |

Exact versions are pinned in `Cargo.lock` once Phase 0 is done.

---

## 3. Project structure

The layout follows common Rust practice but stays small and flat, so someone new to Rust can find things quickly.

```
kunotes-gpui/
├── Cargo.toml                 # workspace: member crates, shared dependency versions
├── Cargo.lock                 # exact dependency versions (committed; this is an app)
├── rust-toolchain.toml        # pinned Rust version
├── crates/
│   ├── kunotes-core/          # LIBRARY: pure logic, no GPUI, fully unit-tested
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs         # lists the modules; start reading here
│   │   │   ├── error.rs       # CoreError
│   │   │   ├── node.rs        # VaultNode + scan a folder into a tree
│   │   │   ├── search.rs      # flatten files, quick-switcher filter, tree visible rows
│   │   │   ├── fs_ops.rs      # create / rename / move / trash, atomic_write, unique names
│   │   │   ├── names.rs       # filename validation (cross-platform rules)
│   │   │   ├── paths.rs       # relative path, breadcrumb, note title, remap after rename
│   │   │   ├── format.rs      # formatter-bar transforms (bold, link, heading, ...)
│   │   │   ├── cursor.rs      # byte offset -> (line, col), character count
│   │   │   ├── line_ending.rs # keep \n or \r\n unchanged across load/save
│   │   │   ├── settings.rs    # Settings + load/save JSON
│   │   │   └── live.rs        # Live-mode logic (Phase 10; may become live/ folder)
│   │   └── tests/             # integration tests that touch the real filesystem
│   │       ├── fs_ops.rs
│   │       ├── scan.rs
│   │       └── settings.rs
│   └── kunotes/               # BINARY: the GPUI desktop app
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs        # entry point only: logging + start the app
│           ├── app.rs         # setup: theme, menus, open the main window
│           ├── actions.rs     # every action + its keybinding, in one place
│           ├── platform.rs    # the ONLY place for OS-specific code and wording
│           ├── vault_store.rs # VaultStore entity: shared app state + events
│           ├── watcher.rs     # file watcher -> VaultStore::refresh
│           ├── autosave.rs    # SaveDebouncer
│           └── ui/            # all views, grouped by feature
│               ├── mod.rs
│               ├── workspace.rs       # root view: title bar + sidebar + editor
│               ├── title_bar.rs
│               ├── sidebar/
│               │   ├── mod.rs         # Sidebar view: header buttons + tree
│               │   └── file_tree.rs   # FileTree (uniform_list rows, DnD, context menu)
│               ├── editor/
│               │   ├── mod.rs         # EditorPane: loads a file, switches view modes
│               │   ├── formatter_bar.rs
│               │   ├── preview.rs
│               │   ├── status_bar.rs
│               │   └── live/          # custom Live editor (Phase 10)
│               ├── quick_switcher.rs
│               ├── dialogs.rs         # rename prompt, delete confirm
│               └── empty_state.rs
├── assets/                    # app icon and images used at runtime
├── fixtures/sample-vault/     # sample notes for manual testing
├── packaging/                 # per-OS bundling files (Phase 8)
├── docs/
└── .github/workflows/         # CI
```

### Why two crates?

- **`kunotes-core`** has no GPUI dependency. Its tests build in seconds and run headless on every OS. Anything that can be written without GPUI belongs here: file operations, parsing, text transforms.
- **`kunotes`** is the app. It only does layout, events, and wiring. It calls into `kunotes-core` for the real work.

This split is the main "best practice" in the layout. Keeping business logic out of the UI keeps both sides simple.

### Rules for adding code

1. **Start with one file.** Make a folder only when a module needs more than one file (as with `ui/sidebar/` and `ui/editor/`). Don't create empty folders "for later".
2. **Folder modules use `mod.rs`** (`ui/editor/mod.rs`), so everything for a feature lives inside its folder.
3. **Group UI by feature, not by type.** All editor views go in `ui/editor/`. There are no `components/` or `widgets/` folders.
4. **One main type per file, named after the file:** `file_tree.rs` → `FileTree`, `vault_store.rs` → `VaultStore`.
5. **At most two folder levels under `src/`** (`ui/editor/live/` is the deepest).
6. **Every file starts with a `//!` comment** that says what it's for, in one or two lines.
7. **Keep files under ~400 lines.** Past that, split by responsibility.
8. **Tests:**
   - Unit tests go at the bottom of the same file in `#[cfg(test)] mod tests`.
   - Tests that create real files go in `crates/kunotes-core/tests/`.
9. **Visibility:** keep items private by default. Use `pub` only for what another module needs. Use `pub(crate)` for helpers shared inside a crate.

### Where does my code go?

| I'm writing… | Put it in |
|---|---|
| Logic that doesn't need GPUI (parsing, paths, file ops, text math) | `kunotes-core/src/<topic>.rs` + tests |
| A new shortcut or menu command | `kunotes/src/actions.rs` (define and bind), handler in the view that owns it |
| State shared by several views | `vault_store.rs` (or a new `<name>_store.rs` entity if it's unrelated to the vault) |
| A new view for an existing feature | that feature's folder in `ui/` |
| A new feature with a single view | `ui/<feature>.rs` |
| Code that differs per OS | `platform.rs` |

## 4. Runtime model

```
                     ┌────────────────────────────────────────┐
                     │        Workspace (root view)            │
                     │  owns Entity<VaultStore>, view mode,    │
                     │  sidebar visibility, Settings           │
                     └──────┬───────────────────┬──────────────┘
                 subscribe  │                   │ subscribe
              ┌─────────────▼─────┐     ┌───────▼────────────────┐
              │  Sidebar          │     │  EditorPane            │
              │  └ FileTree       │     │  (re-created per file) │
              └─────────┬─────────┘     └───────┬────────────────┘
                        │ update()              │ read file / SaveDebouncer
                        ▼                       ▼
              ┌──────────────────────────────────────────────┐
              │  VaultStore (Entity)                          │
              │  vault_root, root_node, selected_file,        │
              │  selected_path, expanded: HashSet<PathBuf>    │
              │  emits VaultEvent                              │
              └──────────▲───────────────────────┬───────────┘
                         │ refresh()             │ fs ops → kunotes-core
              ┌──────────┴──────────┐            ▼
              │  Watcher (notify)   │      disk (source of truth)
              │  background thread  │
              └─────────────────────┘
```

### 4.1 `VaultStore` (Entity)

This is the single source of truth for the vault tree and selection.

```rust
pub struct VaultStore {
    vault_root: Option<PathBuf>,
    root_node: Option<VaultNode>,
    selected_file: Option<PathBuf>,   // file shown in the editor
    selected_path: Option<PathBuf>,   // tree selection (file OR folder), used by delete
    expanded: HashSet<PathBuf>,       // tree expansion; survives refreshes
    watcher: Option<VaultWatcher>,
    scan_generation: u64,             // discards stale background scans
}

pub enum VaultEvent {
    TreeChanged,
    SelectedFileChanged(Option<PathBuf>),
    SelectionChanged(Option<PathBuf>),
    Error(String),                    // surfaced as a notification
}
impl EventEmitter<VaultEvent> for VaultStore {}
```

Methods (each wraps a `kunotes-core` function, then calls `refresh` and updates selection):

| Method | Behavior |
|---|---|
| `open_vault(path)` | Stop the old watcher, set the root, save settings, `refresh`, start the watcher. |
| `restore_last_vault()` | Read `Settings.last_vault`. If the directory still exists, call `open_vault`; otherwise clear it. |
| `close_vault()` | Drop the watcher, clear state, forget the saved vault. |
| `refresh()` | Rescan on `cx.background_spawn`, apply the result on the main thread if `scan_generation` still matches, emit `TreeChanged`. Prune `expanded` entries that no longer exist. |
| `create_file(parent)` | Pick a unique `Untitled.md`, seed it with `# Untitled\n`, select it, expand the parent. |
| `create_folder(parent)` | Pick a unique `New Folder`. |
| `rename(path, new_name)` | Validate the name (§8.2). Append `.md` unless the name already ends in `.md`. Remap paths (below). |
| `move_into(path, folder)` | Reject moving into itself or a descendant, and reject collisions. Remap paths (below). |
| `trash(path)` | `trash::delete`. Clear the selection if it pointed at the path or something under it. |

**Remapping paths on rename and move:** any `selected_file`, `selected_path`, or `expanded` entry equal to or under the old path is rewritten to the new path. Renaming a folder that contains the open file keeps that file open.

### 4.2 Threading rules

- **Main thread:** GPUI rendering and entity updates only.
- **Background (`cx.background_spawn`):** directory scans, file reads larger than a trivial size, and writes.
- **Watcher thread:** owned by `notify`. It only sends a unit signal over an async channel. A `cx.spawn` loop on the GPUI side receives it and calls `VaultStore::refresh`.
- Small single-file reads and writes on file switch may stay synchronous if profiling shows they're fine, but scans must never run on the main thread.

### 4.3 Settings persistence

The app isn't sandboxed, so the last vault is stored as a plain path.

```rust
// dirs::config_dir()/kunotes/settings.json
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub last_vault: Option<PathBuf>,
    pub view_mode: ViewMode,          // Live | Source | Split | Preview
    pub sidebar_width: Option<f32>,
    pub sidebar_visible: bool,
}
```

Settings are written atomically and saved on change (debounced). A missing or corrupt file falls back to `Default`.

---

## 5. Window and layout

```
┌─ TitleBar ───────────────────────────────────────────────────────────┐
│ ● ● ●  [▤ sidebar toggle]            Example                           │
├─ Sidebar (resizable 200–400, default 250) ─┬─ EditorPane ─────────────┤
│ [📁][✎][📁+][🗑]                       [🔍] │ TEST › Example  [Live|Source|Split|Preview]
│ ▾ 📁 TEST                                   │ [B][I] | H1 H2 | 🔗 <> {} ❝ | • 1. | ─
│     📄 Example.md   (selected)              │ ┌─ editor ──────┬─ preview ─────┐
│ ▸ 📁 Account                                │ │ # Example      │ Example       │
│ ▸ 📁 AI                                     │ │ ...            │ ...           │
│   ...                                       │ └───────────────┴───────────────┘
│                                             │ Ln 5, Col 13          539 characters
└─────────────────────────────────────────────┴──────────────────────────┘
```

- Window options: `TitleBar::window_options()` with `app_owns_titlebar_drag: true` on macOS. On Windows and Linux, `TitleBar` draws its own window controls.
- Root: `gpui_kit::open_window(..)` wraps the workspace in `Root`, which hosts dialogs, sheets, and notifications.
- Layout: `h_resizable("workspace")` → `resizable_panel().size(250).size_range(200..400).visible(sidebar_visible)` plus the detail element. Subscribe to `ResizablePanelEvent::Resized` to persist `sidebar_width`.
- Theme: follows the system light/dark appearance. Accent color is blue.

---

## 6. Components

### 6.1 Sidebar (`ui/sidebar/mod.rs`)

The header icon bar, left to right:

| Icon | Action | Shortcut | Disabled when |
|---|---|---|---|
| folder | Open vault… | `secondary-o` | – |
| square-pen | New file in vault root | `secondary-n` | no vault |
| folder-plus | New folder in vault root | `secondary-shift-n` | no vault |
| trash | Delete selection (confirm) | `backspace` / `delete` / `secondary-backspace` *(FileTree context only)* | nothing selected |
| *(spacer)* | | | |
| search | Quick switcher | `secondary-k`, `secondary-shift-o` | no vault |

The body shows `FileTree` when a vault is open. Otherwise it shows "No Vault Open" and an "Open Vault…" button.

The delete keybinding is scoped to the `FileTree` key context, so it can never fire while typing in the editor.

### 6.2 File tree (`ui/sidebar/file_tree.rs`): custom, not `Tree`

The gpui-kit `Tree` component's docs don't cover double-click, context menus, or drag-and-drop, and they don't expose expansion changes. We need expansion to survive rescans triggered by the watcher.

**Decision:** build `FileTree` on GPUI `uniform_list` over a **flattened list of visible rows**, styled with gpui-kit `ListItem` and icons. We own the model, so every interaction is explicit:

```rust
struct VisibleRow { path: PathBuf, name: SharedString, depth: usize, is_dir: bool, is_expanded: bool }

fn visible_rows(root: &VaultNode, expanded: &HashSet<PathBuf>) -> Vec<VisibleRow>  // in kunotes-core, tested
```

| Interaction | Implementation |
|---|---|
| Single click | Select. If it's a file, set `selected_file` too. |
| Double-click a folder | Toggle expansion (check `ClickEvent` click count == 2). |
| Click the chevron | Toggle expansion. |
| Keyboard | `up`/`down` move the selection; `right` expands; `left` collapses or jumps to the parent; `enter` toggles a folder or opens a file. |
| Context menu | `ContextMenuExt::context_menu` on each row. Folders: New File, New Folder, separator. All rows: Rename…, Delete, separator, Reveal in Finder / Show in Explorer / Open Containing Folder, Copy Path, Copy Relative Path. |
| Drag and drop | Row `.on_drag(DraggedEntry { path }, preview)`. Folder rows and empty space below the list get `.drag_over::<DraggedEntry>(highlight)` and `.on_drop::<DraggedEntry>(→ VaultStore::move_into)`. Dropping on a file row is ignored. Dropping on empty space moves the item to the vault root. |
| Icons | Folder (blue) and file-text (muted). The icon turns white on the selected row. |
| Sort | Folders first, then natural case-insensitive order (`natord`). |

If Phase 0 shows that `Tree` can cover all of this cleanly, swapping it in is a contained change, because `visible_rows` and the `expanded` set stay the same.

### 6.3 Editor pane (`ui/editor/mod.rs`)

A new `EditorPane` entity is created whenever `selected_file` changes, so per-file state never leaks between files.

- **Load:** read the file as UTF-8 (lossy fallback with a warning notification), then `set_value`. Keep the original line ending style (`\n` vs `\r\n`) and write it back unchanged.
- **Header row:** breadcrumb on the left (path relative to the vault root, vault name omitted, `.md` stripped, last segment highlighted, chevron separators). Live/Source/Split/Preview `ToggleGroup::segmented()` on the right. ToggleGroup is multi-state, so the view keeps a single `ViewMode` and sets `checked(mode == X)` on each toggle.
- **Formatter bar:** shown in Live, Source, and Split modes (§6.4).
- **Body:** the four view modes:

| Mode | What you see | Editable | Built with |
|---|---|---|---|
| **Live** (default) | Formatted as you type: headings large, bold bold, markdown markers hidden except around the cursor, clickable checkboxes (§6.9) | yes | `LiveEditor` |
| **Source** | Raw markdown, monospace, syntax highlighted (like VS Code / Zed) | yes | gpui-kit editor |
| **Split** | Source on the left, Preview on the right; `h_resizable("split")`, 50/50 default | left side | both |
| **Preview** | Read-only rendered document | no | `TextView` (§6.5) |

- **One buffer, many views:** the markdown text is the only model. Every mode reads and writes the same string, so switching modes never changes the file. The cursor/selection offset carries over when switching between Live and Source.
- **Source styling:** monospace 15px, soft wrap, ~25px padding, no line numbers, markdown highlighting via the `tree-sitter-markdown` feature.
- **Default mode:** Live once Live stage A ships (PLAN Phase 9). Until then the default is Source.
- **Status bar** (`StatusBar` component): left `Ln {line}, Col {col}` (1-based, from the cursor offset via `kunotes_core::cursor`), right `{n} characters` (grapheme count). Monospace 11px.
- **Title:** the window title and the centered title-bar text show the file name without `.md`.

### 6.4 Formatter bar (`ui/editor/formatter_bar.rs`)

Each button calls a **pure** transform in `kunotes_core::format` and applies the result to the editor in one undoable replace. Then it sets the new selection or cursor.

```rust
pub struct Edit { pub range: Range<usize>, pub replacement: String, pub new_selection: Range<usize> }

pub fn wrap(text: &str, sel: Range<usize>, prefix: &str, suffix: &str) -> Edit;   // **bold**, *italic*, `code`
pub fn line_prefix(text: &str, sel: Range<usize>, prefix: &str) -> Edit;           // "# ", "## ", "> ", "- ", "1. "
pub fn link(text: &str, sel: Range<usize>) -> Edit;                                // [sel](url), selects "url"
pub fn code_block(text: &str, sel: Range<usize>) -> Edit;                          // ```\nsel\n```
pub fn horizontal_rule(text: &str, sel: Range<usize>) -> Edit;                     // \n\n---\n\n
```

Groups: **B** *I* | H1 H2 | link, inline code, code block, quote | bullet, numbered | horizontal rule.

Ranges are **UTF-8 byte offsets on char boundaries**, which matches GPUI's text APIs. Tests cover multi-byte text (emoji, CJK).

With no selection, transforms apply at the cursor position. They fall back to the end of the document only if the editor has never been focused.

### 6.5 Preview (`ui/editor/preview.rs`)

- `TextView::markdown("preview", content).scrollable(true)` with selectable text (on by default) and ~25px padding.
- It must render: headings H1–H6, paragraphs, bold, italic, inline code (accent color), links (accent color with underline, opened in the browser via `cx.open_url`), fenced code blocks (muted rounded background, monospace), ordered and unordered lists with nesting, task lists with checked and unchecked boxes, blockquotes (accent left bar, muted text), and thematic breaks.
- **Throttle:** in Split mode, update the preview at most every ~150ms while typing, so large notes don't re-parse on every keystroke.
- **Styling:** `TextViewStyle` sets heading sizes, code-block background, and inline-code style. Blockquote styling is fixed by the component (muted text, left border in the border color) and is accepted as is.

### 6.6 Quick switcher (`ui/quick_switcher.rs`)

- Opened with `window.open_dialog(..)` containing a gpui-kit `Command` palette. Width 480.
- Items: every `.md` file in the vault, flattened (`kunotes_core::search::flatten_files`). The label is the name without `.md`. The right-hand hint is the parent folder name in uppercase.
- Filter: case-insensitive substring on the file name (Command's built-in matching; add the relative path as a keyword).
- Keys: `up`/`down` move the highlight, `enter` opens the file and closes the dialog, `escape` closes it. Footer hints: "↑↓ move · ↵ open · esc close".
- Empty state: "No matches".

### 6.7 Dialogs (`ui/dialogs.rs`)

- **Rename:** a dialog with an `Input` pre-filled with the name (without `.md` for files) and focused with the text selected. Enter confirms and Esc cancels. Invalid names show an inline error and don't close the dialog.
- **Delete:** an `AlertDialog` titled "Delete?" that names the item, with a destructive "Move to Trash" button and Cancel.
- **Errors** (I/O failures, name collisions) appear as `window.push_notification(..)`. Nothing fails silently.

### 6.8 Empty states (`ui/empty_state.rs`)

- No vault: a large muted folder-plus icon, "No Vault Selected", "Open a folder to use it as your vault.", and an [Open Vault…] button.
- Vault open but no file selected: a note icon, "No File Selected", "Select a file from the sidebar to start writing."

### 6.9 Live mode (`ui/editor/live/`)

Live mode is a "realtime formatter", in the style of Obsidian Live Preview or Typora. It is **not** a rich-text WYSIWYG: there is no separate document model and no HTML round-trip. The raw markdown string stays the single source of truth, and Live mode is only a different way to draw and edit it. Saving writes exactly the characters in the buffer.

It ships in two stages.

#### Stage A: styled source (PLAN Phase 9)

- Uses the **unstyled** `gpui_base::input::Editor` over an `EditorState`, with a proportional UI font. The styled `Editor` component can't be used here because it forces the global highlight theme, which would also restyle Source mode.
- A custom highlight theme for the markdown tree-sitter captures: headings bold and accent-colored, `**bold**` bold, `*italic*` italic, inline code monospace with a muted background, links accent-colored, and markers (`#`, `**`, `*`, `` ` ``, `>`, `-`, `[ ]`) dimmed.
- Highlight styles come from a custom `HighlightStyleResolver` passed through `set_editor_style`, which allows color, weight, italic, and background per capture.
- Limits: markers stay visible and every line keeps the same font size (GPUI highlight styles have no font size), so headings are bold and colored but not bigger.
- Low effort. Gives a usable formatted writing view early.

#### Stage B: custom live editor (PLAN Phase 10)

A purpose-built editor element, because the gpui-kit editor can't hide characters or vary line heights.

```
ropey::Rope (buffer, byte offsets)
   │  on change: incremental re-parse of affected blocks
   ▼
pulldown-cmark (offset iter) ──► Vec<Block { range, kind, inline_spans }>
   │
   ▼
layout: per block → shaped lines (StyledText / text_system().shape_line) with runs
        markers hidden unless the block or inline span contains the cursor/selection
   │
   ▼
paint: text, cursor, selection, widgets (checkbox, hr, code-block background, quote bar)
   ▲
   │  EntityInputHandler (typing, IME composition, marked text; UTF-16 ranges ⇄ rope byte offsets) + key actions
```

Rendering rules:
- **Reveal on cursor:** the block containing the cursor (and any block the selection touches) shows its raw markers, still styled. Other blocks hide markers: `## Title` shows as a large "Title", `**x**` as bold "x", `[label](url)` as a link "label".
- **Headings:** sizes H1 > H2 > … > H6 with matching line heights.
- **Task items:** `- [ ]` / `- [x]` render as a checkbox. Clicking toggles the character in the buffer (one undoable edit).
- **Code blocks:** monospace with a muted background. The fences show only when the cursor is inside. Optional syntax highlight later.
- **Blockquote:** accent left bar. **Thematic break:** a horizontal rule. **Lists:** bullets and numbers drawn as markers, nesting by indent.
- **Links:** Cmd/Ctrl+click opens the URL. A plain click places the cursor.
- Images and tables are out of scope for v2 and show as styled source.

Editing requirements (the hard part; each needs tests or a manual check on all three OSes):
- Cursor movement over hidden markers (left/right, up/down keeping the visual column, home/end, word jumps, page up/down).
- Mouse: click, drag-select, double-click word, triple-click line, shift-click extend.
- IME composition (CJK input) through `EntityInputHandler` marked-text APIs.
- Undo/redo with grouping, clipboard copy/cut/paste as plain markdown, select all.
- Soft wrap, vertical scrolling, scroll-to-cursor, and only laying out visible blocks for large notes.
- Smart list continuation: Enter on a list item continues the list, Enter on an empty item ends it.
- The formatter bar and editor shortcuts use the same `kunotes_core::format` transforms as Source mode.

Code layout: `ui/editor/live/{mod.rs, buffer.rs, layout.rs, element.rs, input.rs, actions.rs}`. Pure parts (block parsing, marker ranges, cursor movement over hidden ranges, list continuation) live in `kunotes-core::live` with unit tests.

Fallback: if stage B can't hit the editing requirements on every OS, Live keeps stage A and stage B continues behind a setting.

---

## 7. Data integrity

### 7.1 Save pipeline (`autosave.rs`)

```
InputEvent::Change ─► SaveDebouncer::schedule(content, path)
                          │  replaces the pending task (drop = cancel)
                          ▼
                 cx.spawn: timer(500ms) ─► flush()
flush(): if pending.take() → background atomic_write(path, content)
```

When to flush immediately:
- the selected file changes (before the old `EditorPane` is dropped),
- the vault is closed or switched,
- `cx.on_app_quit` (and the main window closing),
- `secondary-s`, as an explicit "save now".

`atomic_write`: write to `.<name>.kunotes.tmp` in the same directory, `fsync`, then rename over the target. `std::fs::rename` replaces existing files on Windows too. The temp name starts with a dot, so the tree scanner skips it.

### 7.2 Watcher (`watcher.rs`)

- `notify-debouncer-full` uses a recursive watch on the vault root with a 500ms debounce.
- Any event sends a refresh signal. Our own writes also trigger a rescan. That's harmless because the rescan is cheap and runs in the background, and the tree diff keeps the selection and expansion.
- On Linux, inotify watch limits can be hit on huge vaults. On error, log it, push one notification ("Live sync unavailable"), and keep the app working.

### 7.3 External changes to the open file

Out of scope for v1: the editor doesn't reload when the open file changes on disk, so the next autosave overwrites the external edit. A later improvement would detect an mtime mismatch before saving and prompt.

---

## 8. Cross-platform concerns

### 8.1 Keybindings

Use GPUI's `secondary-` modifier, which is `cmd` on macOS and `ctrl` on Windows and Linux, for every app shortcut. Never hard-code `cmd-`.

| Action | Binding | Context |
|---|---|---|
| Open vault | `secondary-o` | Workspace |
| New file | `secondary-n` | Workspace |
| New folder | `secondary-shift-n` | Workspace |
| Quick switcher | `secondary-k`, `secondary-shift-o` | Workspace |
| Save now | `secondary-s` | EditorPane |
| Toggle sidebar | `secondary-\` | Workspace |
| View: Live / Source / Split / Preview | `secondary-1` / `secondary-2` / `secondary-3` / `secondary-4` | Workspace |
| Cycle view mode | `secondary-e` | Workspace |
| Bold / Italic / Link | `secondary-b` / `secondary-i` / `secondary-shift-k` | Editor |
| Delete selection | `backspace`, `delete`, `secondary-backspace` | FileTree |
| Rename selection | `f2` (Win/Linux), `enter` (macOS Finder-like) | FileTree |
| Quit | `cmd-q` (macOS), `ctrl-q` (Linux), `alt-f4` (Windows, provided by the OS) | global |

### 8.2 Filenames (`kunotes_core::names`)

Rename and create reject, with a readable error:
- empty or whitespace-only names, `.` and `..`,
- path separators `/` and `\` (prevents escaping the parent dir),
- on **all** platforms, Windows-invalid characters `< > : " | ? *` and control characters (vaults move between OSes via git, Dropbox, and similar),
- Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, case-insensitive, with or without an extension),
- names ending in a space or a dot.

**Case-only renames** (`note.md` → `Note.md`) on case-insensitive filesystems (the macOS and Windows defaults): rename via a temporary name so the collision check doesn't block it.

### 8.3 Scanning

- Include directories and files whose extension is `md` (case-insensitive). Skip anything whose name starts with `.` (covers `.git`, `.obsidian`, and our temp files).
- **Don't follow directory symlinks** (prevents cycles). Symlinked `.md` files are shown.
- Unreadable directories are shown empty, not as an error.

### 8.4 OS integration labels (`platform.rs`)

| | macOS | Windows | Linux |
|---|---|---|---|
| Reveal | "Reveal in Finder" | "Show in Explorer" | "Open Containing Folder" |
| Trash wording | "Move to Trash" | "Move to Recycle Bin" | "Move to Trash" |
| App menu | native menu bar via `cx.set_menus` | menu in the title bar | menu in the title bar |

### 8.5 Linux (Fedora) runtime requirements
A Wayland or X11 session with a working Vulkan driver (`vulkan-loader` plus Mesa or the vendor driver). The folder picker uses the XDG desktop portal. Fedora Workstation ships `xdg-desktop-portal-gnome`; the KDE spin ships `xdg-desktop-portal-kde`. Distribution is `.rpm` (see PLAN Phase 8).

---

## 9. Error handling

- `kunotes-core` returns `Result<T, CoreError>` (`thiserror`), with variants such as `NameInvalid(reason)`, `AlreadyExists(path)`, `MoveIntoSelf`, and `Io(io::Error)`.
- `VaultStore` turns errors into `VaultEvent::Error`, and `Workspace` shows them with `push_notification`.
- Nothing is silently swallowed except "file vanished during scan", which is expected during external churn.

---

## 10. Feature summary

| Area | Features | Spec |
|---|---|---|
| Vault | Open a folder via the native picker, reopen the last vault on launch | §4.1, §4.3 |
| File tree | Folders + `.md` only, folders first, natural sort; click to select, double-click or chevron to expand; keyboard nav; drag-and-drop move; context menu | §6.2, §8.3 |
| File ops | New file (`Untitled.md`, seeded `# Untitled`), new folder, rename (auto `.md`, validated), delete to OS trash with confirmation | §4.1, §6.7, §8.2 |
| Editor | Live (realtime formatter), Source (highlighted raw markdown), Split, Preview modes on one text buffer; formatter bar, breadcrumb, status bar (Ln/Col, characters) | §6.3, §6.4, §6.9 |
| Preview | CommonMark: headings, emphasis, inline code, code blocks, nested lists, task lists, blockquotes, links, rules; selectable text | §6.5 |
| Saving | Debounced atomic autosave, flushed on switch and quit | §7.1 |
| Quick switcher | Filter all notes by name, keyboard driven | §6.6 |
| Live sync | Changes made outside the app appear in the tree | §7.2 |
| Layout | Resizable, toggleable sidebar; view mode and sidebar width remembered | §5, §4.3 |

---

## 11. Open questions (Phase 0 spike results)

Answered from crate source. Details, signatures, and file references are in [implementation/spike-notes.md](./implementation/spike-notes.md).

| # | Question | Result |
|---|---|---|
| 1 | Editor API | `EditorState` + `.language("markdown")`, `.line_number(false)`, soft wrap on by default. Byte offsets. No public range replace: select the range, then `replace` (undoable). |
| 2 | TextView | Renders task lists (display-only), heading-size and code-block style hooks, link handler. Blockquote style is fixed (muted + left border), which we accept. No pulldown-cmark fallback needed. |
| 3 | `secondary-` | Supported. |
| 4 | Double-click | `ClickEvent::click_count()`. |
| 5 | Drag and drop | `on_drag` / `on_drop` / `drag_over` exist; `ListItem` supports them. Runtime check inside `uniform_list` pending. |
| 6 | ToggleGroup | Multi-select; re-clicking unchecks. View mode derives the changed index and ignores un-checking the active mode. |
| 7 | App menu Win/Linux | `AppMenuBar` component, fed via `GlobalState::set_app_menus`. |
| 8 | Theme sync | `Theme::sync_system_appearance` + `cx.observe_window_appearance`. |
| 9 | Folder picker | `prompt_for_paths` returns a oneshot receiver; `Err` shows a notification. Fedora runtime check pending. |
| 10 | Live stage A styling | Color, weight, italic per capture; no per-capture font size. Must use the unstyled `gpui_base::input::Editor` + `set_editor_style`, because the styled component forces the global highlight theme. |
| 11 | Live stage B input | `EntityInputHandler` with **UTF-16** ranges; reference `gpui-pre/examples/input.rs`. IME runtime check on Windows/Fedora pending. |
