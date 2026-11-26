//! Set row heights and column widths and apply table-line drags.

use super::*;

fn stored_size(line: &Line, attr: &str) -> Option<f64> {
    get(&line.attrs, attr).and_then(|value| value.parse().ok())
}

fn set_row_height(row: &mut Line, height: f64) {
    let value = geom::num(height);
    put(&mut row.attrs, "SingleRowHeight", value.clone());
    put(&mut row.attrs, "MinimumHeight", value);
}

/// Rows `rows` of a table: their height (an auto-grow row's minimum and its height, an exact
/// row's height) and whether they grow with their text.
pub(in crate::ops) fn set_rows(w: &mut Work<'_>, table: &str, rows: &[usize], height: Option<f64>, auto_grow: Option<bool>) -> Result<(), EditError> {
    if height.is_some_and(|h| !(h.is_finite() && (3.0..=10000.0).contains(&h))) {
        return Err(EditError::new("invalid", "row height: 3 to 10000 pt"));
    }
    let mut x = TableX::read(w, table)?;
    for &r in rows {
        let row = x.rows.get_mut(r).ok_or_else(|| EditError::new("invalid", format!("no row {r}")))?;
        if let Some(h) = height {
            set_row_height(row, h);
        }
        if let Some(g) = auto_grow {
            if g {
                row.attrs.retain(|(k, _)| k != "AutoGrow");
            } else {
                put(&mut row.attrs, "AutoGrow", "false".into());
            }
        }
    }
    x.write(w)
}

/// Columns `columns` of a table become `width` points wide.
pub(in crate::ops) fn set_columns(w: &mut Work<'_>, table: &str, columns: &[usize], width: f64) -> Result<(), EditError> {
    if !(width.is_finite() && (3.0..=10000.0).contains(&width)) {
        return Err(EditError::new("invalid", "column width: 3 to 10000 pt"));
    }
    let mut x = TableX::read(w, table)?;
    for &c in columns {
        let col = x.columns.get_mut(c).ok_or_else(|| EditError::new("invalid", format!("no column {c}")))?;
        put(&mut col.attrs, "SingleColumnWidth", geom::num(width));
    }
    x.write(w)
}

/// Dragging a table line with the Type tool (InDesign 2025, real mouse, `rules/docinput-c3.ko.md` section 8, `rules/tabledrag-c1.ko.md`):
/// `line` is the line between column (row) `line - 1` and `line`, `line == n` the table's right (bottom) edge, `by` how far it went along its axis.
/// - plain: only the column (row) before the line changes by `by` and everything after it moves along, so the table grows or shrinks by `by`;
/// - Shift on an inner line: the column (row) after it gives or takes what the one before takes or gives (the table stays the size it was);
/// - Shift on the outer edge: every column (row) is scaled by the same factor (the table grows by `by`);
/// - `insert` (Alt) dragging out: on a column line (inner or the right edge) a new column, `by` wide, goes in after the line; on the bottom edge
///   `max(1, round(by / height of the last row))` rows as high as the last row are added; on an inner row line Alt does nothing more than a plain drag
///   (all measured; dragging back with Alt was not measured and is a plain drag too);
/// - Shift on the line between the header rows and the body does nothing (InDesign 2025: 5 of 5 drags).
///   `size`: how big the column (row) before the line shows now (a row grown by its content is taller than its stored height); it is
///   taken as the old size.
///   A column or row never gets narrower than 3 pt (measured: 3 pt; a row also keeps the height of its content: see `min`).
///
/// `min` is what the row before and the row after the line need at least for their text (the layout's `content_min`; absent: the 3 pt floor). With Shift
/// the row that gives up height stops there, and so does the row before the line when it is the one that shrinks (InDesign 2025, docdrag-c1: a row of three
/// text lines, 60 pt set and 41.52 pt needed, kept 41.52 whichever side the line was dragged from).
pub(in crate::ops) fn drag_line(w: &mut Work<'_>, table: &str, columns: bool, line: usize, by: f64, shift: bool, insert: bool, size: Option<f64>, min: [Option<f64>; 2]) -> Result<(), EditError> {
    const MIN: f64 = 3.0;
    if !by.is_finite() || size.is_some_and(|s| !s.is_finite() || s < 0.0) || min.iter().flatten().any(|m| !m.is_finite() || *m < 0.0) {
        return Err(EditError::new("invalid", "a table line goes by a finite number of points"));
    }
    let mut x = TableX::read(w, table)?;
    let header = x.header();
    let (attr, lines) = if columns { ("SingleColumnWidth", &mut x.columns) } else { ("SingleRowHeight", &mut x.rows) };
    let n = lines.len();
    if line == 0 || line > n {
        return Err(EditError::new("invalid", format!("a table line is 1 to {n} (the line before column/row `line`; {n} is the outer edge)")));
    }
    let stored = |l: &Line| stored_size(l, attr).unwrap_or(0.0);
    let mut old: Vec<f64> = lines.iter().map(stored).collect();
    if let Some(s) = size {
        old[line - 1] = s;
    }
    if insert && by > 0.0 && (columns || line == n) {
        let amount = if columns { 1 } else { ((by / old[n - 1].max(MIN)).round() as u32).max(1) };
        let width = by.max(MIN);
        drop(x);
        if columns {
            insert_columns(w, table, line - 1, amount, false)?;
            let mut x = TableX::read(w, table)?;
            put(&mut x.columns[line].attrs, attr, geom::num(width));
            return x.write(w);
        }
        return insert_rows(w, table, n - 1, amount, false);
    }
    let mut new = old.clone();
    if shift && !columns && header > 0 && line == header {
        return Ok(());
    }
    if shift && line == n {
        let total: f64 = old.iter().sum();
        let f = if total > 0.0 { (total + by) / total } else { 1.0 };
        for v in new.iter_mut() {
            *v = (*v * f).max(MIN);
        }
    } else if shift {
        let (floor_before, floor_after) = (min[0].unwrap_or(MIN).max(MIN), min[1].unwrap_or(MIN).max(MIN));
        let by = by.clamp(-(old[line - 1] - floor_before).max(0.0), (old[line] - floor_after).max(0.0));
        new[line - 1] += by;
        new[line] -= by;
    } else {
        new[line - 1] = (old[line - 1] + by).max(MIN);
    }
    for (l, (a, b)) in lines.iter_mut().zip(old.iter().zip(&new)) {
        if (a - b).abs() > 1e-9 || (size.is_some() && (stored(l) - b).abs() > 1e-9) {
            if columns {
                put(&mut l.attrs, attr, geom::num(*b));
            } else {
                set_row_height(l, *b);
            }
        }
    }
    x.write(w)
}

/// Table ▸ Distribute Rows Evenly: the rows [first, last] share their heights equally; each
/// gets the mean as its height and minimum, at least or exactly as before
/// (tbl-distribute-rows, tbl-distribute-rows-exact).
pub(in crate::ops) fn distribute_rows(w: &mut Work<'_>, table: &str, first: usize, last: usize) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    if first >= last || last >= x.rows.len() {
        return Err(EditError::new("invalid", "distribute rows: two rows or more of the table"));
    }
    let height = |r: &Line| stored_size(r, "SingleRowHeight").unwrap_or(0.0);
    let total = x.rows[first..=last].iter().map(height).fold(0.0, |a, h| a + h);
    let mean = geom::num(total / (last - first + 1) as f64);
    for r in &mut x.rows[first..=last] {
        put(&mut r.attrs, "SingleRowHeight", mean.clone());
        put_before(&mut r.attrs, "MinimumHeight", mean.clone(), "AutoGrow");
    }
    x.write(w)
}

/// Table ▸ Distribute Columns Evenly: the columns [first, last] share their widths equally
/// (tbl-distribute-columns, tbl-distribute-columns-two).
pub(in crate::ops) fn distribute_columns(w: &mut Work<'_>, table: &str, first: usize, last: usize) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    if first >= last || last >= x.columns.len() {
        return Err(EditError::new("invalid", "distribute columns: two columns or more of the table"));
    }
    let width = |c: &Line| stored_size(c, "SingleColumnWidth").unwrap_or(0.0);
    let total = x.columns[first..=last].iter().map(width).fold(0.0, |a, v| a + v);
    let mean = geom::num(total / (last - first + 1) as f64);
    for c in &mut x.columns[first..=last] {
        put(&mut c.attrs, "SingleColumnWidth", mean.clone());
    }
    x.write(w)
}

/// What composing gives the auto-grow rows of a table: their heights (`SingleRowHeight`).
pub(in crate::ops) fn set_row_heights(w: &mut Work<'_>, table: &str, heights: &[f64]) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let mut changed = false;
    for (row, h) in x.rows.iter_mut().zip(heights) {
        if get(&row.attrs, "AutoGrow") == Some("false") || !h.is_finite() {
            continue;
        }
        let now = stored_size(row, "SingleRowHeight");
        if now.is_none_or(|v| (v - h).abs() > 1e-6) {
            put(&mut row.attrs, "SingleRowHeight", geom::num(*h));
            changed = true;
        }
    }
    if changed { x.write(w) } else { Ok(()) }
}
