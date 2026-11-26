use folio_edit::{Editor, commands::RegisteredCommand};
use std::collections::BTreeMap;
use serde_json::json;

#[test]
fn quotes_target_one_language_preserve_unknown_data_and_undo_atomically() {
    let xml = "<Document><Language Self=\"en\" Name=\"$ID/English\" SingleQuotes=\"‘’\" DoubleQuotes=\"“”\" Keep=\"yes\"/><Language Self=\"de\" Name=\"$ID/German\" SingleQuotes=\"‚‘\" DoubleQuotes=\"„“\"/></Document>";
    let mut ed=Editor::new(BTreeMap::from([("designmap.xml".into(),xml.into())]));
    let cmd=|props| RegisteredCommand::SetLanguageQuotes { language:"en".into(),props }.into();
    ed.apply("quotes", &[cmd(BTreeMap::from([("SingleQuotes".into(),json!("<>")),("DoubleQuotes".into(),json!("😀😁"))]))]).unwrap();
    let actual=serde_json::to_value(ed.language_quotes().unwrap()).unwrap();
    assert_eq!(actual[0]["singleQuotes"], "<>");
    assert_eq!(actual[0]["doubleQuotes"], "\u{f600}\u{f601}");
    assert_eq!(actual[1]["singleQuotes"], "‚‘");
    assert!(ed.part("designmap.xml").unwrap().contains("Keep=\"yes\""));
    let changed=ed.parts().clone();
    assert!(ed.apply("invalid", &[cmd(BTreeMap::from([("SingleQuotes".into(),json!("[]")),("DoubleQuotes".into(),json!("x"))]))]).is_err());
    assert_eq!(ed.parts(), &changed);
    ed.undo().unwrap(); assert_eq!(ed.part("designmap.xml"), Some(xml));
    ed.redo().unwrap(); assert_eq!(ed.parts(), &changed);
}
