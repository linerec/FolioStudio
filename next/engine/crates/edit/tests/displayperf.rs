//! Display Performance on the basic fixture (oracle basic/dp-* measures the IDML against InDesign; here the model read back, the
//! refusals, the undo and what Clear leaves alone): the frame and its picture take a setting together, the picture alone when the
//! content grabber names it, Clear sets every spread item back and keeps the parent page's.
use folio_edit::{Command, Editor, commands::RegisteredCommand, xml::Index};
use folio_model::{ItemKind, LocalDisplay, PageItem};
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

fn set(ids: &[&str], setting: &str, content: bool) -> Command {
    RegisteredCommand::SetDisplayPerformance { ids: ids.iter().map(|s| s.to_string()).collect(), setting: setting.into(), content }.into()
}

fn clear() -> Command {
    RegisteredCommand::ClearDisplayPerformance {}.into()
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

/// (the item's setting, its picture's) as the reader sees the document now.
fn read(e: &Editor, id: &str) -> (LocalDisplay, Option<LocalDisplay>) {
    let doc = folio_idml::read_parts(e.parts(), Path::new("."), &folio_idml::ReaderData::default()).unwrap();
    let item = doc.spreads.iter().filter_map(|s| find(&s.items, id)).chain(doc.master_spreads.iter().filter_map(|m| find(&m.items, id))).next().unwrap();
    let graphic = match &item.kind {
        ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => content.as_ref().map(|g| g.local_display),
        _ => None,
    };
    (item.local_display, graphic)
}

#[test]
fn a_frame_and_its_picture_take_the_setting_together_and_undo_takes_both_back() {
    let (mut e, l) = fixture();
    assert_eq!(read(&e, &l["img"]), (LocalDisplay::UseView, Some(LocalDisplay::UseView)));
    e.apply("fast", &[set(&[&l["img"]], "Optimized", false)]).unwrap();
    assert_eq!(read(&e, &l["img"]), (LocalDisplay::Optimized, Some(LocalDisplay::Optimized)));
    e.undo().unwrap();
    assert_eq!(read(&e, &l["img"]), (LocalDisplay::UseView, Some(LocalDisplay::UseView)));
    e.redo().unwrap();
    assert_eq!(read(&e, &l["img"]), (LocalDisplay::Optimized, Some(LocalDisplay::Optimized)));
}

#[test]
fn the_content_grabber_sets_the_picture_alone() {
    let (mut e, l) = fixture();
    e.apply("typical", &[set(&[&l["img"]], "Typical", true)]).unwrap();
    assert_eq!(read(&e, &l["img"]), (LocalDisplay::UseView, Some(LocalDisplay::Typical)));
}

#[test]
fn a_group_gives_its_setting_to_every_member() {
    let (mut e, l) = fixture();
    e.apply("high", &[set(&[&l["grp"]], "HighQuality", false)]).unwrap();
    for id in ["grp", "grp-a", "grp-b"] {
        assert_eq!(read(&e, &l[id]).0, LocalDisplay::HighQuality, "{id}");
    }
    e.apply("child", &[set(&[&l["grp-a"]], "Default", false)]).unwrap();
    assert_eq!(read(&e, &l["grp-a"]).0, LocalDisplay::UseView);
    assert_eq!(read(&e, &l["grp"]).0, LocalDisplay::HighQuality);
}

#[test]
fn clear_sets_the_spreads_back_and_leaves_the_parent_page() {
    let (mut e, l) = fixture();
    e.apply("many", &[set(&[&l["img"], &l["oval"], &l["grp"]], "Optimized", false), set(&[&l["parent-bar"]], "Optimized", false)]).unwrap();
    e.apply("clear", &[clear()]).unwrap();
    for id in ["img", "oval", "grp", "grp-a"] {
        assert_eq!(read(&e, &l[id]).0, LocalDisplay::UseView, "{id}");
    }
    assert_eq!(read(&e, &l["img"]).1, Some(LocalDisplay::UseView));
    assert_eq!(read(&e, &l["parent-bar"]).0, LocalDisplay::Optimized, "InDesign keeps parent page items' settings");
    // with nothing to clear it changes nothing
    let before = e.parts().clone();
    e.apply("clear again", &[clear()]).unwrap();
    assert_eq!(&before, e.parts());
}

#[test]
fn refusals_name_the_mistake() {
    let (mut e, l) = fixture();
    let err = e.apply("bad", &[set(&[&l["img"]], "Fast", false)]).unwrap_err().to_string();
    assert!(err.contains("not a display setting"), "{err}");
    let err = e.apply("bad", &[set(&["u-nothing"], "Optimized", false)]).unwrap_err().to_string();
    assert!(err.contains("u-nothing"), "{err}");
}
