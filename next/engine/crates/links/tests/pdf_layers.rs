use folio_links::pdfplace::{self, layers};
use lopdf::Document;
use std::path::PathBuf;

fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links/own-ocg.pdf")
}

#[test]
fn layer_overrides_change_paint_and_cache_identity() {
    let path = source();
    let info = folio_links::probe::probe(&path).unwrap();
    let mut source = pdfplace::inspect(&path,&info,Default::default()).unwrap();
    let layers = source.layers.as_mut().unwrap();
    assert_eq!(layers.layers.iter().map(|l| (l.id,l.name.as_str(),l.visible)).collect::<Vec<_>>(),vec![(5,"Red",true),(6,"Blue",false)]);
    let cache = std::env::temp_dir().join(format!("folio-ocg-test-{}",std::process::id()));
    let red = pdfplace::previews::cached(&path,&source,&cache).unwrap();
    source.layers.as_mut().unwrap().layers[1].visible = true;
    let both = pdfplace::previews::cached(&path,&source,&cache).unwrap();
    assert_ne!(red,both);
    assert_ne!(std::fs::read(&red).unwrap(),std::fs::read(&both).unwrap());
    source.layers.as_mut().unwrap().layers[0].visible = false;
    let blue = pdfplace::previews::cached(&path,&source,&cache).unwrap();
    assert_ne!(std::fs::read(&blue).unwrap(),std::fs::read(&both).unwrap());
    source.layers.as_mut().unwrap().layers[1].visible = false;
    let blank = pdfplace::previews::cached(&path,&source,&cache).unwrap();
    assert_ne!(std::fs::read(&blank).unwrap(),std::fs::read(&blue).unwrap());
    let _ = std::fs::remove_dir_all(cache);
}

#[test]
fn stale_layer_identity_fails_instead_of_showing_wrong_source() {
    let mut doc = Document::load(source()).unwrap();
    let mut options = layers::read(&doc).unwrap().unwrap();
    options.layers[0].name = "Renamed".into();
    assert!(layers::apply(&mut doc,Some(&options)).is_err());
}

#[test]
fn ordinary_pdf_is_unchanged_and_ocg_markers_are_removed() {
    let mut doc = Document::load(source()).unwrap();
    layers::apply(&mut doc,None).unwrap();
    assert!(!doc.catalog().unwrap().has(b"OCProperties"));
    let page = *doc.get_pages().values().next().unwrap();
    let data = doc.get_page_content(page);
    assert!(!String::from_utf8_lossy(&data).contains("BDC"));
    let before = doc.objects.clone();
    layers::apply(&mut doc,None).unwrap();
    assert_eq!(format!("{before:?}"),format!("{:?}",doc.objects));
}
