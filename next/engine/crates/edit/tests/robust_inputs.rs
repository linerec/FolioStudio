//! Keep rejected wire input and malformed parts on the existing error path.
use folio_edit::{Command, Editor, commands::{RegisteredCommand, oracle}, data_merge_selection, xml::Index};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn oracle_adapter_rejects_malformed_fields_without_panicking() {
    for step in [Value::Null, json!(false), json!(42), json!([]), json!({}), json!({"op": 3}), json!({"op": "unknown"})] {
        assert!(oracle::command(&step, None).unwrap().is_none());
    }
    for props in [Value::Null, json!(false), json!(3), json!([]), json!("wrong")] {
        let error = oracle::command(&json!({"op": "setFileInfo", "props": props}), None).unwrap_err();
        assert_eq!(error.code, "invalid");
    }
    let error = oracle::command(&json!({"op": "insertFootnoteText", "at": 0, "text": "x"}), None).unwrap_err();
    assert_eq!((error.code, error.message.as_str()), ("invalid", "target required"));
    for at in [json!(-1), json!(1.5), json!("bad"), Value::Null] {
        let error = oracle::command(&json!({"op": "insertFootnoteText", "at": at, "text": "x"}), Some("missing")).unwrap_err();
        assert_eq!(error.code, "invalid");
    }
}

#[test]
fn metadata_validation_preserves_error_text_and_document_bytes() {
    let mut editor = Editor::new(BTreeMap::new());
    for (key, value, message) in [
        ("author", Value::Null, "invalid metadata author"),
        ("author", json!(7), "invalid metadata author"),
        ("author", json!("a\u{0000}b"), "invalid metadata author"),
        ("copyrightStatus", json!(false), "invalid metadata copyrightStatus"),
        ("copyrightStatus", json!("invalid"), "copyrightStatus"),
        ("missing", json!("value"), "invalid metadata missing"),
    ] {
        let before = editor.parts().clone();
        let error = editor.apply("invalid metadata", &[RegisteredCommand::SetFileInfo {
            props: BTreeMap::from([(key.into(), value)]),
        }.into()]).unwrap_err();
        assert_eq!((error.code, error.message.as_str()), ("invalid", message));
        assert_eq!(editor.parts(), &before);
        assert_eq!(editor.rev(), 0);
        assert_eq!(editor.undo_label(), None);
    }
}

#[test]
fn truncated_xml_and_multibyte_attributes_do_not_panic() {
    let source = "<?xml version=\"1.0\"?><Story Self=\"한글\"><Content a='é'>내용</Content><!--끝--></Story>";
    for end in (0..=source.len()).filter(|&i| source.is_char_boundary(i)) {
        let prefix = &source[..end];
        if let Ok(index) = Index::scan(prefix) {
            for i in 0..index.nodes.len() {
                let _ = index.local(prefix, i);
                let _ = index.attrs(prefix, i);
                let _ = index.children(i);
            }
        }
    }
    for source in ["<", "<Story>", "<Story a=\"", "</Story>", "<Story></Other>", "<!--", "<![CDATA[", "<?xml"] {
        assert!(Index::scan(source).is_err(), "{source:?}");
    }
    assert!(Index::scan("").unwrap().nodes.is_empty());
}

#[test]
fn missing_references_and_invalid_record_numbers_are_errors() {
    let mut editor = Editor::new(BTreeMap::new());
    let error = editor.apply("missing item", &[Command::Move { ids: vec!["missing".into()], by: [1.0, 2.0] }]).unwrap_err();
    assert_eq!(error.code, "notFound");
    assert!(editor.parts().is_empty());
    for range in ["0", "-1", "2-1", "4", "x", "1.5", "1,", "99999999999999999999999999999999999"] {
        let error = data_merge_selection(range, 3).unwrap_err();
        assert_eq!((error.code, error.message.as_str()), ("invalid", "One or more of the records specified are not valid record numbers."));
    }
    assert_eq!(data_merge_selection("2-3, 1, 2", 3).unwrap(), vec![1, 2, 0, 1]);
}
