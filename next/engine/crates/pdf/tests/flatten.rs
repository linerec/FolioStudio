//! PDF/X-1a and PDF/X-3 through the whole export: PDF 1.3 with no transparency anywhere,
//! transparent objects flattened into ink rasters drawn in paint order (what lies above stays
//! vector), CMYK conversion of RGB colour, pages without transparency written as PDF/X-4
//! writes them.

use folio_model::layout::DocumentLayout;
use folio_model::{Affine, BlendMode, ColorModel, ColorSpace, Document, ItemKind, Page, PageItem, Paint, PathPoint, Point, PrintSetup, Rect, Spread, SubPath, Swatch};
use folio_pdf::{ExportOptions, Preset, export};
use lopdf::Object;
use std::sync::atomic::{AtomicUsize, Ordering};

fn rect(w: f64, h: f64) -> Vec<SubPath> {
    let p = |x: f64, y: f64| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(0.0, 0.0), p(0.0, h), p(w, h), p(w, 0.0)], open: false }]
}

fn swatch(id: &str, model: ColorModel, space: ColorSpace, values: &[f64]) -> (String, Swatch) {
    (id.into(), Swatch { id: id.into(), name: id.trim_start_matches("Color/").into(), model, space, values: values.to_vec(), base: None, tint: None, display_rgb: [0, 0, 0] })
}

fn item(id: &str, fill: &str, x: f64, y: f64, w: f64, h: f64) -> PageItem {
    PageItem {
        id: id.into(),
        kind: ItemKind::Rectangle { content: None },
        transform: Affine::translate(x, y),
        paths: rect(w, h),
        fill: Paint::Swatch { id: fill.into(), tint: 100.0 },
        ..PageItem::default()
    }
}

/// Two pages (200 × 300 pt): page 1 a cyan bar under a 50 % orange square under a black bar;
/// page 2 only opaque CMYK fills.
fn document() -> Document {
    let swatches = [
        swatch("Color/Cyan", ColorModel::Process, ColorSpace::Cmyk, &[100.0, 0.0, 0.0, 0.0]),
        swatch("Color/Orange", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 60.0, 100.0, 0.0]),
        swatch("Color/Black", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0]),
        swatch("Color/Green RGB", ColorModel::Process, ColorSpace::Rgb, &[20.0, 160.0, 60.0]),
    ]
    .into_iter()
    .collect();
    let mut orange = item("orange", "Color/Orange", 40.0, 40.0, 100.0, 100.0);
    orange.opacity = 0.5;
    let page = |i: usize, x: f64| Page { id: format!("p{i}"), name: format!("{}", i + 1), index: i, bounds: Rect { left: 0.0, top: 0.0, right: 200.0, bottom: 300.0 }, transform: Affine::translate(x, 0.0), ..Page::default() };
    Document {
        spreads: vec![
            Spread {
                id: "s1".into(),
                pages: vec![page(0, 0.0)],
                items: vec![item("cyan", "Color/Cyan", 0.0, 70.0, 200.0, 40.0), orange, item("black", "Color/Black", 0.0, 200.0, 200.0, 20.0)],
                ..Spread::default()
            },
            Spread {
                id: "s2".into(),
                pages: vec![page(1, 0.0)],
                items: vec![item("cyan2", "Color/Cyan", 10.0, 10.0, 50.0, 50.0), item("green", "Color/Green RGB", 70.0, 10.0, 50.0, 50.0)],
                ..Spread::default()
            },
        ],
        swatches,
        print: PrintSetup { overprint_black: true, ..PrintSetup::default() },
        ..Document::default()
    }
}

/// A minimal CMYK output-class ICC header (enough for the writer's checks; not a usable
/// profile: colour converts without management).
fn fake_profile(space: &[u8; 4], class: &[u8; 4]) -> std::path::PathBuf {
    static NEXT_PROFILE: AtomicUsize = AtomicUsize::new(0);
    let mut p = vec![0u8; 132];
    p[0..4].copy_from_slice(&132u32.to_be_bytes());
    p[8] = 2;
    p[12..16].copy_from_slice(class);
    p[16..20].copy_from_slice(space);
    p[20..24].copy_from_slice(b"Lab ");
    p[36..40].copy_from_slice(b"acsp");
    let dir = std::env::temp_dir().join(format!("folio-pdf-flat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = NEXT_PROFILE.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("fake-{}-{id}.icc", String::from_utf8_lossy(space).trim()));
    std::fs::write(&path, p).unwrap();
    path
}

fn export_doc(doc: &Document, preset: Preset) -> (Vec<u8>, folio_pdf::Report) {
    static PROFILES: std::sync::OnceLock<(std::path::PathBuf, std::path::PathBuf)> = std::sync::OnceLock::new();
    let (cmyk, rgb) = PROFILES.get_or_init(|| (fake_profile(b"CMYK", b"prtr"), fake_profile(b"RGB ", b"mntr")));
    let fonts = folio_text::FontStore::empty();
    let opts = ExportOptions {
        preset,
        output_profile: Some(cmyk.clone()),
        rgb_profile: Some(rgb.clone()),
        threads: 2,
        ..Default::default()
    };
    let out = export(doc, &DocumentLayout::default(), &fonts, None, &opts).expect("export");
    (out.pdf, out.report)
}

fn page_content(d: &lopdf::Document, n: usize) -> String {
    let pid = d.get_pages()[&(n as u32 + 1)];
    String::from_utf8_lossy(&d.get_page_content(pid)).into_owned()
}

fn deref<'a>(d: &'a lopdf::Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => d.get_object(*r).unwrap(),
        o => o,
    }
}

#[test]
fn x1a_is_pdf13_without_transparency_and_identified() {
    let (pdf, report) = export_doc(&document(), Preset::X1a);
    assert!(pdf.starts_with(b"%PDF-1.3"), "{:?}", &pdf[..8]);
    let raw = String::from_utf8_lossy(&pdf);
    for bad in ["/SMask", "/CA ", "/ca ", "/BM", "/Transparency", "/ICCBased", "/DeviceRGB"] {
        assert!(!raw.contains(bad), "{bad} in a PDF/X-1a");
    }
    assert_eq!(report.preset, "x1a");
    assert!(report.standard.starts_with("PDF/X-1a:2001"));
    let d = lopdf::Document::load_mem(&pdf).unwrap();
    let info = deref(&d, d.trailer.get(b"Info").unwrap()).as_dict().unwrap();
    assert_eq!(info.get(b"GTS_PDFXVersion").unwrap().as_str().unwrap(), b"PDF/X-1:2001");
    assert_eq!(info.get(b"GTS_PDFXConformance").unwrap().as_str().unwrap(), b"PDF/X-1a:2001");
    let f = report.flatten.expect("flattening report");
    assert_eq!((f.pages, f.regions, f.objects), (1, 1, 1));
    assert!(f.images > 0 && f.line_art_images > 0, "{f:?}");
}

#[test]
fn transparent_square_is_rasterised_in_paint_order() {
    let (pdf, _) = export_doc(&document(), Preset::X1a);
    let d = lopdf::Document::load_mem(&pdf).unwrap();
    let c = page_content(&d, 0);
    // the cyan bar below is clipped out of the region (even-odd), the region's raster form
    // follows it, the black bar above is drawn after the raster as vector
    let clip = c.find("W* n").expect("clip-out");
    let raster = c.find(" Do").expect("raster form");
    let black = c.find("0 0 0 1 k").expect("black fill");
    assert!(clip < raster && raster < black, "{c}");
    // the raster holds the 50 % orange over cyan as ink: C 50 %, M 30 %, Y 50 %
    let pid = d.get_pages()[&1];
    let (res, _) = d.get_page_resources(pid).unwrap();
    let xo = deref(&d, res.unwrap().get(b"XObject").unwrap()).as_dict().unwrap();
    let mut seen = 0;
    for (_, form) in xo.iter() {
        let Object::Stream(f) = deref(&d, form) else { continue };
        let inner = deref(&d, deref(&d, f.dict.get(b"Resources").unwrap()).as_dict().unwrap().get(b"XObject").unwrap()).as_dict().unwrap();
        for (_, im) in inner.iter() {
            let Object::Stream(s) = deref(&d, im) else { continue };
            assert_eq!(s.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), b"DeviceCMYK");
            let data = s.decompressed_content().unwrap();
            for px in data.chunks_exact(4) {
                if (px[0] as i32 - 128).abs() <= 2 && (px[1] as i32 - 77).abs() <= 2 && (px[2] as i32 - 128).abs() <= 2 && px[3] == 0 {
                    seen += 1;
                }
            }
        }
    }
    assert!(seen > 1000, "blended ink pixels: {seen}");
}

#[test]
fn a_flattened_tile_overprints_all_inks_in_pdf13_groups() {
    let mut doc = document();
    doc.spreads.truncate(1);
    let (id, process) = swatch("Color/Process", ColorModel::Process, ColorSpace::Cmyk, &[28.0, 37.0, 46.0, 55.0]);
    doc.swatches.insert(id.clone(), process);
    let mut items = vec![item("process", &id, 12.0, 12.0, 160.0, 160.0)];
    for i in 1..=9 {
        let (id, spot) = swatch(&format!("Color/Ink-{i}"), ColorModel::Spot, ColorSpace::Cmyk, &[50.0, 0.0, 0.0, 0.0]);
        doc.swatches.insert(id.clone(), spot);
        let mut square = item(&format!("spot-{i}"), &id, 20.0 + f64::from(i), 20.0, 128.0, 128.0);
        square.opacity = 0.75;
        square.blend_mode = BlendMode::Multiply;
        items.push(square);
    }
    doc.spreads[0].items = items;
    let (pdf, _) = export_doc(&doc, Preset::X1a);
    let parsed = lopdf::Document::load_mem(&pdf).unwrap();
    let mut verified = false;
    for stream in parsed.objects.values().filter_map(|o| o.as_stream().ok()) {
        let Some(res) = stream.dict.get(b"Resources").ok().and_then(|o| o.as_dict().ok()) else { continue };
        let Some(states) = res.get(b"ExtGState").ok().and_then(|o| o.as_dict().ok()) else { continue };
        let Ok(state) = states.get(b"Inks") else { continue };
        let paint = String::from_utf8(stream.decompressed_content().unwrap()).unwrap();
        let state = deref(&parsed, state).as_dict().unwrap();
        assert_eq!(state.get(b"op").unwrap(), &Object::Boolean(true));
        assert_eq!(state.get(b"OPM").unwrap(), &Object::Integer(1));
        let images = res.get(b"XObject").unwrap().as_dict().unwrap();
        for line in paint.lines().filter(|l| l.contains(" /Inks gs ")) {
            let mut found = std::collections::BTreeSet::new();
            let groups = line.split_whitespace().filter(|name| name.starts_with("/R"));
            for name in groups {
                let image = deref(&parsed, images.get(&name.as_bytes()[1..]).unwrap()).as_stream().unwrap();
                let cs = deref(&parsed, image.dict.get(b"ColorSpace").unwrap()).as_array().unwrap();
                match cs[0].as_name().unwrap() {
                    b"DeviceN" => {
                        let names = cs[1].as_array().unwrap();
                        assert!(names.len() <= 8);
                        for name in names {
                            assert!(found.insert(name.as_name().unwrap()));
                        }
                    }
                    b"Separation" => assert!(found.insert(cs[1].as_name().unwrap())),
                    other => panic!("unexpected ink space: {other:?}"),
                }
            }
            verified |= found.len() == 13;
        }
    }
    assert!(verified, "one flattened tile must contain four process inks and nine spots");
}

#[test]
fn x1a_converts_rgb_and_keeps_opaque_pages_as_x4_writes_them() {
    let (x1a, _) = export_doc(&document(), Preset::X1a);
    let (x4, _) = export_doc(&document(), Preset::X4);
    let (a, b) = (lopdf::Document::load_mem(&x1a).unwrap(), lopdf::Document::load_mem(&x4).unwrap());
    let (ca, cb) = (page_content(&a, 1), page_content(&b, 1));
    // the RGB green became DeviceCMYK; everything else is the same content
    assert!(cb.contains(" scn") || cb.contains(" sc"), "{cb}");
    assert!(!ca.contains("/CS"), "{ca}");
    let strip = |s: &str| s.lines().filter(|l| !l.ends_with(" k") && !l.ends_with(" cs") && !l.ends_with(" sc") && !l.ends_with(" scn")).collect::<Vec<_>>().join("\n");
    assert_eq!(strip(&ca), strip(&cb));
}

#[test]
fn x3_keeps_rgb_and_flattens() {
    let (pdf, report) = export_doc(&document(), Preset::X3);
    assert!(pdf.starts_with(b"%PDF-1.3"));
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/ICCBased") || raw.contains("/DeviceRGB") || report.warnings.iter().any(|w| w.message.contains("RGB")), "RGB kept in PDF/X-3");
    assert!(!raw.contains("/SMask") && !raw.contains("/BM") && !raw.contains("/Transparency"));
    assert_eq!(report.flatten.map(|f| f.pages), Some(1));
}

#[test]
fn blend_modes_flatten_in_ink() {
    let mut doc = document();
    doc.spreads[0].items[1].opacity = 1.0;
    doc.spreads[0].items[1].blend_mode = BlendMode::Multiply;
    let (pdf, report) = export_doc(&doc, Preset::X1a);
    assert!(!String::from_utf8_lossy(&pdf).contains("/BM"));
    assert_eq!(report.flatten.map(|f| f.objects), Some(1));
}
