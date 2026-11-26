use folio_edit::{Command, Editor, GraphicSource};
use std::{collections::BTreeMap, io::Read, path::Path};

fn editor() -> Editor {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().unwrap().into_owned();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        parts.insert(name, text);
    }
    Editor::new(parts)
}
fn command(id: &str) -> Command {
    Command::PlaceGraphic {
        source: GraphicSource {
            uri: "folio:ref/test".into(),
            bytes: 333,
            size: [144., 96.],
            ppi: [72., 72.],
            format: "PNG".into(),
            embedded_profile: false,
            space: "RGB".into(),
            orientation: 1,
            pdf: None,
            xmp: None,
            rendering_intent: None,
        },
        id: Some(id.into()),
        page: None,
        point: None,
        rect: None,
        layer: None,
    }
}
fn labelled(ed: &Editor, label: &str) -> String {
    for text in ed.parts().values() {
        if !text.contains(&format!("Value=\"{label}\"")) {
            continue;
        }
        let ix = folio_edit::xml::Index::scan(text).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.attr(text, i, "Value") == Some(label) {
                return ix
                    .ancestors(i)
                    .find_map(|a| ix.attr(text, a, "Self").map(str::to_owned))
                    .unwrap();
            }
        }
    }
    panic!("no label {label}")
}
#[test]
fn refused_placement_is_atomic_and_preserves_redo() {
    let mut ed = editor();
    let plain = labelled(&ed, "r-plain");
    let locked = labelled(&ed, "r-locked");
    let before = ed.parts().clone();
    ed.apply("place", &[command(&plain)]).unwrap();
    ed.undo().unwrap();
    assert!(
        ed.apply("bad", &[command(&plain), command(&locked)])
            .is_err()
    );
    assert_eq!(ed.parts(), &before);
    assert_eq!(ed.redo_label(), Some("place"));
    ed.redo().unwrap();
    assert_ne!(ed.parts(), &before);
}
#[test]
fn text_frame_and_locked_layer_are_not_replaced() {
    let mut ed = editor();
    let before = ed.parts().clone();
    for name in ["tf", "r-on-locked"] {
        let id = labelled(&ed, name);
        assert!(ed.apply("bad", &[command(&id)]).is_err(), "{name}");
        assert_eq!(ed.parts(), &before);
    }
}
#[test]
fn invalid_source_dimensions_do_not_edit() {
    let mut ed = editor();
    let before = ed.parts().clone();
    let mut cmd = command(&labelled(&ed, "r-plain"));
    if let Command::PlaceGraphic { source, .. } = &mut cmd {
        source.size[0] = f64::NAN;
    }
    assert!(ed.apply("bad", &[cmd]).is_err());
    assert_eq!(ed.parts(), &before);
}

#[test]
fn unmeasured_profile_policy_does_not_silently_assign_document_rgb() {
    let ed = editor();
    let mut parts = ed.parts().clone();
    let dm = parts.get_mut("designmap.xml").unwrap();
    *dm = dm.replace(
        "RGBPolicy=\"PreserveEmbeddedProfiles\"",
        "RGBPolicy=\"IgnoreEmbeddedProfiles\"",
    );
    let before = parts.clone();
    let mut ed = Editor::new(parts);
    let mut cmd = command(&labelled(&ed, "r-plain"));
    if let Command::PlaceGraphic { source, .. } = &mut cmd {
        source.embedded_profile = true;
    }
    assert_eq!(ed.apply("place", &[cmd]).unwrap_err().code, "unsupported");
    assert_eq!(ed.parts(), &before);
}
