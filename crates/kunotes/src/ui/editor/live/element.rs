//! The GPUI element that draws the Live editor: backgrounds, checkboxes,
//! the selection, the text, and the caret. It also registers the editor as
//! the window's text input target, so typing and IME go to `LiveEditor`.
//!
//! An element goes through three steps every frame:
//! 1. `request_layout`: tell the layout engine how big we are. Our height
//!    depends on the width (long lines wrap), so we use a *measured* layout:
//!    a closure the engine calls once it knows the width.
//! 2. `prepaint`: lay out the text for the final bounds.
//! 3. `paint`: draw it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{
    App, AvailableSpace, BorderStyle, Bounds, Corners, Element, ElementId, ElementInputHandler,
    Entity, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Size, Style,
    TextAlign, TransformationMatrix, Window, fill, outline, point, px, relative, size,
};
use kunotes_core::live_view::LineKind;

use super::LiveEditor;
use super::layout::{self, DocLayout, FONT_SIZE, LineInput};
use super::style::Colors;

/// gpui-kit's check icon (embedded in the app with its default icons).
const CHECK_ICON: &str = "icons/check.svg";

/// Shown in an empty note.
const PLACEHOLDER: &str = "Start writing…";

pub struct LiveElement {
    editor: Entity<LiveEditor>,
}

impl LiveElement {
    pub fn new(editor: Entity<LiveEditor>) -> Self {
        LiveElement { editor }
    }
}

impl IntoElement for LiveElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Shared between the measure closure and `prepaint`, so the text is shaped once.
type LayoutCell = Rc<RefCell<Option<DocLayout>>>;

pub struct RequestLayoutState {
    inputs: Rc<Vec<LineInput>>,
    cell: LayoutCell,
}

impl Element for LiveElement {
    type RequestLayoutState = RequestLayoutState;
    type PrepaintState = Option<DocLayout>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let editor = self.editor.read(cx);
        let (text, selection) = (editor.buffer.text().to_string(), editor.buffer.selection());
        let note_dir = editor.note_dir.clone();
        let inputs = Rc::new(layout::line_inputs(&text, selection, &note_dir, window, cx));
        let cell: LayoutCell = Rc::default();

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let measure_inputs = inputs.clone();
        let measure_cell = cell.clone();
        let layout_id = window.request_measured_layout(
            style,
            move |known: Size<Option<Pixels>>, available: Size<AvailableSpace>, window, _cx| {
                let width = known.width.unwrap_or(match available.width {
                    AvailableSpace::Definite(width) => width,
                    _ => px(600.),
                });
                let doc = layout::build(&measure_inputs, width, window);
                let height = doc.height;
                *measure_cell.borrow_mut() = Some(doc);
                size(width, height)
            },
        );
        (layout_id, RequestLayoutState { inputs, cell })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // Reuse what the measure step shaped, unless it was for another width.
        let doc = match state.cell.borrow_mut().take() {
            Some(doc) if doc.width == bounds.size.width => doc,
            _ => layout::build(&state.inputs, bounds.size.width, window),
        };
        self.editor
            .update(cx, |editor, _| editor.reveal_cursor(&doc, bounds));
        Some(doc)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        doc: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(doc) = doc.take() else {
            return;
        };
        let editor = self.editor.read(cx);
        let focus_handle = editor.focus_handle.clone();
        let selection = editor.buffer.selection();
        let cursor = editor.buffer.cursor();
        let is_empty = editor.buffer.text().is_empty();
        let colors = Colors::from_theme(cx);
        let origin = bounds.origin;

        // Typing and IME input go to the editor while it has focus.
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        // Only draw lines that are on screen.
        let visible = window.content_mask().bounds;
        for line in &doc.lines {
            let top = origin.y + line.top;
            let bottom = origin.y + line.bottom();
            if bottom < visible.top() || top > visible.bottom() {
                continue;
            }
            paint_decorations(line, &doc, origin, &colors, window, cx);
        }

        for rect in doc.selection_rects(selection.clone()) {
            window.paint_quad(fill(rect + origin, colors.selection));
        }

        for line in &doc.lines {
            let top = origin.y + line.top;
            if origin.y + line.bottom() < visible.top() || top > visible.bottom() {
                continue;
            }
            if let Some(row) = &line.table_row {
                row.paint(origin, line.line_height, &colors, window, cx);
                continue;
            }
            let text_origin = point(origin.x + line.text_left, origin.y + line.text_top);
            // Backgrounds (inline code) first, then the text on top.
            if let Err(error) = line.shaped.paint_background(
                text_origin,
                line.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                log::warn!("Couldn't draw a line's background: {error}");
            }
            if let Err(error) = line.shaped.paint(
                text_origin,
                line.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                log::warn!("Couldn't draw a line: {error}");
            }
        }

        // Pictures of image lines.
        for line in &doc.lines {
            let Some((picture, bounds)) = &line.image else {
                continue;
            };
            let bounds = *bounds + origin;
            if bounds.bottom() < visible.top() || bounds.top() > visible.bottom() {
                continue;
            }
            let corners = Corners::all(px(4.));
            if let Err(error) =
                window.paint_image(bounds, bounds, corners, picture.clone(), 0, false)
            {
                log::warn!("Couldn't draw an image: {error}");
            }
        }

        if is_empty {
            paint_placeholder(origin, &colors, window, cx);
        }

        if focus_handle.is_focused(window)
            && selection.is_empty()
            && let Some((caret, height)) = doc.caret(cursor)
        {
            window.paint_quad(fill(
                Bounds::new(caret + origin, size(px(2.), height)),
                colors.caret,
            ));
        }

        self.editor.update(cx, |editor, _| {
            editor.last_layout = Some(doc);
            editor.last_bounds = Some(bounds);
        });
    }
}

/// Code backgrounds, quote bars, rules and checkboxes behind a line.
fn paint_decorations(
    line: &layout::LaidLine,
    doc: &DocLayout,
    origin: gpui_kit::Point<Pixels>,
    colors: &Colors,
    window: &mut Window,
    cx: &App,
) {
    let text_top = origin.y + line.text_top;
    match line.kind {
        LineKind::CodeBlock | LineKind::Fence => {
            window.paint_quad(fill(
                Bounds::new(point(origin.x, text_top), size(doc.width, line.text_height)),
                colors.code_background,
            ));
        }
        LineKind::Quote => {
            window.paint_quad(fill(
                Bounds::new(point(origin.x, text_top), size(px(3.), line.text_height)),
                colors.border,
            ));
        }
        LineKind::Rule if !line.revealed => {
            let y = text_top + line.line_height / 2.;
            window.paint_quad(fill(
                Bounds::new(point(origin.x, y), size(doc.width, px(1.))),
                colors.border,
            ));
        }
        LineKind::Task { checked } => {
            if let Some(checkbox) = line.checkbox() {
                paint_checkbox(checkbox + origin, checked, colors, window, cx);
            }
        }
        _ => {}
    }
}

fn paint_checkbox(
    bounds: Bounds<Pixels>,
    checked: bool,
    colors: &Colors,
    window: &mut Window,
    cx: &App,
) {
    let radius = px(1.);
    if checked {
        window.paint_quad(fill(bounds, colors.accent).corner_radii(radius));
        // The check mark is an icon (a sprite), not a drawn path: GPUI keeps a
        // window-sized GPU texture around once any path has been drawn.
        let inset = bounds.size.width * 0.15;
        let icon = Bounds::new(
            bounds.origin + point(inset, inset),
            bounds.size - size(inset * 2., inset * 2.),
        );
        if let Err(error) = window.paint_svg(
            icon,
            CHECK_ICON.into(),
            None,
            TransformationMatrix::unit(),
            colors.on_accent,
            cx,
        ) {
            log::warn!("Couldn't draw a check mark: {error}");
        }
    } else {
        window.paint_quad(outline(bounds, colors.accent, BorderStyle::Solid).corner_radii(radius));
    }
}

fn paint_placeholder(
    origin: gpui_kit::Point<Pixels>,
    colors: &Colors,
    window: &mut Window,
    cx: &mut App,
) {
    let run = gpui_kit::TextRun {
        len: PLACEHOLDER.len(),
        font: window.text_style().font(),
        color: colors.muted,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(PLACEHOLDER.into(), px(FONT_SIZE), &[run], None);
    if let Err(error) = line.paint(
        origin,
        px(FONT_SIZE * 1.6),
        TextAlign::Left,
        None,
        window,
        cx,
    ) {
        log::warn!("Couldn't draw the placeholder: {error}");
    }
}
