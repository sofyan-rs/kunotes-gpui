# Contributing

This guide covers setting up the project, building it, and the rules for changes. Read [ARCHITECTURE.md](./ARCHITECTURE.md) for the design and [implementation/PLAN.md](./implementation/PLAN.md) for what's being built next.

---

## 1. Prerequisites

All platforms need **Rust ≥ 1.92** via [rustup](https://rustup.rs/). `rust-toolchain.toml` pins the exact version, and rustup installs it automatically.

### macOS (15+)
```bash
xcode-select --install
xcode-select -p        # must print a developer directory
```

### Windows (10+)
- Visual Studio 2022 Build Tools (or Community) with the **"Desktop development with C++"** workload, which includes MSVC and the Windows SDK.
- Use the **MSVC** Rust toolchain, not GNU. `rustup show active-toolchain` should end in `-msvc`.
- CMake on `PATH` (`cmake --version`).

### Linux (Fedora)
Fedora is the supported Linux target.
```bash
sudo dnf install -y gcc gcc-c++ clang cmake pkgconf-pkg-config \
  fontconfig-devel wayland-devel webkit2gtk4.1-devel \
  libxkbcommon-x11-devel libxcb-devel libX11-devel \
  openssl-devel libzstd-devel vulkan-loader vulkan-validation-layers vulkan-tools
```
- This package list maps gpui-kit's documented Ubuntu dependencies to Fedora names. Confirm it during Phase 0 and fix this section if anything is missing.
- Running the app needs a Wayland or X11 session and a **working Vulkan driver** (`vulkaninfo` should succeed).
- The folder picker needs the XDG desktop portal. Fedora Workstation includes `xdg-desktop-portal-gnome` and the KDE spin includes `xdg-desktop-portal-kde`.
- Other distros aren't supported, but equivalent packages should work.

---

## 2. Build and run

```bash
cargo run -p kunotes                       # debug build of the app
cargo run -p kunotes --release             # release build
RUST_LOG=kunotes=debug cargo run -p kunotes  # verbose logging
cargo test --workspace                     # all tests
cargo test -p kunotes-core                 # fast pure-logic tests only
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Windows PowerShell: `$env:RUST_LOG="kunotes=debug"; cargo run -p kunotes`

The first build is slow because GPUI is large. Framework crates are built at `opt-level = 3` even in debug (see `[profile.dev.package]` in the root `Cargo.toml`), which makes the first build slower but keeps the debug app responsive.

A test vault: open `fixtures/sample-vault/`. Its `Example.md` uses every markdown element the preview must render.

---

## 3. Project structure

```
crates/kunotes-core/   library: pure logic, no GPUI (start at src/lib.rs)
crates/kunotes/        app: main.rs → app.rs → ui/workspace.rs
  src/ui/sidebar/      file tree and sidebar header
  src/ui/editor/       editor pane, preview, formatter bar, status bar
fixtures/              sample vault for manual testing
docs/                  architecture, plan, spike notes
```

The full tree, the rules for adding files, and the "where does my code go?" table are in [ARCHITECTURE §3](./ARCHITECTURE.md#3-project-structure).

**Rule of thumb:** if code can be written without `gpui`, it goes in `kunotes-core` with tests. UI files should mostly be layout and event wiring.

### Reading order for newcomers

1. `crates/kunotes-core/src/lib.rs`, then any module with its tests at the bottom. The tests show how each function is meant to be used.
2. `crates/kunotes/src/main.rs` → `app.rs` → `ui/workspace.rs`. This follows how the window is built.
3. `vault_store.rs`, which is the shared state every view reads from.
4. One feature folder, e.g. `ui/sidebar/`.

---

## 4. Code conventions

### General
- Edition 2024. `rustfmt` defaults. Clippy must be clean with `-D warnings`.
- Don't call `unwrap()` or `expect()` in app code paths. Exceptions are startup invariants and tests. Use `?` and surface errors (§6).
- Use `std::path::{Path, PathBuf}` for paths. **Never** build paths with string concatenation or a hard-coded `/`.
- Every file starts with a `//!` comment saying what it's for. Doc comments (`///`) go on public items. Otherwise comment *why*, not *what*.
- Write for readers who are new to Rust. Prefer plain structs, enums, and functions over clever generics, trait tricks, or custom macros. When a Rust or GPUI idiom isn't obvious (a lifetime, `cx.listener`, `entity.update`), add a one-line comment.
- Follow the structure rules in ARCHITECTURE §3: one main type per file, at most two folder levels, files under ~400 lines, private by default.

### GPUI patterns
- Shared state is an `Entity<T>` (`VaultStore`). Views hold `Entity` handles and `cx.subscribe` to events. They don't hold copies of the state.
- Call `cx.notify()` after mutating any state that affects rendering.
- **No blocking I/O on the main thread.** Directory scans and writes go through `cx.background_spawn`. Results come back via `cx.spawn` / `this.update(..)`.
- Keep `Subscription`s alive by storing them in the view (`_subscriptions: Vec<Subscription>`).
- Timers and debounces: store the `Task`. Dropping or replacing it cancels the work.
- Key contexts: give focusable views a `key_context` (`Workspace`, `FileTree`, `EditorPane`) and scope keybindings to them.
- Import from `gpui_kit::*` and `gpui_kit::component::…`. Don't add `gpui` as a direct dependency.

### Cross-platform rules
- Keybindings use `secondary-` (cmd on macOS, ctrl elsewhere). Never `cmd-` or `ctrl-` alone unless the binding really is OS-specific, and then gate it with `#[cfg]`.
- User-facing OS terms (Finder/Explorer, Trash/Recycle Bin) come from `platform.rs`, never inline strings.
- `#[cfg(target_os = "…")]` belongs only in `platform.rs` (and tests that need it). Everything else stays platform-neutral.
- Don't assume a case-sensitive filesystem or `\n` line endings. Preserve what's on disk.
- Filenames must pass `kunotes_core::names::validate_name`.

### Data safety (non-negotiable)
- Every write to a user file goes through `kunotes_core::fs_ops::atomic_write`.
- Deletes go through `trash`. **Never** call `remove_file` or `remove_dir_all` on vault content.
- Any change that touches loading or saving needs a manual test: type, then switch files or quit within 500ms, then reopen and check that nothing was lost.

---

## 5. Testing

| Layer | How |
|---|---|
| `kunotes-core` | Plain `#[test]`. Filesystem tests use `tempfile::tempdir()`. OS-specific cases use `#[cfg(unix)]` / `#[cfg(windows)]`. |
| UI behavior | `gpui_kit::test` / `TestWindowExt` for view-level tests where they add value (tree selection, keybindings). |
| Manual | The feature checklist in PLAN.md, per OS, before each release. |

Every bug fix in `kunotes-core` gets a regression test. New format transforms need tests for empty selections, multi-byte text, and the start and end of the document.

---

## 6. Errors and logging

- `kunotes-core`: return `Result<_, CoreError>` (`thiserror`).
- App: `anyhow` is fine internally. User-visible failures become `VaultEvent::Error`, which shows as a notification.
- Use `log::{warn, error, debug}`, not `println!`.

---

## 7. Dependencies

- Add shared versions under `[workspace.dependencies]` and reference them with `dep.workspace = true`.
- Before adding a crate, check that it supports macOS, Windows, and Linux, is maintained, and that we really need it.
- gpui-kit upgrades go in their own PR, with notes on API changes and a manual smoke test on all three OSes.

---

## 8. Git workflow

- Branch from `main`: `feat/…`, `fix/…`, `docs/…`, `chore/…`.
- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat(tree): drag-and-drop move`, `fix(save): flush on window close`.
- Keep PRs small and focused. One plan checklist item per PR is ideal.

### PR checklist
- [ ] `cargo fmt`, `clippy -D warnings`, and `cargo test --workspace` pass locally.
- [ ] CI is green on macOS, Windows, and Fedora.
- [ ] New logic in `kunotes-core` has tests.
- [ ] No new `cfg(target_os)` outside `platform.rs`.
- [ ] Data-safety rules (§4) are respected. Save/load changes were tested manually.
- [ ] `docs/implementation/PLAN.md` checkboxes are updated. ARCHITECTURE.md is updated if the design changed.
- [ ] UI changes include screenshots (ideally on more than one OS).

---

## 9. Scope guard

KuNotes is intentionally minimal: **open a folder, browse it, edit markdown.** Proposals for wikilinks, backlinks, tags, graph, plugins, or sync should start as a discussion issue, not a PR.
