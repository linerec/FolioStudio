//! Command and query adapters for table editing.

use super::*;

/// `dragTableLine`: a row or column line of a table dragged with the Type tool (ops/tables.rs).
pub(crate) fn drag_table_line(w: &mut Work<'_>, table: &str, columns: &bool, line: &usize, by: &f64, shift: &bool, insert: &Option<bool>, size: &Option<f64>, min_before: &Option<f64>, min_after: &Option<f64>) -> Result<(), EditError> {
    tables::drag_line(w, table, *columns, *line, *by, *shift, insert.unwrap_or(false), *size, [*min_before, *min_after])
}

pub(crate) fn table_of(w: &mut Work<'_>, id: &str) -> Result<serde_json::Value, EditError> {
    tables::table_info(w, id)
}

/// Table and Cell Styles panels: Break Link to Style and Clear Attributes Not Defined by Style (registered commands, tables/styles.rs).
pub(crate) fn break_table_style_link(w: &mut Work<'_>, table: &str) -> Result<(), EditError> {
    tables::styles::break_table_link(w, table)
}
pub(crate) fn break_cell_style_link(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]]) -> Result<(), EditError> {
    tables::styles::break_cell_link(w, table, cells)
}
pub(crate) fn clear_cell_style_attributes(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]]) -> Result<(), EditError> {
    tables::styles::clear_cell_attributes(w, table, cells)
}
