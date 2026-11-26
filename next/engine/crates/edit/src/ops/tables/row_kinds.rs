//! Set header and footer counts and convert rows between table parts.

use super::*;

impl TableX {
    pub(super) fn footer(&self) -> usize {
        get(&self.attrs, "FooterRowCount").and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// Header (0), body (1) or footer (2) row.
    pub(super) fn part_of(&self, row: usize) -> u8 {
        let (h, f) = (self.header(), self.footer());
        if row < h { 0 } else if row + f >= self.rows.len() { 2 } else { 1 }
    }
}

/// Table Options ▸ Headers and Footers, header rows: more are new empty rows under the
/// existing ones (minimum height 3 pt); fewer delete the last header rows (tbl-header-fewer,
/// tbl-header-two-to-one).
pub(in crate::ops) fn set_header_rows(w: &mut Work<'_>, table: &str, header: usize) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let h = x.header();
    if header > MAX_HEADER_FOOTER_ROWS as usize {
        return Err(EditError::new("invalid", format!("header rows: 0 to {MAX_HEADER_FOOTER_ROWS}")));
    }
    if header < h {
        if x.rows.len() - (h - header) == 0 {
            return Err(EditError::new("invalid", "a table keeps a row"));
        }
        x.remove_rows(header, h);
        return x.write(w);
    }
    if header == h {
        return Ok(());
    }
    let mut used = 0;
    for k in h..header {
        let like = x.rows[k.min(x.rows.len() - 1)].clone();
        for c in x.cells.iter_mut() {
            if c.row >= k {
                c.row += 1;
            }
        }
        let mut row = like;
        put(&mut row.attrs, "MinimumHeight", "3".into());
        x.rows.insert(k, row);
        let reference = if k > 0 { k - 1 } else { k + 1 };
        for col in 0..x.columns.len() {
            let Some(src) = x.at(col, reference) else { continue };
            let id = x.new_cell_id(&mut used);
            let cell = empty_like(&x.cells[src], id, col, k, &x.indent);
            x.cells.push(cell);
        }
    }
    put(&mut x.attrs, "HeaderRowCount", header.to_string());
    x.write(w)
}

/// Table Options ▸ Headers and Footers, footer rows: more are new empty rows at the end
/// (minimum height 3 pt, tbl-footer-rows); fewer delete the first footer rows, next to the
/// body (tbl-footer-fewer).
pub(in crate::ops) fn set_footer_rows(w: &mut Work<'_>, table: &str, footer: usize) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let (f, n) = (x.footer(), x.rows.len());
    if footer > MAX_HEADER_FOOTER_ROWS as usize {
        return Err(EditError::new("invalid", format!("footer rows: 0 to {MAX_HEADER_FOOTER_ROWS}")));
    }
    if footer < f {
        if n - (f - footer) == 0 {
            return Err(EditError::new("invalid", "a table keeps a row"));
        }
        x.remove_rows(n - f, n - footer);
        return x.write(w);
    }
    let mut used = 0;
    for _ in f..footer {
        let k = x.rows.len();
        let mut row = x.rows[k - 1].clone();
        put(&mut row.attrs, "MinimumHeight", "3".into());
        x.rows.push(row);
        for col in 0..x.columns.len() {
            let Some(src) = x.at(col, k - 1) else { continue };
            let id = x.new_cell_id(&mut used);
            let cell = empty_like(&x.cells[src], id, col, k, &x.indent);
            x.cells.push(cell);
        }
    }
    put(&mut x.attrs, "FooterRowCount", footer.to_string());
    x.write(w)
}

/// Table ▸ Convert Rows ▸ To Header / To Body / To Footer: rows next to the header or footer
/// change part; the rows stay (tbl-row-to-header, tbl-row-to-footer, tbl-header-to-body).
pub(in crate::ops) fn convert_rows(w: &mut Work<'_>, table: &str, at: usize, count: usize, to: RowKind) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let (h, f, n) = (x.header(), x.footer(), x.rows.len());
    let end = at + count;
    if count == 0 || end > n {
        return Err(EditError::new("invalid", "convert rows: rows of the table"));
    }
    let (header, footer) = match to {
        RowKind::Header if at <= h && end > h && end <= n - f => (end, f),
        RowKind::Footer if end >= n - f && at < n - f && at >= h => (h, n - at),
        RowKind::Body if end == h && at < h => (at, f),
        RowKind::Body if at == n - f && f > 0 => (h, n - end),
        _ => return Err(EditError::new("invalid", "only rows next to the header or footer change part")),
    };
    if header + footer >= n {
        return Err(EditError::new("invalid", "a table keeps a body row"));
    }
    put(&mut x.attrs, "HeaderRowCount", header.to_string());
    put(&mut x.attrs, "FooterRowCount", footer.to_string());
    x.write(w)
}

/// A part of a table: its header, body or footer rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RowKind {
    Header,
    Body,
    Footer,
}
