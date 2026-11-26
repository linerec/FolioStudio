use folio_edit::{Editor, commands::RegisteredCommand};
use std::collections::BTreeMap;
use serde_json::json;

#[test]
fn xmp_preserves_translations_unknown_namespaces_and_history() {
    let source = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><r:Description xmlns:d="http://purl.org/dc/elements/1.1/" xmlns:other="urn:other"><other:title>keep</other:title><d:title><r:Alt><r:li xml:lang="ko-KR">제목</r:li><r:li xml:lang="x-default">Old</r:li></r:Alt></d:title><other:History>untouched</other:History></r:Description></r:RDF></x:xmpmeta>"#;
    let mut ed = Editor::new(BTreeMap::from([("META-INF/metadata.xml".into(),source.into())]));
    let before = ed.parts().clone();
    let props = BTreeMap::from([("documentTitle".into(),json!("Title & <text>")),("description".into(),json!("First\nSecond")),("keywords".into(),json!("alpha\nbeta & gamma")),("copyrightStatus".into(),json!("True"))]);
    ed.apply("file info", &[RegisteredCommand::SetFileInfo { props: props.clone() }.into()]).unwrap();
    let values = ed.file_info().unwrap();
    for (k,v) in props { assert_eq!(&values[&k], v.as_str().unwrap()); }
    let source = ed.part("META-INF/metadata.xml").unwrap();
    assert!(source.contains("<r:li xml:lang=\"ko-KR\">제목</r:li>"));
    assert!(source.contains("<other:title>keep</other:title>"));
    assert!(source.contains("<other:History>untouched</other:History>"));
    ed.undo().unwrap(); assert_eq!(ed.parts(), &before);
    ed.redo().unwrap(); assert_eq!(ed.file_info().unwrap()["documentTitle"],"Title & <text>");
}

#[test]
fn creates_missing_packet_and_rejects_invalid_input_atomically() {
    let mut ed = Editor::new(BTreeMap::new());
    ed.apply("title", &[RegisteredCommand::SetFileInfo { props:BTreeMap::from([("author".into(),json!("One\nTwo"))]) }.into()]).unwrap();
    assert_eq!(ed.file_info().unwrap()["author"],"One\nTwo");
    let before = ed.parts().clone();
    assert!(ed.apply("invalid", &[RegisteredCommand::SetFileInfo { props:BTreeMap::from([("author".into(),json!("Changed")),("copyrightStatus".into(),json!("bad"))]) }.into()]).is_err());
    assert_eq!(ed.parts(),&before);
    ed.undo().unwrap(); assert!(ed.part("META-INF/metadata.xml").is_none());
}
