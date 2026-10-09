//! The quick switcher (`secondary-k`): a searchable list of every note in the vault.
//! Type to filter by name or path, use the arrow keys to choose, Enter to open,
//! Escape to close.

use std::ops::Range;
use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollStrategy, StatefulInteractiveElement as _, Styled as _,
    Subscription, UniformListScrollHandle, Window, div, prelude::FluentBuilder as _, px,
    uniform_list,
};
use kunotes_core::paths::{note_title, relative_path};
use kunotes_core::search::flatten_files;

use crate::actions::{QUICK_SWITCHER, SwitcherDown, SwitcherOpen, SwitcherUp};
use crate::vault_store::VaultStore;

const ROW_HEIGHT: f32 = 36.;
const MAX_VISIBLE_ROWS: usize = 9;

/// One note in the list.
struct Note {
    path: PathBuf,
    title: String,
    /// Parent folder name, shown on the right (empty for notes in the vault root).
    folder: String,
    /// Lowercased "title + path inside the vault", what the search matches against.
    search_text: String,
}

/// Opens the quick switcher. Does nothing if no vault is open.
pub fn open(vault: Entity<VaultStore>, window: &mut Window, cx: &mut App) {
    let notes = {
        let store = vault.read(cx);
        let (Some(root), Some(tree)) = (store.root(), store.tree()) else {
            return;
        };
        flatten_files(tree)
            .into_iter()
            .map(|node| {
                let title = note_title(&node.path);
                let relative = relative_path(root, &node.path).unwrap_or_default();
                Note {
                    search_text: format!("{title} {relative}").to_lowercase(),
                    folder: node
                        .path
                        .parent()
                        .filter(|parent| *parent != root)
                        .map(note_title)
                        .unwrap_or_default(),
                    title,
                    path: node.path.clone(),
                }
            })
            .collect()
    };

    let switcher = cx.new(|cx| QuickSwitcher::new(notes, vault, window, cx));
    let content = switcher.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .close_button(false)
            .p_0()
            .w(px(480.))
            .child(content.clone())
    });
    switcher.update(cx, |switcher, cx| {
        switcher
            .query
            .update(cx, |query, cx| query.focus(window, cx));
    });
}

pub struct QuickSwitcher {
    notes: Vec<Note>,
    /// Indexes into `notes` that match the query, in list order.
    matches: Vec<usize>,
    /// Position in `matches` of the highlighted row.
    selected: usize,
    query: Entity<InputState>,
    vault: Entity<VaultStore>,
    scroll_handle: UniformListScrollHandle,
    _subscription: Subscription,
}

impl QuickSwitcher {
    fn new(
        notes: Vec<Note>,
        vault: Entity<VaultStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes…"));
        let subscription = cx.subscribe(&query, |switcher, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                switcher.update_matches(cx);
            }
        });
        let matches = (0..notes.len()).collect();
        QuickSwitcher {
            notes,
            matches,
            selected: 0,
            query,
            vault,
            scroll_handle: UniformListScrollHandle::new(),
            _subscription: subscription,
        }
    }

    /// Keeps the notes whose title or path contains every typed word (any case).
    fn update_matches(&mut self, cx: &mut Context<Self>) {
        let query = self.query.read(cx).value().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        self.matches = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, note)| words.iter().all(|word| note.search_text.contains(word)))
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let last = self.matches.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.scroll_handle
            .scroll_to_item(self.selected, ScrollStrategy::Center);
        cx.notify();
    }

    fn select_up(&mut self, _: &SwitcherUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(-1, cx);
    }

    fn select_down(&mut self, _: &SwitcherDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(1, cx);
    }

    fn open_selected(&mut self, _: &SwitcherOpen, window: &mut Window, cx: &mut Context<Self>) {
        self.open_match(self.selected, window, cx);
    }

    /// Opens the note at `position` in the filtered list and closes the switcher.
    fn open_match(&mut self, position: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(note) = self.matches.get(position).map(|&index| &self.notes[index]) else {
            return;
        };
        let path = note.path.clone();
        self.vault.update(cx, |vault, cx| vault.open_note(path, cx));
        window.close_dialog(cx);
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = cx.theme();
        let (selected_bg, hover_bg, muted) =
            (theme.list_active, theme.list_hover, theme.muted_foreground);
        range
            .filter_map(|position| {
                let note = &self.notes[*self.matches.get(position)?];
                let is_selected = position == self.selected;
                Some(
                    h_flex()
                        .id(("switcher-row", position))
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .px_3()
                        .gap_3()
                        .rounded_md()
                        .when(is_selected, |row| row.bg(selected_bg))
                        .when(!is_selected, |row| row.hover(|row| row.bg(hover_bg)))
                        // A long title shrinks and ends with "…"; the folder keeps its size.
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(note.title.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_xs()
                                .text_color(muted)
                                .child(note.folder.to_uppercase()),
                        )
                        .on_click(cx.listener(move |switcher, _, window, cx| {
                            switcher.open_match(position, window, cx);
                        }))
                        .into_any_element(),
                )
            })
            .collect()
    }
}

impl Render for QuickSwitcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let visible_rows = self.matches.len().min(MAX_VISIBLE_ROWS);

        let list: AnyElement = if self.matches.is_empty() {
            div()
                .px_4()
                .py_6()
                .text_sm()
                .text_color(muted)
                .child("No matches")
                .into_any_element()
        } else {
            uniform_list(
                "switcher-list",
                self.matches.len(),
                cx.processor(Self::render_rows),
            )
            .track_scroll(&self.scroll_handle)
            .h(px(ROW_HEIGHT * visible_rows as f32))
            .into_any_element()
        };

        v_flex()
            .id("quick-switcher")
            .key_context(QUICK_SWITCHER)
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::open_selected))
            .w_full()
            .rounded(theme.radius_lg)
            .overflow_hidden()
            // Search row: equal space above and below, so the text sits centered.
            .child(
                h_flex()
                    .px_4()
                    .py_2p5()
                    .gap_2()
                    .border_b_1()
                    .border_color(border)
                    .child(Icon::new(IconName::Search).text_color(muted))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.query).appearance(false).p_0()),
                    ),
            )
            .child(div().p_1p5().child(list))
            .child(
                h_flex()
                    .gap_3()
                    .px_4()
                    .py_2()
                    .text_xs()
                    .text_color(muted)
                    .border_t_1()
                    .border_color(border)
                    .child("↑↓ move")
                    .child("↵ open")
                    .child("esc close"),
            )
    }
}
