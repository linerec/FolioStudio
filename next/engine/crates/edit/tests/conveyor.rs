//! The conveyor of the Content Collector / Placer tools (server `conveyor.rs`) rests on three things of the editor: collecting is
//! the copy Edit ▸ Copy makes without touching the clipboard, a collected set splits into items that place alone, and placing an
//! item is a Paste. (The conveyor's own list and the request that places are tested in the server crate and by
//! `next/oracle/conveyor-ui-check.py`.)
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

/// The basic oracle document with its objects by their `oracle.id` label.
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
        if name.starts_with("Spreads/") {
            let ix = Index::scan(&t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some("oracle.id") {
                    let owner = ix.ancestors(i).find(|&a| ix.attr(&t, a, "Self").is_some()).unwrap();
                    labels.insert(ix.value(&t, i, "Value").unwrap(), ix.attr(&t, owner, "Self").unwrap().into());
                }
            }
        }
        parts.insert(name, t);
    }
    (Editor::new(parts), labels)
}

fn first_page(e: &Editor) -> String {
    for (name, t) in e.parts() {
        if name.starts_with("Spreads/") {
            let ix = Index::scan(t).unwrap();
            if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page") {
                return ix.attr(t, i, "Self").unwrap().to_owned();
            }
        }
    }
    panic!("no page");
}

fn count(e: &Editor, element: &str) -> usize {
    e.parts().iter().filter(|(n, _)| n.starts_with("Spreads/")).map(|(_, t)| {
        let ix = Index::scan(t).unwrap();
        (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == element).count()
    }).sum()
}

#[test]
fn collecting_leaves_the_clipboard_alone() {
    let (mut e, l) = fixture();
    assert!(!e.can_paste());
    let item = e.collect(&[l["r-plain"].clone()]).unwrap();
    assert!(!e.can_paste(), "the conveyor is not the clipboard");
    // an earlier Copy stays what Paste pastes
    e.copy(&[l["r-fill"].clone()]).unwrap();
    let before = e.clipboard().unwrap().summary();
    e.collect(&[l["r-plain"].clone()]).unwrap();
    assert_eq!(e.clipboard().unwrap().summary(), before);
    assert_eq!(item.summary().len(), 1);
    assert_eq!(item.summary()[0].0, l["r-plain"]);
    assert_eq!(item.summary()[0].1, "Rectangle");
    // an unnamed object (IDML `Name="$ID/"`) has no name in the list
    assert_eq!(item.summary()[0].2, "");
}

#[test]
fn collecting_changes_nothing_in_the_document() {
    let (e, l) = fixture();
    let before = e.parts().clone();
    e.collect(&[l["r-plain"].clone(), l["tf"].clone()]).unwrap();
    assert_eq!(e.parts(), &before);
    assert_eq!(e.rev(), 0);
}

#[test]
fn nothing_to_collect_is_refused_and_a_missing_object_too() {
    let (e, _) = fixture();
    assert_eq!(e.collect(&[]).unwrap_err().code, "invalid");
    assert_eq!(e.collect(&["nope".into()]).unwrap_err().code, "notFound");
}

#[test]
fn a_set_splits_into_items_that_each_hold_one_object_in_stacking_order() {
    let (e, l) = fixture();
    let set = e.collect(&[l["tf"].clone(), l["r-plain"].clone()]).unwrap();
    let whole = set.summary();
    assert_eq!(whole.len(), 2);
    let items = set.split();
    assert_eq!(items.len(), 2);
    let parts: Vec<_> = items.iter().map(|i| i.summary()).collect();
    assert!(parts.iter().all(|p| p.len() == 1));
    assert_eq!(parts.iter().map(|p| p[0].clone()).collect::<Vec<_>>(), whole);
}

#[test]
fn placing_an_item_is_one_undoable_paste_of_that_object_only() {
    let (mut e, l) = fixture();
    let items = e.collect(&[l["r-plain"].clone(), l["r-fill"].clone()]).unwrap().split();
    let page = first_page(&e);
    let (rects, rev) = (count(&e, "Rectangle"), e.rev());
    let before = e.parts().clone();
    // the second item: Content Placer click at (100, 120) on the page
    e.set_clipboard(Some(items[1].clone()));
    let out = e.apply("place", &[Command::Paste { page: Some(page.clone()), at: Some([100.0, 120.0]), layer: None, into: None, remember_layers: false }]).unwrap();
    assert_eq!(out.created.len(), 1);
    assert_eq!(count(&e, "Rectangle"), rects + 1);
    assert_eq!(e.rev(), rev + 1);
    // the item is still whole: it places again, as a copy of the same object
    e.set_clipboard(Some(items[1].clone()));
    let again = e.apply("place", &[Command::Paste { page: Some(page), at: Some([300.0, 120.0]), layer: None, into: None, remember_layers: false }]).unwrap();
    assert_eq!(again.created.len(), 1);
    assert_ne!(again.created[0], out.created[0]);
    assert_eq!(count(&e, "Rectangle"), rects + 2);
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}

fn transform_of(e: &Editor, id: &str) -> String {
    for (name, t) in e.parts() {
        if name.starts_with("Spreads/") {
            let ix = Index::scan(t).unwrap();
            if let Some(i) = ix.by_self(t, id) {
                return ix.attr(t, i, "ItemTransform").unwrap_or("").to_owned();
            }
        }
    }
    panic!("no object {id}");
}

#[test]
fn an_item_collected_before_an_edit_places_as_it_was() {
    let (mut e, l) = fixture();
    let item = e.collect(&[l["r-plain"].clone()]).unwrap();
    let was = transform_of(&e, &l["r-plain"]);
    e.apply("move", &[Command::Move { ids: vec![l["r-plain"].clone()], by: [50.0, 0.0] }]).unwrap();
    assert_ne!(transform_of(&e, &l["r-plain"]), was);
    let page = first_page(&e);
    e.set_clipboard(Some(item));
    // Paste in place: the copy comes where the object was when it was collected
    let out = e.apply("place", &[Command::Paste { page: Some(page), at: None, layer: None, into: None, remember_layers: false }]).unwrap();
    assert_eq!(out.created.len(), 1);
    assert_eq!(transform_of(&e, &out.created[0]), was);
}
