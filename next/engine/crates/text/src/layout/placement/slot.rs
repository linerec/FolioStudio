//! Try a line in one column. The caller advances the flow if the line cannot fit.

use super::*;

pub(super) struct LineSlot {
    pub(super) baseline: f64,
    pub(super) left: f64,
    pub(super) right: f64,
    pub(super) next: Option<(f64, f64)>,
    pub(super) notes: Vec<NoteBlock>,
    measured: bool,
    first_band: Option<(f64, f64)>,
}

impl LineSlot {
    fn new(baseline: f64, left: f64, right: f64, notes: Vec<NoteBlock>) -> Self {
        Self { baseline, left, right, notes, next: None, measured: false, first_band: None }
    }

    pub(super) fn room(&self, flow: &Flow<'_>, indent: f64, leading: f64, column_start: bool) -> Room {
        Room {
            width: (self.right - self.left) - indent,
            at: self.measured.then_some((flow.fi, flow.ci, self.baseline, leading)),
            first: self.first_band,
            next: self.next.map(|(left, right)| right - left),
            column_start,
        }
    }
}

impl ComposedLine<'_, '_> {
    pub(super) fn note_runs(&self, flow: &Flow<'_>) -> Vec<usize> {
        let p = self.paragraph;
        if p.notes.is_empty() || flow.notes.is_none() {
            return Vec::new();
        }
        let mut runs: Vec<_> = self
            .items
            .clone()
            .filter_map(|i| self.prepared.origin[i])
            .map(|ci| p.shaped.clusters[ci].run)
            .filter(|&r| p.notes.get(r).is_some_and(Option::is_some))
            .collect();
        runs.dedup();
        runs
    }

    pub(super) fn fit_column(
        &self,
        ctx: &Ctx<'_, '_>,
        flow: &mut Flow<'_>,
        column: &Column,
        original_column: (usize, usize),
        saved_place: Option<(f64, (f64, f64))>,
        runs: &[usize],
        need: f64,
        st: &mut TextState,
    ) -> Option<LineSlot> {
        let p = self.paragraph;
        let a = &p.para.attrs;
        let m = &self.metrics;
        let mut notes_at = |flow: &Flow<'_>, baseline| {
            if runs.is_empty() { Some(Vec::new()) } else { ctx.note_blocks(p, runs, flow, baseline, st) }
        };
        // Whole points the band went down in the frame's own shape.
        let mut nudge = 0.0;
        loop {
            if let Some(rp) = self.right_part.filter(|_| (flow.fi, flow.ci) == original_column)
                && let Some(notes) = notes_at(flow, rp.baseline)
            {
                return Some(LineSlot::new(rp.baseline, rp.left, rp.right, notes));
            }
            let frame = flow.frame().expect("column implies frame");
            if let Some((y, (left, right))) = saved_place
                && let Some(notes) = notes_at(flow, column.top + y)
            {
                let origin = frame.cols.first().map_or(column.left, |c| c.left);
                flow.saved_columns.insert((flow.fi, flow.ci));
                // A saved area too narrow for the text falls back to the column.
                let natural: f64 = self.prepared.items[self.items.clone()].iter().map(|it| it.advance).sum();
                let room = (right - left) - a.left_indent - a.right_indent;
                let (left, right) = if natural > room * 1.5 + 1.0 && natural <= column.width() + 1.0 {
                    (column.left, column.right)
                } else {
                    (origin + left, origin + right)
                };
                return Some(LineSlot::new(column.top + y, left, right, notes));
            }
            let baseline = flow.line_baseline(frame, column, m, a, p.same_style, self.number, nudge);
            // Vertical text keeps the whole em box in the column.
            let mut bottom = flow.text_bottom(column) - if flow.vertical { m.embox_below } else { 0.0 };
            let notes = notes_at(flow, baseline);
            if notes.is_none() {
                bottom = f64::NEG_INFINITY;
            }
            if baseline <= bottom + EPS {
                match flow.fit_band(column, m, baseline, need, self.forced, &mut nudge) {
                    band::Band::Fits { left, right, next, first_band } => {
                        return Some(LineSlot {
                            baseline,
                            left,
                            right,
                            next,
                            first_band,
                            measured: true,
                            notes: notes.unwrap_or_default(),
                        });
                    }
                    band::Band::Retry => continue,
                    band::Band::NextColumn => {}
                }
            }
            // A saved line stays in its column, past the bottom or beside the wrap.
            return if self.forced {
                notes.map(|notes| LineSlot::new(baseline, column.left, column.right, notes))
            } else {
                None
            };
        }
    }
}
