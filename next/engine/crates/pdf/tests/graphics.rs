//! Lowered graphics in the PDF: stroke parts as real dash patterns, arrowheads as fills,
//! image clipping paths as clips, inner effects as live soft-masked fills.

use folio_model::geom::{self, Arrowhead, CornerAdjust, LineEnds, StrokePattern, StrokeSpec};
use folio_model::layout::DocumentLayout;
use folio_model::*;
use folio_pdf::{ExportOptions, export};

#[path = "../../../test_support.rs"]
mod test_support;

fn poly(pts: &[(f64, f64)], open: bool) -> SubPath {
    SubPath { points: pts.iter().map(|&(x, y)| PathPoint { anchor: Point::new(x, y), left: Point::new(x, y), right: Point::new(x, y) }).collect(), open }
}

fn rect(l: f64, t: f64, r: f64, b: f64) -> SubPath {
    poly(&[(l, t), (l, b), (r, b), (r, t)], false)
}

fn black() -> Paint {
    Paint::Swatch { id: "Color/Black".into(), tint: 100.0 }
}

fn export_items(items: Vec<PageItem>) -> (String, folio_pdf::Report, lopdf::Document) {
    let mut doc = Document::default();
    doc.swatches.insert(
        "Color/Black".into(),
        Swatch {
            id: "Color/Black".into(),
            name: "Black".into(),
            model: ColorModel::Process,
            space: ColorSpace::Cmyk,
            values: vec![0.0, 0.0, 0.0, 100.0],
            base: None,
            tint: None,
            display_rgb: [0, 0, 0],
        },
    );
    doc.spreads.push(Spread {
        id: "s".into(),
        pages: vec![Page { id: "p".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 300.0, bottom: 300.0 }, ..Default::default() }],
        items,
        ..Default::default()
    });
    let fonts = folio_text::FontStore::empty();
    let out = export(&doc, &DocumentLayout::default(), &fonts, None, &ExportOptions { output_profile: Some(test_support::test_cmyk_profile()), ..Default::default() }).expect("export");
    let d = lopdf::Document::load_mem(&out.pdf).expect("parse");
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    let content = String::from_utf8_lossy(&d.get_page_content(pid)).into_owned();
    (content, out.report, d)
}

#[test]
fn stroke_parts_are_dash_patterns_and_fills() {
    let line = vec![poly(&[(20.0, 50.0), (170.0, 50.0)], true)];
    let spec = StrokeSpec {
        weight: 4.0,
        miter_limit: 4.0,
        pattern: StrokePattern::Dashed { pattern: vec![12.0, 6.0], adjust: CornerAdjust::DashesAndGaps, cap: None, builtin: true },
        ends: LineEnds { start: Arrowhead::Triangle, ..LineEnds::default() },
        ..Default::default()
    };
    let item = PageItem {
        id: "l".into(),
        kind: ItemKind::GraphicLine,
        paths: line.clone(),
        stroke: Stroke { paint: black(), weight: 4.0, art: geom::stroke_art(&line, &spec), ..Default::default() },
        ..Default::default()
    };
    let (content, _, _) = export_items(vec![item]);
    // the line behind the head (9.849 pt) is 140.151 long: 7 × (12+6) + 12 with +0.143 on
    // every dash and gap, starting at the line's new start
    assert!(content.contains("[12.143"), "dash pattern in {content}");
    assert!(content.contains("] 0 d\n29.8476"), "dash from the pulled-back start in {content}");
    assert!(content.contains("\nh\nf\n"), "arrowhead filled in {content}");
}

#[test]
fn clipping_path_clips_the_graphic() {
    let g = Graphic {
        id: "g".into(),
        element: "Image".into(),
        bounds: Rect { left: 0.0, top: 0.0, right: 100.0, bottom: 100.0 },
        clip: Some(GraphicClip { kind: "PhotoshopPath".into(), paths: vec![poly(&[(0.0, 100.0), (50.0, 0.0), (100.0, 100.0)], false)], invert: false }),
        ..Default::default()
    };
    let item = PageItem { id: "f".into(), kind: ItemKind::Rectangle { content: Some(g) }, paths: vec![rect(0.0, 0.0, 100.0, 100.0)], ..Default::default() };
    let (content, _, _) = export_items(vec![item]);
    // the frame clip, then the triangle in the graphic's own space
    assert!(content.matches("W
n").count() >= 2, "{content}");
    assert!(content.contains("0 100 m
50 0 l
100 100 l
h
W
n"), "triangle clip in {content}");
}

/// A picture the exporter cannot encode is "not printed" with a warning, but the page still draws it: its XObject must exist
/// (an invisible placeholder), or the PDF names a resource it does not have (found by pdfx-verify.py: `xobject.undefined`).
#[test]
fn an_image_that_cannot_be_written_leaves_a_valid_reference() {
    let g = Graphic {
        id: "g".into(),
        element: "Image".into(),
        bounds: Rect { left: 0.0, top: 0.0, right: 100.0, bottom: 100.0 },
        embedded_data: Some(std::sync::Arc::new(b"\x89PNG\r\n\x1a\nnot a png".to_vec())),
        ..Default::default()
    };
    let item = PageItem { id: "f".into(), kind: ItemKind::Rectangle { content: Some(g) }, paths: vec![rect(0.0, 0.0, 100.0, 100.0)], ..Default::default() };
    let (content, report, d) = export_items(vec![item]);
    assert!(report.warnings.iter().any(|w| w.message.contains("could not be written")), "{:?}", report.warnings.iter().map(|w| &w.message).collect::<Vec<_>>());
    let drawn: Vec<&str> = content.lines().filter_map(|l| l.strip_suffix(" Do").and_then(|n| n.strip_prefix('/'))).filter(|n| n.starts_with("Im")).collect();
    assert_eq!(drawn.len(), 1, "the page draws the picture: {content}");
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    let (resources, inherited) = d.get_page_resources(pid).unwrap();
    let resources = resources.or_else(|| inherited.first().and_then(|id| d.get_dictionary(*id).ok())).expect("page resources");
    let xobjects = d.dereference(resources.get(b"XObject").expect("XObject resources")).unwrap().1.as_dict().unwrap();
    assert!(xobjects.has(drawn[0].as_bytes()), "{} is drawn but not in the resources", drawn[0]);
}

#[test]
fn inner_effects_are_live_soft_masks() {
    let inner = ShadowEffect { blend: BlendMode::Multiply, opacity: 0.75, x_offset: 3.5, y_offset: 6.06, size: 7.0, color: black(), ..Default::default() };
    let bevel = BevelEmboss { size: 7.0, depth: 1.0, up: true, angle: 120.0, altitude: 30.0, highlight: black(), highlight_opacity: 0.75, shadow: black(), shadow_opacity: 0.75, shadow_blend: BlendMode::Multiply, ..Default::default() };
    let item = PageItem {
        id: "fx".into(),
        kind: ItemKind::Rectangle { content: None },
        paths: vec![rect(50.0, 50.0, 200.0, 170.0)],
        fill: black(),
        print: ItemPrint { fx: ObjectEffects { inner_shadow: Some(inner), bevel: Some(bevel), ..Default::default() }, effects: vec!["InnerShadowSetting (object)".into(), "BevelAndEmbossSetting (object)".into()], ..Default::default() },
        ..Default::default()
    };
    let (_, report, d) = export_items(vec![item]);
    let soft_masks = d.objects.values().filter(|o| matches!(o, lopdf::Object::Dictionary(dict) if dict.has(b"SMask") && dict.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"ExtGState".as_slice()))).count();
    assert!(soft_masks >= 3, "inner shadow, bevel highlight and shadow: {soft_masks}");
    assert!(!report.warnings.iter().any(|w| w.message.contains("not exported")), "{:?}", report.warnings.iter().map(|w| &w.message).collect::<Vec<_>>());
}
