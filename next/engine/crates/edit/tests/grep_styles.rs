use folio_edit::{Command, Editor};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let path=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/grep-styles.idml");
    let mut zip=zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts=BTreeMap::new();
    for n in 0..zip.len() {
        let mut entry=zip.by_index(n).unwrap();
        if entry.is_dir() {continue;}
        let name=entry.name().unwrap().into_owned();
        if name.ends_with(".xml") {let mut text=String::new();entry.read_to_string(&mut text).unwrap();parts.insert(name,text);}
    }
    Editor::new(parts)
}
fn set(rules:Value)->Command {serde_json::from_value(json!({"op":"setGrepStyles","style":"ParagraphStyle/Grep 0","rules":rules})).unwrap()}

#[test]
fn invalid_expression_and_source_leave_no_history() {
    let mut e=fixture();let before=e.parts().clone();
    for rules in [json!([{"style":"Red","expression":"(?<=x+)y"}]), json!([{"style":"Missing","expression":"word"}]), json!([{"style":"Red","expression":"word","source":9}])] {
        assert!(e.apply("invalid", &[set(rules)]).is_err());
        assert_eq!(e.parts(), &before); assert_eq!(e.undo_label(),None);
    }
}

#[test]
fn reorder_preserves_opaque_record_and_undo_is_exact() {
    let mut e=fixture();
    e.apply("add",&[set(json!([{"style":"Red","expression":"word"},{"style":"Blue","expression":"word"}]))]).unwrap();
    let mut parts=e.parts().clone();
    let xml=parts.get_mut("Resources/Styles.xml").unwrap();
    *xml=xml.replace("<AllGREPStyles type=\"list\">", "<AllGREPStyles type=\"list\" Future=\"yes\"><Unknown>retain</Unknown>")
        .replacen("<ListItem type=\"record\">", "<ListItem type=\"record\" FutureRecord=\"yes\"><UnknownRecord>retain</UnknownRecord>",1)
        .replacen("<GrepExpression type=\"string\">", "<GrepExpression type=\"string\" FutureExpression=\"yes\">",1);
    let mut e=Editor::new(parts);let before=e.parts().clone();
    e.apply("reorder",&[set(json!([{"style":"Blue","expression":"word","source":1},{"style":"Red","expression":"(?i)word","source":0}]))]).unwrap();
    let xml=&e.parts()["Resources/Styles.xml"];
    for token in ["Future=\"yes\"", "FutureRecord=\"yes\"", "FutureExpression=\"yes\"", "<Unknown>retain</Unknown>", "<UnknownRecord>retain</UnknownRecord>"] {assert!(xml.contains(token),"{token}");}
    let rules=folio_idml::grep::rules(xml,"ParagraphStyle/Grep 0").unwrap();
    assert_eq!(rules[0].style,"CharacterStyle/Blue");assert_eq!(rules[1].expression,"(?i)word");
    e.undo().unwrap();assert_eq!(e.parts(),&before);
    e.redo().unwrap();assert_eq!(folio_idml::grep::rules(&e.parts()["Resources/Styles.xml"],"ParagraphStyle/Grep 0").unwrap(),rules);
}

#[test]
fn effective_characters_match_indesign_dom() {
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/grep-styles");
    for name in ["grep-basic","grep-overlap","grep-reorder-delete","grep-boundaries-codes"] {
        let mut parts=fixture().parts().clone();
        let mut todo=vec![root.join(name).join("after")];
        while let Some(dir)=todo.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let p=entry.path();if p.is_dir(){todo.push(p);continue;}
                let rel=p.strip_prefix(root.join(name).join("after")).unwrap().to_string_lossy().replace('\\',"/");
                parts.insert(rel,std::fs::read_to_string(p).unwrap());
            }
        }
        let doc=folio_idml::read_parts(&parts,Path::new("grep-styles.idml"),&Default::default()).unwrap();
        let dom:Value=serde_json::from_str(&std::fs::read_to_string(root.join(name).join("indesign.json")).unwrap()).unwrap();
        for frame in dom["extension"]["frames"].as_array().unwrap() {
            let label=frame["label"].as_str().unwrap();let Some(index)=label.strip_prefix("grep-") else {continue};let style=format!("Grep {index}");
            let story=doc.stories.values().find(|s|s.paragraphs.first().is_some_and(|p|p.style==style||p.style==format!("ParagraphStyle/{style}"))).unwrap_or_else(||panic!("missing {style}: {:?}",doc.stories.values().flat_map(|s|s.paragraphs.iter().map(|p|&p.style)).collect::<Vec<_>>()));
            let actual:Vec<_>=story.paragraphs.iter().flat_map(|p|p.runs.iter()).flat_map(|r|r.text.chars().map(move |_|&r.attrs)).collect();
            let expected:Vec<_>=frame["characters"].as_array().unwrap().iter().filter(|v|v["text"]!="\r").collect();
            assert_eq!(actual.len(),expected.len(),"{name}/{label} length");
            for (index,(a,e)) in actual.iter().zip(expected).enumerate() {
                let fill=match &a.fill {folio_model::Paint::Swatch{id,..}=>id.strip_prefix("Color/").unwrap_or(id),_=>"None"};
                // The DOM returns the first fill on an overlap, but the exported PDF
                // prints the last fill. See the recorded DOM/PDF discrepancy.
                let expected_fill=if label=="grep-0"&&(5..9).contains(&index) {
                    match name {"grep-overlap"=>"Grep Blue","grep-reorder-delete"=>"Grep Red",_=>e["fill"].as_str().unwrap()}
                } else {e["fill"].as_str().unwrap()};
                assert_eq!(fill,expected_fill,"{name}/{label}/{index} fill");
                assert_eq!(a.font_style,e["fontStyle"].as_str().unwrap(),"{name}/{label}/{index} font");
                let style=if a.character_style.is_empty(){"[None]"}else{&a.character_style};
                assert_eq!(style,e["style"].as_str().unwrap(),"{name}/{label}/{index} explicit style");
            }
        }
    }
}
