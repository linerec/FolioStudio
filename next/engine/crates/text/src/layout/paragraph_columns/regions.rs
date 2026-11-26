use std::borrow::Cow;
use std::collections::BTreeSet;

use folio_model::VerticalJustification;

use super::balance::{balance_kept, keep_rows, rows_in};
use super::{
    Column, ColumnLayout, EPS, Flow, FlowFrame, Paragraph, ParagraphAttrs, ParagraphColumns,
};

#[derive(Clone, Copy)]
enum Region {
    Span(usize),
    Split { parent: usize, count: usize },
}

struct FrameRegion {
    fi: usize,
    start: usize,
    end: usize,
    region: Region,
}

pub(in crate::layout) struct Session {
    options: ParagraphColumns,
    frames: Vec<FrameRegion>,
}

pub(in crate::layout) fn effective<'a>(
    attrs: &'a ParagraphAttrs,
    pi: usize,
    unsplit: &BTreeSet<usize>,
) -> Cow<'a, ParagraphColumns> {
    // R13
    if attrs.columns.layout == ColumnLayout::Split && unsplit.contains(&pi) {
        Cow::Owned(ParagraphColumns::default())
    } else {
        Cow::Borrowed(&attrs.columns)
    }
}

fn set_position(flow: &mut Flow<'_>, ci: usize) {
    flow.ci = ci;
    flow.last = flow.column().and_then(|column| column.previous);
    flow.region_top = None;
    flow.jumped_from = None;
    flow.pending_part = None;
}

pub(in crate::layout) fn begin(
    flow: &mut Flow<'_>,
    options: &ParagraphColumns,
    paragraphs: &[Paragraph],
) -> Option<Session> {
    if options.layout == ColumnLayout::Single || flow.column().is_none() {
        return None;
    }
    let mut frames = Vec::new();
    for fi in flow.fi..flow.frames.len() {
        let frame = &flow.frames[fi];
        let current = fi == flow.fi;
        let start = frame.start;
        let end = frame.cols.len();
        if end <= start {
            continue;
        }
        let region = match options.layout {
            ColumnLayout::Span => {
                let count = options
                    .count
                    .map_or(end - start, |n| (n as usize).clamp(1, end - start));
                let columns: Vec<_> = (start..start + count).collect();
                let first = frame.cols[start];
                let right = frame.cols[start + count - 1].right;
                let mut previous = first.previous;
                if current {
                    let rows = rows_in(frame, &columns);
                    let (_, allowed) = keep_rows(flow, fi, &rows, paragraphs);
                    previous =
                        balance_kept(&mut flow.frames[fi], &rows, &columns, allowed.as_deref())
                            .or(previous)
                            .map(|b| b + options.before);
                }
                flow.frames[fi].cols.push(Column {
                    right,
                    previous,
                    ..first
                });
                flow.keep.spans.insert((fi, end));
                Region::Span(count)
            }
            ColumnLayout::Split => {
                let parent = if current { flow.ci } else { start };
                let count = options.count.unwrap_or(2).clamp(2, 40) as usize;
                let frame = &mut flow.frames[fi];
                for ci in parent..end {
                    let mut base = frame.cols[ci];
                    if current && ci == parent {
                        base.previous = flow.last;
                    }
                    let width = ((base.width()
                        - 2.0 * options.outside
                        - (count - 1) as f64 * options.inside)
                        / count as f64)
                        .max(0.0);
                    for j in 0..count {
                        let left =
                            base.left + options.outside + j as f64 * (width + options.inside);
                        frame.cols.push(Column {
                            left,
                            right: left + width,
                            ..base
                        });
                    }
                }
                Region::Split { parent, count }
            }
            ColumnLayout::Single => unreachable!(),
        };
        flow.frames[fi].start = end;
        frames.push(FrameRegion {
            fi,
            start,
            end,
            region,
        });
        if current {
            set_position(flow, end);
        }
    }
    (!frames.is_empty()).then(|| Session {
        options: options.clone(),
        frames,
    })
}

// R2
fn same_region(a: &ParagraphColumns, b: &ParagraphColumns) -> bool {
    a.layout == b.layout
        && a.count == b.count
        && (a.layout != ColumnLayout::Split
            || ((a.inside - b.inside).abs() <= EPS && (a.outside - b.outside).abs() <= EPS))
}

fn reopen(
    frame: &mut FlowFrame<'_>,
    parents: std::ops::Range<usize>,
    covered: usize,
    boundary: Option<f64>,
) -> usize {
    let start = frame.cols.len();
    for (i, ci) in parents.enumerate() {
        let mut base = frame.cols[ci];
        if let Some(baseline) = boundary.filter(|_| i < covered) {
            base.previous = Some(baseline);
            base.top = baseline;
        }
        frame.cols.push(base);
    }
    frame.start = start;
    start
}

// R10
fn finish_span(
    frame: &mut FlowFrame<'_>,
    region: &FrameRegion,
    count: usize,
    after: f64,
    next: Option<&ParagraphColumns>,
) -> usize {
    let last = frame
        .lines
        .iter()
        .rev()
        .find(|line| line.col == region.end && line.para.is_some())
        .map(|line| line.line.baseline);
    let gap = next
        .filter(|next| next.layout == ColumnLayout::Span)
        .map_or(after, |next| (after - next.before).max(0.0));
    reopen(
        frame,
        region.start..region.end,
        count,
        last.map(|b| b + gap),
    )
}

// R12
fn finish_split(
    flow: &mut Flow<'_>,
    region: &FrameRegion,
    parent: usize,
    count: usize,
    paragraphs: &[Paragraph],
    next_pi: Option<usize>,
) -> usize {
    let fi = region.fi;
    let group = (flow.ci - region.end) / count;
    let deferred = next_pi
        .and_then(|pi| flow.keep.targets.get(&(pi, 0)))
        .is_some_and(|&target| target > (fi, flow.keep_area_of(fi, flow.ci)));
    let balance = flow.frames[fi].frame.vertical_justification != VerticalJustification::Top
        || (next_pi.is_some() && !deferred);
    let mut boundary = flow.last;
    if balance {
        let first = region.end + group * count;
        let columns: Vec<_> = (first..first + count).collect();
        let rows = rows_in(&flow.frames[fi], &columns);
        let (keys, allowed) = keep_rows(flow, fi, &rows, paragraphs);
        let frame = &mut flow.frames[fi];
        boundary = balance_kept(frame, &rows, &columns, allowed.as_deref()).or(boundary);
        for (key, &index) in keys.into_iter().zip(&rows) {
            let col = frame.lines[index].col;
            flow.keep
                .subs
                .insert(key, (frame.cols[col].left * 1000.0).round() as i64 + 1);
        }
    }
    reopen(
        &mut flow.frames[fi],
        parent + group..region.end,
        1,
        boundary,
    )
}

pub(in crate::layout) fn finish(
    flow: &mut Flow<'_>,
    session: &mut Option<Session>,
    next: Option<&ParagraphColumns>,
    paragraphs: &[Paragraph],
    next_pi: usize,
) {
    let Some(active) = session.as_ref() else {
        return;
    };
    if next.is_some_and(|next| same_region(&active.options, next)) {
        return;
    }
    let Some(active) = session.take() else { return };
    for region in active.frames {
        let fi = region.fi;
        if fi < flow.fi {
            continue;
        }
        if fi > flow.fi {
            let frame = &mut flow.frames[fi];
            frame.cols.truncate(region.end);
            frame.start = region.start;
            flow.keep.spans.remove(&(fi, region.end));
            continue;
        }
        let start = match region.region {
            Region::Span(count) => finish_span(
                &mut flow.frames[fi],
                &region,
                count,
                paragraphs[next_pi - 1].attrs.columns.after,
                next,
            ),
            Region::Split { parent, count } => finish_split(
                flow,
                &region,
                parent,
                count,
                paragraphs,
                next.map(|_| next_pi),
            ),
        };
        set_position(flow, start);
    }
}
