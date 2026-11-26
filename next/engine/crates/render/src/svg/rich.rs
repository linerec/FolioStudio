//! SVG beyond the static subset the own reader draws (text, filters, masks, gradients,
//! patterns, clip paths, opacity, CSS style sheets, embedded bitmaps, markers): read and drawn
//! by resvg/usvg (Apache-2.0 OR MIT). Text is set with the fonts installed on this computer
//! (fontdb, loaded once). The screen draws the whole picture through resvg; PDF output
//! ([`Scene::parts`]) keeps plain paths of solid colour (glyph outlines too) as vectors,
//! clipped by single-path clip paths, and draws what PDF output does not hold as vectors here
//! (filters, masks, gradients, patterns, opacity, blend modes, bitmaps) as pixels.
//!
//! An `<image>` that names a file is not drawn: the picture would no longer be next to the SVG
//! once it is placed into a project (principle 1); a `data:` picture inside the SVG is drawn.
//! Scripts and animation are not run (the picture as it first appears).
use super::{Part, Scene, Shape};
use folio_model::{Affine, LineCap, LineJoin, Rect};
use std::sync::{Arc, OnceLock};
use resvg::usvg;
use resvg::tiny_skia;

/// The parsed picture (usvg's tree).
pub struct Rich(pub(super) usvg::Tree);

impl std::fmt::Debug for Rich {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rich").field("size", &self.0.size()).finish()
    }
}

/// The installed fonts SVG text is set with (read once per process).
fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    }).clone()
}

/// Reads `text` with usvg. `Err` names why the picture cannot be read.
pub(super) fn read(text: &str) -> Result<Scene, String> {
    let mut opt = usvg::Options { fontdb: fonts(), ..usvg::Options::default() };
    // a character the text's font lacks: the system's face for its writing first (Malgun Gothic, Yu Gothic, Microsoft YaHei,
    // Apple SD Gothic Neo, Noto CJK …; folio_text::glyphfill), as placed text is filled. usvg sets a whole run in the fallback
    // face when that face has every character of it.
    opt.font_resolver.select_fallback = Box::new(|c, exclude, db| {
        use folio_text::glyphfill::FillScript;
        let base = db.face(*exclude.first()?)?;
        let (weight, style) = (base.weight, base.style);
        let has = |id| db.with_face_data(id, |data, index| ttf_parser::Face::parse(data, index).is_ok_and(|f| f.glyph_index(c).is_some())).unwrap_or(false);
        for family in FillScript::of(c, FillScript::Korean).os_families() {
            let query = usvg::fontdb::Query { families: &[usvg::fontdb::Family::Name(family)], weight, style, ..Default::default() };
            if let Some(id) = db.query(&query).filter(|id| !exclude.contains(id) && has(*id)) {
                return Some(id);
            }
        }
        (usvg::FontResolver::default_fallback_selector())(c, exclude, db)
    });
    // pictures inside the SVG (data: URLs) only; a file it names is not read
    opt.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
        resolve_string: Box::new(|_, _| None),
    };
    let tree = usvg::Tree::from_str(text, &opt).map_err(|e| format!("SVG: {e}"))?;
    let size = tree.size();
    Ok(Scene { width: size.width() as f64, height: size.height() as f64, shapes: Vec::new(), rich: Some(Arc::new(Rich(tree))) })
}

fn affine(t: tiny_skia::Transform) -> Affine {
    Affine::new(t.sx as f64, t.ky as f64, t.kx as f64, t.sy as f64, t.tx as f64, t.ty as f64)
}

fn skia(t: &Affine) -> tiny_skia::Transform {
    tiny_skia::Transform::from_row(t.a as f32, t.b as f32, t.c as f32, t.d as f32, t.tx as f32, t.ty as f32)
}

/// A tiny-skia path as the shapes' sub-paths (through SVG path data and the own parser).
fn subpaths(p: &tiny_skia::Path) -> Option<Vec<folio_model::SubPath>> {
    use std::fmt::Write;
    let mut d = String::new();
    for s in p.segments() {
        let _ = match s {
            tiny_skia::PathSegment::MoveTo(a) => write!(d, "M{} {}", a.x, a.y),
            tiny_skia::PathSegment::LineTo(a) => write!(d, "L{} {}", a.x, a.y),
            tiny_skia::PathSegment::QuadTo(c, a) => write!(d, "Q{} {} {} {}", c.x, c.y, a.x, a.y),
            tiny_skia::PathSegment::CubicTo(c1, c2, a) => write!(d, "C{} {} {} {} {} {}", c1.x, c1.y, c2.x, c2.y, a.x, a.y),
            tiny_skia::PathSegment::Close => write!(d, "Z"),
        };
    }
    super::path::parse(&d, &Affine::IDENTITY)
}

fn solid(paint: &usvg::Paint) -> Option<[u8; 3]> {
    match paint {
        usvg::Paint::Color(c) => Some([c.red, c.green, c.blue]),
        _ => None,
    }
}

/// A path PDF output holds as it is: solid paint, full opacity.
fn plain_path(p: &usvg::Path) -> Option<Shape> {
    let fill = match p.fill() {
        Some(f) if f.opacity().get() >= 1.0 => Some((solid(f.paint())?, f.rule() == usvg::FillRule::EvenOdd)),
        Some(_) => return None,
        None => None,
    };
    let stroke = match p.stroke() {
        Some(s) if s.opacity().get() >= 1.0 => Some((solid(s.paint())?, s)),
        Some(_) => return None,
        None => None,
    };
    // stroke under the fill: only when there is no fill to cover it
    if p.paint_order() == usvg::PaintOrder::StrokeAndFill && fill.is_some() && stroke.is_some() {
        return None;
    }
    let paths = subpaths(p.data())?;
    let (width, cap, join, miter, dash) = match stroke {
        Some((_, s)) => (
            s.width().get() as f64,
            match s.linecap() { usvg::LineCap::Butt => LineCap::Butt, usvg::LineCap::Round => LineCap::Round, usvg::LineCap::Square => LineCap::Projecting },
            match s.linejoin() { usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter, usvg::LineJoin::Round => LineJoin::Round, usvg::LineJoin::Bevel => LineJoin::Bevel },
            s.miterlimit().get() as f64,
            s.dasharray().map(|a| folio_model::geom::Dash { array: a.iter().map(|v| *v as f64).collect(), phase: s.dashoffset() as f64 }),
        ),
        None => (0.0, LineCap::Butt, LineJoin::Miter, 4.0, None),
    };
    Some(Shape {
        paths,
        transform: affine(p.abs_transform()),
        fill: fill.map(|f| f.0),
        stroke: stroke.map(|s| s.0),
        width,
        cap,
        join,
        miter,
        dash,
        evenodd: fill.is_some_and(|f| f.1),
        color: [0, 0, 0],
        fill_current: false,
        stroke_current: false,
        dash_offset: 0.0,
        visible: true,
    })
}

/// The single path a group's clip path is, in scene coordinates: only a clip path of one plain path (no clip of its own).
fn plain_clip(g: &usvg::Group) -> Option<Part> {
    let clip = g.clip_path()?;
    if clip.clip_path().is_some() {
        return None;
    }
    let [usvg::Node::Path(p)] = clip.root().children() else { return None };
    if !p.is_visible() {
        return None;
    }
    // as resvg clips: the group's own coordinates, then the clip path's transform (its paths draw without their own)
    let t = g.abs_transform().pre_concat(clip.transform());
    Some(Part::Clip { paths: subpaths(p.data())?, transform: affine(t), evenodd: p.fill().is_some_and(|f| f.rule() == usvg::FillRule::EvenOdd) })
}

/// The largest raster drawn for one node (pixels per side).
const MAX_SIDE: f64 = 6000.0;

/// Node `n` (whose parent's transform to the scene is `parent`) as pixels: `px` pixels per scene unit.
fn raster(n: &usvg::Node, parent: tiny_skia::Transform, px: f64) -> Option<Part> {
    let b = n.abs_layer_bounding_box()?;
    let px = px.min(MAX_SIDE / b.width().max(b.height()) as f64).max(0.01);
    let (w, h) = ((b.width() as f64 * px).ceil().max(1.0) as u32, (b.height() as f64 * px).ceil().max(1.0) as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    // render_node draws in scene coordinates shifted to the node's box and its own transform only: the parent's goes first
    let to_pixels = tiny_skia::Transform::from_scale(px as f32, px as f32).pre_translate(-b.x(), -b.y());
    let t = to_pixels.pre_concat(parent).pre_translate(b.x(), b.y());
    resvg::render_node(n, t, &mut pixmap.as_mut())?;
    let rect = Rect { left: b.x() as f64, top: b.y() as f64, right: b.x() as f64 + w as f64 / px, bottom: b.y() as f64 + h as f64 / px };
    Some(Part::Raster { pixmap, rect })
}

fn walk(g: &usvg::Group, out: &mut Vec<Part>, px: f64) {
    // the transform from the children's coordinates to the scene
    let here = g.abs_transform();
    for n in g.children() {
        match n {
            usvg::Node::Group(c) => {
                let plain = c.filters().is_empty() && c.mask().is_none() && c.opacity().get() >= 1.0 && c.blend_mode() == usvg::BlendMode::Normal;
                let clip = match c.clip_path() {
                    None => None,
                    Some(_) => plain_clip(c),
                };
                if !plain || (c.clip_path().is_some() && clip.is_none()) {
                    out.extend(raster(n, here, px));
                    continue;
                }
                let clipped = clip.is_some();
                out.extend(clip);
                walk(c, out, px);
                if clipped {
                    out.push(Part::EndClip);
                }
            }
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                match plain_path(p) {
                    Some(s) => out.push(Part::Shape(s)),
                    None => out.extend(raster(n, here, px)),
                }
            }
            usvg::Node::Text(t) => walk(t.flattened(), out, px),
            usvg::Node::Image(_) => out.extend(raster(n, here, px)),
        }
    }
}

impl Rich {
    pub(super) fn parts(&self, px: f64) -> Vec<Part> {
        let mut out = Vec::new();
        walk(self.0.root(), &mut out, px);
        out
    }

    /// Draws the picture through `t` (scene → pixmap) into `pixmap`, inside `mask`, its colours through `rgb`.
    pub(super) fn draw(&self, pixmap: &mut tiny_skia::Pixmap, t: &Affine, mask: &tiny_skia::Mask, rgb: &dyn Fn([u8; 3]) -> [u8; 3]) {
        let size = self.0.size();
        let corners = [(0.0, 0.0), (size.width() as f64, 0.0), (0.0, size.height() as f64), (size.width() as f64, size.height() as f64)].map(|(x, y)| t.apply(folio_model::Point { x, y }));
        let x0 = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min).floor().max(0.0);
        let y0 = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min).floor().max(0.0);
        let x1 = corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max).ceil().min(pixmap.width() as f64);
        let y1 = corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max).ceil().min(pixmap.height() as f64);
        if !(x1 > x0 && y1 > y0) {
            return;
        }
        let Some(mut layer) = tiny_skia::Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else { return };
        let shifted = Affine::translate(-x0, -y0).then_inner(t);
        resvg::render(&self.0, skia(&shifted), &mut layer.as_mut());
        // the document's colour handling (CMYK proof, RGB profile) for every colour drawn
        let mut seen: std::collections::HashMap<[u8; 3], [u8; 3]> = std::collections::HashMap::new();
        for p in layer.pixels_mut() {
            let a = p.alpha();
            if a == 0 {
                continue;
            }
            let c = p.demultiply();
            let key = [c.red(), c.green(), c.blue()];
            let v = *seen.entry(key).or_insert_with(|| rgb(key));
            if v != key {
                *p = tiny_skia::ColorU8::from_rgba(v[0], v[1], v[2], a).premultiply();
            }
        }
        pixmap.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), Some(mask));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RICH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 200 100">
      <defs>
        <linearGradient id="g" x1="0" x2="1"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>
        <filter id="b"><feGaussianBlur stdDeviation="3"/></filter>
        <mask id="m"><rect width="200" height="50" fill="#fff"/></mask>
        <clipPath id="c"><circle cx="150" cy="50" r="30"/></clipPath>
        <style>.ink{fill:#008000}</style>
      </defs>
      <rect width="100" height="40" fill="url(#g)"/>
      <rect x="10" y="50" width="40" height="40" fill="#000" filter="url(#b)"/>
      <rect x="60" y="50" width="40" height="40" fill="#f0f" mask="url(#m)"/>
      <g clip-path="url(#c)"><rect class="ink" x="100" y="0" width="100" height="100"/></g>
      <rect x="0" y="0" width="200" height="100" fill="none" stroke="#000" opacity="0.5"/>
      <text x="110" y="95" font-family="Arial" font-size="12" fill="#000">Folio 글자</text>
    </svg>"##;

    #[test]
    fn rich_svg_is_read_and_split_into_vectors_and_pixels() {
        assert!(super::super::parse(RICH).is_err(), "beyond the static subset");
        let scene = super::super::parse_any(RICH).unwrap();
        assert_eq!((scene.width, scene.height), (200.0, 100.0));
        let parts = scene.parts(2.0);
        let rasters = parts.iter().filter(|p| matches!(p, Part::Raster { .. })).count();
        let shapes = parts.iter().filter(|p| matches!(p, Part::Shape(_))).count();
        let clips = parts.iter().filter(|p| matches!(p, Part::Clip { .. })).count();
        // gradient, blur, mask, half-opaque frame as pixels; the clipped rectangle a vector in a clip; glyph outlines vectors
        assert_eq!(rasters, 4, "{parts:?}");
        assert_eq!(clips, 1);
        assert!(shapes >= 2, "{shapes}");
        // the screen: the gradient's left end is red, the green square is clipped to the circle
        let mut pixmap = tiny_skia::Pixmap::new(200, 100).unwrap();
        let mut mask = tiny_skia::Mask::new(200, 100).unwrap();
        mask.data_mut().fill(255);
        scene.rich.as_ref().unwrap().draw(&mut pixmap, &Affine::IDENTITY, &mask, &|c| c);
        let px = |x: u32, y: u32| pixmap.pixel(x, y).unwrap().demultiply();
        assert!(px(2, 20).red() > 200 && px(2, 20).blue() < 60);
        assert!(px(150, 50).green() > 100 && px(150, 50).red() < 40);
        assert_eq!(px(195, 5).alpha(), 0, "outside the clip circle");
    }

    #[test]
    fn text_takes_installed_faces_and_the_systems_face_for_another_writing() {
        let db = fonts();
        let has = |name: &str| db.faces().any(|f| f.families.iter().any(|n| n.0 == name));
        for (fam, text, want) in [("'Arial', sans-serif", "Ab", "Arial"), ("Arial", "EM 글", "Malgun Gothic")] {
            if !has(want) {
                continue;
            }
            let s = read(&format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40"><text x="0" y="30" font-family="{fam}" font-weight="700" font-size="20">{text}</text></svg>"#)).unwrap();
            let tree = &s.rich.as_ref().unwrap().0;
            let mut used = Vec::new();
            for n in tree.root().children() {
                if let usvg::Node::Text(t) = n {
                    for span in t.layouted() {
                        for g in &span.positioned_glyphs {
                            used.push(db.face(g.font).map(|f| (f.families.first().map(|n| n.0.clone()).unwrap_or_default(), f.weight.0)));
                        }
                    }
                }
            }
            assert!(!used.is_empty() && used.iter().all(|u| u.as_ref().is_some_and(|(f, _)| f == want)), "{fam} {text}: {used:?}");
        }
    }

    #[test]
    fn a_named_picture_file_is_not_read() {
        let s = super::super::parse_any(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><image width="10" height="10" xlink:href="C:/secret.png"/><text>x</text></svg>"#).unwrap();
        assert!(s.parts(1.0).iter().all(|p| !matches!(p, Part::Raster { .. })));
    }
}
