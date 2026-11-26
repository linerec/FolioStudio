//! Read and fit the graphic inside a page item.

use super::*;

/// `FrameFittingOption` crops: how far each edge of the frame lies inside the graphic, in
/// the graphic's own space (negative: the frame reaches beyond it).
pub(crate) fn fitting_crops(ix: &Index, t: &str, frame: usize, frame_box: [f64; 4]) -> Vec<(std::ops::Range<usize>, String)> {
    let Some(&fit) = ix.named(t, frame, "FrameFittingOption").first() else { return Vec::new() };
    // the content: a graphic (its GraphicBounds) or a page item pasted into the frame (its path
    // bounds, clip-paste-into)
    let bounds = |g: usize| -> Option<[f64; 4]> {
        if GRAPHICS.contains(&ix.local(t, g)) {
            let gb = ix.path(t, g, &["Properties", "GraphicBounds"])?;
            let num = |k| ix.attr(t, gb, k).and_then(|v| v.parse::<f64>().ok());
            Some([num("Left")?, num("Top")?, num("Right")?, num("Bottom")?])
        } else if is_item(ix, t, g) {
            own_bounds(ix, t, g, &Mat::ID)
        } else {
            None
        }
    };
    let Some((g, b)) = ix.children(frame).iter().find_map(|&c| bounds(c).map(|b| (c, b))) else { return Vec::new() };
    let Some(crops) = frame_crops(frame_box, &item_transform(ix, t, g), b) else { return Vec::new() };
    ["LeftCrop", "TopCrop", "RightCrop", "BottomCrop"].iter().zip(crops).map(|(k, v)| attr_edit(ix, t, fit, k, &geom::num(v))).collect()
}

/// A frame's crops (left, top, right, bottom) of a graphic with transform `m` and bounds `g`
/// (its own space) in a frame box `b`: measured in the graphic's space, each given to the frame
/// side its graphic side faces — a graphic turned a quarter turn crops its bottom at the frame's
/// left (basic-place-formats: place-jpeg-exif6, LeftCrop -30 TopCrop 30 RightCrop -30 BottomCrop 30).
/// A convex polygon cut by the line `p[axis] = at`; `keep_above` keeps the side with larger values.
fn clip_axis(poly: &[(f64, f64)], axis: usize, at: f64, keep_above: bool) -> Vec<(f64, f64)> {
    let v = |p: &(f64, f64)| if axis == 0 { p.0 } else { p.1 };
    let inside = |p: &(f64, f64)| if keep_above { v(p) >= at - 1e-9 } else { v(p) <= at + 1e-9 };
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        if inside(&p) {
            out.push(p);
        }
        if inside(&p) != inside(&q) {
            let t = (at - v(&p)) / (v(&q) - v(&p));
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
        }
    }
    out
}

fn frame_crops(b: [f64; 4], m: &Mat, g: [f64; 4]) -> Option<[f64; 4]> {
    let inv = m.invert()?;
    let corners = [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])].map(|p| inv.apply(p));
    // a frame that leans in the graphic's space (a sheared graphic): its edges are read inside the graphic's own band, as InDesign
    // does (oracle: ctl2-shear-content-*), the left and right edges between the graphic's top and bottom, the top and bottom
    // between its left and right. An upright frame needs no clipping.
    let poly = vec![corners[0], corners[1], corners[3], corners[2]];
    let leans = (0..4).any(|i| {
        let (p, q) = (poly[i], poly[(i + 1) % 4]);
        (p.0 - q.0).abs() > 1e-9 && (p.1 - q.1).abs() > 1e-9
    });
    let span = |axis: usize, band: usize| -> (f64, f64) {
        let mut pts = poly.clone();
        if leans {
            pts = clip_axis(&pts, band, g[band], true);
            pts = clip_axis(&pts, band, g[band + 2], false);
        }
        let pts = if pts.is_empty() { poly.clone() } else { pts };
        let v = |p: &(f64, f64)| if axis == 0 { p.0 } else { p.1 };
        (pts.iter().map(v).fold(f64::INFINITY, f64::min), pts.iter().map(v).fold(f64::NEG_INFINITY, f64::max))
    };
    let (x0, x1) = span(0, 1);
    let (y0, y1) = span(1, 0);
    let crops = [x0 - g[0], y0 - g[1], g[2] - x1, g[3] - y1];
    let [a, bb, c, d, ..] = m.0;
    // outward directions of the graphic's sides in the frame: left, top, right, bottom
    let sides = [(-a, -bb), (-c, -d), (a, bb), (c, d)];
    let facing = |dir: (f64, f64)| {
        sides.iter().position(|s| {
            let len = s.0.hypot(s.1);
            len > 0.0 && (s.0 / len - dir.0).abs() < 1e-6 && (s.1 / len - dir.1).abs() < 1e-6
        })
    };
    Some(match [(-1.0, 0.0), (0.0, -1.0), (1.0, 0.0), (0.0, 1.0)].map(facing) {
        [Some(l), Some(t), Some(r), Some(bt)] => [crops[l], crops[t], crops[r], crops[bt]],
        _ => crops,
    })
}

pub(super) struct Content {
    pub(super) frame: usize,
    pub(super) graphic: usize,
    /// graphic → frame inner
    pub(super) m: Mat,
    /// GraphicBounds in the graphic's space
    pub(super) bounds: [f64; 4],
}

pub(super) fn content_of(ix: &Index, t: &str, frame: usize) -> Option<Content> {
    let g = *ix.children(frame).iter().find(|&&c| GRAPHICS.contains(&ix.local(t, c)))?;
    let gb = ix.path(t, g, &["Properties", "GraphicBounds"])?;
    let num = |k| ix.attr(t, gb, k).and_then(|v| v.parse::<f64>().ok());
    Some(Content { frame, graphic: g, m: item_transform(ix, t, g), bounds: [num("Left")?, num("Top")?, num("Right")?, num("Bottom")?] })
}

/// The graphic moves in its frame; the fitting crops follow (oracle: content-move).
pub(super) fn move_content(w: &mut Work<'_>, id: &str, by: (f64, f64)) -> Result<(), EditError> {
    let (part, ix, f) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let c = content_of(&ix, &t, f).ok_or_else(|| EditError::new("invalid", format!("{id} holds no placed graphic")))?;
    let to_frame = parent_transform(&ix, &t, f).then_after(&item_transform(&ix, &t, f));
    let v = to_frame.unapply_vector(by).ok_or_else(|| EditError::new("unsupported", format!("{id} cannot be inverted")))?;
    let mut m = c.m;
    m.0[4] += v.0;
    m.0[5] += v.1;
    write_content(w, &part, &ix, &t, &c, m, None, true)
}

pub(super) fn frame_box(ix: &Index, t: &str, frame: usize) -> Option<[f64; 4]> {
    let path = own_path(ix, t, frame)?;
    let shapes: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, p, c)| (p.clone(), *c)).collect();
    geom::path_bounds(&shapes)
}

/// Object ▸ Fitting (oracle: fit-*). The three that scale the graphic set the crops to 0 and
/// keep their kind in `FittingOnEmptyFrame`; centring keeps the scale; Frame to Content sets
/// the frame's path to the graphic's box. The graphic's `EffectivePpi` follows its scale.
pub(super) fn fit(w: &mut Work<'_>, id: &str, how: Fit) -> Result<(), EditError> {
    let (part, ix, f) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let c = content_of(&ix, &t, f).ok_or_else(|| EditError::new("invalid", format!("{id} holds no placed graphic")))?;
    let [a, b, cc, d, _, _] = c.m.0;
    if b.abs() > 1e-9 || cc.abs() > 1e-9 || a <= 0.0 || d <= 0.0 {
        return Err(EditError::new("unsupported", "fitting rotated, sheared or flipped content is not built yet"));
    }
    let [fx0, fy0, fx1, fy1] = frame_box(&ix, &t, f).ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    let [gx0, gy0, gx1, gy1] = c.bounds;
    let (fw, fh, gw, gh) = (fx1 - fx0, fy1 - fy0, gx1 - gx0, gy1 - gy0);
    if gw <= 0.0 || gh <= 0.0 {
        return Err(EditError::new("unsupported", "the graphic has no size"));
    }
    if how == Fit::FrameToContent {
        let p = [c.m.apply((gx0, gy0)), c.m.apply((gx1, gy1))];
        let rect = [p[0].0.min(p[1].0), p[0].1.min(p[1].1), p[0].0.max(p[1].0), p[0].1.max(p[1].1)];
        resize(w, id, Target::Box(rect))?;
        let (part, ix, f) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        let Some(fit) = ix.named(&t, f, "FrameFittingOption").first().copied() else { return Ok(()) };
        let edits = ["LeftCrop", "TopCrop", "RightCrop", "BottomCrop"].iter().map(|k| attr_edit(&ix, &t, fit, k, "0")).collect();
        return w.splice(&part, edits);
    }
    let anchor = ix
        .named(&t, f, "FrameFittingOption")
        .first()
        .and_then(|&o| ix.attr(&t, o, "FittingAlignment"))
        .map(fitting_anchor)
        .unwrap_or((0.5, 0.5));
    let (sx, sy) = match how {
        Fit::Proportionally => {
            let s = (fw / gw).min(fh / gh);
            (s, s)
        }
        Fit::Fill => {
            let s = (fw / gw).max(fh / gh);
            (s, s)
        }
        Fit::ContentToFrame => (fw / gw, fh / gh),
        _ => (a, d),
    };
    // the graphic's box placed at the alignment point of the frame
    let tx = fx0 + (fw - gw * sx) * anchor.0 - gx0 * sx;
    let ty = fy0 + (fh - gh * sy) * anchor.1 - gy0 * sy;
    let m = Mat([sx, 0.0, 0.0, sy, tx, ty]);
    let mode = match how {
        Fit::Proportionally => Some("Proportionally"),
        Fit::Fill => Some("FillProportionally"),
        Fit::ContentToFrame => Some("ContentToFrame"),
        _ => None,
    };
    write_content(w, &part, &ix, &t, &c, m, mode, how == Fit::Center)
}

pub(super) fn fitting_anchor(a: &str) -> (f64, f64) {
    match a {
        "TopLeftAnchor" => (0.0, 0.0),
        "TopCenterAnchor" => (0.5, 0.0),
        "TopRightAnchor" => (1.0, 0.0),
        "LeftCenterAnchor" => (0.0, 0.5),
        "RightCenterAnchor" => (1.0, 0.5),
        "BottomLeftAnchor" => (0.0, 1.0),
        "BottomCenterAnchor" => (0.5, 1.0),
        "BottomRightAnchor" => (1.0, 1.0),
        _ => (0.5, 0.5),
    }
}

/// Writes the graphic's new transform, its effective resolution, and the frame's fitting:
/// crops measured (`measure`) or 0, and the fitting kind when given.
// Keep the content transform, fitting mode and measurement inputs separate at this XML writer.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_content(w: &mut Work<'_>, part: &str, ix: &Index, t: &str, c: &Content, m: Mat, mode: Option<&str>, measure: bool) -> Result<(), EditError> {
    let mut edits = vec![attr_edit(ix, t, c.graphic, "ItemTransform", &m.text())];
    if let Some(ppi) = ix.attr(t, c.graphic, "ActualPpi").and_then(geom::parse_pair) {
        // the scale along y without the shear: the area over the x scale (oracle: ctl2-shear-content-*)
        let sx = (m.0[0] * m.0[0] + m.0[1] * m.0[1]).sqrt();
        let sy = if sx > 0.0 { (m.0[0] * m.0[3] - m.0[1] * m.0[2]).abs() / sx } else { 0.0 };
        if sx > 0.0 && sy > 0.0 {
            edits.push(attr_edit(ix, t, c.graphic, "EffectivePpi", &format!("{} {}", (ppi.0 / sx).round(), (ppi.1 / sy).round())));
        }
    }
    if let Some(&fit) = ix.named(t, c.frame, "FrameFittingOption").first() {
        if measure {
            if let Some(b) = frame_box(ix, t, c.frame) {
                let crops = frame_crops(b, &m, c.bounds).ok_or_else(|| EditError::new("unsupported", "content cannot be inverted"))?;
                for (k, v) in ["LeftCrop", "TopCrop", "RightCrop", "BottomCrop"].iter().zip(crops) {
                    edits.push(attr_edit(ix, t, fit, k, &geom::num(v)));
                }
            }
        } else {
            for k in ["LeftCrop", "TopCrop", "RightCrop", "BottomCrop"] {
                if ix.attr(t, c.frame, "AppliedObjectStyle") == Some("ObjectStyle/$ID/[None]") {
                    if let Some(e) = attr_removal(ix, t, fit, k) { edits.push(e); }
                } else {
                    edits.push(attr_edit(ix, t, fit, k, "0"));
                }
            }
        }
        if let Some(mode) = mode {
            edits.push(attr_edit(ix, t, fit, "FittingOnEmptyFrame", mode));
        }
    }
    w.splice(part, edits)
}
