use folio_edit::{Command, Editor};
use std::{collections::BTreeMap, path::Path};

#[test]
fn tracked_note_keeps_revision_metadata_through_edit_conversion_and_undo() {
    use folio_edit::commands::RegisteredCommand as R;
    let mut parts=measured("01-note");
    let source=parts.get_mut("Stories/Story_u117.xml").unwrap();
    let start=source.find("<Note ").unwrap();let end=source.find("</Note>").unwrap()+7;
    let raw=source[start..end].replace("<Note ","<Note FutureNote=\"keep\" ");
    source.replace_range(start..end,&format!("<Change ChangeType=\"InsertedText\" UserName=\"Unknown User Name\" Date=\"2026-10-22T21:00:00\" FutureChange=\"keep\">{raw}</Change>"));
    *source=source.replace("TrackChanges=\"false\"","TrackChanges=\"true\"");
    let mut ed=Editor::new(parts.clone());
    ed.apply("collapse",&[R::CollapseNote{story:"u117".into(),index:0,collapsed:true}.into()]).unwrap();
    let source=ed.part("Stories/Story_u117.xml").unwrap();
    assert!(source.contains("FutureNote=\"keep\""));assert!(source.contains("FutureChange=\"keep\""));
    ed.undo().unwrap();assert_eq!(ed.parts(),&parts);
    ed.apply("convert",&[R::NoteToTextAs{story:"u117".into(),index:0,user:"Unknown User Name".into(),date:"2026-10-22T23:00:00".into()}.into()]).unwrap();
    let source=ed.part("Stories/Story_u117.xml").unwrap();
    assert!(source.contains("FutureChange=\"keep\""));
    assert!(source.contains("Date=\"2026-10-22T21:00:00\""));
    assert_eq!(folio_edit::text::contents(source,"u117").unwrap(),"AB Memo CDE");
    ed.undo().unwrap();assert_eq!(ed.parts(),&parts);
}

#[test]
fn authored_edits_add_document_user_and_undo_atomically() {
    use folio_edit::commands::RegisteredCommand as R;
    let mut parts=measured("00-base");
    parts.insert("designmap.xml".into(),"<Document><DocumentUser Self=\"dDocumentUser0\" UserName=\"Existing\" Future=\"keep\" /></Document>".into());
    let mut ed=Editor::new(parts.clone());
    ed.apply("tracking",&[R::SetTrackChanges{story:"u117".into(),document:false,enabled:true}.into()]).unwrap();
    let before=ed.parts().clone();
    ed.apply("typing",&[R::InsertTextAs{story:"u117".into(),at:2,text:"한😀".into(),user:"Kim & Lee".into(),date:"2026-10-22T21:00:00".into()}.into()]).unwrap();
    assert!(ed.part("designmap.xml").unwrap().contains("UserName=\"Kim &amp; Lee\""));
    assert!(ed.part("designmap.xml").unwrap().contains("Future=\"keep\""));
    assert!(ed.part("Stories/Story_u117.xml").unwrap().contains("AppliedDocumentUser=\"dDocumentUser1\""));
    assert_eq!(folio_edit::text::contents(ed.part("Stories/Story_u117.xml").unwrap(),"u117").unwrap(),"AB한😀CDE");
    let typed=ed.parts().clone();
    ed.apply("erase own",&[R::DeleteTextAs{story:"u117".into(),start:2,end:4,user:"Kim & Lee".into(),date:"2026-10-22T21:00:01".into()}.into()]).unwrap();
    assert!(!ed.part("Stories/Story_u117.xml").unwrap().contains("<Change "));
    ed.undo().unwrap();assert_eq!(ed.parts(),&typed);
    ed.undo().unwrap();assert_eq!(ed.parts(),&before);
    assert!(ed.apply("unsupported control",&[R::InsertTextAs{story:"u117".into(),at:2,text:"a\u{7}b".into(),user:"New".into(),date:"2026-10-22T21:00:00".into()}.into()]).is_err());
    assert_eq!(ed.parts(),&before);
    // a typed paragraph break is a Br inside the insertion record (measured: changes-break-insert-text) and one undo step
    ed.apply("break",&[R::InsertTextAs{story:"u117".into(),at:2,text:"a\rb".into(),user:"New".into(),date:"2026-10-22T21:00:00".into()}.into()]).unwrap();
    assert!(ed.part("Stories/Story_u117.xml").unwrap().contains("<Content>a</Content><Br /><Content>b</Content>"));
    ed.undo().unwrap();assert_eq!(ed.parts(),&before);
}

#[test]
fn tracked_changes_resolve_and_undo_without_losing_unknown_metadata() {
    use folio_edit::commands::RegisteredCommand as R;
    for (accept,index,expected,left) in [(true,0,"ABxyE",1),(false,0,"ABE",1),(true,1,"ABxyE",1),(false,1,"ABxyCDE",1),(true,-1,"ABxyE",0),(false,-1,"ABCDE",0)] {
        let parts=measured("05-delete");let mut ed=Editor::new(parts.clone());
        ed.apply("resolve",&[R::ResolveChanges{story:"u117".into(),document:false,index,user:String::new(),accept}.into()]).unwrap();
        let source=ed.part("Stories/Story_u117.xml").unwrap();
        assert_eq!(folio_edit::text::contents(source,"u117").unwrap(),expected);
        assert_eq!(folio_idml::editorial::metadata(source,"u117").unwrap().1.len(),left);
        ed.undo().unwrap();assert_eq!(ed.parts(),&parts);
    }
    let parts=measured("05-delete");let mut ed=Editor::new(parts.clone());
    ed.apply("other user",&[R::ResolveChanges{story:"u117".into(),document:false,index:-1,user:"Other".into(),accept:true}.into()]).unwrap();
    assert_eq!(ed.parts(),&parts);
    ed.apply("off",&[R::SetTrackChanges{story:"u117".into(),document:true,enabled:false}.into()]).unwrap();
    assert!(!folio_idml::editorial::metadata(ed.part("Stories/Story_u117.xml").unwrap(),"u117").unwrap().2);
    ed.undo().unwrap();assert_eq!(ed.parts(),&parts);
}

#[test]
fn copy_in_a_tracked_story_takes_the_visible_text() {
    let mut ed=Editor::new(measured("05-delete"));
    let fragment=ed.copy_text("u117",1,4).unwrap();
    assert_eq!(fragment.text,"Bxy");
}

fn measured(name: &str) -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/notes/structure").join(format!("{name}.xml"));
    BTreeMap::from([("Stories/Story_u117.xml".into(), std::fs::read_to_string(path).unwrap())])
}

#[test]
fn note_marker_preserves_metadata_and_unknown_xml_during_typing_and_undo() {
    let mut parts = measured("01-note");
    let source = parts.get_mut("Stories/Story_u117.xml").unwrap();
    *source = source.replace("<Note ", "<Note Future=\"keep &amp; preserve\" ");
    let start = source.find("<Note ").unwrap();
    let end = source.find("</Note>").unwrap() + 7;
    let raw = source[start..end].to_owned();
    assert_eq!(folio_edit::text::contents(source, "u117").unwrap(), "AB\u{feff}CDE");
    let mut ed = Editor::new(parts.clone());
    ed.apply("type", &[Command::InsertText {story:"u117".into(),at:3,text:"한😀".into()}]).unwrap();
    let changed = ed.part("Stories/Story_u117.xml").unwrap();
    assert!(changed.contains(&raw));
    assert_eq!(folio_edit::text::contents(changed, "u117").unwrap(), "AB\u{feff}한😀CDE");
    ed.undo().unwrap(); assert_eq!(ed.parts(), &parts);
    ed.redo().unwrap();
    let before_delete = ed.parts().clone();
    ed.apply("delete marker", &[Command::DeleteText {story:"u117".into(),start:2,end:3}]).unwrap();
    assert!(!ed.part("Stories/Story_u117.xml").unwrap().contains("<Note "));
    ed.undo().unwrap(); assert_eq!(ed.parts(), &before_delete);
}

#[test]
fn imported_changes_roundtrip_without_mutating_any_source_bytes() {
    let original = measured("05-delete");
    let ed = Editor::new(original.clone());
    assert_eq!(ed.parts(), &original);
    let source = &ed.parts()["Stories/Story_u117.xml"];
    let first = folio_idml::editorial::metadata(source, "u117").unwrap();
    let loaded = Editor::new(ed.parts().clone());
    assert_eq!(folio_idml::editorial::metadata(&loaded.parts()["Stories/Story_u117.xml"], "u117").unwrap(), first);
}

#[test]
fn convert_note_spaces_match_native_word_boundaries() {
    for (input,start,end,expected) in [
        ("AB CD EF",3,5,"AB \u{feff}EF"), ("AB CD EF",2,5,"AB\u{feff} EF"),
        ("AB CD EF",3,6,"AB \u{feff}EF"), ("AB  CD  EF",4,6,"AB  \u{feff}  EF"),
        ("CD EF",0,2,"\u{feff}EF"), ("AB CD",3,5,"AB\u{feff}"),
        ("ABCD EF",2,4,"AB\u{feff} EF"), ("AB CDXY",3,5,"AB \u{feff}XY"),
        ("AB  CD EF",4,6,"AB  \u{feff}EF"), ("AB CD  EF",3,5,"AB\u{feff}  EF"),
        ("CD  EF",0,2,"\u{feff}  EF"), ("AB  CD",4,6,"AB  \u{feff}"),
        ("AB CD EF",2,6,"AB \u{feff}EF"), ("ABCDE",1,4,"A\u{feff}E"),
    ] {
        let mut parts=measured("00-base");
        let source=parts.get_mut("Stories/Story_u117.xml").unwrap();
        *source=source.replace("ABCDE",input);
        let mut ed=Editor::new(parts.clone());
        ed.apply("note", &[folio_edit::commands::RegisteredCommand::ConvertToNote{story:"u117".into(),start,end,user:"Unknown User Name".into(),date:"2026-10-22T21:04:22".into()}.into()]).unwrap();
        assert_eq!(folio_edit::text::contents(ed.part("Stories/Story_u117.xml").unwrap(),"u117").unwrap(),expected,"{input} [{start},{end}]");
        ed.undo().unwrap();assert_eq!(ed.parts(),&parts);
    }
}
