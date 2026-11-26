//! Refused creation must leave no orphan Story or designmap registration.
use folio_edit::{Command, Editor, ItemKind, xml::Index};
use std::collections::BTreeMap;
use std::path::Path;

fn fixture() -> (Editor, String, String) {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic-create.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut entry, &mut s).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), s);
    }
    let dm = &parts["designmap.xml"];
    let dx = Index::scan(dm).unwrap();
    let doc = (0..dx.nodes.len())
        .find(|&i| dx.local(dm, i) == "Document")
        .unwrap();
    let layer = dx.value(dm, doc, "ActiveLayer").unwrap();
    let spread = (0..dx.nodes.len())
        .find(|&i| dx.local(dm, i) == "Spread")
        .unwrap();
    let t = &parts[dx.attr(dm, spread, "src").unwrap()];
    let ix = Index::scan(t).unwrap();
    let p = (0..ix.nodes.len())
        .find(|&i| ix.local(t, i) == "Page")
        .unwrap();
    let page = ix.value(t, p, "Self").unwrap();
    (Editor::new(parts), page, layer)
}

fn create(page: &str, kind: ItemKind, rect: [f64; 4]) -> Command {
    Command::CreateItem {
        page: page.into(),
        kind,
        rect,
        layer: None,
        sides: 6,
    }
}

#[test]
fn creation_rejects_invalid_geometry_without_history() {
    let (mut ed, page, _) = fixture();
    let before = ed.parts().clone();
    for command in [
        create(&page, ItemKind::Rectangle, [0., 0., 0., 10.]),
        create(&page, ItemKind::TextFrame, [10., 0., 0., 10.]),
        create(&page, ItemKind::Line, [0., 0., 0., 0.]),
        create(&page, ItemKind::Ellipse, [0., 0., f64::NAN, 10.]),
        create("missing-page", ItemKind::TextFrame, [0., 0., 100., 100.]),
    ] {
        assert!(ed.apply("invalid creation", &[command]).is_err());
        assert_eq!(ed.parts(), &before);
        assert!(ed.undo_label().is_none());
    }
}

#[test]
fn creation_refuses_hidden_and_locked_active_layers() {
    for (visible, locked) in [(false, false), (true, true)] {
        let (mut ed, page, layer) = fixture();
        ed.apply(
            "layer",
            &[Command::SetLayer { name: None, color: None, show_guides: None, lock_guides: None,
                id: layer,
                visible: Some(visible),
                locked: Some(locked),
                printable: None,
            }],
        )
        .unwrap();
        let before = ed.parts().clone();
        let error = ed
            .apply(
                "draw",
                &[create(&page, ItemKind::TextFrame, [0., 0., 100., 100.])],
            )
            .unwrap_err();
        assert_eq!(error.code, "locked");
        assert_eq!(ed.parts(), &before);
        assert_eq!(ed.undo_label(), Some("layer"));
    }
}

#[test]
fn failed_batch_rolls_back_new_story_and_registration() {
    let (mut ed, page, _) = fixture();
    let before = ed.parts().clone();
    assert!(
        ed.apply(
            "draw two",
            &[
                create(&page, ItemKind::TextFrame, [0., 0., 100., 100.]),
                create(&page, ItemKind::Rectangle, [0., 0., 0., 0.]),
            ]
        )
        .is_err()
    );
    assert_eq!(ed.parts(), &before);
    assert!(ed.undo_label().is_none());
}
