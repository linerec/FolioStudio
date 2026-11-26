//! Merge, unmerge and split cells and maintain their spans.

use super::*;

/// Table ▸ Merge Cells: the cells of the rectangle from `from` to `to` ([column, row]) become
/// its first cell, holding their text joined by paragraph ends.
pub(in crate::ops) fn merge_cells(w: &mut Work<'_>, table: &str, from: [usize; 2], to: [usize; 2]) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let (c0, c1) = (from[0].min(to[0]), from[0].max(to[0]));
    let (r0, r1) = (from[1].min(to[1]), from[1].max(to[1]));
    if c1 >= x.columns.len() || r1 >= x.rows.len() {
        return Err(EditError::new("invalid", "merge: cells of the table"));
    }
    let inside: Vec<usize> = (0..x.cells.len()).filter(|&k| {
        let c = &x.cells[k];
        c.column + c.columns > c0 && c.column <= c1 && c.row + c.rows > r0 && c.row <= r1
    }).collect();
    if inside.iter().any(|&k| {
        let c = &x.cells[k];
        c.column < c0 || c.column + c.columns - 1 > c1 || c.row < r0 || c.row + c.rows - 1 > r1
    }) {
        return Err(EditError::new("invalid", "merge: a merged cell reaches outside the cells"));
    }
    let first = x.at(c0, r0).ok_or_else(|| EditError::new("xml", "no first cell"))?;
    let mut others: Vec<usize> = inside.iter().copied().filter(|&k| k != first).collect();
    others.sort_by_key(|&k| (x.cells[k].row, x.cells[k].column));
    let mut inner = x.cells[first].inner.clone();
    for &k in &others {
        if has_text(&x.cells[k]) {
            inner = join_cells(&inner, &x.cells[k].inner);
        }
    }
    let removed: Vec<(usize, usize, String)> = others.iter().map(|&k| (x.cells[k].column, x.cells[k].row, x.cells[k].id.clone())).collect();
    {
        let c = &mut x.cells[first];
        c.columns = c1 - c0 + 1;
        c.rows = r1 - r0 + 1;
        c.inner = inner;
    }
    let mut keep: Vec<CellX> = Vec::new();
    for (k, c) in x.cells.drain(..).enumerate() {
        if !others.contains(&k) {
            keep.push(c);
        }
    }
    x.cells = keep;
    // InDesign keeps the cells a merge covers; unmerging gives them back
    for (c, r, id) in removed {
        w.ed.merged_cells.insert((table.to_owned(), c, r), id);
    }
    x.write(w)
}

/// One cell's text after another's, a paragraph end between them: in the same character range
/// when both are one range of the same formatting.
fn join_cells(a: &str, b: &str) -> String {
    let one = |s: &str| -> Option<(String, String, String)> {
        // a single paragraph range holding a single character range: (open tags, contents, close tags)
        let ps = s.find("<ParagraphStyleRange")?;
        if s[ps + 1..].contains("<ParagraphStyleRange") { return None; }
        let cs = s.find("<CharacterStyleRange")?;
        if s[cs + 1..].contains("<CharacterStyleRange") { return None; }
        let open_end = s[cs..].find('>')? + cs + 1;
        if s[..open_end].ends_with("/>") { return None; }
        let close = s.rfind("</CharacterStyleRange>")?;
        Some((s[..open_end].to_owned(), s[open_end..close].to_owned(), s[close..].to_owned()))
    };
    let tag = |s: &str, t: &str| s.find(&format!("<{t}")).and_then(|i| s[i..].find('>').map(|e| s[i..i + e].to_owned()));
    if let (Some((ao, ac, aclose)), Some((_, bc, _))) = (one(a), one(b))
        && tag(a, "ParagraphStyleRange") == tag(b, "ParagraphStyleRange") && tag(a, "CharacterStyleRange") == tag(b, "CharacterStyleRange")
    {
        let sep = ac.rsplit_once('>').map(|(_, s)| s.to_owned()).filter(|s| s.trim().is_empty()).unwrap_or_default();
        let ac = ac.trim_end();
        let bc = bc.trim_start();
        return format!("{ao}{ac}{sep}<Br />{sep}{bc}{aclose}");
    }
    // otherwise b's paragraphs follow a's last one, ended
    let close = a.rfind("</CharacterStyleRange>").unwrap_or(a.len());
    let body = b.find("<ParagraphStyleRange").map_or(b, |i| &b[i..]);
    format!("{}<Br />{}{}", &a[..close], &a[close..].trim_end_matches(char::is_whitespace), body)
}

/// Table ▸ Unmerge Cells: the merged cell at [column, row] spans one cell again; the cells it
/// covered come back empty, with their ids.
pub(in crate::ops) fn unmerge_cell(w: &mut Work<'_>, table: &str, cell: [usize; 2]) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let k = x.at(cell[0], cell[1]).ok_or_else(|| EditError::new("invalid", "unmerge: a cell of the table"))?;
    let (c0, r0, cs, rs) = (x.cells[k].column, x.cells[k].row, x.cells[k].columns, x.cells[k].rows);
    if cs == 1 && rs == 1 {
        return Ok(());
    }
    x.cells[k].columns = 1;
    x.cells[k].rows = 1;
    let like = x.cells[k].clone();
    let mut used = 0;
    for r in r0..r0 + rs {
        for c in c0..c0 + cs {
            if (c, r) == (c0, r0) {
                continue;
            }
            let id = w.ed.merged_cells.remove(&(table.to_owned(), c, r)).unwrap_or_else(|| x.new_cell_id(&mut used));
            let new = empty_like(&like, id, c, r, &x.indent);
            x.cells.push(new);
        }
    }
    x.write(w)
}

/// Table ▸ Split Cell Horizontally / Vertically: a new row under the cell's row (both get half
/// its height as their minimum) or a new column beside its column (half the width each); the
/// row's or column's other cells span both.
pub(in crate::ops) fn split_cell(w: &mut Work<'_>, table: &str, cell: [usize; 2], vertical: bool) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let k = x.at(cell[0], cell[1]).ok_or_else(|| EditError::new("invalid", "split: a cell of the table"))?;
    let (c0, r0, cs, rs) = (x.cells[k].column, x.cells[k].row, x.cells[k].columns, x.cells[k].rows);
    if (vertical && cs != 1) || (!vertical && rs != 1) {
        return Err(EditError::new("unsupported", "splitting a merged cell across its span is not measured"));
    }
    let like = x.cells[k].clone();
    let mut used = 0;
    if vertical {
        // InDesign cuts the column at its middle in table coordinates (from the table's left
        // edge): the halves are mid − left and right − mid, not width / 2 each (tbl-split-v).
        let width_of = |l: &Line| get(&l.attrs, "SingleColumnWidth").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let left = x.columns[..c0].iter().map(width_of).fold(0.0, |a, w| a + w);
        let right = left + width_of(&x.columns[c0]);
        let mid = (left + right) / 2.0;
        put(&mut x.columns[c0].attrs, "SingleColumnWidth", geom::num(mid - left));
        let mut col = x.columns[c0].clone();
        put(&mut col.attrs, "SingleColumnWidth", geom::num(right - mid));
        for c in x.cells.iter_mut() {
            if c.column > c0 {
                c.column += 1;
            } else if c.column + c.columns > c0 && c.id != like.id {
                c.columns += 1;
            }
        }
        x.columns.insert(c0 + 1, col);
        let id = x.new_cell_id(&mut used);
        let mut new = empty_like(&like, id, c0 + 1, r0, &x.indent);
        new.rows = rs;
        x.cells.push(new);
    } else {
        let height = get(&x.rows[r0].attrs, "SingleRowHeight").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let half = geom::num(height / 2.0);
        put_before(&mut x.rows[r0].attrs, "MinimumHeight", half.clone(), "AutoGrow");
        let row = x.rows[r0].clone();
        for c in x.cells.iter_mut() {
            if c.row > r0 {
                c.row += 1;
            } else if c.row + c.rows > r0 && c.id != like.id {
                c.rows += 1;
            }
        }
        x.rows.insert(r0 + 1, row);
        let id = x.new_cell_id(&mut used);
        let mut new = empty_like(&like, id, c0, r0 + 1, &x.indent);
        new.columns = cs;
        x.cells.push(new);
    }
    x.write(w)
}
