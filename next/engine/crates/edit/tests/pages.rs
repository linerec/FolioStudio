//! Transaction and data-preservation boundaries not exercised by the native print oracle.
use folio_edit::{Command, Editor};
use std::collections::BTreeMap;
use std::io::Read;

fn fixture() -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../oracle/fixtures/pages.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(e.name().unwrap().into_owned(), t);
    }
    parts
}
fn page_ids(parts: &BTreeMap<String, String>) -> Vec<String> {
    parts
        .iter()
        .filter(|(p, _)| p.starts_with("Spreads/"))
        .flat_map(|(_, t)| {
            let ix = folio_edit::xml::Index::scan(t).unwrap();
            (0..ix.nodes.len())
                .filter(|&i| ix.local(t, i) == "Page")
                .map(|i| ix.attr(t, i, "Self").unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect()
}
#[test]
fn invalid_page_batch_is_atomic() {
    let parts = fixture();
    let ids = page_ids(&parts);
    let mut ed = Editor::new(parts.clone());
    assert!(
        ed.apply(
            "invalid",
            &[
                Command::MovePages {
                    pages: vec![ids[2].clone()],
                    before: Some(ids[0].clone())
                },
                Command::DeletePages { pages: ids.clone() },
            ]
        )
        .is_err()
    );
    assert_eq!(ed.parts(), &parts);
    assert!(ed.undo_label().is_none());
    assert!(
        ed.apply(
            "invalid",
            &[Command::DocumentSetup {
                width: f64::NAN,
                height: 300.
            }]
        )
        .is_err()
    );
    assert_eq!(ed.parts(), &parts);
}
#[test]
fn shuffle_flag_does_not_discard_saved_composition() {
    let mut ed = Editor::new(fixture());
    let out = ed
        .apply(
            "shuffle",
            &[Command::SetPageShuffle {
                allowed: false,
                spread: None,
            }],
        )
        .unwrap();
    assert_eq!(out.parts, ["Resources/Preferences.xml"]);
    assert!(out.recompose.is_empty());
}
#[test]
fn new_blank_page_does_not_duplicate_page_guides_or_identity() {
    let mut parts = fixture();
    let ids = page_ids(&parts);
    let last = ids.last().unwrap();
    let (part, t) = parts
        .iter_mut()
        .find(|(p, t)| p.starts_with("Spreads/") && t.contains(&format!("Self=\"{last}\"")))
        .unwrap();
    let _ = part;
    let ix = folio_edit::xml::Index::scan(t).unwrap();
    let i = ix.by_self(t, last).unwrap();
    let at = ix.nodes[i].close_start;
    t.insert_str(at,"<Guide Self=\"u7fff\" Location=\"12\"/><Label><KeyValuePair Key=\"folio.page\" Value=\"keep-original\"/></Label>");
    let mut ed = Editor::new(parts.clone());
    ed.apply(
        "insert",
        &[Command::InsertPages {
            count: 1,
            before: None,
            master: None,
        }],
    )
    .unwrap();
    let all = ed.parts().values().cloned().collect::<String>();
    assert_eq!(all.matches("Self=\"u7fff\"").count(), 1);
    assert_eq!(all.matches("keep-original").count(), 1);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &parts);
}
