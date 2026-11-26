use folio_edit::{Command, Editor};
use folio_edit::commands::RegisteredCommand as Footnote;
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/footnotes.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() { continue; }
        let name = e.name().unwrap().into_owned();
        let mut text = String::new(); e.read_to_string(&mut text).unwrap();
        parts.insert(name, text);
    }
    parts
}

#[test]
fn marker_restoration_is_atomic_and_navigation_is_read_only() {
    let mut ed = Editor::new(fixture());
    ed.apply("two notes", &[
        Footnote::InsertFootnote {story:"u12e".into(),at:30}.into(),
        Footnote::InsertFootnote {story:"u12e".into(),at:60}.into(),
    ]).unwrap();
    let before = ed.parts().clone();
    let text = &before["Stories/Story_u12e.xml"];
    assert_eq!(folio_edit::text::note_context(text,"u12e",30).unwrap()["footnoteTarget"], "u12e/footnote/0");
    assert_eq!(folio_edit::text::note_context(text,"u12e",61).unwrap()["footnoteTarget"], "u12e/footnote/1");
    assert!(folio_edit::text::note_context(text,"u12e",40).unwrap()["footnoteTarget"].is_null());
    assert_eq!(ed.parts(), &before);
    let restore: Command = Footnote::InsertFootnoteMarker {story:"u12e".into(),index:1,at:0}.into();
    assert!(ed.apply("duplicate marker", std::slice::from_ref(&restore)).is_err());
    assert_eq!(ed.parts(), &before);
    ed.apply("remove marker", &[Footnote::DeleteFootnoteText {story:"u12e".into(),index:1,start:0,end:1}.into()]).unwrap();
    let source = ed.part("Stories/Story_u12e.xml").unwrap();
    assert_eq!(folio_edit::text::note_context(source,"u12e/footnote/1",0).unwrap()["canInsertFootnoteMarker"], true);
    assert!(ed.apply("outside footnote", &[Command::InsertSpecial {story:"u12e".into(),at:0,special:"FOOTNOTE_SYMBOL".into()}]).is_err());
    ed.apply("restore marker", &[restore]).unwrap();
    let source = ed.part("Stories/Story_u12e.xml").unwrap();
    assert_eq!(folio_edit::text::contents(source,"u12e/footnote/1").unwrap(), "\u{fffc}\t");
    ed.undo().unwrap();
    assert_eq!(folio_edit::text::contents(ed.part("Stories/Story_u12e.xml").unwrap(),"u12e/footnote/1").unwrap(), "\t");
    ed.redo().unwrap();
    assert_eq!(folio_edit::text::contents(ed.part("Stories/Story_u12e.xml").unwrap(),"u12e/footnote/1").unwrap(), "\u{fffc}\t");
}

#[test]
fn nested_text_address_survives_reload_and_reference_deletion_is_reversible() {
    let mut ed = Editor::new(fixture());
    let made = ed.apply("note", &[Footnote::InsertFootnote {story:"u12e".into(), at:30}.into()]).unwrap();
    let id = &made.created[0];
    assert_eq!(id, "u12e/footnote/0");
    ed.apply("type", &[Command::InsertText {story:id.clone(),at:2,text:"각주 😀\rNext.".into()}]).unwrap();
    let parts = ed.parts().clone();
    let source = &parts["Stories/Story_u12e.xml"];
    assert!(source.contains("<Footnote>"));
    assert!(!source.contains("/footnote/"), "the cursor address is never written into IDML");
    assert_eq!(folio_edit::text::contents(source,id).unwrap(), "\u{fffc}\t각주 😀\rNext.");
    let doc = folio_idml::read_parts(&parts, Path::new("."), &Default::default()).unwrap();
    let note = doc.stories["u12e"].paragraphs.iter().flat_map(|p| &p.runs).find_map(|r|r.note.as_ref()).unwrap();
    assert_eq!(&note.id,id);
    let mut loaded = Editor::new(parts.clone());
    loaded.apply("delete reference", &[Command::DeleteText {story:"u12e".into(),start:30,end:31}]).unwrap();
    assert!(!loaded.part("Stories/Story_u12e.xml").unwrap().contains("<Footnote>"));
    loaded.undo().unwrap(); assert_eq!(loaded.parts(), &parts);
    loaded.redo().unwrap();
    assert!(folio_edit::text::contents(loaded.part("Stories/Story_u12e.xml").unwrap(),id).is_err());
}

#[test]
fn options_preserve_unknown_values_and_reject_invalid_changes_atomically() {
    let mut parts = fixture();
    let source = parts.get_mut("Resources/Preferences.xml").unwrap();
    *source = source.replace("<FootnoteOption ", "<FootnoteOption FutureOption=\"keep &amp; preserve\" ");
    let mut ed = Editor::new(parts);
    ed.apply("note", &[Footnote::InsertFootnote {story:"u12e".into(),at:30}.into()]).unwrap();
    let before = ed.parts().clone();
    let props = BTreeMap::from([("StartAt".into(),serde_json::json!(1.5)),("Prefix".into(),serde_json::json!("["))]);
    assert!(ed.apply("invalid", &[Footnote::SetFootnoteOptions{props}.into()]).is_err());
    assert_eq!(ed.parts(), &before);
    let props = BTreeMap::from([("StartAt".into(),serde_json::json!(7)),("SeparatorText".into(),serde_json::json!(": "))]);
    ed.apply("options", &[Footnote::SetFootnoteOptions{props}.into()]).unwrap();
    assert_eq!(ed.footnote_options().unwrap()["StartAt"], "7");
    assert!(ed.part("Resources/Preferences.xml").unwrap().contains("FutureOption=\"keep &amp; preserve\""));
    assert_eq!(folio_edit::text::contents(ed.part("Stories/Story_u12e.xml").unwrap(),"u12e/footnote/0").unwrap(), "\u{fffc}: ");
    ed.undo().unwrap(); assert_eq!(ed.parts(), &before);
}

#[test]
fn implicit_defaults_in_a_new_document_materialize_only_when_edited() {
    let mut parts = fixture();
    let source = parts.get_mut("Resources/Preferences.xml").unwrap();
    let start = source.find("<FootnoteOption ").unwrap();
    let end = source[start..].find("</FootnoteOption>").unwrap()+start+"</FootnoteOption>".len();
    source.replace_range(start..end, "");
    let mut ed = Editor::new(parts.clone());
    assert_eq!(ed.footnote_options().unwrap()["EnableStraddling"],"true");
    assert_eq!(ed.parts(),&parts,"reading options does not change a new document");
    ed.apply("note", &[Footnote::InsertFootnote {story:"u12e".into(),at:30}.into()]).unwrap();
    assert!(ed.part("Resources/Preferences.xml").unwrap().contains("<FootnoteOption "));
    ed.undo().unwrap(); assert_eq!(ed.parts(),&parts);
    let props = BTreeMap::from([("StartAt".into(),serde_json::json!(-1))]);
    assert!(ed.apply("invalid", &[Footnote::SetFootnoteOptions{props}.into()]).is_err());
    assert_eq!(ed.parts(),&parts,"invalid edit also rolls back default materialization");
}
