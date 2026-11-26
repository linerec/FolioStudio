//! Attributes panel and Script Label panel on the basic fixture (oracle basic/ap-* measures the IDML against InDesign; here the
//! model read back, what does not apply, groups, undo, labels). A script is refused where the setting does not apply; a selection of
//! several objects and the inside of a group skip those members.
use folio_edit::{Command, Editor, commands::RegisteredCommand, xml::Index};
use folio_model::{ItemKind, PageItem};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, BTreeMap<String, String>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut labels = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        if name.contains("Spread") {
            let ix = Index::scan(&t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some("oracle.id") {
                    let owner = ix.ancestors(i).find_map(|a| ix.attr(&t, a, "Self")).unwrap();
                    labels.insert(ix.attr(&t, i, "Value").unwrap().to_owned(), owner.to_owned());
                }
            }
        }
        parts.insert(name, t);
    }
    (Editor::new(parts), labels)
}

fn set(ids: &[&str], props: serde_json::Value) -> Command {
    let props = props.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    RegisteredCommand::SetObjectAttributes { ids: ids.iter().map(|s| s.to_string()).collect(), props }.into()
}

/// An attribute of the element `id` in the spread parts.
fn attr(e: &Editor, id: &str, name: &str) -> Option<String> {
    for (part, t) in e.parts() {
        if !part.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) {
            return ix.attr(t, i, name).map(str::to_owned);
        }
    }
    panic!("no {id}");
}

/// The label pairs (`Properties/Label/KeyValuePair`) of the element `id`.
fn labels(e: &Editor, id: &str) -> Vec<(String, String)> {
    for (part, t) in e.parts() {
        if !part.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) {
            return ix.children(i).iter().flat_map(|&p| ix.named(t, p, "Label")).flat_map(|l| ix.named(t, l, "KeyValuePair"))
                .map(|k| (ix.value(t, k, "Key").unwrap(), ix.value(t, k, "Value").unwrap())).collect();
        }
    }
    panic!("no {id}");
}

fn find<'a>(items: &'a [PageItem], id: &str) -> Option<&'a PageItem> {
    for it in items {
        if it.id == id {
            return Some(it);
        }
        if let ItemKind::Group { children } = &it.kind
            && let Some(f) = find(children, id)
        {
            return Some(f);
        }
    }
    None
}

/// (nonprinting, overprint fill, overprint stroke) as the reader sees the item.
fn read(e: &Editor, id: &str) -> (bool, bool, bool) {
    let doc = folio_idml::read_parts(e.parts(), Path::new("."), &folio_idml::ReaderData::default()).unwrap();
    let item = doc.spreads.iter().filter_map(|s| find(&s.items, id)).next().unwrap();
    (item.print.nonprinting, item.print.overprint_fill, item.print.overprint_stroke)
}

#[test]
fn overprint_is_written_where_it_differs_and_goes_away_when_off_and_undo_takes_it_back() {
    let (mut e, l) = fixture();
    let id = &l["r-rot"];
    assert_eq!(read(&e, id), (false, false, false));
    e.apply("on", &[set(&[id], serde_json::json!({"overprintFill": true, "overprintStroke": true, "nonprinting": true}))]).unwrap();
    assert_eq!(attr(&e, id, "OverprintFill").as_deref(), Some("true"));
    assert_eq!(attr(&e, id, "OverprintStroke").as_deref(), Some("true"));
    assert_eq!(attr(&e, id, "Nonprinting").as_deref(), Some("true"));
    assert_eq!(read(&e, id), (true, true, true));
    e.apply("off", &[set(&[id], serde_json::json!({"overprintFill": false, "overprintStroke": false, "nonprinting": false}))]).unwrap();
    for name in ["OverprintFill", "OverprintStroke", "Nonprinting"] {
        assert_eq!(attr(&e, id, name), None, "{name} is the default");
    }
    e.undo().unwrap();
    assert_eq!(read(&e, id), (true, true, true));
    e.undo().unwrap();
    assert_eq!(read(&e, id), (false, false, false));
    e.redo().unwrap();
    assert_eq!(read(&e, id), (true, true, true));
}

#[test]
fn what_does_not_apply_is_refused_for_one_object_and_skipped_for_several() {
    let (mut e, l) = fixture();
    // r-fill has a fill and no stroke; r-plain a stroke and no fill; a graphic line has no fill
    for (item, prop) in [("r-fill", "overprintStroke"), ("r-plain", "overprintFill"), ("line", "overprintFill"), ("r-plain", "overprintGap")] {
        let err = e.apply("bad", &[set(&[&l[item]], serde_json::json!({ prop: true }))]).unwrap_err().to_string();
        assert!(err.contains("does not apply"), "{item} {prop}: {err}");
    }
    let before = e.parts().clone();
    e.apply("several", &[set(&[&l["r-fill"], &l["r-plain"]], serde_json::json!({"overprintFill": true}))]).unwrap();
    assert_eq!(attr(&e, &l["r-fill"], "OverprintFill").as_deref(), Some("true"));
    assert_eq!(attr(&e, &l["r-plain"], "OverprintFill"), None);
    assert_ne!(&before, e.parts());
    // nonprinting always applies
    e.apply("np", &[set(&[&l["line"]], serde_json::json!({"nonprinting": true}))]).unwrap();
    assert_eq!(attr(&e, &l["line"], "Nonprinting").as_deref(), Some("true"));
}

#[test]
fn a_gap_needs_a_stroke_type_and_a_gap_colour() {
    let (mut e, l) = fixture();
    let id = &l["r-plain"];
    e.apply("dashed", &[Command::SetStroke { ids: vec![id.clone()], change: folio_edit::StrokeChange { kind: Some("StrokeStyle/$ID/Dashed".into()), ..Default::default() } }]).unwrap();
    assert!(e.apply("bad", &[set(&[id], serde_json::json!({"overprintGap": true}))]).is_err(), "no gap colour");
    e.apply("gap", &[Command::SetStroke { ids: vec![id.clone()], change: folio_edit::StrokeChange { gap_color: Some("Color/Black".into()), ..Default::default() } }]).unwrap();
    e.apply("ok", &[set(&[id], serde_json::json!({"overprintGap": true}))]).unwrap();
    assert_eq!(attr(&e, id, "OverprintGap").as_deref(), Some("true"));
}

#[test]
fn a_group_gives_its_setting_to_the_members_where_it_applies() {
    let (mut e, l) = fixture();
    // both members are filled and have no stroke: a group of them has nothing to overprint on a stroke (InDesign refuses a script)
    let err = e.apply("bad", &[set(&[&l["grp"]], serde_json::json!({"overprintStroke": true}))]).unwrap_err().to_string();
    assert!(err.contains("does not apply"), "{err}");
    e.apply("group", &[set(&[&l["grp"]], serde_json::json!({"nonprinting": true, "overprintFill": true}))]).unwrap();
    for id in ["grp", "grp-a", "grp-b"] {
        assert_eq!(attr(&e, &l[id], "Nonprinting").as_deref(), Some("true"), "{id}");
        assert_eq!(attr(&e, &l[id], "OverprintFill").as_deref(), Some("true"), "{id}");
        assert_eq!(attr(&e, &l[id], "OverprintStroke"), None, "{id}");
    }
    e.undo().unwrap();
    assert_eq!(attr(&e, &l["grp-a"], "Nonprinting"), None);
}

#[test]
fn the_label_is_the_script_label_and_an_empty_one_takes_it_away() {
    let (mut e, l) = fixture();
    let id = &l["oval"];
    let keep = ("oracle.id".to_owned(), "oval".to_owned());
    assert_eq!(labels(&e, id), vec![keep.clone()]);
    e.apply("one", &[set(&[id], serde_json::json!({"label": "one"}))]).unwrap();
    assert_eq!(labels(&e, id), vec![keep.clone(), ("Label".into(), "one".into())]);
    e.apply("two", &[set(&[id], serde_json::json!({"label": "first\rsecond & <third>"}))]).unwrap();
    assert_eq!(labels(&e, id), vec![keep.clone(), ("Label".into(), "first\rsecond & <third>".into())]);
    let text = e.parts().values().find(|t| t.contains("first&#xd;second &amp; &lt;third&gt;")).is_some();
    assert!(text, "a return is written as InDesign writes it");
    e.apply("none", &[set(&[id], serde_json::json!({"label": ""}))]).unwrap();
    assert_eq!(labels(&e, id), vec![keep]);
    // clearing what is not there changes nothing
    let before = e.parts().clone();
    e.apply("again", &[set(&[id], serde_json::json!({"label": ""}))]).unwrap();
    assert_eq!(&before, e.parts());
}

#[test]
fn the_only_label_goes_with_its_element_and_a_new_one_comes_back() {
    let (mut e, l) = fixture();
    // the fixture's objects carry `oracle.id`, so make one that has only the script label
    let id = &l["oval"];
    e.apply("a", &[set(&[id], serde_json::json!({"label": "x"}))]).unwrap();
    e.apply("b", &[set(&[id], serde_json::json!({"label": ""}))]).unwrap();
    e.apply("c", &[set(&[id], serde_json::json!({"label": "y"}))]).unwrap();
    assert_eq!(labels(&e, id).last().unwrap(), &("Label".to_owned(), "y".to_owned()));
}

#[test]
fn unknown_names_and_wrong_kinds_are_refused() {
    let (mut e, l) = fixture();
    let err = e.apply("bad", &[set(&[&l["oval"]], serde_json::json!({"overprint": true}))]).unwrap_err().to_string();
    assert!(err.contains("is not an attribute"), "{err}");
    let err = e.apply("bad", &[set(&[&l["oval"]], serde_json::json!({"nonprinting": "yes"}))]).unwrap_err().to_string();
    assert!(err.contains("true or false"), "{err}");
    let err = e.apply("bad", &[set(&["u-nothing"], serde_json::json!({"nonprinting": true}))]).unwrap_err().to_string();
    assert!(err.contains("u-nothing"), "{err}");
}
