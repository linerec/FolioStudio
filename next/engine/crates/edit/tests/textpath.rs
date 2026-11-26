use folio_edit::{Command, Editor};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    named_fixture("tools2-path")
}

fn named_fixture(name: &str) -> Editor {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../../oracle/fixtures/{name}.idml"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut entry = zip.by_index(k).unwrap();
        if entry.is_dir() { continue; }
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), text);
    }
    Editor::new(parts)
}

#[test]
fn threaded_removal_keeps_story_and_reconnects_next_frame() {
    let mut ed = named_fixture("textpath-threaded");
    let before = ed.parts().clone();
    let stories: BTreeMap<_,_> = before.iter().filter(|(k,_)| k.starts_with("Stories/")).map(|(k,v)|(k.clone(),v.clone())).collect();
    let ids = ed.text_path_items().unwrap();
    ed.apply("remove threaded path", &[remove(ids)]).unwrap();
    for (k,v) in stories { assert_eq!(ed.part(&k), Some(v.as_str())); }
    let doc = folio_idml::read_parts(ed.parts(), Path::new("test.idml"), &Default::default()).unwrap();
    let frame = doc.spreads.iter().flat_map(|s| &s.items).find_map(|i| match &i.kind { folio_model::ItemKind::TextFrame(f) => Some(f), _ => None }).unwrap();
    assert!(frame.previous.is_none());
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
}

fn remove(ids: Vec<String>) -> Command {
    serde_json::from_value(json!({"op":"removeTextFromPath","ids":ids})).unwrap()
}

#[test]
fn removing_type_keeps_path_and_undo_restores_all_parts() {
    let mut ed = fixture();
    let ids = ed.text_path_items().unwrap();
    assert_eq!(ids.len(), 1);
    let before = ed.parts().clone();
    ed.apply("remove type", &[remove(ids.clone())]).unwrap();
    assert!(ed.text_path_items().unwrap().is_empty());
    assert!(ed.parts().values().any(|t| t.contains(&format!("Self=\"{}\"",ids[0]))));
    let after = ed.parts().clone();
    assert!(after.len() < before.len());
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
    assert_eq!(ed.text_path_items().unwrap(), ids);
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}

#[test]
fn invalid_later_target_rolls_back_earlier_removal() {
    let mut ed = fixture();
    let mut ids = ed.text_path_items().unwrap();
    ids.push("missing".into());
    let before = ed.parts().clone();
    assert!(ed.apply("remove type", &[remove(ids)]).is_err());
    assert_eq!(ed.parts(), &before);
    assert_eq!(ed.undo_label(), None);
}

#[test]
fn options_roundtrip_undo_and_rejection_are_atomic() {
    let mut ed = named_fixture("textpath-formatted");
    let ids = ed.text_path_items().unwrap();
    let before = ed.parts().clone();
    let command = |props| serde_json::from_value(json!({"op":"setTextPathOptions","ids":ids,"props":props})).unwrap();
    ed.apply("path options", &[command(json!({"PathAlignment":"TopPathAlignment","StartBracket":"20","EndBracket":"200"}))]).unwrap();
    assert_eq!(ed.text_path_options(&ids[0]).unwrap()["props"]["PathAlignment"], "TopPathAlignment");
    let doc = folio_idml::read_parts(ed.parts(),Path::new("test.idml"),&Default::default()).unwrap();
    let path=doc.spreads.iter().flat_map(|s|&s.items).find_map(|i|i.text_path.as_ref()).unwrap();
    assert_eq!(path.path_alignment,folio_model::textpath::PathAlignment::Top);
    let changed=ed.parts().clone();
    ed.undo().unwrap();assert_eq!(ed.parts(),&before);
    ed.redo().unwrap();assert_eq!(ed.parts(),&changed);
    for props in [json!({"PathAlignment":"BottomPathAlignment","FlipPathEffect":"invalid"}),json!({"EndBracket":"9999"}),json!({"StartBracket":"NaN"}),json!({"TextAlignment":"InvalidTextAlignment"})] {
        assert!(ed.apply("unsupported", &[command(props)]).is_err());
        assert_eq!(ed.parts(),&changed);
    }
}

#[test]
fn threaded_options_are_rejected_without_changes() {
    let mut ed=named_fixture("textpath-threaded");let before=ed.parts().clone();
    let command=serde_json::from_value(json!({"op":"setTextPathOptions","ids":ed.text_path_items().unwrap(),"props":{"PathAlignment":"TopPathAlignment"}})).unwrap();
    assert!(ed.apply("unsupported",&[command]).is_err());assert_eq!(ed.parts(),&before);
}

#[test]
fn locked_parent_rejects_delete_and_options_without_changes() {
    let mut ed = fixture();
    let ids = ed.text_path_items().unwrap();
    ed.apply("lock", &[Command::SetItem{id:ids[0].clone(),locked:Some(true),visible:None}]).unwrap();
    let mut ed = Editor::new(ed.parts().clone());
    let before = ed.parts().clone();
    let options = serde_json::from_value(json!({"op":"setTextPathOptions","ids":ids,"props":{"PathAlignment":"TopPathAlignment"}})).unwrap();
    for command in [remove(ids), options] {
        assert!(ed.apply("locked", &[command]).is_err());
        assert_eq!(ed.parts(), &before);
        assert_eq!(ed.undo_label(), None);
    }
}

#[test]
fn registered_oracle_adapter_preserves_target_and_options() {
    let mut ed = fixture();
    let id = ed.text_path_items().unwrap().remove(0);
    let step = json!({"op":"setTextPathOptions","target":"path","props":{"PathAlignment":"TopPathAlignment"}});
    let command = folio_edit::commands::oracle::command(&step,Some(&id)).unwrap().unwrap();
    ed.apply("oracle", &[command]).unwrap();
    assert_eq!(ed.text_path_options(&id).unwrap()["props"]["PathAlignment"], "TopPathAlignment");
}

fn create(ids: Vec<String>, text: &str) -> Command {
    serde_json::from_value(json!({"op":"createTextPath","ids":ids,"text":text})).unwrap()
}

#[test]
fn create_returns_child_and_story_preserves_parent_and_undoes_once() {
    let mut ed = named_fixture("textpath-formatted");
    let ids = ed.text_path_items().unwrap();
    ed.apply("remove", &[remove(ids.clone())]).unwrap();
    let before = ed.parts().clone();
    let result = ed.apply("create", &[create(ids.clone(), "AV Oag")]).unwrap();
    assert_eq!(result.created.len(), 2);
    assert_eq!(ed.text_path_items().unwrap(), ids);
    let story = ed.part(&format!("Stories/Story_{}.xml",result.created[1])).unwrap();
    assert!(story.contains("AV Oag"));
    assert_eq!(ed.text_path_options(&ids[0]).unwrap()["props"]["EndBracket"], "240");
    for (name, original) in &before {
        if name.starts_with("Spreads/") {
            let after = ed.part(name).unwrap();
            let ix = folio_edit::xml::Index::scan(after).unwrap();
            let i = (0..ix.nodes.len()).find(|&i| ix.attr(after,i,"Self")==Some(&result.created[0])).unwrap();
            let mut restored = after.to_owned();
            restored.replace_range(ix.nodes[i].start..ix.nodes[i].end, "");
            assert_eq!(&restored, original);
        }
    }
    let changed = ed.parts().clone();
    ed.undo().unwrap(); assert_eq!(ed.parts(), &before);
    ed.redo().unwrap(); assert_eq!(ed.parts(), &changed);
}

#[test]
fn create_rejections_roll_back_story_parts_and_all_targets() {
    let mut ed = fixture();
    let ids = ed.text_path_items().unwrap();
    let before = ed.parts().clone();
    assert!(ed.apply("already attached", &[create(ids.clone(), "")]).is_err());
    assert_eq!(ed.parts(), &before);
    ed.apply("remove", &[remove(ids.clone())]).unwrap();
    let empty = ed.parts().clone();
    for targets in [vec![], vec![ids[0].clone(), "missing".into()], vec![ids[0].clone(), ids[0].clone()]] {
        assert!(ed.apply("invalid", &[create(targets, "Hello")]).is_err());
        assert_eq!(ed.parts(), &empty);
    }
    ed.apply("lock", &[Command::SetItem{id:ids[0].clone(),locked:Some(true),visible:None}]).unwrap();
    let locked = ed.parts().clone();
    assert!(ed.apply("locked", &[create(ids, "")]).is_err());
    assert_eq!(ed.parts(), &locked);
}

#[test]
fn create_rejects_closed_geometry_without_changes() {
    let mut ed = named_fixture("textpath-formatted");
    let ids = ed.text_path_items().unwrap();
    ed.apply("remove", &[remove(ids.clone())]).unwrap();
    let base = ed.parts().clone();
    let (part, original) = base.iter().find(|(k,v)| k.starts_with("Spreads/") && v.contains(&format!("Self=\"{}\"", ids[0]))).unwrap();
    let changed = original.replace("PathOpen=\"true\"", "PathOpen=\"false\"");
    assert_ne!(&changed, original);
    let mut parts=base.clone(); parts.insert(part.clone(), changed);
    let mut ed=Editor::new(parts.clone());
    assert!(ed.apply("unsupported shape", &[create(ids.clone(), "")]).is_err());
    assert_eq!(ed.parts(), &parts);
    assert_eq!(ed.undo_label(), None);
}

/// A curve (handles off the anchors) and a path that runs right to left take the same default range as a straight
/// left-to-right line: from the start to the end of the path, measured along the curve.
#[test]
fn create_accepts_curves_and_reversed_lines_with_the_path_length() {
    let mut ed = named_fixture("textpath-formatted");
    let ids = ed.text_path_items().unwrap();
    ed.apply("remove", &[remove(ids.clone())]).unwrap();
    let base = ed.parts().clone();
    let (part, original) = base.iter().find(|(k,v)| k.starts_with("Spreads/") && v.contains(&format!("Self=\"{}\"", ids[0]))).unwrap();
    let ix = folio_edit::xml::Index::scan(original).unwrap();
    let item = (0..ix.nodes.len()).find(|&i| ix.attr(original,i,"Self")==Some(&ids[0])).unwrap();
    let points: Vec<_> = (item..ix.nodes.len()).filter(|&i| ix.local(original,i)=="PathPointType").collect();
    let anchor = ix.attr(original, points[0], "Anchor").unwrap();
    let end = ix.attr(original, points[1], "Anchor").unwrap();
    for (changed, length) in [
        (original.replace(&format!("RightDirection=\"{anchor}\""), "RightDirection=\"0 1\""), None),
        (original.replace(anchor, "TMP_POINT").replace(end, anchor).replace("TMP_POINT", end), Some("240")),
    ] {
        assert_ne!(&changed, original);
        let mut parts=base.clone(); parts.insert(part.clone(), changed);
        let mut ed=Editor::new(parts);
        ed.apply("create", &[create(ids.clone(), "")]).unwrap();
        let props = ed.text_path_options(&ids[0]).unwrap();
        let end: f64 = props["props"]["EndBracket"].as_str().unwrap().parse().unwrap();
        assert!(end > 0.0);
        if let Some(length) = length { assert_eq!(props["props"]["EndBracket"], length); }
    }
}
