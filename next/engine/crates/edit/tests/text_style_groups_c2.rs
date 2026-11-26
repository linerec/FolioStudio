//! Rename Style Group and Delete Style Group of the paragraph and character styles: what the measured scenarios (oracle tsg-*) do not reach —
//! the refusals, the references of nested/GREP/drop cap styles, one undo step — on the oracle fixtures `text-format` and `nested-styles`.
use folio_edit::{Command, Editor};
use std::collections::BTreeMap;

fn fixture(name: &str) -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../../oracle/fixtures/{name}.idml"));
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

fn cmd(wire: serde_json::Value) -> Command {
    serde_json::from_value(wire).unwrap()
}
fn run(ed: &mut Editor, wire: serde_json::Value) {
    ed.apply("step", &[cmd(wire)]).unwrap_or_else(|e| panic!("{e}"));
}
fn refused(ed: &mut Editor, wire: serde_json::Value) -> String {
    let before = ed.parts().clone();
    let err = ed.apply("step", &[cmd(wire)]).expect_err("refused");
    assert_eq!(ed.parts(), &before, "a refused step changes nothing");
    err.to_string()
}
fn group(ed: &mut Editor, kind: &str, name: &str, parent: &str) {
    run(ed, serde_json::json!({"op": "createTextStyleGroup", "kind": kind, "name": name, "parent": parent}));
}
fn mv(ed: &mut Editor, kind: &str, style: &str, to: &str) {
    run(ed, serde_json::json!({"op": "moveTextStyle", "kind": kind, "style": style, "group": to}));
}
const STYLES: &str = "Resources/Styles.xml";

#[test]
fn rename_refuses_what_the_dom_refuses_and_a_same_name_changes_nothing() {
    let mut ed = Editor::new(fixture("text-format"));
    group(&mut ed, "paragraph", "Heads", "");
    group(&mut ed, "paragraph", "Other", "");
    mv(&mut ed, "paragraph", "Format Heading", "Heads");
    // a group beside it, a style of the same place, brackets, a colon, nothing
    for name in ["Other", "Format Body", "[Brackets]", "A:B", " ", ""] {
        refused(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "paragraph", "group": "Heads", "name": name}));
    }
    refused(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "paragraph", "group": "Nope", "name": "X"}));
    refused(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "paragraph", "group": "", "name": "X"}));
    refused(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "table", "group": "Heads", "name": "X"}));
    let before = ed.parts().clone();
    run(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "paragraph", "group": "Heads", "name": "Heads"}));
    assert_eq!(ed.parts(), &before, "the same name is no change");
    // a group of another kind with the same name is a different place (the DOM allows it)
    group(&mut ed, "character", "Heads", "");
    run(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "paragraph", "group": "Heads", "name": "Titles"}));
    assert!(ed.parts()[STYLES].contains("ParagraphStyle/Titles%3aFormat Heading"));
    assert!(ed.parts()[STYLES].contains("CharacterStyleGroup/$ID/Heads"));
}

#[test]
fn rename_and_delete_are_one_undo_step() {
    let before = fixture("text-format");
    let mut ed = Editor::new(before.clone());
    group(&mut ed, "paragraph", "Heads", "");
    mv(&mut ed, "paragraph", "Format Heading", "Heads");
    let grouped = ed.parts().clone();
    run(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "paragraph", "group": "Heads", "name": "Titles"}));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &grouped, "one undo undoes the rename");
    run(&mut ed, serde_json::json!({"op": "deleteTextStyleGroup", "kind": "paragraph", "group": "Heads"}));
    assert!(!ed.parts()[STYLES].contains("Format Heading"));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &grouped, "one undo undoes the delete: the text has its style again");
    ed.redo().unwrap();
    assert!(!ed.parts()[STYLES].contains("Format Heading"));
}

#[test]
fn delete_refuses_a_group_that_is_not_there_and_deleting_the_empty_one_leaves_the_rest() {
    let mut ed = Editor::new(fixture("text-format"));
    refused(&mut ed, serde_json::json!({"op": "deleteTextStyleGroup", "kind": "paragraph", "group": "Nope"}));
    refused(&mut ed, serde_json::json!({"op": "deleteTextStyleGroup", "kind": "paragraph", "group": ""}));
    refused(&mut ed, serde_json::json!({"op": "deleteTextStyleGroup", "kind": "table", "group": "X"}));
    group(&mut ed, "paragraph", "A", "");
    group(&mut ed, "paragraph", "Sub", "A");
    group(&mut ed, "paragraph", "Keep", "");
    run(&mut ed, serde_json::json!({"op": "deleteTextStyleGroup", "kind": "paragraph", "group": "A:Sub"}));
    let t = &ed.parts()[STYLES];
    assert!(t.contains("ParagraphStyleGroup/$ID/A\"") && !t.contains("A%3aSub") && t.contains("ParagraphStyleGroup/$ID/Keep"), "{t}");
}

#[test]
fn nested_grep_and_drop_cap_references_follow_a_rename_and_become_none_on_delete() {
    // `nested-styles`: paragraph styles with nested styles that name the character styles Red, Bold and Blue
    let mut ed = Editor::new(fixture("nested-styles"));
    let uses = |t: &str, id: &str| t.matches(&format!(">{id}<")).count() + t.matches(&format!("=\"{id}\"")).count();
    let before = uses(&ed.parts()[STYLES], "CharacterStyle/Red");
    assert!(before > 0, "the fixture's nested styles use Red");
    group(&mut ed, "character", "Marks", "");
    mv(&mut ed, "character", "Red", "Marks");
    assert_eq!(uses(&ed.parts()[STYLES], "CharacterStyle/Marks%3aRed"), before, "the move already follows");
    run(&mut ed, serde_json::json!({"op": "renameTextStyleGroup", "kind": "character", "group": "Marks", "name": "Emphasis"}));
    assert_eq!(uses(&ed.parts()[STYLES], "CharacterStyle/Emphasis%3aRed"), before, "the nested styles name the renamed style");
    assert_eq!(uses(&ed.parts()[STYLES], "CharacterStyle/Marks%3aRed"), 0);
    let renamed = ed.parts().clone();
    run(&mut ed, serde_json::json!({"op": "deleteTextStyleGroup", "kind": "character", "group": "Emphasis"}));
    let t = &ed.parts()[STYLES];
    assert_eq!(uses(t, "CharacterStyle/Emphasis%3aRed"), 0, "no reference to the deleted style is left");
    assert!(uses(t, "CharacterStyle/$ID/[No character style]") >= before, "the nested styles name [No character style] now");
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &renamed);
}
