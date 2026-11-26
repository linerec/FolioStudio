//! Table Options ▸ Row Strokes / Column Strokes / Fills (oracle tables-alternating measures what InDesign writes; here the limits, the
//! table style the settings are compared with, reading the settings back and undo) on the tables fixture.
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, String) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/tables.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(name, t);
    }
    let table = parts.values().find_map(|t| {
        let ix = Index::scan(t).unwrap();
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Table").and_then(|i| ix.attr(t, i, "Self").map(str::to_owned))
    });
    (Editor::new(parts), table.unwrap())
}

fn set(table: &str, pairs: &[(&str, &str)]) -> Command {
    Command::SetTableOptions { table: table.into(), options: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect() }
}

/// The attribute `name` of the table element, as written in the part.
fn attr(e: &Editor, table: &str, name: &str) -> Option<String> {
    e.parts().values().find_map(|t| {
        let ix = Index::scan(t).ok()?;
        let i = ix.by_self(t, table)?;
        ix.attr(t, i, name).map(str::to_owned)
    })
}

#[test]
fn a_setting_is_written_where_it_differs_from_the_default_and_taken_away_where_it_does_not() {
    let (mut e, t) = fixture();
    e.apply("fill", &[set(&t, &[("StartRowFillCount", "1"), ("EndRowFillCount", "1"), ("StartRowFillColor", "Color/Oracle Orange"), ("StartRowFillTint", "20")])]).unwrap();
    assert_eq!(attr(&e, &t, "StartRowFillCount").as_deref(), Some("1"));
    assert_eq!(attr(&e, &t, "StartRowFillColor").as_deref(), Some("Color/Oracle Orange"));
    // the default tint (20) is not written
    assert_eq!(attr(&e, &t, "StartRowFillTint"), None);
    // set back to the default: gone
    e.apply("off", &[set(&t, &[("StartRowFillCount", "0"), ("EndRowFillCount", "0"), ("StartRowFillColor", "Color/Black")])]).unwrap();
    for k in ["StartRowFillCount", "EndRowFillCount", "StartRowFillColor"] {
        assert_eq!(attr(&e, &t, k), None, "{k}");
    }
    // numbers read as numbers: "2.0" is 2, "0.250" is the end weight's default
    e.apply("w", &[set(&t, &[("StartRowStrokeCount", "2.0"), ("EndRowStrokeWeight", "0.250")])]).unwrap();
    assert_eq!(attr(&e, &t, "StartRowStrokeCount").as_deref(), Some("2"));
    assert_eq!(attr(&e, &t, "EndRowStrokeWeight"), None);
}

#[test]
fn the_end_column_stroke_type_is_a_line_style() {
    let (mut e, t) = fixture();
    e.apply("type", &[set(&t, &[("EndColumnLineStyle", "StrokeStyle/$ID/Dashed"), ("StartColumnStrokeType", "StrokeStyle/$ID/Canned Dotted")])]).unwrap();
    assert_eq!(attr(&e, &t, "EndColumnLineStyle").as_deref(), Some("StrokeStyle/$ID/Dashed"));
    assert_eq!(attr(&e, &t, "StartColumnStrokeType").as_deref(), Some("StrokeStyle/$ID/Canned Dotted"));
    // there is no EndColumnStrokeType
    assert!(e.apply("bad", &[set(&t, &[("EndColumnStrokeType", "StrokeStyle/$ID/Dashed")])]).unwrap_err().to_string().contains("not a table pattern setting"));
}

#[test]
fn what_the_dialog_does_not_accept_is_refused_and_nothing_changes() {
    let (mut e, t) = fixture();
    let before = e.parts().clone();
    for (k, v) in [("StartRowFillTint", "150"), ("StartRowFillTint", "-1"), ("StartRowFillCount", "-1"), ("StartRowFillCount", "1.5"), ("StartRowStrokeWeight", "801"), ("SkipFirstAlternatingFillRows", "1000"),
        ("ColumnFillsPriority", "yes"), ("StartRowStrokeColor", "Black"), ("StartRowStrokeType", "Dashed"), ("StartRowFillCount", "")] {
        assert!(e.apply("bad", &[set(&t, &[(k, v)])]).is_err(), "{k}={v} should be refused");
    }
    // a table that is not one
    assert!(e.apply("bad", &[set("nope", &[("StartRowFillCount", "1")])]).is_err());
    // one bad setting stops the others of the same command
    assert!(e.apply("bad", &[set(&t, &[("StartRowFillCount", "1"), ("StartRowFillTint", "101")])]).is_err());
    assert_eq!(e.parts(), &before);
}

#[test]
fn a_setting_equal_to_the_applied_table_styles_goes_away_and_one_that_differs_is_written() {
    let (e, t) = fixture();
    // a table style with a pattern (Zebra: every other row), the table using it
    let mut parts = e.parts().clone();
    let styles = parts.get_mut("Resources/Styles.xml").unwrap();
    let at = styles.find("<TableStyle Self=\"TableStyle/$ID/[Basic Table]\"").unwrap();
    styles.insert_str(at, "<TableStyle Self=\"TableStyle/Zebra\" Name=\"Zebra\" StartRowFillCount=\"1\" EndRowFillCount=\"1\" StartRowFillTint=\"40\"><Properties><BasedOn type=\"string\">$ID/[No table style]</BasedOn></Properties></TableStyle>\n");
    for text in parts.values_mut() {
        if text.contains(&format!("Self=\"{t}\"")) {
            *text = text.replacen("AppliedTableStyle=\"TableStyle/$ID/[No table style]\"", "AppliedTableStyle=\"TableStyle/Zebra\"", 1);
        }
    }
    let mut e2 = Editor::new(parts);
    // the table has the style's settings
    let now = e2.table_options(&t).unwrap();
    assert_eq!((now["StartRowFillCount"].as_str(), now["StartRowFillTint"].as_str(), now["StartRowFillColor"].as_str()), ("1", "40", "Color/Black"));
    // the style's value is not written again; another is; turning the pattern off writes the zero the style would otherwise override
    e2.apply("same", &[set(&t, &[("StartRowFillCount", "1"), ("StartRowFillTint", "40")])]).unwrap();
    assert_eq!((attr(&e2, &t, "StartRowFillCount"), attr(&e2, &t, "StartRowFillTint")), (None, None));
    e2.apply("off", &[set(&t, &[("StartRowFillCount", "0"), ("EndRowFillCount", "0"), ("StartRowFillTint", "20")])]).unwrap();
    assert_eq!((attr(&e2, &t, "StartRowFillCount").as_deref(), attr(&e2, &t, "EndRowFillCount").as_deref(), attr(&e2, &t, "StartRowFillTint").as_deref()), (Some("0"), Some("0"), Some("20")));
    assert_eq!(e2.table_options(&t).unwrap()["StartRowFillCount"], "0");
}

#[test]
fn the_settings_are_read_back_with_their_defaults_and_one_undo_takes_the_change_back() {
    let (mut e, t) = fixture();
    let o = e.table_options(&t).unwrap();
    assert_eq!(o["StartRowStrokeWeight"], "1");
    assert_eq!(o["EndRowStrokeWeight"], "0.25");
    assert_eq!(o["EndRowFillColor"], "Swatch/None");
    assert_eq!(o["StartColumnFillTint"], "20");
    assert_eq!(o["ColumnFillsPriority"], "false");
    assert_eq!(o["EndColumnStrokeGapColor"], "Color/Paper");
    assert!(e.table_options("nope").is_none());
    let before = e.parts().clone();
    e.apply("alt", &[set(&t, &[("StartRowStrokeCount", "1"), ("EndRowStrokeCount", "1"), ("StartRowStrokeWeight", "3"), ("SkipFirstAlternatingStrokeRows", "1")])]).unwrap();
    assert_eq!(e.table_options(&t).unwrap()["StartRowStrokeWeight"], "3");
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
    e.redo().unwrap();
    assert_eq!(attr(&e, &t, "SkipFirstAlternatingStrokeRows").as_deref(), Some("1"));
}
