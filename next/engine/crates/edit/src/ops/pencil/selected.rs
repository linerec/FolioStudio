use super::chain::Chain;
use super::*;

struct Stroke {
    drag: Vec<Point>,
    part: String,
    tol: f64,
    reach: f64,
    hit: f64,
}
impl Stroke {
    fn read(
        w: &mut Work<'_>,
        page: &str,
        points: &[Point],
        tol: f64,
        within: f64,
        scale: f64,
    ) -> Result<Self, EditError> {
        let (part, origin) = page_frame(w, page)?;
        Ok(Self {
            drag: points.iter().map(|&p| p + origin).collect(),
            part,
            tol,
            reach: tolerance(within, scale)?,
            hit: within * 1.28 / scale,
        })
    }
    fn target(&self, w: &mut Work<'_>, id: &str) -> Result<(Vec<Point>, Mat, f64), EditError> {
        let (path, m, part) = pathedit::open_path(w, id, 0)?;
        if self.part != part {
            return Err(invalid());
        }
        if path.len() < 2 {
            return Err(unsupported());
        }
        let curve = controls(&path, false, 0)
            .into_iter()
            .map(|p| transform(m, p))
            .collect::<Vec<_>>();
        let distance = bezier::hit_distance(&curve, self.drag[0]);
        Ok((curve, m, distance))
    }
    fn apply(&self, w: &mut Work<'_>, id: &str, curve: Vec<Point>, matrix: Mat, alt: bool) -> Result<(), EditError> {
        let inverse = inverse(matrix)?;
        let drag = fit::fit(&self.drag, self.tol, Point::ZERO, Point::ZERO, 72.);
        if drag.len() < 4 || drag.iter().any(|p| !p.finite()) {
            return Err(invalid());
        }
        let result = edit_curve::extend(Chain::new(curve, self.tol), Chain::new(drag, self.tol), self.reach);
        let local = result
            .controls
            .into_iter()
            .map(|p| transform(inverse, p))
            .collect::<Vec<_>>();
        let mut path = anchors(&local)?;
        let delta = local[0] - local[local.len() - 1];
        if path.len() >= 3 && delta.0.abs() <= 1e-8 && delta.1.abs() <= 1e-8 {
            fold_seam(&mut path);
        }
        pathedit::replace_path(w, id, 0, &[(path, alt)])
    }
}

pub(crate) fn extend(
    w: &mut Work<'_>,
    id: &str,
    page: &str,
    points: &[[f64; 2]],
    fidelity: &f64,
    scale: &f64,
    within: &f64,
    alt: &bool,
) -> Result<(), EditError> {
    let tol = tolerance(*fidelity, *scale)?;
    positive(*within)?;
    let stroke = Stroke::read(w, page, &drag(points)?, tol, *within, *scale)?;
    let (curve, matrix, distance) = stroke.target(w, id)?;
    if distance > stroke.hit {
        return Err(EditError::new("far", "The pencil starts too far from the path."));
    }
    stroke.apply(w, id, curve, matrix, *alt)
}

pub(crate) fn stroke(
    w: &mut Work<'_>,
    page: &str,
    points: &[[f64; 2]],
    fidelity: &f64,
    smoothness: &f64,
    scale: &f64,
    within: &f64,
    alt: &bool,
    selected: &[String],
    layer: &str,
) -> Result<(), EditError> {
    let tol = tolerance(*fidelity, *scale)?;
    positive(*within)?;
    check_smoothness(*smoothness)?;
    let points = drag(points)?;
    let stroke = Stroke::read(w, page, &points, tol, *within, *scale)?;
    let mut closest = None;
    let mut best = f64::INFINITY;
    for id in selected {
        if let Ok((curve, matrix, distance)) = stroke.target(w, id) {
            if distance <= stroke.hit && distance < best {
                closest = Some((id, curve, matrix));
                best = distance;
            }
        }
    }
    match closest {
        Some((id, curve, matrix)) => stroke.apply(w, id, curve, matrix, *alt),
        None => create_stroke(w, page, &points, tol, *smoothness, layer, *alt),
    }
}
