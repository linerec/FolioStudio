//! The built-in text styles: what the DOM allows and refuses (oracle probe `artifacts/styles-c2-20261023/measurement/builtin-probe.json`,
//! scenarios `bs-*`). `[Basic Paragraph]` takes values, a base, a next style, nested/GREP/line styles and is duplicated; `[No Paragraph Style]`
//! and `[None]` (the root styles) take nothing; no built-in style is renamed, deleted or moved into a group.
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
/// The step is refused, nothing changes; the message is returned.
fn refused(ed: &mut Editor, wire: serde_json::Value) -> String {
    let before = ed.parts().clone();
    let err = ed.apply("step", &[cmd(wire)]).expect_err("refused");
    assert_eq!(ed.parts(), &before, "a refused step changes nothing");
    err.to_string()
}
const STYLES: &str = "Resources/Styles.xml";
const BASIC: &str = "[Basic Paragraph]";
const ROOT_MESSAGE: &str = "Invalid request on a root style.";

#[test]
fn root_styles_take_nothing() {
    let mut ed = Editor::new(fixture("nested-styles"));
    let steps = [
        serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": "[No Paragraph Style]", "props": {"pointSize": 14}}),
        serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": "[No Paragraph Style]", "props": {"basedOn": "NS 0"}}),
        serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": "[No Paragraph Style]", "props": {"name": "Plain"}}),
        serde_json::json!({"op": "styleEdit", "kind": "character", "style": "[None]", "props": {"pointSize": 14}}),
        serde_json::json!({"op": "styleEdit", "kind": "character", "style": "[None]", "props": {"name": "Plain"}}),
        serde_json::json!({"op": "duplicateTextStyle", "kind": "paragraph", "style": "[No Paragraph Style]", "name": ""}),
        serde_json::json!({"op": "duplicateTextStyle", "kind": "character", "style": "[None]", "name": ""}),
        serde_json::json!({"op": "setNestedStyles", "style": "[No Paragraph Style]", "items": []}),
        serde_json::json!({"op": "setGrepStyles", "style": "[No Paragraph Style]", "rules": []}),
        serde_json::json!({"op": "setLineStyles", "style": "[No Paragraph Style]", "items": []}),
    ];
    for step in steps {
        let message = refused(&mut ed, step.clone());
        assert!(message.contains(ROOT_MESSAGE), "{step}: {message}");
    }
}

#[test]
fn no_built_in_style_is_deleted_renamed_or_moved() {
    let mut ed = Editor::new(fixture("nested-styles"));
    // delete (the replacement may be anything)
    for (kind, style, replacement) in [("paragraph", BASIC, "NS 0"), ("paragraph", "[No Paragraph Style]", BASIC), ("character", "[None]", "Red")] {
        let message = refused(&mut ed, serde_json::json!({"op": "styleDelete", "kind": kind, "style": style, "replacement": replacement}));
        assert!(message.contains("The default style cannot be deleted."), "{style}: {message}");
    }
    // rename: the DOM says "Invalid request on a root style." for the Basic Paragraph style too
    let message = refused(&mut ed, serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": BASIC, "props": {"name": "Plain"}}));
    assert!(message.contains(ROOT_MESSAGE), "{message}");
    // a built-in style does not go into a group
    run(&mut ed, serde_json::json!({"op": "createTextStyleGroup", "kind": "paragraph", "name": "Heads", "parent": ""}));
    for style in [BASIC, "[No Paragraph Style]"] {
        refused(&mut ed, serde_json::json!({"op": "moveTextStyle", "kind": "paragraph", "style": style, "group": "Heads"}));
    }
}

#[test]
fn basic_paragraph_takes_values_a_base_and_is_one_undo_step() {
    let before = fixture("nested-styles");
    let mut ed = Editor::new(before.clone());
    run(&mut ed, serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": BASIC, "props": {"pointSize": 14, "basedOn": "NS 0"}}));
    assert_ne!(ed.parts()[STYLES], before[STYLES], "the values are written");
    let info = ed.style_info("paragraph", BASIC).unwrap();
    assert_eq!(info["name"], BASIC, "the panel name, not the `$ID/NormalParagraphStyle` of the file");
    assert_eq!(info["protected"], false, "the Basic Paragraph style can be edited");
    assert_eq!(info["builtIn"], true);
    assert_eq!(info["props"]["pointSize"].as_f64(), Some(14.0));
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before, "one undo undoes the edit: the style, the preferences and the text");
    // a cycle is refused: a style based on Basic Paragraph is not its base
    let mut ed = Editor::new(before);
    run(&mut ed, serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": "NS 0", "props": {"basedOn": BASIC}}));
    refused(&mut ed, serde_json::json!({"op": "styleEdit", "kind": "paragraph", "style": BASIC, "props": {"basedOn": "NS 0"}}));
    let root = ed.style_info("paragraph", "[No Paragraph Style]").unwrap();
    assert_eq!(root["protected"], true);
    assert_eq!(root["name"], "[No Paragraph Style]");
}

#[test]
fn redefining_a_root_style_is_refused() {
    let mut ed = Editor::new(fixture("text-format"));
    let message = refused(&mut ed, serde_json::json!({"op": "styleRedefine", "kind": "paragraph", "style": "[No Paragraph Style]", "story": "format", "start": 11, "end": 16}));
    assert!(message.contains(ROOT_MESSAGE), "{message}");
}
