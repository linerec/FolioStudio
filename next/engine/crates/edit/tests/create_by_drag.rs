//! Drawing with the mouse on a spread turned by View > Rotate Spread (`createByDrag`): every real drag and every typed-dimensions click measured in
//! InDesign 2025 (next/oracle/measured/turned-draw-2025.json, rules in next/oracle/rules/liveinput-c2.ko.md) is rebuilt here from the box InDesign made
//! (as the view shows it), and what is stored must be what InDesign's exported IDML holds: the ItemTransform (the inverse of the spread's turn, no
//! translation), the path in the standard order in the upright box, the gradient handles, style and content type, and the place on the page.
use folio_edit::{commands::RegisteredCommand, Editor, xml::Index};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

/// The fixture with the spread of the first page turned by `matrix` instead (a turn by an angle that View > Rotate Spread cannot make: Spread.transform, an IDML written elsewhere).
fn fixture_turned(matrix: &[f64]) -> (Editor, Vec<String>) {
    let (ed, pages) = fixture();
    let mut parts = ed.parts().clone();
    for text in parts.values_mut() {
        let Ok(ix) = Index::scan(text) else { continue };
        let Some(i) = ix.by_self(text, &pages[0]) else { continue };
        let spread = ix.ancestors(i).find(|&a| ix.local(text, a) == "Spread").unwrap();
        let tag = &text[ix.nodes[spread].start..ix.nodes[spread].start + text[ix.nodes[spread].start..].find('>').unwrap()];
        let from = ix.nodes[spread].start + tag.find(" ItemTransform=\"").unwrap() + " ItemTransform=\"".len();
        let to = from + text[from..].find('"').unwrap();
        text.replace_range(from..to, &matrix.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        break;
    }
    (Editor::new(parts), pages)
}

fn fixture() -> (Editor, Vec<String>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/pages-turned.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() { continue; }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut entry, &mut s).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), s);
    }
    let mut pages = Vec::new();
    for t in parts.values().filter(|t| t.contains("<idPkg:Spread ")) {
        let ix = Index::scan(t).unwrap();
        pages.extend((0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Page").filter_map(|i| ix.attr(t, i, "Self").map(str::to_owned)));
    }
    (Editor::new(parts), pages)
}

fn numbers(s: &str) -> Vec<f64> { s.split_whitespace().map(|v| v.parse().unwrap()).collect() }
fn apply(m: &[f64], p: &[f64]) -> [f64; 2] { [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]] }
fn aabb(points: &[[f64; 2]]) -> [f64; 4] {
    points.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

/// What the item holds: tag, ItemTransform, path anchors, a few attributes.
struct Stored { tag: String, matrix: Vec<f64>, anchors: Vec<Vec<f64>>, attrs: BTreeMap<String, String> }
fn stored(ed: &Editor, id: &str) -> Stored {
    for t in ed.parts().values() {
        let Ok(ix) = Index::scan(t) else { continue };
        let Some(i) = ix.by_self(t, id) else { continue };
        let anchors = (0..ix.nodes.len()).filter(|&n| ix.local(t, n) == "PathPointType" && ix.ancestors(n).any(|a| a == i))
            .map(|n| numbers(ix.attr(t, n, "Anchor").unwrap())).collect();
        let attrs = ["GradientFillStart", "GradientFillLength", "GradientFillAngle", "GradientStrokeStart", "GradientStrokeLength", "GradientStrokeAngle", "AppliedObjectStyle", "ContentType"]
            .iter().filter_map(|k| ix.attr(t, i, k).map(|v| (k.to_string(), v.to_owned()))).collect();
        return Stored { tag: ix.local(t, i).to_owned(), matrix: numbers(ix.attr(t, i, "ItemTransform").unwrap_or("1 0 0 1 0 0")), anchors, attrs };
    }
    panic!("{id} not found");
}
/// The page's top-left corner on its spread (spread coordinates) and the spread's transform.
fn page_origin(ed: &Editor, page: &str) -> ([f64; 2], Vec<f64>) {
    for t in ed.parts().values() {
        let Ok(ix) = Index::scan(t) else { continue };
        let Some(i) = ix.by_self(t, page) else { continue };
        let b = numbers(ix.attr(t, i, "GeometricBounds").unwrap());
        let m = numbers(ix.attr(t, i, "ItemTransform").unwrap_or("1 0 0 1 0 0"));
        let spread = ix.ancestors(i).find(|&a| ix.local(t, a) == "Spread").unwrap();
        return (apply(&m, &[b[1], b[0]]), numbers(ix.attr(t, spread, "ItemTransform").unwrap_or("1 0 0 1 0 0")));
    }
    panic!("{page} not found");
}
fn box_on_page(matrix: &[f64], anchors: &[Vec<f64>], origin: [f64; 2]) -> [f64; 4] {
    let b = aabb(&anchors.iter().map(|a| apply(matrix, a)).collect::<Vec<_>>());
    [b[0] - origin[0], b[1] - origin[1], b[2] - origin[0], b[3] - origin[1]]
}
fn relative(anchors: &[Vec<f64>]) -> Vec<[f64; 2]> {
    let b = aabb(&anchors.iter().map(|a| [a[0], a[1]]).collect::<Vec<_>>());
    anchors.iter().map(|a| [a[0] - b[0], a[1] - b[1]]).collect()
}

#[test]
fn every_measured_drawing_on_a_turned_spread_is_stored_as_in_indesign() {
    let all: Value = serde_json::from_str(include_str!("../../../../oracle/measured/turned-draw-2025.json")).unwrap();
    let cases = all["cases"].as_array().unwrap();
    let mut problems = Vec::new();
    for case in cases {
        let label = format!("{} {}", case["turn"].as_str().unwrap(), case["label"].as_str().unwrap());
        let (mut ed, pages) = if case["turn"] == "t30" {
            fixture_turned(&case["spread"]["itemTransform"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>())
        } else {
            fixture()
        };
        let page = &pages[match case["turn"].as_str().unwrap() { "cw" | "t30" => 0, "ccw" => 1, "180" => 3, other => panic!("{other}") }];
        let kind = match case["tool"].as_str().unwrap() { "rect" => "rectangle", "oval" => "ellipse", "poly" => "polygon", "rectframe" => "rectangleFrame", "ovalframe" => "ellipseFrame", "polyframe" => "polygonFrame", other => panic!("{other}") };
        let s: Vec<f64> = case["shown"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let command = RegisteredCommand::CreateByDrag { page: page.clone(), kind: kind.into(), rect: [s[0], s[1], s[2], s[3]], layer: String::new(), sides: 6, inset: 0. };
        let made = match ed.apply("draw", &[command.into()]) { Ok(m) => m, Err(e) => { problems.push(format!("{label}: {e}")); continue; } };
        let have = stored(&ed, &made.created[0]);
        let want = &case["idml"];
        let want_matrix: Vec<f64> = want["itemTransform"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        if want["tag"] != have.tag.as_str() { problems.push(format!("{label}: a {} was made, InDesign made a {}", have.tag, want["tag"])); }
        if have.matrix.iter().zip(&want_matrix).any(|(a, b)| (a - b).abs() > 1e-6) { problems.push(format!("{label}: ItemTransform {:?} (InDesign {want_matrix:?})", have.matrix)); }
        // the path: its shape in the upright box, point for point (the position follows from the page, below)
        let want_anchors: Vec<Vec<f64>> = want["anchors"].as_array().unwrap().iter().map(|p| p.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()).collect();
        let (hr, wr) = (relative(&have.anchors), relative(&want_anchors));
        if hr.len() != wr.len() || hr.iter().zip(&wr).any(|(a, b)| (a[0] - b[0]).abs() > 0.02 || (a[1] - b[1]).abs() > 0.02) { problems.push(format!("{label}: path {hr:?} (InDesign {wr:?})")); }
        // where it lies on the page, in the spread's own space
        let (origin, _) = page_origin(&ed, page);
        let mine = box_on_page(&have.matrix, &have.anchors, origin);
        let wp = &case["page"]["topLeftOnSpread"];
        let theirs = box_on_page(&want_matrix, &want_anchors, [wp[0].as_f64().unwrap(), wp[1].as_f64().unwrap()]);
        if mine.iter().zip(&theirs).any(|(a, b)| (a - b).abs() > 0.02) { problems.push(format!("{label}: box on the page {mine:?} (InDesign {theirs:?})")); }
        for (key, attr) in [("gradientFillStart", "GradientFillStart"), ("gradientFillLength", "GradientFillLength"), ("gradientFillAngle", "GradientFillAngle"), ("style", "AppliedObjectStyle"), ("contentType", "ContentType")] {
            if have.attrs.get(attr).map(String::as_str) != want[key].as_str() { problems.push(format!("{label}: {attr} {:?} (InDesign {})", have.attrs.get(attr), want[key])); }
        }
        // one undo step
        ed.undo().unwrap();
    }
    assert!(problems.is_empty(), "{} of {} measured drawings differ:\n{}", problems.len(), cases.len(), problems.join("\n"));
    assert!(cases.len() >= 40, "{} cases", cases.len());
    assert!(cases.iter().any(|c| c["turn"] == "t30"), "no case on the spread turned 30 degrees");
}

/// On an unturned page the mouse and the script make the same item.
#[test]
fn on_an_unturned_page_it_is_create_in_view() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/pages.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() { continue; }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut entry, &mut s).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), s);
    }
    let page = parts.values().filter(|t| t.contains("<idPkg:Spread ")).find_map(|t| {
        let ix = Index::scan(t).unwrap();
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page").and_then(|i| ix.attr(t, i, "Self").map(str::to_owned))
    }).unwrap();
    let mut results = Vec::new();
    for by_drag in [false, true] {
        let mut ed = Editor::new(parts.clone());
        let (kind, rect) = ("rectangle".to_string(), [40., 60., 190., 150.]);
        let command: folio_edit::Command = if by_drag {
            RegisteredCommand::CreateByDrag { page: page.clone(), kind, rect, layer: String::new(), sides: 6, inset: 0. }.into()
        } else {
            RegisteredCommand::CreateInView { page: page.clone(), kind, rect, layer: String::new(), sides: 6, inset: 0. }.into()
        };
        let made = ed.apply("draw", &[command]).unwrap();
        let s = stored(&ed, &made.created[0]);
        results.push((s.matrix, s.anchors));
    }
    assert_eq!(results[0], results[1]);
}
