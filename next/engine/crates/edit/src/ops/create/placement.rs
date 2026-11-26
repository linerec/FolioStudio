//! Place a new item from page, view, or drag coordinates in its spread.
use super::{EditError, Frame, ItemKind, Mat};

pub(super) struct Placement {
    /// Bounds relative to the page's bounding-box corner on the spread.
    pub(super) page_rect: [f64; 4],
    /// Bounds in the item's own coordinate system.
    pub(super) local_rect: [f64; 4],
    pub(super) matrix: Mat,
    pub(super) upright: bool,
    pub(super) mirror: (bool, bool),
    pub(super) point_offset: (f64, f64),
}

pub(super) fn resolve(
    pm: Mat,
    sm: Mat,
    page_bounds: &str,
    kind: ItemKind,
    rect: [f64; 4],
    frame: Frame,
    has_custom_path: bool,
) -> Result<Placement, EditError> {
    let (spread_turn, scale) = drawing_turn(&pm, &sm, frame, has_custom_path)?;
    let page_bounds = page_bounds
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| EditError::new("xml", "invalid page bounds"))?;
    if page_bounds.len() != 4 {
        return Err(EditError::new("xml", "invalid page bounds"));
    }
    // A placeholder frame on a turned or scaled spread gets gradient handles InDesign works out in the view (angles of -90, -180,
    // -270 and lengths that are not the frame's; drawtools.turned.frames): not reproduced.
    if kind.placeholder() && (spread_turn, scale) != (0, 1.) && frame != Frame::Drag {
        return Err(EditError::new(
            "unsupported",
            "a placeholder frame on a turned or scaled page or spread has not been measured",
        ));
    }
    // the page's top-left corner in the spread's own space: of the page as it lies there (a turned page shows its bounding box)
    let corners = [
        (page_bounds[1], page_bounds[0]),
        (page_bounds[3], page_bounds[0]),
        (page_bounds[3], page_bounds[2]),
        (page_bounds[1], page_bounds[2]),
    ]
    .map(|c| pm.apply(c));
    let origin = top_left(&corners);
    // The view: the spread's linear part shows its own space turned and scaled. A rect read in the view is measured from the
    // top-left of the page's bounding box there and comes back through the inverse into the spread's own space (the sides swap
    // for a quarter turn). `home` is where InDesign puts the item's origin: the middle of its default 10 point box at that corner.
    let mut rect = rect;
    let mut home = (origin.0 + 5., origin.1 + 5.);
    let mut mirror = (false, false);
    // a shape the mouse drew on a turned spread: the item stands upright in the view (ItemTransform: the spread's linear part inverted, no translation)
    let mut upright: Option<(Mat, [f64; 4])> = None;
    if frame != Frame::Page && (spread_turn, scale) != (0, 1.) {
        let lin = Mat([sm.0[0], sm.0[1], sm.0[2], sm.0[3], 0., 0.]);
        let inv = lin
            .invert()
            .ok_or_else(|| EditError::new("xml", "singular spread transform"))?;
        let corner = top_left(&corners.map(|c| lin.apply(c)));
        let back = |p: (f64, f64)| inv.apply((corner.0 + p.0, corner.1 + p.1));
        let view_box = [
            corner.0 + rect[0],
            corner.1 + rect[1],
            corner.0 + rect[2],
            corner.1 + rect[3],
        ];
        let (a, b) = (back((rect[0], rect[1])), back((rect[2], rect[3])));
        rect = if kind == ItemKind::Line {
            [
                a.0 - origin.0,
                a.1 - origin.1,
                b.0 - origin.0,
                b.1 - origin.1,
            ]
        } else {
            [
                a.0.min(b.0) - origin.0,
                a.1.min(b.1) - origin.1,
                a.0.max(b.0) - origin.0,
                a.1.max(b.1) - origin.1,
            ]
        };
        home = back((5., 5.));
        // InDesign stores a shape drawn on a quarter-turned spread mirrored in the spread's own space: a turn of 90 flips it top to
        // bottom, 270 left to right, 180 both (drawtools.turned.mirror). A line keeps the two points as given.
        mirror = match spread_turn {
            90 => (false, true),
            180 => (true, true),
            270 => (true, false),
            _ => (false, false),
        };
        if matches!(kind, ItemKind::Line | ItemKind::TextFrame) {
            mirror = (false, false);
        }
        if frame == Frame::Drag {
            mirror = (false, false);
            upright = Some((inv, view_box));
        }
    }
    // add(geometricBounds) starts from InDesign's default 10pt box: its 5pt centre
    // remains in the transform and the requested dimensions change the path (create-*).
    let matrix = if let Some((inverse, _)) = upright {
        inverse
    } else if kind == ItemKind::Line {
        Mat::ID
    } else {
        Mat([1., 0., 0., 1., home.0, home.1])
    };
    let local = if let Some((_, view_box)) = upright {
        // the box the mouse drew, in the view: the item's own (upright) box, the item carrying the turn's inverse
        view_box
    } else if kind == ItemKind::Line {
        [
            rect[0] + origin.0,
            rect[1] + origin.1,
            rect[2] + origin.0,
            rect[3] + origin.1,
        ]
    } else {
        [
            rect[0] + origin.0 - home.0,
            rect[1] + origin.1 - home.1,
            rect[2] + origin.0 - home.0,
            rect[3] + origin.1 - home.1,
        ]
    };
    let point_offset = if kind == ItemKind::Line {
        origin
    } else if upright.is_some() && !has_custom_path {
        (0., 0.)
    } else {
        (origin.0 - home.0, origin.1 - home.1)
    };
    Ok(Placement {
        page_rect: rect,
        local_rect: local,
        matrix,
        upright: upright.is_some(),
        mirror,
        point_offset,
    })
}

fn top_left(corners: &[(f64, f64); 4]) -> (f64, f64) {
    corners.iter().fold((f64::INFINITY, f64::INFINITY), |o, c| {
        (o.0.min(c.0), o.1.min(c.1))
    })
}

fn drawing_turn(
    page: &Mat,
    spread: &Mat,
    frame: Frame,
    has_custom_path: bool,
) -> Result<(u32, f64), EditError> {
    let unsupported = || {
        EditError::new(
            "unsupported",
            "drawing on a spread turned other than by quarter turns, or on a turned or scaled page, has not been measured",
        )
    };
    // Custom page points start at the page's bounding-box corner even when the page is scaled.
    // Drawing tools also use that corner on a turned page when its spread is unturned.
    let unturned_spread = matches!(quarter(spread), Some((0, ss)) if (ss - 1.).abs() <= 1e-9);
    if !matches!(quarter(page), Some((0, ps)) if (ps - 1.).abs() <= 1e-9)
        && !(frame == Frame::Page && has_custom_path)
        && !(turned_page(page) && unturned_spread)
    {
        return Err(unsupported());
    }
    match frame {
        Frame::Page => Ok((0, 1.)),
        Frame::View => quarter(spread).ok_or_else(unsupported),
        // A non-quarter turn still leaves a dragged shape upright. Any non-quarter value selects that case.
        Frame::Drag => quarter(spread)
            .or_else(|| turn_by_any_angle(spread).map(|s| (1, s)))
            .ok_or_else(unsupported),
    }
}

/// The scale of a transform whose linear part is a turn by some angle (with a uniform scale of 1), if it is nothing else.
fn turn_by_any_angle(m: &Mat) -> Option<f64> {
    let [a, b, c, d, _, _] = m.0;
    let s = a.hypot(b);
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * s.max(1.);
    (s > 1e-9 && near(a, d) && near(b, -c) && (s - 1.).abs() <= 1e-9).then_some(s)
}
/// A page's transform whose linear part is a turn by some angle and nothing else, its numbers as an IDML written elsewhere rounds them (six
/// decimals: `0.866025 0.5 -0.5 0.866025` for 30 degrees).
fn turned_page(m: &Mat) -> bool {
    let [a, b, c, d, _, _] = m.0;
    let near = |x: f64, y: f64| (x - y).abs() <= 2e-6;
    near(a, d) && near(b, -c) && near(a.hypot(b), 1.)
}
/// Quarter turn (0, 90, 180, 270) and scale of a transform's linear part, if it is nothing else.
fn quarter(m: &Mat) -> Option<(u32, f64)> {
    let [a, b, c, d, _, _] = m.0;
    let s = a.hypot(b);
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * s.max(1.);
    if !(s > 1e-9 && near(a, d) && near(b, -c)) {
        return None;
    }
    let turn = if near(b, 0.) {
        if a > 0. { 0 } else { 180 }
    } else if near(a, 0.) {
        if b > 0. { 90 } else { 270 }
    } else {
        return None;
    };
    Some((turn, s))
}
