//! Table size limits against InDesign 2025's (`next/oracle/measured/tablelimits-2025.json`, probe `next/oracle/indesign/tablelimit-probe.jsx`):
//! a table is made with 1 to 10000 body rows, 1 to 200 columns and 0 to 25 header and 0 to 25 footer rows (each its own range, the sum is not
//! limited); inserting rows or columns and splitting cells have no limit of their own.

use folio_edit::xml::Index;
use folio_edit::{Command, Editor};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

fn fixture() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/tables.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        out.insert(e.name().unwrap().into_owned(), s);
    }
    out
}

/// The Self of the frame labelled `oracle.id` = `tbl-new` (the empty text frame of the oracle's `tables` fixture).
fn new_frame(parts: &BTreeMap<String, String>) -> String {
    for (name, t) in parts {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Key") == Some("oracle.id") && ix.value(t, i, "Value").as_deref() == Some("tbl-new") {
                let owner = ix.ancestors(i).find(|&a| ix.attr(t, a, "Self").is_some()).unwrap();
                return ix.attr(t, owner, "Self").unwrap().to_owned();
            }
        }
    }
    panic!("no frame labelled tbl-new");
}

fn story_of(ed: &Editor) -> String {
    let frame = new_frame(ed.parts());
    ed.parts().values().find_map(|t| {
        let ix = Index::scan(t).ok()?;
        let i = ix.by_self(t, &frame)?;
        ix.attr(t, i, "ParentStory").map(str::to_owned)
    }).expect("the frame's story")
}

fn table_in(ed: &Editor, story: &str) -> Option<String> {
    let t = &ed.parts()[&format!("Stories/Story_{story}.xml")];
    let ix = Index::scan(t).unwrap();
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Table").and_then(|i| ix.attr(t, i, "Self")).map(str::to_owned)
}

fn insert(ed: &mut Editor, story: &str, body: u32, columns: u32, header: u32, footer: u32) -> Result<(), String> {
    ed.apply("insert", &[Command::InsertTable { story: story.to_owned(), at: None, rows: body, columns, header, footer }]).map(|_| ()).map_err(|e| e.to_string())
}

fn rows_of(ed: &Editor, table: &str) -> (usize, usize) {
    let part = ed.parts().values().find(|t| t.contains(&format!("Self=\"{table}\""))).unwrap();
    (part.matches("<Row ").count(), part.matches("<Column ").count())
}

#[test]
fn a_table_is_made_up_to_the_limits_of_insert_table_in_indesign() {
    let base = fixture();
    let measured: Value = serde_json::from_str(include_str!("../../../../oracle/measured/tablelimits-2025.json")).unwrap();
    let mut checked = 0;
    for c in measured["create"].as_array().unwrap() {
        let n = |k: &str| c[k].as_u64().unwrap() as u32;
        let (body, columns, header, footer) = (n("body"), n("columns"), n("header"), n("footer"));
        let mut ed = Editor::new(base.clone());
        let story = story_of(&ed);
        let made = insert(&mut ed, &story, body, columns, header, footer);
        let what = format!("{body} body, {columns} columns, {header} header, {footer} footer rows");
        assert_eq!(made.is_ok(), c["ok"].as_bool().unwrap(), "{what}: {made:?}");
        if made.is_ok() {
            let table = table_in(&ed, &story).unwrap();
            assert_eq!(rows_of(&ed, &table), ((body + header + footer) as usize, columns as usize), "{what}");
            let after = ed.parts().clone();
            ed.undo().unwrap();
            assert_eq!(ed.parts(), &base, "{what}: undo");
            ed.redo().unwrap();
            assert_eq!(ed.parts(), &after, "{what}: redo");
        } else {
            assert_eq!(ed.parts(), &base, "{what}: a refused table leaves the document as it was");
        }
        checked += 1;
    }
    assert!(checked >= 18, "{checked}");
}

#[test]
fn rows_and_columns_are_inserted_past_the_table_size_limits_as_in_indesign() {
    let base = fixture();
    let mut ed = Editor::new(base);
    let story = story_of(&ed);
    insert(&mut ed, &story, 9_999, 1, 0, 0).unwrap();
    let table = table_in(&ed, &story).unwrap();
    // InDesign: Rows.add has no limit (10000 rows, then 10001 and on: 13000 were reached)
    ed.apply("rows", &[Command::InsertRows { table: table.clone(), at: 0, count: 1, before: false }]).unwrap();
    ed.apply("rows more", &[Command::InsertRows { table: table.clone(), at: 0, count: 2, before: false }]).unwrap();
    assert_eq!(rows_of(&ed, &table), (10_002, 1));
    // ... and Columns.add the same (200 columns, then 251)
    let mut wide = Editor::new(fixture());
    let story = story_of(&wide);
    insert(&mut wide, &story, 2, 199, 0, 0).unwrap();
    let table = table_in(&wide, &story).unwrap();
    wide.apply("columns", &[Command::InsertColumns { table: table.clone(), at: 0, count: 1, before: false }]).unwrap();
    wide.apply("columns more", &[Command::InsertColumns { table: table.clone(), at: 0, count: 51, before: true }]).unwrap();
    assert_eq!(rows_of(&wide, &table), (2, 251));
    // Cell.split is not limited either (a 10000-row table's cell gives row 10001)
    let mut small = Editor::new(fixture());
    let story = story_of(&small);
    insert(&mut small, &story, 10_000, 1, 0, 0).unwrap();
    let table = table_in(&small, &story).unwrap();
    small.apply("split", &[Command::SplitCell { table: table.clone(), cell: [0, 0], vertical: false }]).unwrap();
    assert_eq!(rows_of(&small, &table), (10_001, 1));
    // the one-step count only keeps the Table Options field reachable: from one row to 10000 body rows
    let mut one = Editor::new(fixture());
    let story = story_of(&one);
    insert(&mut one, &story, 1, 1, 0, 0).unwrap();
    let table = table_in(&one, &story).unwrap();
    one.apply("grow", &[Command::InsertRows { table: table.clone(), at: 0, count: 9_999, before: false }]).unwrap();
    assert_eq!(rows_of(&one, &table), (10_000, 1));
}

/// A table of the most body rows lays out (the model reads it, its rows break over the frame's pages) without a crash or a blow-up; the time is
/// printed (`--nocapture`). No speed work is done for it, the bound only catches a hang.
#[test]
fn a_ten_thousand_row_table_lays_out() {
    let mut ed = Editor::new(fixture());
    let story = story_of(&ed);
    let started = std::time::Instant::now();
    insert(&mut ed, &story, 10_000, 2, 1, 1).unwrap();
    let made = started.elapsed();
    let doc = folio_idml::read_parts(ed.parts(), std::path::Path::new("tablelimit.idml"), &Default::default()).unwrap();
    let read = started.elapsed();
    let fonts = folio_text::FontStore::discover(&[]);
    let layout = folio_text::layout_document(&doc, &fonts);
    let laid = started.elapsed();
    eprintln!("10000-row table: insert {made:?}, read model {:?}, layout {:?} ({} frames, overset {:?})", read - made, laid - read, layout.frames.len(), layout.overset.len());
    assert!(laid.as_secs() < 120, "{laid:?}");
}
