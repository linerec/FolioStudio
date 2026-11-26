//! screens-import: pictures InDesign places that Folio now places (BMP, low bit depth palette PNG, a GIF with a transparent
//! colour) and a text file placed into a shape. InDesign's own output is `oracle/goldens/basic/<name>/after`; those goldens were
//! captured in a session where InDesign renumbered the objects of the fixture, so the ids differ from the fixture's: here the
//! picture's own attributes (Space, ImageTypeName, ActualPpi, ItemTransform, GraphicBounds, link format) and the text frame's
//! element are compared with the ids taken out, instead of the whole-spread comparison of `oracle.rs`.
use folio_edit::{Command, Editor, GraphicSource};
use std::{collections::BTreeMap, io::Read, path::{Path, PathBuf}};

fn oracle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}
fn editor() -> Editor {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(oracle().join("fixtures/basic.idml")).unwrap()).unwrap();
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
fn labelled(ed: &Editor, label: &str) -> String {
    for text in ed.parts().values() {
        if !text.contains(&format!("Value=\"{label}\"")) {
            continue;
        }
        let ix = folio_edit::xml::Index::scan(text).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.attr(text, i, "Value") == Some(label) {
                return ix.ancestors(i).find_map(|a| ix.attr(text, a, "Self").map(str::to_owned)).unwrap();
            }
        }
    }
    panic!("no label {label}")
}
fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    Some(tag[at..tag[at..].find('"')? + at].to_owned())
}
/// The first `<name ...>` opening tag in the parts' text.
fn tag_of(parts: &BTreeMap<String, String>, name: &str) -> String {
    for text in parts.values() {
        if let Some(at) = text.find(&format!("<{name} ")) {
            return text[at..text[at..].find('>').unwrap() + at + 1].to_owned();
        }
    }
    panic!("no {name}")
}
fn golden_parts(name: &str) -> BTreeMap<String, String> {
    let dir = oracle().join("goldens/basic").join(name).join("after/Spreads");
    std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).map(|p| (p.display().to_string(), std::fs::read_to_string(p).unwrap())).collect()
}
fn spreads(ed: &Editor) -> BTreeMap<String, String> {
    ed.parts().iter().filter(|(k, _)| k.starts_with("Spreads/")).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// A picture is placed into `r-plain`; its Image and Link carry what InDesign wrote.
fn placed(file: &str, golden: &str) {
    let mut ed = editor();
    let frame = labelled(&ed, "r-plain");
    let path = oracle().join("fixtures/Links").join(file);
    let source = GraphicSource::read(&path, "file:///x/".to_owned() + file).unwrap_or_else(|e| panic!("{file}: {e}"));
    ed.apply("place", &[Command::PlaceGraphic { source, id: Some(frame), page: None, point: None, rect: None, layer: None }]).unwrap_or_else(|e| panic!("{file}: {e:?}"));
    // the Image's own tag is only its attributes; ids, volatile counters and the link's file stamp are not compared
    let one = |parts: &BTreeMap<String, String>| BTreeMap::from([("frame".to_owned(), element(parts, "Rectangle", "r-plain"))]);
    let (ours, theirs) = (one(&spreads(&ed)), one(&golden_parts(golden)));
    for (who, parts) in [("ours", &ours), ("theirs", &theirs)] {
        assert!(parts["frame"].contains("<Image"), "{file}: {who} has no Image: {}", &parts["frame"][..parts["frame"].len().min(300)]);
    }
    let (a, b) = (tag_of(&ours, "Image"), tag_of(&theirs, "Image"));
    for name in ["Space", "ActualPpi", "EffectivePpi", "ImageTypeName", "ItemTransform", "FillColor", "ImageRenderingIntent"] {
        let (x, y) = (attr(&a, name), attr(&b, name));
        if name == "ItemTransform" {
            // InDesign's matrix is of the frame's position: equal within a hundredth of a point
            let n = |v: &Option<String>| v.clone().unwrap().split_whitespace().map(|n| n.parse::<f64>().unwrap()).collect::<Vec<_>>();
            assert!(n(&x).iter().zip(n(&y)).all(|(p, q)| (p - q).abs() < 0.01), "{file} {name}: {x:?} / {y:?}");
        } else {
            assert_eq!(x, y, "{file} {name}
ours:   {a}
theirs: {b}");
        }
    }
    let (a, b) = (tag_of(&ours, "Link"), tag_of(&theirs, "Link"));
    assert_eq!(attr(&a, "LinkResourceFormat"), attr(&b, "LinkResourceFormat"), "{file} link format");
    assert_eq!(attr(&a, "StoredState"), attr(&b, "StoredState"), "{file} stored state");
    let (a, b) = (tag_of(&ours, "GraphicBounds"), tag_of(&theirs, "GraphicBounds"));
    for name in ["Right", "Bottom"] {
        let n = |t: &str| attr(t, name).unwrap().parse::<f64>().unwrap();
        assert!((n(&a) - n(&b)).abs() < 0.001, "{file} {name}: {a} / {b}");
    }
}

#[test]
fn bmp_pictures_are_placed_as_indesign_places_them() {
    placed("place-bmp24.bmp", "place-bmp24");
    placed("place-bmp8.bmp", "place-bmp8");
    placed("place-bmp4.bmp", "place-bmp4");
    placed("place-bmp1.bmp", "place-bmp1");
}

#[test]
fn palette_pictures_of_few_bits_and_a_transparent_gif_are_placed() {
    placed("place-indexed4.png", "place-png-indexed4");
    placed("place-indexed2.png", "place-png-indexed2");
    placed("place-indexed1.png", "place-png-indexed1");
    placed("place-transparent.gif", "place-gif-transparent");
}

#[test]
fn formats_folio_does_not_place_are_refused_by_name() {
    let dir = std::env::temp_dir().join(format!("folio-screens-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // an EPS (PostScript) file is placed now (build-a: shown through its preview); a BMP of 16 bits is still refused by name
    let eps = dir.join("a.eps");
    std::fs::write(&eps, b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 100 100\n").unwrap();
    let source = GraphicSource::read(&eps, "file:///a.eps".into()).unwrap();
    assert_eq!((source.format.as_str(), source.size), ("EPS", [100.0, 100.0]));
    let mut bmp16 = std::fs::read(oracle().join("fixtures/Links/place-bmp24.bmp")).unwrap();
    bmp16[28] = 16;
    let path = dir.join("b.bmp");
    std::fs::write(&path, bmp16).unwrap();
    let e = GraphicSource::read(&path, "file:///b.bmp".into()).unwrap_err();
    assert!(e.contains("아직 지원하지 않습니다"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Every tag with its attributes in alphabetical order (InDesign and Folio write them in different orders).
fn sorted_attributes(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let end = rest[at..].find('>').unwrap() + at;
        let tag = &rest[at + 1..end];
        let (body, close) = match tag.strip_suffix('/') { Some(b) => (b, "/"), None => (tag, "") };
        let (name, mut attrs) = (body.split(' ').next().unwrap(), Vec::new());
        let mut r = &body[name.len()..];
        while let Some(eq) = r.find("=\"") {
            let key = r[..eq].trim();
            let value_end = r[eq + 2..].find('"').unwrap() + eq + 2;
            attrs.push(format!("{key}=\"{}\"", &r[eq + 2..value_end]));
            r = &r[value_end + 1..];
        }
        attrs.sort();
        out.push('<');
        out.push_str(name);
        for a in &attrs {
            out.push(' ');
            out.push_str(a);
        }
        out.push_str(close);
        out.push('>');
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}
/// What the tags of an element say once the ids and the whitespace between tags are taken out.
fn flatten(element: &str) -> String {
    let mut out = String::new();
    for line in element.lines() {
        out.push_str(line.trim());
    }
    let mut text = sorted_attributes(&out.replace(" />", "/>"));
    for name in ["Self", "ParentStory", "ItemLayer"] {
        while let Some(at) = text.find(&format!(" {name}=\"")) {
            let start = at + name.len() + 3;
            let end = text[start..].find('"').unwrap() + start;
            text.replace_range(at..end + 1, "");
        }
    }
    text
}
fn element(parts: &BTreeMap<String, String>, tag: &str, label: &str) -> String {
    for text in parts.values() {
        let mut from = 0;
        while let Some(at) = text[from..].find(&format!("<{tag} ")) {
            let start = from + at;
            let end = text[start..].find(&format!("</{tag}>")).unwrap() + start + tag.len() + 3;
            if text[start..end].contains(&format!("Value=\"{label}\"")) {
                return text[start..end].to_owned();
            }
            from = end;
        }
    }
    panic!("no {tag} labelled {label}")
}

#[test]
fn a_text_file_placed_into_a_shape_makes_it_a_text_frame() {
    for (label, golden, source) in [("r-plain", "placetext-into-rect", "Rectangle"), ("oval", "placetext-into-oval", "Oval")] {
        let mut ed = editor();
        let id = labelled(&ed, label);
        let before = element(&spreads(&ed), source, label);
        let text = folio_textimport::read(&oracle().join("fixtures/Links/placetext-utf8.txt"), &folio_textimport::Options::default()).unwrap();
        ed.apply("place", &[Command::PlaceText { source: text, id: Some(id), page: None, rect: None, layer: None, point: None }]).unwrap();
        let ours = element(&spreads(&ed), "TextFrame", label);
        let theirs = element(&golden_parts(golden), "TextFrame", label);
        let (a, b) = (flatten(&ours), flatten(&theirs));
        if a != b {
            let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
            let from = at.saturating_sub(200);
            panic!("{label}: the shape {} became a text frame, differing at {at}
ours:   ...{}
theirs: ...{}", before.len(), &a[from..(at + 300).min(a.len())], &b[from..(at + 300).min(b.len())]);
        }
    }
}

#[test]
fn a_shape_with_a_picture_and_a_thread_refuse_a_text_file() {
    let text = folio_textimport::read(&oracle().join("fixtures/Links/placetext-utf8.txt"), &folio_textimport::Options::default()).unwrap();
    let mut ed = editor();
    let before = ed.parts().clone();
    for label in ["img", "tf-1"] {
        let id = labelled(&ed, label);
        let result = ed.apply("place", &[Command::PlaceText { source: text.clone(), id: Some(id), page: None, rect: None, layer: None, point: None }]);
        assert!(result.is_err(), "{label}");
        assert_eq!(ed.parts(), &before, "{label}: a refused placement changes nothing");
    }
}

/// The `EndnoteRange` of the endnote story among some parts, ids and the whitespace between tags taken out.
fn endnote_range(parts: &BTreeMap<String, String>) -> String {
    for text in parts.values() {
        if text.contains("IsEndnoteStory=\"true\"") {
            let at = text.find("<EndnoteRange ").expect("an endnote range");
            let end = text[at..].find("</EndnoteRange>").unwrap() + at + "</EndnoteRange>".len();
            let mut flat = flatten(&text[at..end]);
            let from = flat.find(" SourceEndnote=\"").unwrap();
            let to = flat[from + 16..].find('"').unwrap() + from + 17;
            flat.replace_range(from..to, "");
            return flat;
        }
    }
    panic!("no endnote story")
}

#[test]
fn an_endnote_keeps_the_character_formatting_of_its_runs() {
    // docxtext3-q04: "Plain **bold** end." in the endnote; InDesign (the golden, ids renumbered) puts a character range per run in the endnote range
    let mut zip = zip::ZipArchive::new(std::fs::File::open(oracle().join("fixtures/docxtext2.idml")).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if !entry.is_dir() {
            let name = entry.name().unwrap().into_owned();
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            parts.insert(name, text);
        }
    }
    let mut ed = Editor::new(parts);
    let page = ed.parts().values().find_map(|t| t.find("<Page Self=\"").map(|at| t[at + 12..t[at + 12..].find('"').unwrap() + at + 12].to_owned())).unwrap();
    let text = folio_textimport::read(&oracle().join("fixtures/Links/docxtext3-q04.docx"), &folio_textimport::Options::default()).unwrap();
    assert!(!text.warnings.iter().any(|w| w.contains("미주 안의 글자 서식")), "{:?}", text.warnings);
    ed.apply("place", &[Command::PlaceText { source: text, id: None, page: Some(page), rect: Some([72., 72., 540., 720.]), layer: None, point: None }]).unwrap();
    let golden: BTreeMap<String, String> = std::fs::read_dir(oracle().join("goldens/docxtext2/screens-endnote-bold/after/Stories")).unwrap()
        .map(|e| e.unwrap().path()).map(|p| (p.display().to_string(), std::fs::read_to_string(p).unwrap())).collect();
    let (ours, theirs) = (endnote_range(ed.parts()), endnote_range(&golden));
    assert_eq!(ours, theirs);
    assert!(ours.contains("FontStyle=\"Bold\""), "{ours}");
}
