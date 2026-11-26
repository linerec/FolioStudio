//! Type ▸ Create Outlines on whole text frames (rules: next/oracle/rules/outlines.toml; oracle: outlines-*).
//!
//! The editor knows no composition, so the host fills `shapes` (see `folio-server` `outlines.rs`): per frame the
//! glyph paths of every line, in the frame's inner coordinates, grouped as InDesign groups them. InDesign
//! (measured) removes the frame and its story and puts in its place one Polygon (one line, one fill), or a Group of
//! Polygons (a Polygon per line and per run of one fill). A polygon's item transform is the frame's with its
//! top left corner moved to the origin; its points are relative to that corner.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct FrameShapes {
    frame: String,
    polys: Vec<Poly>,
}

#[derive(Deserialize)]
struct Poly {
    fill: String,
    tint: f64,
    stroke: String,
    weight: f64,
    /// Per subpath, per point: left x y, anchor x y, right x y.
    paths: Vec<Vec<[f64; 6]>>,
}

const WRAP: &str = r#"<TextWrapPreference Inverse="false" ApplyToMasterPageOnly="false" TextWrapSide="BothSides" TextWrapMode="None"><Properties><TextWrapOffset Top="0" Left="0" Bottom="0" Right="0" /></Properties></TextWrapPreference>"#;

/// A lone 4 point axis-aligned rectangle, which InDesign makes a Rectangle instead of a Polygon.
fn is_rect(p: &[Vec<[f64; 6]>]) -> bool {
    let [path] = p else { return false };
    if path.len() != 4 || path.iter().any(|q| q[0] != q[2] || q[1] != q[3] || q[4] != q[2] || q[5] != q[3]) {
        return false;
    }
    let pt = |k: usize| (path[k][2], path[k][3]);
    let h = |a: (f64, f64), b: (f64, f64)| a.1 == b.1 && a.0 != b.0;
    let v = |a: (f64, f64), b: (f64, f64)| a.0 == b.0 && a.1 != b.1;
    (h(pt(0), pt(1)) && v(pt(1), pt(2)) && h(pt(2), pt(3)) && v(pt(3), pt(0))) || (v(pt(0), pt(1)) && h(pt(1), pt(2)) && v(pt(2), pt(3)) && h(pt(3), pt(0)))
}

/// The attributes InDesign writes on every item it makes here (measured: outlines-*).
const BASE: &str = r#" ParentInterfaceChangeCount="" TargetInterfaceChangeCount="" LastUpdatedInterfaceChangeCount="" OverriddenPageItemProps="" HorizontalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" VerticalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension""#;
const GRADIENT: &str = r#" GradientFillStart="0 0" GradientFillLength="0" GradientFillAngle="0" GradientStrokeStart="0 0" GradientStrokeLength="0" GradientStrokeAngle="0""#;
const GRADIENT_HILITE: &str = r#" GradientFillHiliteLength="0" GradientFillHiliteAngle="0" GradientStrokeHiliteLength="0" GradientStrokeHiliteAngle="0""#;

/// Black (and only black) swatches overprint, as the text they come from did.
fn overprints(swatch: &str) -> bool {
    swatch == "Color/Black"
}

/// Fill, stroke and the rest of the paint attributes of a polygon or group (`stroke` only on polygons).
fn paint_attrs(fill: &str, tint: f64, stroke: Option<(&str, f64)>) -> (String, String) {
    let mut head = String::new();
    if !fill.is_empty() {
        head.push_str(&format!(" FillColor=\"{}\"", escape(fill)));
        if tint != 100.0 && tint >= 0.0 {
            head.push_str(&format!(" FillTint=\"{}\"", geom::num(tint)));
        }
        if overprints(fill) {
            head.push_str(" OverprintFill=\"true\"");
        }
    }
    head.push_str(" CornerRadius=\"0.167\"");
    let mut tail = String::new();
    match stroke {
        Some((color, weight)) if !color.is_empty() => {
            head.push_str(&format!(" StrokeWeight=\"{}\" StrokeColor=\"{}\"", geom::num(weight), escape(color)));
            if overprints(color) {
                tail.push_str(" OverprintStroke=\"true\"");
            }
        }
        _ => head.push_str(" StrokeTint=\"-1\""),
    }
    (head, tail)
}

fn polygon_xml(id: &str, layer: Option<&str>, m: &Mat, p: &Poly, tl: (f64, f64)) -> String {
    polygon_core(id, layer, &m.text(), p, tl, None)
}

/// `anchor` is the AnchoredObjectSetting element of an inline polygon (`Some`, maybe empty; `None` for a page item).
fn polygon_core(id: &str, layer: Option<&str>, transform: &str, p: &Poly, tl: (f64, f64), anchor: Option<&str>) -> String {
    let inline = anchor.is_some();
    let anchor = anchor.unwrap_or_default();
    let tag = if is_rect(&p.paths) { "Rectangle" } else { "Polygon" };
    let (head, tail) = paint_attrs(&p.fill, p.tint, Some((&p.stroke, p.weight)));
    let mut s = format!("<{tag} Self=\"{}\" ContentType=\"Unassigned\" StoryTitle=\"$ID/\"{BASE}{head}{GRADIENT}{tail}", escape(id));
    if let Some(layer) = layer {
        s.push_str(&format!(" ItemLayer=\"{}\" Locked=\"false\"", escape(layer)));
    } else if inline {
        s.push_str(" Locked=\"false\"");
    }
    s.push_str(&format!(" LocalDisplaySetting=\"Default\"{GRADIENT_HILITE} AppliedObjectStyle=\"ObjectStyle/$ID/[None]\" Visible=\"true\" Name=\"$ID/\" TopLeftCornerRadius=\"0.167\" ItemTransform=\"{transform}\"><Properties><PathGeometry>"));
    for path in &p.paths {
        s.push_str("<GeometryPathType PathOpen=\"false\"><PathPointArray>");
        for q in path {
            let at = |x: f64, y: f64| geom::pair((x - tl.0, y - tl.1));
            s.push_str(&format!("<PathPointType Anchor=\"{}\" LeftDirection=\"{}\" RightDirection=\"{}\"/>", at(q[2], q[3]), at(q[0], q[1]), at(q[4], q[5])));
        }
        s.push_str("</PathPointArray></GeometryPathType>");
    }
    s.push_str(&format!("</PathGeometry></Properties>{anchor}{WRAP}{}{}</{tag}>", create::INCOPY_SPACED, create::EXPORT));
    s
}

/// A Group of the polygons: the fill they all share is the group's too (as `group_fill` keeps it).
fn group_xml(id: &str, layer: &str, polys: &[Poly], members: &str) -> String {
    let first = &polys[0];
    let shared = polys.iter().all(|p| p.fill == first.fill && p.tint == first.tint);
    // the stroke the members all share is the group's too; with different strokes the group keeps none (and no
    // StrokeTint), but overprints when any member's black stroke does (measured: outlines-frames frame-strokes*)
    let stroked = |p: &Poly| !p.stroke.is_empty() && p.weight > 0.0;
    let shared_stroke = stroked(first) && polys.iter().all(|p| p.stroke == first.stroke && p.weight == first.weight);
    let (mut head, tail) = paint_attrs(if shared { &first.fill } else { "" }, first.tint, shared_stroke.then_some((first.stroke.as_str(), first.weight)));
    if !shared && polys.iter().any(|p| overprints(&p.fill)) {
        head = format!(" OverprintFill=\"true\"{head}");
    }
    if !shared_stroke && polys.iter().any(stroked) {
        head = head.replace(" StrokeTint=\"-1\"", "");
        if polys.iter().any(|p| stroked(p) && overprints(&p.stroke)) {
            head.push_str(" OverprintStroke=\"true\"");
        }
    }
    head.push_str(&tail);
    format!("<Group Self=\"{}\"{BASE}{head}{GRADIENT} ItemLayer=\"{}\" Locked=\"false\" LocalDisplaySetting=\"Default\"{GRADIENT_HILITE} AppliedObjectStyle=\"ObjectStyle/$ID/[None]\" Visible=\"true\" Name=\"$ID/\" TopLeftCornerRadius=\"0.167\" ItemTransform=\"1 0 0 1 0 0\">{WRAP}{}{members}</Group>",
        escape(id), escape(layer), create::EXPORT)
}

pub(crate) fn create(w: &mut Work<'_>, ids: &[String], shapes: &str) -> Result<(), EditError> {
    create_with(w, ids, shapes, false)
}

/// Create Outlines without Deleting Text (Ctrl+Alt+Shift+O, tools-c2): the outlines go in front of the frames (just after each frame in its spread)
/// and the text frames stay under them; one edit, so one Undo takes it all back. `created` = the outlines (the screen selects them).
pub(crate) fn create_keep(w: &mut Work<'_>, ids: &[String], shapes: &str) -> Result<(), EditError> {
    create_with(w, ids, shapes, true)
}

fn create_with(w: &mut Work<'_>, ids: &[String], shapes: &str, keep: bool) -> Result<(), EditError> {
    if ids.is_empty() {
        return Err(EditError::new("invalid", "select a text frame"));
    }
    let shapes: Vec<FrameShapes> = serde_json::from_str(shapes).map_err(|e| EditError::new("invalid", format!("the glyph shapes are filled in by the engine host: {e}")))?;
    // a threaded frame brings the frames of its whole story (the host lists them all)
    if ids.iter().any(|id| !shapes.iter().any(|s| &s.frame == id)) {
        return Err(EditError::new("invalid", "the glyph shapes do not match the selected frames"));
    }
    for sh in &shapes {
        let (part, ix, i) = w.find(&sh.frame)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, &sh.frame)?;
        if ix.local(&t, i) != "TextFrame" {
            return Err(EditError::new("unsupported", format!("{} is not a text frame", sh.frame)));
        }
        super::textpath::unlocked(w, &ix, &t, i)?;
        if sh.polys.is_empty() || sh.polys.iter().any(|p| p.paths.is_empty()) {
            return Err(EditError::new("invalid", "the frame has no text to turn into outlines"));
        }
        let own = own_path(&ix, &t, i).ok_or_else(|| EditError::new("xml", "text frame without a path"))?;
        let anchors: Vec<(f64, f64)> = own.subpaths.iter().flat_map(|s| s.1.iter().map(|p| p.anchor)).collect();
        if anchors.is_empty() {
            return Err(EditError::new("xml", "text frame without a path"));
        }
        let tl = (anchors.iter().map(|a| a.0).fold(f64::INFINITY, f64::min), anchors.iter().map(|a| a.1).fold(f64::INFINITY, f64::min));
        let frame = Mat::parse(ix.attr(&t, i, "ItemTransform").unwrap_or("1 0 0 1 0 0")).unwrap_or(Mat::ID);
        let [a, b, c, d, e, f] = frame.0;
        let m = Mat([a, b, c, d, e + a * tl.0 + c * tl.1, f + b * tl.0 + d * tl.1]);
        let layer = ix.attr(&t, i, "ItemLayer").unwrap_or_default().to_owned();
        let span = element_span(&ix, &t, i);
        let at = if keep { span.end } else { span.start };
        let mut xml = String::new();
        let mut made = Vec::new();
        if sh.polys.len() > 1 {
            let gid = w.new_id();
            made.push(gid.clone());
            let mut members = String::new();
            // IDML lists a group's members back to front: the last line (run) first
            for p in sh.polys.iter().rev() {
                let pid = w.new_id();
                members.push_str(&polygon_xml(&pid, None, &m, p, tl));
                made.push(pid);
            }
            xml.push_str(&group_xml(&gid, &layer, &sh.polys, &members));
        } else {
            let pid = w.new_id();
            xml.push_str(&polygon_xml(&pid, Some(&layer), &m, &sh.polys[0], tl));
            made.push(pid);
        }
        // the shapes go in first (just before the frame); deleting the frame then leaves them in its place
        w.splice(&part, vec![(at..at, format!("\n\t\t{xml}"))])?;
        // keeping the text, the shapes went in just after the frame: in front of it
        if !keep { delete(w, &sh.frame)?; }
        w.created.extend(made);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Create Outlines on a text range (rules: next/oracle/rules/outlines-c2.ko.md; oracle: outlines-range-*).

#[derive(Deserialize)]
struct RangeShapes {
    story: String,
    runs: Vec<RunShape>,
}

#[derive(Deserialize)]
struct RunShape {
    frame: String,
    start: usize,
    end: usize,
    poly: Poly,
    baseline: f64,
    leading: f64,
}

/// The extent (left, top, right, bottom) of closed cubic paths, curve extrema included.
fn path_bounds(paths: &[Vec<[f64; 6]>]) -> Option<(f64, f64, f64, f64)> {
    let mut b: Option<(f64, f64, f64, f64)> = None;
    let mut add = |x: f64, y: f64| {
        b = Some(match b {
            None => (x, y, x, y),
            Some((l, t, r, bo)) => (l.min(x), t.min(y), r.max(x), bo.max(y)),
        });
    };
    for path in paths {
        for (k, q) in path.iter().enumerate() {
            add(q[2], q[3]);
            let n = &path[(k + 1) % path.len()];
            let p = [[q[2], q[3]], [q[4], q[5]], [n[0], n[1]], [n[2], n[3]]];
            for axis in 0..2 {
                let (p0, p1, p2, p3) = (p[0][axis], p[1][axis], p[2][axis], p[3][axis]);
                // B'(t)/3 = a t^2 + 2 b t + c
                let (a, bb, c) = (p3 - 3.0 * p2 + 3.0 * p1 - p0, p2 - 2.0 * p1 + p0, p1 - p0);
                let mut roots = Vec::new();
                if a.abs() < 1e-12 {
                    if bb.abs() > 1e-12 {
                        roots.push(-c / (2.0 * bb));
                    }
                } else {
                    let d = bb * bb - a * c;
                    if d >= 0.0 {
                        roots.push((-bb + d.sqrt()) / a);
                        roots.push((-bb - d.sqrt()) / a);
                    }
                }
                for t in roots.into_iter().filter(|t| *t > 0.0 && *t < 1.0) {
                    let u = 1.0 - t;
                    let at = |i: usize| u * u * u * p[0][i] + 3.0 * u * u * t * p[1][i] + 3.0 * u * t * t * p[2][i] + t * t * t * p[3][i];
                    add(at(0), at(1));
                }
            }
        }
    }
    b
}

/// The extent (left, top, right, bottom) of the closed paths stroked `weight` wide with mitre joins (limit 4, InDesign's
/// default): the stroke's outer edge, a corner's mitre tip included. This, not the bare path, is what an inline polygon's
/// `ItemTransform` puts on the insertion point and the baseline (outlines-c3: 33 cases of
/// `artifacts/outlines-c3-20261023/measure/idml-s` equal to 0.002 pt; the rule is `folio_model::strokebounds`).
fn stroke_bounds(paths: &[Vec<[f64; 6]>], weight: f64) -> Option<(f64, f64, f64, f64)> {
    if weight <= 0.0 {
        return path_bounds(paths);
    }
    let sub: Vec<folio_model::SubPath> = paths
        .iter()
        .map(|path| folio_model::SubPath {
            points: path
                .iter()
                .map(|q| folio_model::PathPoint { anchor: folio_model::Point::new(q[2], q[3]), left: folio_model::Point::new(q[0], q[1]), right: folio_model::Point::new(q[4], q[5]) })
                .collect(),
            open: false,
        })
        .collect();
    folio_model::strokebounds::stroke_bounds(&sub, weight, 4.0).map(|r| (r.left, r.top, r.right, r.bottom))
}

pub(crate) fn create_range(w: &mut Work<'_>, story: &str, start: &usize, end: &usize, shapes: &str) -> Result<(), EditError> {
    let (start, end) = (*start, *end);
    let shapes: RangeShapes = serde_json::from_str(shapes).map_err(|e| EditError::new("invalid", format!("the glyph shapes are filled in by the engine host: {e}")))?;
    let story_id = crate::text::story_of(w, story)?;
    if shapes.story != story_id || shapes.runs.is_empty() || shapes.runs.iter().any(|r| r.start < start || r.end > end || r.start >= r.end) {
        return Err(EditError::new("invalid", "the glyph shapes do not match the selected text"));
    }
    // later runs first: replacing a run by one character moves only what follows it
    let mut made: Vec<String> = Vec::new();
    for run in shapes.runs.iter().rev() {
        let (part, ix, i) = w.find(&run.frame)?;
        let t = w.text(&part)?.to_owned();
        let own = own_path(&ix, &t, i).ok_or_else(|| EditError::new("xml", "text frame without a path"))?;
        let anchors: Vec<(f64, f64)> = own.subpaths.iter().flat_map(|s| s.1.iter().map(|p| p.anchor)).collect();
        if anchors.is_empty() || run.poly.paths.is_empty() {
            return Err(EditError::new("xml", "text frame without a path"));
        }
        let tl = (anchors.iter().map(|a| a.0).fold(f64::INFINITY, f64::min), anchors.iter().map(|a| a.1).fold(f64::INFINITY, f64::min));
        let rel: Vec<Vec<[f64; 6]>> = run.poly.paths.iter().map(|p| p.iter().map(|q| [q[0] - tl.0, q[1] - tl.1, q[2] - tl.0, q[3] - tl.1, q[4] - tl.0, q[5] - tl.1]).collect()).collect();
        let (min_x, min_y, _, max_y) = path_bounds(&rel).ok_or_else(|| EditError::new("invalid", "empty outline"))?;
        // the object sits on the baseline by its bottom edge: AnchorYoffset is how far that edge is above it; an
        // object taller than the leading is lifted by the excess (BaselineShift, measured: outlines-c2 f-*)
        let baseline = run.baseline - tl.1;
        let shift = ((baseline - min_y) - run.leading).max(0.0);
        let yo = baseline - max_y - shift;
        let anchor = if yo.abs() < 1e-4 { String::new() } else { format!("<AnchoredObjectSetting AnchorYoffset=\"{}\" />", geom::num(yo)) };
        // a stroked polygon is put by the outer edge of its stroke (outlines-c3), the object's height and place by the bare path
        let (vis_x, _, _, vis_y) = if run.poly.stroke.is_empty() { (min_x, min_y, 0.0, max_y) } else { stroke_bounds(&rel, run.poly.weight).ok_or_else(|| EditError::new("invalid", "empty outline"))? };
        let transform = Mat([1.0, 0.0, 0.0, 1.0, -vis_x, -vis_y]).text();
        let id = w.new_id();
        let poly = Poly { fill: run.poly.fill.clone(), tint: run.poly.tint, stroke: run.poly.stroke.clone(), weight: run.poly.weight, paths: run.poly.paths.clone() };
        let xml = polygon_core(&id, None, &transform, &poly, tl, Some(&anchor));
        crate::text::replace_with_object(w, &story_id, run.start, run.end, &xml, run.leading, shift)?;
        made.push(id);
    }
    made.reverse();
    w.created.extend(made);
    w.recompose(&story_id);
    Ok(())
}
