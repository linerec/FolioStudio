use folio_edit::{Editor, commands::RegisteredCommand};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn bugfind_self_closing_story_can_set_preference_and_undo() {
    for xml in ["<Story Self=\"s\"/>", "<Story Self=\"s\"></Story>", "<x:Story xmlns:x=\"example\" Self=\"s\"/>"] {
        let mut ed = Editor::new(BTreeMap::from([("Stories/s.xml".into(), xml.into())]));
        let original = ed.parts().clone();
        ed.apply("optical", &[RegisteredCommand::SetOpticalMargin {
            story: "s".into(), props: BTreeMap::from([("enabled".into(), json!(true))]),
    }.into()]).unwrap();
    assert!(ed.optical_margin("s").unwrap().enabled);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &original);
    }
}

fn editor(preference: &str) -> Editor {
    Editor::new(BTreeMap::from([
        ("Stories/s.xml".into(), format!("<Story Self=\"s\">{preference}<ParagraphStyleRange><CharacterStyleRange><Content>“Text.”</Content></CharacterStyleRange><Table><Cell Self=\"cell\"/></Table></ParagraphStyleRange></Story>")),
        ("Spreads/p.xml".into(), "<Spread><TextFrame Self=\"frame\" ParentStory=\"s\"/></Spread>".into()),
    ]))
}

#[test]
fn settings_preserve_unknown_xml_validate_atomically_and_undo() {
    let mut ed=editor("<StoryPreference OpticalMarginAlignment=\"false\" OpticalMarginSize=\"12\" Future=\"keep\"><Future/></StoryPreference>");
    let original=ed.parts().clone();
    let command=|enabled,size|RegisteredCommand::SetOpticalMargin {story:"frame".into(),props:BTreeMap::from([("enabled".into(),json!(enabled)),("size".into(),json!(size))])}.into();
    ed.apply("optical", &[command(true,24.0)]).unwrap();
    let value=ed.optical_margin("cell").unwrap();
    assert!(value.enabled);assert_eq!(value.size,24.0);
    assert!(ed.part("Stories/s.xml").unwrap().contains("Future=\"keep\"><Future/>"));
    let changed=ed.parts().clone();
    for invalid in [0.0,0.09,1296.01] {
        assert!(ed.apply("invalid", &[command(false,invalid)]).is_err());
        assert_eq!(ed.parts(),&changed);
    }
    ed.undo().unwrap();assert_eq!(ed.parts(),&original);
    ed.redo().unwrap();assert_eq!(ed.parts(),&changed);
}

#[test]
fn absent_preference_defaults_and_registered_oracle_mapping() {
    let mut ed=editor("");
    assert_eq!(ed.optical_margin("s").unwrap(),Default::default());
    let step=json!({"op":"setOpticalMargin","target":"label","props":{"size":0.1}});
    let command=folio_edit::commands::oracle::command(&step,Some("frame")).unwrap().unwrap();
    ed.apply("minimum",&[command]).unwrap();
    assert_eq!(ed.optical_margin("s").unwrap().size,0.1);
    assert!(!ed.optical_margin("s").unwrap().enabled);
}
