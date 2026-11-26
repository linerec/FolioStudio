//! Parses an IDML package and renders chosen pages to PNG without text layout.
//! Usage: cargo run -p folio-render --release --example render_pages -- <document.idml> <result.json> <out dir> <scale> <page>...
use folio_model::layout::{DocumentLayout, FontId, GlyphSource, OutlineCmd};
use std::sync::Arc;

struct NoGlyphs;
impl GlyphSource for NoGlyphs {
    fn outline(&self, _: FontId, _: u16) -> Option<Arc<Vec<OutlineCmd>>> {
        None
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (idml, result, out, scale) = (&a[0], &a[1], &a[2], a[3].parse::<f64>().unwrap());
    let t = std::time::Instant::now();
    let doc = folio_idml::read_package(std::path::Path::new(idml), Some(std::path::Path::new(result))).expect("parse");
    println!("parsed in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    // the document's profiles as installed on this machine (no cache)
    let index = folio_color::ProfileIndex::scan_installed();
    let dc = folio_color::document_colors(doc.print.cmyk_profile.as_deref(), doc.print.rgb_profile.as_deref(), &index, None, None);
    for w in &dc.warnings {
        println!("{w}");
    }
    let layout = DocumentLayout::default();
    let images = folio_render::ImageCache::new();
    let glyph_cache = folio_render::GlyphCache::new();
    let scene = folio_render::Scene { doc: &doc, colors: &dc.colors, layout: &layout, glyphs: &NoGlyphs, glyph_cache: &glyph_cache, images: &images };
    for p in &a[4..] {
        let i: usize = p.parse().unwrap();
        let cold = folio_render::render_page(&scene, i, scale, None).expect("page");
        let r = folio_render::render_page(&scene, i, scale, None).expect("page");
        print!("cold {:.1} ms, ", cold.stats.elapsed_ms);
        let img = image::RgbaImage::from_raw(r.width, r.height, r.rgba).unwrap();
        let path = format!("{out}/page-{i:03}.png");
        img.save(&path).unwrap();
        println!("{path} {}x{} items {} images {} (pattern {}) layers {} in {:.1} ms", r.width, r.height, r.stats.items, r.stats.images, r.stats.pattern_images, r.stats.layers, r.stats.elapsed_ms);
    }
}
