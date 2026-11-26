//! Export Layers and Create Acrobat Layers, against what InDesign 2025 writes for the same document
//! (oracle/indesign/buildc2-ocg-probe.jsx; measured PDFs read with oracle/buildc2-ocg-read.py, artifacts/build-c2-20261023/ocg):
//! layers Alpha (visible, printable; bottom), Beta (visible, not printing), Gamma (hidden, printing), Delta (hidden, not printing; top).

use folio_model::layout::DocumentLayout;
use folio_model::{Affine, ColorModel, ColorSpace, Document, ItemKind, Layer, Page, PageItem, Paint, PathPoint, Point, Rect, Spread, SubPath, Swatch};
use folio_pdf::{ExportLayers, ExportOptions, Preset, export};
use lopdf::Object;

fn rect(w: f64, h: f64) -> Vec<SubPath> {
    let p = |x: f64, y: f64| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(0.0, 0.0), p(0.0, h), p(w, h), p(w, 0.0)], open: false }]
}

fn fake_profile() -> std::path::PathBuf {
    let mut p = vec![0u8; 132];
    p[0..4].copy_from_slice(&132u32.to_be_bytes());
    p[8] = 2;
    p[12..16].copy_from_slice(b"prtr");
    p[16..20].copy_from_slice(b"CMYK");
    p[20..24].copy_from_slice(b"Lab ");
    p[36..40].copy_from_slice(b"acsp");
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("folio-pdf-layers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("fake-cmyk-{n}.icc"));
    std::fs::write(&path, p).unwrap();
    path
}

const FILLS: [(&str, [f64; 4]); 4] = [("C", [100.0, 0.0, 0.0, 0.0]), ("M", [0.0, 100.0, 0.0, 0.0]), ("Y", [0.0, 0.0, 100.0, 0.0]), ("K", [0.0, 0.0, 0.0, 60.0])];

pub fn document(layers: &[(&str, bool, bool)]) -> Document {
    let swatches = FILLS
        .iter()
        .map(|(n, v)| {
            let id = format!("Color/{n}");
            (id.clone(), Swatch { id, name: n.to_string(), model: ColorModel::Process, space: ColorSpace::Cmyk, values: v.to_vec(), base: None, tint: None, display_rgb: [0, 0, 0] })
        })
        .collect();
    let items = layers
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| PageItem {
            id: format!("r{i}"),
            kind: ItemKind::Rectangle { content: None },
            transform: Affine::translate(10.0, 10.0 + 40.0 * i as f64),
            paths: rect(180.0, 30.0),
            fill: Paint::Swatch { id: format!("Color/{}", FILLS[i % 4].0), tint: 100.0 },
            layer: Some(format!("L{name}")),
            ..PageItem::default()
        })
        .collect();
    let spread = Spread {
        id: "s".into(),
        pages: vec![Page { id: "p".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 200.0, bottom: 200.0 }, ..Page::default() }],
        items,
        ..Spread::default()
    };
    Document {
        spreads: vec![spread],
        swatches,
        layers: layers.iter().enumerate().map(|(i, (name, visible, printable))| Layer { id: format!("L{name}"), name: name.to_string(), visible: *visible, printable: *printable, order: i, ..Layer::default() }).collect(),
        ..Document::default()
    }
}

fn four() -> Document {
    document(&[("Alpha", true, true), ("Beta", true, false), ("Gamma", false, true), ("Delta", false, false)])
}

pub fn export_pdf(doc: &Document, preset: Preset, acrobat: bool, which: ExportLayers) -> Vec<u8> {
    let fonts = folio_text::FontStore::empty();
    let opts = ExportOptions { preset, acrobat_layers: acrobat, export_layers: which, output_profile: Some(fake_profile()), document_name: "four.folio".into(), ..Default::default() };
    export(doc, &DocumentLayout::default(), &fonts, None, &opts).expect("export").pdf
}

fn fills(content: &str) -> Vec<&str> {
    ["1 0 0 0 k", "0 1 0 0 k", "0 0 1 0 k", "0 0 0 0.6 k"].into_iter().filter(|f| content.contains(f)).collect()
}

fn page_content(d: &lopdf::Document) -> String {
    let (_, pid) = d.get_pages().into_iter().next().unwrap();
    String::from_utf8_lossy(&d.get_page_content(pid)).into_owned()
}

fn name_of(d: &lopdf::Document, o: &Object) -> String {
    let g = d.get_object(o.as_reference().unwrap()).unwrap().as_dict().unwrap();
    String::from_utf8_lossy(g.get(b"Name").unwrap().as_str().unwrap()).into_owned()
}

fn names(d: &lopdf::Document, a: &Object) -> Vec<String> {
    a.as_array().unwrap().iter().map(|o| name_of(d, o)).collect()
}

/// (OCGs, ON, OFF, Order label, Order list) of the default configuration.
fn groups(d: &lopdf::Document) -> Option<(Vec<String>, Vec<String>, Vec<String>, String, Vec<String>)> {
    let cat = d.catalog().unwrap();
    let oc = cat.get(b"OCProperties").ok()?.as_dict().unwrap();
    let def = oc.get(b"D").unwrap().as_dict().unwrap();
    let order = def.get(b"Order").unwrap().as_array().unwrap()[0].as_array().unwrap();
    let label = String::from_utf8_lossy(order[0].as_str().unwrap()).into_owned();
    let list = order[1..].iter().map(|o| name_of(d, o)).collect();
    assert!(def.get(b"AS").is_err(), "no usage application (PDF/X-4 forbids /AS)");
    assert_eq!(def.get(b"RBGroups").unwrap().as_array().unwrap().len(), 0);
    Some((names(d, oc.get(b"OCGs").unwrap()), names(d, def.get(b"ON").unwrap()), names(d, def.get(b"OFF").unwrap()), label, list))
}

#[test]
fn acrobat_layers_write_every_layer_in_its_group_with_export_layers_as_the_default_view() {
    let doc = four();
    for (which, on) in [
        (ExportLayers::VisiblePrintable, vec!["Alpha"]),
        (ExportLayers::Visible, vec!["Alpha", "Beta"]),
        (ExportLayers::All, vec!["Alpha", "Beta", "Gamma", "Delta"]),
    ] {
        let pdf = export_pdf(&doc, Preset::X4, true, which);
        assert!(pdf.starts_with(b"%PDF-1.6"));
        let d = lopdf::Document::load_mem(&pdf).unwrap();
        let (ocgs, got_on, off, label, order) = groups(&d).expect("OCProperties");
        assert_eq!(ocgs, ["Guides and Grids", "Alpha", "Beta", "Gamma", "Delta"]);
        assert_eq!(got_on, on);
        let mut want_off = vec!["Guides and Grids"];
        want_off.extend(["Alpha", "Beta", "Gamma", "Delta"].into_iter().filter(|n| !on.contains(n)));
        assert_eq!(off, want_off);
        assert_eq!(label, "four.folio");
        assert_eq!(order, ["Delta", "Gamma", "Beta", "Alpha", "Guides and Grids"]);
        // every layer's content, bottom first, each in /OC /MCn BDC … EMC
        let c = page_content(&d);
        assert_eq!(fills(&c).len(), 4, "{which:?}: {c}");
        assert_eq!(c.matches("/OC /MC").count(), 4);
        assert_eq!(c.matches("EMC").count(), 4);
        let (_, pid) = d.get_pages().into_iter().next().unwrap();
        let page = d.get_dictionary(pid).unwrap();
        let res = match page.get(b"Resources").unwrap() { Object::Reference(r) => d.get_dictionary(*r).unwrap(), o => o.as_dict().unwrap() };
        let props = res.get(b"Properties").unwrap().as_dict().unwrap();
        let mapped: Vec<(String, String)> = props.iter().map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), name_of(&d, v))).collect();
        assert_eq!(mapped, [("MC0".to_string(), "Alpha".to_string()), ("MC1".into(), "Beta".into()), ("MC2".into(), "Gamma".into()), ("MC3".into(), "Delta".into())]);
        let g = d.get_object(props.get(b"MC0").unwrap().as_reference().unwrap()).unwrap().as_dict().unwrap();
        assert_eq!(g.get(b"Intent").unwrap().as_array().unwrap().len(), 2);
        assert!(g.get(b"Usage").unwrap().as_dict().unwrap().get(b"CreatorInfo").is_ok());
    }
}

#[test]
fn without_acrobat_layers_export_layers_picks_the_content() {
    let doc = four();
    for (which, want) in [
        (ExportLayers::VisiblePrintable, vec!["1 0 0 0 k"]),
        (ExportLayers::Visible, vec!["1 0 0 0 k", "0 1 0 0 k"]),
        (ExportLayers::All, vec!["1 0 0 0 k", "0 1 0 0 k", "0 0 1 0 k", "0 0 0 0.6 k"]),
    ] {
        let d = lopdf::Document::load_mem(&export_pdf(&doc, Preset::X4, false, which)).unwrap();
        assert!(groups(&d).is_none());
        let c = page_content(&d);
        assert_eq!(fills(&c), want, "{which:?}");
        assert!(!c.contains("BDC"));
    }
}

#[test]
fn pdf_1_3_presets_make_no_layers_and_pdf_1_4_ones_become_1_5() {
    let doc = four();
    // PDF/X-1a with all layers (InDesign: no OCGs, PDF 1.3, every layer's content)
    let x1a = export_pdf(&doc, Preset::X1a, true, ExportLayers::All);
    assert!(x1a.starts_with(b"%PDF-1.3"));
    let d = lopdf::Document::load_mem(&x1a).unwrap();
    assert!(groups(&d).is_none());
    assert_eq!(fills(&page_content(&d)).len(), 4);
    // [High Quality Print] with layers: PDF 1.5 (InDesign asks for Acrobat 6 compatibility)
    let hq = export_pdf(&doc, Preset::High, true, ExportLayers::All);
    assert!(hq.starts_with(b"%PDF-1.5"));
    assert!(groups(&lopdf::Document::load_mem(&hq).unwrap()).is_some());
    assert!(export_pdf(&doc, Preset::High, false, ExportLayers::All).starts_with(b"%PDF-1.4"));
}

#[test]
fn a_book_joins_identically_named_layers_into_one_group_or_keeps_each_documents() {
    let one = export_pdf(&document(&[("Alpha", true, true), ("Text", true, true)]), Preset::X4, true, ExportLayers::All);
    let two = export_pdf(&document(&[("Alpha", true, true), ("Other", true, true)]), Preset::X4, true, ExportLayers::All);
    let names = ["chapter1.folio".to_string(), "chapter2.folio".to_string()];
    let docs = [one, two];
    // merged (InDesign's book-merge.pdf): Guides and Grids, Alpha, Text, Other; listed under the book, top first
    let merged = folio_pdf::book::join_layers(&docs, Some(&folio_pdf::book::BookLayers { book_name: "book.foliobook", document_names: &names, merge: true })).unwrap().0;
    let d = lopdf::Document::load_mem(&merged).unwrap();
    let (ocgs, on, off, label, order) = groups(&d).unwrap();
    assert_eq!(ocgs, ["Guides and Grids", "Alpha", "Text", "Other"]);
    assert_eq!((on, off), (vec!["Alpha".to_string(), "Text".into(), "Other".into()], vec!["Guides and Grids".to_string()]));
    assert_eq!(label, "book.foliobook");
    assert_eq!(order, ["Other", "Text", "Alpha", "Guides and Grids"]);
    // both pages mark Alpha with the one group
    let alpha: Vec<lopdf::ObjectId> = d.get_pages().into_values().map(|pid| {
        let page = d.get_dictionary(pid).unwrap();
        let res = match page.get(b"Resources").unwrap() { Object::Reference(r) => d.get_dictionary(*r).unwrap(), o => o.as_dict().unwrap() };
        res.get(b"Properties").unwrap().as_dict().unwrap().get(b"MC0").unwrap().as_reference().unwrap()
    }).collect();
    assert_eq!(alpha[0], alpha[1]);
    // not merged (book-nomerge.pdf): every document's groups, each listed under its document
    let kept = folio_pdf::book::join_layers(&docs, Some(&folio_pdf::book::BookLayers { book_name: "book.foliobook", document_names: &names, merge: false })).unwrap().0;
    let d = lopdf::Document::load_mem(&kept).unwrap();
    let (ocgs, ..) = groups(&d).unwrap();
    assert_eq!(ocgs, ["Guides and Grids", "Alpha", "Text", "Guides and Grids", "Alpha", "Other"]);
    let def = d.catalog().unwrap().get(b"OCProperties").unwrap().as_dict().unwrap().get(b"D").unwrap().as_dict().unwrap();
    let lists = def.get(b"Order").unwrap().as_array().unwrap();
    assert_eq!(lists.len(), 2);
    let label = |i: usize| String::from_utf8_lossy(lists[i].as_array().unwrap()[0].as_str().unwrap()).into_owned();
    assert_eq!((label(0), label(1)), ("chapter1.folio".to_string(), "chapter2.folio".to_string()));
}
