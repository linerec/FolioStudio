//! One synthetic document per rule: the rule fires on the problem and stays quiet on the
//! fixed variant.

use folio_links::{How, ImageFormat, ImageInfo, LinkEntry, LinkReport, LinkStatus, Modified, PlacedFont, ProbeError, VectorColors};
use folio_model::layout::{DocumentLayout, FrameText, LaidLine, MissingGlyphs, PlacedGlyph};
use folio_model::*;
use folio_preflight::rules::*;
use folio_preflight::{FontFacts, Options, Report, Severity, preflight};
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------
// Builders

fn rect_path(l: f64, t: f64, r: f64, b: f64) -> Vec<SubPath> {
    let p = |x: f64, y: f64| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(l, t), p(l, b), p(r, b), p(r, t)], open: false }]
}

fn swatch(id: &str, name: &str, model: ColorModel, space: ColorSpace, values: &[f64]) -> Swatch {
    Swatch { id: id.into(), name: name.into(), model, space, values: values.to_vec(), base: None, tint: None, display_rgb: [0, 0, 0] }
}

fn paint(id: &str) -> Paint {
    Paint::Swatch { id: id.into(), tint: 100.0 }
}

fn frame(id: &str, l: f64, t: f64, r: f64, b: f64, fill: &str) -> PageItem {
    PageItem {
        id: id.into(),
        kind: ItemKind::Rectangle { content: None },
        paths: rect_path(l, t, r, b),
        fill: if fill.is_empty() { Paint::None } else { paint(fill) },
        ..PageItem::default()
    }
}

// Test fixtures specify the frame edges and image resolution independently.
#[allow(clippy::too_many_arguments)]
fn graphic_frame(id: &str, gid: &str, l: f64, t: f64, r: f64, b: f64, scale: f64, ppi: f64) -> PageItem {
    // image of (r-l)/scale × (b-t)/scale points at `ppi`, scaled by `scale` into the frame
    let w = (r - l) / scale;
    let h = (b - t) / scale;
    let g = Graphic {
        id: gid.into(),
        element: "Image".into(),
        transform: Affine::new(scale, 0.0, 0.0, scale, l, t),
        bounds: Rect { left: 0.0, top: 0.0, right: w, bottom: h },
        actual_ppi: Some((ppi, ppi)),
        ..Graphic::default()
    };
    PageItem { id: id.into(), kind: ItemKind::Rectangle { content: Some(g) }, paths: rect_path(l, t, r, b), ..PageItem::default() }
}

/// One page 612 × 792 at the spread origin, document bleed 9 pt, SWOP profile, process
/// swatches Black / Paper / Registration / Red (CMYK) / RGB red.
fn doc(items: Vec<PageItem>) -> Document {
    let mut d = Document {
        spreads: vec![Spread {
            id: "s1".into(),
            pages: vec![Page { id: "p1".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 612.0, bottom: 792.0 }, ..Page::default() }],
            items,
            ..Spread::default()
        }],
        ..Document::default()
    };
    d.print.bleed = [9.0; 4];
    d.print.cmyk_profile = Some("U.S. Web Coated (SWOP) v2".into());
    d.print.overprint_black = true;
    for s in [
        swatch("Color/Black", "Black", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0]),
        swatch("Color/Paper", "Paper", ColorModel::Paper, ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 0.0]),
        swatch("Color/Registration", "Registration", ColorModel::Registration, ColorSpace::Cmyk, &[100.0, 100.0, 100.0, 100.0]),
        swatch("Color/Red", "Red", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 100.0, 100.0, 0.0]),
        swatch("Color/K100", "K100 copy", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0]),
        swatch("Color/RGB", "RGB red", ColorModel::Process, ColorSpace::Rgb, &[255.0, 0.0, 0.0]),
        swatch("Color/Heavy", "Heavy", ColorModel::Process, ColorSpace::Cmyk, &[90.0, 90.0, 90.0, 90.0]),
    ] {
        d.swatches.insert(s.id.clone(), s);
    }
    d
}

fn run(d: &Document, layout: &DocumentLayout, links: &LinkReport) -> Report {
    let report = preflight(d, layout, links, &Options::default());
    for finding in &report.findings {
        let wire = serde_json::to_value(finding).unwrap();
        assert!(wire["messageId"].as_str().unwrap().starts_with("preflight."), "{}", finding.rule);
        assert!(wire["messageArgs"].is_object(), "{}", finding.rule);
        assert_eq!(wire["message"], finding.message);
    }
    report
}

#[test]
fn message_metadata_preserves_legacy_and_user_data() {
    let mut d = doc(vec![]);
    d.spreads[0].pages[0].name = "사용자 쪽 이름".into();
    let report = plain(&d);
    let finding = find(&report, BLANK_PAGE);
    assert_eq!(finding.message, "빈 쪽: 사용자 쪽 이름");
    assert_eq!(finding.message_id, "preflight.page.blank");
    assert_eq!(finding.message_args, serde_json::json!({"name":"사용자 쪽 이름"}));
    let mut wire = serde_json::to_value(finding).unwrap();
    wire.as_object_mut().unwrap().remove("messageId");
    wire.as_object_mut().unwrap().remove("messageArgs");
    let legacy: folio_preflight::Finding = serde_json::from_value(wire).unwrap();
    assert_eq!(legacy.message, finding.message);
    assert!(legacy.message_id.is_empty());
}

fn plain(d: &Document) -> Report {
    run(d, &DocumentLayout::default(), &LinkReport::default())
}

fn has(r: &Report, rule: &str) -> bool {
    r.findings.iter().any(|f| f.rule == rule)
}

fn find<'a>(r: &'a Report, rule: &str) -> &'a folio_preflight::Finding {
    r.findings.iter().find(|f| f.rule == rule).unwrap_or_else(|| panic!("{rule} not reported: {:#?}", r.findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()))
}

fn image_info(format: ImageFormat, w: u32, h: u32, space: folio_links::ColorSpace, bits: u8) -> ImageInfo {
    ImageInfo {
        format,
        width: w,
        height: h,
        color_space: space,
        bits,
        channels: 4,
        alpha: false,
        icc: None,
        ppi: None,
        vector: false,
        pages: None,
        page_boxes: Vec::new(),
        orientation: None,
        vector_colors: None,
        fonts: Vec::new(),
        file_size: 1,
        file_mtime_unix: None,
    }
}

fn entry(owner: &str, status: Option<LinkStatus>, info: Option<Result<ImageInfo, ProbeError>>) -> LinkEntry {
    LinkEntry {
        owner: owner.into(),
        element: "Image".into(),
        recorded: LinkInfo { uri: format!("file:/Users/a/{owner}.tif"), stored_state: "Normal".into(), ..LinkInfo::default() },
        embedded: false,
        status,
        modified: None,
        identity_matches: Vec::new(),
        info,
    }
}

fn links(entries: Vec<LinkEntry>) -> LinkReport {
    LinkReport { links: entries, ..LinkReport::default() }
}

fn found(path: &str) -> Option<LinkStatus> {
    Some(LinkStatus::Found { path: PathBuf::from(path), how: How::Original })
}

/// A text frame on the page with one story; `lines` = (start, end, baseline, x, size).
fn text_doc(attrs: CharacterAttrs, text: &str, frame_rect: [f64; 4], lines: &[(usize, usize, f64, f64, f64)]) -> (Document, DocumentLayout) {
    let tf = TextFrame { story: "st".into(), columns: 1, ..TextFrame::default() };
    let item = PageItem {
        id: "tf".into(),
        kind: ItemKind::TextFrame(tf),
        paths: rect_path(frame_rect[0], frame_rect[1], frame_rect[2], frame_rect[3]),
        ..PageItem::default()
    };
    let mut d = doc(vec![item]);
    d.stories.insert(
        "st".into(),
        Story { id: "st".into(), paragraphs: vec![Paragraph { runs: vec![TextRun { text: text.into(), attrs, ..TextRun::default() }], ..Paragraph::default() }], ..Story::default() },
    );
    let mut layout = DocumentLayout::default();
    let laid: Vec<LaidLine> = lines
        .iter()
        .map(|&(s, e, baseline, x, size)| LaidLine {
            baseline,
            left: x,
            right: x + 100.0,
            start: s,
            end: e,
            glyphs: vec![PlacedGlyph { font: 0, glyph: 1, x, y: baseline, size, hscale: 1.0, vscale: 1.0, skew: 0.0, path_transform: None, fill: Paint::None, stroke: Paint::None, stroke_weight: 0.0, overprint_fill: false, overprint_stroke: false, fill_gradient: None, stroke_gradient: None }],
            ..LaidLine::default()
        })
        .collect();
    layout.frames.insert("tf".into(), FrameText { frame: "tf".into(), lines: laid });
    (d, layout)
}

fn black_text(size: f64) -> CharacterAttrs {
    CharacterAttrs { point_size: size, fill: paint("Color/Black"), ..CharacterAttrs::default() }
}

// ---------------------------------------------------------------------------------------
// Fonts

#[test]
fn fonts_missing_and_substituted() {
    let (d, mut layout) = text_doc(black_text(10.0), "Hello", [100.0, 100.0, 300.0, 200.0], &[(0, 5, 120.0, 110.0, 10.0)]);
    layout.missing_fonts = vec![("Minion Pro".into(), "Regular".into())];
    let r = run(&d, &layout, &LinkReport::default());
    let f = find(&r, FONT_MISSING);
    assert_eq!((f.severity, f.page, f.item.as_deref()), (Severity::Error, Some(0), Some("tf")));
    let o = Options { fonts: Some(FontFacts { substituted: vec![("Minion Pro".into(), "Regular".into(), "Minion Pro Bold".into())], missing: vec![] }), ..Options::default() };
    let r = preflight(&d, &layout, &LinkReport::default(), &o);
    assert!(has(&r, FONT_SUBSTITUTED) && !has(&r, FONT_MISSING));
    layout.missing_fonts.clear();
    assert!(!has(&run(&d, &layout, &LinkReport::default()), FONT_MISSING));
}

#[test]
fn missing_glyphs() {
    let (d, mut layout) = text_doc(black_text(10.0), "Hello 한글", [100.0, 100.0, 300.0, 200.0], &[(0, 8, 120.0, 110.0, 10.0)]);
    let boxes = MissingGlyphs {
        story: "st".into(),
        family: "Arial".into(),
        style: "Regular".into(),
        installed: true,
        chars: "한글".into(),
        count: 2,
        ranges: vec![(6, 8)],
    };
    layout.missing_glyphs = vec![boxes];
    let r = run(&d, &layout, &LinkReport::default());
    let f = find(&r, MISSING_GLYPH);
    assert_eq!((f.severity, f.page, f.item.as_deref()), (Severity::Error, Some(0), Some("tf")));
    assert_eq!((f.data["characters"].as_u64(), f.data["installed"].as_bool()), (Some(2), Some(true)));
    // Only in overset text: information, on no page.
    layout.missing_glyphs[0].ranges = vec![(20, 22)];
    let r = run(&d, &layout, &LinkReport::default());
    let f = find(&r, MISSING_GLYPH);
    assert_eq!((f.severity, f.page), (Severity::Info, None));
    layout.missing_glyphs.clear();
    assert!(!has(&run(&d, &layout, &LinkReport::default()), MISSING_GLYPH));
}

#[test]
fn fonts_not_embedded_in_placed_pdf() {
    let d = doc(vec![graphic_frame("f", "g", 100.0, 100.0, 300.0, 300.0, 1.0, 300.0)]);
    let mut info = image_info(ImageFormat::Pdf, 200, 200, folio_links::ColorSpace::Cmyk, 0);
    info.vector = true;
    info.fonts = vec![PlacedFont { name: "Helvetica".into(), kind: "Type1".into(), embedded: false }, PlacedFont { name: "Myriad".into(), kind: "Type1".into(), embedded: true }];
    let r = run(&d, &DocumentLayout::default(), &links(vec![entry("g", found("x.pdf"), Some(Ok(info.clone())))]));
    let f = find(&r, FONT_NOT_EMBEDDED);
    assert_eq!(f.data["fonts"], serde_json::json!(["Helvetica"]));
    info.fonts[0].embedded = true;
    assert!(!has(&run(&d, &DocumentLayout::default(), &links(vec![entry("g", found("x.pdf"), Some(Ok(info)))])), FONT_NOT_EMBEDDED));
}

// ---------------------------------------------------------------------------------------
// Links

#[test]
fn links_missing_ambiguous_embedded_relocated() {
    let d = doc(vec![
        graphic_frame("f1", "g1", 0.0, 0.0, 100.0, 100.0, 1.0, 300.0),
        graphic_frame("f2", "g2", 100.0, 0.0, 200.0, 100.0, 1.0, 300.0),
        graphic_frame("f3", "g3", 200.0, 0.0, 300.0, 100.0, 1.0, 300.0),
        graphic_frame("f4", "g4", 300.0, 0.0, 400.0, 100.0, 1.0, 300.0),
        graphic_frame("f5", "g5", 400.0, 0.0, 500.0, 100.0, 1.0, 300.0),
        graphic_frame("f6", "g6", 500.0, 0.0, 600.0, 100.0, 1.0, 300.0),
    ]);
    let mut embedded = entry("g3", None, None);
    embedded.embedded = true;
    let rel = Some(LinkStatus::Found { path: "b/a.tif".into(), how: How::DocumentRelative });
    let name_only = Some(LinkStatus::Found { path: "z/a.tif".into(), how: How::Search { score: 0 } });
    let unreadable = entry("g6", found("broken.tif"), Some(Err(ProbeError::Malformed("TIFF header".into()))));
    let rep = links(vec![
        entry("g1", Some(LinkStatus::Missing), None),
        entry("g2", Some(LinkStatus::Ambiguous(vec!["a/x.tif".into(), "b/x.tif".into()])), None),
        embedded,
        entry("g4", rel, None),
        entry("g5", name_only, None),
        unreadable,
    ]);
    let r = run(&d, &DocumentLayout::default(), &rep);
    assert_eq!(find(&r, LINK_MISSING).item.as_deref(), Some("f1"));
    assert_eq!(find(&r, LINK_AMBIGUOUS).severity, Severity::Error);
    assert_eq!(find(&r, LINK_EMBEDDED).severity, Severity::Info);
    assert_eq!(find(&r, LINK_RELOCATED).item.as_deref(), Some("f4"));
    assert_eq!(find(&r, LINK_NAME_ONLY).severity, Severity::Warning);
    assert_eq!(find(&r, LINK_UNREADABLE).item.as_deref(), Some("f6"));
    // the same document with every file at its recorded place: nothing about links
    let ok = links((1..=6).map(|i| entry(&format!("g{i}"), found("a.tif"), None)).collect());
    let r = run(&d, &DocumentLayout::default(), &ok);
    assert!(r.findings.iter().all(|f| !f.rule.starts_with("LINK_")), "{:?}", r.findings);
}

#[test]
fn a_placed_eps_says_it_prints_from_its_preview() {
    // build-a: a found EPS is shown and printed through the preview it carries
    let d = doc(vec![graphic_frame("f1", "g1", 0.0, 0.0, 100.0, 100.0, 1.0, 300.0)]);
    let mut eps = entry("g1", found("ad.eps"), None);
    eps.element = "EPS".into();
    let r = run(&d, &DocumentLayout::default(), &links(vec![eps]));
    let f = find(&r, LINK_EPS_PREVIEW);
    assert_eq!((f.item.as_deref(), f.severity), (Some("f1"), Severity::Warning));
    assert!(!has(&run(&d, &DocumentLayout::default(), &links(vec![entry("g1", found("a.tif"), None)])), LINK_EPS_PREVIEW));
}

#[test]
fn link_modified() {
    let d = doc(vec![graphic_frame("f", "g", 0.0, 0.0, 100.0, 100.0, 1.0, 300.0)]);
    let mut e = entry("g", found("a.tif"), None);
    e.modified = Some(Modified { recorded_size: Some(10), actual_size: 12, recorded_mtime_unix: None, actual_mtime_unix: None, size_differs: true, time_differs: false });
    let r = run(&d, &DocumentLayout::default(), &links(vec![e]));
    assert_eq!(find(&r, LINK_MODIFIED).severity, Severity::Error);
}

// ---------------------------------------------------------------------------------------
// Images

#[test]
fn image_resolution_from_file_and_idml() {
    // 300 ppi image at 100 %: fine; at 200 %: 150 ppi (error); at 125 %: 240 ppi (warning)
    let d = doc(vec![
        graphic_frame("ok", "g1", 0.0, 0.0, 72.0, 72.0, 1.0, 300.0),
        graphic_frame("low", "g2", 100.0, 0.0, 244.0, 144.0, 2.0, 300.0),
        graphic_frame("mid", "g3", 300.0, 0.0, 390.0, 90.0, 1.25, 300.0),
    ]);
    let r = plain(&d); // links unknown: pixel size from IDML's actual ppi
    let low = r.findings.iter().find(|f| f.rule == IMAGE_RESOLUTION && f.item.as_deref() == Some("low")).expect("low");
    assert_eq!(low.severity, Severity::Error);
    assert!((low.data["effectivePpi"][0].as_f64().unwrap() - 150.0).abs() < 0.5);
    let mid = r.findings.iter().find(|f| f.rule == IMAGE_RESOLUTION && f.item.as_deref() == Some("mid")).expect("mid");
    assert_eq!(mid.severity, Severity::Warning);
    assert!(!r.findings.iter().any(|f| f.rule == IMAGE_RESOLUTION && f.item.as_deref() == Some("ok")));
    // from the file header: a 1-bit image of 300 px over 72 pt = 300 ppi < 1200
    let d = doc(vec![graphic_frame("bw", "g", 0.0, 0.0, 72.0, 72.0, 1.0, 300.0)]);
    let info = image_info(ImageFormat::Tiff, 300, 300, folio_links::ColorSpace::Gray, 1);
    let r = run(&d, &DocumentLayout::default(), &links(vec![entry("g", found("bw.tif"), Some(Ok(info)))]));
    let f = find(&r, IMAGE_RESOLUTION);
    assert_eq!((f.severity, f.data["bitmap"].as_bool()), (Severity::Error, Some(true)));
}

/// Effective resolution = InDesign 2025's, to the whole number: DOM `Image.effectivePpi` / IDML `EffectivePpi` of a 300 ppi picture at the
/// scale, flip-free rotation and non-uniform scale listed (artifacts/pdfxcheck-c2-20261023/work/indesign/images.result.json, round.result.json;
/// the numbers are in artifacts/pdfxcheck-c2-20261023/indesign-dom.json). The links panel reads the same facts (`Report::images`).
#[test]
fn effective_ppi_matches_indesign_dom() {
    // (horizontal %, vertical %, rotation degrees, InDesign effectivePpi x, y)
    let dom: [(f64, f64, f64, f64, f64); 17] = [
        (100.0, 100.0, 0.0, 300.0, 300.0), (50.0, 50.0, 0.0, 600.0, 600.0), (200.0, 200.0, 0.0, 150.0, 150.0), (100.0, 50.0, 0.0, 300.0, 600.0),
        (100.0, 100.0, 90.0, 300.0, 300.0), (75.0, 75.0, 30.0, 400.0, 400.0), (100.0, 100.0, 45.0, 300.0, 300.0), (60.0, 60.0, 0.0, 500.0, 500.0),
        (150.0, 150.0, 0.0, 200.0, 200.0), (70.0, 70.0, 0.0, 429.0, 429.0), (33.3, 33.3, 0.0, 901.0, 901.0), (130.0, 130.0, 0.0, 231.0, 231.0),
        (85.0, 100.0, 0.0, 353.0, 300.0), (100.0, 100.0, 30.0, 300.0, 300.0), (99.8, 99.8, 0.0, 301.0, 301.0), (100.0, 85.0, 90.0, 300.0, 353.0),
        (100.0, 100.0, 0.0, 300.0, 300.0),
    ];
    for (n, (hs, vs, degrees, ex, ey)) in dom.iter().enumerate() {
        let (sx, sy) = (hs / 100.0, vs / 100.0);
        let (c, s) = (f64::to_radians(*degrees).cos(), f64::to_radians(*degrees).sin());
        // the picture is 144 x 144 pt at its own 300 ppi (600 px); the frame carries the rotation, the picture the scale
        let g = Graphic {
            id: format!("g{n}"),
            element: "Image".into(),
            transform: Affine::new(sx * c, sx * s, -sy * s, sy * c, 10.0, 10.0),
            bounds: Rect { left: 0.0, top: 0.0, right: 144.0, bottom: 144.0 },
            actual_ppi: Some((300.0, 300.0)),
            ..Graphic::default()
        };
        let item = PageItem { id: format!("f{n}"), kind: ItemKind::Rectangle { content: Some(g) }, paths: rect_path(10.0, 10.0, 150.0, 150.0), ..PageItem::default() };
        let r = plain(&doc(vec![item]));
        let fact = &r.images[&format!("g{n}")];
        assert_eq!((fact.ppi[0].round(), fact.ppi[1].round()), (*ex, *ey), "scale {hs}/{vs} rotation {degrees}");
    }
}

/// 299.9994 ppi is "300" in InDesign (a PNG's pixels-per-metre rounds 300 dpi that way): it must not be reported against a 300 ppi minimum,
/// while 299.4 ("299") is.
#[test]
fn whole_number_resolution_decides_the_minimum() {
    let d = doc(vec![graphic_frame("edge", "g1", 0.0, 0.0, 72.0, 72.0, 1.0, 299.9994), graphic_frame("under", "g2", 100.0, 0.0, 172.0, 72.0, 1.0, 299.4)]);
    let r = plain(&d);
    assert!(!r.findings.iter().any(|f| f.rule == IMAGE_RESOLUTION && f.item.as_deref() == Some("edge")), "{:?}", r.findings);
    let under = r.findings.iter().find(|f| f.rule == IMAGE_RESOLUTION && f.item.as_deref() == Some("under")).expect("299 is below 300");
    assert!(under.message.contains("299 ppi"), "{}", under.message);
}

#[test]
fn color_space_images_vectors_text() {
    let mut rgb_frame = frame("vec", 0.0, 0.0, 50.0, 50.0, "Color/RGB");
    rgb_frame.stroke = Stroke { paint: paint("Color/RGB"), weight: 1.0, ..Stroke::default() };
    let d = doc(vec![rgb_frame, graphic_frame("img", "g", 100.0, 0.0, 172.0, 72.0, 1.0, 300.0), frame("cmyk", 200.0, 0.0, 250.0, 50.0, "Color/Red")]);
    let info = image_info(ImageFormat::Jpeg, 300, 300, folio_links::ColorSpace::Rgb, 8);
    let r = run(&d, &DocumentLayout::default(), &links(vec![entry("g", found("a.jpg"), Some(Ok(info)))]));
    let cs: Vec<_> = r.findings.iter().filter(|f| f.rule == COLOR_SPACE).collect();
    assert_eq!(cs.len(), 3, "{cs:?}"); // vector fill, vector stroke, image
    assert!(cs.iter().all(|f| f.item.as_deref() != Some("cmyk")));
    // placed PDF with RGB inside
    let mut pdf = image_info(ImageFormat::Pdf, 100, 100, folio_links::ColorSpace::Rgb, 0);
    pdf.vector = true;
    pdf.vector_colors = Some(VectorColors { spaces: vec![folio_links::ColorSpace::Rgb, folio_links::ColorSpace::Cmyk], ..VectorColors::default() });
    let d = doc(vec![graphic_frame("pdf", "g", 0.0, 0.0, 72.0, 72.0, 1.0, 300.0)]);
    let r = run(&d, &DocumentLayout::default(), &links(vec![entry("g", found("a.pdf"), Some(Ok(pdf)))]));
    assert_eq!(find(&r, COLOR_SPACE).data["placedFile"], serde_json::json!(true));
    // RGB output: no colour-space findings
    let o = Options { cmyk_output: false, ..Options::default() };
    let d = doc(vec![frame("vec", 0.0, 0.0, 50.0, 50.0, "Color/RGB")]);
    assert!(!has(&preflight(&d, &DocumentLayout::default(), &LinkReport::default(), &o), COLOR_SPACE));
}

#[test]
fn image_ink_coverage_from_cmyk_tiff() {
    // 8 × 8 uncompressed CMYK TIFF, every pixel at 90 / 90 / 90 / 90 %
    let dir = std::env::temp_dir().join(format!("folio-preflight-tac-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("heavy.tif");
    std::fs::write(&path, cmyk_tiff(8, 8, [230, 230, 230, 230])).unwrap();
    let info = folio_links::probe(&path).expect("probe");
    assert_eq!(info.color_space, folio_links::ColorSpace::Cmyk);
    let d = doc(vec![graphic_frame("f", "g", 0.0, 0.0, 72.0, 72.0, 1.0, 8.0)]);
    let rep = links(vec![entry("g", Some(LinkStatus::Found { path: path.clone(), how: How::Original }), Some(Ok(info)))]);
    let o = Options { image_ink: true, ..Options::default() };
    let r = preflight(&d, &DocumentLayout::default(), &rep, &o);
    let f = find(&r, IMAGE_INK_COVERAGE);
    assert!(f.data["max"].as_f64().unwrap() > 355.0, "{:?}", f.data);
    // light image: no finding
    let light = dir.join("light.tif");
    std::fs::write(&light, cmyk_tiff(8, 8, [50, 50, 50, 50])).unwrap();
    let info = folio_links::probe(&light).unwrap();
    let rep = links(vec![entry("g", Some(LinkStatus::Found { path: light, how: How::Original }), Some(Ok(info)))]);
    assert!(!has(&preflight(&d, &DocumentLayout::default(), &rep, &o), IMAGE_INK_COVERAGE));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Minimal little-endian uncompressed CMYK TIFF.
fn cmyk_tiff(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    let data_len = (w * h * 4) as usize;
    let mut b = Vec::new();
    b.extend_from_slice(b"II*\0");
    b.extend_from_slice(&8u32.to_le_bytes());
    let entries: Vec<(u16, u16, u32, u32)> = vec![
        (256, 4, 1, w),
        (257, 4, 1, h),
        (258, 3, 4, 0), // bits: offset patched below
        (259, 3, 1, 1),
        (262, 3, 1, 5),
        (273, 4, 1, 0), // strip offset patched
        (277, 3, 1, 4),
        (278, 4, 1, h),
        (279, 4, 1, data_len as u32),
        (284, 3, 1, 1),
    ];
    let ifd_len = 2 + entries.len() * 12 + 4;
    let bits_at = 8 + ifd_len as u32;
    let data_at = bits_at + 8;
    b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, typ, count, value) in entries {
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&typ.to_le_bytes());
        b.extend_from_slice(&count.to_le_bytes());
        let v = match tag {
            258 => bits_at,
            273 => data_at,
            _ => value,
        };
        if typ == 3 && count == 1 {
            b.extend_from_slice(&(v as u16).to_le_bytes());
            b.extend_from_slice(&[0, 0]);
        } else {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..4 {
        b.extend_from_slice(&8u16.to_le_bytes());
    }
    for _ in 0..w * h {
        b.extend_from_slice(&px);
    }
    b
}

// ---------------------------------------------------------------------------------------
// Spot colours and inks

fn spot_doc(items: Vec<PageItem>) -> Document {
    let mut d = doc(items);
    for s in [
        swatch("Color/P280", "PANTONE 280 C", ColorModel::Spot, ColorSpace::Cmyk, &[100.0, 72.0, 0.0, 18.0]),
        swatch("Color/Unused", "PANTONE 1797 C", ColorModel::Spot, ColorSpace::Cmyk, &[0.0, 100.0, 99.0, 4.0]),
        swatch("Color/SpotBlack", "Black", ColorModel::Spot, ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0]),
    ] {
        d.swatches.insert(s.id.clone(), s);
    }
    d.print.inks = vec![
        Ink { name: "Process Cyan".into(), process: true, print: true, ..Ink::default() },
        Ink { name: "PANTONE 280 C".into(), print: true, ..Ink::default() },
        Ink { name: "PANTONE 1797 C".into(), convert_to_process: true, print: true, ..Ink::default() },
        Ink { name: "Black".into(), alias: Some("Process Black".into()), print: true, ..Ink::default() },
    ];
    d
}

#[test]
fn spot_list_unused_process_like_alias_converted() {
    let d = spot_doc(vec![frame("a", 0.0, 0.0, 50.0, 50.0, "Color/P280")]);
    let r = plain(&d);
    let list = find(&r, SPOT_COLORS);
    assert_eq!(list.data["separating"], serde_json::json!(["PANTONE 280 C"]));
    let unused: Vec<_> = r.findings.iter().filter(|f| f.rule == SPOT_UNUSED).map(|f| f.data["name"].as_str().unwrap().to_owned()).collect();
    assert!(unused.contains(&"PANTONE 1797 C".to_owned()) && !unused.contains(&"PANTONE 280 C".to_owned()));
    assert_eq!(find(&r, SPOT_LOOKS_PROCESS).data["name"], "Black");
    assert_eq!(find(&r, INK_ALIAS).data["alias"], "Process Black");
    assert_eq!(find(&r, SPOT_TO_PROCESS).data["inks"], serde_json::json!(["PANTONE 1797 C"]));
    // a document without spot colours reports none of these
    let r = plain(&doc(vec![frame("a", 0.0, 0.0, 50.0, 50.0, "Color/Red")]));
    assert!(![SPOT_COLORS, SPOT_UNUSED, SPOT_LOOKS_PROCESS, INK_ALIAS, SPOT_TO_PROCESS].iter().any(|r2| has(&r, r2)));
}

#[test]
fn ink_coverage_vector() {
    let r = plain(&doc(vec![frame("heavy", 0.0, 0.0, 50.0, 50.0, "Color/Heavy"), frame("ok", 60.0, 0.0, 90.0, 50.0, "Color/Red")]));
    let f = find(&r, INK_COVERAGE);
    assert_eq!((f.item.as_deref(), f.data["coverage"].as_f64()), (Some("heavy"), Some(360.0)));
    assert_eq!(r.findings.iter().filter(|f| f.rule == INK_COVERAGE).count(), 1);
}

#[test]
fn registration_on_content() {
    let r = plain(&doc(vec![frame("reg", 0.0, 0.0, 50.0, 50.0, "Color/Registration")]));
    assert_eq!(find(&r, REGISTRATION_COLOR).severity, Severity::Error);
    // on the pasteboard (crop marks drawn by hand) it is not content
    let r = plain(&doc(vec![frame("reg", -200.0, 0.0, -150.0, 50.0, "Color/Registration")]));
    assert!(!has(&r, REGISTRATION_COLOR));
}

// ---------------------------------------------------------------------------------------
// Overprint

#[test]
fn white_overprint_and_colour_overprint() {
    let mut white = frame("w", 0.0, 0.0, 50.0, 50.0, "Color/Paper");
    white.print.overprint_fill = true;
    let mut red = frame("r", 60.0, 0.0, 110.0, 50.0, "Color/Red");
    red.print.overprint_fill = true;
    let mut black = frame("k", 120.0, 0.0, 170.0, 50.0, "Color/K100");
    black.print.overprint_fill = true;
    let r = plain(&doc(vec![white, red, black]));
    assert_eq!(find(&r, WHITE_OVERPRINT).item.as_deref(), Some("w"));
    assert_eq!(find(&r, OVERPRINT_COLOR).item.as_deref(), Some("r"));
    // K-only overprint is the good kind
    assert!(!r.findings.iter().any(|f| f.item.as_deref() == Some("k") && f.rule.contains("OVERPRINT")));
}

#[test]
fn black_text_knockout() {
    // a CMYK 0/0/0/100 swatch that is not [Black]: knocks out
    let attrs = CharacterAttrs { point_size: 9.0, fill: paint("Color/K100"), ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "Body text", [100.0, 100.0, 300.0, 200.0], &[(0, 9, 120.0, 110.0, 9.0)]);
    let r = run(&d, &layout, &LinkReport::default());
    assert_eq!(find(&r, BLACK_TEXT_KNOCKOUT).severity, Severity::Warning);
    // [Black] with the document's "overprint [Black]" preference: fine
    let (d, layout) = text_doc(black_text(9.0), "Body text", [100.0, 100.0, 300.0, 200.0], &[(0, 9, 120.0, 110.0, 9.0)]);
    assert!(!has(&run(&d, &layout, &LinkReport::default()), BLACK_TEXT_KNOCKOUT));
    // explicit overprint fill: fine
    let attrs = CharacterAttrs { point_size: 9.0, fill: paint("Color/K100"), overprint_fill: true, ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "Body text", [100.0, 100.0, 300.0, 200.0], &[(0, 9, 120.0, 110.0, 9.0)]);
    assert!(!has(&run(&d, &layout, &LinkReport::default()), BLACK_TEXT_KNOCKOUT));
}

// ---------------------------------------------------------------------------------------
// Text

#[test]
fn small_text_with_several_inks() {
    let attrs = CharacterAttrs { point_size: 8.0, fill: paint("Color/Red"), ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "Small red", [100.0, 100.0, 300.0, 200.0], &[(0, 9, 120.0, 110.0, 8.0)]);
    let r = run(&d, &layout, &LinkReport::default());
    assert_eq!(find(&r, SMALL_TEXT_INKS).data["inks"], 2);
    // large enough
    let attrs = CharacterAttrs { point_size: 14.0, fill: paint("Color/Red"), ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "Big red", [100.0, 100.0, 300.0, 200.0], &[(0, 7, 120.0, 110.0, 14.0)]);
    assert!(!has(&run(&d, &layout, &LinkReport::default()), SMALL_TEXT_INKS));
    // a 90 % black tint knocks out (the [Black] overprint preference is for 100 % only)
    let attrs = CharacterAttrs { point_size: 9.0, fill: Paint::Swatch { id: "Color/Black".into(), tint: 90.0 }, ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "Grey body", [100.0, 100.0, 300.0, 200.0], &[(0, 9, 120.0, 110.0, 9.0)]);
    assert_eq!(find(&run(&d, &layout, &LinkReport::default()), SMALL_TEXT_INKS).data["reason"], "knockout");
    // 100 % [Black] overprints by preference: fine
    let (d, layout) = text_doc(black_text(9.0), "Body", [100.0, 100.0, 300.0, 200.0], &[(0, 4, 120.0, 110.0, 9.0)]);
    assert!(!has(&run(&d, &layout, &LinkReport::default()), SMALL_TEXT_INKS));
    // RGB text separates into several inks too
    let attrs = CharacterAttrs { point_size: 8.0, fill: paint("Color/RGB"), ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "rgb", [100.0, 100.0, 300.0, 200.0], &[(0, 3, 120.0, 110.0, 8.0)]);
    assert!(has(&run(&d, &layout, &LinkReport::default()), SMALL_TEXT_INKS));
    // overset text does not print: not checked
    let attrs = CharacterAttrs { point_size: 8.0, fill: paint("Color/Red"), ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "Small red", [100.0, 100.0, 300.0, 200.0], &[]);
    assert!(!has(&run(&d, &layout, &LinkReport::default()), SMALL_TEXT_INKS));
}

#[test]
fn overset_text() {
    let (d, mut layout) = text_doc(black_text(10.0), "Hello world", [100.0, 100.0, 300.0, 200.0], &[(0, 5, 120.0, 110.0, 10.0)]);
    layout.overset = vec!["st".into()];
    let r = run(&d, &layout, &LinkReport::default());
    let f = find(&r, OVERSET_TEXT);
    assert_eq!((f.severity, f.data["overset"].as_u64()), (Severity::Error, Some(6)));
    layout.overset.clear();
    assert!(!has(&run(&d, &layout, &LinkReport::default()), OVERSET_TEXT));
}

#[test]
fn hairlines_items_and_text() {
    let mut thin = frame("thin", 0.0, 0.0, 50.0, 50.0, "");
    thin.stroke = Stroke { paint: paint("Color/Black"), weight: 0.1, ..Stroke::default() };
    // 1 pt stroke scaled to 20 %: 0.2 pt on paper
    let mut scaled = frame("scaled", 0.0, 0.0, 50.0, 50.0, "");
    scaled.stroke = Stroke { paint: paint("Color/Black"), weight: 1.0, ..Stroke::default() };
    scaled.transform = Affine::new(0.2, 0.0, 0.0, 0.2, 100.0, 100.0);
    let mut ok = frame("ok", 0.0, 100.0, 50.0, 150.0, "");
    ok.stroke = Stroke { paint: paint("Color/Black"), weight: 0.5, ..Stroke::default() };
    let r = plain(&doc(vec![thin, scaled, ok]));
    let hl: Vec<_> = r.findings.iter().filter(|f| f.rule == HAIRLINE).filter_map(|f| f.item.clone()).collect();
    assert_eq!(hl, ["scaled", "thin"].map(String::from).to_vec().into_iter().filter(|i| hl.contains(i)).collect::<Vec<_>>());
    assert!(hl.contains(&"thin".into()) && hl.contains(&"scaled".into()) && !hl.contains(&"ok".into()));
    let attrs = CharacterAttrs { point_size: 20.0, fill: paint("Color/Black"), stroke: paint("Color/Black"), stroke_weight: 0.1, ..CharacterAttrs::default() };
    let (d, layout) = text_doc(attrs, "Outlined", [100.0, 100.0, 300.0, 200.0], &[(0, 8, 130.0, 110.0, 20.0)]);
    assert!(has(&run(&d, &layout, &LinkReport::default()), HAIRLINE));
}

// ---------------------------------------------------------------------------------------
// Bleed, safety margin

#[test]
fn bleed_missing_and_reaching() {
    // touches the left trim, stops there: error; another reaches the 9 pt bleed: fine
    let short = frame("short", 0.0, 100.0, 200.0, 300.0, "Color/Red");
    let bleeds = frame("bleeds", -9.0, 400.0, 200.0, 500.0, "Color/Red");
    let partial = frame("partial", -4.0, 600.0, 200.0, 700.0, "Color/Red");
    let r = plain(&doc(vec![short, bleeds, partial]));
    let b: Vec<_> = r.findings.iter().filter(|f| f.rule == BLEED_MISSING).filter_map(|f| f.item.clone()).collect();
    assert!(b.contains(&"short".into()) && b.contains(&"partial".into()) && !b.contains(&"bleeds".into()), "{b:?}");
    // an unpainted frame on the trim is not artwork
    assert!(!has(&plain(&doc(vec![frame("empty", 0.0, 100.0, 200.0, 300.0, "")])), BLEED_MISSING));
    // a trim-size image in a frame that reaches the bleed: the image edge decides
    let mut ad = graphic_frame("ad", "g", -9.0, 100.0, 200.0, 300.0, 1.0, 300.0);
    if let ItemKind::Rectangle { content: Some(g) } = &mut ad.kind {
        g.transform = Affine::translate(0.0, 100.0);
        g.bounds = Rect { left: 0.0, top: 0.0, right: 200.0, bottom: 200.0 };
    }
    assert!(has(&plain(&doc(vec![ad])), BLEED_MISSING));
}

#[test]
fn safety_margin_text_and_objects() {
    // text 3 pt from the right trim (< 3 mm): warning; well inside: nothing
    let (d, layout) = text_doc(black_text(10.0), "Edge", [500.0, 100.0, 612.0, 200.0], &[(0, 4, 120.0, 603.0, 10.0)]);
    let r = run(&d, &layout, &LinkReport::default());
    let f = find(&r, SAFETY_MARGIN);
    assert_eq!((f.severity, f.data["text"].as_bool()), (Severity::Warning, Some(true)));
    let (d, layout) = text_doc(black_text(10.0), "Inside", [100.0, 100.0, 300.0, 200.0], &[(0, 6, 120.0, 110.0, 10.0)]);
    assert!(!has(&run(&d, &layout, &LinkReport::default()), SAFETY_MARGIN));
    // a painted box 2 pt inside the top trim: info
    let r = plain(&doc(vec![frame("box", 100.0, 2.0, 200.0, 50.0, "Color/Red")]));
    let f = find(&r, SAFETY_MARGIN);
    assert_eq!(f.severity, Severity::Info);
}

// ---------------------------------------------------------------------------------------
// Visibility, empty frames, blank pages

#[test]
fn hidden_and_nonprinting_content() {
    let mut hidden = frame("hidden", 0.0, 0.0, 50.0, 50.0, "Color/Red");
    hidden.visible = false;
    let mut np = frame("np", 60.0, 0.0, 110.0, 50.0, "Color/Red");
    np.print.nonprinting = true;
    let mut on_layer = frame("layered", 120.0, 0.0, 170.0, 50.0, "Color/Red");
    on_layer.layer = Some("L2".into());
    let mut d = doc(vec![hidden, np, on_layer]);
    d.layers = vec![
        Layer { id: "L1".into(), name: "Layer 1".into(), visible: true, printable: true, order: 0, ..Layer::default() },
        Layer { id: "L2".into(), name: "Notes".into(), visible: true, printable: false, order: 1, ..Layer::default() },
    ];
    let r = plain(&d);
    let ids: Vec<_> = r.findings.iter().filter(|f| f.rule == HIDDEN_CONTENT).filter_map(|f| f.item.clone()).collect();
    assert!(ids.contains(&"hidden".into()) && ids.contains(&"np".into()) && ids.contains(&"L2".into()), "{ids:?}");
    // hidden items are not checked otherwise
    assert!(!has(&r, BLEED_MISSING) && !has(&r, INK_COVERAGE));
}

#[test]
fn empty_frames() {
    let mut stroked = frame("stroked", 200.0, 100.0, 250.0, 150.0, "");
    stroked.stroke = Stroke { paint: paint("Color/Black"), weight: 1.0, ..Stroke::default() };
    let r = plain(&doc(vec![frame("empty", 100.0, 100.0, 150.0, 150.0, ""), stroked, frame("filled", 300.0, 100.0, 350.0, 150.0, "Color/Red")]));
    let ids: Vec<_> = r.findings.iter().filter(|f| f.rule == EMPTY_FRAME).filter_map(|f| f.item.clone()).collect();
    assert_eq!(ids, ["empty".to_owned()]);
}

#[test]
fn blank_pages() {
    let mut d = doc(vec![frame("a", 100.0, 100.0, 150.0, 150.0, "Color/Red")]);
    d.spreads.push(Spread {
        id: "s2".into(),
        pages: vec![Page { id: "p2".into(), name: "2".into(), bounds: Rect { left: 0.0, top: 0.0, right: 612.0, bottom: 792.0 }, index: 1, ..Page::default() }],
        ..Spread::default()
    });
    let r = plain(&d);
    let b: Vec<_> = r.findings.iter().filter(|f| f.rule == BLANK_PAGE).map(|f| f.page).collect();
    assert_eq!(b, [Some(1)]);
}

// ---------------------------------------------------------------------------------------
// Transparency, effects

#[test]
fn transparency_over_spot_and_used() {
    let spot = frame("spot", 0.0, 0.0, 200.0, 200.0, "Color/P280");
    let mut glass = frame("glass", 100.0, 100.0, 300.0, 300.0, "Color/Red");
    glass.opacity = 0.5;
    let far = {
        let mut f = frame("far", 400.0, 400.0, 500.0, 500.0, "Color/Red");
        f.blend_mode = BlendMode::Multiply;
        f
    };
    let r = plain(&spot_doc(vec![spot, glass, far]));
    let f = find(&r, TRANSPARENCY_SPOT);
    assert_eq!((f.item.as_deref(), f.severity), (Some("glass"), Severity::Warning));
    assert_eq!(r.findings.iter().filter(|f| f.rule == TRANSPARENCY_SPOT).count(), 1);
    let used: Vec<_> = r.findings.iter().filter(|f| f.rule == TRANSPARENCY_USED).filter_map(|f| f.item.clone()).collect();
    assert_eq!(used, ["far".to_owned(), "glass".to_owned()]);
    // the spot converted to process by the Ink Manager: only informational
    let mut d = spot_doc(vec![frame("spot", 0.0, 0.0, 200.0, 200.0, "Color/P280"), {
        let mut g = frame("glass", 100.0, 100.0, 300.0, 300.0, "Color/Red");
        g.opacity = 0.5;
        g
    }]);
    d.print.inks[1].convert_to_process = true;
    assert_eq!(find(&plain(&d), TRANSPARENCY_SPOT).severity, Severity::Info);
}

#[test]
fn unsupported_effects_and_items() {
    let mut shadow = frame("shadow", 0.0, 0.0, 50.0, 50.0, "Color/Red");
    shadow.print.effects = vec!["DropShadowSetting (object)".into()];
    let button = PageItem { id: "btn".into(), kind: ItemKind::Other { element: "Button".into() }, paths: rect_path(100.0, 0.0, 150.0, 50.0), ..PageItem::default() };
    let r = plain(&doc(vec![shadow, button, frame("plain", 200.0, 0.0, 250.0, 50.0, "Color/Red")]));
    let ids: Vec<_> = r.findings.iter().filter(|f| f.rule == UNSUPPORTED_EFFECT).filter_map(|f| f.item.clone()).collect();
    assert_eq!(ids, ["btn".to_owned(), "shadow".to_owned()]);
}

// ---------------------------------------------------------------------------------------
// Document

#[test]
fn cmyk_profile_and_page_sizes() {
    let mut d = doc(vec![frame("a", 100.0, 100.0, 150.0, 150.0, "Color/Red")]);
    assert!(!has(&plain(&d), NO_CMYK_PROFILE) && !has(&plain(&d), PAGE_SIZES));
    d.print.cmyk_profile = None;
    assert_eq!(find(&plain(&d), NO_CMYK_PROFILE).severity, Severity::Warning);
    d.spreads.push(Spread {
        id: "s2".into(),
        pages: vec![Page { id: "p2".into(), name: "2".into(), bounds: Rect { left: 0.0, top: 0.0, right: 595.0, bottom: 842.0 }, index: 1, ..Page::default() }],
        items: vec![frame("b", 100.0, 100.0, 150.0, 150.0, "Color/Red")],
        ..Spread::default()
    });
    let r = plain(&d);
    let f = find(&r, PAGE_SIZES);
    assert_eq!(f.data["sizes"].as_array().map(Vec::len), Some(2));
}

#[test]
fn every_rule_id_is_tested_here() {
    // keeps this file in step with `rules::ALL`
    let src = include_str!("rules.rs");
    for id in ALL {
        let constant = id.to_string();
        assert!(src.matches(constant.as_str()).count() >= 1, "rule {id} has no test");
    }
}
