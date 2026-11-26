use folio_edit::{Command, Editor, xml::Index};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (BTreeMap<String,String>,String,String) {
    let p=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/anchored.idml");
    let mut z=zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts=BTreeMap::new();let mut id=String::new();let mut story_part=String::new();
    for n in 0..z.len() {let mut e=z.by_index(n).unwrap();if e.is_dir(){continue;}let name=e.name().unwrap().into_owned();let mut t=String::new();e.read_to_string(&mut t).unwrap();
        if name.starts_with("Stories/"){let ix=Index::scan(&t).unwrap();for i in 0..ix.nodes.len(){if ix.attr(&t,i,"Key")==Some("oracle.id")&&ix.attr(&t,i,"Value")==Some("custom"){id=ix.ancestors(i).find_map(|a|ix.attr(&t,a,"Self")).unwrap().into();story_part=name.clone();}}}
        parts.insert(name,t);
    }
    assert!(!id.is_empty());(parts,id,story_part)
}
#[test]
fn preserves_unknown_settings_and_restores_original_bytes() {
    let (mut parts,id,part)=fixture();
    let t=parts.get_mut(&part).unwrap();*t=t.replace("<AnchoredObjectSetting ","<AnchoredObjectSetting FutureOption=\"kept &amp; safe\" ");
    let before=parts.clone();let mut ed=Editor::new(parts);
    let c:Command=serde_json::from_value(json!({"op":"setAnchoredObject","ids":[id],"props":{"anchorXoffset":-12.25,"anchorYoffset":9,"lockPosition":true}})).unwrap();
    ed.apply("options",&[c]).unwrap();
    assert!(ed.parts()[&part].contains("FutureOption=\"kept &amp; safe\""));
    let q=ed.anchored_object(&id).unwrap();assert_eq!(q["props"]["anchorXoffset"],-12.25);assert_eq!(q["props"]["lockPosition"],true);
    let edited=ed.parts().clone();ed.undo().unwrap();assert_eq!(ed.parts(),&before);ed.redo().unwrap();assert_eq!(ed.parts(),&edited);
    let mut reopened=Editor::new(edited);assert_eq!(reopened.anchored_object(&id).unwrap()["props"],q["props"]);
}
#[test]
fn invalid_options_are_atomic() {
    let (parts,id,_)=fixture();let before=parts.clone();let mut ed=Editor::new(parts);
    for props in [json!({"anchorXoffset":100001}),json!({"anchoredPosition":"invalid"}),json!({"pinPosition":"true"}),json!({"unknown":12})] {
        let c:Command=serde_json::from_value(json!({"op":"setAnchoredObject","ids":[id],"props":props})).unwrap();
        assert!(ed.apply("invalid",&[c]).is_err());assert_eq!(ed.parts(),&before);
    }
}
#[test]
fn omitted_defaults_and_object_style_override() {
    let defaults=folio_idml::anchored::values("<Rectangle/>","<Styles/>").unwrap();
    assert_eq!(defaults["anchoredPosition"],"InlinePosition");assert_eq!(defaults["pinPosition"],true);assert_eq!(defaults["anchorPoint"],"BottomRightAnchor");
    let styles=r#"<Styles><RootObjectStyleGroup Self="root"><ObjectStyle Self="ObjectStyle/custom" Name="custom" EnableAnchoredObjectOptions="true"><AnchoredObjectSetting AnchoredPosition="AboveLine" AnchorYoffset="7"/></ObjectStyle></RootObjectStyleGroup></Styles>"#;
    let v=folio_idml::anchored::values(r#"<Rectangle AppliedObjectStyle="custom"><AnchoredObjectSetting AnchorXoffset="-4"/></Rectangle>"#,styles).unwrap();
    assert_eq!(v["anchoredPosition"],"AboveLine");assert_eq!(v["anchorYoffset"],7.0);assert_eq!(v["anchorXoffset"],-4.0);
}
