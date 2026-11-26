//! Resolve paragraph matches before explicit character styles and range overrides.
use crate::{color::Colors, grep::Compiled, styles::{CharState, StyleSheet, apply_char}, warn::Warnings, xml::Element};
use folio_model::{marker, nested::NestedStyle, Paragraph, ParagraphStart};
use std::{ops::Range, sync::Arc};

pub(crate) struct Source<'a> {
    pub span: Range<usize>,
    pub base: CharState,
    pub range: Option<&'a Element<'a>>,
    pub rules: Arc<Vec<Compiled>>,
    pub nested: Arc<Vec<NestedStyle>>,
}

/// The characters of a paragraph's text as matching sees them: a marker run (page number, section marker, note reference, text
/// variable) stands for its character in `folio_model::marker`; an anchored object is U+FFFC and a table U+0016 as in the text.
fn marker_chars(p: &Paragraph) -> Vec<char> {
    let mut out = Vec::new();
    for r in &p.runs {
        match r.variable {
            Some(v) => out.extend(r.text.chars().map(|_| marker::of_variable(v))),
            // A U+FEFF is an index marker, or the anchor of an editorial note where the importer said so.
            None => for c in r.text.chars() {
                out.push(if c != '\u{FEFF}' { c } else if p.attrs.note_anchor_marks.contains(&out.len()) { marker::NOTE_ANCHOR } else { marker::INDEX });
            },
        }
    }
    out
}

/// What an expression or a nested style list is matched against: the paragraph's characters, the End Nested Style Characters put back
/// where they sit (`with_end`) and the paragraph's own return (`terminator`). `at[i]` is the offset into the paragraph's text where
/// character `i` stands (the last entry is the end of the text), to bring a range back to the text.
struct Subject { chars: Vec<char>, at: Vec<usize> }

/// Which invisible characters stay in the text an expression is matched against. GREP does not see them unless the expression names them
/// (measured, rules/specialchars-c1.ko.md); nested styles see them all.
#[derive(Clone, Copy, Default)]
struct Keep { end_nested: bool, discretionary_break: bool, index: bool, indent_tab: bool, note_anchor: bool }

impl Keep {
    const ALL: Keep = Keep { end_nested: true, discretionary_break: true, index: true, indent_tab: true, note_anchor: true };
    fn named_by(rx: &crate::grep::Regex) -> Keep {
        Keep { end_nested: rx.names_end_nested(), discretionary_break: rx.names_discretionary_break(), index: rx.names_index(), indent_tab: rx.names_indent_tab(), note_anchor: false }
    }
    fn any(&self) -> bool { self.end_nested || self.discretionary_break || self.index || self.indent_tab || self.note_anchor }
}

fn subject(base: &[char], marks: &[usize], terminator: bool, keep: Keep) -> Subject {
    let with_end = keep.end_nested;
    let mut chars = Vec::with_capacity(base.len() + marks.len() + 1);
    let mut at = Vec::with_capacity(base.len() + marks.len() + 2);
    let mut next = 0;
    for (i, &c) in base.iter().enumerate() {
        while with_end && next < marks.len() && marks[next] <= i { chars.push(marker::END_NESTED); at.push(i); next += 1; }
        // The discretionary line break (U+200B) is not seen unless the expression names it (measured, rules/specialchars-c1.ko.md).
        if c == '\u{200b}' && !keep.discretionary_break { continue; }
        if (c == marker::INDEX && !keep.index) || (c == marker::NOTE_ANCHOR && !keep.note_anchor) || (c == '\u{7}' && !keep.indent_tab) { continue; }
        chars.push(c);
        at.push(i);
    }
    while with_end && next < marks.len() { chars.push(marker::END_NESTED); at.push(base.len()); next += 1; }
    if terminator { chars.push('\r'); at.push(base.len()); }
    at.push(base.len());
    Subject { chars, at }
}

/// A match in a subject as a range of the paragraph's text: from its first to just past its last character of the text (an End
/// Nested Style Character at either edge is no part of it; one inside is).
fn model_range(sub: &Subject, m: Range<usize>) -> Option<Range<usize>> {
    let real = |i: &usize| sub.chars[*i] != marker::END_NESTED;
    let first = (m.start..m.end).find(real)?;
    let last = (m.start..m.end).rev().find(real)?;
    Some(sub.at[first]..sub.at[last] + 1)
}

pub(crate) fn apply(paragraphs: &mut [Paragraph], sources: &[Source<'_>], sheet: &StyleSheet<'_>, colors: &Colors, w: &mut Warnings) {
    if sources.is_empty() { return; }
    let mut paragraph_start = 0;
    let last = paragraphs.len().saturating_sub(1);
    for (index, p) in paragraphs.iter_mut().enumerate() {
        let text: String = p.runs.iter().map(|r|r.text.as_str()).collect();
        let length = text.chars().count();
        // InDesign matches the paragraph with its own return (CR): `[^a]` and `\s` take it, the last paragraph of a story has none.
        let base = marker_chars(p);
        let marks = p.attrs.end_nested_marks.clone();
        let term = if index != last { p.attrs.ends_with_break } else { ParagraphStart::Anywhere };
        let plain = subject(&base, &marks, index != last, Keep::default());
        let selected: Vec<_> = sources.iter().filter(|s|s.span.start < paragraph_start + length && s.span.end > paragraph_start).collect();
        let mut hits = Vec::new();
        for source in &selected {
            let mut ranges = Vec::new();
            if !source.nested.is_empty() {
                // Nested styles are laid out from the start of the paragraph. A GREP style is applied after them: in the exported
                // PDF it wins where they overlap (the DOM reports the nested style: rules/nested-styles.ko.md).
                let sub = subject(&base, &marks, false, Keep::ALL);
                let drop = p.attrs.drop_cap_characters as usize;
                let drop = if drop == 0 { 0 } else { sub.at[..sub.chars.len()].iter().position(|&a| a >= drop).unwrap_or(sub.chars.len()) };
                let found = crate::nested::spans_with(&source.nested, &sub.chars, drop);
                if let Some(name) = found.stopped_at { w.add(format!("nested style delimiter {name} is not modelled: the list stops there")); }
                for (span, index) in found.spans {
                    let start = (paragraph_start + sub.at[span.start]).max(source.span.start);
                    let end = (paragraph_start + sub.at[span.end]).min(source.span.end);
                    if start < end { ranges.push((start..end, source.nested[index].style.as_str())); }
                }
            }
            for rule in source.rules.iter() {
                // An expression that names the End Nested Style Character needs it in the text; any other does not see it (transparent).
                let keep = Keep::named_by(&rule.regex);
                let named = keep.any().then(|| subject(&base, &marks, index != last, keep));
                let sub = named.as_ref().unwrap_or(&plain);
                for m in rule.regex.find_all_in(&sub.chars, index == 0, term) {
                    let Some(m) = model_range(sub, m) else { continue };
                    let start = paragraph_start + m.start;
                    let end = paragraph_start + m.end.min(length);
                    let span = start.max(source.span.start)..end.min(source.span.end);
                    if span.start < span.end { ranges.push((span, rule.rule.style.as_str())); }
                }
            }
            hits.push((*source, ranges));
        }
        let mut at = paragraph_start;
        let mut runs = Vec::new();
        for run in std::mem::take(&mut p.runs) {
            let count = run.text.chars().count();
            let end = at + count;
            let mut cuts = vec![at, end];
            for (_, ranges) in &hits {
                for (span, _) in ranges {
                    if span.start < end && span.end > at {
                        cuts.push(span.start.max(at)); cuts.push(span.end.min(end));
                    }
                }
            }
            cuts.sort_unstable(); cuts.dedup();
            if cuts.len() <= 1 || hits.iter().all(|(_,rs)|rs.iter().all(|(r,_)|r.start >= end || r.end <= at)) {
                runs.push(run); at = end; continue;
            }
            // A marker (page number, note reference, variable instance, anchored object) is one character: it is styled whole. A longer run
            // may hold anchored objects and tables, one per U+FFFC / U+0016 in its text: they go with their characters into the pieces.
            let anchors = run.text.chars().filter(|&c| c == '\u{FFFC}').count();
            let tables = run.text.chars().filter(|&c| c == '\u{16}').count();
            let splits = run.variable.is_none() && run.note.is_none() && run.inline.len() == anchors && run.tables.len() == tables;
            if (run.variable.is_some() || !run.inline.is_empty() || !run.tables.is_empty() || run.note.is_some()) && count != 1 && !splits {
                w.add("GREP or nested style on a run of several characters with an inline object, table or variable is not modelled");
                runs.push(run); at = end; continue;
            }
            let bytes: Vec<_> = run.text.char_indices().map(|(i,_)|i).chain(std::iter::once(run.text.len())).collect();
            let (mut next_inline, mut next_table) = (0, 0);
            for cut in cuts.windows(2) {
                let mut piece = run.clone();
                piece.text = run.text[bytes[cut[0]-at]..bytes[cut[1]-at]].to_owned();
                if count != 1 && splits {
                    let (a, t) = (piece.text.chars().filter(|&c| c == '\u{FFFC}').count(), piece.text.chars().filter(|&c| c == '\u{16}').count());
                    piece.inline = run.inline.iter().skip(next_inline).take(a).cloned().collect();
                    piece.tables = run.tables.iter().skip(next_table).take(t).cloned().collect();
                    next_inline += a; next_table += t;
                }
                if let Some((source,ranges)) = hits.iter().find(|(s,_)|s.span.contains(&cut[0])) {
                    let styles: Vec<_> = ranges.iter().filter(|(span,_)|span.contains(&cut[0])).map(|(_,style)|*style).collect();
                    if !styles.is_empty() {
                        let mut state = source.base.clone();
                        for style in styles { sheet.apply_character_style(&mut state, Some(style), w); }
                        if let Some(range) = source.range {
                            sheet.apply_character_style(&mut state, range.attr("AppliedCharacterStyle"), w);
                            apply_char(&mut state, range, w);
                        }
                        let mut attrs = state.finish(colors, w);
                        attrs.character_style = piece.attrs.character_style.clone();
                        attrs.conditions = piece.attrs.conditions.clone();
                        attrs.auto_leading = piece.attrs.auto_leading;
                        piece.attrs = attrs;
                    }
                }
                runs.push(piece);
            }
            at = end;
        }
        p.runs = runs;
        if !p.attrs.line_styles.is_empty() {
            line_variants(p, &hits, paragraph_start, sheet, colors, w);
        }
        paragraph_start += length + 1;
    }
}

/// What each run looks like under each of the paragraph's nested line styles: the paragraph style's character state with
/// the line style, then the nested and GREP styles that cover the run, then the character style and overrides written
/// on its range (a character style applied by hand beats a line style: measured, rules/dropcap.ko.md). The layout
/// swaps these in for the lines each item covers, once it knows the lines.
type SourceHits<'a, 'b, 'c> = (&'a Source<'b>, Vec<(Range<usize>, &'c str)>);

fn line_variants(p: &mut Paragraph, hits: &[SourceHits<'_, '_, '_>], paragraph_start: usize, sheet: &StyleSheet<'_>, colors: &Colors, w: &mut Warnings) {
    let items = p.attrs.line_styles.clone();
    let mut at = paragraph_start;
    for run in &mut p.runs {
        let length = run.text.chars().count();
        // The paragraph return's run (no text) belongs to the last source.
        let found = hits.iter().find(|(s, _)| s.span.contains(&at)).or(hits.last());
        let Some((source, ranges)) = found else { at += length; continue };
        let mut variants = Vec::with_capacity(items.len());
        for item in &items {
            if item.repeat || item.style.is_empty() || crate::props::is_none_ref(&item.style) || item.style == crate::nested::NO_STYLE {
                variants.push(run.attrs.clone());
                continue;
            }
            let mut state = source.base.clone();
            sheet.apply_character_style(&mut state, Some(item.style.as_str()), w);
            for (span, style) in ranges.iter() {
                if span.contains(&at) { sheet.apply_character_style(&mut state, Some(style), w); }
            }
            if let Some(range) = source.range {
                sheet.apply_character_style(&mut state, range.attr("AppliedCharacterStyle"), w);
                apply_char(&mut state, range, w);
            }
            let mut attrs = state.finish(colors, w);
            attrs.character_style = run.attrs.character_style.clone();
            attrs.conditions = run.attrs.conditions.clone();
            attrs.auto_leading = run.attrs.auto_leading;
            variants.push(attrs);
        }
        run.line_attrs = variants;
        at += length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nested;
    use serde_json::Value;

    /// A measured story: its paragraphs as GREP and nested styles see them.
    struct Para { base: Vec<char>, marks: Vec<usize>, term: ParagraphStart }

    fn blank() -> Para { Para { base: Vec::new(), marks: Vec::new(), term: ParagraphStart::Anywhere } }

    /// The story InDesign was given: text and markers; a break character ends a paragraph. `None`: the case has a marker Folio's text
    /// does not model.
    fn story(parts: &Value) -> Option<Vec<Para>> {
        let mut out = Vec::new();
        let mut cur = blank();
        for part in parts.as_array().unwrap() {
            if let Some(t) = part.as_str() { cur.base.extend(t.chars()); continue; }
            let name = part["m"].as_str().unwrap();
            let ended = match name {
                "COLUMN_BREAK" => Some(ParagraphStart::NextColumn),
                "FRAME_BREAK" => Some(ParagraphStart::NextFrame),
                "PAGE_BREAK" => Some(ParagraphStart::NextPage),
                "ODD_PAGE_BREAK" => Some(ParagraphStart::NextOddPage),
                "EVEN_PAGE_BREAK" => Some(ParagraphStart::NextEvenPage),
                _ => None,
            };
            if let Some(kind) = ended {
                cur.term = kind;
                out.push(std::mem::replace(&mut cur, blank()));
                continue;
            }
            match name {
                "AUTO_PAGE_NUMBER" => cur.base.push(marker::PAGE),
                "NEXT_PAGE_NUMBER" => cur.base.push(marker::NEXT_PAGE),
                "PREVIOUS_PAGE_NUMBER" => cur.base.push(marker::PREVIOUS_PAGE),
                "SECTION_MARKER" => cur.base.push(marker::SECTION),
                "END_NESTED_STYLE" => cur.marks.push(cur.base.len()),
                "INDEX" => cur.base.push(marker::INDEX),
                "NOTE" => cur.base.push(marker::NOTE_ANCHOR),
                "INDENT_HERE_TAB" => cur.base.push('\u{7}'),
                "RIGHT_INDENT_TAB" => cur.base.push('\u{8}'),
                "DISCRETIONARY_LINE_BREAK" => cur.base.push('\u{200b}'),
                "FORCED_LINE_BREAK" => cur.base.push('\n'),
                "ZERO_WIDTH_JOINER" => cur.base.push('\u{200d}'),
                "ZERO_WIDTH_NONJOINER" => cur.base.push('\u{200c}'),
                "INLINE" => cur.base.push('\u{fffc}'),
                "FOOTNOTE" => cur.base.push(marker::NOTE),
                "VAR" => cur.base.push(marker::VARIABLE),
                _ => return None,
            }
        }
        out.push(cur);
        Some(out)
    }

    /// The story's characters as InDesign counts them: per paragraph the characters with the End Nested Style Characters in their places,
    /// and the return of every paragraph but the last. Gives, per paragraph, the index of each text offset in the whole story (the
    /// last entry: the paragraph's return) and the story's length.
    fn story_index(paras: &[Para]) -> (Vec<Vec<usize>>, usize) {
        let mut at = 0;
        let mut all = Vec::new();
        for (n, p) in paras.iter().enumerate() {
            let mut idx = Vec::new();
            let mut next = 0;
            for i in 0..p.base.len() {
                while next < p.marks.len() && p.marks[next] <= i { at += 1; next += 1; }
                idx.push(at);
                at += 1;
            }
            while next < p.marks.len() { at += 1; next += 1; }
            idx.push(at);
            if n + 1 < paras.len() { at += 1; }
            all.push(idx);
        }
        (all, at)
    }

    fn nested_items(items: &Value) -> Vec<NestedStyle> {
        items.as_array().unwrap().iter().map(|i| {
            let d = i["delim"].as_str().unwrap();
            let named = match d {
                "ANY_WORD" => "AnyWord", "ANY_CHARACTER" => "AnyCharacter", "SENTENCE" => "Sentence", "LETTERS" => "Letters", "DIGITS" => "Digits",
                "TABS" => "Tabs", "INLINE_GRAPHIC" => "InlineGraphic", "FORCED_LINE_BREAK" => "ForcedLineBreak", "END_NESTED_STYLE" => "EndNestedStyle",
                "INDENT_HERE_TAB" => "IndentHereTab", "EM_SPACE" => "EmSpace", "EN_SPACE" => "EnSpace", "NONBREAKING_SPACE" => "NonbreakingSpace",
                "AUTO_PAGE_NUMBER" => "AutoPageNumber", "SECTION_MARKER" => "SectionMarker", "REPEAT" => "Repeat", _ => "",
            };
            NestedStyle {
                style: format!("CharacterStyle/{}", i["style"].as_str().unwrap()),
                delimiter: if named.is_empty() { d.to_owned() } else { named.to_owned() },
                custom: named.is_empty(),
                repetition: i["rep"].as_u64().unwrap() as u32,
                inclusive: i["incl"].as_bool().unwrap(),
                source: None,
            }
        }).collect()
    }

    /// Cases where Folio and InDesign still differ (id prefix -> why).
    const KNOWN: &[(&str, &str)] = &[
        ("odd-", "InDesign takes `~I` oddly beside an anchor, a look-around or a top-level `|` (`^~I`, `(?<=a)~I`, `X|~I`, `~I|X` find only the marker or nothing: measured); Folio follows the ordinary rules"),
    ];

    /// What InDesign 2025 coloured and ended at, in every case of `goldens/specialchars-c1-markers/indesign.json` and
    /// `goldens/grepspell-c3-markers/indesign.json` (markers in GREP styles and in nested styles; measured with
    /// `indesign/specialchars-c1-probe.jsx`), replayed through the subject Folio builds.
    #[test]
    fn indesign_markers() {
        let (mut agree, mut skipped, mut known, mut differ) = (0, 0, 0, Vec::new());
        let mut cases = Vec::new();
        for name in ["specialchars-c1-markers", "grepspell-c3-markers"] {
            let path = format!("{}/../../../oracle/goldens/{name}/indesign.json", env!("CARGO_MANIFEST_DIR"));
            let golden: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("golden")).unwrap();
            cases.extend(golden["cases"].as_array().unwrap().iter().cloned());
        }
        for case in &cases {
            let (id, mode, want) = (case["id"].as_str().unwrap(), case["mode"].as_str().unwrap(), case["mask"].as_str().unwrap());
            if mode == "plain" { continue; }
            let Some(paras) = story(&case["parts"]) else { skipped += 1; continue };
            let (index, total) = story_index(&paras);
            let mut got = vec!['-'; total];
            if mode == "grep" {
                let pattern = case["pattern"].as_str().unwrap();
                match crate::grep::compile(pattern) {
                    Err(_) => { if want.contains('R') { got = vec!['!'; total]; } }
                    Ok(rx) => {
                        let mut first = 0;
                        for (n, p) in paras.iter().enumerate() {
                            let last = n + 1 == paras.len();
                            let term = if last { ParagraphStart::Anywhere } else { p.term };
                            let keep = Keep::named_by(&rx);
                            let named = keep.end_nested;
                            let sub = subject(&p.base, &p.marks, !last, keep);
                            for m in rx.find_all_in(&sub.chars, n == 0, term) {
                                // story index of a subject character
                                let at = |i: usize| -> usize { if named { first + i } else { index[n][sub.at[i]] } };
                                got[at(m.start)..=at(m.end - 1)].fill('R');
                            }
                            first = index[n].last().unwrap() + 1;
                        }
                    }
                }
            } else {
                let items = nested_items(&case["items"]);
                let p = &paras[0];
                let sub = subject(&p.base, &p.marks, false, Keep::ALL);
                for (span, i) in nested::spans_with(&items, &sub.chars, 0).spans {
                    let letter = items[i].style.rsplit('/').next().unwrap().chars().next().unwrap();
                    for x in span { got[x] = letter; }
                }
            }
            let got: String = got.into_iter().collect();
            if got == want { agree += 1; }
            else if KNOWN.iter().any(|(k, _)| id.starts_with(k)) { known += 1; }
            else { differ.push(format!("{id} {:?} {}\n   InDesign {want}\n   Folio    {got}", case["pattern"].as_str().unwrap_or(""), case["parts"])); }
        }
        eprintln!("markers table: {agree} agree, {known} known differences, {skipped} skipped, {} unexpected", differ.len());
        assert!(differ.is_empty(), "{}", differ.join("\n"));
    }
}
