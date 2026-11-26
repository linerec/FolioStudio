use super::region::{Interval, Outline, Scan, Shape, rectangle};
use super::*;

fn frame_rotation(bounds: Rect, turned: bool) -> Affine {
    if turned {
        Affine::new(0.0, -1.0, 1.0, 0.0, 0.0, bounds.left + bounds.right)
    } else {
        Affine::IDENTITY
    }
}

pub(super) fn own_shape(
    item: &PageItem,
    frame: &TextFrame,
    to_spread: Affine,
    turned: bool,
) -> Option<Shape> {
    if item.text_path.is_some() || rectangle(&item.paths, Affine::IDENTITY) {
        return None;
    }
    let bounds = paths_bounds(&item.paths)?;
    let distance = -(frame.insets[0] + stroke_inset(item));
    let mut outline = Outline::new(&item.paths, distance);
    if outline.0.is_empty() {
        return None;
    }
    if distance != 0.0 {
        let inverse = to_spread.invert()?;
        let origin = folio_model::Point::new(bounds.left, bounds.top);
        let single =
            |p: folio_model::Point| folio_model::Point::new(p.x as f32 as f64, p.y as f32 as f64);
        let spread_origin = single(to_spread.apply(single(origin)));
        let exact_origin = to_spread.apply(origin);
        outline = outline.map(|p| {
            let q = to_spread.apply(p);
            let delta = single(folio_model::Point::new(
                q.x - exact_origin.x,
                q.y - exact_origin.y,
            ));
            inverse.apply(folio_model::Point::new(
                spread_origin.x + delta.x,
                spread_origin.y + delta.y,
            ))
        });
    }
    let cols = if turned {
        vertical_columns(item, frame)
    } else {
        columns_of(item, frame)
    };
    Some(Shape::new(
        outline.transform(frame_rotation(bounds, turned)),
        cols,
    ))
}

fn expand(rect: Rect, offsets: [f64; 4]) -> Rect {
    let [t, l, b, r] = offsets;
    Rect {
        left: rect.left - l,
        top: rect.top - t,
        right: rect.right + r,
        bottom: rect.bottom + b,
    }
}

impl<'d> Obstacle<'d> {
    pub(super) fn from_item(
        item: &'d PageItem,
        spread: usize,
        z: (usize, usize),
        parent: Affine,
        to_spread: Affine,
    ) -> Option<Self> {
        let wrap = item.text_wrap.as_ref().filter(|_| item.wraps)?;
        let visible = parent.apply_rect(&visible_bounds(item)?);
        let shaped = wrap.mode == TextWrapMode::Contour && !rectangle(&item.paths, to_spread);
        let bounds = if wrap.inverse {
            let g = parent.apply_rect(&item_bounds(item)?);
            if shaped {
                g
            } else {
                Rect {
                    left: 2.0 * g.left - visible.left,
                    right: 2.0 * g.right - visible.right,
                    top: 2.0 * g.top - visible.top,
                    bottom: 2.0 * g.bottom - visible.bottom,
                }
            }
        } else {
            visible
        };
        let outline = if shaped && wrap.inverse {
            let inside = expand(bounds, wrap.offsets.map(|x| -x));
            Some(
                Outline::new(&item.paths, 0.0)
                    .transform(to_spread)
                    .polygonal()
                    .map(|p| {
                        folio_model::Point::new(
                            inside.left
                                + (p.x - bounds.left) * inside.width() / bounds.width().max(EPS),
                            inside.top
                                + (p.y - bounds.top) * inside.height() / bounds.height().max(EPS),
                        )
                    }),
            )
        } else if shaped {
            Some(
                Outline::new(&item.paths, wrap.offsets[0] + stroke_outset(item))
                    .transform(to_spread),
            )
        } else {
            None
        };
        Some(Self {
            id: &item.id,
            spread,
            z,
            bounds,
            wrap,
            outline,
        })
    }

    fn in_frame(
        &self,
        frame: &FrameRef<'_>,
        to_frame: Affine,
        bounds: Rect,
        facing: bool,
    ) -> Option<Wrap> {
        let mut offsets = self.wrap.offsets;
        if self.wrap.inverse {
            offsets = offsets.map(|v| -v);
        } else if self.outline.is_some() {
            offsets = [offsets[0]; 4];
        }
        let rect = expand(to_frame.apply_rect(&self.bounds), offsets);
        if !rect.intersects(&bounds) {
            return None;
        }
        let turn = frame_rotation(bounds, frame.turned);
        let outline = self
            .outline
            .as_ref()
            .map(|o| o.transform(turn.then_inner(&to_frame)));
        let side = match self.wrap.side {
            TextWrapSide::SideTowardsSpine | TextWrapSide::SideAwayFromSpine if !facing => {
                TextWrapSide::BothSides
            }
            side => side,
        };
        let scan = outline.as_ref().map(Scan::new);
        Some(Wrap {
            rect: turn.apply_rect(&rect),
            mode: self.wrap.mode,
            inverse: self.wrap.inverse,
            side,
            outline,
            scan,
        })
    }
}

impl DocumentFrames<'_> {
    pub(super) fn assign_wraps(&mut self, z_order_only: bool) {
        for f in self.frames.values_mut() {
            if f.frame.ignore_wrap {
                continue;
            }
            let (Some(spread), Some(to_frame), Some(bounds)) =
                (f.spread, f.to_spread.invert(), paths_bounds(&f.item.paths))
            else {
                continue;
            };
            f.wraps = self
                .obstacles
                .iter()
                .filter(|o| o.spread == spread && o.id != f.item.id && (!z_order_only || o.z > f.z))
                .filter_map(|o| o.in_frame(f, to_frame, bounds, self.facing))
                .collect();
        }
    }
}

impl Wrap {
    /// Closed contour segments in the text frame's coordinates.
    pub fn contour_segments(&self) -> Option<&[Vec<folio_model::geom::Seg>]> {
        self.outline.as_ref().map(|o| o.0.as_slice())
    }
}

fn overlaps((l, r): Interval, (a, b): Interval) -> bool {
    a < r - EPS && b > l + EPS
}

fn sides(
    side: TextWrapSide,
    verso: bool,
    room: Interval,
    block: Interval,
    need: f64,
) -> (Interval, Option<Interval>) {
    let (l, r) = room;
    let left = (l, r.min(block.0));
    let right = (l.max(block.1), r);
    let wl = left.1 - left.0;
    let wr = right.1 - right.0;
    let use_left = match side {
        TextWrapSide::BothSides => wl > EPS && wl >= need - TOL,
        TextWrapSide::LargestArea => wl >= wr,
        TextWrapSide::SideTowardsSpine => !verso,
        TextWrapSide::SideAwayFromSpine => verso,
        TextWrapSide::LeftSide => true,
        TextWrapSide::RightSide => false,
    };
    let next = (use_left && side == TextWrapSide::BothSides && wr > EPS).then_some(right);
    (if use_left { left } else { right }, next)
}

fn contour_sides(
    side: TextWrapSide,
    verso: bool,
    room: Interval,
    blocks: &[Interval],
    need: f64,
    height: f64,
) -> (Interval, Option<Interval>) {
    let outer = (blocks[0].0, blocks.last().unwrap().1);
    if blocks.len() == 1 {
        return sides(side, verso, room, outer, need);
    }
    let left = (room.0, room.1.min(outer.0));
    let right = (room.0.max(outer.1), room.1);
    let gaps = blocks
        .windows(2)
        .map(|b| (room.0.max(b[0].1), room.1.min(b[1].0)))
        .filter(|(l, r)| r - l >= height - EPS);
    let allow_left = match side {
        TextWrapSide::LeftSide | TextWrapSide::BothSides => true,
        TextWrapSide::SideTowardsSpine => !verso,
        TextWrapSide::SideAwayFromSpine => verso,
        _ => false,
    };
    let allow_right = match side {
        TextWrapSide::RightSide | TextWrapSide::BothSides => true,
        TextWrapSide::SideTowardsSpine => verso,
        TextWrapSide::SideAwayFromSpine => !verso,
        _ => false,
    };
    if side == TextWrapSide::LargestArea {
        return (
            gaps.chain([right]).fold(left, |best, candidate| {
                if candidate.1 - candidate.0 > best.1 - best.0 {
                    candidate
                } else {
                    best
                }
            }),
            None,
        );
    }
    let mut rooms = allow_left
        .then_some(left)
        .into_iter()
        .chain(gaps)
        .chain(allow_right.then_some(right))
        .filter(|(l, r)| r - l > EPS);
    let Some(first) = rooms.find(|(l, r)| r - l >= need - TOL) else {
        return sides(side, verso, room, outer, need);
    };
    (first, rooms.next())
}

pub(super) fn wrap_slot(
    col: &Column,
    shape: Option<&Shape>,
    wraps: &[Wrap],
    top: f64,
    bottom: f64,
    verso: bool,
    need: f64,
) -> Slot {
    let mut next = None;
    let room = if let Some(shape) = shape {
        let mut rooms = shape
            .rooms(col, top, bottom)
            .into_iter()
            .filter(|&(l, r)| r - l >= need - 1e-4 && r > l);
        let Some(room) = rooms.next() else {
            return Slot::Step;
        };
        next = rooms.next();
        room
    } else {
        (col.left, col.right)
    };
    let mut available = room;
    let mut below: Option<f64> = None;
    let mut jump: Option<f64> = None;
    let mut previous_blocks = Vec::new();
    for w in wraps {
        let rect = w.rect;
        if !overlaps((top, bottom), (rect.top, rect.bottom))
            || !overlaps(room, (rect.left, rect.right))
        {
            continue;
        }
        match w.mode {
            TextWrapMode::NextColumn => return Slot::NextColumn,
            TextWrapMode::JumpObject => {
                jump = Some(jump.map_or(rect.bottom, |y| y.max(rect.bottom)));
                continue;
            }
            _ => {}
        }
        if w.inverse {
            if top < rect.top - EPS || bottom > rect.bottom + EPS {
                return Slot::SkipRow;
            }
            let inside = (available.0.max(rect.left), available.1.min(rect.right));
            let areas = match &w.scan {
                Some(s) => s.inverse_room(top, bottom, inside),
                None => vec![inside],
            };
            let Some(slot) = areas
                .into_iter()
                .find(|&(l, r)| r - l >= need - TOL && r > l)
            else {
                return Slot::SkipRow;
            };
            available = slot;
            next = None;
            continue;
        }
        let blocks = match &w.scan {
            Some(scan) => scan.blocked(top, bottom),
            None => vec![(rect.left, rect.right)],
        };
        if next.is_some_and(|n| blocks.iter().any(|&b| overlaps(n, b))) {
            next = None;
        }
        if blocks.iter().any(|&b| overlaps(available, b)) {
            let (slot, other) =
                contour_sides(w.side, verso, available, &blocks, need, bottom - top);
            if let Some(other) = other.filter(|&n| !previous_blocks.iter().any(|&b| overlaps(n, b)))
            {
                next = Some(other);
            }
            if slot.1 - slot.0 < need - TOL || slot.1 <= slot.0 {
                below = Some(below.map_or(rect.bottom, |y| y.max(rect.bottom)));
            } else {
                available = slot;
            }
        }
        previous_blocks.extend(blocks);
    }
    if let Some(y) = jump {
        return Slot::JumpBelow(below.map_or(y, |b| y.max(b)));
    }
    if let Some(y) = below {
        return Slot::Below(y);
    }
    Slot::Fits(available.0, available.1, next, room)
}

fn find_band(
    shape: &Shape,
    col: &Column,
    mut top: f64,
    height: f64,
    need: f64,
) -> Option<(f64, f64)> {
    while top + height <= col.bottom + EPS {
        if let Some((l, r)) = shape
            .rooms(col, top, top + height)
            .into_iter()
            .find(|&(l, r)| r > l && r - l >= need - 1e-4)
        {
            return Some((top + height, r - l));
        }
        top += 1.0;
    }
    None
}

pub(super) fn first_room(
    geo: &[(Option<&Shape>, Vec<Column>)],
    fi: usize,
    ci: usize,
    top: f64,
    height: f64,
    need: f64,
) -> Option<f64> {
    let (shape, cols) = geo.get(fi)?;
    find_band((*shape)?, cols.get(ci)?, top, height, need).map(|(_, w)| w)
}

pub(super) fn rooms_ahead(flow: &Flow<'_>, rooms: &[Room]) -> Vec<Ahead> {
    let mut ahead = Vec::new();
    let Some((fi, ci, baseline, leading)) = rooms.last().and_then(|r| r.at) else {
        return ahead;
    };
    if leading <= EPS {
        return ahead;
    }
    let Some(frame) = flow.frames.get(fi) else {
        return ahead;
    };
    let Some(shape) = frame.shape else {
        return ahead;
    };
    for (index, col) in frame.cols.iter().enumerate().skip(ci) {
        let mut top = if index == ci { baseline } else { col.top };
        let mut first = (index != ci).then_some((fi, index, top, leading));
        while let Some((bottom, width)) = find_band(shape, col, top, leading, EPS) {
            ahead.push(Ahead {
                width,
                first: first.take(),
            });
            top = bottom;
        }
    }
    ahead
}
