//! Object > Captions end to end on the oracle's `variables` fixture (artifacts/captions-20261023/STATUS.ko.md for the measurements).
use folio_edit::{Command, Editor};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn oracle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}

fn open() -> Editor {
    let f = std::fs::File::open(oracle().join("fixtures/variables.idml")).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        out.insert(e.name().unwrap().into_owned(), s);
    }
    Editor::new(out)
}

fn first_page(ed: &Editor) -> String {
    ed.parts().iter().filter(|(k, _)| k.starts_with("Spreads/")).find_map(|(_, t)| {
        let ix = folio_edit::xml::Index::scan(t).ok()?;
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page").and_then(|i| ix.attr(t, i, "Self").map(str::to_owned))
    }).unwrap()
}

fn wire(ed: &mut Editor, v: serde_json::Value) -> Vec<String> {
    let c = folio_edit::commands::oracle::command(&v, v["story"].as_str()).unwrap().unwrap();
    ed.apply("caption", &[c]).unwrap().created
}

fn placed(ed: &mut Editor, page: &str) -> String {
    let rect = ed.apply("rect", &[Command::CreateItem { page: page.into(), kind: folio_edit::ItemKind::Rectangle, rect: [100.0, 100.0, 300.0, 200.0], layer: None, sides: 4 }]).unwrap().created[0].clone();
    let source = folio_edit::GraphicSource::read(&oracle().join("fixtures/Links/place-plain.png"), "file:/oracle/Links/place-plain.png".into()).unwrap();
    ed.apply("place", &[Command::PlaceGraphic { source, id: Some(rect.clone()), page: None, point: None, rect: None, layer: None }]).unwrap();
    rect
}

/// A frame's path bounds in spread coordinates (a plain translation as its transform).
fn bounds(ed: &Editor, id: &str) -> [f64; 4] {
    for t in ed.parts().values() {
        let Ok(ix) = folio_edit::xml::Index::scan(t) else { continue };
        let Some(i) = ix.by_self(t, id) else { continue };
        let m: Vec<f64> = ix.attr(t, i, "ItemTransform").unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
        let pts: Vec<(f64, f64)> = (0..ix.nodes.len()).filter(|&k| ix.nodes[k].start > ix.nodes[i].start && ix.nodes[k].end <= ix.nodes[i].end && ix.local(t, k) == "PathPointType")
            .map(|k| { let v: Vec<f64> = ix.attr(t, k, "Anchor").unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect(); (v[0] + m[4], v[1] + m[5]) }).collect();
        let x: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let y: Vec<f64> = pts.iter().map(|p| p.1).collect();
        let (lo, hi) = (|v: &Vec<f64>| v.iter().cloned().fold(f64::MAX, f64::min), |v: &Vec<f64>| v.iter().cloned().fold(f64::MIN, f64::max));
        return [lo(&x), lo(&y), hi(&x), hi(&y)];
    }
    panic!("{id} not found")
}

fn all(ed: &Editor) -> String {
    ed.parts().values().cloned().collect::<Vec<_>>().join("\n")
}

#[test]
fn live_caption_below_the_graphic() {
    let mut ed = open();
    let page = first_page(&ed);
    let rect = placed(&mut ed, &page);
    let made = wire(&mut ed, json!({"op": "generateCaptions", "ids": [rect.clone()], "page": page, "live": true}));
    let text = all(&ed);
    assert!(text.contains("Live Metadata Caption: Name"), "the variable is defined");
    assert!(text.contains("MetadataProviderName=\"$ID/#LinkInfoNameStr\""));
    assert!(text.contains("TextVariableInstance"), "the story holds the instance");
    // one frame, 200 wide, touching the graphic frame from below (top inset = the default offset 1)
    let (b, g) = (bounds(&ed, made.last().unwrap()), bounds(&ed, &rect));
    assert!((b[0] - g[0]).abs() < 1e-6 && (b[2] - g[2]).abs() < 1e-6, "{b:?} {g:?}");
    assert!((b[1] - g[3]).abs() < 1e-6, "{b:?} {g:?}");
    assert!((b[3] - g[3] - 9.723876953125).abs() < 1e-6, "{b:?} {g:?}");
}

#[test]
fn setup_rows_text_and_static_caption() {
    let mut ed = open();
    let page = first_page(&ed);
    let rect = placed(&mut ed, &page);
    wire(&mut ed, json!({"op": "setCaptionSetup", "props": {"rows": [{"before": "Photo: ", "metadata": "Name", "after": " (c)"}, {"metadata": "Page"}], "position": "above", "offset": 5}}));
    wire(&mut ed, json!({"op": "generateCaptions", "ids": [rect], "page": page, "live": false}));
    let text = all(&ed);
    assert!(text.contains("Photo: place-plain.png (c)") || (text.contains("Photo: ") && text.contains("place-plain.png") && text.contains(" (c)")));
    assert!(text.contains("folio.captionSetup"), "the setup stays in the document");
    assert!(!text.contains("Live Metadata Caption"), "a static caption makes no variable");
}

#[test]
fn caption_range_only_enables_selected_live_variable() {
    let mut ed = open();
    let page = first_page(&ed);
    let rect = placed(&mut ed, &page);
    wire(&mut ed, json!({"op":"setCaptionSetup","props":{"rows":[{"before":"Photo: ","metadata":"Name","after":"!"}]}}));
    let made = wire(&mut ed, json!({"op":"generateCaptions","ids":[rect],"page":page,"live":true}));
    let frame = made.last().unwrap();
    assert!(!ed.caption_range(frame,0,0).unwrap());
    assert!(!ed.caption_range(frame,0,7).unwrap());
    assert!(ed.caption_range(frame,7,8).unwrap());
    wire(&mut ed,json!({"op":"convertToStaticCaption","story":frame,"start":7,"end":8}));
    assert!(!ed.caption_range(frame,7,8).unwrap());
    assert!(all(&ed).contains("place-plain.png"));
    ed.undo().unwrap();
    assert!(ed.caption_range(frame,7,8).unwrap());
}

#[test]
fn captions_show_the_xmp_the_picture_carries() {
    // build-a: the file's own packet stays with the graphic; static and live captions show its properties
    let mut ed = open();
    let page = first_page(&ed);
    let rect = ed.apply("rect", &[Command::CreateItem { page: page.clone(), kind: folio_edit::ItemKind::Rectangle, rect: [100.0, 100.0, 300.0, 200.0], layer: None, sides: 4 }]).unwrap().created[0].clone();
    let source = folio_edit::GraphicSource::read(&oracle().join("fixtures/Links/place-xmp.jpg"), "file:/oracle/Links/place-xmp.jpg".into()).unwrap();
    assert!(source.xmp.as_deref().is_some_and(|x| x.contains("Fixture title")), "{:?}", source.xmp);
    ed.apply("place", &[Command::PlaceGraphic { source, id: Some(rect.clone()), page: None, point: None, rect: None, layer: None }]).unwrap();
    assert!(all(&ed).contains("MetadataPacketPreference"), "the packet is kept with the graphic");
    wire(&mut ed, json!({"op": "setCaptionSetup", "props": {"rows": [{"metadata": "Title"}, {"before": "by ", "metadata": "Author"}, {"metadata": "Description"}]}}));
    wire(&mut ed, json!({"op": "generateCaptions", "ids": [rect], "page": page, "live": false}));
    let text = all(&ed);
    assert!(text.contains("Fixture title") && text.contains("by Folio Test") && text.contains("&lt;No data from link&gt;"), "static caption");
    ed.undo().unwrap();
    wire(&mut ed, json!({"op": "generateCaptions", "ids": [rect], "page": page, "live": true}));
    assert!(all(&ed).contains("ResultText=\"Fixture title\""), "a live caption shows the title");
}
