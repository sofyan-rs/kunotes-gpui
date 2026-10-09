# KuNotes

A minimal, cross-platform markdown vault app, in the style of Obsidian but without Electron. It's built in Rust with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://gpui-kit.com/), and runs on **macOS, Windows, and Linux (Fedora)** from one codebase.

Open any folder on disk as a "vault", browse it in a sidebar, and edit markdown files directly. Files on disk are the only source of truth, so there is no database, sync service, or lock-in.

> **Status:** planning. See [docs/implementation/PLAN.md](docs/implementation/PLAN.md).

## Features

- **Vault-based:** open any folder as a vault. Notes stay as plain `.md` files.
- **File tree:** click to select, double-click a folder to expand or collapse, drag and drop to move, and a context menu (new file or folder, rename, delete to trash, reveal in file manager, copy path or relative path).
- **Edit / Split / Preview:** raw markdown, rendered preview, or both side by side.
- **Formatter bar:** bold, italic, H1/H2, link, inline code, code block, quote, lists, horizontal rule.
- **CommonMark preview:** headings, emphasis, inline code, fenced code blocks, ordered, unordered, and task lists, blockquotes, links, rules.
- **Autosave:** debounced atomic writes, with no save button and no data loss on quit.
- **Quick switcher** (`Ctrl/⌘+K`): filter all notes by name and open one from the keyboard.
- **Live external sync:** changes made outside the app show up automatically.
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

- [Architecture](docs/ARCHITECTURE.md): stack, workspace layout, state model, components, cross-platform concerns, feature summary
- [Implementation plan](docs/implementation/PLAN.md): phased roadmap, feature checklist, risks
- [Contributing](docs/CONTRIBUTION.md): setup, conventions, testing, PR checklist

## Scope

This is intentionally minimal: no wikilinks, backlinks, graph view, tags, or plugins. The whole app is "open a folder, browse it, edit markdown."
