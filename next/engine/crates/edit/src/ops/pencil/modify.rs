use super::chain::Chain;
use super::edit_curve::Contact;
use super::*;

struct Target {
    path: Chain,
    index: usize,
    closed: bool,
    compound: bool,
    drag: Vec<Point>,
    contact: Contact,
}
impl Target {
    fn read(w: &mut Work<'_>, id: &str, page: &str, input: &[[f64; 2]], tol: f64) -> Result<Self, EditError> {
        let points = drag(input)?;
        let (paths, m, part) = pathedit::curve_paths(w, id)?;
        let (page_part, origin) = page_frame(w, page)?;
        if part != page_part {
            return Err(invalid());
        }
        let inv = inverse(m)?;
        let drag = points
            .into_iter()
            .map(|p| transform(inv, p + origin))
            .collect::<Vec<_>>();
        let (index, (points, closed)) = paths
            .iter()
            .enumerate()
            .filter(|(_, p)| p.0.len() >= 2)
            .map(|(i, p)| (bezier::hit_distance(&controls(&p.0, p.1, 0), drag[0]), (i, p)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p)
            .ok_or_else(unsupported)?;
        let mut start = 0;
        if *closed {
            let nearest = |p: &PathPoint| {
                drag.iter()
                    .map(|&d| (Point(p.anchor.0, p.anchor.1) - d).length())
                    .fold(f64::INFINITY, f64::min)
            };
            start = (0..points.len())
                .max_by(|&i, &j| nearest(&points[i]).total_cmp(&nearest(&points[j])))
                .unwrap();
        }
        let path = Chain::new(controls(points, *closed, start), tol);
        let contact = Contact::new(&path, drag[0], *drag.last().unwrap());
        Ok(Self {
            path,
            index,
            closed: *closed,
            compound: paths.len() > 1,
            drag,
            contact,
        })
    }
}

pub(crate) fn erase(
    w: &mut Work<'_>,
    id: &str,
    page: &str,
    points: &[[f64; 2]],
    fidelity: &f64,
    scale: &f64,
) -> Result<(), EditError> {
    let t = Target::read(w, id, page, points, tolerance(*fidelity, *scale)?)?;
    let pieces = t.contact.erase(&t.path, &t.drag, t.closed);
    if pieces.len() == 1 && pieces[0] == t.path.controls {
        return Ok(());
    }
    if pieces.is_empty() {
        return Err(EditError::new("unsupported", "Erasing the entire path is unsupported."));
    }
    let paths = pieces.iter().map(|c| anchors(c)).collect::<Result<Vec<_>, _>>()?;
    if t.closed || t.compound {
        let replacements = paths.into_iter().map(|p| (p, false)).collect::<Vec<_>>();
        pathedit::replace_path(w, id, t.index, &replacements)
    } else {
        pathedit::replace_with_pieces(w, id, t.index, &paths)
    }
}

pub(crate) fn smooth(
    w: &mut Work<'_>,
    id: &str,
    page: &str,
    points: &[[f64; 2]],
    fidelity: &f64,
    scale: &f64,
) -> Result<(), EditError> {
    let t = Target::read(w, id, page, points, tolerance(*fidelity, *scale)?)?;
    if t.contact.distant() {
        return Err(EditError::new(
            "unsupported",
            "The smoothing stroke starts too far from the path.",
        ));
    }
    let curve = t.contact.smooth(&t.path);
    if curve.controls == t.path.controls {
        return Ok(());
    }
    let mut path = anchors(&curve.controls)?;
    let keep_closed = t.closed && !t.compound;
    if keep_closed {
        fold_seam(&mut path);
    }
    pathedit::replace_path(w, id, t.index, &[(path, keep_closed)])
}
