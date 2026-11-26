//! Cell and table style groups on the oracle `tablestyles` fixture: what the measured scenarios (cs-group-*, cs-duplicate) do not reach, and
//! that every step undoes byte for byte. The measured shapes are held by `oracle.rs`.
use folio_edit::{Command, Editor, TableStyleKind};
use std::collections::BTreeMap;

fn fixture() -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/tablestyles.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if !e.is_dir() {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut e, &mut s).unwrap();
            out.insert(e.name().unwrap().into_owned(), s);
        }
    }
    out
}

const CELL: TableStyleKind = TableStyleKind::Cell;
const TABLE: TableStyleKind = TableStyleKind::Table;
const ORANGE: &str = "CellStyle/Oracle Cell Orange";
const STYLES: &str = "Resources/Styles.xml";

fn run(ed: &mut Editor, c: Command) {
    ed.apply("step", &[c]).unwrap_or_else(|e| panic!("{e}"));
}
fn group(kind: TableStyleKind, name: &str, parent: Option<&str>) -> Command {
    Command::CreateTableStyleGroup { kind, name: name.into(), parent: parent.map(str::to_owned) }
}
fn mv(kind: TableStyleKind, style: &str, to: Option<&str>) -> Command {
    Command::MoveTableStyle { kind, style: style.into(), group: to.map(str::to_owned) }
}
fn styles(ed: &Editor) -> String {
    ed.parts()[STYLES].clone()
}

#[test]
fn a_style_moves_into_a_group_and_back_out() {
    let before = fixture();
    let mut ed = Editor::new(before.clone());
    run(&mut ed, group(CELL, "Grp", None));
    run(&mut ed, mv(CELL, ORANGE, Some("CellStyleGroup/$ID/Grp")));
    assert!(styles(&ed).contains("Self=\"CellStyle/Grp%3aOracle Cell Orange\""));
    assert!(!styles(&ed).contains("Self=\"CellStyle/Oracle Cell Orange\""));
    // out again: the group is an empty element again and the style is at the end of the top level
    run(&mut ed, mv(CELL, "CellStyle/Grp%3aOracle Cell Orange", None));
    let t = styles(&ed);
    assert!(t.contains("Self=\"CellStyle/Oracle Cell Orange\"") && !t.contains("Grp%3a"));
    assert!(t.contains("<CellStyleGroup Self=\"CellStyleGroup/$ID/Grp\" Name=\"$ID/Grp\" />"), "{t}");
    for _ in 0..3 {
        ed.undo().unwrap();
    }
    assert_eq!(ed.parts(), &before, "undone");
}

#[test]
fn nested_groups_rename_with_their_styles_and_delete() {
    let mut ed = Editor::new(fixture());
    run(&mut ed, group(CELL, "A", None));
    run(&mut ed, group(CELL, "B", Some("CellStyleGroup/$ID/A")));
    run(&mut ed, mv(CELL, ORANGE, Some("CellStyleGroup/$ID/A%3aB")));
    run(&mut ed, Command::RenameTableStyleGroup { kind: CELL, group: "CellStyleGroup/$ID/A".into(), name: "Z".into() });
    let t = styles(&ed);
    assert!(t.contains("Self=\"CellStyleGroup/$ID/Z%3aB\" Name=\"$ID/Z:B\""), "{t}");
    assert!(t.contains("Self=\"CellStyle/Z%3aB%3aOracle Cell Orange\"") && t.contains("Name=\"Z:B:Oracle Cell Orange\""));
    assert!(t.contains("<BasedOn type=\"object\">CellStyle/Z%3aB%3aOracle Cell Orange</BasedOn>"), "the child follows");
    // a cell style group with styles in it is deleted with its styles (cs-group-delete); a table style group with styles is not
    let before_delete = styles(&ed);
    run(&mut ed, Command::DeleteTableStyleGroup { kind: CELL, group: "CellStyleGroup/$ID/Z".into() });
    let t = styles(&ed);
    assert!(!t.contains("Oracle Cell Orange") && !t.contains("StyleGroup Self=\"CellStyleGroup"), "{t}");
    assert!(t.contains("Oracle Cell Child") && t.contains("FillColor=\"Color/Oracle Orange\""), "the child keeps the fill of its base");
    ed.undo().unwrap();
    assert_eq!(styles(&ed), before_delete, "undone");
    run(&mut ed, mv(CELL, "CellStyle/Z%3aB%3aOracle Cell Orange", None));
    run(&mut ed, Command::DeleteTableStyleGroup { kind: CELL, group: "CellStyleGroup/$ID/Z".into() });
    assert!(!styles(&ed).contains("StyleGroup Self=\"CellStyleGroup"), "the groups are gone");
    run(&mut ed, group(TABLE, "Tg", None));
    run(&mut ed, mv(TABLE, "TableStyle/Oracle Zebra", Some("TableStyleGroup/$ID/Tg")));
    let err = ed.apply("x", &[Command::DeleteTableStyleGroup { kind: TABLE, group: "TableStyleGroup/$ID/Tg".into() }]).unwrap_err();
    assert_eq!(err.code, "unsupported");
}

#[test]
fn a_table_style_moves_with_the_tables_that_have_it_and_a_copy_stays_in_its_group() {
    let mut ed = Editor::new(fixture());
    run(&mut ed, group(TABLE, "Tg", None));
    run(&mut ed, mv(TABLE, "TableStyle/Oracle Zebra", Some("TableStyleGroup/$ID/Tg")));
    let t = styles(&ed);
    assert!(t.contains("Self=\"TableStyle/Tg%3aOracle Zebra\"") && t.contains("Name=\"Tg:Oracle Zebra\""));
    run(&mut ed, Command::DuplicateTableStyle { kind: TABLE, style: "TableStyle/Tg%3aOracle Zebra".into(), name: None });
    let t = styles(&ed);
    assert!(t.contains("Self=\"TableStyle/Tg%3aOracle Zebra copy\"") && t.contains("Name=\"Tg:Oracle Zebra copy\""), "{t}");
    // the same name twice is refused, and a name with a colon
    assert!(ed.apply("x", &[Command::DuplicateTableStyle { kind: TABLE, style: "TableStyle/Tg%3aOracle Zebra".into(), name: Some("Oracle Zebra copy".into()) }]).is_err());
    assert!(ed.apply("x", &[group(TABLE, "a:b", None)]).is_err());
    // a style is created in a group, named after it
    run(&mut ed, Command::CreateTableStyle { kind: TABLE, name: "Inner".into(), props: BTreeMap::new(), group: Some("TableStyleGroup/$ID/Tg".into()) });
    assert!(styles(&ed).contains("Self=\"TableStyle/Tg%3aInner\""));
}

#[test]
fn a_style_in_a_group_is_renamed_inside_its_group() {
    let mut ed = Editor::new(fixture());
    run(&mut ed, group(CELL, "Grp", None));
    run(&mut ed, mv(CELL, ORANGE, Some("CellStyleGroup/$ID/Grp")));
    run(&mut ed, Command::EditTableStyle { kind: CELL, style: "CellStyle/Grp%3aOracle Cell Orange".into(), rename: Some("Renamed".into()), props: BTreeMap::new() });
    let t = styles(&ed);
    assert!(t.contains("Self=\"CellStyle/Grp%3aRenamed\"") && t.contains("Name=\"Grp:Renamed\""), "{t}");
}
