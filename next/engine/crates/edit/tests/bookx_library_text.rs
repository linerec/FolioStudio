//! A library item placed in text (`Asset.placeAsset(Text)` of InDesign 2025, probes bookx-c4-inline / bookx-c4-inline2, rules/bookx-c4.ko.md).
//! The positions and the IDML are checked against InDesign by the oracle scenarios `bookx-library-text/*`; this file holds what the
//! oracle cannot say: what is refused, that one undo takes it back, and that the story and the position are the caller's.
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/bookx-library-text.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(name, t);
    }
    Editor::new(parts)
}

/// The `Self` of the object labelled `oracle.id` = `label`.
fn labelled(e: &Editor, label: &str) -> String {
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.attr(t, i, "Value") == Some(label) {
                let owner = ix.ancestors(i).find(|&a| ix.attr(t, a, "Self").is_some()).unwrap();
                return ix.attr(t, owner, "Self").unwrap().to_owned();
            }
        }
    }
    panic!("no object labelled {label}");
}

fn story_of(e: &Editor, frame: &str) -> String {
    e.parts().values().find_map(|t| {
        let ix = Index::scan(t).ok()?;
        ix.attr(t, ix.by_self(t, frame)?, "ParentStory").map(str::to_owned)
    }).unwrap()
}

fn place(e: &mut Editor, ids: &[String], story: &str, start: usize, end: usize) -> Result<Vec<String>, String> {
    let snapshot = e.library_snapshot(ids).map_err(|x| x.to_string())?;
    let command: Command = serde_json::from_value(serde_json::json!({"op":"placeLibraryAssetInText","parts":snapshot.parts,"ids":snapshot.ids,"story":story,"start":start,"end":end})).unwrap();
    e.apply("library", &[command]).map(|o| o.created).map_err(|x| x.to_string())
}

#[test]
fn an_item_goes_into_the_story_and_one_undo_takes_it_back() {
    let mut e = fixture();
    let (rect, story) = (labelled(&e, "rect"), story_of(&e, &labelled(&e, "text")));
    let before = e.parts().clone();
    let created = place(&mut e, std::slice::from_ref(&rect), &story, 3, 3).unwrap();
    assert_eq!(created.len(), 1);
    let xml = e.part(&format!("Stories/Story_{story}.xml")).unwrap();
    // the object sits in the story between "hel" and "lo world", inline and pinned, with the stroke counted in its position
    let at = xml.find(&format!("Self=\"{}\"", created[0])).expect("the new object is in the story");
    assert!(xml[..at].contains("<Content>hel</Content>") && xml[at..].contains("<Content>lo world</Content>"));
    assert!(xml[at..].contains("AnchoredPosition=\"InlinePosition\"") && xml[at..].contains("PinPosition=\"true\""));
    assert!(xml[at..].contains("ItemTransform=\"1 0 0 1 -"), "the bottom left of the visible bounds is the origin: {}", &xml[at..at + 400]);
    // the object has no layer of its own
    assert!(!xml[at..xml[at..].find('>').unwrap() + at].contains("ItemLayer"));
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}

#[test]
fn a_range_is_replaced_and_an_empty_range_is_an_insertion_point() {
    let mut e = fixture();
    let (oval, story) = (labelled(&e, "oval"), story_of(&e, &labelled(&e, "second")));
    // "abcdef" + return + "second paragraph": characters 5..8 are "f", the return and "s"
    place(&mut e, std::slice::from_ref(&oval), &story, 5, 8).unwrap();
    let xml = e.part(&format!("Stories/Story_{story}.xml")).unwrap().to_owned();
    assert!(xml.contains("<Content>abcde</Content>"), "{xml}");
    assert!(xml.contains("<Content>econd paragraph</Content>"), "{xml}");
    assert!(!xml.contains("<Content>f</Content>"));
    let mut e = fixture();
    assert!(place(&mut e, &[oval], &story, 0, 99).is_err(), "a range outside the story is refused");
}

#[test]
fn what_cannot_be_placed_in_text_is_refused_and_leaves_the_document_as_it_was() {
    let mut e = fixture();
    let (rect, oval, rot_free, story) = (labelled(&e, "rect"), labelled(&e, "oval"), labelled(&e, "group"), story_of(&e, &labelled(&e, "text")));
    let before = e.parts().clone();
    // several objects: InDesign's placeAsset(Text) places nothing
    let error = place(&mut e, &[rect.clone(), oval], &story, 3, 3).unwrap_err();
    assert!(error.contains("several objects"), "{error}");
    assert_eq!(e.parts(), &before);
    // a story that is not there
    assert!(place(&mut e, std::slice::from_ref(&rect), "nope", 0, 0).is_err());
    assert_eq!(e.parts(), &before);
    // a turned object: its position in the line is not worked out (rules/bookx-c4.ko.md)
    let mut turned = e.parts().clone();
    for (_, t) in turned.iter_mut() {
        if t.contains(&format!("Self=\"{rect}\"")) {
            let from = t.find(&format!("Self=\"{rect}\"")).unwrap();
            let it = t[from..].find("ItemTransform=\"").unwrap() + from + "ItemTransform=\"".len();
            let end = t[it..].find('"').unwrap() + it;
            t.replace_range(it..end, "0.866 -0.5 0.5 0.866 0 0");
        }
    }
    let mut e2 = Editor::new(turned);
    let error = place(&mut e2, &[rect], &story, 3, 3).unwrap_err();
    assert!(error.contains("turned"), "{error}");
    // a group is one object: it goes in (members get the settings InDesign writes)
    let mut e3 = fixture();
    let created = place(&mut e3, &[rot_free], &story, 3, 3).unwrap();
    assert_eq!(created.len(), 1);
}
