//! Screen drawing of equations: the outlines InDesign stored with each MathML object land
//! inside the object's frame, on the frame's own size (oracle/goldens/mathml-c1/storage).

use folio_model::layout::{DocumentLayout, FontId, GlyphSource, OutlineCmd};
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct NoGlyphs;
impl GlyphSource for NoGlyphs {
    fn outline(&self, _: FontId, _: u16) -> Option<Arc<Vec<OutlineCmd>>> {
        None
    }
}

fn evidence() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/mathml-c1/storage/after.idml")
}

/// The ink's bounding box in pixels (x0, y0, x1, y1 exclusive) inside `area`, and the inked pixel count.
fn ink(r: &folio_render::Raster, area: (u32, u32, u32, u32)) -> Option<((u32, u32, u32, u32), usize)> {
    let (mut x0, mut y0, mut x1, mut y1, mut n) = (u32::MAX, u32::MAX, 0, 0, 0);
    for y in area.1..area.3 {
        for x in area.0..area.2 {
            let i = ((y * r.width + x) * 4) as usize;
            if r.rgba[i] < 128 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
                n += 1;
            }
        }
    }
    (n > 0).then_some(((x0, y0, x1, y1), n))
}

#[test]
fn each_equation_is_drawn_over_its_whole_frame() {
    let doc = folio_idml::read_package(&evidence(), None).unwrap();
    let colors = folio_color::Colors::unmanaged();
    let layout = DocumentLayout::default();
    let images = folio_render::ImageCache::new();
    let glyph_cache = folio_render::GlyphCache::new();
    let scene = folio_render::Scene { doc: &doc, colors: &colors, layout: &layout, glyphs: &NoGlyphs, glyph_cache: &glyph_cache, images: &images };
    let scale = 8.0;
    let r = folio_render::render_page(&scene, 0, scale, None).expect("render");
    let (_, _, page) = folio_render::locate_page(&doc, 0).unwrap();

    let mut frames = vec![];
    folio_idml::for_each_item(&doc, &mut |item| {
        if item.math.is_some() {
            let b = item.transform.apply_rect(&folio_model::paths_bounds(&item.paths).unwrap());
            frames.push(b);
        }
    });
    assert_eq!(frames.len(), 4);
    for f in frames {
        // frame in pixels, page-relative
        let (l, t) = ((f.left - page.left) * scale, (f.top - page.top) * scale);
        let (rr, bb) = ((f.right - page.left) * scale, (f.bottom - page.top) * scale);
        let area = ((l - 4.0).floor() as u32, (t - 4.0).floor() as u32, (rr + 4.0).ceil() as u32, (bb + 4.0).ceil() as u32);
        let (b, n) = ink(&r, area).unwrap_or_else(|| panic!("nothing drawn in {f:?}"));
        // the ink spans most of the view box (a fraction's bar is narrower than its box) and never leaves the frame
        let (w, h) = (rr - l, bb - t);
        assert!(f64::from(b.0) >= l - 2.0 && f64::from(b.2) <= rr + 2.0 && f64::from(b.1) >= t - 2.0 && f64::from(b.3) <= bb + 2.0, "ink {b:?} leaves the frame {l} {t} {rr} {bb}");
        assert!(f64::from(b.2 - b.0) > 0.6 * w, "ink {b:?} is narrower than the frame ({w})");
        assert!(f64::from(b.3 - b.1) > 0.55 * h, "ink {b:?} is lower than the frame ({h})");
        assert!(n > 50, "too little ink: {n}");
    }
    // outside the equations the page is empty
    assert!(ink(&r, (0, 0, 100, 100)).is_none());
}
