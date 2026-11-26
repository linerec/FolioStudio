//! Fit a composed paragraph's lines into the available columns and wrap slots.

use super::*;

impl<'c, 'a> Ctx<'c, 'a> {
    /// One attempt of `layout_paragraph`. `plan` holds the room (column or wrap slot width,
    /// less any indent-to-here) of the first lines; later lines get the last entry.
    /// `Err(rooms)` asks for another attempt with these rooms.
    pub(super) fn place_paragraph(
        &self,
        flow: &mut Flow<'_>,
        p: &ParaCtx<'_>,
        plan: &[Room],
        incremental: bool,
        st: &mut TextState,
    ) -> Result<bool, Vec<Room>> {
        let (para, shaped) = (p.para, p.shaped);
        let a = &para.attrs;
        if !self.advance_to_paragraph_start(flow, p) {
            return Ok(false);
        }
        let Some(frame) = flow.frame() else { return Ok(false) };
        let mut verso = frame.verso;
        let mut settings = settings_of(a, verso);
        let single_line = a.composer.to_ascii_lowercase().contains("single");
        // Kenten are drawn by the Japanese composers only (InDesign 2025: the Paragraph,
        // Single-line and World-Ready composers draw none; content-drops kenten specimen).
        let kenten_drawn = matches!(a.composer.as_str(), "HL Composer J" | "HL Single J");
        let Some(col) = flow.column() else { return Ok(false) };
        let forced = p.forced.filter(|f| !f.is_empty());
        let (starts, word_breaks) = p.saved_starts(forced);
        let prepared = self.prepare(para, shaped, &settings, col.width(), &word_breaks);
        let n = prepared.items.len();
        let (forced_breaks, forced_lines) = prepared.saved_breaks(shaped, forced, &starts);
        let pin_items = prepared.pinned_items(p);
        // Attributes of the paragraph return: the last run's.
        let ret_attrs = para.runs.last().map(|r| &r.attrs);
        let ret_font = p.run_fonts.last().map(|r| r.font);
        let default_room = col.width();
        // Drop cap: the lines beside it start after its advance.
        let drop_lines = p.drop_cap.map_or(0, |d| d.lines);
        // With Align Left Edge (the default) the drop cap sits with its outline's left edge on the text's left edge: it
        // and the text beside it move left by the first glyph's left side bearing (measured: Arial T, dropcap-*).
        let drop_shift: f64 = p.drop_cap.filter(|d| d.align_left_edge).and_then(|d| {
            let c = shaped.clusters.first().filter(|c| c.chars.end <= d.chars)?;
            let g = shaped.glyphs.get(c.glyphs.start)?;
            Some(self.table.left_bearing(g.font, g.id)? * c.size)
        }).unwrap_or(0.0);
        let drop_width: f64 = p.drop_cap.map_or(0.0, |d| shaped.clusters.iter().filter(|c| c.chars.end <= d.chars).map(|c| c.advance + c.tracking).sum::<f64>() - drop_shift);
        let drop_chars = p.drop_cap.map_or(0, |d| d.chars);
        // Extra indent of a line: the first line (holding the drop cap) starts the bearing further left,
        // the others a drop cap's advance (less the bearing) further right.
        let (carry_lines, carry_width) = if drop_lines == 0 { flow.drop_carry.take().unwrap_or((0, 0.0)) } else { (0, 0.0) };
        let drop_extra = |line: usize| match line {
            _ if drop_lines == 0 => if line < carry_lines { carry_width } else { 0.0 },
            0 => -drop_shift,
            // the first-line indent holds for every line beside the drop cap (dropcap-indent-first)
            l if l < drop_lines => drop_width + a.first_line_indent,
            _ => 0.0,
        };

        let mut pos = 0usize;
        let mut line_no = 0usize;
        let mut indent_here = 0.0f64;
        let mut rooms = plan.to_vec();
        // Room of every line placed so far.
        let mut placed_rooms = Vec::new();
        let mut recomposed = 0usize;
        let ops = folio_compose::breaks::opportunities(&prepared.items, &settings);
        'compose: loop {
            let base = line_no;
            let widths = rows::RowWidths::new(flow, &rooms, a, default_room);
            let rows = prepared.compose_rows(
                pos, &settings, single_line, forced.map(|_| forced_breaks.as_slice()), &pin_items,
                |k, unit| widths.slots(base + k, unit, drop_extra(base + k)),
            );
            if flow.keep.active {
                flow.keep.counts.insert(p.pi, flow.keep.offset + base + rows.count);
            }
            for (s, e, part) in rows.parts {
                let col_before = (flow.fi, flow.ci);
                if !flow.keep_target((p.pi, flow.keep.offset + line_no)) { return Ok(false); }
                // wrap-box: incremental recomposition must retry the same slot.
                // Taking pending_part below consumes the right half of a wrapped row.
                let before_line = incremental.then(|| flow.checkpoint());
                let last_line = e == n;
                // A saved line goes to InDesign's column (never back) and is set there; an
                // overset one ends the story.
                let forced_line = forced.and_then(|_| forced_lines.get(line_no).or(forced_lines.last()).copied());
                if let Some(fl) = forced_line {
                    let Some(target) = fl.target else { return Ok(false) };
                    let mut moved = false;
                    while (flow.fi, flow.ci) < target {
                        if !flow.advance() {
                            return Ok(false);
                        }
                        moved = true;
                    }
                    if moved {
                        let nverso = flow.frame().is_some_and(|f| f.verso);
                        if nverso != verso {
                            verso = nverso;
                            settings = settings_of(a, verso);
                        }
                    }
                }
                let m = self.line_metrics(para, shaped, &prepared, s, e, last_line, ret_attrs, ret_font, drop_chars);
                let unit = folio_compose::compose::first_unit(&prepared.items, s, &ops, &settings);
                // Footnote references on the line: their notes need room at the column bottom.
                let line_notes: Vec<usize> = if p.notes.is_empty() || flow.notes.is_none() {
                    Vec::new()
                } else {
                    let mut v: Vec<usize> = (s..e)
                        .filter_map(|i| prepared.origin[i])
                        .map(|ci| shaped.clusters[ci].run)
                        .filter(|&r| p.notes.get(r).is_some_and(Option::is_some))
                        .collect();
                    v.dedup();
                    v
                };
                let mut chosen_notes: Option<Vec<NoteBlock>> = None;
                // InDesign's baseline and area for a saved line (`saved`).
                let saved_place = forced_line
                    .filter(|_| saved::use_baselines())
                    .and_then(|fl| Some((fl.baseline?, fl.area?)));
                // The part of the row right of a both-sides wrap, when the line fits there.
                let right_part = match forced_line {
                    Some(_) if saved_place.is_some() => {
                        flow.pending_part = None;
                        None
                    }
                    // The right part of a both-sides row is used for a saved line's later
                    // segment only.
                    Some(fl) => flow.pending_part.take().filter(|_| fl.segment),
                    // The composer put this text in the row's second slot; any other text
                    // starts a new row.
                    None if part == Part::Right => flow.pending_part.take(),
                    None => {
                        flow.pending_part = None;
                        None
                    }
                };
                let mut split_next = None;
                // A measured band records the column and baseline for recomposition.
                let mut line_area = None;
                // The line is the first of its area: the area top and its band's height.
                let mut first_band = None;
                // Whole points the band went down in the frame's own shape (`Slot::Step`).
                let mut nudge = 0.0f64;
                // Vertical position; move on while the line does not fit.
                let (baseline, sl, sr) = loop {
                    if let Some(rp) = right_part.filter(|_| (flow.fi, flow.ci) == col_before) {
                        if line_notes.is_empty() { break (rp.baseline, rp.left, rp.right); }
                        chosen_notes = self.note_blocks(p, &line_notes, flow, rp.baseline, st);
                        if chosen_notes.is_some() { break (rp.baseline, rp.left, rp.right); }
                    }
                    let Some(c) = flow.column() else { return Ok(false) };
                    let ff = flow.frame().expect("column implies frame");
                    // A saved line sits on InDesign's baseline in InDesign's area of the
                    // frame's text area (`saved`).
                    if let Some((y, (l, r))) = saved_place
                        && (line_notes.is_empty() || {
                            chosen_notes = self.note_blocks(p, &line_notes, flow, c.top + y, st);
                            chosen_notes.is_some()
                        })
                    {
                        let left = ff.cols.first().map_or(c.left, |c0| c0.left);
                        flow.saved_columns.insert((flow.fi, flow.ci));
                        // An area far narrower than the line's text is not InDesign's (its
                        // lines fit their area within justification): the column instead.
                        let natural: f64 = prepared.items[s..e].iter().map(|it| it.advance).sum();
                        let room = (r - l) - a.left_indent - a.right_indent;
                        if natural > room * 1.5 + 1.0 && natural <= c.width() + 1.0 {
                            break (c.top + y, c.left, c.right);
                        }
                        break (c.top + y, left + l, left + r);
                    }
                    let b = flow.line_baseline(ff, &c, &m, a, p.same_style, line_no, nudge);
                    // Vertical text: InDesign 2025 keeps the whole em box in the column (vtext-c1, vt-cols: 11 lines, not 12).
                    let mut bottom = flow.text_bottom(&c) - if flow.vertical { m.embox_below } else { 0.0 };
                    let mut notes_here = None;
                    if !line_notes.is_empty() {
                        notes_here = self.note_blocks(p, &line_notes, flow, b, st);
                        if notes_here.is_none() { bottom = f64::NEG_INFINITY; }
                    }
                    if b <= bottom + EPS {
                        let first = if line_no == 0 { a.first_line_indent } else { 0.0 };
                        let need = unit + a.left_indent + a.right_indent + first + indent_here + drop_extra(line_no);
                        match flow.fit_band(&c, &m, b, need, forced_line.is_some(), &mut nudge) {
                            band::Band::Fits { left, right, next, area, first_band: first } => {
                                split_next = next;
                                line_area = Some(area);
                                first_band = first;
                                chosen_notes = notes_here.take();
                                break (b, left, right);
                            }
                            band::Band::Retry => continue,
                            band::Band::NextColumn => {}
                        }
                    }
                    // A saved line stays in its column, past the bottom or beside the wrap.
                    if forced_line.is_some() && (line_notes.is_empty() || notes_here.is_some()) {
                        chosen_notes = notes_here.take();
                        break (b, c.left, c.right);
                    }
                    let last_column = flow.column().map_or(default_room, |c| c.width());
                    nudge = 0.0;
                    if !flow.advance() {
                        // Overset text is composed at the width of the last column, without
                        // its wraps: the paragraph's lines before it follow from that (#347
                        // u4df96: 4 lines beside a wrap at the end of the last frame).
                        // With `hyphenateAcrossColumns` off the overset line is the first of no column either: a hyphen before it
                        // costs, so the composition must know it.
                        let overset_start = !settings.hyphen.across_columns && line_no > 0 && !widths.column_start(line_no)
                            && forced_line.is_none() && part != Part::Right
                            && folio_compose::breaks::hyphen_break(&prepared.items, s, &settings).is_some();
                        if !incremental && (overset_start || plan.get(line_no).is_none_or(|r| (r.width - last_column).abs() > EPS)) {
                            let mut learned = placed_rooms.clone();
                            learned.push(Room { width: last_column, at: None, first: None, next: None, column_start: true });
                            return Err(learned);
                        }
                        return Ok(false);
                    }
                    if flow.column().is_none() {
                        return Ok(false);
                    }
                    let nverso = flow.frame().is_some_and(|f| f.verso);
                    if nverso != verso {
                        verso = nverso;
                        settings = settings_of(a, verso);
                    }
                };
                // The line starts a column when the flow moved on while it was placed. With `hyphenateAcrossColumns` off, a
                // hyphen ended the line before it and the composition did not know of the column: plan again, the composer
                // then prices that hyphen (InDesign's second search).
                let new_column = line_no > 0 && (flow.fi, flow.ci) != col_before;
                let across_replan = !settings.hyphen.across_columns && new_column && !widths.column_start(line_no)
                    && forced_line.is_none() && part != Part::Right
                    && folio_compose::breaks::hyphen_break(&prepared.items, s, &settings).is_some();
                // A row whose rooms differ from the composition's: plan again (a right part was
                // planned with its row).
                let room = (sr - sl) - indent_here;
                let next_room = split_next.map(|(l, r)| r - l);
                let (planned, planned_next) = widths.room(line_no, None, drop_extra(line_no));
                let same_next = match (next_room, planned_next) {
                    (Some(x), Some(y)) => (x - y).abs() <= EPS,
                    (None, None) => true,
                    _ => false,
                };
                if forced_line.is_none() && part != Part::Right && ((room - planned).abs() > EPS || !same_next || across_replan) {
                    let mut learned = placed_rooms.clone();
                    learned.push(Room { width: room, at: line_area.map(|_| (flow.fi, flow.ci, baseline, m.leading)), first: first_band, next: next_room, column_start: new_column });
                    if !incremental {
                        return Err(learned);
                    }
                    if recomposed < 64 {
                        recomposed += 1;
                        rooms = learned;
                        pos = s;
                        flow.restore(before_line.as_ref().expect("incremental retry"));
                        continue 'compose;
                    }
                }
                // A right-to-left paragraph's first-line indent is on the right (InDesign 2025,
                // content-drops specimen drops-rtl); its other indents stay left and right.
                let first = if line_no == 0 && part != Part::Right { a.first_line_indent } else { 0.0 };
                let (first_left, first_right) = if a.right_to_left { (0.0, first) } else { (first, 0.0) };
                let line_left = sl + a.left_indent + first_left + indent_here + if part == Part::Right { 0.0 } else { drop_extra(line_no) };
                let line_right = sr - a.right_indent - first_right - if last_line { a.last_line_indent } else { 0.0 };
                let available = (line_right - line_left).max(0.0);
                // Right of a both-sides wrap, centred text is set flush left, toward the wrap.
                let centred = matches!(settings.alignment, Alignment::Center | Alignment::CenterJustified);
                let part_settings = (right_part.is_some() && centred).then(|| Settings {
                    alignment: if settings.alignment == Alignment::Center { Alignment::Left } else { Alignment::LeftJustified },
                    ..settings
                });
                // Optical margin alignment: the first and last glyph of the line hang past the margins, so the line
                // is set in a measure widened by both (next/oracle/rules/optmargin.ko.md).
                let (hang_left, hang_right) = self.optical_hang(st, a, &prepared, shaped, s, e);
                let line_left = line_left - hang_left;
                let available = available + hang_left + hang_right;
                let placed = if s < e {
                    let line_settings = part_settings.as_ref().unwrap_or(&settings);
                    // A last line set against the Last Line Indent is chosen for the narrower measure but a ragged one is not squeezed
                    // into it: InDesign sets 'brown fox ... dog.' (1.4 pt over the indent) at its natural spacing (oracle cp-last-just-30).
                    let squeeze = if last_line && a.last_line_indent > 0.0 && matches!(line_settings.alignment, Alignment::Left | Alignment::LeftJustified) {
                        available + a.last_line_indent
                    } else {
                        available
                    };
                    Some(folio_compose::place_line(&prepared.items[s..e], 0, e - s, squeeze, line_settings, last_line))
                } else {
                    None
                };
                let (line, text) = line_content::LineContent {
                    paragraph: p, prepared: &prepared, placed: placed.as_ref(), flow, metrics: &m,
                    baseline, left: line_left, right: line_right, slot_left: sl,
                    alignment: settings.alignment, verso, kenten_drawn,
                }.build(self, s..e, last_line, &mut indent_here);
                let ci = flow.ci;
                if flow.keep.active && part != Part::Right {
                    // A paragraph that spans the columns stands in an empty area only when nothing else is in its frame.
                    let area = flow.keep_area(flow.fi, ci);
                    let empty = if flow.keep.spans.contains(&(flow.fi, ci)) { !flow.frames[flow.fi].lines.iter().any(|l| l.col != ci) }
                        else { !flow.frames[flow.fi].lines.iter().any(|l| flow.keep_area(flow.fi, l.col) == area) };
                    let sub = flow.keep_sub(flow.fi, ci);
                    flow.keep.subs.insert((p.pi, flow.keep.offset + line_no), sub);
                    flow.keep.placed.push(((p.pi, flow.keep.offset + line_no), area, empty));
                }
                let grid = flow.frames[flow.fi].grid.and_then(|grid| grid.for_line(a, line_no));
                flow.frames[flow.fi].lines.push(FlowLine { col: ci, line, para: Some(p.pi), m, grid, text });
                for b in chosen_notes.take().unwrap_or_default() {
                    flow.frames[b.frame].notes.push(b);
                }
                flow.table_after = 0.0;
                flow.last = Some(baseline);
                flow.jumped_from = None;
                flow.pending_after = 0.0;
                if let Some(rp) = right_part
                    && rp.shift != 0.0
                    && let Some(left) = flow.frames.get_mut(rp.line.0).and_then(|f| f.lines.get_mut(rp.line.1))
                {
                    shift_line(&mut left.line, rp.shift);
                }
                let shift = if centred { placed.as_ref().and_then(|p| p.x.first()).copied().unwrap_or(0.0) } else { 0.0 };
                flow.pending_part = split_next.map(|(left, right)| RightPart {
                    baseline,
                    left,
                    right,
                    line: (flow.fi, flow.frames[flow.fi].lines.len() - 1),
                    shift,
                });
                // A row's rooms are kept with its first part; the row ends with its last.
                if forced_line.is_some() || part != Part::Right {
                    placed_rooms.push(Room { width: room, at: line_area.map(|_| (flow.fi, flow.ci, baseline, m.leading)), first: first_band, next: next_room, column_start: new_column });
                }
                if forced_line.is_some() || part != Part::Left {
                    line_no += 1;
                }
            }
            break;
        }
        flow.pending_after = a.space_after;
        // A drop cap taller than the paragraph goes on beside the next paragraph's lines.
        flow.drop_carry = if drop_lines > line_no { Some((drop_lines - line_no, drop_width)) } else if carry_lines > line_no { Some((carry_lines - line_no, carry_width)) } else { None };
        // The next paragraph starts a new row.
        flow.pending_part = None;
        Ok(true)
    }

    fn advance_to_paragraph_start(&self, flow: &mut Flow<'_>, p: &ParaCtx<'_>) -> bool {
        let a = &p.para.attrs;
        // A paragraph that starts in the next column, frame or page (Start Paragraph, or a
        // break character before it) moves there unless it already starts a column; with
        // nowhere to go its text is overset (oracle text-special special-*-break).
        if a.start != folio_model::ParagraphStart::Anywhere && p.forced.is_none() && flow.last.is_some() {
            let page_key = |id: &str| self.frames.frame(id).map(|f| (f.spread, f.page_in_spread));
            let (fi0, page0) = (flow.fi, flow.frame().and_then(|f| page_key(f.id)));
            loop {
                if !flow.advance() {
                    return false;
                }
                let Some(f) = flow.frame() else { return false };
                let other_page = page_key(f.id) != page0;
                let there = match a.start {
                    folio_model::ParagraphStart::NextFrame => flow.fi != fi0,
                    folio_model::ParagraphStart::NextPage => other_page,
                    folio_model::ParagraphStart::NextOddPage => other_page && !f.verso,
                    folio_model::ParagraphStart::NextEvenPage => other_page && f.verso,
                    _ => true,
                };
                if there {
                    break;
                }
            }
        }
        true
    }
}
