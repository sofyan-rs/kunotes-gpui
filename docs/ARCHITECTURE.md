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
- Split editor groups (several tabs side by side).
- Reloading the open editor's content when the file changes on disk (see [§7.3](#73-external-changes-to-the-open-file)).

---

## 2. Tech stack

| Concern | Choice | Notes |
|---|---|---|
| UI framework | `gpui-kit = "0.7.1"` | Re-exports GPUI. Do not add `gpui` separately. |
| Toolchain | Rust ≥ 1.92, edition 2024 | 1.92 is required by gpui-kit's locked dependency graph. Windows needs the MSVC toolchain. |
| Markdown preview | `gpui_kit::component::text::TextView::markdown` | Renders task lists; style hooks via `TextViewStyle` (§6.5). |
| Editor (Source, Split) | gpui-kit `EditorState` (`.language("markdown")`, soft wrap, no line numbers) | Styled by our own `InputHighlighter` (`ui/editor/markdown_style.rs`). |
| Editor (Live) | Custom `LiveEditor` element on GPUI primitives (`EntityInputHandler`, `shape_text` layout) over a plain `String` buffer (`kunotes_core::live_buffer`) | See §6.9. |
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
│   │   │   ├── live.rs        # markdown parts (spans) for Live and Source styling
│   │   │   ├── live_view.rs   # what each Live line draws; drawn ⇄ file positions
│   │   │   └── live_buffer.rs # Live editor text, selection, undo, list continuation
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
│           ├── assets.rs      # icons: gpui-kit defaults + the extra icons we use
│           ├── settings_store.rs # Settings as a GPUI global, saved on change
│           ├── vault_store.rs # VaultStore entity: shared app state + events
│           ├── watcher.rs     # file watcher -> VaultStore::refresh
│           └── ui/            # all views, grouped by feature
│               ├── mod.rs
│               ├── workspace.rs       # root view: title bar + sidebar + editor
│               ├── title_bar.rs
│               ├── sidebar/
│               │   ├── mod.rs         # Sidebar view: header buttons + tree
│               │   ├── file_tree.rs   # FileTree (uniform_list rows, drag and drop)
│               │   ├── inline_edit.rs # rename / new note named inline
│               │   ├── keyboard.rs    # arrow-key navigation
│               │   ├── context_menu.rs
│               │   └── file_tree_tests.rs # headless UI tests (click, keys, drag)
│               ├── editor/
│               │   ├── mod.rs         # EditorPane: load, autosave, view modes, formatting
│               │   ├── editor_tests.rs # headless UI tests (autosave, rename, format)
│               │   ├── formatter_bar.rs
│               │   ├── markdown_style.rs # Source/Split colors (InputHighlighter)
│               │   ├── preview.rs     # rendered markdown, clickable checkboxes
│               │   ├── status_bar.rs
│               │   ├── view_mode_switch.rs
│               │   └── live/          # custom Live editor: mod, keys, context_menu, input, layout, style, element
│               ├── editor_area/       # tabs: EditorArea (mod.rs), tab_bar.rs, tests
│               ├── quick_switcher.rs
│               ├── quick_switcher_tests.rs
│               ├── dialogs.rs         # delete confirmation
│               └── empty_state.rs
├── assets/icon/kunotes.png    # 1024px app icon master
├── fixtures/sample-vault/     # sample notes for manual testing
├── packaging/                 # per-OS bundling files
│   ├── make_icons.sh          # regenerates all icons from the master (macOS)
│   ├── macos/KuNotes.icns
│   ├── windows/kunotes.ico
│   └── linux/icons/hicolor/   # freedesktop icon sizes for Fedora
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
                        │ update()              │ load_note / autosave
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
| `create_note(folder, name)` | Create exactly the typed name (`.md` added unless present), seeded with `# <name>`. Refuses a taken name. Opens the note and emits `NoteCreated` (the editor takes focus). |
| `create_folder(folder, name)` | Create exactly the typed name. Refuses a taken name. |
| `target_folder()` | Where a new item goes: the selected folder, the selected note's folder, or the root. |
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
// $KUNOTES_CONFIG_DIR/settings.json if set, else dirs::config_dir()/kunotes/settings.json
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
| Context menu (`context_menu.rs`) | Folders: New Note, New Folder, separator. All rows: Rename, Delete, separator, Reveal in Finder / Show in Explorer / Open Containing Folder, Copy Path, Copy Relative Path. |
| Inline editing (`inline_edit.rs`) | Like VS Code/Zed. **Rename** (F2, menu) turns the row's name into a field with the name minus `.md` selected. **New note/folder** (header buttons, `secondary-n`, `secondary-shift-n`, menu) shows an empty field at the top of the target folder, expanding it. Enter confirms; Escape cancels; focus leaving the field confirms (or cancels when empty). An invalid or taken name keeps the field open after Enter and shows a notification. Focus loss uses `cx.on_focus_out` on the field. |
| Drag and drop | Row `.on_drag(DraggedEntry { path }, preview)`. Folder rows and empty space below the list get `.drag_over::<DraggedEntry>(highlight)` and `.on_drop::<DraggedEntry>(→ VaultStore::move_into)`. Dropping on a file row is ignored. Dropping on empty space moves the item to the vault root. |
| Icons | Folder (blue) and file-text (muted). The icon turns white on the selected row. |
| Sort | Folders first, then natural case-insensitive order (`natord`). |

If Phase 0 shows that `Tree` can cover all of this cleanly, swapping it in is a contained change, because `visible_rows` and the `expanded` set stay the same.

### 6.3 Editor pane (`ui/editor/mod.rs`)

A new `EditorPane` entity is created whenever `selected_file` changes, so per-file state never leaks between files.

- **Load:** read the file as UTF-8 (lossy fallback with a warning notification), then `set_value`. Keep the original line ending style (`\n` vs `\r\n`) and write it back unchanged.
- **Header row:** breadcrumb on the left (path relative to the vault root, vault name omitted, `.md` stripped, last segment highlighted, chevron separators). A segmented control on the right (`ui/editor/view_mode_switch.rs`): muted track, icon + label per mode, the active mode raised; clicking sends the same action as the shortcut, with the shortcut in a tooltip.
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
- **Default mode:** Live.
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

- `TextView::markdown("preview", content).scrollable(true)` with selectable text (on by default) and the same 24px side padding as Live and Source.
- **Clickable task checkboxes:** lists with tasks are parsed into a custom block and drawn by us; a click flips `[ ]` ⇄ `[x]` in the note (§6.9).
- It must render: headings H1–H6, paragraphs, bold, italic, inline code (accent color), links (accent color with underline, opened in the browser via `cx.open_url`), fenced code blocks (muted rounded background, monospace), ordered and unordered lists with nesting, task lists with checked and unchecked boxes, blockquotes (accent left bar, muted text), and thematic breaks.
- **Throttle:** in Split mode, update the preview at most every ~150ms while typing, so large notes don't re-parse on every keystroke.
- **Styling:** `TextViewStyle` sets heading sizes, code-block background, and inline-code style. Blockquote styling is fixed by the component (muted text, left border in the border color) and is accepted as is.

### 6.6 Quick switcher (`ui/quick_switcher.rs`)

- A small `QuickSwitcher` view (own search field + `uniform_list`) inside `window.open_dialog(..)`, width 480. gpui-kit's `Command` palette was replaced because its search field can't be padded (it looked cramped).
- Items: every `.md` file in the vault, flattened (`kunotes_core::search::flatten_files`). The label is the name without `.md` (long names end with "…"). The right-hand hint is the parent folder name in uppercase.
- Filter: every typed word must appear in "title + vault-relative path" (case-insensitive).
- Keys (key context `QuickSwitcher`): `up`/`down` move the highlight, `enter` opens the note and closes the dialog, `escape` closes it. Enter is bound in the switcher's own context so it's handled before the dialog's own Enter (which would only close it). Footer hints: "↑↓ move · ↵ open · esc close".
- Empty state: "No matches".

### 6.7 Dialogs (`ui/dialogs.rs`)

- **Rename and new items** are edited inline in the tree (§6.2), not in dialogs.
- **Delete:** an `AlertDialog` titled "Delete?" that names the item, with a destructive "Move to Trash" button and Cancel.
- **Errors** (I/O failures, name collisions) appear as `window.push_notification(..)`. Nothing fails silently.

### 6.8 Empty states (`ui/empty_state.rs`)

- No vault: a large muted folder-plus icon, "No Vault Selected", "Open a folder to use it as your vault.", and an [Open Vault…] button.
- Vault open but no file selected: a note icon, "No File Selected", "Select a file from the sidebar to start writing."

### 6.10 Tabs (`ui/editor_area/`)

`EditorArea` is the right side of the window: a tab bar above the active note's editor, like VS Code. The tab rules live in `kunotes_core::tabs::TabList` (pure, unit-tested); `EditorArea` connects them to editors and the tree.

- **Preview tabs:** a single click in the tree opens a *preview* tab (italic), which the next clicked note replaces. Typing in it, double-clicking the note in the tree, double-clicking the tab, or opening it from the quick switcher or as a new note makes it permanent.
- **One editor per tab** (`EditorPane`), created the first time the tab is shown, so the cursor and unsaved changes survive switching. Leaving a tab saves it immediately; every pane also autosaves.
- **Pinned tabs** stay on the left with a pin icon, and Close Others / to the Right / All skip them. Dragging keeps pinned and unpinned tabs separate.
- **Tab bar:** click activates; double-click keeps; middle-click closes; drag to reorder; a dot replaces the close button while a note has unsaved changes. Right-click selects the tab and shows: Close (`secondary-w`), Close Others (`secondary-alt-t`), Close to the Right, Close Saved, Close All (`secondary-shift-w`), Pin/Unpin, Copy Path, Copy Relative Path, Reveal in Finder/Explorer. `ctrl-tab` / `ctrl-shift-tab` cycle tabs.
- **Sync with the vault:** the active tab is the vault's `selected_file` (so the tree highlights it). Renames and moves remap every tab (`TabList::remap`) and its editor. Tabs of notes that no longer exist close without saving.
- **Restore:** open tabs (order, pin, preview) and the active tab are saved in settings (`open_tabs`, `active_tab`) and reopened for the same vault on launch. Opening another vault saves and closes the old tabs.
- Tab actions are bound in the `Workspace` context and forwarded to `EditorArea`, so they work wherever the focus is.

### 6.9 Live mode (`ui/editor/live/`)

Live mode is a "realtime formatter", in the style of Obsidian Live Preview or Typora. It is **not** a rich-text WYSIWYG: there is no separate document model and no HTML round-trip. The raw markdown string stays the single source of truth, and Live mode is only a different way to draw and edit it. Saving writes exactly the characters in the buffer.

It shipped in two stages. Stage A (a styled gpui-kit editor with markers faded but visible) was replaced by stage B, a custom editor, because the gpui-kit editor can't hide characters or vary font sizes per line.

**Who owns the text.** `EditorPane` holds two editors: `LiveEditor` (Live) and gpui-kit's `EditorState` (Source and Split). Only one is in charge at a time (`live_active`). When the mode switches between them, `sync_active_editor` hands the text and cursor over (`LiveEditor::set_text` / `EditorState::replace_all`, only if the text differs). Preview keeps whichever was in charge. Saving, the preview, the status bar and the formatter bar all read from the editor in charge (`EditorPane::text`). The file bytes never change from switching (tested).

**Pure logic (`kunotes-core`, unit-tested):**
- `live.rs`: `spans(text)`, a single-pass, line-by-line scan into styled parts (headings, bold/italic, inline code, links, list markers and task boxes, quotes, fenced code, rules, and the marker characters around them). Also drives Source-mode colors.
- `live_view.rs`: `lines(text)` splits into lines with their spans; `line_view(line, spans, revealed)` decides what one line draws (`LineKind`: heading, task, bullet, quote, code, rule, ...) and maps cursor positions between the drawn text and the file (`to_display` / `to_source`).
- `live_buffer.rs`: `LiveBuffer`, the text + selection: grapheme-aware movement, word movement, select word/line, delete, Enter with list continuation (`- `, `1. ` → `2. `, `- [ ] `; Enter on an empty item ends the list), `toggle_task`, undo/redo with typing grouped. `task_toggle(text, offset)` is shared with the Preview's checkboxes.

**App (`ui/editor/live/`):**
- `mod.rs`: the `LiveEditor` entity (buffer, focus, IME marked range, last layout, scroll) and the mouse: click, shift-click, drag-select, double-click word, triple-click line, checkbox click.
- `context_menu.rs`: right-click menu (Cut, Copy, Paste, Bold, Italic, Link, Select All). Items are actions with `action_context` set to the editor, so they show their shortcuts. Right-click outside the selection moves the cursor first.
- `keys.rs`: keyboard actions. The root uses gpui-kit's **`Input` key context**, so its bindings (arrows, Home/End, word jumps, delete-word, undo/redo, clipboard, select all, Enter, Tab) work with each OS's usual keys; we only handle the actions.
- `input.rs`: `EntityInputHandler` (typing, IME composition, emoji picker), converting the OS's UTF-16 ranges to UTF-8 byte offsets.
- `layout.rs`: one shaped `WrappedLine` per line (`text_system().shape_text` with a wrap width). The element uses a *measured* layout (`request_measured_layout`), because the height depends on the width. Also hit-testing (`offset_for_point`), caret position, Up/Down at a kept x, selection rectangles.
- `style.rs`: fonts and colors from the theme.
- `element.rs`: paints code backgrounds, quote bars, rules, checkboxes, the selection, text backgrounds (inline code), text, placeholder and caret; registers the input handler; scrolls the caret into view.

**Rendering rules:**
- **Reveal on cursor:** every line the cursor or selection touches shows its raw markers (dimmed). Other lines hide them: `## Title` draws a large "Title", `**x**` a bold "x", `[label](url)` a link "label", `- item` a "• item", `- [x] done` a checkbox and "done".
- **Looks like Preview:** heading sizes and weights (28/21/17.5/16px, bold/semibold), foreground heading color, inline code on the accent background, links in the primary color, 14px square checkboxes, muted quotes with a 3px bar, and empty lines as tall as Preview's paragraph gap. Live, Source and Preview text all start 24px from the editor's left edge.
- **Code blocks:** monospace with a background; the fences stay visible.
- Images and tables show as styled source.

**Preview checkboxes.** `preview.rs` installs a markdown block parser that turns lists containing tasks into a custom block, drawn with our own clickable checkboxes (the item text is rendered by a nested `TextView`). A click calls `EditorPane::toggle_task(offset)` through a `WeakEntity` captured by the renderer (actions don't reach the pane in Preview, since nothing there has focus).

Not done yet: page up/down, Cmd/Ctrl+click on links, laying out only visible lines for very large notes, and a manual IME check on Windows and Fedora.

---

## 7. Data integrity

### 7.1 Save pipeline (`ui/editor/mod.rs`)

```
InputEvent::Change ─► dirty = true; schedule_save()
                          │  replacing `save_task` drops (cancels) the previous timer
                          ▼
                 cx.spawn: timer(500ms) ─► save_now()
save_now(): if dirty && can_save → atomic_write(path, line_ending.apply(text))  (UI thread)
```

The write itself runs on the UI thread, on purpose. Notes are small, so it takes well under a frame, and a single writer means two saves can never race on the same temp file.

When to save immediately (`save_now`):
- another note gets selected. The workspace saves the old pane first, unless its file no longer exists: a deleted note must not be written back.
- the open note is renamed or moved. The pane isn't recreated; `VaultStore::last_move` lets the workspace call `set_path`, so unsaved typing follows the note.
- `cx.on_app_quit`, and when the window closes (the app quits when its only window closes).
- `secondary-s`, as an explicit "save now".

Files that aren't valid UTF-8 open read-only (`can_save = false`) and are never written.

`atomic_write`: write to `.<name>.kunotes.tmp` in the same directory, `fsync`, then rename over the target. `std::fs::rename` replaces existing files on Windows too. The temp name starts with a dot, so the tree scanner skips it.

### 7.2 Watcher (`watcher.rs`)

- `watch_os` (no GPUI, tested with real files) runs `notify-debouncer-full` recursively on the vault with a 500ms debounce. It reports a change only if some event path is visible, meaning no part below the vault root starts with `.`. That skips `.git` churn and our own `.name.kunotes.tmp` save files.
- Event paths are compared against both the opened path and its canonical form, because the OS may report the real path (macOS `/var` → `/private/var`, symlinked vaults). Canonicalizing alone would break Windows (`\\?\` prefix).
- `start` sends signals over a `futures` channel to a `cx.spawn` loop that calls `VaultStore::refresh`, which scans in the background.
- After each rescan, `forget_missing_paths` drops a selection or expanded folder whose path is gone. A note deleted outside the app therefore closes, and the workspace doesn't write it back.
- On a watcher error (e.g. the Linux inotify limit): log it, show one notification ("Live sync is unavailable…"), keep working.
- UI tests call `VaultStore::disable_live_sync`, because the watcher's OS thread breaks GPUI's deterministic test scheduler.

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
| Rename selection | `f2` (all platforms; `enter` opens/toggles instead) | FileTree |
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
| File ops | New note / folder and rename, named inline in the tree (auto `.md`, validated, taken names refused), delete to OS trash with confirmation | §4.1, §6.2, §6.7, §8.2 |
| Editor | Live (realtime formatter), Source (highlighted raw markdown), Split, Preview modes on one text buffer; formatter bar, breadcrumb, status bar (Ln/Col, characters) | §6.3, §6.4, §6.9 |
| Preview | CommonMark: headings, emphasis, inline code, code blocks, nested lists, task lists, blockquotes, links, rules; selectable text | §6.5 |
| Saving | Debounced atomic autosave, flushed on switch and quit | §7.1 |
| Quick switcher | Filter all notes by name, keyboard driven | §6.6 |
| Tabs | Several notes open; preview tabs; pin, close variants, drag to reorder, copy path, reveal; restored on launch | §6.10 |
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
