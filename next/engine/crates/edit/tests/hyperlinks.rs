use folio_edit::{Command, Editor};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/xref.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut entry = zip.by_index(k).unwrap();
        if entry.is_dir() { continue; }
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        parts.insert(name, text);
    }
    Editor::new(parts)
}

#[test]
fn format_commands_return_ids_and_reject_used_deletion_atomically() {
    let mut ed=fixture();
    let before=ed.parts().clone();
    let data=folio_idml::hyperlinks::read(ed.parts()).unwrap();
    let used=data.sources.iter().find(|s|!s.format.is_empty()).unwrap();
    let command=|v|serde_json::from_value::<Command>(v).unwrap();
    assert!(ed.apply("used",&[command(serde_json::json!({"op":"deleteCrossReferenceFormat","id":used.format}))]).is_err());
    assert_eq!(ed.parts(),&before);
    assert_eq!(ed.rev(),0);
    let mut definition=data.formats.iter().find(|f|f.id==used.format).unwrap().clone();
    definition.blocks[0].kind="UnknownBuildingBlock".into();
    assert!(ed.apply("invalid used definition",&[command(serde_json::json!({"op":"saveCrossReferenceFormat","format":definition,"locations":[]}))]).is_err());
    assert_eq!(ed.parts(),&before);
    assert_eq!(ed.rev(),0);
    let f=&data.formats[0];
    assert!(ed.apply("duplicate name",&[command(serde_json::json!({"op":"duplicateCrossReferenceFormat","id":f.id,"name":f.name}))]).is_err());
    assert_eq!(ed.parts(),&before);
    let result=ed.apply("copy",&[command(serde_json::json!({"op":"duplicateCrossReferenceFormat","id":f.id,"name":"Test copy"}))]).unwrap();
    assert_eq!(result.created.len(),1);
    let copied=folio_idml::hyperlinks::read(ed.parts()).unwrap().formats.into_iter().find(|f|f.id==result.created[0]).unwrap();
    assert_eq!(copied.blocks,f.blocks);
    ed.undo().unwrap();
    assert_eq!(ed.parts(),&before);
}

#[test]
fn source_query_is_read_only_and_unsupported_edits_are_atomic() {
    let mut editor = fixture();
    let data = folio_idml::hyperlinks::read(editor.parts()).unwrap();
    let source = data.sources.iter().find(|s| s.format.is_empty()).unwrap();
    let original = editor.parts().clone();
    assert!(editor.text_format(&source.story, source.start, source.end).is_ok());
    assert_eq!(editor.parts(), &original);
    for at in [source.start, source.start + 1, source.end] {
        let cmd: Command = serde_json::from_value(serde_json::json!({
            "op":"insertText", "story":source.story, "at":at, "text":"x"
        })).unwrap();
        assert!(editor.apply("unsupported source edit", &[cmd]).is_err());
        assert_eq!(editor.parts(), &original);
        assert_eq!(editor.rev(), 0);
        assert_eq!(editor.undo_label(), None);
    }
}

#[test]
fn failed_reference_options_preserve_bytes_and_history() {
    let mut editor = fixture();
    let data = folio_idml::hyperlinks::read(editor.parts()).unwrap();
    let link = data.links.iter().find(|l| l.name == "Reference").unwrap();
    let original = editor.parts().clone();
    for (name, format) in [("Unsupported", "Page Number"), ("Website", "Folio paragraph")] {
        let cmd: Command = serde_json::from_value(serde_json::json!({
            "op":"setCrossReference", "id":link.id, "name":name,
            "destination":link.destination, "format":format, "locations":[]
        })).unwrap();
        assert!(editor.apply("invalid options", &[cmd]).is_err());
        assert_eq!(editor.parts(), &original);
        assert_eq!(editor.undo_label(), None);
    }
}

#[test]
fn paragraph_targets_read_the_story_inside_the_package_wrapper() {
    let editor = fixture();
    let original = editor.parts().clone();
    let targets = editor.cross_reference_targets().unwrap();
    let second = targets.as_array().unwrap().iter().find(|p|p["text"]=="Second paragraph").unwrap();
    assert_eq!(second["at"], 15);
    assert!(!second["story"].as_str().unwrap().is_empty());
    assert_eq!(editor.parts(), &original);
}

#[test]
fn navigation_targets_preserve_document_and_undo_history() {
    use folio_idml::hyperlinks::{Target, target};
    let mut editor = fixture();
    let mut data = folio_idml::hyperlinks::read(editor.parts()).unwrap();
    let reference = data.links.iter().find(|l| l.name == "Reference").unwrap().clone();
    let website = data.links.iter().find(|l| l.name == "Website").unwrap();
    let before = editor.parts().clone();
    let resolved = editor.hyperlink_target(&reference.id).unwrap();
    assert!(matches!(resolved, Target::Text { start: 0, end: 0, .. }));
    assert_eq!(editor.hyperlink_target("absent").unwrap(), Target::MissingLink);
    assert!(matches!(editor.hyperlink_target(&website.id).unwrap(), Target::Url { url } if !url.is_empty()));
    assert_eq!(editor.parts(), &before);
    assert_eq!(editor.rev(), 0);
    assert_eq!(editor.undo_label(), None);
    data.destinations.retain(|d| d.id != reference.destination);
    assert_eq!(target(&data, &reference.id), Target::MissingDestination);

    let command: Command = serde_json::from_value(serde_json::json!({"op":"deleteHyperlink","id":reference.id})).unwrap();
    editor.apply("delete link", &[command]).unwrap();
    let rev = editor.rev();
    let edited = editor.parts().clone();
    assert_eq!(editor.hyperlink_target(&reference.id).unwrap(), Target::MissingLink);
    assert_eq!(editor.parts(), &edited);
    assert_eq!(editor.rev(), rev);
    assert_eq!(editor.undo_label(), Some("delete link"));
}
