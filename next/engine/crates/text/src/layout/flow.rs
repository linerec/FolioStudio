//! Track the current column, footnote space and saved positions during composition.

use super::*;

impl<'c> Flow<'c> {
    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            keep_lines: self.keep.placed.len(),
            fi: self.fi,
            ci: self.ci,
            last: self.last,
            region_top: self.region_top,
            jumped_from: self.jumped_from,
            pending_after: self.pending_after,
            drop_carry: self.drop_carry,
            table_after: self.table_after,
            pending_part: self.pending_part,
            lines: self.frames.iter().map(|f| f.lines.len()).collect(),
            notes: self.frames.iter().map(|f| f.notes.len()).collect(),
            cols: self.frames.iter().map(|f| (f.cols.len(), f.start)).collect(),
        }
    }

    pub(super) fn restore(&mut self, c: &Checkpoint) {
        self.keep.placed.truncate(c.keep_lines);
        self.fi = c.fi;
        self.ci = c.ci;
        self.last = c.last;
        self.region_top = c.region_top;
        self.jumped_from = c.jumped_from;
        self.pending_after = c.pending_after;
        self.drop_carry = c.drop_carry;
        self.table_after = c.table_after;
        self.pending_part = c.pending_part;
        for (((f, &n), &k), &(cols, start)) in self.frames.iter_mut().zip(&c.lines).zip(&c.notes).zip(&c.cols) {
            f.lines.truncate(n);
            f.notes.truncate(k);
            f.cols.truncate(cols);
            f.start = start;
        }
    }

    /// Depth reserved below the body in the current column.
    pub(super) fn note_depth(&self) -> f64 {
        let Some(opts) = self.notes else { return 0.0 };
        let Some(f) = self.frames.get(self.fi) else { return 0.0 };
        let span = f.spans_notes(opts);
        let heights = f.notes.iter().filter(|n| span || n.col == self.ci).map(|n| n.height);
        let (mut sum, mut n) = (0.0, 0usize);
        for h in heights {
            sum += h;
            n += 1;
        }
        let depth = if n == 0 { 0.0 } else { sum + opts.space_between * (n - 1) as f64 + opts.spacer };
        if span { depth.max(f.reserve) } else { depth }
    }

    /// Width footnotes are set at in the current column: the column's, or the frame's text
    /// width when they span its columns.
    pub(super) fn note_width(&self, c: &Column) -> f64 {
        match (self.notes, self.frames.get(self.fi)) {
            (Some(o), Some(f)) if f.spans_notes(o) => {
                f.cols.last().map_or(c.right, |l| l.right) - f.cols.first().map_or(c.left, |x| x.left)
            }
            _ => c.width(),
        }
    }

    /// Lowest baseline for text in the current column: its bottom, above its footnotes.
    pub(super) fn text_bottom(&self, c: &Column) -> f64 {
        if let (Some(opts), Some(f)) = (self.notes, self.frame())
            && opts.end_of_story && f.notes.iter().any(|b| b.continued && (f.spans_notes(opts) || b.col == self.ci))
        {
            return c.top;
        }
        c.bottom - self.note_depth()
    }

    pub(super) fn column(&self) -> Option<Column> {
        self.frames.get(self.fi)?.cols.get(self.ci).copied()
    }
    /// The keep area of column `ci` of frame `fi`: the frame's own column that holds it. A parcel of paragraph columns (a
    /// split's sub-column, the rest of a column after a split) stands in the column it lies in, so a keep group that has to
    /// move goes to the next column of the frame. A paragraph that spans the columns keeps its own area, after all of them.
    /// 0 for a column of the frame (or what stands in one after a split), else the sub-column of a split paragraph's parcel.
    pub(super) fn keep_sub(&self, fi: usize, ci: usize) -> i64 {
        let Some(f) = self.frames.get(fi) else { return 0 };
        if ci < f.base_columns || self.keep.spans.contains(&(fi, ci)) { return 0; }
        let Some(c) = f.cols.get(ci) else { return 0 };
        let (_, own) = self.keep_area(fi, ci);
        match f.cols.get(own) {
            Some(b) if (b.left - c.left).abs() < EPS && (b.right - c.right).abs() < EPS => 0,
            _ => (c.left * 1000.0).round() as i64 + 1,
        }
    }
    /// The column part of `keep_area`.
    pub(super) fn keep_area_of(&self, fi: usize, ci: usize) -> usize {
        self.keep_area(fi, ci).1
    }
    pub(super) fn keep_area(&self, fi: usize, ci: usize) -> (usize, usize) {
        let Some(f) = self.frames.get(fi) else { return (fi, ci) };
        if ci < f.base_columns || self.keep.spans.contains(&(fi, ci)) { return (fi, ci); }
        let Some(c) = f.cols.get(ci) else { return (fi, ci) };
        let own = (0..f.base_columns.min(f.cols.len())).find(|&k| f.cols[k].left - EPS <= c.left && c.right <= f.cols[k].right + EPS);
        (fi, own.unwrap_or(ci))
    }
    pub(super) fn frame(&self) -> Option<&FlowFrame<'c>> {
        self.frames.get(self.fi)
    }
    /// Moves to the next column (or frame). False when there is none.
    pub(super) fn advance(&mut self) -> bool {
        self.last = None;
        self.region_top = None;
        self.jumped_from = None;
        self.pending_after = 0.0;
        self.pending_part = None;
        if let Some(f) = self.frames.get(self.fi)
            && self.ci + 1 < f.cols.len()
        {
            self.ci += 1;
            self.last = self.column().and_then(|c| c.previous);
            return true;
        }
        self.fi += 1;
        self.ci = 0;
        while self.fi < self.frames.len() && self.frames[self.fi].cols.is_empty() {
            self.fi += 1;
        }
        if let Some(f) = self.frames.get(self.fi) { self.ci = f.start; }
        self.last = self.column().and_then(|c| c.previous);
        self.fi < self.frames.len()
    }
}

impl Flow<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn line_baseline(
        &self, frame: &FlowFrame<'_>, c: &Column, m: &LineMetrics, a: &ParagraphAttrs,
        same_style: bool, line_no: usize, nudge: f64,
    ) -> f64 {
        let tf = frame.frame;
        let baseline = match self.last {
            // Below a wrap: InDesign moves the line that met it down in whole
            // points until it clears (#347 p. 78: 162.000 pt below the column's
            // first baseline, not 161.33 where the ascent would reach the wrap).
            None if self.region_top.is_some() && self.jumped_from.is_some() => {
                let from = self.jumped_from.unwrap_or_default();
                let clear = self.region_top.unwrap_or(c.top) + first_baseline(tf, m);
                // A rectangular frame: continuous below the wrap (InDesign 2025, oracle wrapjump:
                // wrap bottom 25..35 pt, always bottom + first baseline).
                if frame.shape.is_none() { from.max(clear) } else { from + (clear - from - EPS).max(0.0).ceil() }
            }
            // A line under above-line objects starts a leading below the top.
            None if m.above_line > 0.0 => self.region_top.unwrap_or(c.top) + m.leading + m.above_line,
            None => {
                let top = self.region_top.unwrap_or(c.top);
                let b = top + first_baseline(tf, m);
                // KeepRuleAboveInFrame: the column's first line sits low enough that the rule above
                // (its offset plus its weight above the baseline) stays inside the frame.
                match a.rule_above.as_ref().filter(|r| r.keep_in_frame && line_no == 0) {
                    Some(r) => b.max(top + r.offset + r.weight),
                    None => b,
                }
            }
            Some(prev) => {
                let gap = if line_no == 0 {
                    match (same_style, a.same_style_spacing) {
                        (true, Some(v)) => v,
                        _ => self.pending_after + a.space_before,
                    }
                } else {
                    0.0
                };
                prev + gap + self.table_after + m.leading + m.above_line
            }
        } + nudge;
        frame.grid.and_then(|grid| grid.for_line(a, line_no)).map_or(baseline, |grid| grid.place(baseline))
    }
}
