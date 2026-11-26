//! Nested line styles (Paragraph panel menu > Drop Caps and Nested Styles > Nested Line Styles): the paragraph's lines, from
//! the top, get character styles. The styles change the glyph widths and so the line breaks, so a paragraph is laid out,
//! its lines read, the styles put on the characters of those lines, and it is laid out again until the lines stop changing
//! (measured with InDesign 2025: next/oracle/rules/dropcap.ko.md).
use super::*;
use folio_model::nested::NestedLineStyle;

/// Laying a paragraph again after the styles moved a line break: at most this many times (more than InDesign needed in any
/// measured case; a paragraph that never settles keeps its last layout).
pub(super) const MAX_PASSES: usize = 6;
/// Lines settled one at a time before the rest of the paragraph is left without line styles.
pub(super) const MAX_LINES: usize = 40;

/// The item of the paragraph's nested line styles that each character is under (`None`: none), given the character
/// offset each line starts at (the first is 0) and the paragraph's character count. One entry more than there are
/// characters, for the paragraph return, which belongs to the last line (measured: the return takes the last line's style).
pub(super) fn assignment(items: &[NestedLineStyle], line_starts: &[usize], chars: usize) -> Vec<Option<usize>> {
    let per_line = NestedLineStyle::per_line(items, line_starts.len().max(1));
    (0..=chars).map(|at| {
        let line = line_starts.partition_point(|&s| s <= at).saturating_sub(1);
        per_line.get(line).copied().flatten()
    }).collect()
}

/// The paragraph with each run (split where the item changes) in the attributes its line style gives it. Runs that are
/// one object (a variable, an anchored object, a note) are not split.
pub(super) fn restyle(para: &Paragraph, assign: &[Option<usize>]) -> Paragraph {
    let mut out = Paragraph { style: para.style.clone(), attrs: para.attrs.clone(), runs: Vec::with_capacity(para.runs.len() + 2) };
    let mut at = 0usize;
    for run in &para.runs {
        let n = run.text.chars().count();
        let attrs_of = |item: Option<usize>| item.and_then(|i| run.line_attrs.get(i)).unwrap_or(&run.attrs).clone();
        let atomic = n <= 1 || run.variable.is_some() || !run.inline.is_empty() || !run.tables.is_empty() || run.note.is_some();
        if run.line_attrs.is_empty() || atomic {
            let mut piece = run.clone();
            piece.attrs = attrs_of(assign.get(at).copied().flatten());
            piece.line_attrs = Vec::new();
            out.runs.push(piece);
        } else {
            let chars: Vec<char> = run.text.chars().collect();
            let mut from = 0usize;
            while from < n {
                let item = assign.get(at + from).copied().flatten();
                let mut to = from + 1;
                while to < n && assign.get(at + to).copied().flatten() == item { to += 1; }
                let mut piece = run.clone();
                piece.text = chars[from..to].iter().collect();
                piece.attrs = attrs_of(item);
                piece.line_attrs = Vec::new();
                out.runs.push(piece);
                from = to;
            }
        }
        at += n;
    }
    out
}

/// The styles while the lines are settled one at a time: the lines before `pins.len()` are fixed (the line after pin `i`
/// starts at `pins[i]`) and each gets its item; the line being settled and all the text after it are set in its style, so
/// that its break is found with every character on it measured in that style. Past `max` lines, or with no item for the
/// line, the text stays plain.
pub(super) fn tentative(items: &[NestedLineStyle], pins: &[usize], chars: usize, max: usize) -> Vec<Option<usize>> {
    let m = pins.len();
    let per_line = NestedLineStyle::per_line(items, m + 1);
    let mut starts = vec![0usize];
    starts.extend_from_slice(pins);
    (0..=chars).map(|at| {
        let line = starts.partition_point(|&s| s <= at).saturating_sub(1);
        if line >= max { None } else { per_line[line] }
    }).collect()
}
