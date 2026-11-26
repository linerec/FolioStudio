use crate::FontStore;
use folio_model::{
    Affine, PageItem, Point,
    layout::{LaidLine, PlacedGlyph},
    textpath::{HorizontalTextPath, Offset, OffsetJoin, PathEffect, PathMeasure, PathTextAlignment},
};

fn axes(effect: PathEffect, t: Point, p: Point, center: Point, sign: f64) -> Affine {
    let normal = Point::new(-t.y, t.x);
    let horizontal = Point::new(sign, 0.0);
    let vertical = Point::new(0.0, sign);
    let (x, y) = match effect {
        PathEffect::Rainbow => (t, normal),
        PathEffect::Skew => (t, vertical),
        PathEffect::Ribbon => (horizontal, normal),
        PathEffect::Stair => (horizontal, vertical),
        PathEffect::Gravity => {
            let r = Point::new(center.x - p.x, center.y - p.y);
            let det = t.x * r.y - t.y * r.x;
            let y = if det.abs() <= 1e-9 {
                normal
            } else {
                Point::new(r.x / det, r.y / det)
            };
            (t, y)
        }
    };
    Affine::new(x.x, x.y, y.x, y.y, 0.0, 0.0)
}

fn text_height(alignment: PathTextAlignment, glyph: &PlacedGlyph, fonts: &FontStore) -> f64 {
    if alignment == PathTextAlignment::Baseline {
        return 0.0;
    }
    let metrics = fonts.metrics(glyph.font);
    let height = match alignment {
        PathTextAlignment::Ascender => metrics.bbox_top,
        PathTextAlignment::Descender => -metrics.bbox_bottom,
        PathTextAlignment::Center => (metrics.bbox_top - metrics.bbox_bottom) / 2.0,
        PathTextAlignment::Baseline => 0.0,
    };
    height * glyph.size * glyph.vscale
}

fn stroke_baseline(item: &PageItem, options: &HorizontalTextPath, source: &PathMeasure) -> Option<Offset> {
    let height = options.path_alignment.height(item.stroke.weight)
        * if options.flipped { -1.0 } else { 1.0 };
    let join = OffsetJoin::of_stroke(item.stroke.join, item.stroke.miter_limit.unwrap_or(4.0));
    (height != 0.0).then(|| source.offset_with(height, join)).flatten()
}

pub(crate) fn width(item: &PageItem, options: &HorizontalTextPath) -> f64 {
    let forward_width = (options.end - options.start).abs();
    let Some(source) = options.curve.as_ref()
        .filter(|curve| !curve.open)
        .and_then(PathMeasure::new) else { return forward_width };
    if !source.text_runs_backwards(options.flipped) || options.end <= options.start {
        return forward_width;
    }
    // The first anchor falls inside the backwards arc between ascending brackets.
    let aligned = stroke_baseline(item, options, &source);
    let length = aligned.as_ref().map_or_else(|| source.length(), |offset| offset.path.length());
    (length - forward_width).max(0.0)
}

pub(crate) fn place(item: &PageItem, line: &mut LaidLine, fonts: &FontStore) {
    let Some(options) = &item.text_path else {
        return;
    };
    let Some(curve) = &options.curve else { return };
    let Some(source) = PathMeasure::new(curve) else {
        return;
    };
    let sign = if source.text_runs_backwards(options.flipped) { -1.0 } else { 1.0 };
    let center = source.center();
    let aligned = stroke_baseline(item, options, &source);
    let baseline = aligned.as_ref().map_or(&source, |offset| &offset.path);
    let spaced = (options.spacing != 0.0)
        .then(|| baseline.offset(sign * options.spacing))
        .flatten();
    let walk = spaced.as_ref().unwrap_or(baseline);
    let map = |x: f64, y: f64, width: f64, vertical: f64| {
        let half = width / 2.0;
        let d = x + half;
        let at = if sign < 0.0 {
            (2.0 * options.start - d).rem_euclid(walk.length())
        } else {
            d
        };
        let (q, t) = walk.at(at);
        let t = Point::new(t.x * sign, t.y * sign);
        let p = Point::new(q.x - options.spacing * t.y, q.y + options.spacing * t.x);
        let mut transform = axes(options.effect, t, p, center, sign);
        let h = y - line.baseline + vertical;
        let origin = Point::new(
            p.x - half * transform.a + h * transform.c,
            p.y - half * transform.b + h * transform.d,
        );
        let straight = transform.apply(Point::new(x, y));
        transform.tx = origin.x - straight.x;
        transform.ty = origin.y - straight.y;
        (origin, transform)
    };

    let mut order: Vec<_> = (0..line.text_clusters.len()).collect();
    let left = |i: usize| {
        let c = &line.text_clusters[i];
        c.x0.min(c.x1)
    };
    order.sort_by(|&a, &b| left(a).total_cmp(&left(b)));
    let starts: Vec<_> = order.iter().map(|&i| left(i)).collect();
    let empty_y = line.glyphs.first().map_or(line.baseline, |g| g.y);
    let empty_height = line
        .glyphs
        .first()
        .map_or(0.0, |g| text_height(options.text_alignment, g, fonts));
    for g in &mut line.glyphs {
        let index = starts
            .partition_point(|&x| x <= g.x + 1e-8)
            .saturating_sub(1);
        let cluster = order.get(index).map(|&i| &mut line.text_clusters[i]);
        let width = cluster.as_ref().map_or(0.0, |c| (c.x1 - c.x0).abs());
        let (origin, m) = map(
            g.x,
            g.y,
            width,
            text_height(options.text_alignment, g, fonts),
        );
        if let Some(c) = cluster {
            c.path_map.get_or_insert(m);
        }
        g.x = origin.x;
        g.y = origin.y;
        let linear = Affine {
            tx: 0.0,
            ty: 0.0,
            ..m
        };
        g.path_transform = (linear != Affine::IDENTITY).then_some(linear);
    }
    for c in &mut line.text_clusters {
        c.path_map.get_or_insert_with(|| {
            map(c.x0.min(c.x1), empty_y, (c.x1 - c.x0).abs(), empty_height).1
        });
    }
}
