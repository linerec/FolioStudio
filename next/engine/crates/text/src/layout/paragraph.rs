//! Prepare paragraph text, tabs and composition settings for line placement.

use super::*;

/// Composer settings for a paragraph.
pub(super) fn settings_of(p: &ParagraphAttrs, verso: bool) -> Settings {
    let alignment = match p.justification {
        Justification::LeftAlign => Alignment::Left,
        Justification::CenterAlign => Alignment::Center,
        Justification::RightAlign => Alignment::Right,
        Justification::LeftJustified => Alignment::LeftJustified,
        Justification::CenterJustified => Alignment::CenterJustified,
        Justification::RightJustified => Alignment::RightJustified,
        Justification::FullyJustified => Alignment::FullyJustified,
        // Binding is on the right of a left-hand page.
        Justification::ToBindingSide => {
            if verso {
                Alignment::Right
            } else {
                Alignment::Left
            }
        }
        Justification::AwayFromBindingSide => {
            if verso {
                Alignment::Left
            } else {
                Alignment::Right
            }
        }
    };
    let r = |s: &folio_model::SpacingRange| CRange::new(s.min, s.desired, s.max);
    Settings {
        alignment,
        single_word: match p.single_word_justification {
            Justification::LeftAlign => Alignment::Left,
            Justification::CenterAlign => Alignment::Center,
            Justification::RightAlign => Alignment::Right,
            _ => Alignment::FullyJustified,
        },
        // InDesign balances only ragged paragraphs: with Balance Ragged Lines on, justified Korean and Arabic paragraphs set exactly as
        // with it off (goldens parcomp-balance-korean / -arabic equal noop pixel for pixel), while the English ragged cases change.
        balance_ragged_lines: p.balance_ragged_lines && !alignment.is_justified(),
        word_spacing: r(&p.word_spacing),
        letter_spacing: r(&p.letter_spacing),
        glyph_scaling: r(&p.glyph_scaling),
        composer: match p.composer.as_str() {
            "HL Composer" => Composer::RomanParagraph,
            "HL Composer J" => Composer::JapaneseParagraph,
            _ => Composer::Other,
        },
        hyphenation: p.hyphenation,
        hyphen: HyphenSettings {
            words_longer_than: p.hyphenate_words_longer_than as usize,
            after_first: p.hyphenate_after_first as usize,
            before_last: p.hyphenate_before_last as usize,
            capitalized: p.hyphenate_capitalized,
            ladder_limit: p.hyphen_ladder_limit,
            last_word: p.hyphenate_last_word,
            zone: p.hyphenation_zone,
            weight: folio_compose::cost::hyphen_weight_value(p.hyphen_weight),
            across_columns: p.hyphenate_across_columns,
            ..HyphenSettings::default()
        },
        last_line_indent: p.last_line_indent,
        ..Settings::default()
    }
}

pub(super) fn leading_of(a: &CharacterAttrs) -> f64 {
    match a.leading {
        Leading::Fixed(v) => v,
        Leading::Auto => a.auto_leading / 100.0 * a.point_size,
    }
}

impl<'c, 'a> Ctx<'c, 'a> {
    pub(super) fn run_fonts(&self, para: &Paragraph, missing: &mut BTreeSet<(String, String)>) -> Vec<Resolved> {
        para.runs
            .iter()
            .map(|r| {
                let res = self.fonts.resolve(&r.attrs.font_family, &r.attrs.font_style, &self.doc.font_table);
                if !res.exact {
                    missing.insert((r.attrs.font_family.clone(), r.attrs.font_style.clone()));
                }
                res
            })
            .collect()
    }

    /// Lays out paragraphs from the flow's position (a story, a table cell, a footnote).
    /// False when text is left over (overset).
    /// `plan`: the saved lines of a story laid out from its start (`saved`).
    pub(super) fn lay_text(&self, flow: &mut Flow<'_>, paragraphs: &[Paragraph], st: &mut TextState, plan: Option<&StoryPlan>) -> bool {
        if plan.is_none() && paragraphs.iter().any(|p| p.attrs.keep_lines_together || p.attrs.keep_with_next > 0) {
            return self.lay_kept_text(flow, paragraphs, st);
        }
        self.lay_text_once(flow, paragraphs, st, plan)
    }

    pub(super) fn lay_text_once(&self, flow: &mut Flow<'_>, paragraphs: &[Paragraph], st: &mut TextState, plan: Option<&StoryPlan>) -> bool {
        let shaper = Shaper::new(self.table, self.fallbacks).with_prefs(self.prefs);
        let n_paras = paragraphs.len();
        let mut columns = None;
        for (pi, src) in paragraphs.iter().enumerate() {
            flow.keep.offset = 0;
            let label = st.list_label(&src.attrs);
            let labelled = label.is_some();
            let shown = self.display(src, label, st);
            let para: &Paragraph = shown.para.as_ref().unwrap_or(src);
            let base_para = para;
            // Story index of every shown character and of the paragraph end: a variable (page
            // or note number) is one character of the story however long its text, a bullet
            // or number none.
            let n_chars: usize = para.runs.iter().map(|r| r.text.chars().count()).sum();
            let mut index = Vec::with_capacity(n_chars + 1);
            let mut at = st.char_start;
            for (k, run) in para.runs.iter().enumerate() {
                let shown_n = run.text.chars().count();
                let own = shown.owns.as_ref().map_or(shown_n, |o| o[k]);
                match st.fixed_index {
                    Some(i) => index.extend(std::iter::repeat_n(i, shown_n)),
                    None => index.extend((0..shown_n).map(|j| at + if shown_n == own { j } else { 0 })),
                }
                at += own;
            }
            index.push(st.fixed_index.unwrap_or(at));
            st.paragraphs += 1;
            let forced = plan.and_then(|pl| pl.paras.get(pi)).map(Vec::as_slice);
            // A drop cap: InDesign's saved lines already hold its geometry, and a paragraph with a
            // table or a list label is set without one.
            let plain = forced.is_none() && !labelled && !para.runs.iter().any(|r| !r.tables.is_empty());
            let has_tables = para.runs.iter().any(|r| !r.tables.is_empty());
            if columns.is_none() {
                let own = paragraph_columns::effective(&para.attrs, pi, &flow.keep.unsplit);
                // A paragraph of paragraph columns that the keep pass starts further on (its first line is the one that moves)
                // does not begin in this column: the lines before it are not balanced for it (keepopts-span).
                if own.layout != folio_model::paragraph_columns::ColumnLayout::Single { flow.keep_target((pi, 0)); }
                columns = paragraph_columns::begin(flow, &own, paragraphs);
            }
            // Nested line styles: the styles move the line breaks that decide which characters they cover, so the paragraph
            // is laid out again with the styles of the lines it just got, until the lines stay. When they never do (a word
            // that fits a line in the style of the line after it but not in its own), the lines are settled one at a time,
            // each broken with its own style (`linestyles`).
            let line_styled = !src.attrs.line_styles.is_empty() && !has_tables && st.fixed_index.is_none();
            let (mut restyled, mut assigned): (Option<Paragraph>, Vec<Option<usize>>) = (None, Vec::new());
            let start_state = line_styled.then(|| (flow.checkpoint(), st.clone()));
            let (mut pass, mut one_by_one, mut pins) = (0usize, false, Vec::<usize>::new());
            let ok = loop {
                let para: &Paragraph = restyled.as_ref().unwrap_or(para);
                let dropped = if plain { self.with_drop_cap(para, &mut st.missing) } else { None };
                let (para, drop_cap) = match &dropped {
                    Some((q, d)) => (q, Some(*d)),
                    None => (para, None),
                };
                let p = Para {
                    pi,
                    last_para: pi + 1 == n_paras,
                    same_style: pi > 0 && paragraphs[pi - 1].style == src.style,
                    notes: &shown.notes,
                    forced,
                    drop_cap,
                    pins: &pins,
                };
                let ok = if has_tables {
                    self.layout_with_tables(flow, para, &index, &p, &shaper, st)
                } else {
                    let run_fonts = self.run_fonts(para, &mut st.missing);
                    let object_width = |run: usize, k: usize| para.runs[run].inline.get(k).map_or(0.0, anchors::object_advance);
                    let mut shaped = shaper.shape_paragraph(para, &run_fonts, &object_width);
                    if flow.vertical { crate::vertical::adjust(&mut shaped, para, self.table); }
                    st.note_missing_glyphs(para, &shaped, &run_fonts, &index);
                    let ok = self.layout_paragraph(flow, para, &shaped, &run_fonts, &index, &p, st);
                    // Overset means characters left over: an empty last paragraph (a story
                    // ending with a return) that does not fit has none.
                    ok || (p.last_para && shaped.char_count == 0)
                };
                let Some((checkpoint, state)) = &start_state else { break ok };
                // The lines this pass set, as the character each starts with.
                let mut starts: Vec<usize> = flow.frames.iter().zip(&checkpoint.lines)
                    .flat_map(|(f, &n)| f.lines[n.min(f.lines.len())..].iter().filter(|l| l.para == Some(pi)).map(|l| index.partition_point(|&x| x < l.line.start)))
                    .collect();
                starts.sort_unstable();
                starts.dedup();
                if starts.first() != Some(&0) { starts.insert(0, 0); }
                let total = index.len() - 1;
                let items = &src.attrs.line_styles;
                if !one_by_one {
                    let next = linestyles::assignment(items, &starts, total);
                    pass += 1;
                    if next == assigned { break ok; }
                    if pass < linestyles::MAX_PASSES {
                        restyled = Some(linestyles::restyle(base_para, &next));
                        assigned = next;
                    } else {
                        // never settled: start over, one line at a time
                        one_by_one = true;
                        restyled = Some(linestyles::restyle(base_para, &linestyles::tentative(items, &[], total, linestyles::MAX_LINES)));
                    }
                } else {
                    // `pins` hold the starts of the lines settled so far; this pass broke the line after them with the style
                    // of that line on all the text from there.
                    let m = pins.len();
                    let Some(&next_start) = starts.get(m + 1) else { break ok };
                    if m >= linestyles::MAX_LINES || NestedLineStyle::per_line(items, m + 1)[m].is_none() { break ok; }
                    pins.push(next_start);
                    restyled = Some(linestyles::restyle(base_para, &linestyles::tentative(items, &pins, total, linestyles::MAX_LINES)));
                }
                flow.restore(checkpoint);
                *st = state.clone();
            };
            let next_columns = if ok { paragraphs.get(pi + 1).map(|p| paragraph_columns::effective(&p.attrs, pi + 1, &flow.keep.unsplit)) } else { None };
            paragraph_columns::finish(flow, &mut columns, next_columns.as_deref(), paragraphs, pi + 1);
            st.char_start = at + 1;
            if !ok {
                return false;
            }
        }
        true
    }

    /// The paragraph as laid out when it differs from the story's: page, footnote and
    /// endnote numbers filled in and the bullet or number in front.
    fn display(&self, src: &Paragraph, label: Option<(String, CharacterAttrs)>, st: &mut TextState) -> Shown {
        let variable = |r: &TextRun| {
            matches!(r.variable, Some(TextVariable::PageNumber) | Some(TextVariable::EndnoteNumber) | Some(TextVariable::Named))
                || r.variable == Some(TextVariable::FootnoteNumber)
        };
        if label.is_none() && !src.runs.iter().any(variable) {
            return Shown { para: None, owns: None, notes: Vec::new() };
        }
        let opts = &self.doc.footnotes;
        let mut p = Paragraph { style: src.style.clone(), attrs: src.attrs.clone(), runs: Vec::with_capacity(src.runs.len() + 1) };
        let mut owns = Vec::with_capacity(src.runs.len() + 1);
        let mut notes = Vec::new();
        if let Some((text, attrs)) = label {
            p.runs.push(TextRun { text, attrs, ..TextRun::default() });
            owns.push(0);
            notes.push(None);
        }
        for r in &src.runs {
            let own = if r.variable == Some(TextVariable::FootnoteNumber) { 1 } else { r.text.chars().count() };
            let mut run = r.clone();
            let mut note = None;
            match r.variable {
                Some(TextVariable::PageNumber | TextVariable::PreviousPageNumber) => run.text = st.page_name.clone(),
                Some(TextVariable::NextPageNumber) => run.text = st.next_page.clone(),
                // A text variable: what it gives on this page, else the result InDesign stored
                Some(TextVariable::Named) => {
                    if let Some(n) = &r.named {
                        run.text = st.variables.get(&n.id).cloned().unwrap_or_else(|| n.result.clone());
                    }
                }
                // An empty section marker is about a word space wide in InDesign 2025
                // (oracle text-special special-section-marker: the text after it moves 3.5 pt).
                Some(TextVariable::SectionMarker) => {
                    run.text = if st.section_marker.is_empty() { " ".into() } else { st.section_marker.clone() };
                }
                // the endnote options' numbering style, with the prefix and suffix where they are shown (in the text, in the endnote, both)
                Some(TextVariable::EndnoteNumber) => {
                    let e = &self.doc.endnotes;
                    run.text = r.note.as_ref().and_then(|n| n.number.map(|k| (k, n.body))).map_or_else(|| "#".into(), |(k, body)| {
                        let number = format_number(k, &e.numbering);
                        if e.wraps(body) { format!("{}{number}{}", e.prefix, e.suffix) } else { number }
                    });
                }
                Some(TextVariable::FootnoteNumber) if r.note.is_some() => {
                    let n = r.note.as_ref().and_then(|n| st.note_numbers.get(&n.id)).copied().unwrap_or(st.next_note);
                    st.next_note += 1;
                    let in_text = matches!(opts.show_prefix_suffix.as_str(), "PrefixSuffixReference" | "PrefixSuffixBoth" | "PrefixSuffixInText" | "PrefixSuffixInBoth");
                    let in_note = matches!(opts.show_prefix_suffix.as_str(), "PrefixSuffixMarker" | "PrefixSuffixBoth" | "PrefixSuffixInFootnote" | "PrefixSuffixInBoth");
                    let number = format_number(n, &opts.numbering);
                    run.text = if in_text { format!("{}{number}{}", opts.prefix, opts.suffix) } else { number.clone() };
                    let own_text = if in_note { format!("{}{number}{}", opts.prefix, opts.suffix) } else { number };
                    let mut body = r.note.as_deref().cloned().unwrap_or_default();
                    for q in &mut body.paragraphs {
                        for x in &mut q.runs {
                            if x.variable == Some(TextVariable::FootnoteNumber) && x.note.is_none() {
                                x.text = own_text.clone();
                            }
                        }
                    }
                    run.note = None;
                    note = Some(body);
                }
                _ => {}
            }
            p.runs.push(run);
            owns.push(own);
            notes.push(note);
        }
        if notes.iter().all(Option::is_none) {
            notes.clear();
        }
        Shown { para: Some(p), owns: Some(owns), notes }
    }

    /// A paragraph with tables: its text before, between and after the tables laid out as
    /// lines of their own, each table as a block (InDesign sets a table on a line of its own).
    fn layout_with_tables(
        &self,
        flow: &mut Flow<'_>,
        para: &Paragraph,
        index: &[usize],
        p: &Para<'_>,
        shaper: &Shaper<'_, '_>,
        st: &mut TextState,
    ) -> bool {
        // Character positions of the tables.
        let mut tables: Vec<(usize, &folio_model::Table)> = Vec::new();
        let mut at = 0usize;
        for r in &para.runs {
            let mut k = 0usize;
            for c in r.text.chars() {
                if c == '\u{16}' {
                    if let Some(t) = r.tables.get(k) {
                        tables.push((at, t));
                    }
                    k += 1;
                }
                at += 1;
            }
        }
        let n = at;
        let mut from = 0usize;
        let mut first = true;
        for seg in 0..=tables.len() {
            let to = tables.get(seg).map_or(n, |t| t.0);
            let last = seg == tables.len();
            let mut text = slice_paragraph(para, from, to, last);
            let has_text = text.runs.iter().any(|r| r.text.chars().any(|c| !c.is_whitespace()));
            if has_text {
                flow.keep.offset = flow.keep.counts.get(&p.pi).copied().unwrap_or(0);
                if !first {
                    text.attrs.space_before = 0.0;
                    text.attrs.first_line_indent = 0.0;
                    text.attrs.drop_cap_lines = 0;
                }
                if !last {
                    text.attrs.space_after = 0.0;
                }
                let mut sub_index: Vec<usize> = index[from..to].to_vec();
                sub_index.push(index[to.min(index.len() - 1)]);
                let run_fonts = self.run_fonts(&text, &mut st.missing);
                let object_width = |run: usize, k: usize| text.runs[run].inline.get(k).map_or(0.0, anchors::object_advance);
                let mut shaped = shaper.shape_paragraph(&text, &run_fonts, &object_width);
                if flow.vertical { crate::vertical::adjust(&mut shaped, &text, self.table); }
                st.note_missing_glyphs(&text, &shaped, &run_fonts, &sub_index);
                let forced = p.forced.and_then(|f| saved::part(f, &sub_index));
                let sp = Para { pi: p.pi, last_para: p.last_para && last, same_style: p.same_style && first, notes: &[], forced, drop_cap: None, pins: &[] };
                if !self.layout_paragraph(flow, &text, &shaped, &run_fonts, &sub_index, &sp, st) {
                    return false;
                }
                first = false;
            }
            if let Some(&(pos, table)) = tables.get(seg) {
                let gap = if first { para.attrs.space_before } else { 0.0 };
                if !self.layout_table(flow, table, &para.attrs, gap, index[pos], p.pi, st) {
                    return false;
                }
                first = false;
                from = pos + 1;
            }
        }
        flow.pending_after = para.attrs.space_after;
        true
    }

    /// Composer items for a paragraph at a column width. Tab widths assume each line
    /// starts at the paragraph start or after a forced break.
    /// `breaks`: clusters starting a line InDesign broke inside a word (`saved`), which
    /// end the line before with a hyphen.
    pub(super) fn prepare(&self, para: &Paragraph, shaped: &ShapedParagraph, settings: &Settings, col_width: f64, breaks: &[usize]) -> Prepared {
        let a = &para.attrs;
        let ws = a.word_spacing.desired / 100.0;
        let gs = a.glyph_scaling.desired / 100.0;
        let mut items: Vec<Item> = Vec::with_capacity(shaped.clusters.len());
        for cl in &shaped.clusters {
            let run = &para.runs[cl.run].attrs;
            // The run's 100 % space width: the unit word and letter spacing are measured in.
            let unit = self.table.metrics(cl.font).space_advance * run.point_size * run.horizontal_scale / 100.0;
            let letter = if a.letter_spacing.desired != 0.0 { a.letter_spacing.desired / 100.0 * unit } else { 0.0 };
            let size = run.point_size;
            let item = match cl.kind {
                ClusterKind::Glyphs => {
                    let w = (cl.advance + letter) * gs;
                    Item { ch: cl.ch, advance: w + cl.tracking, end_advance: w - letter * gs, point_size: size, unit }
                }
                ClusterKind::Space => {
                    let w = (cl.advance * ws + letter) * gs + cl.tracking;
                    Item { ch: cl.ch, advance: w, end_advance: w - cl.tracking - letter * gs, point_size: size, unit }
                }
                ClusterKind::FixedSpace => Item {
                    ch: cl.ch,
                    advance: cl.advance + cl.tracking,
                    end_advance: cl.advance,
                    point_size: size,
                    unit,
                },
                ClusterKind::SoftHyphen => Item { unit, ..Item::soft_hyphen(cl.advance, size) },
                ClusterKind::Break => Item { ch: '\n', advance: 0.0, end_advance: 0.0, point_size: size, unit },
                ClusterKind::Tab | ClusterKind::RightTab => {
                    Item { ch: '\t', advance: 0.0, end_advance: 0.0, point_size: size, unit }
                }
                ClusterKind::IndentHere => Item { ch: cl.ch, advance: 0.0, end_advance: 0.0, point_size: size, unit },
                ClusterKind::Object => {
                    Item { ch: '\u{FFFC}', advance: cl.advance, end_advance: cl.advance, point_size: size, unit }
                }
            };
            items.push(item);
        }
        self.set_tabs(para, shaped, &mut items, col_width);

        let runs: Vec<usize> = shaped.clusters.iter().map(|c| c.run).collect();
        let mut points = if settings.hyphenation {
            hyphenate::points_with_width(&items, settings, (col_width - a.left_indent - a.right_indent).max(1.0), &|i| {
                Dictionary::for_language(&para.runs[runs[i]].attrs.language)
            })
        } else {
            Vec::new()
        };
        if settings.hyphenation && !breaks.is_empty() {
            points.extend_from_slice(breaks);
            points.sort_unstable();
            points.dedup();
        }
        if points.is_empty() {
            return Prepared { items, origin: (0..runs.len()).map(Some).collect(), run: runs };
        }
        let h = hyphenate::insert(&items, &points, &|i| shaped.hyphens[runs[i]].width);
        let mut run = Vec::with_capacity(h.items.len());
        let mut last = 0;
        for o in &h.origin {
            if let Some(i) = o {
                last = runs[*i];
            }
            run.push(last);
        }
        Prepared { items: h.items, origin: h.origin, run }
    }

    /// Tab widths from the paragraph's tab stops (positions from the column's left edge)
    /// or default stops every 36 pt; right indent tabs fill to the right indent.
    fn set_tabs(&self, para: &Paragraph, shaped: &ShapedParagraph, items: &mut [Item], col_width: f64) {
        if !shaped.clusters.iter().any(|c| matches!(c.kind, ClusterKind::Tab | ClusterKind::RightTab)) {
            return;
        }
        let a = &para.attrs;
        let mut stops: Vec<&folio_model::TabStop> = a.tabs.iter().collect();
        stops.sort_by(|x, y| x.position.total_cmp(&y.position));
        let mut x = a.left_indent + a.first_line_indent;
        let mut first_line = true;
        let n = items.len();
        for i in 0..n {
            let kind = shaped.clusters[i].kind;
            if kind == ClusterKind::Break {
                x = a.left_indent;
                first_line = false;
                continue;
            }
            if !matches!(kind, ClusterKind::Tab | ClusterKind::RightTab) {
                x += items[i].advance;
                continue;
            }
            // A tab with no room left on its line starts the next line and counts from there
            // (#347 u4a52c: "제시카 박 \t" fills the line, the second tab begins the next).
            if x >= col_width - a.right_indent - EPS {
                x = a.left_indent;
            }
            // Width of the text after the tab up to the next tab or line end.
            let segment_end = (i + 1..n)
                .find(|&j| matches!(shaped.clusters[j].kind, ClusterKind::Tab | ClusterKind::RightTab | ClusterKind::Break))
                .unwrap_or(n);
            let seg: f64 = items[i + 1..segment_end].iter().map(|it| it.advance).sum();
            // A hanging indent (negative first-line indent) has a tab stop at the left indent
            // on its first line, before any later stop (InDesign 2025: bullets and numbers,
            // scratch experiment exp1 "F").
            let next_stop = stops.iter().find(|s| s.position > x + EPS);
            let hanging = first_line
                && a.first_line_indent < 0.0
                && x < a.left_indent - EPS
                && next_stop.is_none_or(|s| s.position > a.left_indent + EPS);
            let w = if kind == ClusterKind::RightTab {
                (col_width - a.right_indent - x - seg).max(0.0)
            } else if hanging {
                a.left_indent - x
            } else {
                match next_stop {
                    Some(stop) => match stop.alignment {
                        TabAlignment::Left => stop.position - x,
                        TabAlignment::Right => (stop.position - x - seg).max(0.0),
                        TabAlignment::Center => (stop.position - x - seg / 2.0).max(0.0),
                        TabAlignment::Character => {
                            let target = stop.align_char.chars().next().unwrap_or('.');
                            let before: f64 = (i + 1..segment_end)
                                .take_while(|&j| shaped.clusters[j].ch != target)
                                .map(|j| items[j].advance)
                                .sum();
                            (stop.position - x - before).max(0.0)
                        }
                    },
                    None => {
                        let next = ((x + EPS) / DEFAULT_TAB).floor() * DEFAULT_TAB + DEFAULT_TAB;
                        next - x
                    }
                }
            };
            // A stop past the line end: the tab fills the rest of the line (InDesign keeps
            // it on the line; measured on #347).
            let w = w.min((col_width - a.right_indent - x).max(0.0));
            items[i].advance = w;
            items[i].end_advance = w;
            x += w;
        }
    }

    /// Lays out one paragraph from the flow's current position. The paragraph is composed
    /// as a whole with the width each line gets (its column, a column further on, the room
    /// beside a text wrap, an indent-to-here): when a line lands where its width differs
    /// from the plan, the flow goes back to the paragraph start and the paragraph is
    /// composed again with the widths learned. After `MAX_REPLANS` attempts the remainder
    /// is recomposed from the line that differs instead.
    #[allow(clippy::too_many_arguments)]
    fn layout_paragraph(
        &self,
        flow: &mut Flow<'_>,
        para: &Paragraph,
        shaped: &ShapedParagraph,
        run_fonts: &[Resolved],
        index: &[usize],
        p: &Para<'_>,
        st: &mut TextState,
    ) -> bool {
        let saved = flow.checkpoint();
        let mut plan: Vec<Room> = Vec::new();
        for attempt in 0..=MAX_REPLANS {
            let incremental = attempt == MAX_REPLANS;
            let ctx = ParaCtx {
                para,
                shaped,
                run_fonts,
                index,
                pi: p.pi,
                last_para: p.last_para,
                same_style: p.same_style,
                notes: p.notes,
                forced: p.forced,
                drop_cap: p.drop_cap,
                pins: p.pins,
            };
            match self.place_paragraph(flow, &ctx, &plan, incremental, st) {
                Ok(fits) => return fits,
                Err(learned) => {
                    flow.restore(&saved);
                    plan = learned;
                }
            }
        }
        false
    }
}
