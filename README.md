# KuNotes

A minimal, cross-platform markdown vault app, in the style of Obsidian but without Electron. It's built in Rust with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://gpui-kit.com/), and runs on **macOS, Windows, and Linux (Fedora)** from one codebase.

Open any folder on disk as a "vault", browse it in a sidebar, and edit markdown files directly. Files on disk are the only source of truth, so there is no database or lock-in. Optionally, a vault can sync through your own git repository.

> **Status:** planning. See [docs/implementation/PLAN.md](docs/implementation/PLAN.md).

## Features

- **Vault-based:** open any folder as a vault. Notes stay as plain `.md` files.
- **File tree:** click to select, double-click a folder to expand or collapse, drag and drop to move, and a context menu (new file or folder, rename, delete to trash, reveal in file manager, copy path or relative path).
- **Four view modes on one markdown buffer:**
  - **Live:** formats as you type, like Obsidian Live Preview. Markers hide when the cursor leaves them.
  - **Source:** raw markdown with syntax highlighting, like VS Code or Zed.
  - **Split:** source and rendered preview side by side.
  - **Preview:** read-only rendered document.
- **Formatter bar:** bold, italic, strikethrough, H1–H3, link, image, inline code, code block, quote, bullet/numbered/task lists, table, horizontal rule.
- **Images:** inserted images are copied into a `.img` folder next to the note and shown in Live and Preview.
- **CommonMark preview:** headings, emphasis, inline code, fenced code blocks, ordered, unordered, and task lists, blockquotes, links, rules.
- **Autosave:** debounced atomic writes, with no save button and no data loss on quit.
- **Tabs** like VS Code: preview tabs, pin, close others/to the right/saved/all, drag to reorder, copy path, reveal in file manager; reopened on launch.
- **Quick switcher** (`Ctrl/⌘+K`): filter all notes by name and open one from the keyboard.
- **Live external sync:** changes made outside the app show up automatically.
- **Locked notes:** right-click a note → *Lock Note* to encrypt it (for credentials). One password per vault; notes lock again after 5 minutes. Encrypted with [age](https://age-encryption.org), so they also open with the `age` tool: `age -d .kunotes-lock.age > key.txt`, then `age -d -i key.txt "Note.md.age"`.
- **Git sync (optional):** enter a repository address (e.g. on GitHub) and the vault syncs automatically. Uses the git installed on your computer and its sign-in; a note changed on two computers keeps both versions.
- **Remembers your last vault** across launches.

## Requirements

- Rust ≥ 1.92
- macOS 15+, Windows 10+ (MSVC toolchain), or Fedora Linux (Wayland/X11 with Vulkan)

Per-platform setup is in [docs/CONTRIBUTION.md](docs/CONTRIBUTION.md#1-prerequisites).

## Build

```bash
cargo run -p kunotes            # debug
cargo run -p kunotes --release  # release
cargo test --workspace
```

## Docs

- [Architecture](docs/ARCHITECTURE.md): stack, project structure, state model, components, cross-platform concerns, feature summary
- [Implementation plan](docs/implementation/PLAN.md): phased roadmap, feature checklist, risks
- [Contributing](docs/CONTRIBUTION.md): setup, conventions, testing, PR checklist

## Scope

This is intentionally minimal: no wikilinks, backlinks, graph view, tags, or plugins. Sync is optional and only through your own git repository. The whole app is "open a folder, browse it, edit markdown."
