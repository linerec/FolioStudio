//! Separations preview on a synthetic document: which plates exist after the Ink Manager,
//! what each plate carries under knockout, overprint (explicit and [Black] at 100 %),
//! opacity and blend modes, and how the views show it.

use folio_color::Colors;
use folio_model::layout::{DocumentLayout, FontId, GlyphSource, OutlineCmd};
use folio_model::{
    Affine, BlendMode, ColorModel, ColorSpace, Document, Ink, ItemKind, ItemPrint, Page, PageItem, Paint, PathPoint, Point, PrintSetup, Rect, Spread,
    SubPath, Swatch,
};
use folio_render::sep::{InkImages, InkKind, View, composite, document_inks, render_inks};
use folio_render::{GlyphCache, ImageCache, Scene};

struct NoGlyphs;
impl GlyphSource for NoGlyphs {
    fn outline(&self, _: FontId, _: u16) -> Option<std::sync::Arc<Vec<OutlineCmd>>> {
        None
    }
}

fn rect(w: f64, h: f64) -> Vec<SubPath> {
    let p = |x: f64, y: f64| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(0.0, 0.0), p(0.0, h), p(w, h), p(w, 0.0)], open: false }]
}

fn swatch(id: &str, name: &str, model: ColorModel, space: ColorSpace, values: &[f64]) -> (String, Swatch) {
    let s = Swatch { id: id.into(), name: name.into(), model, space, values: values.to_vec(), base: None, tint: None, display_rgb: [0, 0, 0] };
    (id.into(), s)
}

fn ink(name: &str, convert: bool, alias: Option<&str>) -> Ink {
    Ink { name: name.into(), process: false, convert_to_process: convert, alias: alias.map(String::from), print: true, ink_type: "Normal".into(), neutral_density: None }
}

/// A 40 × 40 pt square at (x, y).
fn square(id: &str, fill: &str, x: f64, y: f64) -> PageItem {
    PageItem {
        id: id.into(),
        kind: ItemKind::Rectangle { content: None },
        transform: Affine::translate(x, y),
        paths: rect(40.0, 40.0),
        fill: Paint::Swatch { id: fill.into(), tint: 100.0 },
        visible: true,
        opacity: 1.0,
        ..PageItem::default()
    }
}

/// One 200 × 200 pt page. A cyan background over the left half; on it: an overprinting
/// spot square, a knockout spot square, a [Black] square (overprints by the document
/// setting), a 50 % opaque black square, a multiply yellow square, an aliased spot and a
/// converted spot, a nonprinting square.
fn document() -> Document {
    let swatches = [
        swatch("Color/Black", "Black", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0]),
        swatch("Color/Cyan", "Cyan", ColorModel::Process, ColorSpace::Cmyk, &[100.0, 0.0, 0.0, 0.0]),
        swatch("Color/Yellow", "Yellow", ColorModel::Process, ColorSpace::Cmyk, &[0.0, 0.0, 50.0, 0.0]),
        swatch("Color/Orange", "Orange", ColorModel::Spot, ColorSpace::Cmyk, &[0.0, 60.0, 100.0, 0.0]),
        swatch("Color/Gold", "Gold", ColorModel::Spot, ColorSpace::Cmyk, &[0.0, 20.0, 100.0, 10.0]),
        swatch("Color/Blue", "Blue", ColorModel::Spot, ColorSpace::Cmyk, &[100.0, 50.0, 0.0, 0.0]),
    ]
    .into_iter()
    .collect();
    let mut background = square("bg", "Color/Cyan", 0.0, 0.0);
    background.paths = rect(100.0, 200.0);
    let mut op_spot = square("opSpot", "Color/Orange", 10.0, 10.0);
    op_spot.print = ItemPrint { overprint_fill: true, ..Default::default() };
    let knock_spot = square("koSpot", "Color/Orange", 50.0, 10.0);
    let black = square("black", "Color/Black", 10.0, 60.0);
    let mut half = square("half", "Color/Black", 50.0, 60.0);
    half.fill = Paint::Swatch { id: "Color/Black".into(), tint: 99.0 }; // not [Black] at 100 %: knocks out
    half.opacity = 0.5;
    let mut multiply = square("multiply", "Color/Yellow", 10.0, 110.0);
    multiply.blend_mode = BlendMode::Multiply;
    let gold = square("gold", "Color/Gold", 110.0, 10.0);
    let blue = square("blue", "Color/Blue", 110.0, 60.0);
    let mut hidden = square("nonprinting", "Color/Black", 110.0, 110.0);
    hidden.print = ItemPrint { nonprinting: true, ..Default::default() };
    let spread = Spread {
        id: "s".into(),
        pages: vec![Page { id: "p".into(), name: "1".into(), bounds: Rect { left: 0.0, top: 0.0, right: 200.0, bottom: 200.0 }, ..Page::default() }],
        items: vec![background, op_spot, knock_spot, black, half, multiply, gold, blue, hidden],
        ..Spread::default()
    };
    Document {
        spreads: vec![spread],
        swatches,
        print: PrintSetup {
            overprint_black: true,
            inks: vec![ink("Orange", false, None), ink("Gold", false, Some("Orange")), ink("Blue", true, None)],
            ..PrintSetup::default()
        },
        ..Document::default()
    }
}

#[test]
fn plates_follow_print() {
    let doc = document();
    let layout = DocumentLayout::default();
    let colors = Colors::unmanaged();
    let (glyph_cache, images, ink_images) = (GlyphCache::new(), ImageCache::new(), InkImages::new());
    let scene = Scene { doc: &doc, colors: &colors, layout: &layout, glyphs: &NoGlyphs, glyph_cache: &glyph_cache, images: &images };

    // plates: CMYK and the one spot ink that keeps its plate; Gold prints on it, Blue is process
    let inks = document_inks(&doc, &layout, &colors);
    let names: Vec<&str> = inks.inks.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, ["Cyan", "Magenta", "Yellow", "Black", "Orange"]);
    assert_eq!(inks.inks[4].kind, InkKind::Spot);
    assert_eq!(inks.inks[4].aliases, ["Gold"]);
    assert_eq!(inks.converted, ["Blue"]);
    assert_eq!(inks.pages, [vec![0, 1, 2, 3, 4]]);

    // 1 px per point
    let planes = render_inks(&scene, &ink_images, &inks, 0, 1.0, None).expect("planes");
    let at = |x: u32, y: u32| planes.at(x, y).unwrap().iter().map(|v| v.round() as i32).collect::<Vec<_>>();
    // cyan background alone
    assert_eq!(at(5, 190), [100, 0, 0, 0, 0]);
    // overprinting spot: cyan kept
    assert_eq!(at(30, 30), [100, 0, 0, 0, 100]);
    // knockout spot: cyan gone (it also runs past the background: spot alone)
    assert_eq!(at(70, 30), [0, 0, 0, 0, 100]);
    // [Black] at 100 % overprints by the document setting
    assert_eq!(at(30, 80), [100, 0, 0, 100, 0]);
    // 99 % black at 50 % opacity knocks out half of the cyan
    let half = at(70, 80);
    assert!((half[0] - 50).abs() <= 1 && (half[3] - 50).abs() <= 1, "{half:?}");
    // multiply 50 % yellow over cyan: both inks
    assert_eq!(at(30, 130), [100, 0, 50, 0, 0]);
    // the alias prints on Orange's plate, the converted spot on process plates
    assert_eq!(at(130, 30), [0, 0, 0, 0, 100]);
    assert_eq!(at(130, 80), [100, 50, 0, 0, 0]);
    // nonprinting: nothing
    assert_eq!(at(130, 130), [0, 0, 0, 0, 0]);

    // a single plate shows as black ink on white
    let only_spot = composite(&planes, &inks, &colors, &View::Separations { visible: vec![false, false, false, false, true] });
    let px = |rgba: &[u8], x: usize, y: usize| rgba[(y * 200 + x) * 4];
    assert_eq!((px(&only_spot, 30, 30), px(&only_spot, 5, 190)), (0, 255));
    // "Show Single Plates in Black" off: the plate shows in its own ink colour (white where bare)
    let own = folio_render::sep::composite_with(&planes, &inks, &colors, &View::Separations { visible: vec![false, false, false, false, true] }, folio_render::sep::Look { single_plates_black: false, ..Default::default() });
    let rgb3 = |rgba: &[u8], x: usize, y: usize| [rgba[(y * 200 + x) * 4], rgba[(y * 200 + x) * 4 + 1], rgba[(y * 200 + x) * 4 + 2]];
    assert_eq!(rgb3(&own, 5, 190), [255, 255, 255]);
    assert_ne!(rgb3(&own, 30, 30), [0, 0, 0]);
    assert_ne!(rgb3(&own, 30, 30), [255, 255, 255]);
    // ink limit 150 %: the overprinted squares (200 %) are red, the rest gray
    let limit = composite(&planes, &inks, &colors, &View::InkLimit { limit: 150.0 });
    let rgb = |x: usize, y: usize| &limit[(y * 200 + x) * 4..(y * 200 + x) * 4 + 3];
    assert!(rgb(30, 30)[0] > 150 && rgb(30, 30)[1] < 120, "{:?}", rgb(30, 30));
    assert!(rgb(5, 190)[0] == rgb(5, 190)[1], "{:?}", rgb(5, 190));
    // overprint preview: the spot simulated over cyan is darker than either alone
    let op = composite(&planes, &inks, &colors, &View::Overprint);
    let sum = |x: usize, y: usize| op[(y * 200 + x) * 4..(y * 200 + x) * 4 + 3].iter().map(|v| *v as u32).sum::<u32>();
    assert!(sum(30, 30) < sum(70, 30) && sum(30, 30) < sum(5, 190));
}

#[test]
fn bleed_and_slug_regions_show_paint_outside_trim_in_both_renderers() {
    let mut doc = document();
    doc.print.bleed = [12.0, 8.0, 5.0, 7.0];
    doc.print.slug = [24.0, 4.0, 3.0, 18.0];
    doc.spreads[0].items = vec![square("outside", "Color/Cyan", -10.0, -10.0)];
    let layout = DocumentLayout::default();
    let colors = Colors::unmanaged();
    let (glyph_cache, images, ink_images) = (GlyphCache::new(), ImageCache::new(), InkImages::new());
    let scene = Scene { doc: &doc, colors: &colors, layout: &layout, glyphs: &NoGlyphs, glyph_cache: &glyph_cache, images: &images };
    let inks = document_inks(&doc, &layout, &colors);
    let requested = Rect { left: -100.0, top: -100.0, right: 300.0, bottom: 300.0 };
    let area = folio_render::page_region(&doc, 0, Some(requested)).unwrap();
    assert_eq!([area.left, area.top, area.right, area.bottom], [-18.0, -24.0, 218.0, 208.0]);
    let trim = folio_render::render_page(&scene, 0, 1.0, None).unwrap();
    let rgb = folio_render::render_page(&scene, 0, 1.0, Some(requested)).unwrap();
    let planes = render_inks(&scene, &ink_images, &inks, 0, 1.0, Some(requested)).unwrap();
    assert_eq!((trim.width, trim.height), (200, 200));
    assert_eq!((rgb.width, rgb.height), (236, 232));
    assert_eq!((planes.width, planes.height), (rgb.width, rgb.height));
    // This sample lies at (-5, -5), outside trim and inside the cyan square.
    assert_eq!(planes.at(13, 19).unwrap()[0], 100.0);
    let px = (19 * rgb.width as usize + 13) * 4;
    assert!(rgb.rgba[px] < 250, "the outside of trim must contain actual paint");
    assert!(folio_render::page_region(&doc, 0, Some(Rect { left: 500.0, ..requested })).is_none());
    assert!(folio_render::page_region(&doc, 0, Some(Rect { left: f64::NAN, ..requested })).is_none());
}

/// Desaturate Black (Separations Preview panel menu) shows the black plate at 80 % (InDesign 2026; the colours of the 14 patches are in
/// the color crate's desaturate_black_reference test).
#[test]
fn desaturate_black_scales_only_the_black_plate_of_the_separations_view() {
    use folio_render::sep::{Look, View, composite_with};
    let colors = folio_color::Colors::unmanaged();
    // one pixel of 60/40/40/100 ink
    let inks = folio_render::sep::DocInks::default();
    let planes = folio_render::sep::InkPlanes { width: 1, height: 1, data: vec![153, 102, 102, 255], count: 4, stats: Default::default() };
    let all = View::Separations { visible: vec![true; 4] };
    let plain = composite_with(&planes, &inks, &colors, &all, Look::default());
    let desat = composite_with(&planes, &inks, &colors, &all, Look { desaturate_black: true, ..Look::default() });
    let expected = folio_color::Colors::unmanaged().cmyk8_to_srgb(153, 102, 102, 204);
    assert_eq!(&desat[..3], &expected);
    assert_ne!(&plain[..3], &desat[..3]);
    // other views are not changed by it
    assert_eq!(composite_with(&planes, &inks, &colors, &View::Overprint, Look::default()), composite_with(&planes, &inks, &colors, &View::Overprint, Look { desaturate_black: true, ..Look::default() }));
}
