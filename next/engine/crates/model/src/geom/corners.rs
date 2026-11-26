use super::{CornerKind, CornerOption, Measured, Seg, add, mul, segments, sub, unit};
use crate::{PathPoint, Point, SubPath};

/// Applies corner options in path order (R1–R5).
pub fn apply_corners(sp: &SubPath, options: &[CornerOption; 4]) -> SubPath {
    if sp.points.len() < 3 || !options.iter().any(CornerOption::active) {
        return sp.clone();
    }

    let normalized = without_closing_marker(sp);
    let path = normalized.as_ref().unwrap_or(sp);
    let count = path.points.len();
    if count < 3 {
        return sp.clone();
    }

    let edges: Vec<_> = segments(path).into_iter().map(Measured::new).collect();
    let mut cuts = vec![None; count];
    let eligible = if path.open { 1..count - 1 } else { 0..count };
    for i in eligible {
        let option = options[if count == 4 { i } else { 0 }];
        let previous = (i + count - 1) % count;
        cuts[i] = Cut::at(option, &edges[previous], &edges[i]);
    }
    if cuts.iter().all(Option::is_none) {
        return sp.clone();
    }

    let edges: Vec<_> = edges
        .iter()
        .enumerate()
        .map(|(i, edge)| {
            let from = cuts[i].map_or(0.0, |cut| cut.outgoing);
            let to = cuts[(i + 1) % count].map_or(0.0, |cut| cut.incoming);
            trim(edge, from, to)
        })
        .collect();

    let mut points = Vec::with_capacity(count);
    let mut start = 0;
    for (i, original) in path.points.iter().enumerate() {
        let incoming = if i == 0 && path.open {
            None
        } else {
            edges.get((i + count - 1) % count)
        };
        let outgoing = edges.get(i);
        if let (Some(cut), Some(before), Some(after)) = (cuts[i], incoming, outgoing) {
            append_join(&mut points, cut, original.anchor, before, after);
        } else {
            points.push(PathPoint {
                left: incoming.map_or(original.left, |edge| edge.c),
                right: outgoing.map_or(original.right, |edge| edge.b),
                ..*original
            });
        }
        if i == 0 {
            start = points.len() - 1;
        }
    }
    points.rotate_left(start);
    SubPath {
        points,
        open: sp.open,
    }
}

fn without_closing_marker(path: &SubPath) -> Option<SubPath> {
    if path.open {
        return None;
    }
    let first = path.points.first()?;
    let mut incoming = first.left;
    let mut count = path.points.len();
    while count > 1 {
        let last = &path.points[count - 1];
        if [last.anchor, last.right, incoming] != [first.anchor; 3] {
            break;
        }
        incoming = last.left;
        count -= 1;
    }
    if count == path.points.len() {
        return None;
    }
    let mut points = path.points[..count].to_vec();
    points[0].left = incoming;
    Some(SubPath {
        points,
        open: false,
    })
}

#[derive(Clone, Copy)]
struct Cut {
    kind: CornerKind,
    incoming: f64,
    outgoing: f64,
}

impl Cut {
    fn at(option: CornerOption, before: &Measured, after: &Measured) -> Option<Self> {
        if !option.active() {
            return None;
        }

        let incoming_dir = before.seg.end_dir()?;
        let outgoing_dir = after.seg.start_dir()?;
        let dot = incoming_dir.x * outgoing_dir.x + incoming_dir.y * outgoing_dir.y;
        if dot > 1.0 - 1e-9 {
            return None;
        }

        let mut incoming = cut_length(before, option.radius)?;
        let mut outgoing = cut_length(after, option.radius)?;
        if option.kind == CornerKind::Rounded {
            let common = incoming.min(outgoing);
            incoming = common;
            outgoing = common;
        }
        Some(Self {
            kind: option.kind,
            incoming,
            outgoing,
        })
    }
}

fn cut_length(edge: &Measured, radius: f64) -> Option<f64> {
    let length = edge.length();
    (length.is_finite() && length > 0.0).then(|| radius.min(length / 2.0))
}

fn trim(edge: &Measured, from: f64, to: f64) -> Seg {
    if from == 0.0 && to == 0.0 {
        return edge.seg;
    }
    let start = edge.t_at(from);
    let end = edge.t_at(edge.length() - to);
    edge.seg.sub(start, end)
}

fn append_join(points: &mut Vec<PathPoint>, cut: Cut, corner: Point, before: &Seg, after: &Seg) {
    let a = before.d;
    let b = after.a;
    let inset = |x, y| add(corner, add(mul(sub(a, corner), x), mul(sub(b, corner), y)));
    let mut entry = PathPoint {
        anchor: a,
        left: before.c,
        right: a,
    };
    let mut exit = PathPoint {
        anchor: b,
        left: b,
        right: after.b,
    };

    // R3
    const ROUND_HANDLE: f64 = 0.552_284_749_831;
    let round_handle = |start, distance| {
        unit(sub(corner, start)).map_or(start, |toward| {
            add(start, mul(toward, distance * ROUND_HANDLE))
        })
    };
    match cut.kind {
        CornerKind::Rounded => {
            entry.right = round_handle(a, cut.incoming);
            exit.left = round_handle(b, cut.outgoing);
        }
        CornerKind::InverseRounded => exit.left = inset(1.0, 1.0),
        _ => {}
    }

    // R4
    let steps: &[(f64, f64)] = match cut.kind {
        CornerKind::Inset => &[(1.0, 1.0)],
        CornerKind::Fancy => {
            const THIRD: f64 = 1.0 / 3.0;
            const TWO_THIRDS: f64 = 2.0 / 3.0;
            &[
                (1.0, TWO_THIRDS),
                (THIRD, TWO_THIRDS),
                (THIRD, THIRD),
                (TWO_THIRDS, THIRD),
                (TWO_THIRDS, 1.0),
            ]
        }
        _ => &[],
    };
    points.push(entry);
    points.extend(steps.iter().map(|&(x, y)| {
        let p = inset(x, y);
        PathPoint {
            anchor: p,
            left: p,
            right: p,
        }
    }));
    points.push(exit);
}
