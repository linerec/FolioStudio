use super::super::{Slot, wrap_slot};
use super::balance::Row;
use super::{Column, EPS, FlowFrame};

// R9
pub(super) fn baseline(
    frame: &FlowFrame<'_>,
    column: &Column,
    row: Row,
    mut baseline: f64,
    first: bool,
) -> Option<f64> {
    let fresh = first && column.previous.is_none();
    let mut top = column.top;
    for _ in 0..64 {
        let band = if fresh { top } else { baseline - row.leading };
        if frame.wraps.iter().any(|wrap| {
            let r = wrap.rect;
            r.top < baseline - EPS
                && r.bottom > band + EPS
                && r.left < column.right - EPS
                && r.right > column.left + EPS
                && (r.left > column.left + EPS
                    || r.right < column.right - EPS
                    || wrap.scan.is_some())
        }) {
            return None;
        }
        match wrap_slot(
            column,
            None,
            frame.wraps,
            band,
            baseline,
            frame.verso,
            column.width(),
        ) {
            Slot::Fits(left, right, None)
                if (left - column.left).abs() <= EPS && (right - column.right).abs() <= EPS =>
            {
                return Some(baseline);
            }
            Slot::Below(y) | Slot::JumpBelow(y) if fresh => {
                top = y;
                baseline = baseline.max(y + row.first);
            }
            Slot::Below(y) | Slot::JumpBelow(y) if row.leading > EPS => {
                let advances = ((y - band - EPS) / row.leading).ceil().max(1.0);
                baseline += row.leading * advances;
            }
            _ => return None,
        }
    }
    None
}

pub(super) fn distribute(
    frame: &FlowFrame<'_>,
    columns: &[usize],
    rows: &[Row],
    origin: f64,
    depth: f64,
    original: &[(usize, f64)],
    allowed: Option<&[bool]>,
) -> Option<Vec<(usize, f64)>> {
    let mut previous = None;
    let mut pushed = false;
    for (&(ci, original), &row) in original.iter().zip(rows) {
        let first = previous != Some(ci);
        let actual = baseline(frame, &frame.cols[ci], row, original, first)?;
        pushed |= actual > original + EPS;
        previous = Some(ci);
    }
    if !pushed {
        return None;
    }

    let fill = |limit: f64| -> Option<Vec<(usize, f64)>> {
        let mut placements = Vec::with_capacity(rows.len());
        for &ci in columns {
            let column = &frame.cols[ci];
            let ceiling = limit.min(column.bottom) + EPS;
            let mut previous = None;
            while let Some(&row) = rows.get(placements.len()) {
                let next = baseline(
                    frame,
                    column,
                    row,
                    row.baseline(origin, previous),
                    previous.is_none(),
                )?;
                if next > ceiling {
                    break;
                }
                placements.push((ci, next));
                previous = Some(next);
            }
        }
        Some(placements)
    };
    let mut low = origin + depth;
    let mut high = columns
        .iter()
        .map(|&ci| frame.cols[ci].bottom)
        .fold(low, f64::max);
    let mut best = fill(high)?;
    if best.len() != rows.len() {
        return None;
    }
    for _ in 0..64 {
        if high - low <= EPS / 16.0 {
            break;
        }
        let mid = low + (high - low) / 2.0;
        let placements = fill(mid)?;
        if placements.len() == rows.len() {
            high = mid;
            best = placements;
        } else {
            low = mid;
        }
    }
    if let Some(allowed) = allowed {
        if best
            .windows(2)
            .zip(allowed.iter().skip(1))
            .any(|(pair, &ok)| pair[0].0 != pair[1].0 && !ok)
        {
            return None;
        }
    }
    Some(best)
}
