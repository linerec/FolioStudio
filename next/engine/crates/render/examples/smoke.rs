//! Renders a hand-built two-page spread (with a master item) to PNG files, to check
//! coordinate handling without the IDML parser. Usage: cargo run -p folio-render --example smoke -- <out dir>
use folio_model::layout::{DocumentLayout, FontId, GlyphSource, OutlineCmd};
use folio_model::*;
use std::sync::Arc;

struct NoGlyphs;
impl GlyphSource for NoGlyphs {
    fn outline(&self, _: FontId, _: u16) -> Option<Arc<Vec<OutlineCmd>>> {
        None
    }
}

fn rect_path(l: f64, t: f64, r: f64, b: f64) -> Vec<SubPath> {
    let p = |x, y| PathPoint { anchor: Point::new(x, y), left: Point::new(x, y), right: Point::new(x, y) };
    vec![SubPath { points: vec![p(l, t), p(r, t), p(r, b), p(l, b)], open: false }]
}

fn swatch(id: &str, rgb: [u8; 3]) -> Swatch {
    Swatch { id: id.into(), name: id.into(), model: ColorModel::Process, space: ColorSpace::Rgb, values: vec![], base: None, tint: None, display_rgb: rgb }
}

fn item(id: &str, path: Vec<SubPath>, t: Affine, fill: &str) -> PageItem {
    PageItem { id: id.into(), kind: ItemKind::Rectangle { content: None }, transform: t, paths: path, fill: Paint::Swatch { id: fill.into(), tint: 100.0 }, ..Default::default() }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let mut doc = Document::default();
    for (id, rgb) in [("red", [220, 40, 40]), ("blue", [40, 80, 220]), ("gray", [200, 200, 200])] {
        doc.swatches.insert(id.into(), swatch(id, rgb));
    }
    let page = |id: &str, name: &str, tx: f64, index: usize, master: Option<&str>| Page {
        id: id.into(), name: name.into(), bounds: Rect { left: 0.0, top: 0.0, right: 612.0, bottom: 792.0 },
        transform: Affine::translate(tx, -396.0), master: master.map(Into::into), master_transform: Affine::IDENTITY, index, ..Default::default()
    };
    // master: a gray band at the top of each master page
    doc.master_spreads.push(Spread {
        id: "m".into(), name: Some("A-Master".into()), transform: Affine::IDENTITY,
        pages: vec![page("m1", "A", -612.0, 0, None), page("m2", "A", 0.0, 1, None)],
        items: vec![
            item("band-left", rect_path(-612.0, -396.0, 0.0, -340.0), Affine::IDENTITY, "gray"),
            item("band-right", rect_path(0.0, -396.0, 612.0, -340.0), Affine::IDENTITY, "gray"),
        ],
        ..Spread::default()
    });
    // spread: red square on the left page, a blue square rotated 30° on the right page
    let rot = Affine::new(0.866, 0.5, -0.5, 0.866, 300.0, 0.0);
    doc.spreads.push(Spread {
        id: "s".into(), name: None, transform: Affine::IDENTITY,
        pages: vec![page("p1", "2", -612.0, 0, Some("m")), page("p2", "3", 0.0, 1, Some("m"))],
        items: vec![
            item("red", rect_path(-500.0, -200.0, -300.0, 0.0), Affine::IDENTITY, "red"),
            item("blue", rect_path(-100.0, -100.0, 100.0, 100.0), rot, "blue"),
        ],
        ..Spread::default()
    });
    let colors = folio_color::Colors::unmanaged();
    let layout = DocumentLayout::default();
    let images = folio_render::ImageCache::new();
    let glyph_cache = folio_render::GlyphCache::new();
    let scene = folio_render::Scene { doc: &doc, colors: &colors, layout: &layout, glyphs: &NoGlyphs, glyph_cache: &glyph_cache, images: &images };
    for i in 0..2 {
        let r = folio_render::render_page(&scene, i, 0.5, None).expect("page");
        let img = image::RgbaImage::from_raw(r.width, r.height, r.rgba).unwrap();
        let path = format!("{out}/smoke-page{i}.png");
        img.save(&path).unwrap();
        println!("{path} {}x{} items {} in {:.1} ms", r.width, r.height, r.stats.items, r.stats.elapsed_ms);
    }
}
