//! Rejected definitions must leave both document parts and history untouched.
use folio_edit::{Command, Editor};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let path=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/objectstyles.idml");
    let mut zip=zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts=BTreeMap::new();
    for n in 0..zip.len() {
        let mut entry=zip.by_index(n).unwrap();
        if entry.is_dir() {continue;}
        let name=entry.name().unwrap().into_owned();
        if name.ends_with(".xml") {
            let mut text=String::new();entry.read_to_string(&mut text).unwrap();parts.insert(name,text);
        }
    }
    Editor::new(parts)
}

fn command(value:Value) -> Command {serde_json::from_value(value).unwrap()}

#[test]
fn invalid_properties_are_atomic() {
    let mut editor=fixture();
    for props in [json!({"fillColor":"Color/Missing"}),json!({"topLeftCornerOption":"BoundingBoxTextWrap"}),
        json!({"strokeWeight":-1}),json!({"textFramePreferences":{"textColumnCount":1.5}})] {
        let before=editor.parts().clone();let rev=editor.rev();
        let result=editor.apply("invalid",&[command(json!({"op":"createObjectStyle","name":"Rejected","props":props}))]);
        assert!(result.is_err());assert_eq!(editor.parts(),&before);assert_eq!(editor.rev(),rev);
        assert_eq!(editor.undo_label(),None);
    }
}

#[test]
fn indirect_descendant_replacement_cannot_create_a_cycle() {
    let mut editor=fixture();
    editor.apply("chain",&[
        command(json!({"op":"createObjectStyle","name":"Child","basedOn":"ObjectStyle/Oracle Box","props":{}})),
        command(json!({"op":"createObjectStyle","name":"Grandchild","basedOn":"ObjectStyle/Child","props":{}})),
    ]).unwrap();
    let before=editor.parts().clone();let rev=editor.rev();
    let result=editor.apply("delete",&[command(json!({"op":"deleteObjectStyle","style":"ObjectStyle/Oracle Box","replacement":"ObjectStyle/Grandchild"}))]);
    assert!(result.is_err());assert_eq!(editor.parts(),&before);assert_eq!(editor.rev(),rev);
    assert_eq!(editor.undo_label(),Some("chain"));
}
