//! The Table menu (next/oracle/scenarios/tables.json). A table is a `Table` element in its story
//! (one character of the text). Its `Row`s and `Column`s are named by position (Self = the
//! table's Self + "Row"/"Column" + index, Name = the index) and its `Cell`s "column:row", in row
//! then column order; a merged cell spans `RowSpan` × `ColumnSpan` and the cells it covers are
//! left out. New cells take the table's Self + "i" + the next hex number; a new table the
//! story's Self + "i" + the document's next number. Every change rewrites the whole table
//! element, so positions and names always agree.
//!
//! Measured rules: an inserted row or column copies the one it is inserted beside (attributes;
//! its cells copy the neighbour's cell attributes, empty text in the neighbour's paragraph
//! style); a column insert widens the table. Merging joins the non-empty cells' text with
//! paragraph ends (row by row) into the first cell; unmerging gives the covered cells back their
//! ids (InDesign keeps them), empty. Splitting a cell across halves its row's height for both
//! rows (their minimum) or its column's width, and the other cells of that row or column span
//! both. More header rows are new empty rows (minimum height 3 pt). Auto-grow rows' heights
//! come from composing them (`SetRowHeights`, folio-text tables).
use std::collections::BTreeMap;

use super::*;

pub(super) mod celltype;
pub(crate) mod rowkeeps;
pub(super) mod cells;
pub(super) mod styles;

/// InDesign 2025's table size limits (`Table.add` parameters and the Table Setup properties; `measured/tablelimits-2025.json`): body rows
/// 1-10000, columns 1-200, header and footer rows 0-25 each. Inserting rows or columns and splitting cells have no limit of their own.
pub const MAX_BODY_ROWS: u32 = 10_000;
pub const MAX_COLUMNS: u32 = 200;
pub const MAX_HEADER_FOOTER_ROWS: u32 = 25;

pub use cells::DiagonalLines;

mod model;
mod story_table;
mod grid;
mod cell_spans;
mod dimensions;
mod cell_format;
mod row_kinds;

use model::{Line, CellX, TableX, get, put, put_before, empty_like, empty_cell_text, has_text, render};
pub(crate) use model::table_info;
pub(super) use story_table::{insert_table, convert_to_table, to_text, delete_table};
pub(super) use grid::{insert_rows, insert_columns, paste_rows, paste_columns, delete_rows, delete_columns};
pub(super) use cell_spans::{merge_cells, unmerge_cell, split_cell};
pub(super) use dimensions::{set_rows, set_columns, drag_line, distribute_rows, distribute_columns, set_row_heights};
use cell_format::opposite;
pub(super) use cell_format::set_cells;
pub use cell_format::{CellChange, EdgeStroke};
pub(super) use row_kinds::{set_header_rows, set_footer_rows, convert_rows};
pub use row_kinds::RowKind;
