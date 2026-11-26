use folio_edit::{Editor, commands::RegisteredCommand};
use std::collections::BTreeMap;
use serde_json::json;

#[test]
fn assignment_preserves_numbers_and_undo_and_rejects_invalid_values() {
    let mut ed = Editor::new(BTreeMap::from([
        ("designmap.xml".into(),"<Document Self=\"d\"><Other Keep=\"yes\"/></Document>".into()),
        ("Resources/Graphic.xml".into(),"<Color ColorValue=\"10 20 30 40\"/>".into()),
    ]));
    let before=ed.parts().clone();
    let props=BTreeMap::from([("RGBProfile".into(),json!("A & B")),("CMYKProfile".into(),json!("Coated")),("RGBPolicy".into(),json!("PreserveEmbeddedProfiles"))]);
    ed.apply("assign", &[RegisteredCommand::SetColorPrefs{props:props.clone()}.into()]).unwrap();
    for (key,value) in props { assert_eq!(ed.color_preferences().unwrap()[&key],value); }
    assert_eq!(ed.part("Resources/Graphic.xml"),before.get("Resources/Graphic.xml").map(String::as_str));
    assert!(ed.part("designmap.xml").unwrap().contains("<Other Keep=\"yes\"/>"));
    let assigned=ed.parts().clone();
    ed.undo().unwrap();assert_eq!(ed.parts(),&before);
    ed.redo().unwrap();assert_eq!(ed.parts(),&assigned);
    let props=BTreeMap::from([("RGBProfile".into(),json!("Changed")),("SolidColorIntent".into(),json!("invalid"))]);
    assert!(ed.apply("bad", &[RegisteredCommand::SetColorPrefs{props}.into()]).is_err());
    assert_eq!(ed.parts(),&assigned);
}
