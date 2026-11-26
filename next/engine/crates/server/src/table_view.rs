//! Story Editor table view: how the Story Editor lists the cells of a table, by rows or by columns
//! (InDesign 2025: the table icon's menu items "Arrange by rows" / "Arrange by columns", rules/arrange-c1.ko.md).
//!
//! The order is a view setting of one table in the open document: it is not an edit (no undo step, the document
//! does not become modified), it is not in the IDML or the INDD (measured: save, close and open again gives rows),
//! and it stays while the document stays open. It therefore lives in the document's session state (`EditState`),
//! never in the model.
use folio_model::{Document, Table, TableCell};
use serde_json::{Value, json};

/// The order the Story Editor shows a table's cells in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Rows,
    Columns,
}

impl Order {
    pub fn name(self) -> &'static str {
        match self {
            Order::Rows => "rows",
            Order::Columns => "columns",
        }
    }
}

/// Every table anchored in a story (not looking into cells), in the order of their anchors in the text.
fn tables_of<'d>(doc: &'d Document, story: &str) -> Vec<&'d Table> {
    doc.stories.get(story).map_or_else(Vec::new, |s| s.paragraphs.iter().flat_map(|p| &p.runs).flat_map(|r| &r.tables).collect())
}

/// A table of any story of the document.
fn find<'d>(doc: &'d Document, id: &str) -> Option<&'d Table> {
    doc.stories.keys().find_map(|story| tables_of(doc, story).into_iter().find(|t| t.id == id))
}

/// The cells of a table in the order the Story Editor shows them (a merged cell once, at its first position).
/// `Rows`: row after row, left to right. `Columns`: column after column, top to bottom.
pub fn cells_in_order(table: &Table, order: Order) -> Vec<&TableCell> {
    let mut cells: Vec<&TableCell> = table.cells.iter().collect();
    match order {
        Order::Rows => cells.sort_by_key(|c| (c.row, c.column)),
        Order::Columns => cells.sort_by_key(|c| (c.column, c.row)),
    }
    cells
}

/// The tables of a story with their cells in display order. `group` counts the dashed divider the cell sits under
/// (a new row, or a new column); `label` is the number InDesign writes on it: the row's number, and in the column view
/// "Column 1" on every divider (measured on a 3 x 3 table: InDesign 2025 writes 1 on all three).
pub fn story_tables(doc: &Document, story: &str, views: &std::collections::BTreeMap<String, Order>) -> Value {
    let tables: Vec<Value> = tables_of(doc, story)
        .into_iter()
        .map(|t| {
            let order = views.get(&t.id).copied().unwrap_or_default();
            let mut group = 0usize;
            let mut last: Option<usize> = None;
            let cells: Vec<Value> = cells_in_order(t, order)
                .into_iter()
                .map(|c| {
                    let key = if order == Order::Rows { c.row } else { c.column };
                    let first = last != Some(key);
                    if first {
                        group += 1;
                        last = Some(key);
                    }
                    json!({"id": c.id, "row": c.row, "column": c.column, "group": group - 1, "label": if order == Order::Rows { c.row + 1 } else { 1 }, "first": first})
                })
                .collect();
            json!({"id": t.id, "order": order.name(), "rows": t.rows.len(), "columns": t.columns.len(), "cells": cells})
        })
        .collect();
    json!({"tables": tables})
}

/// `arrangeByRows` / `arrangeByColumns`: set the order of one table (`table` is its id); the answer says what changed.
pub fn arrange(req: &Value, session: Option<&crate::Session>, order: Order) -> Result<Value, String> {
    let session = session.ok_or("no document open")?;
    let id = req["table"].as_str().ok_or("table required")?;
    if find(&session.doc, id).is_none() {
        return Err(format!("table not found: {id}"));
    }
    let shared = session.edit.as_ref().ok_or("the Story Editor view of a table needs an editable document")?;
    let mut state = shared.lock().map_err(|_| "edit state unavailable")?;
    let before = state.table_views.get(id).copied().unwrap_or_default();
    state.table_views.insert(id.to_owned(), order);
    Ok(json!({"table": id, "order": order.name(), "was": before.name(), "changed": before != order}))
}

/// `storyTables`: the tables of a story and how the Story Editor lists them now (a read-only document: all by rows).
pub fn query(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let session = session.ok_or("no document open")?;
    let story = req["story"].as_str().ok_or("story required")?;
    let empty = Default::default();
    match session.edit.as_ref() {
        Some(shared) => {
            let state = shared.lock().map_err(|_| "edit state unavailable")?;
            Ok(story_tables(&session.doc, story, &state.table_views))
        }
        None => Ok(story_tables(&session.doc, story, &empty)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_model::{Paragraph, Story, TextRun};

    fn table(id: &str, rows: usize, columns: usize) -> Table {
        let mut t = Table { id: id.into(), rows: vec![Default::default(); rows], columns: vec![50.0; columns], ..Default::default() };
        // row-major, as the model keeps them
        for r in 0..rows {
            for c in 0..columns {
                t.cells.push(TableCell { id: format!("{id}c{r}{c}"), row: r, column: c, row_span: 1, column_span: 1, ..Default::default() });
            }
        }
        t
    }

    fn doc_with(t: Table) -> Document {
        let mut doc = Document::default();
        let story = Story { id: "s".into(), paragraphs: vec![Paragraph { runs: vec![TextRun { tables: vec![t], ..Default::default() }], ..Default::default() }], ..Default::default() };
        doc.stories.insert("s".into(), story);
        doc
    }

    fn ids(v: &Value) -> Vec<String> {
        v["tables"][0]["cells"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_owned()).collect()
    }

    #[test]
    fn rows_go_row_after_row_and_columns_column_after_column() {
        let doc = doc_with(table("t", 3, 3));
        let by_rows = story_tables(&doc, "s", &Default::default());
        assert_eq!(by_rows["tables"][0]["order"], "rows");
        assert_eq!(ids(&by_rows), ["tc00", "tc01", "tc02", "tc10", "tc11", "tc12", "tc20", "tc21", "tc22"]);
        let mut views = std::collections::BTreeMap::new();
        views.insert("t".to_owned(), Order::Columns);
        let by_columns = story_tables(&doc, "s", &views);
        assert_eq!(by_columns["tables"][0]["order"], "columns");
        assert_eq!(ids(&by_columns), ["tc00", "tc10", "tc20", "tc01", "tc11", "tc21", "tc02", "tc12", "tc22"]);
    }

    #[test]
    fn dividers_count_rows_and_every_column_divider_says_one() {
        let doc = doc_with(table("t", 2, 3));
        let rows = story_tables(&doc, "s", &Default::default());
        let c = rows["tables"][0]["cells"].as_array().unwrap();
        assert_eq!((c[0]["group"].clone(), c[2]["group"].clone(), c[3]["group"].clone()), (json!(0), json!(0), json!(1)));
        assert_eq!((c[0]["label"].clone(), c[3]["label"].clone()), (json!(1), json!(2)));
        assert_eq!((c[0]["first"].clone(), c[1]["first"].clone(), c[3]["first"].clone()), (json!(true), json!(false), json!(true)));
        let mut views = std::collections::BTreeMap::new();
        views.insert("t".to_owned(), Order::Columns);
        let columns = story_tables(&doc, "s", &views);
        let c = columns["tables"][0]["cells"].as_array().unwrap();
        assert_eq!((c[0]["group"].clone(), c[1]["group"].clone(), c[2]["group"].clone(), c[4]["group"].clone()), (json!(0), json!(0), json!(1), json!(2)));
        assert!(c.iter().all(|x| x["label"] == 1));
        assert_eq!((c[0]["first"].clone(), c[1]["first"].clone(), c[2]["first"].clone()), (json!(true), json!(false), json!(true)));
    }

    #[test]
    fn a_merged_cell_is_listed_once_and_other_stories_have_no_tables() {
        let mut t = table("t", 2, 2);
        t.cells.retain(|c| !(c.row == 0 && c.column == 1));
        t.cells[0].column_span = 2;
        let doc = doc_with(t);
        assert_eq!(ids(&story_tables(&doc, "s", &Default::default())), ["tc00", "tc10", "tc11"]);
        assert_eq!(story_tables(&doc, "other", &Default::default())["tables"], json!([]));
        assert!(find(&doc, "t").is_some() && find(&doc, "nope").is_none());
    }
}
