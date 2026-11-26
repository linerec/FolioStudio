mod bezier;
mod chain;
mod edit_curve;
mod fit;
mod join;
mod modify;
mod samples;
mod selected;

pub(crate) use modify::{erase, smooth};
pub(crate) use selected::{extend, stroke};

use super::{EditError, Index, Mat, PathPoint, Work, create, geom, item_transform, pathedit};
use bezier::Point;

fn invalid() -> EditError {
    EditError::new(
        "invalid",
        "The pencil needs finite coordinates, valid settings and a drawable path.",
    )
}
fn unsupported() -> EditError {
    EditError::new("unsupported", "This edit requires a path with at least two anchors.")
}

fn positive(value: f64) -> Result<f64, EditError> {
    if value.is_finite() && value > 0. {
        Ok(value)
    } else {
        Err(invalid())
    }
}

fn tolerance(pixels: f64, scale: f64) -> Result<f64, EditError> {
    positive(positive(pixels)? * (25.4 / 72.) / positive(scale)?)
}

fn check_smoothness(value: f64) -> Result<(), EditError> {
    if (0. ..=100.).contains(&value) {
        Ok(())
    } else {
        Err(invalid())
    }
}

fn drag(points: &[[f64; 2]]) -> Result<Vec<Point>, EditError> {
    if points.is_empty() || !points.iter().flatten().all(|v| v.is_finite()) {
        return Err(invalid());
    }
    Ok(points.iter().map(|&[x, y]| Point(x, y)).collect())
}

pub(super) fn page_origin(ix: &Index, text: &str, page: usize) -> Result<(f64, f64), EditError> {
    let bounds = ix
        .attr(text, page, "GeometricBounds")
        .unwrap_or("0 0 0 0")
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>();
    let Ok([top, left, bottom, right]) = bounds.as_deref() else {
        return Err(EditError::new("xml", "The page bounds are invalid."));
    };
    let m = item_transform(ix, text, page);
    let corners = [(*left, *top), (*right, *top), (*left, *bottom), (*right, *bottom)];
    Ok(corners
        .into_iter()
        .map(|p| m.apply(p))
        .fold((f64::INFINITY, f64::INFINITY), |(x, y), p| (x.min(p.0), y.min(p.1))))
}

fn page_frame(w: &mut Work<'_>, page: &str) -> Result<(String, Point), EditError> {
    let (part, ix, i) = w.find(page)?;
    let text = w.text(&part)?;
    if ix.local(text, i) != "Page" {
        return Err(invalid());
    }
    let (x, y) = page_origin(&ix, text, i)?;
    Ok((part, Point(x, y)))
}

fn inverse(m: Mat) -> Result<Mat, EditError> {
    m.invert()
        .ok_or_else(|| EditError::new("xml", "The path transform is singular."))
}

fn transform(m: Mat, p: Point) -> Point {
    let (x, y) = m.apply((p.0, p.1));
    Point(x, y)
}

fn controls(path: &[PathPoint], closed: bool, start: usize) -> Vec<Point> {
    let point = |(x, y)| Point(x, y);
    let mut out = vec![point(path[start].anchor)];
    for offset in 1..path.len() + usize::from(closed) {
        let i = (start + offset) % path.len();
        let prev = (i + path.len() - 1) % path.len();
        out.extend([point(path[prev].right), point(path[i].left), point(path[i].anchor)]);
    }
    out
}

fn anchors(curve: &[Point]) -> Result<Vec<PathPoint>, EditError> {
    if curve.len() < 4 || curve.iter().any(|p| !p.finite()) {
        return Err(invalid());
    }
    let pair = |p: Point| (p.0, p.1);
    Ok((0..curve.len())
        .step_by(3)
        .map(|i| PathPoint {
            anchor: pair(curve[i]),
            left: pair(curve[i.saturating_sub(1)]),
            right: pair(curve[(i + 1).min(curve.len() - 1)]),
        })
        .collect())
}

fn fold_seam(path: &mut Vec<PathPoint>) {
    let last = path.pop().unwrap();
    path[0].left = last.left;
}

fn create_stroke(
    w: &mut Work<'_>,
    page: &str,
    points: &[Point],
    tol: f64,
    smoothness: f64,
    layer: &str,
    close: bool,
) -> Result<(), EditError> {
    let curve = fit::new_stroke(points, tol, smoothness);
    let mut path = anchors(&curve)?;
    if close && path.len() >= 3 && (curve[0] - curve[curve.len() - 1]).length() <= tol {
        fold_seam(&mut path);
    }
    let geometry = [(path, close)];
    let mut rect = geom::path_bounds(&geometry).ok_or_else(invalid)?;
    for axis in 0..2 {
        if rect[axis + 2] == rect[axis] {
            rect[axis + 2] += 1e-3;
        }
    }
    create::run(
        w,
        page,
        create::ItemKind::Line,
        rect,
        (!layer.is_empty()).then_some(layer),
        6,
        Some(&geometry[0].0),
    )?;
    if close {
        let id = w.created.last().unwrap().clone();
        super::convert::convert_shape(w, &[id], super::convert::Shape::ClosedPath)?;
    }
    Ok(())
}

pub(crate) fn draw(
    w: &mut Work<'_>,
    page: &str,
    points: &[[f64; 2]],
    fidelity: &f64,
    smoothness: &f64,
    scale: &f64,
    layer: &str,
) -> Result<(), EditError> {
    let tol = tolerance(*fidelity, *scale)?;
    check_smoothness(*smoothness)?;
    create_stroke(w, page, &drag(points)?, tol, *smoothness, layer, false)
}
