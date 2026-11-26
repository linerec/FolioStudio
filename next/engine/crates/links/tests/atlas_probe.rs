//! Header probes of the InDesign Feature Atlas links (real files made for the atlas;
//! skipped when the fixture folder is absent).

use folio_links::{ColorSpace, ImageFormat, probe};
use std::path::PathBuf;

fn links_dir() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../samples/feature-atlas/Links");
    d.is_dir().then_some(d)
}

#[test]
fn atlas_files() {
    let Some(dir) = links_dir() else { return };
    let p = |n: &str| probe(&dir.join(n)).unwrap_or_else(|e| panic!("{n}: {e}"));
    let i = p("grid-rgb.jpg");
    assert_eq!((i.format, i.width, i.height, i.color_space, i.bits), (ImageFormat::Jpeg, 1200, 800, ColorSpace::Rgb, 8));
    assert_eq!(i.ppi.map(|(x, y)| (x.round(), y.round())), Some((300.0, 300.0)));
    let i = p("grid-cmyk.tif");
    assert_eq!((i.format, i.color_space, i.channels, i.pages), (ImageFormat::Tiff, ColorSpace::Cmyk, 4, Some(1)));
    let i = p("grid-1bit.tif");
    assert_eq!((i.color_space, i.bits), (ColorSpace::Gray, 1));
    assert_eq!(p("grid-gray.tif").color_space, ColorSpace::Gray);
    assert_eq!(p("multipage.tif").pages, Some(2));
    let i = p("alpha.png");
    assert_eq!((i.format, i.width, i.height, i.alpha), (ImageFormat::Png, 800, 600, true));
    let i = p("flat-alpha.psd");
    assert_eq!((i.format, i.width, i.height, i.color_space, i.alpha), (ImageFormat::Psd, 400, 300, ColorSpace::Rgb, true));
    let i = p("vector-pages.pdf");
    assert_eq!((i.format, i.pages, i.vector), (ImageFormat::Pdf, Some(2), true));
    let b = &i.page_boxes[0];
    assert_eq!((b.media, b.crop, b.trim), ([0.0, 0.0, 360.0, 260.0], Some([12.0, 12.0, 348.0, 248.0]), Some([24.0, 24.0, 336.0, 236.0])));
    // Helvetica is referenced but not embedded (InDesign's preflight reports it missing)
    assert!(i.fonts.iter().any(|f| f.name == "Helvetica" && !f.embedded), "{:?}", i.fonts);
    assert!(i.vector_colors.as_ref().is_some_and(|c| c.content_scanned && c.spaces.contains(&ColorSpace::Rgb)));
    let i = p("vector.eps");
    assert_eq!((i.format, i.width, i.height, i.vector), (ImageFormat::Eps, 300, 200, true));
    let i = p("vector.svg");
    assert_eq!((i.format, i.width, i.height), (ImageFormat::Svg, 450, 300));
    assert!(probe(&dir.join("motion.mp4")).is_err());
}
