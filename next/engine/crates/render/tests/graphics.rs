//! Screen drawing of lowered graphics: dashed stroke parts, cornered paths, image clipping
//! paths, drop / inner shadows and partial (fill) opacity, checked on rendered pixels.

use folio_model::geom::{self, CornerAdjust, CornerKind, CornerOption, StrokePattern, StrokeSpec};
use folio_model::layout::{DocumentLayout, FontId, GlyphSource, OutlineCmd};
use folio_model::*;
use std::sync::Arc;

struct NoGlyphs;
impl GlyphSource for NoGlyphs {
    fn outline(&self, _: FontId, _: u16) -> Option<Arc<Vec<OutlineCmd>>> {
        None
    }
}

fn poly(pts: &[(f64, f64)], open: bool) -> SubPath {
    SubPath { points: pts.iter().map(|&(x, y)| PathPoint { anchor: Point::new(x, y), left: Point::new(x, y), right: Point::new(x, y) }).collect(), open }
}

fn rect(l: f64, t: f64, r: f64, b: f64) -> SubPath {
    poly(&[(l, t), (l, b), (r, b), (r, t)], false)
}

fn swatch(id: &str, rgb: [u8; 3]) -> Swatch {
    Swatch { id: id.into(), name: id.into(), model: ColorModel::Process, space: ColorSpace::Rgb, values: rgb.iter().map(|v| *v as f64).collect(), base: None, tint: None, display_rgb: rgb }
}

fn paint(id: &str) -> Paint {
    Paint::Swatch { id: id.into(), tint: 100.0 }
}

/// One 200 × 200 pt page holding `items`, drawn at 1 px per point.
fn render(items: Vec<PageItem>) -> folio_render::Raster {
    let mut doc = Document::default();
    for (id, rgb) in [("black", [0, 0, 0]), ("red", [255, 0, 0]), ("blue", [0, 0, 255])] {
        doc.swatches.insert(id.into(), swatch(id, rgb));
    }
    doc.spreads.push(Spread {
        id: "s".into(),
        pages: vec![Page { id: "p".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 200.0, bottom: 200.0 }, ..Default::default() }],
        items,
        ..Default::default()
    });
    let colors = folio_color::Colors::unmanaged();
    let layout = DocumentLayout::default();
    let images = folio_render::ImageCache::new();
    let glyph_cache = folio_render::GlyphCache::new();
    let scene = folio_render::Scene { doc: &doc, colors: &colors, layout: &layout, glyphs: &NoGlyphs, glyph_cache: &glyph_cache, images: &images };
    folio_render::render_page(&scene, 0, 1.0, None).expect("render")
}

fn px(r: &folio_render::Raster, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * r.width + x) * 4) as usize;
    [r.rgba[i], r.rgba[i + 1], r.rgba[i + 2]]
}

#[test]
fn dashed_parts_leave_gaps() {
    let path = vec![poly(&[(20.0, 50.5), (180.0, 50.5)], true)];
    let spec = StrokeSpec {
        weight: 4.0,
        miter_limit: 4.0,
        pattern: StrokePattern::Dashed { pattern: vec![10.0, 10.0], adjust: CornerAdjust::None, cap: None, builtin: false },
        ..Default::default()
    };
    let item = PageItem {
        id: "l".into(),
        kind: ItemKind::GraphicLine,
        paths: path.clone(),
        stroke: Stroke { paint: paint("black"), weight: 4.0, art: geom::stroke_art(&path, &spec), ..Default::default() },
        ..Default::default()
    };
    let r = render(vec![item]);
    assert_eq!(px(&r, 25, 50), [0, 0, 0], "inside the first dash");
    assert_eq!(px(&r, 35, 50), [255, 255, 255], "inside the first gap");
    assert_eq!(px(&r, 45, 50), [0, 0, 0], "second dash");
}

#[test]
fn corner_options_shape_the_fill() {
    let p = geom::apply_corners(&rect(20.0, 20.0, 180.0, 180.0), &[CornerOption { kind: CornerKind::InverseRounded, radius: 40.0 }; 4]);
    let item = PageItem { id: "r".into(), kind: ItemKind::Rectangle { content: None }, paths: vec![p], fill: paint("red"), ..Default::default() };
    let r = render(vec![item]);
    assert_eq!(px(&r, 100, 100), [255, 0, 0]);
    assert_eq!(px(&r, 24, 24), [255, 255, 255], "the corner is cut out");
    assert_eq!(px(&r, 100, 22), [255, 0, 0], "edges stay");
}

#[test]
fn clipping_path_masks_the_image() {
    // a solid blue 20 × 20 PNG placed over the whole frame, clipped to a triangle
    let dir = std::env::temp_dir().join(format!("folio-render-gfx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("blue.png");
    image::RgbImage::from_pixel(20, 20, image::Rgb([0, 0, 255])).save(&file).unwrap();
    let graphic = |invert: bool| Graphic {
        id: "g".into(),
        element: "Image".into(),
        transform: Affine::translate(20.0, 20.0),
        bounds: Rect { left: 0.0, top: 0.0, right: 160.0, bottom: 160.0 },
        preview: Some(file.to_string_lossy().into_owned()),
        // Inverted, InDesign writes the image bounds around the path, wound the other way.
        clip: Some(GraphicClip {
            kind: "PhotoshopPath".into(),
            paths: if invert {
                vec![
                    poly(&[(0.0, 0.0), (160.0, 0.0), (160.0, 160.0), (0.0, 160.0)], false),
                    poly(&[(160.0, 160.0), (80.0, 0.0), (0.0, 160.0)], false),
                ]
            } else {
                vec![poly(&[(0.0, 160.0), (80.0, 0.0), (160.0, 160.0)], false)]
            },
            invert,
        }),
        ..Default::default()
    };
    let frame = |g: Graphic| PageItem { id: "f".into(), kind: ItemKind::Rectangle { content: Some(g) }, paths: vec![rect(20.0, 20.0, 180.0, 180.0)], ..Default::default() };
    let r = render(vec![frame(graphic(false))]);
    assert_eq!(px(&r, 100, 150), [0, 0, 255], "inside the path");
    assert_eq!(px(&r, 30, 40), [255, 255, 255], "inside the frame, outside the path");
    let r = render(vec![frame(graphic(true))]);
    assert_eq!(px(&r, 100, 150), [255, 255, 255], "inverted: the path's inside is cut away");
    assert_eq!(px(&r, 30, 40), [0, 0, 255]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shadows_and_partial_opacity_are_drawn() {
    let shadow = ShadowEffect { blend: BlendMode::Multiply, opacity: 0.75, x_offset: 10.0, y_offset: 10.0, size: 2.0, color: paint("black"), ..Default::default() };
    let inner = ShadowEffect { blend: BlendMode::Multiply, opacity: 0.75, x_offset: 5.0, y_offset: 5.0, size: 4.0, color: paint("black"), ..Default::default() };
    let item = PageItem {
        id: "fx".into(),
        kind: ItemKind::Rectangle { content: None },
        paths: vec![rect(40.0, 40.0, 120.0, 120.0)],
        fill: paint("red"),
        print: ItemPrint { fx: ObjectEffects { drop_shadow: Some(shadow), inner_shadow: Some(inner), ..Default::default() }, ..Default::default() },
        ..Default::default()
    };
    let half = PageItem {
        id: "half".into(),
        kind: ItemKind::Rectangle { content: None },
        paths: vec![rect(150.0, 150.0, 190.0, 190.0)],
        fill: paint("blue"),
        print: ItemPrint { fill_blend: Some((0.5, BlendMode::Normal)), ..Default::default() },
        ..Default::default()
    };
    let r = render(vec![item, half]);
    let shadow_px = px(&r, 125, 125);
    assert!(shadow_px[0] < 100 && shadow_px == [shadow_px[0]; 3], "drop shadow below and right: {shadow_px:?}");
    assert_eq!(px(&r, 30, 30), [255, 255, 255], "no shadow up and left");
    let edge = px(&r, 42, 80);
    assert!(edge[0] < 200 && edge[1] == 0, "inner shadow darkens the left edge inside: {edge:?}");
    assert_eq!(px(&r, 110, 80), [255, 0, 0], "lit side untouched");
    let mixed = px(&r, 170, 170);
    assert!((mixed[0] as i32 - 128).abs() <= 2 && mixed[2] == 255, "50 % blue over paper: {mixed:?}");
}
