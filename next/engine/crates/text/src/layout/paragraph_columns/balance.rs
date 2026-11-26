use super::{EPS, Flow, FlowFrame, FlowLine, Paragraph, first_baseline};
use super::{depth, translate_line, wrapping};

pub(super) fn rows_in(frame: &FlowFrame<'_>, columns: &[usize]) -> Vec<usize> {
    frame
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.para.is_some() && columns.contains(&line.col))
        .map(|(index, _)| index)
        .collect()
}

#[derive(Clone, Copy)]
pub(super) struct Row {
    pub head: f64,
    pub step: f64,
    pub first: f64,
    pub leading: f64,
    cost: f64,
}

impl Row {
    // R4, R7, R8
    fn new(
        frame: &FlowFrame<'_>,
        line: &FlowLine,
        previous: Option<&FlowLine>,
        origin: f64,
        continued: bool,
    ) -> Self {
        let leading = line.m.leading;
        let first = if line.line.table.is_some() {
            line.m.ascent
        } else {
            first_baseline(frame.frame, &line.m)
        };
        let step = match previous {
            Some(prev) if line.col == prev.col => {
                (line.line.baseline - prev.line.baseline).max(0.0)
            }
            Some(_) => leading,
            None => (line.line.baseline - origin).max(0.0),
        };
        let head = if previous.is_none() {
            step
        } else if continued {
            leading
        } else {
            first
        };
        let head = line
            .grid
            .map_or(head, |grid| grid.place(origin + head) - origin);
        let cost = if previous.is_none() || line.grid.is_some() {
            head
        } else {
            first
        };
        Self {
            head,
            step,
            first,
            leading,
            cost,
        }
    }

    pub(super) fn baseline(self, origin: f64, previous: Option<f64>) -> f64 {
        previous.map_or(origin + self.head, |b| b + self.step)
    }
}

type Placement = (usize, f64);

fn positions(counts: &[usize], rows: &[Row], columns: &[usize], origin: f64) -> Vec<Placement> {
    let mut positions = Vec::with_capacity(rows.len());
    let mut at = 0;
    for (&col, &count) in columns.iter().zip(counts) {
        let mut previous = None;
        for row in &rows[at..at + count] {
            let baseline = row.baseline(origin, previous);
            positions.push((col, baseline));
            previous = Some(baseline);
        }
        at += count;
    }
    positions
}

// R6. The regular text rows have the same paragraph order across frames and areas.
pub(super) fn keep_rows(
    flow: &Flow<'_>,
    fi: usize,
    indices: &[usize],
    paragraphs: &[Paragraph],
) -> (Vec<(usize, usize)>, Option<Vec<bool>>) {
    if !flow.keep.active || indices.is_empty() {
        return (Vec::new(), None);
    }
    let mut counts = vec![0usize; paragraphs.len()];
    let mut keys = Vec::with_capacity(indices.len());
    let mut wanted = indices.iter().copied().peekable();
    for (frame_index, frame) in flow.frames.iter().enumerate().take(fi + 1) {
        for (index, line) in frame.lines.iter().enumerate() {
            if let Some(pi) = line.para {
                if frame_index == fi && wanted.peek() == Some(&index) {
                    keys.push((pi, counts[pi]));
                    wanted.next();
                }
                counts[pi] += 1;
            }
        }
    }
    let mut starts = Vec::with_capacity(counts.len());
    let mut position = 0;
    for (pi, &laid) in counts.iter().enumerate() {
        starts.push(position);
        position += flow.keep.counts.get(&pi).copied().unwrap_or(laid).max(laid);
    }
    let mut keep_until = 0;
    let mut inspected = 0;
    let mut allowed = Vec::with_capacity(keys.len());
    for &(pi, number) in &keys {
        for before in inspected..pi {
            keep_until = keep_until
                .max(starts[before + 1] + paragraphs[before].attrs.keep_with_next as usize);
        }
        inspected = pi;
        let attrs = &paragraphs[pi].attrs;
        let count = flow
            .keep
            .counts
            .get(&pi)
            .copied()
            .unwrap_or(counts[pi])
            .max(counts[pi]);
        let together = attrs.keep_lines_together
            && number > 0
            && (attrs.keep_all_lines
                || number < attrs.keep_first_lines as usize
                || number > count.saturating_sub(attrs.keep_last_lines as usize));
        allowed.push(!together && starts[pi] + number >= keep_until);
    }
    let flags = allowed.iter().any(|&flag| !flag).then_some(allowed);
    (keys, flags)
}

pub(super) fn balance(
    frame: &mut FlowFrame<'_>,
    indices: &[usize],
    columns: &[usize],
) -> Option<f64> {
    balance_kept(frame, indices, columns, None)
}

fn placements(
    frame: &FlowFrame<'_>,
    indices: &[usize],
    columns: &[usize],
    allowed: Option<&[bool]>,
) -> Option<Vec<Placement>> {
    let first = frame.cols[*columns.first()?];
    let origin = first.previous.unwrap_or(first.top);
    let mut previous = None;
    let mut rows: Vec<_> = indices
        .iter()
        .map(|&index| {
            let line = &frame.lines[index];
            let row = Row::new(frame, line, previous, origin, first.previous.is_some());
            previous = Some(line);
            row
        })
        .collect();
    let row = rows.first_mut()?;
    let can_wrap = frame.shape.is_none()
        && !frame.wraps.is_empty()
        && indices
            .iter()
            .all(|&index| frame.lines[index].grid.is_none());
    if can_wrap {
        let original = frame.cols[frame.lines[indices[0]].col];
        let natural = if first.previous.is_some() {
            row.leading
        } else {
            row.first
        };
        if row.head > natural + EPS
            && wrapping::baseline(frame, &original, *row, origin + natural, true)
                .is_some_and(|b| b > origin + natural + EPS && (b - origin - row.head).abs() <= EPS)
        {
            row.head = natural;
            row.step = natural;
            row.cost = natural;
        }
    }
    let steps: Vec<_> = rows.iter().map(|row| row.step).collect();
    let costs: Vec<_> = rows.iter().map(|row| row.cost).collect();
    let partition = depth::choose(&steps, &costs, columns.len(), allowed, |counts| {
        positions(counts, &rows, columns, origin)
            .iter()
            .all(|&(ci, b)| b <= frame.cols[ci].bottom + EPS)
    });
    let original = positions(&partition.counts, &rows, columns, origin);
    let wrapped = can_wrap
        .then(|| {
            wrapping::distribute(
                frame,
                columns,
                &rows,
                origin,
                partition.depth,
                &original,
                allowed,
            )
        })
        .flatten();
    Some(wrapped.unwrap_or(original))
}

/// Bottom of the body through a reference when a footnote spans the columns.
pub(in super::super) fn prefix_bottom(flow: &Flow<'_>, fi: usize, reference: usize, paragraphs: &[Paragraph]) -> Option<f64> {
    let frame = &flow.frames[fi];
    let columns: Vec<_> = (0..frame.base_columns).collect();
    let rows: Vec<_> = rows_in(frame, &columns).into_iter()
        .take_while(|&i| frame.lines[i].line.start <= reference)
        .collect();
    let (_, allowed) = keep_rows(flow, fi, &rows, paragraphs);
    placements(frame, &rows, &columns, allowed.as_deref())?.into_iter()
        .map(|(_, b)| b).reduce(f64::max)
}

pub(super) fn balance_kept(
    frame: &mut FlowFrame<'_>,
    indices: &[usize],
    columns: &[usize],
    allowed: Option<&[bool]>,
) -> Option<f64> {
    let positions = placements(frame, indices, columns, allowed)?;
    let first = frame.cols[*columns.first()?];
    let mut lowest = first.previous.unwrap_or(first.top);
    for (&index, (col, baseline)) in indices.iter().zip(positions) {
        let line = &frame.lines[index];
        let old_col = line.col;
        let dx = frame.cols[col].left - frame.cols[old_col].left;
        let dy = baseline - line.line.baseline;
        let table = line.line.table.is_some();
        move_line(&mut frame.lines[index], col, dx, dy);
        if table {
            for child in frame.lines[index + 1..]
                .iter_mut()
                .take_while(|child| child.para.is_none() && child.col == old_col)
            {
                move_line(child, col, dx, dy);
            }
        }
        lowest = lowest.max(baseline);
    }
    Some(lowest)
}

fn move_line(line: &mut FlowLine, col: usize, dx: f64, dy: f64) {
    translate_line(&mut line.line, dx, dy);
    line.col = col;
    line.text.0 += dx;
    line.text.1 += dx;
}
