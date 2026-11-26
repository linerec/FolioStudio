//! Insert, paste and delete rows and columns.

use super::*;

/// Table ▸ Insert ▸ Row(s): `count` rows before or after row `at`, one by one as InDesign's
/// script adds them (each next to `at`).
pub(in crate::ops) fn insert_rows(w: &mut Work<'_>, table: &str, at: usize, count: u32, before: bool) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    // InDesign's `Rows.add` has no upper limit of its own (a table grew to 13000 rows in the measurement), so the one-step count only keeps
    // the Table Options field (up to 10000 body rows) reachable from a one-row table.
    if at >= x.rows.len() || count == 0 || count > MAX_BODY_ROWS {
        return Err(EditError::new("invalid", format!("insert rows: a row of the table, 1 to {MAX_BODY_ROWS}")));
    }
    let mut used = 0;
    for _ in 0..count {
        let new = if before { at } else { at + 1 };
        let like = x.rows[at.min(x.rows.len() - 1)].clone();
        // the rows below move down
        for c in x.cells.iter_mut() {
            if c.row >= new {
                c.row += 1;
            } else if c.row + c.rows > new {
                c.rows += 1;
            }
        }
        x.rows.insert(new, like);
        let reference = if before { at + 1 } else { at };
        for col in 0..x.columns.len() {
            // a column covered there by a cell spanning the new row stays covered
            if x.at(col, new).is_some() {
                continue;
            }
            let Some(src) = x.at(col, reference) else { continue };
            let id = x.new_cell_id(&mut used);
            let cell = empty_like(&x.cells[src], id, col, new, &x.indent);
            x.cells.push(cell);
        }
        // a header row inserted among the header rows is a header row
        let h = x.header();
        if new < h || (before && at < h) {
            put(&mut x.attrs, "HeaderRowCount", (h + 1).to_string());
        }
    }
    x.write(w)
}

/// Table ▸ Insert ▸ Column(s): `count` columns before or after column `at`, each as wide as it;
/// the table widens.
pub(in crate::ops) fn insert_columns(w: &mut Work<'_>, table: &str, at: usize, count: u32, before: bool) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    if at >= x.columns.len() || count == 0 || count > 200 {
        return Err(EditError::new("invalid", "insert columns: a column of the table, 1 to 200"));
    }
    let mut used = 0;
    for _ in 0..count {
        let new = if before { at } else { at + 1 };
        let like = x.columns[at].clone();
        for c in x.cells.iter_mut() {
            if c.column >= new {
                c.column += 1;
            } else if c.column + c.columns > new {
                c.columns += 1;
            }
        }
        x.columns.insert(new, like);
        let reference = if before { at + 1 } else { at };
        for row in 0..x.rows.len() {
            if x.at(new, row).is_some() {
                continue;
            }
            let Some(src) = x.at(reference, row) else { continue };
            let id = x.new_cell_id(&mut used);
            let cell = empty_like(&x.cells[src], id, new, row, &x.indent);
            x.cells.push(cell);
        }
    }
    x.write(w)
}

/// A copied cell may not hold elements that have ids of their own (anchored objects, footnotes, nested tables): those would need new ids.
fn copyable(c: &CellX) -> bool {
    !c.inner.contains(" Self=\"")
}

/// Table ▸ Paste Before / Paste After with copied rows (measured in InDesign 2025: copy row(s) with a row selected, the menu actions "Paste
/// Before" and "Paste After" put a copy of the rows, with their cells' text and formatting, before or after the selected row; a single
/// copied cell leaves both menu items off). The source is a table with as many columns; no cell may reach out of the copied rows, and none of
/// this table's may span across the place the rows go.
pub(in crate::ops) fn paste_rows(w: &mut Work<'_>, table: &str, source: Option<&str>, from: usize, to: usize, at: usize, before: bool) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let src = match source { Some(s) if s != table => TableX::read(w, s)?, _ => TableX::read(w, table)? };
    if from > to || to >= src.rows.len() || at >= x.rows.len() || src.columns.len() != x.columns.len() {
        return Err(EditError::new("invalid", "paste rows: rows of the source, a row of the table, as many columns"));
    }
    let count = to - from + 1;
    let copied: Vec<&CellX> = src.cells.iter().filter(|c| c.row + c.rows > from && c.row <= to).collect();
    if copied.iter().any(|c| c.row < from || c.row + c.rows - 1 > to || !copyable(c)) {
        return Err(EditError::new("invalid", "paste rows: a copied cell reaches out of the rows or holds an object of its own"));
    }
    let new = if before { at } else { at + 1 };
    if x.cells.iter().any(|c| c.row < new && c.row + c.rows > new) {
        return Err(EditError::new("invalid", "paste rows: a merged cell spans the place the rows go"));
    }
    let lines: Vec<Line> = src.rows[from..=to].to_vec();
    let cells: Vec<CellX> = copied.into_iter().cloned().collect();
    for c in x.cells.iter_mut() {
        if c.row >= new {
            c.row += count;
        }
    }
    for (k, line) in lines.into_iter().enumerate() {
        x.rows.insert(new + k, line);
    }
    let mut used = 0;
    for mut c in cells {
        c.id = x.new_cell_id(&mut used);
        c.row = new + (c.row - from);
        x.cells.push(c);
    }
    let h = x.header();
    if new < h {
        put(&mut x.attrs, "HeaderRowCount", (h + count).to_string());
    }
    x.write(w)
}

/// Table ▸ Paste Before / Paste After with copied columns (as `paste_rows`, sideways).
pub(in crate::ops) fn paste_columns(w: &mut Work<'_>, table: &str, source: Option<&str>, from: usize, to: usize, at: usize, before: bool) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let src = match source { Some(s) if s != table => TableX::read(w, s)?, _ => TableX::read(w, table)? };
    if from > to || to >= src.columns.len() || at >= x.columns.len() || src.rows.len() != x.rows.len() {
        return Err(EditError::new("invalid", "paste columns: columns of the source, a column of the table, as many rows"));
    }
    let count = to - from + 1;
    let copied: Vec<&CellX> = src.cells.iter().filter(|c| c.column + c.columns > from && c.column <= to).collect();
    if copied.iter().any(|c| c.column < from || c.column + c.columns - 1 > to || !copyable(c)) {
        return Err(EditError::new("invalid", "paste columns: a copied cell reaches out of the columns or holds an object of its own"));
    }
    let new = if before { at } else { at + 1 };
    if x.cells.iter().any(|c| c.column < new && c.column + c.columns > new) {
        return Err(EditError::new("invalid", "paste columns: a merged cell spans the place the columns go"));
    }
    let lines: Vec<Line> = src.columns[from..=to].to_vec();
    let cells: Vec<CellX> = copied.into_iter().cloned().collect();
    for c in x.cells.iter_mut() {
        if c.column >= new {
            c.column += count;
        }
    }
    for (k, line) in lines.into_iter().enumerate() {
        x.columns.insert(new + k, line);
    }
    let mut used = 0;
    for mut c in cells {
        c.id = x.new_cell_id(&mut used);
        c.column = new + (c.column - from);
        x.cells.push(c);
    }
    x.write(w)
}

/// Table ▸ Delete ▸ Row: `count` rows from `at`; cells spanning into them lose those rows.
pub(in crate::ops) fn delete_rows(w: &mut Work<'_>, table: &str, at: usize, count: u32) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let end = at + count as usize;
    if count == 0 || end > x.rows.len() || end - at == x.rows.len() {
        return Err(EditError::new("invalid", "delete rows: rows of the table, not all of them"));
    }
    x.remove_rows(at, end);
    x.write(w)
}

/// Table ▸ Delete ▸ Column: `count` columns from `at`; the table narrows.
pub(in crate::ops) fn delete_columns(w: &mut Work<'_>, table: &str, at: usize, count: u32) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let end = at + count as usize;
    if count == 0 || end > x.columns.len() || end - at == x.columns.len() {
        return Err(EditError::new("invalid", "delete columns: columns of the table, not all of them"));
    }
    x.cells.retain(|c| !(at..end).contains(&c.column));
    for c in x.cells.iter_mut() {
        let (s, e) = (c.column, c.column + c.columns);
        let cut = e.min(end).saturating_sub(s.max(at));
        c.columns -= cut;
        if s >= end {
            c.column -= count as usize;
        }
    }
    x.columns.drain(at..end);
    x.write(w)
}
