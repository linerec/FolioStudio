//! Build the local path and serialize its anchors and direction handles.
use super::placement::Placement;
use super::{EditError, ItemKind, PathPoint, geom};

pub(super) fn geometry(
    placement: &Placement,
    kind: ItemKind,
    sides: u32,
    star: Option<f64>,
    custom: Option<&[PathPoint]>,
) -> Result<String, EditError> {
    let local = placement.local_rect;
    // a path of the Pen tool: the points as given on the page, in the item's own space like the box's (create-*)
    let star_points: Option<Vec<PathPoint>> = match star {
        Some(inset) => Some(
            folio_model::polygon::points(
                if placement.upright {
                    local
                } else {
                    placement.page_rect
                },
                sides,
                inset,
            )
            .map_err(|e| EditError::new("invalid", e))?
            .into_iter()
            .map(|p| PathPoint {
                anchor: p,
                left: p,
                right: p,
            })
            .collect(),
        ),
        None => None,
    };
    let mut points: Vec<PathPoint> = match custom.or(star_points.as_deref()) {
        Some(c) => {
            let shift = placement.point_offset;
            let at = |p: (f64, f64)| (p.0 + shift.0, p.1 + shift.1);
            c.iter()
                .map(|p| PathPoint {
                    anchor: at(p.anchor),
                    left: at(p.left),
                    right: at(p.right),
                })
                .collect()
        }
        None => shape_path(kind, local, sides),
    };
    let mirror = placement.mirror;
    if mirror != (false, false) {
        // about the middle of the box the shape was made in
        let (cx, cy) = (local[0] + local[2], local[1] + local[3]);
        let flip = |p: (f64, f64)| {
            (
                if mirror.0 { cx - p.0 } else { p.0 },
                if mirror.1 { cy - p.1 } else { p.1 },
            )
        };
        for p in &mut points {
            *p = PathPoint {
                anchor: flip(p.anchor),
                left: flip(p.left),
                right: flip(p.right),
            };
        }
    }
    let geometry = points
        .iter()
        .map(|p| {
            format!(
                "<PathPointType Anchor=\"{}\" LeftDirection=\"{}\" RightDirection=\"{}\"/>",
                geom::pair(p.anchor),
                geom::pair(p.left),
                geom::pair(p.right)
            )
        })
        .collect::<String>();
    Ok(geometry)
}

/// A new shape's path in the box (also Convert Shape's).
pub(in crate::ops) fn shape_path(
    kind: ItemKind,
    [x0, y0, x1, y1]: [f64; 4],
    sides: u32,
) -> Vec<PathPoint> {
    let point = |p| PathPoint {
        anchor: p,
        left: p,
        right: p,
    };
    let (cx, cy, rx, ry) = (
        (x0 + x1) / 2.,
        (y0 + y1) / 2.,
        (x1 - x0) / 2.,
        (y1 - y0) / 2.,
    );
    match kind {
        ItemKind::Line => vec![point((x0, y0)), point((x1, y1))],
        ItemKind::Ellipse | ItemKind::EllipseFrame => {
            // InDesign's exported cubic ellipse (create-ellipse).
            let k = 0.5522847466666667;
            vec![
                PathPoint {
                    anchor: (cx, y1),
                    left: (cx - rx * k, y1),
                    right: (cx + rx * k, y1),
                },
                PathPoint {
                    anchor: (x1, cy),
                    left: (x1, cy + ry * k),
                    right: (x1, cy - ry * k),
                },
                PathPoint {
                    anchor: (cx, y0),
                    left: (cx + rx * k, y0),
                    right: (cx - rx * k, y0),
                },
                PathPoint {
                    anchor: (x0, cy),
                    left: (x0, cy - ry * k),
                    right: (x0, cy + ry * k),
                },
            ]
        }
        ItemKind::Polygon | ItemKind::PolygonFrame => {
            folio_model::polygon::points([x0, y0, x1, y1], sides, 0.)
                .expect("validated polygon geometry")
                .into_iter()
                .map(point)
                .collect()
        }
        _ => vec![
            point((x0, y0)),
            point((x0, y1)),
            point((x1, y1)),
            point((x1, y0)),
        ],
    }
}
