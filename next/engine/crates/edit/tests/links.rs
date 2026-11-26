//! Relink and Embed Link on the basic fixture's placed picture (oracle basic-links measures the results against InDesign;
//! here the refusals and the undo): the link keeps its id, a frame or its graphic can be named, nothing to relink in an empty frame.
use folio_edit::{Command, Editor, GraphicSource, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, String) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut frame = String::new();
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
                if ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some("oracle.id") && ix.attr(&t, i, "Value") == Some("img") {
                    frame = ix.ancestors(i).find_map(|a| ix.attr(&t, a, "Self")).unwrap().into();
                }
            }
        }
        parts.insert(name, t);
    }
    assert!(!frame.is_empty());
    (Editor::new(parts), frame)
}

fn source(file: &str) -> GraphicSource {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(file);
    GraphicSource::read(&p, format!("file:/oracle/Links/{file}")).unwrap()
}

/// (the Link's Self, its URI) of the picture in the frame.
fn link(e: &Editor, frame: &str) -> (String, String) {
    for t in e.parts().values() {
        if !t.contains(&format!(" Self=\"{frame}\"")) {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        let f = ix.by_self(t, frame).unwrap();
        let l = (0..ix.nodes.len()).find(|&n| ix.local(t, n) == "Link" && ix.ancestors(n).any(|a| a == f)).unwrap();
        return (ix.attr(t, l, "Self").unwrap().into(), ix.value(t, l, "LinkResourceURI").unwrap());
    }
    panic!("no frame {frame}");
}

#[test]
fn relink_keeps_the_link_and_takes_the_new_file() {
    let (mut e, frame) = fixture();
    let (id, uri) = link(&e, &frame);
    e.apply("relink", &[Command::RelinkGraphic { source: source("place-icc.jpg"), id: frame.clone() }]).unwrap();
    let (id2, uri2) = link(&e, &frame);
    assert_eq!(id, id2, "the link element is the same");
    assert_ne!(uri, uri2);
    assert!(uri2.ends_with("place-icc.jpg"), "{uri2}");
    // the graphic can be named instead of its frame
    let graphic = {
        let t = e.parts().values().find(|t| t.contains(&format!(" Self=\"{frame}\""))).unwrap();
        let ix = Index::scan(t).unwrap();
        let f = ix.by_self(t, &frame).unwrap();
        let g = ix.children(f).iter().copied().find(|&c| ix.local(t, c) == "Image").unwrap();
        ix.attr(t, g, "Self").unwrap().to_owned()
    };
    e.apply("relink again", &[Command::RelinkGraphic { source: source("place-plain.png"), id: graphic }]).unwrap();
    assert!(link(&e, &frame).1.ends_with("place-plain.png"));
    assert_eq!(link(&e, &frame).0, id);
    // undo: all the way back
    while e.undo().is_some() {}
    assert_eq!(link(&e, &frame), (id, uri));
}

#[test]
fn a_frame_without_a_graphic_has_nothing_to_relink_or_embed() {
    let (mut e, _) = fixture();
    // r-plain: an empty rectangle of the fixture
    let empty = e.parts().values().find_map(|t| {
        let ix = Index::scan(t).unwrap();
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Value") == Some("r-plain")).and_then(|i| ix.ancestors(i).find_map(|a| ix.attr(t, a, "Self")).map(str::to_owned))
    }).unwrap();
    let err = e.apply("relink", &[Command::RelinkGraphic { source: source("place-icc.jpg"), id: empty.clone() }]).unwrap_err().to_string();
    assert!(err.contains("no linked graphic"), "{err}");
    let err = e.apply("embed", &[Command::EmbedLink { id: empty, data: "AAAA".into() }]).unwrap_err().to_string();
    assert!(err.contains("no linked graphic"), "{err}");
}

#[test]
fn embed_writes_the_contents_once_and_undo_takes_it_back() {
    let (mut e, frame) = fixture();
    let before = e.parts().clone();
    e.apply("embed", &[Command::EmbedLink { id: frame.clone(), data: "iVBORw0K\nGgo=".into() }]).unwrap();
    let t = e.parts().values().find(|t| t.contains("<Contents><![CDATA[iVBORw0K\nGgo=]]></Contents>")).expect("the contents are in the graphic");
    assert!(t.contains("StoredState=\"Embedded\""));
    let err = e.apply("embed again", &[Command::EmbedLink { id: frame.clone(), data: "AAAA".into() }]).unwrap_err().to_string();
    assert!(err.contains("embedded already"), "{err}");
    let err = e.apply("bad", &[Command::EmbedLink { id: frame, data: "not base64!".into() }]).unwrap_err().to_string();
    assert!(err.contains("embedded already") || err.contains("not base64"), "{err}");
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}

#[test]
fn relink_and_update_work_on_a_locked_frame() {
    let (mut e, frame) = fixture();
    e.apply("lock", &[Command::SetItem { id: frame.clone(), locked: Some(true), visible: None }]).unwrap();
    e.apply("relink", &[Command::RelinkGraphic { source: source("place-icc.jpg"), id: frame.clone() }]).unwrap();
    assert!(link(&e, &frame).1.ends_with("place-icc.jpg"));
}

#[test]
fn unembed_takes_the_contents_out_and_leaves_the_graphic_as_it_was() {
    let (mut e, frame) = fixture();
    let before = e.parts().clone();
    let err = e.apply("not embedded", &[Command::UnembedLink { id: frame.clone(), uri: "file:/x.png".into() }]).unwrap_err().to_string();
    assert!(err.contains("not embedded"), "{err}");
    e.apply("embed", &[Command::EmbedLink { id: frame.clone(), data: "iVBORw0K\nGgo=".into() }]).unwrap();
    assert_eq!(e.embedded_data(&frame).as_deref(), Some("iVBORw0K\nGgo="));
    e.apply("unembed", &[Command::UnembedLink { id: frame.clone(), uri: "file:/folio/pic.png".into() }]).unwrap();
    assert_eq!(e.embedded_data(&frame), None);
    let (_, uri) = link(&e, &frame);
    assert_eq!(uri, "file:/folio/pic.png");
    let t = e.parts().values().find(|t| t.contains(&format!(" Self=\"{frame}\""))).unwrap();
    assert!(!t.contains("<Contents>") && !t.contains("Embedded"));
    // undo twice: the document as it was
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}
