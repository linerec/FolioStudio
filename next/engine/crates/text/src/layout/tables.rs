//! Tables in the text flow.
//!
//! InDesign sets a table on a line of its own (InDesign 2025, scratch experiments exp1/exp2,
//! Feature Atlas p. 30–31):
//! - The table's outer edge (grid lines plus half the border weight) starts at the column
//!   top in a new column (space before ignored there), else `space before` below the previous
//!   baseline (plus the paragraphs' space after / before). The text after it has its first
//!   baseline `space after` + its leading below the outer bottom; a following table starts
//!   `space before` below it (the first table's space after does not apply).
//! - Horizontally the outer edge sits at the paragraph's left indent, centred or flush right
//!   by the paragraph's alignment.
//! - A row is its cells' top inset + depth of their text to the last baseline + bottom
//!   inset ("at least" rows: no less than the minimum height; "exactly" rows: the stored
//!   height). A cell spanning rows grows its last row. Cell text sits at the insets, shifted
//!   down for centre / bottom alignment over that depth.
//! - Rows that do not fit move to the next column or frame, which repeats the header rows;
//!   each piece ends with the footer rows. Rows tied by a merged cell or "keep with next"
//!   move together.
//! - Fills cover the grid boxes; then the edge strokes centred on the grid lines (a shared
//!   edge takes the higher stroke priority), horizontal strokes extended over the vertical
//!   ones, heavier strokes on top; then the cell text.

use super::strokes::{self, Ends};
use super::structures::{TextState, segment_shape, translate_line};
use super::*;
use folio_model::{CellStroke, Table};

/// Characters `[from, to)` of a paragraph as a paragraph of their own; with `last` the
/// empty run carrying the paragraph return's attributes is kept.
pub(super) fn slice_paragraph(p: &Paragraph, from: usize, to: usize, last: bool) -> Paragraph {
    let mut out = Paragraph { style: p.style.clone(), attrs: p.attrs.clone(), runs: Vec::new() };
    let mut at = 0usize;
    for r in &p.runs {
        let n = r.text.chars().count();
        let (a, b) = (at, at + n);
        at = b;
        let (lo, hi) = (from.max(a), to.min(b));
        let keep_empty = n == 0 && last && a >= from;
        if lo >= hi && !keep_empty {
            continue;
        }
        let mut run = TextRun { attrs: r.attrs.clone(), variable: r.variable, ..TextRun::default() };
        if lo < hi {
            let chars: Vec<char> = r.text.chars().collect();
            let skip = chars[..lo - a].iter().filter(|&&c| c == '\u{FFFC}').count();
            run.text = chars[lo - a..hi - a].iter().collect();
            let k = run.text.chars().filter(|&c| c == '\u{FFFC}').count();
            run.inline = r.inline.iter().skip(skip).take(k).cloned().collect();
            run.note = r.note.clone();
            run.named = r.named.clone();
        }
        out.runs.push(run);
    }
    out
}

impl<'c, 'a> Ctx<'c, 'a> {
    /// Lays out a table from the flow's position. `para_gap` is the paragraph's space before
    /// (the table starts its paragraph); `index` the table character's story index; `pi`
    /// the paragraph. False when rows are left over (overset).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layout_table(
        &self,
        flow: &mut Flow<'_>,
        t: &Table,
        a: &ParagraphAttrs,
        para_gap: f64,
        index: usize,
        pi: usize,
        st: &mut TextState,
    ) -> bool {
        let (nrows, ncols) = (t.rows.len(), t.columns.len());
        if nrows == 0 || ncols == 0 {
            return true;
        }
        let mut xs = vec![0.0; ncols + 1];
        for c in 0..ncols {
            xs[c + 1] = xs[c] + t.columns[c];
        }
        let mut owner = vec![vec![usize::MAX; ncols]; nrows];
        let mut starts: Vec<Vec<usize>> = vec![Vec::new(); nrows];
        for (k, c) in t.cells.iter().enumerate() {
            if c.row >= nrows || c.column >= ncols {
                continue;
            }
            starts[c.row].push(k);
            for row in owner.iter_mut().take((c.row + c.row_span.max(1)).min(nrows)).skip(c.row) {
                for o in row.iter_mut().take((c.column + c.column_span.max(1)).min(ncols)).skip(c.column) {
                    *o = k;
                }
            }
        }
        // Cell text at the cell's width.
        let texts: Vec<(Vec<FlowLine>, f64)> = t
            .cells
            .iter()
            .map(|c| {
                let span_end = (c.column + c.column_span.max(1)).min(ncols);
                let w = xs[span_end] - xs[c.column.min(ncols)] - c.insets[1] - c.insets[3];
                let frame = TextFrame {
                    first_baseline: c.first_baseline,
                    min_first_baseline_offset: c.min_first_baseline_offset,
                    columns: 1,
                    ..TextFrame::default()
                };
                self.lay_nested(&c.paragraphs, w, &frame, st, index, &c.id)
            })
            .collect();
        let h = row_heights(t, &texts);
        let needs = row_needs(t, &texts);
        st.table_rows.insert(t.id.clone(), h.clone());
        // Rotated cells (90°, 270°) set their lines along the cell's height, once the rows are
        // known; a turned cell does not size its row.
        let turned: Vec<Option<(Vec<FlowLine>, f64)>> = t
            .cells
            .iter()
            .map(|c| {
                if !quarter_turn(cell_rotation(c)) {
                    return None;
                }
                let last = (c.row + c.row_span.max(1)).min(nrows);
                let room = h[c.row.min(nrows)..last].iter().sum::<f64>() - c.insets[0] - c.insets[2];
                let frame = TextFrame {
                    first_baseline: c.first_baseline,
                    min_first_baseline_offset: c.min_first_baseline_offset,
                    columns: 1,
                    ..TextFrame::default()
                };
                Some(self.lay_nested(&c.paragraphs, room, &frame, st, index, &c.id))
            })
            .collect();

        // Unbreakable runs of body rows.
        let (hr, fr) = (t.header_rows.min(nrows), t.footer_rows.min(nrows.saturating_sub(t.header_rows)));
        let body_end = nrows - fr;
        let mut blocks: Vec<(usize, usize)> = Vec::new();
        let mut r = hr;
        while r < body_end {
            let mut end = r + 1;
            let mut k = r;
            while k < end {
                for &ci in &starts[k] {
                    end = end.max((t.cells[ci].row + t.cells[ci].row_span).min(body_end));
                }
                if t.rows[k].keep_with_next && k + 1 < body_end {
                    end = end.max(k + 2);
                }
                k += 1;
            }
            blocks.push((r, end));
            r = end;
        }
        let header_h: f64 = h[..hr].iter().sum();
        let footer_h: f64 = h[body_end..].iter().sum();
        // The table sits inside its outer strokes: the outer cells' edges as drawn (they hold
        // the table border unless a cell sets its own), not the `Table` element's border
        // attributes (InDesign 2025: outer edges of 2, 3 and 6 pt cell strokes all start at
        // the frame's inner edge; artifacts/next-20261018/fonts/indesign/text-strokes-x4.pdf).
        let outer = |side: usize, on_edge: &dyn Fn(&folio_model::TableCell) -> bool| {
            t.cells
                .iter()
                .filter(|c| on_edge(c))
                .map(|c| &c.edges[side])
                .filter(|s| s.weight > 0.0 && s.paint != Paint::None)
                .map(|s| s.weight)
                .fold(None, |m: Option<f64>, w| Some(m.map_or(w, |m| m.max(w))))
        };
        let [bt, bl, bb, br] = t.border_weights;
        let bt = outer(0, &|c| c.row == 0).unwrap_or(bt);
        let bl = outer(1, &|c| c.column == 0).unwrap_or(bl);
        let bb = outer(2, &|c| c.row + c.row_span.max(1) >= nrows).unwrap_or(bb);
        let br = outer(3, &|c| c.column + c.column_span.max(1) >= ncols).unwrap_or(br);
        // Header, body and footer are set as blocks whose joining strokes meet at their inner
        // edges: the body's first row starts (header's bottom − its own top stroke) / 2 below
        // the header's bottom grid line, so a heavier header stroke pushes it down and a heavier
        // body top stroke pulls it up (tbl-cell-stroke-header: +1 pt for 3 pt over 1 pt;
        // tbl-cell-stroke: −1 pt for 1 pt over 3 pt; rows inside the body do not move). The
        // footer joins the body the same way (mirrored, not measured).
        let heaviest = |side: usize, on: &dyn Fn(&folio_model::TableCell) -> bool| outer(side, on).unwrap_or(0.0);
        let head_join = |first: usize| if hr == 0 { 0.0 } else {
            (heaviest(2, &|c| c.row + c.row_span.max(1) == hr) - heaviest(0, &|c| c.row == first)) / 2.0
        };
        let foot_join = |last: usize| if fr == 0 { 0.0 } else {
            (heaviest(0, &|c| c.row == body_end) - heaviest(2, &|c| c.row + c.row_span.max(1) == last + 1)) / 2.0
        };
        let align = match a.justification {
            Justification::CenterAlign | Justification::CenterJustified => 0.5,
            Justification::RightAlign | Justification::RightJustified => 1.0,
            _ => 0.0,
        };

        let mut bi = 0usize;
        let mut first_piece = true;
        // The keep groups see a header row where it first stands; a later piece repeats it (a copy, not another row).
        let mut header_placed = false;
        let keep_start = flow.keep.table(pi, nrows);
        loop {
            // The table's first piece starts with its first row (the header row when there is one).
            let row = if bi == 0 { 0 } else { blocks.get(bi).map_or(0, |b| b.0) };
            if !flow.keep_target((pi, keep_start + row)) { return false; }
            let Some(c) = flow.column() else { return false };
            let top_outer = match flow.last {
                None => flow.region_top.unwrap_or(c.top),
                Some(prev) => prev + if first_piece { flow.pending_after + para_gap } else { 0.0 } + t.space_before,
            };
            let grid_top = top_outer + bt / 2.0;
            let show_header = hr > 0 && !(first_piece && t.skip_first_header);
            let limit = flow.text_bottom(&c);
            let start_bi = bi;
            let join = match blocks.get(start_bi) {
                Some(&(first, _)) if show_header => head_join(first),
                _ => 0.0,
            };
            let mut y = grid_top + if show_header { header_h + join } else { 0.0 };
            // Start Row (Cell Options ▸ Keep Options, IDML Row/@StartRow): a row that does not start Anywhere begins the next piece of the table,
            // in the next column (NextColumn), the next frame (NextFrame) or the first frame on the next (odd, even) page
            let mut start_break: Option<&str> = None;
            while bi < blocks.len() {
                let (r0, r1) = blocks[bi];
                // (a row kept with the next one goes along with it: the whole unbreakable run moves)
                let starts_row = (r0..r1).filter_map(|r| t.rows.get(r)).map(|row| row.start.as_str()).find(|s| !s.is_empty() && *s != "Anywhere");
                if bi > start_bi && let Some(s) = starts_row {
                    start_break = Some(s);
                    break;
                }
                if (r0..r1).any(|r| flow.keep.targets.get(&(pi, keep_start + r)).is_some_and(|&target| target > flow.keep_area(flow.fi, flow.ci))) {
                    break;
                }
                let bh: f64 = h[r0..r1].iter().sum();
                let last_block = bi + 1 == blocks.len();
                let foot = if fr > 0 && !(last_block && t.skip_last_footer) { footer_h + foot_join(r1 - 1) } else { 0.0 };
                if y + bh + foot + bb / 2.0 > limit + EPS {
                    break;
                }
                y += bh;
                bi += 1;
            }
            let done = bi == blocks.len();
            if bi == start_bi && !done {
                // Not one row fits here: the next column or frame.
                if !flow.advance() {
                    return false;
                }
                continue;
            }
            let show_footer = fr > 0 && !(done && t.skip_last_footer);
            let mut seq: Vec<(usize, f64)> = Vec::new();
            let mut yy = grid_top;
            let push_rows = |rows: std::ops::Range<usize>, seq: &mut Vec<(usize, f64)>, yy: &mut f64| {
                for r in rows {
                    seq.push((r, *yy));
                    *yy += h[r];
                }
            };
            let body = &blocks[start_bi..bi];
            if show_header {
                push_rows(0..hr, &mut seq, &mut yy);
                if let Some(&(first, _)) = body.first() {
                    yy += head_join(first);
                }
            }
            for &(r0, r1) in body {
                push_rows(r0..r1, &mut seq, &mut yy);
            }
            if show_footer {
                if let Some(&(_, end)) = body.last() {
                    yy += foot_join(end - 1);
                }
                push_rows(body_end..nrows, &mut seq, &mut yy);
            }
            let outer_bottom = yy + bb / 2.0;
            let verso = flow.frame().is_some_and(|f| f.verso);
            let align = match a.justification {
                Justification::ToBindingSide => if verso { 1.0 } else { 0.0 },
                Justification::AwayFromBindingSide => if verso { 0.0 } else { 1.0 },
                _ => align,
            };
            let outer_w = xs[ncols] + (bl + br) / 2.0;
            let room = (c.width() - a.left_indent - a.right_indent - outer_w).max(0.0);
            let x_left = c.left + a.left_indent + room * align + bl / 2.0;
            let gx: Vec<f64> = xs.iter().map(|x| x + x_left).collect();
            let (fi, ci) = (flow.fi, flow.ci);

            let shapes = piece_shapes(t, &owner, &seq, &h, &gx);
            let (left, right) = (gx[0] - bl / 2.0, gx[ncols] + br / 2.0);
            let grid = folio_model::layout::TableGrid { table: t.id.clone(), columns: gx.clone(), rows: seq.iter().map(|&(r, y)| (r, y, y + h[r])).collect(), content_min: seq.iter().map(|&(r, _)| needs[r]).collect() };
            let line = LaidLine { baseline: outer_bottom, left, right, start: index, end: index + 1, shapes, table: Some(grid),
                text_clusters: vec![folio_model::layout::TextCluster { start: index, end: index + 1,
                    x0: left, x1: right, top: top_outer, bottom: outer_bottom, path_map: None }],
                ..LaidLine::default() };
            let depth = outer_bottom - top_outer;
            let m = LineMetrics { ascent: depth, cap: depth, x_height: depth, embox: depth, leading: depth, descent: 0.0, ..LineMetrics::default() };
            if flow.keep.active {
                let area = flow.keep_area(fi, ci);
                let empty = !flow.frames[fi].lines.iter().any(|l| flow.keep_area(fi, l.col) == area);
                for (i, &(r, _)) in seq.iter().enumerate() {
                    if r < hr && header_placed { continue; }
                    flow.keep.placed.push(((pi, keep_start + r), area, empty && i == 0));
                }
                header_placed |= show_header;
            }
            flow.frames[fi].lines.push(FlowLine { col: ci, line, para: Some(pi), m, grid: None, text: (left, right) });
            for (k, cell) in t.cells.iter().enumerate() {
                let Some((y0, y1)) = cell_rows(cell, &seq, &h) else { continue };
                if cell_rotation(cell) != 0.0 {
                    let x1 = gx[(cell.column + cell.column_span.max(1)).min(ncols)];
                    let b = Rect { left: gx[cell.column] + cell.insets[1], top: y0 + cell.insets[0], right: x1 - cell.insets[3], bottom: y1 - cell.insets[2] };
                    let (lines, depth) = turned[k].as_ref().map_or((&texts[k].0, texts[k].1), |x| (&x.0, x.1));
                    for l in turned_lines(cell, lines, depth, &b) {
                        let mut l = FlowLine { col: ci, para: None, ..l };
                        // turned text is not picked by clicks yet: its lines carry no clusters
                        l.line.text_clusters.clear();
                        own_cell_line(&mut l.line, cell, index);
                        flow.frames[fi].lines.push(l);
                    }
                    continue;
                }
                let (lines, depth) = (&texts[k].0, texts[k].1);
                let box_h = y1 - y0 - cell.insets[0] - cell.insets[2];
                let free = (box_h - depth).max(0.0);
                let shift = match cell.vertical_justification {
                    VerticalJustification::Center => free / 2.0,
                    VerticalJustification::Bottom => free,
                    _ => 0.0,
                };
                let (dx, dy) = (gx[cell.column] + cell.insets[1], y0 + cell.insets[0] + shift);
                for l in lines {
                    // Text below the cell's room is overset in the cell (not drawn).
                    if l.line.baseline > box_h + EPS {
                        continue;
                    }
                    let mut l = l.clone();
                    translate_line(&mut l.line, dx, dy);
                    l.text = (l.text.0 + dx, l.text.1 + dx);
                    l.col = ci;
                    l.para = None;
                    own_cell_line(&mut l.line, cell, index);
                    flow.frames[fi].lines.push(l);
                }
            }
            first_piece = false;
            if done {
                flow.last = Some(outer_bottom);
                flow.table_after = t.space_after;
                flow.jumped_from = None;
                flow.pending_after = 0.0;
                flow.pending_part = None;
                return true;
            }
            let frame_before = flow.fi;
            let page_key = |id: &str| self.frames.frame(id).map(|f| (f.spread, f.page_in_spread));
            let page_before = flow.frame().and_then(|f| page_key(f.id));
            if !flow.advance() {
                return false;
            }
            // A Start Row of a frame or page kind skips the other columns of this frame; the page kinds go on to the
            // first frame on another page, of the right side for the odd/even kinds (recto = odd, as Start Paragraph)
            if let Some(s) = start_break.filter(|s| *s != "NextColumn") {
                loop {
                    let Some(f) = flow.frame() else { return false };
                    let other_page = page_key(f.id) != page_before;
                    let there = match s {
                        "NextPage" => other_page,
                        "NextOddPage" => other_page && !f.verso,
                        "NextEvenPage" => other_page && f.verso,
                        _ => flow.fi != frame_before,
                    };
                    if there { break; }
                    if !flow.advance() { return false; }
                }
            }
        }
    }
}

/// A line of cell text in the story: it keeps the cell's own character range and clusters for
/// editing the cell, and shows as the table's character `index` to the story.
fn own_cell_line(line: &mut LaidLine, cell: &folio_model::TableCell, index: usize) {
    line.cell = Some(folio_model::layout::CellText {
        id: cell.id.clone(),
        start: line.start,
        end: line.end,
        clusters: std::mem::take(&mut line.text_clusters),
    });
    line.start = index;
    line.end = index;
}

fn cell_rotation(cell: &folio_model::TableCell) -> f64 {
    let direction = if cell.perpendicular { 90.0 } else { 0.0 };
    (cell.rotation + direction).rem_euclid(360.0)
}

fn quarter_turn(rotation: f64) -> bool {
    let r = rotation.rem_euclid(360.0);
    (r - 90.0).abs() < 1.0 || (r - 270.0).abs() < 1.0
}

/// Lines of a rotated cell turned into its text box `b`: InDesign turns the cell's text frame
/// clockwise by the cell's rotation (90°: lines run down from the box's top, the first line at
/// its right edge; 270°: up from its bottom, the first line at the left; 180°: upside down).
/// Vertical justification works across the lines; underlines and strikethroughs become
/// shapes.
fn turned_lines(cell: &folio_model::TableCell, lines: &[FlowLine], depth: f64, b: &Rect) -> Vec<FlowLine> {
    let r = cell_rotation(cell);
    let across = if quarter_turn(r) { b.width() } else { b.height() };
    let free = (across - depth).max(0.0);
    let shift = match cell.vertical_justification {
        VerticalJustification::Center => free / 2.0,
        VerticalJustification::Bottom => free,
        _ => 0.0,
    };
    // (u along the line, v down the turned frame) → frame coordinates.
    let map = |u: f64, v: f64| -> (f64, f64) {
        let v = v + shift;
        if (r - 90.0).abs() < 1.0 {
            (b.right - v, b.top + u)
        } else if (r - 270.0).abs() < 1.0 {
            (b.left + v, b.bottom - u)
        } else {
            (b.right - u, b.bottom - v)
        }
    };
    let mut out = Vec::new();
    for l in lines {
        if l.line.baseline > across + EPS {
            continue;
        }
        let mut n = l.clone();
        let line = &mut n.line;
        for g in &mut line.glyphs {
            (g.x, g.y) = map(g.x, g.y);
        }
        for o in &mut line.objects {
            (o.x, o.y) = map(o.x, o.y);
        }
        let mut shapes = std::mem::take(&mut line.shapes);
        for d in std::mem::take(&mut line.decorations) {
            for mut shape in d.shapes {
                for sp in &mut shape.paths { for p in &mut sp.points {
                    for q in [&mut p.anchor, &mut p.left, &mut p.right] { q.x += d.x0; q.y += d.y; }
                }}
                shapes.push(shape);
            }
        }
        for s in &mut shapes {
            for sp in &mut s.paths {
                for pt in &mut sp.points {
                    for q in [&mut pt.anchor, &mut pt.left, &mut pt.right] {
                        (q.x, q.y) = map(q.x, q.y);
                    }
                }
            }
        }
        line.shapes = shapes;
        let (p0, p1) = (map(line.left, line.baseline), map(line.right, line.baseline));
        line.left = p0.0.min(p1.0);
        line.right = p0.0.max(p1.0);
        line.baseline = p0.1.max(p1.1);
        line.rotation = r;
        n.text = (line.left, line.right);
        out.push(n);
    }
    out
}

/// Row heights: "exactly" rows keep their height; "at least" rows fit their cells (insets
/// plus text depth) and their minimum, up to their maximum; a cell spanning rows grows the
/// last of them.
fn row_heights(t: &Table, texts: &[(Vec<FlowLine>, f64)]) -> Vec<f64> {
    let n = t.rows.len();
    let mut h: Vec<f64> = t.rows.iter().map(|r| if r.auto_grow { r.min_height.max(0.0) } else { r.height.max(0.0) }).collect();
    let need = |k: usize| t.cells[k].insets[0] + texts[k].1 + t.cells[k].insets[2];
    for (k, c) in t.cells.iter().enumerate() {
        if c.row < n && c.row_span <= 1 && t.rows[c.row].auto_grow && !quarter_turn(cell_rotation(c)) {
            h[c.row] = h[c.row].max(need(k));
        }
    }
    for (k, c) in t.cells.iter().enumerate() {
        if c.row_span > 1 && c.row < n && !quarter_turn(cell_rotation(c)) {
            let last = (c.row + c.row_span - 1).min(n - 1);
            let have: f64 = h[c.row..=last].iter().sum();
            if need(k) > have && t.rows[last].auto_grow {
                h[last] += need(k) - have;
            }
        }
    }
    for (hh, r) in h.iter_mut().zip(&t.rows) {
        if r.auto_grow && r.max_height > 0.0 {
            *hh = hh.min(r.max_height.max(r.min_height));
        }
    }
    h
}

/// What the cells of each row need at least (insets plus text depth; cells over one row, rows that grow with their text): a row line dragged with
/// Shift stops when the row that gives up height is down to this (InDesign 2025, docdrag-c1: a row of three text lines stopped at its 41.52 pt).
fn row_needs(t: &Table, texts: &[(Vec<FlowLine>, f64)]) -> Vec<f64> {
    let n = t.rows.len();
    let mut h = vec![0.0_f64; n];
    for (k, c) in t.cells.iter().enumerate() {
        if c.row < n && c.row_span <= 1 && t.rows[c.row].auto_grow && !quarter_turn(cell_rotation(c)) {
            h[c.row] = h[c.row].max(c.insets[0] + texts[k].1 + c.insets[2]);
        }
    }
    h
}

/// Indices of the first and last row of a cell wholly present in this table piece.
fn cell_row_indices(c: &folio_model::TableCell, seq: &[(usize, f64)]) -> Option<(usize, usize)> {
    let span = c.row_span.max(1);
    let pos = seq.iter().position(|s| s.0 == c.row)?;
    if pos + span > seq.len() || (0..span).any(|d| seq[pos + d].0 != c.row + d) {
        return None;
    }
    Some((pos, pos + span - 1))
}

fn row_bottom(seq: &[(usize, f64)], h: &[f64], i: usize) -> f64 {
    let (row, top) = seq[i];
    top + h[row]
}

/// Top and bottom grid lines of a cell in a piece (all its rows there, in order).
fn cell_rows(c: &folio_model::TableCell, seq: &[(usize, f64)], h: &[f64]) -> Option<(f64, f64)> {
    let (first, last) = cell_row_indices(c, seq)?;
    Some((seq[first].1, row_bottom(seq, h, last)))
}

/// A stroke run along a grid line: from `a` to `b` along it, centred on `pos` across.
struct Seg {
    a: f64,
    b: f64,
    pos: f64,
    w: f64,
    paint: Paint,
    op: bool,
    /// Stroke type ("Solid", "Dashed", …).
    kind: String,
    /// What fills the gaps of a patterned stroke.
    gap: Paint,
    gap_op: bool,
    /// How far a dotted horizontal edge starts and ends inside the cell edge: half a dot at the table's outer edge, none at a crossing,
    /// where a dot sits on the corner (oracle alt-row-stroke-dotted).
    inset: (f64, f64),
}

/// Fills, diagonals and edge strokes of the rows `seq` (row, top) of a table piece; `gx`
/// are the column grid lines.
fn piece_shapes(t: &Table, owner: &[Vec<usize>], seq: &[(usize, f64)], h: &[f64], gx: &[f64]) -> Vec<TextShape> {
    let ncols = gx.len() - 1;
    let nseq = seq.len();
    let nrows = t.rows.len();
    let header = t.header_rows.min(nrows);
    let body_end = nrows - t.footer_rows.min(nrows - header);

    // Adjacent fills meet at the same line as the horizontal border. At a section
    // join this is the header's bottom or the footer's top, even if body rows shift.
    let boundaries: Vec<_> = (0..=nseq).map(|k| {
        match (k.checked_sub(1).map(|i| seq[i].0), seq.get(k)) {
            (Some(up), Some(&(down, _))) if up < header && down >= header => {
                (row_bottom(seq, h, k - 1), true, false)
            }
            (Some(up), Some(&(down, y))) if up < body_end && down >= body_end => {
                (y, false, true)
            }
            (_, Some(&(_, y))) => (y, true, true),
            _ => (row_bottom(seq, h, nseq - 1), true, true),
        }
    }).collect();

    let mut out: Vec<TextShape> = Vec::new();
    let rows: Vec<_> = t.cells.iter().map(|c| cell_row_indices(c, seq)).collect();
    let x_of = |c: &folio_model::TableCell| (gx[c.column.min(ncols)], gx[(c.column + c.column_span.max(1)).min(ncols)]);
    for (c, range) in t.cells.iter().zip(&rows) {
        if let Some((first, last)) = *range {
            let (x0, x1) = x_of(c);
            let (y0, y1) = (boundaries[first].0, boundaries[last + 1].0);
            out.extend(TextShape::rect(x0, y0, x1, y1, c.fill.clone(), c.fill_overprint));
        }
    }
    let diagonals = |front: bool, out: &mut Vec<TextShape>| {
        for (c, range) in t.cells.iter().zip(&rows) {
            let Some((first, last)) = *range else { continue };
            if c.diagonal_in_front != front || !(c.diagonal_down || c.diagonal_up) {
                continue;
            }
            let (y0, y1) = (seq[first].1, row_bottom(seq, h, last));
            let (left, right) = x_of(c);
            let [top, left_inset, bottom, right_inset] = c.edges.each_ref().map(|edge| {
                if edge.paint == Paint::None { 0.0 } else { edge.weight.max(0.0) / 2.0 }
            });
            let (x0, x1) = (left + left_inset, right - right_inset);
            let (y0, y1) = (y0 + top, y1 - bottom);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let d = &c.diagonal;
            let mut diagonal = |a: (f64, f64), b: (f64, f64)| match strokes::line_shapes(a, b, d.weight, &d.stroke_type, Ends::Free, &d.paint, d.overprint) {
                Some(s) => out.extend(s),
                None => out.extend(segment_shape(a, b, d.weight, d.paint.clone(), d.overprint)),
            };
            if c.diagonal_down {
                diagonal((x0, y0), (x1, y1));
            }
            if c.diagonal_up {
                diagonal((x1, y0), (x0, y1));
            }
        }
    };
    diagonals(false, &mut out);

    let at = |i: usize, col: usize| -> Option<usize> {
        let k = owner[seq[i].0][col];
        (k != usize::MAX).then_some(k)
    };
    // equal priorities: the stroke a cell or its cell style sets beats InDesign's default stroke of the other cell (oracle tablestyles
    // cs-edge-top: a cell style's top edge against the default bottom edge of the cell above, both priority 0)
    let pick = |x: &'_ CellStroke, y: &'_ CellStroke| -> CellStroke { if y.priority > x.priority || (y.priority == x.priority && y.explicit && !x.explicit) { y.clone() } else { x.clone() } };
    let visible = |s: CellStroke| (s.weight > 0.0 && s.paint != Paint::None).then_some(s);
    let between = |p: Option<usize>, q: Option<usize>, pe: usize, qe: usize| -> Option<CellStroke> {
        match (p, q) {
            (Some(p), Some(q)) if p == q => None,
            (Some(p), Some(q)) => visible(pick(&t.cells[p].edges[pe], &t.cells[q].edges[qe])),
            (Some(p), None) => visible(t.cells[p].edges[pe].clone()),
            (None, Some(q)) => visible(t.cells[q].edges[qe].clone()),
            (None, None) => None,
        }
    };
    // Vertical strokes at grid line j on row i.
    let vstroke = |i: usize, j: usize| between((j > 0).then(|| at(i, j - 1)).flatten(), (j < ncols).then(|| at(i, j)).flatten(), 3, 1);
    let vw = |i: usize, j: usize| vstroke(i, j).map_or(0.0, |s| s.weight);
    let mut segs: Vec<(bool, Seg)> = Vec::new();
    let flush = |run: &mut Option<Seg>, horizontal: bool, segs: &mut Vec<(bool, Seg)>| {
        if let Some(s) = run.take() {
            segs.push((horizontal, s));
        }
    };
    for (k, (y, keep_up, keep_down)) in boundaries.into_iter().enumerate() {
        let mut run: Option<Seg> = None;
        for c in 0..ncols {
            let s = between((k > 0 && keep_up).then(|| at(k - 1, c)).flatten(), (k < nseq && keep_down).then(|| at(k, c)).flatten(), 2, 0);
            let Some(s) = s else {
                flush(&mut run, true, &mut segs);
                continue;
            };
            let ext = |j: usize| {
                let mut e: f64 = 0.0;
                if k > 0 {
                    e = e.max(vw(k - 1, j));
                }
                if k < nseq {
                    e = e.max(vw(k, j));
                }
                e / 2.0
            };
            // A styled stroke is patterned edge by edge between the grid lines (InDesign
            // centres the pattern on every crossing); solid ones join into one rectangle
            // reaching over the crossing strokes.
            if strokes::pattern(&s.stroke_type, s.weight).is_some() {
                flush(&mut run, true, &mut segs);
                segs.push((true, Seg { a: gx[c], b: gx[c + 1], pos: y, w: s.weight, gap: s.gap, gap_op: s.gap_overprint, inset: (if c == 0 { s.weight / 2.0 } else { 0.0 }, if c + 1 == ncols { s.weight / 2.0 } else { 0.0 }), paint: s.paint, op: s.overprint, kind: s.stroke_type }));
                continue;
            }
            let (x0, x1) = (gx[c] - ext(c), gx[c + 1] + ext(c + 1));
            match run.as_mut() {
                Some(r) if r.w == s.weight && r.paint == s.paint && r.op == s.overprint => r.b = x1,
                _ => {
                    flush(&mut run, true, &mut segs);
                    run = Some(Seg { a: x0, b: x1, pos: y, w: s.weight, gap: Paint::None, gap_op: false, inset: (0.0, 0.0), paint: s.paint, op: s.overprint, kind: s.stroke_type });
                }
            }
        }
        flush(&mut run, true, &mut segs);
    }
    #[allow(clippy::needless_range_loop, reason = "the column count determines which border indexes to visit")]
    for j in 0..=ncols {
        let mut run: Option<Seg> = None;
        for i in 0..nseq {
            let Some(s) = vstroke(i, j) else {
                flush(&mut run, false, &mut segs);
                continue;
            };
            let (y0, y1) = (seq[i].1, seq[i].1 + h[seq[i].0]);
            if strokes::pattern(&s.stroke_type, s.weight).is_some() {
                flush(&mut run, false, &mut segs);
                segs.push((false, Seg { a: y0, b: y1, pos: gx[j], w: s.weight, gap: s.gap, gap_op: s.gap_overprint, inset: (0.0, 0.0), paint: s.paint, op: s.overprint, kind: s.stroke_type }));
                continue;
            }
            match run.as_mut() {
                Some(r) if r.w == s.weight && r.paint == s.paint && r.op == s.overprint && (r.b - y0).abs() < 1e-9 => r.b = y1,
                _ => {
                    flush(&mut run, false, &mut segs);
                    run = Some(Seg { a: y0, b: y1, pos: gx[j], w: s.weight, gap: Paint::None, gap_op: false, inset: (0.0, 0.0), paint: s.paint, op: s.overprint, kind: s.stroke_type });
                }
            }
        }
        flush(&mut run, false, &mut segs);
    }
    // Heavier strokes on top; a patterned stroke under the solid ones (the grid line crosses over a dashed column stroke:
    // oracle alt-column-stroke-dotted).
    segs.sort_by(|x, y| (strokes::pattern(&y.1.kind, y.1.w).is_some()).cmp(&strokes::pattern(&x.1.kind, x.1.w).is_some()).then(x.1.w.total_cmp(&y.1.w)));
    for (horizontal, s) in segs {
        let half = s.w / 2.0;
        let (from, to) = if horizontal { ((s.a, s.pos), (s.b, s.pos)) } else { ((s.pos, s.a), (s.pos, s.b)) };
        // Dashes sit on every corner. Dots on a horizontal edge too, but half a dot in from the table's outer edge; on a vertical edge they
        // keep inside it like an open rule's (oracle alt-row-stroke-dotted, alt-column-stroke-dotted)
        let dotted = matches!(strokes::pattern(&s.kind, s.w), Some(folio_model::geom::StrokePattern::Dotted { .. }));
        let (from, to) = if dotted && horizontal { ((from.0 + s.inset.0, from.1), (to.0 - s.inset.1, to.1)) } else { (from, to) };
        if let Some(shapes) = strokes::line_shapes(from, to, s.w, &s.kind, if dotted && !horizontal { Ends::Free } else { Ends::Corners }, &s.paint, s.op) {
            // the gap colour under the dashes, the whole edge
            if s.gap != Paint::None && !shapes.is_empty() {
                out.extend(if horizontal { TextShape::rect(s.a, s.pos - half, s.b, s.pos + half, s.gap.clone(), s.gap_op) } else { TextShape::rect(s.pos - half, s.a, s.pos + half, s.b, s.gap.clone(), s.gap_op) });
            }
            out.extend(shapes);
            continue;
        }
        let shape = if horizontal {
            TextShape::rect(s.a, s.pos - half, s.b, s.pos + half, s.paint, s.op)
        } else {
            TextShape::rect(s.pos - half, s.a, s.pos + half, s.b, s.paint, s.op)
        };
        out.extend(shape);
    }
    diagonals(true, &mut out);
    out
}
