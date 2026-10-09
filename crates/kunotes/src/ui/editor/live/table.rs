//! Tables in the Live editor. While the cursor is outside a table, it's drawn
//! as a grid like the Preview's: bordered cells, a bold header on a shaded
//! row, and no delimiter row. With the cursor inside, the table shows its
//! markdown like any other line.
//!
//! Each row is still one line of the note; this file lays out and draws the
//! cells of one row. Columns line up across the rows of a table because
//! `column_widths` measures every row first.

use std::collections::HashMap;
use std::ops::Range;

use gpui_kit::{
    App, BorderStyle, Bounds, FontWeight, Pixels, Point, SharedString, TextAlign, TextRun, Window,
    WrappedLine, fill, outline, point, px, size,
};
use kunotes_core::live::Span;
use kunotes_core::live_table;
use kunotes_core::live_view::{LineView, line_view};

use super::layout::FONT_SIZE;
use super::style::{self, Colors};

/// Space inside a cell, left/right and top/bottom.
const PAD_X: f32 = 10.;
const PAD_Y: f32 = 6.;

/// One cell, before the column width is known.
pub struct CellInput {
    /// The cell's text in the line (without the `|`s and spaces around it).
    source: Range<usize>,
    view: LineView,
    runs: Vec<TextRun>,
    /// Width of the text on one line, for sizing the column.
    natural_width: Pixels,
}

/// One row of a table that is drawn as a grid.
pub struct RowInput {
    /// Which table of the note this row belongs to.
    pub table: usize,
    /// The `| --- |` row, which isn't drawn.
    delimiter: bool,
    header: bool,
    cells: Vec<CellInput>,
}

/// One laid-out cell. Positions are relative to the text area.
pub struct LaidCell {
    source: Range<usize>,
    view: LineView,
    shaped: WrappedLine,
    /// The cell's box (borders are drawn on its edges).
    bounds: Bounds<Pixels>,
}

/// One laid-out row.
pub struct LaidRow {
    header: bool,
    cells: Vec<LaidCell>,
}

/// Prepares the cells of a table row. `spans` are the line's styled parts.
pub fn row_input(
    table: usize,
    row_index: usize,
    line_text: &str,
    spans: &[Span],
    colors: &Colors,
    window: &mut Window,
) -> RowInput {
    let header = row_index == 0;
    let delimiter = row_index == 1;
    let cells = live_table::cells(line_text)
        .into_iter()
        .map(|source| {
            // The cell's own styled parts, relative to the cell.
            let cell_spans: Vec<Span> = spans
                .iter()
                .filter(|span| span.range.start >= source.start && span.range.end <= source.end)
                .map(|span| Span {
                    range: span.range.start - source.start..span.range.end - source.start,
                    kind: span.kind,
                })
                .collect();
            let view = line_view(&line_text[source.clone()], &cell_spans, false);
            let mut runs = style::text_runs(&view, colors);
            if header {
                for run in &mut runs {
                    run.font.weight = FontWeight::SEMIBOLD;
                }
            }
            let natural_width = window
                .text_system()
                .shape_line(
                    SharedString::from(view.text.clone()),
                    px(FONT_SIZE),
                    &runs,
                    None,
                )
                .width;
            CellInput {
                source,
                view,
                runs,
                natural_width,
            }
        })
        .collect();
    RowInput {
        table,
        delimiter,
        header,
        cells,
    }
}

/// Column widths for every table, so its rows line up. Columns get the
/// width of their widest cell, shrunk evenly when the table is too wide.
pub fn column_widths<'a>(
    rows: impl Iterator<Item = &'a RowInput>,
    width: Pixels,
) -> HashMap<usize, Vec<Pixels>> {
    let mut tables: HashMap<usize, Vec<Pixels>> = HashMap::new();
    for row in rows.filter(|row| !row.delimiter) {
        let columns = tables.entry(row.table).or_default();
        for (index, cell) in row.cells.iter().enumerate() {
            let wanted = cell.natural_width + px(PAD_X * 2.);
            match columns.get_mut(index) {
                Some(column) => *column = (*column).max(wanted),
                None => columns.push(wanted),
            }
        }
    }
    for columns in tables.values_mut() {
        let total: Pixels = columns.iter().copied().fold(px(0.), |sum, w| sum + w);
        if total > width {
            let scale = width / total;
            for column in columns.iter_mut() {
                *column = (*column * scale).max(px(PAD_X * 3.));
            }
        }
    }
    tables
}

/// Lays out one row at `top`; returns the row and its height.
pub fn lay_out_row(
    row: &RowInput,
    columns: &[Pixels],
    top: Pixels,
    line_height: Pixels,
    window: &mut Window,
) -> (LaidRow, Pixels) {
    if row.delimiter {
        let empty = LaidRow {
            header: false,
            cells: Vec::new(),
        };
        return (empty, px(0.));
    }
    let mut x = px(0.);
    let mut shaped_cells = Vec::new();
    let mut tallest = 1;
    for (index, cell) in row.cells.iter().enumerate() {
        let column = columns.get(index).copied().unwrap_or(px(PAD_X * 3.));
        let shaped = window
            .text_system()
            .shape_text(
                SharedString::from(cell.view.text.clone()),
                px(FONT_SIZE),
                &cell.runs,
                Some((column - px(PAD_X * 2.)).max(px(1.))),
                None,
            )
            .ok()
            .and_then(|mut lines| lines.pop())
            .unwrap_or_default();
        tallest = tallest.max(shaped.wrap_boundaries().len() + 1);
        shaped_cells.push((cell, shaped, x, column));
        x += column;
    }
    let height = line_height * tallest as f32 + px(PAD_Y * 2.);
    let cells = shaped_cells
        .into_iter()
        .map(|(cell, shaped, x, column)| LaidCell {
            source: cell.source.clone(),
            view: cell.view.clone(),
            shaped,
            bounds: Bounds::new(point(x, top), size(column, height)),
        })
        .collect();
    let laid = LaidRow {
        header: row.header,
        cells,
    };
    (laid, height)
}

impl LaidRow {
    /// The offset in the line under `position` (relative to the text area):
    /// the closest character of the cell under it.
    pub fn offset_at(&self, position: Point<Pixels>, line_height: Pixels) -> Option<usize> {
        let cell = self
            .cells
            .iter()
            .find(|cell| position.x < cell.bounds.right())
            .or(self.cells.last())?;
        let local = point(
            position.x - cell.bounds.left() - px(PAD_X),
            (position.y - cell.bounds.top() - px(PAD_Y)).max(px(0.)),
        );
        let display = match cell.shaped.closest_index_for_position(local, line_height) {
            Ok(index) | Err(index) => index.min(cell.view.text.len()),
        };
        Some(cell.source.start + cell.view.to_source(display))
    }

    /// Draws the row: header shading, cell borders, and the text.
    pub fn paint(
        &self,
        origin: Point<Pixels>,
        line_height: Pixels,
        colors: &Colors,
        window: &mut Window,
        cx: &mut App,
    ) {
        for cell in &self.cells {
            let bounds = cell.bounds + origin;
            if self.header {
                window.paint_quad(fill(bounds, colors.code_background));
            }
            window.paint_quad(outline(bounds, colors.border, BorderStyle::Solid));
            let text_origin = bounds.origin + point(px(PAD_X), px(PAD_Y));
            if let Err(error) =
                cell.shaped
                    .paint(text_origin, line_height, TextAlign::Left, None, window, cx)
            {
                log::warn!("Couldn't draw a table cell: {error}");
            }
        }
    }
}
