//! Converts item, glyph, and gradient geometry into tiny-skia drawing primitives.

use folio_model::geom::StrokeArt;
use folio_model::layout::OutlineCmd;
use folio_model::{Affine, GradientGeometry, GradientKind, ItemKind, LineCap, LineJoin, PageItem, Rect, SubPath};
use tiny_skia::{FillRule, GradientStop, LinearGradient, PathBuilder, Point, RadialGradient, Shader, SpreadMode, Transform};

/// Bounds of an item in its parent's coordinates, with room for its stroke and, for text
/// frames, text that runs past the frame (groups: union of their members).
pub(crate) fn item_extent(item: &PageItem) -> Option<Rect> {
    let inner = match &item.kind {
        ItemKind::Group { children } => children.iter().filter_map(item_extent).reduce(|a, b| a.union(&b))?,
        _ => {
            let b = folio_model::paths_bounds(&item.paths)?;
            let m = item.stroke.weight + if item.text_frame().is_some() { 36.0 } else { 0.0 };
            let r = Rect { left: b.left - m, top: b.top - m, right: b.right + m, bottom: b.bottom + m };
            match item.stroke.art.as_ref().and_then(StrokeArt::bounds) {
                Some(a) => r.union(&a),
                None => r,
            }
        }
    };
    Some(item.transform.apply_rect(&inner))
}

fn skia_cap(c: LineCap) -> tiny_skia::LineCap {
    match c {
        LineCap::Butt => tiny_skia::LineCap::Butt,
        LineCap::Round => tiny_skia::LineCap::Round,
        LineCap::Projecting => tiny_skia::LineCap::Square,
    }
}

fn skia_join(j: LineJoin) -> tiny_skia::LineJoin {
    match j {
        LineJoin::Miter => tiny_skia::LineJoin::Miter,
        LineJoin::Round => tiny_skia::LineJoin::Round,
        LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
    }
}

/// tiny-skia stroke settings; InDesign's miter limit defaults to 4. Thin lines keep their
/// true width (a one-pixel minimum, as some viewers draw them, measured worse on #347).
pub(crate) fn skia_stroke(width: f64, cap: LineCap, join: LineJoin, miter: Option<f64>, dash: Option<&folio_model::geom::Dash>) -> tiny_skia::Stroke {
    tiny_skia::Stroke {
        width: width as f32,
        line_cap: skia_cap(cap),
        line_join: skia_join(join),
        miter_limit: miter.unwrap_or(4.0).max(1.0) as f32,
        dash: dash.and_then(|d| tiny_skia::StrokeDash::new(d.array.iter().map(|v| *v as f32).collect(), d.phase as f32)),
    }
}

pub(crate) fn outline_path(cmds: &[OutlineCmd]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for c in cmds {
        match *c {
            OutlineCmd::MoveTo(x, y) => pb.move_to(x, y),
            OutlineCmd::LineTo(x, y) => pb.line_to(x, y),
            OutlineCmd::QuadTo(x1, y1, x, y) => pb.quad_to(x1, y1, x, y),
            OutlineCmd::CubicTo(x1, y1, x2, y2, x, y) => pb.cubic_to(x1, y1, x2, y2, x, y),
            OutlineCmd::Close => pb.close(),
        }
    }
    pb.finish()
}

pub(crate) fn build_path(paths: &[SubPath]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for sp in paths {
        let pts = &sp.points;
        let Some(first) = pts.first() else { continue };
        pb.move_to(first.anchor.x as f32, first.anchor.y as f32);
        for w in pts.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            pb.cubic_to(a.right.x as f32, a.right.y as f32, b.left.x as f32, b.left.y as f32, b.anchor.x as f32, b.anchor.y as f32);
        }
        if !sp.open && pts.len() > 1 {
            let (a, b) = (pts.last().expect("the first point was already found"), first);
            pb.cubic_to(a.right.x as f32, a.right.y as f32, b.left.x as f32, b.left.y as f32, b.anchor.x as f32, b.anchor.y as f32);
            pb.close();
        }
    }
    pb.finish()
}

pub(crate) fn to_skia(t: &Affine) -> Transform {
    Transform::from_row(t.a as f32, t.b as f32, t.c as f32, t.d as f32, t.tx as f32, t.ty as f32)
}

pub(super) fn item_fill_rule(paths: &[SubPath]) -> FillRule {
    if paths.len() > 1 { FillRule::EvenOdd } else { FillRule::Winding }
}

pub(super) fn gradient_shader(
    kind: GradientKind,
    geo: &GradientGeometry,
    stops: Vec<GradientStop>,
    transform: Transform,
) -> Option<Shader<'static>> {
    let start = Point::from_xy(geo.start.x as f32, geo.start.y as f32);
    match kind {
        GradientKind::Linear => {
            // The angle is counter-clockwise on screen; inner y points down.
            let angle = geo.angle.to_radians();
            let end = Point::from_xy(
                (geo.start.x + geo.length * angle.cos()) as f32,
                (geo.start.y - geo.length * angle.sin()) as f32,
            );
            LinearGradient::new(start, end, stops, SpreadMode::Pad, transform)
        }
        GradientKind::Radial => {
            RadialGradient::new(start, 0.0, start, geo.length.max(0.01) as f32, stops, SpreadMode::Pad, transform)
        }
    }
}
