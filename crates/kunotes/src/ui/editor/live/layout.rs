//! Lays out a note for the Live editor: each line is shaped on its own, with
//! its own font size (headings are bigger) and its markers hidden unless the
//! cursor is on it. Also answers "which text offset is at this point?" (clicks)
//! and "where is this offset drawn?" (the caret).
//!
//! A line that is only an image (`![alt](.img/cat.png)`) shows the picture;
//! on the cursor's line the markdown stays visible above it.
//!
//! Positions here are relative to the top-left corner of the text area.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use gpui_kit::{
    App, Bounds, ImgResourceLoader, Pixels, Point, RenderImage, Resource, SharedString, Size,
    TextRun, Window, WrappedLine, point, px, size,
};
use kunotes_core::live_view::{LineKind, LineView, empty_view, image_link, line_view, lines};
use kunotes_core::paths;

use super::style;

/// Body text size. Headings and code are sized relative to it.
pub const FONT_SIZE: f32 = 16.;

/// Everything needed to shape one line, decided before the width is known.
pub struct LineInput {
    /// The line's range in the note (without the line break).
    source: Range<usize>,
    view: LineView,
    revealed: bool,
    runs: Vec<TextRun>,
    font_size: Pixels,
    line_height: Pixels,
    /// Extra space above the line (headings get some breathing room).
    space_above: Pixels,
    /// Space left of the text, for a checkbox or a quote bar.
    text_left: Pixels,
    /// For an image line whose picture has loaded: the picture and its size.
    image: Option<(Arc<RenderImage>, Size<Pixels>)>,
}

/// Space between a picture and the next line.
const IMAGE_GAP: f32 = 8.;

/// One shaped line, ready to draw.
pub struct LaidLine {
    pub source: Range<usize>,
    pub kind: LineKind,
    /// True if all characters are drawn (the cursor is on the line).
    pub revealed: bool,
    /// What is drawn (see `kunotes_core::live_view`).
    view: LineView,
    pub shaped: WrappedLine,
    /// Top of the line's block, including `space_above`.
    pub top: Pixels,
    /// Top of the first row of text.
    pub text_top: Pixels,
    pub text_left: Pixels,
    pub line_height: Pixels,
    /// Height of all rows of text (a long line wraps onto several rows).
    pub text_height: Pixels,
    /// The picture of an image line and where it goes (below the text).
    pub image: Option<(Arc<RenderImage>, Bounds<Pixels>)>,
}

impl LaidLine {
    /// Bottom of the line's block.
    pub fn bottom(&self) -> Pixels {
        match &self.image {
            Some((_, bounds)) => bounds.bottom() + px(IMAGE_GAP),
            None => self.text_top + self.text_height,
        }
    }

    /// The checkbox of a task line (not drawn while the line is revealed).
    pub fn checkbox(&self) -> Option<Bounds<Pixels>> {
        if self.revealed || !matches!(self.kind, LineKind::Task { .. }) {
            return None;
        }
        let side = px(14.); // same as the Preview's checkbox
        let y = self.text_top + (self.line_height - side) / 2.;
        Some(Bounds::new(point(px(0.), y), size(side, side)))
    }

    /// Where `display` (an offset in the drawn text) is, relative to the text area.
    fn point_for_display(&self, display: usize) -> Point<Pixels> {
        let local = self
            .shaped
            .position_for_index(display, self.line_height)
            .unwrap_or_default();
        point(self.text_left + local.x, self.text_top + local.y)
    }
}

/// The whole note, laid out.
pub struct DocLayout {
    pub lines: Vec<LaidLine>,
    pub width: Pixels,
    pub height: Pixels,
}

/// Prepares every line of `text`. Lines touching `selection` are revealed.
/// Image links are relative to `note_dir`.
pub fn line_inputs(
    text: &str,
    selection: Range<usize>,
    note_dir: &Path,
    window: &mut Window,
    cx: &mut App,
) -> Vec<LineInput> {
    let colors = style::Colors::from_theme(cx);
    lines(text)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let revealed = line.range.start <= selection.end && selection.start <= line.range.end;
            let line_text = &text[line.range.clone()];
            let image = image_link(line_text, &line.spans)
                .and_then(|link| load_image(note_dir, &line_text[link], window, cx));
            let view = match &image {
                // The picture replaces the text, unless the cursor is on the line.
                Some(_) if !revealed => empty_view(LineKind::Image, line_text.len()),
                _ => line_view(line_text, &line.spans, revealed),
            };
            let base = px(FONT_SIZE);
            let (font_size, line_height, space_above) = match view.kind {
                LineKind::Heading(level) => {
                    let size = px(style::heading_size(level));
                    let above = if index == 0 { px(0.) } else { base * 0.6 };
                    (size, size * 1.4, above)
                }
                LineKind::CodeBlock | LineKind::Fence => (base * 0.875, base * 1.5, px(0.)),
                // An empty line is the gap between paragraphs: as tall as the
                // Preview's paragraph gap, not a full line.
                LineKind::Paragraph if line.range.is_empty() => (base, base, px(0.)),
                _ => (base, base * 1.6, px(0.)),
            };
            let text_left = match view.kind {
                LineKind::Task { .. } if !revealed => px(20.),
                LineKind::Quote => base * 1.2,
                LineKind::CodeBlock | LineKind::Fence => base * 0.75,
                _ => px(0.),
            };
            let runs = style::text_runs(&view, &colors);
            LineInput {
                source: line.range,
                view,
                revealed,
                runs,
                font_size,
                line_height,
                space_above,
                text_left,
                image,
            }
        })
        .collect()
}

/// The picture at `link` (relative to `note_dir`, or a web address) and its
/// size. `None` while it loads (GPUI redraws the editor once it has) or if it
/// can't be loaded; the line then shows its text like a link.
fn load_image(
    note_dir: &Path,
    link: &str,
    window: &mut Window,
    cx: &mut App,
) -> Option<(Arc<RenderImage>, Size<Pixels>)> {
    let resource = match paths::image_file(note_dir, link) {
        Some(file) => Resource::Path(file.into()),
        None => Resource::Uri(link.to_string().into()),
    };
    let image = window.use_asset::<ImgResourceLoader>(&resource, cx)?.ok()?;
    // One image pixel per point, like the Preview.
    let pixels = image.size(0);
    let natural = size(px(pixels.width.0 as f32), px(pixels.height.0 as f32));
    Some((image, natural))
}

/// Shapes every line to fit `width`.
pub fn build(inputs: &[LineInput], width: Pixels, window: &mut Window) -> DocLayout {
    let mut lines = Vec::with_capacity(inputs.len());
    let mut y = px(0.);
    for input in inputs {
        let wrap_width = (width - input.text_left).max(px(1.));
        let shaped = window
            .text_system()
            .shape_text(
                SharedString::from(input.view.text.clone()),
                input.font_size,
                &input.runs,
                Some(wrap_width),
                None,
            )
            .ok()
            .and_then(|mut shaped| shaped.pop())
            .unwrap_or_default();
        let rows = shaped.wrap_boundaries().len() + 1;
        let shows_text = !(input.image.is_some() && input.view.text.is_empty());
        let text_height = if shows_text {
            input.line_height * rows as f32
        } else {
            px(0.)
        };
        let top = y;
        let text_top = top + input.space_above;
        // A picture goes below the text, shrunk to fit the width if needed.
        let image = input.image.as_ref().map(|(picture, natural)| {
            let max_width = (width - input.text_left).max(px(1.));
            let scale = (max_width / natural.width).min(1.);
            let shown = size(natural.width * scale, natural.height * scale);
            let origin = point(input.text_left, text_top + text_height);
            (picture.clone(), Bounds::new(origin, shown))
        });
        y = match &image {
            Some((_, bounds)) => bounds.bottom() + px(IMAGE_GAP),
            None => text_top + text_height,
        };
        lines.push(LaidLine {
            source: input.source.clone(),
            kind: input.view.kind,
            revealed: input.revealed,
            view: input.view.clone(),
            shaped,
            top,
            text_top,
            text_left: input.text_left,
            line_height: input.line_height,
            text_height,
            image,
        });
    }
    DocLayout {
        lines,
        width,
        height: y,
    }
}

impl DocLayout {
    /// The index of the line containing `offset`.
    fn line_index(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|line| line.source.start <= offset)
            .saturating_sub(1)
    }

    /// The caret position for `offset`: its top-left corner and its height.
    pub fn caret(&self, offset: usize) -> Option<(Point<Pixels>, Pixels)> {
        let line = self.lines.get(self.line_index(offset))?;
        let display = line
            .view
            .to_display(offset.saturating_sub(line.source.start));
        Some((line.point_for_display(display), line.line_height))
    }

    /// The text offset closest to `position` (used for clicks and Up/Down).
    pub fn offset_for_point(&self, position: Point<Pixels>) -> usize {
        let Some(last) = self.lines.last() else {
            return 0;
        };
        if position.y >= last.bottom() {
            return last.source.end;
        }
        let index = self
            .lines
            .partition_point(|line| line.bottom() <= position.y)
            .min(self.lines.len() - 1);
        let line = &self.lines[index];
        // Clicks in the space above a heading land on its first row.
        let local = point(
            position.x - line.text_left,
            (position.y - line.text_top).max(px(0.)),
        );
        let display = match line
            .shaped
            .closest_index_for_position(local, line.line_height)
        {
            Ok(index) | Err(index) => index.min(line.view.text.len()),
        };
        line.source.start + line.view.to_source(display)
    }

    /// The offset one row above or below `offset`, keeping the caret near `x`.
    /// Returns the start or end of the note when there is no row to go to.
    pub fn offset_above_or_below(&self, offset: usize, x: Pixels, down: bool) -> usize {
        let Some((caret, height)) = self.caret(offset) else {
            return 0;
        };
        let y = if down {
            caret.y + height + px(1.)
        } else {
            caret.y - px(1.)
        };
        if y < px(0.) {
            return 0;
        }
        self.offset_for_point(point(x, y))
    }

    /// The task line whose checkbox is at `position`, as an offset on that line.
    pub fn checkbox_at(&self, position: Point<Pixels>) -> Option<usize> {
        self.lines.iter().find_map(|line| {
            let checkbox = line.checkbox()?;
            // A little extra room makes the box easier to hit.
            let hit = Bounds::new(
                checkbox.origin - point(px(3.), px(3.)),
                checkbox.size + size(px(6.), px(6.)),
            );
            hit.contains(&position).then_some(line.source.start)
        })
    }

    /// Rectangles covering `range` (the selection), one per row of text.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        if range.is_empty() {
            return rects;
        }
        for line in &self.lines {
            if line.source.end < range.start || line.source.start > range.end {
                continue;
            }
            let start = line
                .view
                .to_display(range.start.max(line.source.start) - line.source.start);
            let end = line
                .view
                .to_display(range.end.min(line.source.end) - line.source.start);
            let from = line.point_for_display(start);
            let to = line.point_for_display(end);
            // A selected line break shows as a little extra width.
            let past_end = if range.end > line.source.end {
                px(6.)
            } else {
                px(0.)
            };
            let mut row_top = from.y;
            while row_top <= to.y {
                let left = if row_top == from.y {
                    from.x
                } else {
                    line.text_left
                };
                let right = if row_top == to.y {
                    to.x + past_end
                } else {
                    self.width
                };
                if right > left {
                    rects.push(Bounds::new(
                        point(left, row_top),
                        size(right - left, line.line_height),
                    ));
                }
                row_top += line.line_height;
            }
        }
        rects
    }
}
