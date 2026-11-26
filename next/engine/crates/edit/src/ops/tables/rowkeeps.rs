//! Cell Options ▸ Rows and Columns ▸ Keep Options (tools-c2): Keep with Next Row and Start Row of table rows, IDML's `Row/@KeepWithNextRow` and
//! `Row/@StartRow` (Anywhere, NextColumn, NextFrame, NextPage, NextOddPage, NextEvenPage). The model reads both (folio-idml tables); the
//! composition keeps a row with the next one (text/layout/tables.rs) and starts a row in the next column, the next frame or the first
//! frame on the next (odd, even) page (rest-c1; oracle keepopts-rowstart).
use super::*;

pub(crate) const STARTS: [&str; 6] = ["Anywhere", "NextColumn", "NextFrame", "NextPage", "NextOddPage", "NextEvenPage"];

pub(crate) fn set(w: &mut Work<'_>, table: &str, rows: &[usize], keep_with_next: &Option<bool>, start: &Option<String>) -> Result<(), EditError> {
    if rows.is_empty() { return Err(EditError::new("invalid", "choose the rows")); }
    if let Some(s) = start && !STARTS.contains(&s.as_str()) {
        return Err(EditError::new("invalid", format!("start row: one of {}", STARTS.join(", "))));
    }
    let mut x = TableX::read(w, table)?;
    for &r in rows {
        let row = x.rows.get_mut(r).ok_or_else(|| EditError::new("invalid", format!("no row {r}")))?;
        if let Some(k) = keep_with_next {
            // InDesign writes the attribute only when it is on (false is the default)
            if *k { put(&mut row.attrs, "KeepWithNextRow", "true".into()); } else { row.attrs.retain(|(n, _)| n != "KeepWithNextRow"); }
        }
        if let Some(s) = start {
            if s == "Anywhere" { row.attrs.retain(|(n, _)| n != "StartRow"); } else { put(&mut row.attrs, "StartRow", s.clone()); }
        }
    }
    x.write(w)
}
