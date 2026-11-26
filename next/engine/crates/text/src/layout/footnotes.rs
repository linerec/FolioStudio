//! Footnote numbering and placement in the story flow.
use super::*;
use super::structures::translate_line;
use super::strokes::{self, Ends};

impl Ctx<'_, '_> {
    /// Reserve the reference column first. Only the last note on a body line can
    /// continue beyond it, since every reference needs its first line here.
    pub(super) fn note_blocks(&self, p: &ParaCtx<'_>, runs: &[usize], flow: &Flow<'_>, baseline: f64, st: &mut TextState) -> Option<Vec<NoteBlock>> {
        let opts = &self.doc.footnotes;
        let frame = TextFrame {
            first_baseline: opts.first_baseline,
            min_first_baseline_offset: opts.min_first_baseline_offset,
            columns: 1,
            ..TextFrame::default()
        };
        let column = flow.column()?;
        let first = (flow.fi, flow.ci);
        let mut blocks = Vec::new();
        let current = &flow.frames[flow.fi];
        let body_bottom = if current.spans_notes(opts) && current.reserve > 0.0 {
            current.lines.iter().filter(|l| l.para.is_some()).map(|l| l.line.baseline).fold(baseline, f64::max)
        } else { baseline };
        let mut capacity = column.bottom - body_bottom - current.space_before_note(flow.ci, opts);
        for (n, &r) in runs.iter().enumerate() {
            let note = p.notes.get(r)?.as_ref()?;
            let offset = p.para.runs.iter().take(r).map(|r| r.text.chars().count()).sum::<usize>();
            let index = p.index.get(offset).copied().unwrap_or(0);
            let can_continue = !opts.no_splitting && n + 1 == runs.len();
            let mut areas = vec![first];
            let mut cols = vec![note_column(flow.note_width(&column), capacity)];
            if can_continue {
                for (fi, f) in flow.frames.iter().enumerate().skip(flow.fi) {
                    let span = f.spans_notes(opts);
                    let count = if span { 1 } else { f.base_columns };
                    for (ci, col) in f.cols.iter().take(count).enumerate() {
                        if (fi, ci) <= first { continue; }
                        let width = if span { f.cols[f.base_columns - 1].right - col.left } else { col.width() };
                        cols.push(note_column(width, col.bottom - col.top - f.space_before_note(ci, opts)));
                        areas.push((fi, ci));
                    }
                }
            }
            let (lines, complete) = self.lay_nested_columns(&note.paragraphs, cols, &frame, st, index, &note.id);
            if (!complete && !can_continue) || lines.first().is_none_or(|l| l.col != 0) { return None; }
            let mut lines = lines.into_iter().peekable();
            while let Some(column) = lines.peek().map(|l| l.col) {
                let (fi, ci) = areas[column];
                let mut part = NoteBlock { frame: fi, col: ci, lines: Vec::new(), height: 0.0, continued: column > 0, continues: false, overset: false };
                while let Some(mut line) = lines.next_if(|l| l.col == column) {
                    let l = &mut line.line;
                    part.height = part.height.max(l.baseline);
                    l.cell = Some(folio_model::layout::CellText { id: note.id.clone(), start: l.start, end: l.end, clusters: std::mem::take(&mut l.text_clusters) });
                    l.start = index;
                    l.end = index + 1;
                    part.lines.push(line);
                }
                part.overset = !complete && lines.peek().is_none();
                part.continues = !complete || lines.peek().is_some();
                if column == 0 { capacity -= part.height + opts.space_between; }
                blocks.push(part);
            }
        }
        Some(blocks)
    }

    /// Puts each column's footnotes at its bottom, with the separator rule above them.
    pub(super) fn place_notes(&self, flow: &mut Flow<'_>) {
        let opts = &self.doc.footnotes;
        let last_frame = flow.frames.iter().rposition(|f| !f.lines.is_empty() || !f.notes.is_empty());
        for (fi, f) in flow.frames.iter_mut().enumerate() {
            if f.notes.is_empty() {
                continue;
            }
            let notes = std::mem::take(&mut f.notes);
            // Spanning notes: one area across the frame's columns.
            let span = f.spans_notes(opts);
            let spanned = f.cols.first().zip(f.cols.last()).map(|(a, b)| Column { left: a.left, right: b.right, top: a.top, bottom: a.bottom.max(b.bottom), previous: None });
            let areas: Vec<(usize, Column)> = match (span, spanned) {
                (true, Some(c)) => vec![(0, c)],
                _ => f.cols.iter().copied().enumerate().collect(),
            };
            for (ci, col) in areas {
                let blocks: Vec<&NoteBlock> = notes.iter().filter(|n| span || n.col == ci).collect();
                if blocks.is_empty() {
                    continue;
                }
                let depth: f64 = blocks.iter().map(|b| b.height).sum::<f64>() + opts.space_between * (blocks.len() - 1) as f64;
                let mut y = col.bottom - depth;
                if opts.end_of_story && blocks[0].continued {
                    y = y.min(col.top + opts.spacer);
                } else if opts.end_of_story && Some(fi) == last_frame && !blocks.iter().any(|b| b.continues) {
                    let last = f.lines.iter().filter(|l| span || l.col == ci).map(|l| l.line.baseline).fold(col.top, f64::max);
                    y = y.min(last + opts.spacer);
                }
                let mut first = true;
                for b in blocks {
                    for (k, l) in b.lines.iter().enumerate() {
                        let mut l = l.clone();
                        translate_line(&mut l.line, col.left, y);
                        if let Some(text) = &mut l.line.cell {
                            for c in &mut text.clusters {
                                c.x0 += col.left; c.x1 += col.left; c.top += y; c.bottom += y;
                            }
                        }
                        if first && k == 0
                            && let Some(rule) = if b.continued { &opts.continuing_rule } else { &opts.rule }
                        {
                            let x = col.left + rule.left_indent;
                            let end = (x + rule.length).min(col.right).max(x);
                            let cy = y - rule.offset;
                            let h = rule.weight / 2.0;
                            let styled = strokes::line_shapes((x, cy), (end, cy), rule.weight, &rule.stroke_type, Ends::Free, &rule.paint, rule.overprint);
                            match styled {
                                Some(s) => l.line.shapes.extend(s),
                                None => l.line.shapes.extend(TextShape::rect(x, cy - h, end, cy + h, rule.paint.clone(), rule.overprint)),
                            }
                        }
                        l.col = ci;
                        l.text = (l.text.0 + col.left, l.text.1 + col.left);
                        f.lines.push(l);
                    }
                    first = false;
                    y += b.height + opts.space_between;
                }
            }
        }
    }

}

fn note_column(width: f64, height: f64) -> Column {
    Column { left: 0.0, right: width.max(1.0), top: 0.0, bottom: height, previous: None }
}

impl FlowFrame<'_> {
    /// The body gap and the already placed notes, including the gap to the next note.
    fn space_before_note(&self, col: usize, opts: &FootnoteOptions) -> f64 {
        let span = self.spans_notes(opts);
        opts.spacer + self.notes.iter().filter(|b| span || b.col == col)
            .map(|b| b.height + opts.space_between).sum::<f64>()
    }
}

/// Each independent story starts its own footnote sequence.
pub(super) fn footnote_starts(doc: &Document, frames: &DocumentFrames<'_>) -> HashMap<String, u32> {
    frames.stories().map(|id| (id.to_string(), doc.footnotes.start_at)).collect()
}

/// Number references from the pages their note blocks actually reached on the preceding pass.
pub(super) fn numbers(ctx: &Ctx<'_, '_>, flow: &Flow<'_>) -> BTreeMap<String, u32> {
    let opts = &ctx.doc.footnotes;
    if opts.restart == "DontRestart" { return BTreeMap::new(); }
    let mut next = BTreeMap::<String, u32>::new();
    let mut out = BTreeMap::new();
    for f in &flow.frames {
        let Some(frame) = ctx.frames.frame(f.id) else { continue };
        let key = match opts.restart.as_str() {
            "PageRestart" => format!("{:?}/{:?}", frame.spread, frame.page_in_spread),
            "SpreadRestart" => format!("{:?}", frame.spread),
            "SectionRestart" => frame.spread.and_then(|s| ctx.doc.spreads.get(s))
                .and_then(|s| frame.page_in_spread.and_then(|p| s.pages.get(p)))
                .map(|p| p.section_start.to_string()).unwrap_or_default(),
            _ => continue,
        };
        for note in f.notes.iter().filter(|n| !n.continued) {
            let Some(text) = note.lines.iter().find_map(|l| l.line.cell.as_ref()) else { continue };
            let n = next.entry(key.clone()).or_insert(opts.start_at);
            out.insert(text.id.clone(), *n);
            *n += 1;
        }
    }
    out
}
