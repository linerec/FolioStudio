//! Set the horizontal positions of a fitted line and append it to its frame.

use super::*;

struct AlignedLine {
    left: f64,
    right: f64,
    placed: Option<folio_compose::PlacedLine>,
    shift: f64,
}

impl ComposedLine<'_, '_> {
    fn align(
        &self,
        ctx: &Ctx<'_, '_>,
        slot: &LineSlot,
        settings: &Settings,
        indent_here: f64,
        st: &TextState,
    ) -> AlignedLine {
        let p = self.paragraph;
        let a = &p.para.attrs;
        let (s, e) = (self.items.start, self.items.end);
        let last_line = e == self.prepared.items.len();
        let (line_no, part) = (self.number, self.part);
        // A right-to-left paragraph's first-line indent is on the right (InDesign 2025,
        // content-drops specimen drops-rtl); its other indents stay left and right.
        let first = if line_no == 0 && part != Part::Right { a.first_line_indent } else { 0.0 };
        let (first_left, first_right) = if a.right_to_left { (0.0, first) } else { (first, 0.0) };
        let line_left = slot.left
            + a.left_indent
            + first_left
            + indent_here
            + if part == Part::Right { 0.0 } else { self.drop_indent };
        let line_right =
            slot.right - a.right_indent - first_right - if last_line { a.last_line_indent } else { 0.0 };
        let available = (line_right - line_left).max(0.0);
        // Right of a both-sides wrap, centred text is set flush left, toward the wrap.
        let centred = matches!(settings.alignment, Alignment::Center | Alignment::CenterJustified);
        let part_settings = (self.right_part.is_some() && centred).then(|| Settings {
            alignment: if settings.alignment == Alignment::Center {
                Alignment::Left
            } else {
                Alignment::LeftJustified
            },
            ..*settings
        });
        // Optical margin alignment: the first and last glyph of the line hang past the margins, so the line
        // is set in a measure widened by both (next/oracle/rules/optmargin.ko.md).
        let (hang_left, hang_right) = ctx.optical_hang(st, a, self.prepared, p.shaped, s, e);
        let line_left = line_left - hang_left;
        let available = available + hang_left + hang_right;
        let placed = if s < e {
            let line_settings = part_settings.as_ref().unwrap_or(settings);
            // A last line set against the Last Line Indent is chosen for the narrower measure but a ragged one is not squeezed
            // into it: InDesign sets 'brown fox ... dog.' (1.4 pt over the indent) at its natural spacing (oracle cp-last-just-30).
            let squeeze = if last_line
                && a.last_line_indent > 0.0
                && matches!(line_settings.alignment, Alignment::Left | Alignment::LeftJustified)
            {
                available + a.last_line_indent
            } else {
                available
            };
            Some(folio_compose::place_line(
                &self.prepared.items[s..e],
                0,
                e - s,
                squeeze,
                line_settings,
                last_line,
            ))
        } else {
            None
        };
        let shift =
            if centred { placed.as_ref().and_then(|p| p.x.first()).copied().unwrap_or(0.0) } else { 0.0 };
        AlignedLine { left: line_left, right: line_right, placed, shift }
    }

    pub(super) fn append(
        &self,
        ctx: &Ctx<'_, '_>,
        flow: &mut Flow<'_>,
        slot: LineSlot,
        settings: &Settings,
        verso: bool,
        kenten_drawn: bool,
        indent_here: &mut f64,
        st: &TextState,
    ) {
        let p = self.paragraph;
        let a = &p.para.attrs;
        let (line_no, part, m) = (self.number, self.part, self.metrics);
        let last_line = self.items.end == self.prepared.items.len();
        let aligned = self.align(ctx, &slot, settings, *indent_here, st);
        let (line, text) = line_content::LineContent {
            paragraph: p,
            prepared: self.prepared,
            placed: aligned.placed.as_ref(),
            flow,
            metrics: &m,
            baseline: slot.baseline,
            left: aligned.left,
            right: aligned.right,
            slot_left: slot.left,
            alignment: settings.alignment,
            verso,
            kenten_drawn,
        }
        .build(ctx, self.items.clone(), last_line, indent_here);
        let ci = flow.ci;
        if flow.keep.active && part != Part::Right {
            // A paragraph that spans the columns stands in an empty area only when nothing else is in its frame.
            let area = flow.keep_area(flow.fi, ci);
            let empty = if flow.keep.spans.contains(&(flow.fi, ci)) {
                !flow.frames[flow.fi].lines.iter().any(|l| l.col != ci)
            } else {
                !flow.frames[flow.fi].lines.iter().any(|l| flow.keep_area(flow.fi, l.col) == area)
            };
            let sub = flow.keep_sub(flow.fi, ci);
            flow.keep.subs.insert((p.pi, flow.keep.offset + line_no), sub);
            flow.keep.placed.push(((p.pi, flow.keep.offset + line_no), area, empty));
        }
        let grid = flow.frames[flow.fi].grid.and_then(|grid| grid.for_line(a, line_no));
        flow.frames[flow.fi].lines.push(FlowLine { col: ci, line, para: Some(p.pi), m, grid, text });
        for b in slot.notes {
            flow.frames[b.frame].notes.push(b);
        }
        flow.table_after = 0.0;
        flow.last = Some(slot.baseline);
        flow.jumped_from = None;
        flow.pending_after = 0.0;
        if let Some(rp) = self.right_part
            && rp.shift != 0.0
            && let Some(left) = flow.frames.get_mut(rp.line.0).and_then(|f| f.lines.get_mut(rp.line.1))
        {
            shift_line(&mut left.line, rp.shift);
        }
        flow.pending_part = slot.next.map(|(left, right)| RightPart {
            baseline: slot.baseline,
            left,
            right,
            line: (flow.fi, flow.frames[flow.fi].lines.len() - 1),
            shift: aligned.shift,
        });
    }
}
