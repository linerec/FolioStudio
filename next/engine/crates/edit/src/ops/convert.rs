//! Object ▸ Convert Shape and Object ▸ Paths (oracle basic-objects cs-*, path-*). InDesign keeps
//! the object (its element, transform and settings) and gives it a new path that fills the box
//! the object showed before, stroke included: the new path is fitted until its stroked bounds
//! (sides, mitred joins, caps, as `visibleBounds`) are the old ones. A 1 pt stroke so lowers a
//! triangle's apex by 0.302 pt, where the miter reaches 0.5 / sin 38.56° above it (cs-triangle),
//! and draws a line's ends inside the corners by its sloped half width (cs-line). The rounded,
//! bevelled and inverse rounded rectangles only change the corner options; opening a path
//! repeats its first point at the end and sets the corner radii to 0 (path-open).
use super::alignment::{Stroke, stroked};
use super::*;

/// What Convert Shape makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shape {
    Rectangle,
    RoundedRectangle,
    BeveledRectangle,
    InverseRoundedRectangle,
    Ellipse,
    Triangle,
    Polygon,
    Line,
    /// Object ▸ Convert Shape ▸ Orthogonal Line: horizontal, through the middle.
    StraightLine,
    /// Object ▸ Paths ▸ Open Path / Close Path.
    OpenPath,
    ClosedPath,
}

pub(super) fn convert_shape(w: &mut Work<'_>, ids: &[String], to: Shape) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        if !matches!(ix.local(&t, i), "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "TextFrame") {
            return Err(EditError::new("unsupported", format!("{id}: converting its shape is not built")));
        }
        let corner = |o: &str| ["CornerOption", "TopLeftCornerOption", "TopRightCornerOption", "BottomLeftCornerOption", "BottomRightCornerOption"]
            .map(|k| (k, o.to_owned()));
        match to {
            Shape::RoundedRectangle | Shape::BeveledRectangle | Shape::InverseRoundedRectangle => {
                // the path stays; the corners change (cs-rounded, cs-beveled, cs-inverse-rounded)
                set_rectangle(w, &part, &ix, &t, i, false)?;
                let (part, ix, i) = w.find(id)?;
                let t = w.text(&part)?.to_owned();
                let kind = match to { Shape::RoundedRectangle => "RoundedCorner", Shape::BeveledRectangle => "BevelCorner", _ => "InverseRoundedCorner" };
                let edits = corner(kind).iter().map(|(k, v)| attr_edit(&ix, &t, i, k, v)).collect();
                w.splice(&part, edits)?;
            }
            Shape::OpenPath | Shape::ClosedPath => open_close(w, &part, &ix, &t, i, to == Shape::OpenPath)?,
            _ => {
                let path = own_path(&ix, &t, i).ok_or_else(|| EditError::new("xml", format!("{id} has no path")))?;
                let pts: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, p, c)| (p.clone(), *c)).collect();
                let stroke = Stroke::of(w, &ix, &t, i);
                let shown = shown(&pts, &stroke).ok_or_else(|| EditError::new("unsupported", format!("{id} has an empty path")))?;
                let sides = polygon_sides(w)?;
                let (points, open) = fit(to, shown, sides, &stroke);
                write_path(w, &part, &ix, &t, i, &points, open)?;
                if to != Shape::Rectangle {
                    one_inset(w, &part, i)?;
                }
            }
        }
        if let Some(story) = frame_story(w, id) {
            w.recompose(&story);
        }
    }
    Ok(())
}

/// Object ▸ Paths ▸ Reverse Path: each path's points in reverse order, their directions swapped
/// (path-reverse, path-reverse-rect).
pub(super) fn reverse_path(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let path = own_path(&ix, &t, i).ok_or_else(|| EditError::new("xml", format!("{id} has no path")))?;
        let mut edits = Vec::new();
        for (els, pts, _) in &path.subpaths {
            for (k, &e) in els.iter().enumerate() {
                let p = &pts[pts.len() - 1 - k];
                edits.push((ix.nodes[e].start..ix.nodes[e].end, point_xml(&PathPoint { anchor: p.anchor, left: p.right, right: p.left })));
            }
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}

/// The bounds a path shows with its stroke (the item's own space).
fn shown(paths: &[(Vec<PathPoint>, bool)], stroke: &Stroke) -> Option<[f64; 4]> {
    let b = geom::path_bounds(paths)?;
    stroked(b, paths.iter().map(|(p, c)| (p.as_slice(), *c)), &Mat::ID, stroke)
}

/// The new path for `to` whose stroked bounds are `target`: its box starts inside by the stroke
/// and each side moves by what still differs, until nothing does (a few rounds; the miters
/// change with the box's proportions).
fn fit(to: Shape, target: [f64; 4], sides: u32, stroke: &Stroke) -> (Vec<PathPoint>, bool) {
    let o = stroke.outset;
    let mut b = [target[0] + o, target[1] + o, target[2] - o, target[3] - o];
    let make = |b: [f64; 4]| -> (Vec<PathPoint>, bool) {
        let [x0, y0, x1, y1] = b;
        let p = |x: f64, y: f64| PathPoint { anchor: (x, y), left: (x, y), right: (x, y) };
        match to {
            Shape::Ellipse => (create::shape_path(create::ItemKind::Ellipse, b, 4), false),
            Shape::Triangle => (create::shape_path(create::ItemKind::Polygon, b, 3), false),
            Shape::Polygon => (create::shape_path(create::ItemKind::Polygon, b, sides), false),
            Shape::Line => (vec![p(x0, y0), p(x1, y1)], true),
            Shape::StraightLine => {
                let y = (target[1] + target[3]) / 2.0;
                (vec![p(x0, y), p(x1, y)], true)
            }
            _ => (create::shape_path(create::ItemKind::Rectangle, b, 4), false),
        }
    };
    for _ in 0..64 {
        let (pts, open) = make(b);
        let Some(s) = shown(&[(pts, !open)], stroke) else { break };
        let d = [target[0] - s[0], target[1] - s[1], target[2] - s[2], target[3] - s[3]];
        // an orthogonal line keeps its height (its stroke's)
        let d = if to == Shape::StraightLine { [d[0], 0.0, d[2], 0.0] } else { d };
        if d.iter().all(|v| v.abs() < 1e-13) {
            break;
        }
        for k in 0..4 {
            b[k] += d[k];
        }
    }
    make(b)
}

/// The number of sides the document's polygon settings give (star polygons are not built).
fn polygon_sides(w: &mut Work<'_>) -> Result<u32, EditError> {
    let t = w.text("Resources/Preferences.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let p = (0..ix.nodes.len()).find(|&k| ix.local(t, k) == "PolygonPreference");
    let get = |k: &str| p.and_then(|p| ix.attr(t, p, k)).and_then(|v| v.parse::<f64>().ok());
    if get("InsetPercentage").is_some_and(|v| v != 0.0) {
        return Err(EditError::new("unsupported", "star polygons (a star inset) are not built yet"));
    }
    Ok(get("NumberOfSides").map_or(6, |v| v as u32).clamp(3, 100))
}

pub(super) fn point_xml(p: &PathPoint) -> String {
    let pair = |(x, y): (f64, f64)| format!("{} {}", geom::num(x), geom::num(y));
    format!("<PathPointType Anchor=\"{}\" LeftDirection=\"{}\" RightDirection=\"{}\" />", pair(p.anchor), pair(p.left), pair(p.right))
}

/// The item's path becomes one path of `points`.
fn write_path(w: &mut Work<'_>, part: &str, ix: &Index, t: &str, i: usize, points: &[PathPoint], open: bool) -> Result<(), EditError> {
    let geo = ix.path(t, i, &["Properties", "PathGeometry"]).ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let n = &ix.nodes[geo];
    let ind = indent_before(t, n.start);
    let mut s = format!("\n{ind}\t<GeometryPathType PathOpen=\"{open}\">\n{ind}\t\t<PathPointArray>");
    for p in points {
        s.push_str(&format!("\n{ind}\t\t\t{}", point_xml(p)));
    }
    s.push_str(&format!("\n{ind}\t\t</PathPointArray>\n{ind}\t</GeometryPathType>\n{ind}"));
    w.splice(part, vec![(n.open_end..n.close_start, s)])
}

/// The rounded, bevelled and inverse rounded rectangles are rectangles first.
fn set_rectangle(w: &mut Work<'_>, part: &str, ix: &Index, t: &str, i: usize, _: bool) -> Result<(), EditError> {
    let path = own_path(ix, t, i).ok_or_else(|| EditError::new("xml", "no path"))?;
    let rect = path.subpaths.len() == 1 && path.subpaths[0].1.len() == 4 && path.subpaths[0].2
        && path.subpaths[0].1.iter().all(|p| p.left == p.anchor && p.right == p.anchor)
        && { let p = &path.subpaths[0].1; p[0].anchor.0 == p[1].anchor.0 && p[1].anchor.1 == p[2].anchor.1 && p[2].anchor.0 == p[3].anchor.0 && p[3].anchor.1 == p[0].anchor.1 };
    if rect {
        return Ok(());
    }
    let pts: Vec<(Vec<PathPoint>, bool)> = path.subpaths.iter().map(|(_, p, c)| (p.clone(), *c)).collect();
    let stroke = Stroke::of(w, ix, t, i);
    let shown = shown(&pts, &stroke).ok_or_else(|| EditError::new("unsupported", "an empty path"))?;
    let (points, open) = fit(Shape::Rectangle, shown, 4, &stroke);
    write_path(w, part, ix, t, i, &points, open)
}

/// Paths ▸ Open Path: the first point again at the end, the path open, corner radii 0;
/// Close Path: that point taken off again, the path closed (path-open, path-close).
fn open_close(w: &mut Work<'_>, part: &str, ix: &Index, t: &str, i: usize, open: bool) -> Result<(), EditError> {
    let path = own_path(ix, t, i).ok_or_else(|| EditError::new("xml", "no path"))?;
    let [(_, pts, closed)] = path.subpaths.as_slice() else {
        return Err(EditError::new("unsupported", "opening or closing a compound path is not built"));
    };
    if open != *closed {
        return Ok(());
    }
    let mut pts = pts.clone();
    if open {
        let first = pts[0];
        pts.push(first);
    } else if pts.len() > 2 && pts.first().map(|p| p.anchor) == pts.last().map(|p| p.anchor) {
        pts.pop();
    }
    write_path(w, part, ix, t, i, &pts, open)?;
    if open {
        let (part, ix, i) = (part.to_owned(), w.index(part)?, i);
        let t = w.text(&part)?.to_owned();
        let edits = ["CornerRadius", "TopLeftCornerRadius", "TopRightCornerRadius", "BottomLeftCornerRadius", "BottomRightCornerRadius"]
            .iter().map(|k| attr_edit(&ix, &t, i, k, "0")).collect();
        w.splice(&part, edits)?;
    }
    Ok(())
}

/// A text frame that is no rectangle has one inset, all round: its four become the top one
/// (cs-text-oval).
pub(super) fn one_inset(w: &mut Work<'_>, part: &str, i: usize) -> Result<(), EditError> {
    let ix = w.index(part)?;
    let t = w.text(part)?.to_owned();
    if ix.local(&t, i) != "TextFrame" {
        return Ok(());
    }
    let Some(s) = ix.named(&t, i, "TextFramePreference").first().and_then(|&p| ix.path(&t, p, &["Properties", "InsetSpacing"])) else { return Ok(()) };
    if ix.attr(&t, s, "type") != Some("list") {
        return Ok(());
    }
    let top = ix.named(&t, s, "ListItem").first().map(|&k| { let n = &ix.nodes[k]; t[n.open_end..n.close_start].trim().to_owned() }).unwrap_or_else(|| "0".into());
    let n = &ix.nodes[s];
    w.splice(part, vec![(n.start..n.end, format!("<InsetSpacing type=\"unit\">{top}</InsetSpacing>"))])
}

/// The story of a text frame.
fn frame_story(w: &mut Work<'_>, id: &str) -> Option<String> {
    let (part, ix, i) = w.find(id).ok()?;
    let t = w.text(&part).ok()?;
    (ix.local(t, i) == "TextFrame").then(|| ix.attr(t, i, "ParentStory")).flatten().map(str::to_owned)
}
