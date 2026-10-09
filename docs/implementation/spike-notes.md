# Phase 0 spike notes

These answers come from reading the source of `gpui-kit 0.7.1`, `gpui-component 0.7.1`, `gpui-base 0.7.1`, and `gpui-pre 0.3.8` (2026-10-09). Paths are relative to `~/.cargo/registry/src/index.crates.io-*/`:
- **base** = `gpui-base-0.7.1/src`
- **comp** = `gpui-component-0.7.1/src`
- **gpui** = `gpui-pre-0.3.8/src`

Status: ✅ confirmed in source · ⚠️ works with a caveat · 🧪 still needs a runtime check on the OS.

---

## 1. Editor (Source mode) ✅

Use **`EditorState`** (base/input/editor/mod.rs). It's the only multi-line state with `.language()`. `InputState` is single-line, and `TextareaState` has no language support.

```rust
let editor = cx.new(|cx| {
    EditorState::new(window, cx)
        .language("markdown")      // needs gpui-kit feature tree-sitter-markdown
        .line_number(false)        // default is true
        .placeholder("Start writing…")
});                                // soft wrap defaults to true
// render: comp::input::Editor::new(&editor)  (mono font by default; .font_family(..) overrides)
```

| Need | API (base/input/base/state.rs) | Units |
|---|---|---|
| Cursor offset | `cursor() -> usize` | **UTF-8 bytes** |
| Selection | `selected_range() -> Range<usize>`, `set_selected_range(range, cx)` (clips to char boundaries) | bytes |
| Line/col | `cursor_position() -> lsp_types::Position` | column in **chars** |
| Insert at cursor | `insert(text, window, cx)` | undoable |
| Replace selection | `replace(text, window, cx)` | undoable, one step |
| Replace everything | `replace_all(..)` keeps undo history; `set_value(..)` clears it | |
| Focus | `focus(window, cx)` | |
| Events | `InputEvent::{Change, PressEnter{secondary, shift}, Focus, Blur}` | |

⚠️ There's **no public "replace byte range"** method. The formatter bar has to call `set_selected_range(edit.range)`, then `replace(edit.replacement)`, then `set_selected_range(edit.new_selection)`. `kunotes_core::format` already uses byte offsets, which matches.

Status bar: compute Ln/Col with our own `kunotes_core::cursor::line_col(text, cursor())`, so the column units are under our control.

## 2. Highlight theming (Live stage A) ⚠️

- Each tree-sitter capture style (`ThemeStyle`, comp/highlighter/registry.rs:226) can set **color, italic, and font weight**. It can't set a background or a font size, and it's built by deserializing Zed-format theme JSON.
- GPUI `HighlightStyle` has a background but **no font size**. **Per-capture font size is impossible**, so stage A headings can only be bold and colored, not bigger. ARCHITECTURE §6.9 already lists this limit.
- ⚠️ The styled `Input`/`Editor` component overwrites its highlight style with the **global** `cx.theme().highlight_theme` on every render (comp/input/input.rs:556). A Live-specific theme would leak into Source mode. Stage A must use the **unstyled `gpui_base::input::Editor`** with `set_editor_style(InputEditorStyle { highlight_styles, .. })` (state.rs:866) and a custom `HighlightStyleResolver` (base/input/editor/highlighting.rs:13). That route also allows backgrounds, for example for inline code.
- A proportional font works: `.font_family(..)` on the element, with no monospace assumptions in layout.

## 3. TextView preview ✅ (no fallback needed)

```rust
TextView::markdown("preview", src)          // comp/text/compat.rs:68
    .selectable(true)
    .scrollable(true)
    .style(TextViewStyle::default().heading_font_size(|level, base| ..))
    .on_link_click(|url, _ev, _w, cx| cx.open_url(url))
```

- Task lists render as **display-only checkboxes** (base/text/node.rs:2957). That's fine for Preview.
- `TextViewStyle` hooks: `heading_font_size`, `code_block(StyleRefinement)` (background), `inline_code(HighlightStyle)`, `paragraph_gap`.
- ⚠️ Blockquote styling is hard-coded: muted foreground with a 3px left border in the border color, not the accent color. We accept this rather than build the pulldown-cmark fallback.

## 4. `secondary-` keybinding ✅

`secondary` maps to `platform` (cmd) on macOS and `control` elsewhere (gpui/platform/keystroke.rs:115-150).

## 5. Double-click ✅

`ClickEvent::click_count() -> usize` (gpui/interactive.rs:424). `ListItem::on_click(Fn(&ClickEvent, &mut Window, &mut App))` (comp/list/list_item.rs:115). Use `e.click_count() == 2` to toggle a folder.

## 6. Drag and drop ✅ 🧪

- `on_drag(value, |&T, Point<Pixels>, &mut Window, &mut App| -> Entity<W>)` needs `.id(..)` (gpui/elements/div.rs:1620).
- `on_drop::<T>(..)` (:1229), `drag_over::<T>(|style, &T, w, cx| style)` (:1191), `can_drop` (:1239).
- `ListItem` implements both the stateless and stateful interactive traits. Rows inside `uniform_list` haven't been tested yet; see the reference example gpui-pre/examples/drag_drop.rs.

## 7. ToggleGroup ⚠️

- `ToggleGroup::on_click(Fn(&Vec<bool>, ..))` (comp/button/toggle.rs:263).
- Clicking an already-checked toggle **unchecks it** (`next[ix] = !next[ix]`, :394). The group is multi-select.
- View-mode handling: compare the new states with the current mode to find the index that changed. If it went from checked to unchecked (re-clicking the active mode), keep the current mode.

## 8. App menus ✅

- macOS: `cx.set_menus([Menu::new(..).items([MenuItem::action(..), MenuItem::separator(), ..])])` (gpui/app.rs:2668).
- Windows/Linux: **`comp::menu::AppMenuBar::new(cx)`** (comp/menu/app_menu_bar.rs:34) renders an in-window menu bar. It must first be fed with `GlobalState::global_mut(cx).set_app_menus(menus.into_iter().map(Menu::owned).collect())` (base/global_state.rs:117). Build one `Vec<Menu>` and feed it to both. Put the bar in the `TitleBar` on non-mac targets.

## 9. System theme ✅

`Theme::sync_system_appearance(Some(window), cx)` (comp/theme/mod.rs:354). Nothing calls it automatically on changes, so subscribe in the workspace:

```rust
cx.observe_window_appearance(window, |_, w, cx| Theme::sync_system_appearance(Some(w), cx)).detach();
```

## 10. Folder picker ✅ 🧪

```rust
let rx = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Open Vault".into()) });
// rx: oneshot::Receiver<Result<Option<Vec<PathBuf>>>>
cx.spawn(async move |this, cx| if let Ok(Ok(Some(paths))) = rx.await { .. }).detach();
```

🧪 The behavior on Fedora without a portal backend is still unknown. Treat `Err` as "show a notification".

## 11. Custom editor input (Live stage B) ✅

- `EntityInputHandler` (gpui/input.rs:13). Methods: `text_for_range`, `selected_text_range`, `marked_text_range`, `unmark_text`, `replace_text_in_range`, `replace_and_mark_text_in_range`, `bounds_for_range`, `character_index_for_point` (+ defaults).
- ⚠️ **All ranges are UTF-16.** The live editor needs UTF-8 ⇄ UTF-16 offset conversion (`ropey` has `char_to_utf16_cu` / `utf16_cu_to_char`).
- Wiring: `window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx)` in `Element::paint`.
- A complete reference is in **gpui-pre-0.3.8/examples/input.rs**.

🧪 IME has to be checked on Windows (MS IME) and Fedora (ibus/fcitx5, Wayland) before Phase 10.

## 12. Other confirmed signatures

- `gpui_kit::open_window(opts, cx, |w, cx| Entity<V>) -> Result<(AnyWindowHandle, Entity<V>)>` wraps the view in `Root`. Dialogs and notifications work through `WindowExt`.
- `TitleBar::window_options()`, `TitleBar::new()`.
- `h_resizable(id)` / `v_resizable(id)`. `resizable_panel().size(px).size_range(a..b).visible(bool)`. `.with_state(&Entity<ResizableState>)`, `.on_resize(..)`.
- `StatusBar::new().left(el).right(el)`.
- `CommandState::new(window, cx)`. `Command::new(&state).items(..).placeholder(..).on_confirm(Fn(IndexPath, ..)).on_cancel(..)`. `CommandItem::new().label(..).keywords(..)`.
- `window.open_dialog(cx, |dialog, w, cx| dialog..)`, `window.open_alert_dialog(cx, |alert, w, cx| alert.title(..).description(..).ok_text(..).on_ok(|_, w, cx| true))`, `window.close_dialog(cx)`, `window.push_notification(msg, cx)`.
- `.context_menu(|menu: PopupMenu, w, cx| menu..)`.
- `uniform_list(id, count, |range, w, cx| Vec<impl IntoElement>)`.
- `cx.background_spawn(fut) -> Task<R>`, `cx.background_executor().timer(Duration)`.
- `cx.on_app_quit(|this, cx| async {})`. The quit can't be cancelled, so flush synchronously inside it.

## Runtime checks so far

| Check | macOS 15.8 | Windows | Fedora |
|---|---|---|---|
| Workspace builds (`cargo build --release`) | ✅ | ✅ CI | ✅ CI (fedora:latest container) |
| Hello window opens, no errors in log | ✅ | 🧪 needs a real machine | 🧪 needs a real machine (Vulkan) |
| `cargo test`, `clippy -D warnings` | ✅ | ✅ CI | ✅ CI |

## Live editor (Phase 10) findings

- **Reusing gpui-kit's key bindings:** giving a custom element the key context `"Input"` makes all of gpui-kit's text-input bindings dispatch to it (`gpui_kit::base::input::{Backspace, MoveLeft, Undo, Copy, Enter { secondary, shift }, ...}` and `gpui_kit::base::actions::{SelectLeft, SelectUp, ...}`). The element only needs `.on_action` handlers; per-OS keys come for free.
- **Height depends on width:** use `window.request_measured_layout(style, |known, available, window, cx| size)`. Share the shaped result with `prepaint` through an `Rc<RefCell<..>>` and reshape only if the final width differs.
- **Shaping:** `window.text_system().shape_text(text, font_size, &runs, Some(wrap_width), None)` returns one `WrappedLine` per `\n`-separated line (an empty string still gives one). `closest_index_for_position` returns `Result<usize, usize>`; both arms are usable indices. `position_for_index` gives the caret's top-left.
- **Backgrounds:** `WrappedLine::paint` doesn't draw `TextRun::background_color`; call `paint_background` first.
- **Check marks:** `PathBuilder::stroke(width)` + `move_to`/`line_to` + `build()` → `window.paint_path(path, color)`.
- **Clickable things in the markdown preview:** `TextView::markdown_block_parser` (gets `markdown_ast::Node` + source offsets) and `markdown_block_renderer(name, Fn + Send + Sync)`. Passing new closures every frame doesn't re-parse (gpui-kit compares the parser configuration). `window.dispatch_action` from a preview click doesn't reach the pane (nothing focused); capture a `WeakEntity` (it's `Send + Sync`) instead.
- **Test clicks** on elements with `("name", n)` ids work with `.test_support()` after `.id(..)`.
- **Local images in the preview:** the component `TextView` (`gpui_kit::component::text`) doesn't expose `image_source`; gpui-base's (`gpui_kit::base::TextView`) does, and the component one adds nothing else unless a text style is set. Without a resolver, a relative `![](x.png)` is treated as a URI and never loads.
- **Images in a custom element:** `window.use_asset::<ImgResourceLoader>(&Resource::Path(path.into()), cx)` returns `None` while loading and redraws the view when done; `RenderImage::size(0)` is in image pixels; `window.paint_image(bounds, bounds, corners, image, 0, false)` draws it.
- **File picker:** `cx.prompt_for_paths(PathPromptOptions { files, directories, multiple, prompt })` has no type filter, so `import_image` checks the extension.
