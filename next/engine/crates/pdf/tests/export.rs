//! Synthetic documents through the whole export: page boxes, ink-native colour spaces,
//! Ink Manager conversions, overprint and PDF/X-4 identification, read back with lopdf.

use folio_model::layout::DocumentLayout;
use folio_model::{
    Affine, ColorModel, ColorSpace, Document, Ink, ItemKind, ItemPrint, Page, PageItem, Paint, PathPoint, Point, PrintSetup, Rect,
    Spread, SubPath, Swatch,
};
use folio_pdf::{ExportOptions, Marks, export};
use lopdf::Object;

#[path = "../../../test_support.rs"]
mod test_support;
use test_support::test_cmyk_profile as fake_profile;

fn rect(w: f64, h: f64) -> Vec<SubPath> {
    let p = |x: f64, y: f64| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(0.0, 0.0), p(0.0, h), p(w, h), p(w, 0.0)], open: false }]
}

#[test]
fn exported_metadata_contains_no_source_folder_or_author() {
    for source in [r"C:\Users\private-editor\secret\issue.folio", "/home/private-editor/secret/issue.folio"] {
        let (pdf, _) = export_doc(&document(), ExportOptions {
            output_profile: Some(fake_profile()), document_name: source.into(), ..Default::default()
        });
        let info = deref(&pdf, pdf.trailer.get(b"Info").unwrap()).as_dict().unwrap();
        let bytes = info.get(b"Title").unwrap().as_str().unwrap();
        let title = if bytes.starts_with(&[0xfe, 0xff]) {
            String::from_utf16(&bytes[2..].chunks_exact(2).map(|p| u16::from_be_bytes([p[0], p[1]])).collect::<Vec<_>>()).unwrap()
        } else { String::from_utf8(bytes.to_vec()).unwrap() };
        assert_eq!(title, "issue.folio");
        assert!(info.get(b"Author").is_err());
        let catalog = pdf.catalog().unwrap();
        let xmp = deref(&pdf, catalog.get(b"Metadata").unwrap()).as_stream().unwrap();
        let text = String::from_utf8_lossy(&xmp.content);
        assert!(text.contains("issue.folio"));
        assert!(!text.contains("private-editor") && !text.contains("secret"));
        assert!(!text.contains(".partial"));
    }
}

#[test]
fn info_dates_match_xmp_dates() {
    let (pdf, _) = export_doc(&document(), ExportOptions {
        output_profile: Some(fake_profile()), ..Default::default()
    });
    let info = deref(&pdf, pdf.trailer.get(b"Info").unwrap()).as_dict().unwrap();
    let metadata = deref(&pdf, pdf.catalog().unwrap().get(b"Metadata").unwrap()).as_stream().unwrap();
    let xmp = String::from_utf8_lossy(&metadata.content);
    for (key, tag) in [(b"CreationDate".as_slice(), "CreateDate"), (b"ModDate".as_slice(), "ModifyDate")] {
        let raw = std::str::from_utf8(info.get(key).unwrap().as_str().unwrap()).unwrap();
        assert!(raw.ends_with("Z") || raw.ends_with("+00'00'"), "UTC Info date: {raw}");
        let iso = format!("{}-{}-{}T{}:{}:{}Z", &raw[2..6], &raw[6..8], &raw[8..10], &raw[10..12], &raw[12..14], &raw[14..16]);
        assert!(xmp.contains(&format!("<xmp:{tag}>{iso}</xmp:{tag}>")), "{tag} differs between Info and XMP");
    }
}

fn swatch(id: &str, name: &str, model: ColorModel, space: ColorSpace, values: &[f64]) -> (String, Swatch) {
    (
        id.into(),
        Swatch {
            id: id.into(),
            name: name.into(),
            model,
            space,
            values: values.to_vec(),
            base: None,
            tint: None,
            display_rgb: [0, 0, 0],
        },
    )
}

fn ink(name: &str, convert: bool) -> Ink {
    Ink { name: name.into(), process: false, convert_to_process: convert, alias: None, print: true, ink_type: "Normal".into(), neutral_density: None }
}

fn item(id: &str, fill: &str, tint: f64, x: f64, overprint: bool) -> PageItem {
    PageItem {
        id: id.into(),
        kind: ItemKind::Rectangle { content: None },
        transform: Affine::translate(x, 10.0),
        paths: rect(50.0, 50.0),
        fill: Paint::Swatch { id: fill.into(), tint },
        print: ItemPrint { overprint_fill: overprint, ..Default::default() },
        ..PageItem::default()
    }
}

/// A one-page document (200 × 300 pt) with process, spot (kept and converted), black-tint
/// and registration fills.
fn document() -> Document {
    let swatches = [
        swatch("Color/Black", "Black", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0]),
        swatch("Color/Spot A", "Spot A", ColorModel::Spot, ColorSpace::Cmyk, &[0.0, 60.0, 100.0, 0.0]),
        swatch("Color/Spot B", "Spot B", ColorModel::Spot, ColorSpace::Lab, &[50.0, 60.0, 30.0]),
        swatch("Color/Registration", "Registration", ColorModel::Registration, ColorSpace::Cmyk, &[100.0; 4]),
        swatch("Color/Cyan", "Cyan", ColorModel::Process, ColorSpace::Cmyk, &[100.0, 0.0, 0.0, 0.0]),
    ]
    .into_iter()
    .collect();
    let spread = Spread {
        id: "s".into(),
        pages: vec![Page { id: "p".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 200.0, bottom: 300.0 }, ..Page::default() }],
        items: vec![
            item("cyan", "Color/Cyan", 100.0, 0.0, false),
            item("black62", "Color/Black", 62.0, 60.0, false),
            item("spotA", "Color/Spot A", 40.0, 120.0, true),
            item("spotB", "Color/Spot B", 100.0, 0.0, false),
            item("reg", "Color/Registration", 100.0, 60.0, false),
        ],
        ..Spread::default()
    };
    Document {
        spreads: vec![spread],
        swatches,
        print: PrintSetup {
            bleed: [9.0, 9.0, 9.0, 9.0],
            overprint_black: true,
            inks: vec![ink("Spot A", false), ink("Spot B", true)],
            ..PrintSetup::default()
        },
        ..Document::default()
    }
}

fn export_doc(doc: &Document, opts: ExportOptions) -> (lopdf::Document, folio_pdf::Report) {
    let fonts = folio_text::FontStore::empty();
    let layout = DocumentLayout::default();
    let out = export(doc, &layout, &fonts, None, &opts).expect("export");
    (lopdf::Document::load_mem(&out.pdf).expect("parse"), out.report)
}

fn deref<'a>(d: &'a lopdf::Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => d.get_object(*r).unwrap(),
        o => o,
    }
}

fn page_content(d: &lopdf::Document) -> String {
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    String::from_utf8_lossy(&d.get_page_content(pid)).into_owned()
}

#[test]
fn boxes_without_and_with_bleed_and_marks() {
    let doc = document();
    let opts = ExportOptions { output_profile: Some(fake_profile()), ..Default::default() };
    let (d, _) = export_doc(&doc, opts.clone());
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    let page = d.get_object(pid).unwrap().as_dict().unwrap();
    let rect = |k: &[u8]| -> Vec<f32> {
        page.get(k).unwrap().as_array().unwrap().iter().map(|x| x.as_float().unwrap()).collect()
    };
    assert_eq!(rect(b"MediaBox"), vec![0.0, 0.0, 200.0, 300.0]);
    assert_eq!(rect(b"TrimBox"), vec![0.0, 0.0, 200.0, 300.0]);

    let (d, _) = export_doc(&doc, ExportOptions { bleed: true, marks: Marks { crop: true, bleed: true, registration: true, page_info: false }, ..opts });
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    let page = d.get_object(pid).unwrap().as_dict().unwrap();
    let rect = |k: &[u8]| -> Vec<f32> {
        page.get(k).unwrap().as_array().unwrap().iter().map(|x| x.as_float().unwrap()).collect()
    };
    // InDesign: 9 pt bleed + 3 pt gap + 18 pt bleed marks = 30 pt around the trim
    assert_eq!(rect(b"TrimBox"), vec![30.0, 30.0, 230.0, 330.0]);
    assert_eq!(rect(b"BleedBox"), vec![21.0, 21.0, 239.0, 339.0]);
    assert_eq!(rect(b"MediaBox"), vec![0.0, 0.0, 260.0, 360.0]);
}

#[test]
fn inks_stay_native() {
    let doc = document();
    let (d, report) = export_doc(&doc, ExportOptions { output_profile: Some(fake_profile()), ..Default::default() });
    let content = page_content(&d);
    // Process cyan and the 62 % black tint are DeviceCMYK values: K alone for the black.
    assert!(content.contains("1 0 0 0 k"), "{content}");
    assert!(content.contains("0 0 0 0.62 k"), "{content}");
    // Spot A keeps its plate (Separation, tint 0.4); Spot B is converted to process by the
    // Ink Manager (its Lab definition, no plate); Registration is Separation /All.
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    let (res, _) = d.get_page_resources(pid).unwrap();
    let res = res.unwrap();
    let cs = deref(&d, res.get(b"ColorSpace").unwrap()).as_dict().unwrap();
    let mut seps = Vec::new();
    let mut lab = false;
    for (_, v) in cs.iter() {
        let a = deref(&d, v).as_array().unwrap();
        match a[0].as_name().unwrap() {
            b"Separation" => seps.push(String::from_utf8_lossy(a[1].as_name().unwrap()).into_owned()),
            b"Lab" => lab = true,
            _ => {}
        }
    }
    seps.sort();
    assert_eq!(seps, vec!["All".to_owned(), "Spot A".to_owned()]);
    assert!(lab, "converted Lab spot written as Lab");
    assert!(content.contains("0.4 scn"), "{content}");
    let b = report.inks.iter().find(|i| i.name == "Spot B").unwrap();
    assert_eq!(b.output, "converted to process");
    assert_eq!(report.counts.overprint_fills, 1);
    assert_eq!(report.counts.separation_spaces, 2);
}

#[test]
fn overprint_and_pdfx_identification() {
    let doc = document();
    let (d, _) = export_doc(&doc, ExportOptions { output_profile: Some(fake_profile()), ..Default::default() });
    // The overprinting fill uses an ExtGState with op true and OPM 1.
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    let (res, _) = d.get_page_resources(pid).unwrap();
    let gs = deref(&d, res.unwrap().get(b"ExtGState").unwrap()).as_dict().unwrap();
    let any_op = gs.iter().any(|(_, v)| {
        let g = deref(&d, v).as_dict().unwrap();
        g.get(b"op").ok().and_then(|o| o.as_bool().ok()) == Some(true) && g.get(b"OPM").ok().and_then(|o| o.as_i64().ok()) == Some(1)
    });
    assert!(any_op);
    let cat = d.catalog().unwrap();
    let ois = deref(&d, cat.get(b"OutputIntents").unwrap()).as_array().unwrap();
    let oi = deref(&d, &ois[0]).as_dict().unwrap();
    assert_eq!(oi.get(b"S").unwrap().as_name().unwrap(), b"GTS_PDFX");
    let meta = deref(&d, cat.get(b"Metadata").unwrap()).as_stream().unwrap();
    let xmp = String::from_utf8_lossy(&meta.content);
    assert!(xmp.contains("<pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion>"));
    assert!(xmp.contains("<pdf:Trapped>False</pdf:Trapped>"));
    assert!(d.trailer.get(b"ID").is_ok());
    assert!(d.trailer.get(b"Encrypt").is_err());
}

#[test]
fn missing_output_profile_is_an_error() {
    let doc = document();
    let fonts = folio_text::FontStore::empty();
    let r = export(&doc, &DocumentLayout::default(), &fonts, None, &ExportOptions::default());
    assert!(r.is_err());
}

#[test]
fn transparency_becomes_a_group() {
    let mut doc = document();
    doc.spreads[0].items[0].opacity = 0.5;
    doc.spreads[0].items[0].blend_mode = folio_model::BlendMode::Multiply;
    let (d, report) = export_doc(&doc, ExportOptions { output_profile: Some(fake_profile()), ..Default::default() });
    assert_eq!(report.counts.transparency_groups, 1);
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    let page = d.get_object(pid).unwrap().as_dict().unwrap();
    let group = deref(&d, page.get(b"Group").unwrap()).as_dict().unwrap();
    assert_eq!(group.get(b"CS").unwrap().as_name().unwrap(), b"DeviceCMYK");
    let (res, _) = d.get_page_resources(pid).unwrap();
    let gs = deref(&d, res.unwrap().get(b"ExtGState").unwrap()).as_dict().unwrap();
    let blend = gs.iter().any(|(_, v)| {
        let g = deref(&d, v).as_dict().unwrap();
        g.get(b"BM").ok().and_then(|o| o.as_name().ok()) == Some(b"Multiply".as_slice())
            && g.get(b"ca").ok().and_then(|o| o.as_float().ok()) == Some(0.5)
    });
    assert!(blend);
}

#[test]
fn exported_text_references_a_parseable_embedded_truetype_subset() {
    use folio_model::{CharacterAttrs, Paragraph, ParagraphAttrs, Story, TextFrame, TextRun};
    use folio_text::{FontSource, FontStore, layout_document};

    let dirs: Vec<_> = FontStore::system_dirs().into_iter().filter(|(_, source)| *source == FontSource::System).collect();
    let fonts = FontStore::from_dirs(&dirs);
    let face = fonts.faces().iter().find(|face| {
        if face.cff { return false; }
        let resolved = fonts.resolve(&face.family, &face.style, &[]);
        resolved.exact && fonts.with_face(resolved.font, |f| {
            f.glyph_index('A').is_some() && f.glyph_index('B').is_some()
                && f.permissions() != Some(ttf_parser::Permissions::Restricted)
                && f.is_outline_embedding_allowed()
        }).unwrap_or(false)
    }).expect("a system TrueType font with Latin letters is required; do not silently skip embedding coverage");
    let mut doc = document();
    doc.spreads[0].items = vec![PageItem {
        id: "text".into(), paths: rect(150.0, 100.0),
        kind: ItemKind::TextFrame(TextFrame { story: "story".into(), columns: 1, ..Default::default() }),
        ..Default::default()
    }];
    doc.stories.insert("story".into(), Story {
        id: "story".into(), paragraphs: vec![Paragraph {
            style: String::new(), attrs: ParagraphAttrs { hyphenation: false, ..Default::default() },
            runs: vec![TextRun { text: "ABBA".into(), attrs: CharacterAttrs {
                font_family: face.family.clone(), font_style: face.style.clone(), point_size: 12.0,
                fill: Paint::Swatch { id: "Color/Black".into(), tint: 100.0 }, ..Default::default()
            }, ..Default::default() }],
        }], ..Default::default()
    });
    let layout = layout_document(&doc, &fonts);
    assert!(layout.overset.is_empty());
    assert_eq!(layout.frames["text"].lines.iter().map(|line| line.glyphs.len()).sum::<usize>(), 4);
    let output = export(&doc, &layout, &fonts, None, &ExportOptions {
        output_profile: Some(fake_profile()), ..Default::default()
    }).unwrap();
    assert_eq!(output.report.fonts_embedded.len(), 1);
    let pdf = lopdf::Document::load_mem(&output.pdf).unwrap();
    let (_, page_id) = pdf.get_pages().into_iter().next().unwrap();
    let page = pdf.get_object(page_id).unwrap().as_dict().unwrap();
    let resources = deref(&pdf, page.get(b"Resources").unwrap()).as_dict().unwrap();
    let font_dict = deref(&pdf, resources.get(b"Font").unwrap()).as_dict().unwrap();
    assert_eq!(font_dict.len(), 1);
    for (_, font) in font_dict {
        let type0 = deref(&pdf, font).as_dict().unwrap();
        let children = type0.get(b"DescendantFonts").unwrap().as_array().unwrap();
        let cid = deref(&pdf, &children[0]).as_dict().unwrap();
        let descriptor = deref(&pdf, cid.get(b"FontDescriptor").unwrap()).as_dict().unwrap();
        let stream = deref(&pdf, descriptor.get(b"FontFile2").expect("text resources must reach the embedded font file"))
            .as_stream().unwrap();
        let bytes = stream.decompressed_content().unwrap();
        let subset = ttf_parser::Face::parse(&bytes, 0).expect("embedded bytes are a valid TrueType font");
        assert!(subset.number_of_glyphs() >= 3, "notdef plus both used letters");
        assert_eq!(stream.dict.get(b"Length1").unwrap().as_i64().unwrap(), bytes.len() as i64);
    }
}
