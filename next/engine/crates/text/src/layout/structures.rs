//! Text structures around the line breaking: list numbers and bullets, footnote and endnote
//! numbers, footnotes at the column bottom, paragraph rules, shading and borders, nested
//! text (table cells, footnotes) and vertical justification.
//!
//! Geometry follows InDesign 2025 measurements (scratch experiments exp1–exp3, the Feature
//! Atlas reference PDF):
//! - Rule above: bottom of the rule `offset` above the first baseline; rule below: top of the
//!   rule `offset` below the last baseline. Column width spans the column (paragraph
//!   indents do not apply) less the rule's indents; text width spans the line's text less
//!   the rule's indents.
//! - Shading and border boxes run from the first line's ascent / baseline / baseline −
//!   leading to the last line's descent (`OS/2` typographic) / baseline, plus offsets
//!   outward; column width = the column's edges. Border strokes are centred on the box
//!   edges, horizontal ones spanning the vertical ones. Consecutive paragraphs with the same
//!   border share one box ("merge"), the shading follows that box; shading alone does not
//!   merge. A box split over columns leaves its closing edges open unless "display if
//!   splits".
//! - Footnotes: each laid at the column width, depth to its last baseline; the column's
//!   notes stack upward from the column bottom with "space between"; the separator rule is
//!   centred `offset` above them; text above keeps "minimum space before first footnote".

use super::strokes::{self, Ends};
use super::*;

mod rules;

/// Running state of one text being laid out (a story, a cell, a footnote).
#[derive(Clone)]
pub(super) struct TextState {
    pub page_name: String,
    /// The section marker of the page (`TextVariable::SectionMarker`).
    pub section_marker: String,
    /// What a next page number shows.
    pub next_page: String,
    /// What each text variable of the document shows on this page, by the variable's `Self` (a variable not here shows the result
    /// InDesign stored with the instance).
    pub variables: BTreeMap<String, String>,
    /// Number of the next footnote reference.
    pub next_note: u32,
    pub note_numbers: BTreeMap<String, u32>,
    /// Every character gets this story index (nested text, which has no story position).
    pub fixed_index: Option<usize>,
    /// Nested text: the story character it hangs on (missing glyphs are reported there, also
    /// for a cell's text, which counts its own characters).
    pub anchor: Option<usize>,
    /// Story index of the next paragraph's first character.
    pub char_start: usize,
    pub missing: BTreeSet<(String, String)>,
    /// Characters set as .notdef boxes, per font (family, style).
    pub missing_glyphs: BTreeMap<(String, String), MissingGlyphs>,
    pub paragraphs: usize,
    /// List numbers per list name and level (1–9).
    lists: HashMap<String, super::listnumbers::NumberingState>,
    pub list_labels: Option<super::listplan::Labels>,
    /// The composed row heights of the tables set (nested ones too), by table.
    pub table_rows: BTreeMap<String, Vec<f64>>,
    /// Optical margin alignment of the story: its optical size in points (`None`: off).
    pub optical: Option<f64>,
}

impl TextState {
    pub fn new(page_name: String, next_note: u32, fixed_index: Option<usize>) -> TextState {
        TextState {
            section_marker: String::new(),
            next_page: page_name.clone(),
            variables: BTreeMap::new(),
            page_name,
            next_note,
            note_numbers: BTreeMap::new(),
            fixed_index,
            anchor: fixed_index,
            char_start: 0,
            missing: BTreeSet::new(),
            missing_glyphs: BTreeMap::new(),
            paragraphs: 0,
            lists: HashMap::new(),
            list_labels: None,
            table_rows: BTreeMap::new(),
            optical: None,
        }
    }

    /// Records the characters of a shaped paragraph set as .notdef boxes; `index` maps its
    /// characters to story positions.
    pub fn note_missing_glyphs(&mut self, para: &Paragraph, shaped: &ShapedParagraph, run_fonts: &[Resolved], index: &[usize]) {
        if shaped.missing.is_empty() {
            return;
        }
        let chars: Vec<char> = para.runs.iter().flat_map(|r| r.text.chars()).collect();
        for &(k, run) in &shaped.missing {
            let a = &para.runs[run].attrs;
            let at = self.anchor.or_else(|| index.get(k).or(index.last()).copied()).unwrap_or(0);
            let m = MissingGlyphs {
                family: a.font_family.clone(),
                style: a.font_style.clone(),
                installed: run_fonts.get(run).is_some_and(|r| r.exact),
                chars: chars.get(k).map(|c| c.to_string()).unwrap_or_default(),
                count: 1,
                ranges: vec![(at, at + 1)],
                ..MissingGlyphs::default()
            };
            self.add_missing_glyphs((m.family.clone(), m.style.clone()), m);
        }
    }

    /// Merges `m` into the record of its font.
    pub fn add_missing_glyphs(&mut self, key: (String, String), m: MissingGlyphs) {
        let Some(e) = self.missing_glyphs.get_mut(&key) else {
            self.missing_glyphs.insert(key, m);
            return;
        };
        e.installed &= m.installed;
        e.count += m.count;
        for c in m.chars.chars() {
            if e.chars.chars().count() < 64 && !e.chars.contains(c) {
                e.chars.push(c);
            }
        }
        for r in m.ranges {
            match e.ranges.last_mut() {
                Some(last) if r.0 >= last.0 && r.0 <= last.1 => last.1 = last.1.max(r.1),
                _ => e.ranges.push(r),
            }
        }
    }

    /// The bullet or number text of a paragraph (counting it) and its attributes.
    pub fn list_label(&mut self, a: &ParagraphAttrs) -> Option<(String, CharacterAttrs)> {
        let l = a.list.as_ref()?;
        if let Some(labels) = &self.list_labels {
            return labels.get(&self.char_start).map(|label| (label.clone(), l.marker.clone()));
        }
        let text = match l.kind {
            ListKind::Bullet => format!("{}{}", l.bullet, l.bullet_text_after),
            ListKind::Numbered => self.lists.entry(l.list.clone()).or_default().label(l),
        };
        Some((text, l.marker.clone()))
    }
}

/// A paragraph as laid out when it differs from the story's (`None` = the story's own).
pub(super) struct Shown {
    pub para: Option<Paragraph>,
    /// Story characters of each run (variables one, generated runs none).
    pub owns: Option<Vec<usize>>,
    /// Footnote text of each run that is a footnote reference.
    pub notes: Vec<Option<folio_model::Note>>,
}

impl<'c> Flow<'c> {
    pub(super) fn new(frames: Vec<FlowFrame<'c>>, notes: Option<&'c FootnoteOptions>) -> Flow<'c> {
        let mut flow = Flow {
            frames,
            fi: 0,
            ci: 0,
            last: None,
            region_top: None,
            jumped_from: None,
            pending_after: 0.0,
            drop_carry: None,
            pending_part: None,
            table_after: 0.0,
            notes,
            saved_columns: HashSet::new(),
            keep: keep::State::default(),
            vertical: false,
        };
        while flow.fi < flow.frames.len() && flow.frames[flow.fi].cols.is_empty() {
            flow.fi += 1;
        }
        flow
    }
}

/// A number in a list or footnote numbering format: InDesign's paragraph formats
/// ("1, 2, 3, 4...", "01, 02, 03...", "I, II, III, IV...", "a, b, c, d...", …), footnote
/// style names ("Arabic", "UpperRoman", "Asterisks", …) and CJK sequences ("一, 二, 三…",
/// "가, 나, 다…", "ㄱ, ㄴ, ㄷ…", "①, ②, ③…", "１, ２, ３…").
pub fn format_number(n: u32, format: &str) -> String {
    let key = format.split(',').next().unwrap_or(format).trim();
    let roman = |n: u32| {
        let mut n = n;
        let mut s = String::new();
        for (v, r) in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
            while n >= v {
                s.push_str(r);
                n -= v;
            }
        }
        s
    };
    // InDesign counts letters like a spreadsheet's columns: A..Z, AA, AB, ..., AZ, BA, ..., ZZ, AAA (base 26 without a zero digit); measured
    // for both cases up to four letters (oracle/measured/listletters-2025.json).
    let letters = |n: u32, base: u8| {
        let mut n = n;
        let mut out = Vec::new();
        while n > 0 {
            n -= 1;
            out.push((base + (n % 26) as u8) as char);
            n /= 26;
        }
        out.iter().rev().collect::<String>()
    };
    let cycle = |n: u32, set: &[char]| if n == 0 { String::new() } else { set[((n - 1) as usize) % set.len()].to_string() };
    let kanji = |n: u32| {
        const D: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
        match n {
            0..=9 => D[n as usize].to_string(),
            10..=99 => {
                let (t, u) = (n / 10, n % 10);
                let mut s = String::new();
                if t > 1 {
                    s.push(D[t as usize]);
                }
                s.push('十');
                if u > 0 {
                    s.push(D[u as usize]);
                }
                s
            }
            _ => n.to_string(),
        }
    };
    match key {
        "01" | "SingleLeadingZeros" => format!("{n:02}"),
        "001" | "DoubleLeadingZeros" => format!("{n:03}"),
        "0001" | "TripleLeadingZeros" => format!("{n:04}"),
        "I" | "UpperRoman" => roman(n).to_uppercase(),
        "i" | "LowerRoman" => roman(n),
        "A" | "UpperLetters" => letters(n, b'A'),
        "a" | "LowerLetters" => letters(n, b'a'),
        "*" | "Asterisks" => "*".repeat(n as usize),
        "Symbols" if n > 0 => {
            const SYMBOLS: [char; 5] = ['*', '†', '‡', '§', '¶'];
            SYMBOLS[((n - 1) % 5) as usize].to_string().repeat(((n - 1) / 5 + 1) as usize)
        },
        "一" | "Kanji" => kanji(n),
        "가" => cycle(n, &['가', '나', '다', '라', '마', '바', '사', '아', '자', '차', '카', '타', '파', '하']),
        "ㄱ" => cycle(n, &['ㄱ', 'ㄴ', 'ㄷ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅅ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ']),
        "①" => match n {
            1..=20 => char::from_u32(0x2460 + n - 1).map_or_else(|| n.to_string(), |c| c.to_string()),
            21..=35 => char::from_u32(0x3251 + n - 21).map_or_else(|| n.to_string(), |c| c.to_string()),
            36..=50 => char::from_u32(0x32B1 + n - 36).map_or_else(|| n.to_string(), |c| c.to_string()),
            _ => n.to_string(),
        },
        "１" | "FullWidthArabic" => n.to_string().chars().map(|c| char::from_u32(0xFF10 + c as u32 - '0' as u32).unwrap_or(c)).collect(),
        "FormatNone" | "None" => String::new(),
        _ => n.to_string(),
    }
}

/// Moves a laid line and everything on it.
pub(super) fn translate_line(l: &mut LaidLine, dx: f64, dy: f64) {
    for c in &mut l.text_clusters {
        c.x0 += dx; c.x1 += dx; c.top += dy; c.bottom += dy;
    }
    l.baseline += dy;
    l.left += dx;
    l.right += dx;
    if let Some(g) = &mut l.table {
        g.columns.iter_mut().for_each(|x| *x += dx);
        g.rows.iter_mut().for_each(|r| { r.1 += dy; r.2 += dy; });
    }
    for g in &mut l.glyphs {
        g.x += dx;
        g.y += dy;
    }
    for d in &mut l.decorations {
        d.x0 += dx;
        d.x1 += dx;
        d.y += dy;
    }
    for o in &mut l.objects {
        if !o.fixed_x {
            o.x += dx;
        }
        if !o.fixed_y {
            o.y += dy;
        }
    }
    for s in &mut l.shapes {
        for sp in &mut s.paths {
            for pt in &mut sp.points {
                for q in [&mut pt.anchor, &mut pt.left, &mut pt.right] {
                    q.x += dx;
                    q.y += dy;
                }
            }
        }
    }
}

/// Vertical justification (Center / Bottom / Justify) per column. InDesign aligns the text
/// from the first line's top (its first-baseline offset) to the last *baseline*, without the
/// last line's descent: bottom-aligned text has its last baseline on the frame's bottom inset
/// (#347: all 8 bottom-aligned frames, all 9 centred ones). Justify keeps the first line and
/// puts the last baseline there too, the space shared equally between the lines (paragraph
/// spacing limit 0, InDesign's default; basic-textframe: tfo-vj-justify). Footnotes are
/// placed after.
pub(super) fn justify_vertically(flow: &mut Flow<'_>, blocks: &[bool]) {
    for (fi, f) in flow.frames.iter_mut().enumerate() {
        // Frames with paragraph-column parcels are justified as one block (`vjustify`).
        if blocks.get(fi).copied().unwrap_or(false) {
            continue;
        }
        let vj = f.frame.vertical_justification;
        if !matches!(vj, VerticalJustification::Center | VerticalJustification::Bottom | VerticalJustification::Justify) {
            continue;
        }
        for (ci, col) in f.cols.iter().enumerate() {
            // InDesign's saved baselines have its vertical justification.
            if flow.saved_columns.contains(&(fi, ci)) {
                continue;
            }
            let used = f.lines.iter().filter(|l| l.col == ci).map(|l| l.line.baseline).fold(f64::MIN, f64::max);
            if used == f64::MIN {
                continue;
            }
            let free = col.bottom - used;
            if free <= 0.0 {
                continue;
            }
            if vj == VerticalJustification::Justify {
                let n = f.lines.iter().filter(|l| l.col == ci).count();
                if n < 2 {
                    continue;
                }
                let step = free / (n - 1) as f64;
                for (k, l) in f.lines.iter_mut().filter(|l| l.col == ci).enumerate() {
                    translate_line(&mut l.line, 0.0, step * k as f64);
                }
                continue;
            }
            let shift = if vj == VerticalJustification::Center { free / 2.0 } else { free };
            for l in f.lines.iter_mut().filter(|l| l.col == ci) {
                translate_line(&mut l.line, 0.0, shift);
            }
        }
    }
}

/// A filled quadrilateral along the segment `a`–`b`, `w` wide (a diagonal stroke).
pub(super) fn segment_shape(a: (f64, f64), b: (f64, f64), w: f64, paint: Paint, overprint: bool) -> Option<TextShape> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0 || w <= 0.0 || paint == Paint::None {
        return None;
    }
    let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
    let p = |x: f64, y: f64| {
        let a = folio_model::Point::new(x, y);
        folio_model::PathPoint { anchor: a, left: a, right: a }
    };
    let points = vec![p(a.0 + nx, a.1 + ny), p(b.0 + nx, b.1 + ny), p(b.0 - nx, b.1 - ny), p(a.0 - nx, a.1 - ny)];
    Some(TextShape { paths: vec![folio_model::SubPath { points, open: false }], paint, overprint, ..Default::default() })
}

impl<'c, 'a> Ctx<'c, 'a> {
    /// Lays out nested text (a table cell, a footnote) in one column `width` wide from
    /// y = 0: its lines (x from 0) with paragraph rules, shading and borders, and its depth to
    /// the last baseline. It hangs on story character `index`; its characters count from 0.
    pub(super) fn lay_nested(&self, paragraphs: &[Paragraph], width: f64, frame: &TextFrame, st: &mut TextState, index: usize, text_id: &str) -> (Vec<FlowLine>, f64) {
        let col = Column { left: 0.0, right: width.max(1.0), top: 0.0, bottom: 1.0e7, previous: None };
        let (lines, _) = self.lay_nested_columns(paragraphs, vec![col], frame, st, index, text_id);
        let depth = lines.iter().map(|l| l.line.baseline).fold(0.0, f64::max);
        (lines, depth)
    }

    /// Each nested column has its own width and starts at the origin. Line indices
    /// continue across columns, including when only part of a paragraph fits.
    pub(super) fn lay_nested_columns(&self, paragraphs: &[Paragraph], cols: Vec<Column>, frame: &TextFrame, st: &mut TextState, index: usize, text_id: &str) -> (Vec<FlowLine>, bool) {
        let col = cols[0];
        let ff = FlowFrame {
            facing: false,
            grid: None,
            id: "",
            frame,
            verso: false,
            base_columns: cols.len(),
            cols,
            start: 0,
            wraps: &[],
            shape: None,
            lines: Vec::new(),
            notes: Vec::new(),
            reserve: 0.0,
            area: Rect { left: col.left, top: col.top, right: col.right, bottom: col.bottom },
            page: None,
            vmap: None,
        };
        let mut flow = Flow::new(vec![ff], None);
        let mut sub = TextState::new(st.page_name.clone(), st.next_note, None);
        sub.list_labels = self.list_plan.0.get(text_id).cloned();
        sub.section_marker = st.section_marker.clone();
        sub.next_page = st.next_page.clone();
        sub.variables = st.variables.clone();
        sub.optical = st.optical;
        sub.anchor = Some(st.anchor.unwrap_or(index));
        let complete = self.lay_text(&mut flow, paragraphs, &mut sub, None);
        st.next_note = sub.next_note;
        st.missing.extend(sub.missing);
        st.table_rows.extend(sub.table_rows);
        for (key, m) in sub.missing_glyphs {
            st.add_missing_glyphs(key, m);
        }
        self.decorate(&mut flow, paragraphs);
        let lines = flow.frames.pop().map(|f| f.lines).unwrap_or_default();
        (lines, complete)
    }

    /// Paragraph rules, shading and borders of the laid paragraphs, as shapes on their lines.
    pub(super) fn decorate(&self, flow: &mut Flow<'_>, paragraphs: &[Paragraph]) {
        let any = paragraphs.iter().any(|p| {
            let a = &p.attrs;
            a.rule_above.is_some() || a.rule_below.is_some() || a.shading.is_some() || a.border.is_some()
        });
        if !any {
            return;
        }
        // Pieces: the lines of one paragraph in one column, in order.
        struct Piece {
            fi: usize,
            col: usize,
            pi: usize,
            first: usize,
            last: usize,
        }
        let mut pieces: Vec<Piece> = Vec::new();
        for (fi, f) in flow.frames.iter().enumerate() {
            for (li, l) in f.lines.iter().enumerate() {
                let Some(pi) = l.para else { continue };
                match pieces.last_mut() {
                    Some(pc) if pc.fi == fi && pc.col == l.col && pc.pi == pi => pc.last = li,
                    _ => pieces.push(Piece { fi, col: l.col, pi, first: li, last: li }),
                }
            }
        }
        let fill_of = |p: &Paragraph, last: bool| -> Paint {
            let mut runs = p.runs.iter().filter(|r| !r.text.is_empty());
            let r = if last { runs.next_back() } else { runs.next() };
            r.or(p.runs.last()).map(|r| r.attrs.fill.clone()).unwrap_or(Paint::None)
        };
        let mut adds: Vec<(usize, usize, TextShape)> = Vec::new();
        let n = pieces.len();
        let mut i = 0;
        while i < n {
            let pc = &pieces[i];
            let para = &paragraphs[pc.pi];
            let a = &para.attrs;
            let frame = &flow.frames[pc.fi];
            let col = frame.cols[pc.col];
            let first_piece = i == 0 || pieces[i - 1].pi != pc.pi;
            // Consecutive pieces sharing one bordered box.
            let mut j = i;
            if let Some(b) = a.border.as_ref().filter(|b| b.merge) {
                while j + 1 < n {
                    let nx = &pieces[j + 1];
                    let same_box = nx.fi == pc.fi
                        && nx.col == pc.col
                        && nx.pi == pieces[j].pi + 1
                        && paragraphs[nx.pi].attrs.border.as_ref() == Some(b);
                    if !same_box {
                        break;
                    }
                    j += 1;
                }
            }
            let group = &pieces[i..=j];
            let lines = |g: &[Piece]| -> Vec<&FlowLine> {
                g.iter().flat_map(|q| frame.lines[q.first..=q.last].iter().filter(|l| l.para == Some(q.pi))).collect()
            };
            let all = lines(group);
            let (Some(top_line), Some(bottom_line)) = (all.first(), all.last()) else {
                i = j + 1;
                continue;
            };
            let text_span = |ls: &[&FlowLine]| {
                let l = ls.iter().map(|l| l.text.0).fold(f64::MAX, f64::min);
                let r = ls.iter().map(|l| l.text.1).fold(f64::MIN, f64::max);
                (l, r)
            };
            let span = text_span(&all);
            let bx = |width: RuleWidth, top: TopOrigin, bottom: BottomOrigin, off: [f64; 4]| -> Rect {
                let (l, r) = match width {
                    RuleWidth::Column => (col.left, col.right),
                    RuleWidth::Text => span,
                };
                let t = top_line.line.baseline
                    - match top {
                        TopOrigin::Ascent => top_line.m.ascent,
                        TopOrigin::Baseline => 0.0,
                        TopOrigin::Leading => top_line.m.leading,
                    };
                let b = bottom_line.line.baseline
                    + match bottom {
                        BottomOrigin::Descent => bottom_line.m.descent,
                        BottomOrigin::Baseline => 0.0,
                    };
                Rect { left: l - off[1], top: t - off[0], right: r + off[3], bottom: b + off[2] }
            };
            let first_li = group[0].first;
            // Shading: the shared box when every paragraph of the group has the same shading,
            // else each paragraph's own.
            let shading_same = group.iter().all(|q| paragraphs[q.pi].attrs.shading == a.shading);
            if shading_same && let Some(s) = &a.shading {
                let r = bx(s.width, s.top, s.bottom, s.offsets);
                adds.extend(TextShape::rect(r.left, r.top, r.right, r.bottom, s.paint.clone(), s.overprint).map(|x| (pc.fi, first_li, x)));
            } else {
                for q in group {
                    let qa = &paragraphs[q.pi].attrs;
                    let Some(s) = &qa.shading else { continue };
                    let ls = lines(std::slice::from_ref(q));
                    let (Some(t), Some(b)) = (ls.first(), ls.last()) else { continue };
                    let (l, r) = match s.width {
                        RuleWidth::Column => (col.left, col.right),
                        RuleWidth::Text => text_span(&ls),
                    };
                    let top = t.line.baseline
                        - match s.top {
                            TopOrigin::Ascent => t.m.ascent,
                            TopOrigin::Baseline => 0.0,
                            TopOrigin::Leading => t.m.leading,
                        }
                        - s.offsets[0];
                    let bot = b.line.baseline + if s.bottom == BottomOrigin::Descent { b.m.descent } else { 0.0 } + s.offsets[2];
                    adds.extend(
                        TextShape::rect(l - s.offsets[1], top, r + s.offsets[3], bot, s.paint.clone(), s.overprint).map(|x| (pc.fi, q.first, x)),
                    );
                }
            }
            if let Some(b) = &a.border {
                let r = bx(b.width, b.top, b.bottom, b.offsets);
                let last_of_group = j + 1 == n || pieces[j + 1].pi != pieces[j].pi;
                let first_of_group = first_piece;
                let [wt, wl, wb, wr] = b.weights;
                let (top_on, bottom_on) = (first_of_group || b.display_if_splits, last_of_group || b.display_if_splits);
                let (y0, y1) = (if top_on { r.top - wt / 2.0 } else { r.top }, if bottom_on { r.bottom + wb / 2.0 } else { r.bottom });
                // A styled border with four equal edges is one box, its corners turned like
                // a frame's; other styled borders are edges between corners.
                let whole = top_on && bottom_on && wt > 0.0 && [wl, wb, wr].iter().all(|w| (w - wt).abs() < 1e-9);
                let boxed = whole
                    .then(|| strokes::box_shapes(r, wt, &b.stroke_type, &b.paint, b.overprint).or_else(|| strokes::solid_box(r, wt, &b.paint, b.overprint)))
                    .flatten();
                // a patterned border shows its gap colour under the pattern: a solid stroke of the same box (or, below, of each edge)
                let gap = b.gap_paint != Paint::None && b.paint != Paint::None && strokes::pattern(&b.stroke_type, wt.max(wl).max(wb).max(wr)).is_some();
                if gap && boxed.is_some() {
                    adds.extend(strokes::solid_box(r, wt, &b.gap_paint, b.gap_overprint).into_iter().flatten().map(|x| (pc.fi, first_li, x)));
                }
                let equal = wt > 0.0 && [wl, wb, wr].iter().all(|w| (w - wt).abs() < 1e-9);
                let edges = if boxed.is_none() && equal { strokes::solid_edges(r, wt, top_on, bottom_on, &b.stroke_type, &b.paint, b.overprint) } else { None };
                if let Some(shapes) = boxed.or(edges) {
                    adds.extend(shapes.into_iter().map(|x| (pc.fi, first_li, x)));
                } else {
                    let mut edge = |l: f64, t: f64, rr: f64, bb: f64, from: (f64, f64), to: (f64, f64), w: f64| {
                        if gap && strokes::pattern(&b.stroke_type, w).is_some() {
                            adds.extend(TextShape::rect(l, t, rr, bb, b.gap_paint.clone(), b.gap_overprint).map(|x| (pc.fi, first_li, x)));
                        }
                        match strokes::line_shapes(from, to, w, &b.stroke_type, Ends::Corners, &b.paint, b.overprint) {
                            Some(s) => adds.extend(s.into_iter().map(|x| (pc.fi, first_li, x))),
                            None => adds.extend(TextShape::rect(l, t, rr, bb, b.paint.clone(), b.overprint).map(|x| (pc.fi, first_li, x))),
                        }
                    };
                    if top_on && wt > 0.0 {
                        edge(r.left - wl / 2.0, r.top - wt / 2.0, r.right + wr / 2.0, r.top + wt / 2.0, (r.left, r.top), (r.right, r.top), wt);
                    }
                    if bottom_on && wb > 0.0 {
                        edge(r.left - wl / 2.0, r.bottom - wb / 2.0, r.right + wr / 2.0, r.bottom + wb / 2.0, (r.right, r.bottom), (r.left, r.bottom), wb);
                    }
                    if wl > 0.0 {
                        edge(r.left - wl / 2.0, y0, r.left + wl / 2.0, y1, (r.left, r.bottom), (r.left, r.top), wl);
                    }
                    if wr > 0.0 {
                        edge(r.right - wr / 2.0, y0, r.right + wr / 2.0, y1, (r.right, r.top), (r.right, r.bottom), wr);
                    }
                }
            }
            i = j + 1;
        }
        // Rules: above the paragraph's first line, below its last.
        for (k, pc) in pieces.iter().enumerate() {
            let qp = &paragraphs[pc.pi];
            let qa = &qp.attrs;
            if qa.rule_above.is_none() && qa.rule_below.is_none() {
                continue;
            }
            let frame = &flow.frames[pc.fi];
            let col = frame.cols[pc.col];
            let ls: Vec<(usize, &FlowLine)> =
                (pc.first..=pc.last).map(|li| (li, &frame.lines[li])).filter(|(_, l)| l.para == Some(pc.pi)).collect();
            let first_piece = k == 0 || pieces[k - 1].pi != pc.pi;
            let last_piece = k + 1 == n || pieces[k + 1].pi != pc.pi;
            if let (Some(rule), true, Some(&(li, l))) = (&qa.rule_above, first_piece, ls.first()) {
                let paint = rules::paint(rule, || fill_of(qp, false));
                let (x0, x1) = rules::span(rule, &col, l);
                let bottom = l.line.baseline - rule.offset;
                let mid = bottom - rule.weight / 2.0;
                if rule.gap_paint != Paint::None && strokes::pattern(&rule.stroke_type, rule.weight).is_some() {
                    adds.extend(TextShape::rect(x0, bottom - rule.weight, x1, bottom, rule.gap_paint.clone(), rule.gap_overprint).map(|x| (pc.fi, li, x)));
                }
                match strokes::line_shapes((x0, mid), (x1, mid), rule.weight, &rule.stroke_type, Ends::Free, &paint, rule.overprint) {
                    Some(s) => adds.extend(s.into_iter().map(|x| (pc.fi, li, x))),
                    None => adds.extend(TextShape::rect(x0, bottom - rule.weight, x1, bottom, paint, rule.overprint).map(|x| (pc.fi, li, x))),
                }
            }
            if let (Some(rule), true, Some(&(li, l))) = (&qa.rule_below, last_piece, ls.last()) {
                let paint = rules::paint(rule, || fill_of(qp, true));
                let (x0, x1) = rules::span(rule, &col, l);
                let top = l.line.baseline + rule.offset;
                let mid = top + rule.weight / 2.0;
                if rule.gap_paint != Paint::None && strokes::pattern(&rule.stroke_type, rule.weight).is_some() {
                    adds.extend(TextShape::rect(x0, top, x1, top + rule.weight, rule.gap_paint.clone(), rule.gap_overprint).map(|x| (pc.fi, li, x)));
                }
                match strokes::line_shapes((x0, mid), (x1, mid), rule.weight, &rule.stroke_type, Ends::Free, &paint, rule.overprint) {
                    Some(s) => adds.extend(s.into_iter().map(|x| (pc.fi, li, x))),
                    None => adds.extend(TextShape::rect(x0, top, x1, top + rule.weight, paint, rule.overprint).map(|x| (pc.fi, li, x))),
                }
            }
        }
        for (fi, li, shape) in adds {
            flow.frames[fi].lines[li].line.shapes.push(shape);
        }
    }
}
