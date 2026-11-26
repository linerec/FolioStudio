//! Nested styles of a paragraph style or paragraph: reading the list and finding the character ranges
//! its items cover. Measured with InDesign 2025 (rules/nested-styles.ko.md).
use crate::xml::Element;
use folio_model::marker;
use folio_model::nested::{NestedLineStyle, NestedStyle};
use std::ops::Range;

/// `CharacterStyle/$ID/[No character style]`: an item that only consumes text.
pub const NO_STYLE: &str = "CharacterStyle/$ID/[No character style]";

/// Delimiters this crate can evaluate; the others are kept as written but styled nothing.
pub const NAMED: &[&str] = &[
    "Dropcap", "AnyCharacter", "AnyWord", "Sentence", "Letters", "Digits", "Tabs", "ForcedLineBreak", "IndentHereTab",
    "EmSpace", "EnSpace", "NonbreakingSpace", "AutoPageNumber", "SectionMarker", "InlineGraphic", "EndNestedStyle", "Repeat",
];
/// The drop cap's character style is the list's first item with this delimiter: it styles the paragraph's first
/// `DropCapCharacters` characters whatever the drop cap's lines, and the items after it start behind them
/// (measured: rules/dropcap.ko.md).
pub const DROP_CAP: &str = "Dropcap";

/// The list written in `style`'s own properties, if it has one. An empty list that replaces an inherited one
/// is `EmptyNestedStyles="true"` with no `AllNestedStyles`.
pub(crate) fn read(style: &Element<'_>) -> Option<Vec<NestedStyle>> {
    let list = style.child("Properties").and_then(|p| p.child("AllNestedStyles"));
    if let Some(list) = list {
        return Some(items(list));
    }
    (style.attr("EmptyNestedStyles") == Some("true")).then(Vec::new)
}

/// A paragraph range's own list (a local override), which always lists every item.
pub(crate) fn read_local(range: &Element<'_>) -> Option<Vec<NestedStyle>> {
    range.child("Properties").and_then(|p| p.child("AllNestedStyles")).map(items)
}

/// The nested line styles written in `style`'s own properties (`AllLineStyles`), if it has any. An empty list that
/// replaces an inherited one is `EmptyLineStyles="true"` with no `AllLineStyles` (measured: rules/dropcap.ko.md).
pub(crate) fn read_lines(style: &Element<'_>) -> Option<Vec<NestedLineStyle>> {
    let list = style.child("Properties").and_then(|p| p.child("AllLineStyles"));
    if let Some(list) = list {
        return Some(line_items(list));
    }
    (style.attr("EmptyLineStyles") == Some("true")).then(Vec::new)
}

/// A paragraph range's own line styles (a local override), which always lists every item.
pub(crate) fn read_local_lines(range: &Element<'_>) -> Option<Vec<NestedLineStyle>> {
    range.child("Properties").and_then(|p| p.child("AllLineStyles")).map(line_items)
}

/// `ListItem` records: `AppliedCharacterStyle`, `LineCount`, and `RepeatLast` (-1, or the item count of a `[Repeat]`).
fn line_items(list: &Element<'_>) -> Vec<NestedLineStyle> {
    let number = |item: &Element<'_>, name: &str| item.child(name).and_then(|n| n.text().trim().parse::<i64>().ok());
    list.children_named("ListItem").enumerate().map(|(index, item)| {
        let repeat_last = number(item, "RepeatLast").unwrap_or(-1);
        let repeat = repeat_last >= 0;
        NestedLineStyle {
            style: item.child("AppliedCharacterStyle").map(Element::text).unwrap_or_default(),
            lines: if repeat { repeat_last as u32 } else { number(item, "LineCount").unwrap_or(1).max(0) as u32 },
            repeat,
            source: Some(index),
        }
    }).collect()
}

/// A paragraph style's nested line styles, for tests and the editor.
pub fn line_rules(source: &str, id: &str) -> Result<Vec<NestedLineStyle>, String> {
    let root = crate::xml::parse(source).map_err(|e| e.0)?;
    let mut styles = Vec::new();
    root.descendants_named("ParagraphStyle", &mut styles);
    let style = styles.into_iter().find(|s| s.attr("Self") == Some(id)).ok_or("paragraph style not found")?;
    Ok(read_lines(style).unwrap_or_default())
}

/// Refuses what could never be applied: more items than InDesign takes, a `[Repeat]` of nothing or of more items
/// than precede it.
pub fn check_lines(items: &[NestedLineStyle]) -> Result<(), String> {
    for (index, item) in items.iter().enumerate() {
        if item.repeat {
            if item.lines < 1 || item.lines as usize > index { return Err("[Repeat] must repeat 1 up to the number of items before it".into()); }
        } else if item.lines > 9999 {
            return Err("a nested line style count must be 0..9999".into());
        }
    }
    Ok(())
}

fn items(list: &Element<'_>) -> Vec<NestedStyle> {
    list.children_named("ListItem").enumerate().map(|(index, item)| {
        let delimiter = item.child("Delimiter");
        NestedStyle {
            style: item.child("AppliedCharacterStyle").map(Element::text).unwrap_or_default(),
            delimiter: delimiter.map(Element::text).unwrap_or_else(|| "AnyWord".into()),
            custom: delimiter.and_then(|d| d.attr("type")) == Some("string"),
            repetition: item.child("Repetition").and_then(|r| r.text().trim().parse().ok()).unwrap_or(1),
            inclusive: item.child("Inclusive").map(|i| i.text().trim() != "false").unwrap_or(true),
            source: Some(index),
        }
    }).collect()
}

pub fn rules(source: &str, id: &str) -> Result<Vec<NestedStyle>, String> {
    let root = crate::xml::parse(source).map_err(|e| e.0)?;
    let mut styles = Vec::new();
    root.descendants_named("ParagraphStyle", &mut styles);
    let style = styles.into_iter().find(|s| s.attr("Self") == Some(id)).ok_or("paragraph style not found")?;
    Ok(read(style).unwrap_or_default())
}

/// Refuses what could never be applied: a count of 0, a `Repeat` that repeats more than precedes it,
/// an empty user character set.
pub fn check(items: &[NestedStyle]) -> Result<(), String> {
    for (index, item) in items.iter().enumerate() {
        if item.repetition < 1 || item.repetition > 9999 { return Err("a nested style count must be 1..9999".into()); }
        if item.custom {
            if item.delimiter.is_empty() || item.delimiter.chars().any(char::is_control) {
                return Err("a nested style's own delimiter needs printable characters".into());
            }
        } else if !NAMED.contains(&item.delimiter.as_str()) {
            return Err(format!("unknown nested style delimiter {}", item.delimiter));
        }
        if !item.custom && item.delimiter == DROP_CAP && index != 0 {
            return Err("the drop cap's character style must be the first item".into());
        }
        if !item.custom && item.delimiter == "Repeat" && item.repetition as usize > index {
            return Err("[Repeat] cannot repeat more items than precede it".into());
        }
    }
    Ok(())
}

/// Result of laying the items over one paragraph's characters.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Spans {
    /// Character range and index of the item that styles it (items that only consume text are left out).
    pub spans: Vec<(Range<usize>, usize)>,
    /// A delimiter this crate cannot find in Folio's text stopped the list there.
    pub stopped_at: Option<String>,
}

pub fn spans(items: &[NestedStyle], text: &[char]) -> Spans { spans_with(items, text, 0) }

/// `spans` for a paragraph whose first `drop` characters are its drop cap.
pub fn spans_with(items: &[NestedStyle], text: &[char], drop: usize) -> Spans {
    let mut out = Spans::default();
    let mut at = 0usize;
    let mut index = 0usize;
    while index < items.len() && at < text.len() {
        let item = &items[index];
        if !item.custom && item.delimiter == "Repeat" {
            let count = item.repetition as usize;
            if count == 0 || count > index { break; }
            // The items before it, again and again, to the end of the paragraph. Items after a repeat never run.
            'cycle: loop {
                let before = at;
                for (offset, repeated) in items[index - count..index].iter().enumerate() {
                    let j = index - count + offset;
                    if at >= text.len() { break 'cycle; }
                    if repeated.delimiter == "Repeat" && !repeated.custom { break 'cycle; }
                    match step(repeated, text, at, drop) {
                        Ok((end, stop)) => { push(&mut out, at, end, j, repeated); at = end; if stop { break 'cycle; } }
                        Err(name) => { out.stopped_at = Some(name); break 'cycle; }
                    }
                }
                if at == before { break; }
            }
            break;
        }
        match step(item, text, at, drop) {
            Ok((end, stop)) => { push(&mut out, at, end, index, item); at = end; if stop { break; } }
            Err(name) => { out.stopped_at = Some(name); break; }
        }
        index += 1;
    }
    out
}

fn push(out: &mut Spans, start: usize, end: usize, index: usize, item: &NestedStyle) {
    if end > start && item.style != NO_STYLE && !item.style.is_empty() { out.spans.push((start..end, index)); }
}

/// Where `item` ends when it starts at `at`: the `repetition`-th delimiter from there, to the end of the
/// paragraph when there are fewer. The second value is true when nothing after the item is applied any more.
///
/// An End Nested Style Character (`marker::END_NESTED` in `text`; measured, rules/specialchars-c1.ko.md) ends the item that is
/// scanning when the scan gets to it, and the item takes it in (through or up to alike). A scan "gets to it" when the character is
/// at or before the last character the scan looked at: the delimiter itself (the n-th character, a tab, a sentence's end) or, for a
/// word that is taken through its white space, the character after that white space. After an item that a count of characters or
/// of letters was cut short that way, the list is over.
fn step(item: &NestedStyle, text: &[char], at: usize, drop: usize) -> Result<(usize, bool), String> {
    let n = item.repetition.max(1) as usize;
    let inc = item.inclusive;
    // (end, the last character looked at, whether the list is over when an End Nested Style Character cuts the item short)
    let (end, scan, stops): (usize, usize, bool) = if item.custom {
        let set: Vec<char> = item.delimiter.chars().collect();
        by_char(text, at, n, inc, |c| set.contains(&c), false)
    } else {
        match item.delimiter.as_str() {
            DROP_CAP => return Ok(((at + drop).min(text.len()), false)),
            // The index marker and an editorial note's anchor are no characters to count (measured, rules/specialchars-c1.ko.md, grepspell-c3).
            "AnyCharacter" => by_char(text, at, n, inc, |c| c != marker::INDEX && c != marker::NOTE_ANCHOR, true),
            "Letters" => by_char(text, at, n, inc, char::is_alphabetic, true),
            "Digits" => by_char(text, at, n, inc, |c| c.is_ascii_digit(), false),
            "Tabs" => by_char(text, at, n, inc, |c| c == '\t', false),
            "ForcedLineBreak" => by_char(text, at, n, inc, |c| c == '\n', false),
            "IndentHereTab" => by_char(text, at, n, inc, |c| c == '\u{7}', false),
            "EmSpace" => by_char(text, at, n, inc, |c| c == '\u{2003}', false),
            "EnSpace" => by_char(text, at, n, inc, |c| c == '\u{2002}', false),
            "NonbreakingSpace" => by_char(text, at, n, inc, |c| c == '\u{a0}', false),
            "AutoPageNumber" => by_char(text, at, n, inc, marker::is_page_number, false),
            "SectionMarker" => by_char(text, at, n, inc, |c| c == marker::SECTION, false),
            "InlineGraphic" => by_char(text, at, n, inc, |c| c == '\u{fffc}', false),
            // Through or up to alike, the character itself is part of the item (measured).
            "EndNestedStyle" => by_char(text, at, n, true, |c| c == marker::END_NESTED, false),
            "AnyWord" => { let (e, s) = by_word(text, at, n, inc); (e, s, false) }
            "Sentence" => { let (e, s) = by_sentence(text, at, n, inc); (e, s, false) }
            other => return Err(other.to_owned()),
        }
    };
    // The End Nested Style Character as the item's own delimiter is counted like any delimiter.
    if !item.custom && item.delimiter == "EndNestedStyle" { return Ok((end, false)); }
    if let Some(m) = (at..text.len()).find(|&i| text[i] == marker::END_NESTED) && m <= scan {
        return Ok((m + 1, stops));
    }
    Ok((end, false))
}

/// The delimiter is the character itself: "through" ends after the n-th, "up to" before it. Also gives the index of that
/// character (the last one looked at; the end of the text when there are fewer) and `stops` as it was passed.
fn by_char(text: &[char], at: usize, n: usize, inclusive: bool, hit: impl Fn(char) -> bool, stops: bool) -> (usize, usize, bool) {
    let mut seen = 0;
    for (i, &c) in text.iter().enumerate().skip(at) {
        if hit(c) {
            seen += 1;
            if seen == n { return (if inclusive { i + 1 } else { i }, i, stops); }
        }
    }
    (text.len(), text.len(), stops)
}

/// White space that separates words: a no-break space does not.
fn is_space(c: char) -> bool { c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{202f}' | '\u{2007}') }

/// Hangul syllables are a word each (measured with Latin text around them).
fn is_single(c: char) -> bool { matches!(c, '\u{ac00}'..='\u{d7a3}' | '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}') }

/// A word is white space, the characters up to the next white space, and the white space after them
/// ("through" takes all of it, "up to" stops before the trailing white space). Also gives the last character looked at:
/// through, the one after the trailing white space; up to, the white space that ends the word.
fn by_word(text: &[char], at: usize, n: usize, inclusive: bool) -> (usize, usize) {
    let mut i = at;
    for k in 1..=n {
        while i < text.len() && is_space(text[i]) { i += 1; }
        if i >= text.len() { return (text.len(), text.len()); }
        if is_single(text[i]) { i += 1; } else {
            while i < text.len() && !is_space(text[i]) && !is_single(text[i]) { i += 1; }
        }
        let token_end = i;
        while i < text.len() && is_space(text[i]) { i += 1; }
        if k == n { return if inclusive { (i, i) } else { (token_end, token_end) }; }
    }
    (i, i)
}

/// A sentence ends with `.` `?` `!` (then closing quotes or brackets) followed by white space, or with a
/// full-width `。` `！` `？` that needs none; the delimiter is the white space after it. Also gives the last character looked at:
/// through, the last white space; up to, the character after the closing marks.
fn by_sentence(text: &[char], at: usize, n: usize, inclusive: bool) -> (usize, usize) {
    let closer = |c: char| matches!(c, '"' | '\'' | '\u{201d}' | '\u{2019}' | '\u{bb}' | '\u{203a}' | ')' | ']' | '}' | '\u{300d}' | '\u{300f}' | '\u{ff09}');
    let ascii_end = |c: char| matches!(c, '.' | '?' | '!');
    let wide_end = |c: char| matches!(c, '\u{3002}' | '\u{ff01}' | '\u{ff1f}');
    let mut seen = 0;
    let mut i = at;
    while i < text.len() {
        let c = text[i];
        if ascii_end(c) || wide_end(c) {
            let mut j = i;
            while j < text.len() && (ascii_end(text[j]) || wide_end(text[j])) { j += 1; }
            while j < text.len() && closer(text[j]) { j += 1; }
            let mut after = j;
            while after < text.len() && is_space(text[after]) { after += 1; }
            let wide = text[i..j].iter().any(|&c| wide_end(c));
            if after > j || wide {
                seen += 1;
                if seen == n { return if inclusive { (after, after.saturating_sub(1)) } else { (j, j) }; }
                i = after;
                continue;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    (text.len(), text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(style: &str, delimiter: &str, repetition: u32, inclusive: bool) -> NestedStyle {
        NestedStyle { style: format!("CharacterStyle/{style}"), delimiter: delimiter.into(), custom: false, repetition, inclusive, source: None }
    }
    fn custom(style: &str, set: &str, repetition: u32, inclusive: bool) -> NestedStyle {
        NestedStyle { custom: true, ..item(style, set, repetition, inclusive) }
    }
    /// One letter per character: the style of the item that covers it, `-` for none.
    fn map(items: &[NestedStyle], text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let out = spans(items, &chars);
        let mut line = vec!['-'; chars.len()];
        for (range, i) in out.spans {
            let letter = items[i].style.rsplit('/').next().unwrap().chars().next().unwrap();
            for c in range { line[c] = letter; }
        }
        line.into_iter().collect()
    }

    // Every expectation below is InDesign 2025's per-character DOM answer (artifacts/nested-c1-20261023/measurement).
    #[test]
    fn counts_and_words() {
        let a = |d: &str, n, inc| vec![item("A", d, n, inc)];
        assert_eq!(map(&a("AnyCharacter", 3, true), "Hello world"), "AAA--------");
        assert_eq!(map(&a("AnyCharacter", 3, false), "Hello world"), "AA---------");
        assert_eq!(map(&a("AnyCharacter", 1, false), "abc"), "---");
        assert_eq!(map(&a("AnyWord", 1, true), "The quick brown fox."), "AAAA----------------");
        assert_eq!(map(&a("AnyWord", 1, false), "The quick brown fox."), "AAA-----------------");
        assert_eq!(map(&a("AnyWord", 2, true), "The quick brown fox."), "AAAAAAAAAA----------");
        assert_eq!(map(&a("AnyWord", 2, false), "The quick brown fox."), "AAAAAAAAA-----------");
        assert_eq!(map(&a("AnyWord", 2, true), "The quick, brown-fox  jumps."), "AAAAAAAAAAA-----------------");
        assert_eq!(map(&a("AnyWord", 1, true), "  Lead word here"), "AAAAAAA---------");
        assert_eq!(map(&a("AnyWord", 1, false), "  Lead word"), "AAAAAA-----");
        assert_eq!(map(&a("AnyWord", 1, true), "ab\tcd ef"), "AAA-----");
        assert_eq!(map(&a("AnyWord", 1, true), "ab\ncd ef"), "AAA-----");
        assert_eq!(map(&a("AnyWord", 1, true), "ab\u{2003}cd ef"), "AAA-----");
        assert_eq!(map(&a("AnyWord", 1, true), "ab\u{a0}cd ef"), "AAAAAA--");
        assert_eq!(map(&a("AnyWord", 1, true), "ab  cd ef"), "AAAA-----");
        assert_eq!(map(&a("AnyWord", 1, false), "ab  cd ef"), "AA-------");
        assert_eq!(map(&a("AnyWord", 2, true), "ab  cd ef"), "AAAAAAA--");
        assert_eq!(map(&a("AnyWord", 3, true), "ab cd"), "AAAAA");
        assert_eq!(map(&a("AnyWord", 2, true), "a - b c"), "AAAA---");
        assert_eq!(map(&a("AnyWord", 1, true), "a-b c"), "AAAA-");
        assert_eq!(map(&a("AnyWord", 1, true), "a\u{2014}b c"), "AAAA-");
        assert_eq!(map(&a("AnyWord", 2, true), "12 3.5 x"), "AAAAAAA-");
        // Hangul: a word per syllable
        assert_eq!(map(&a("AnyWord", 1, true), "\u{c548}\u{b155} \u{d558}\u{c138}\u{c694} \u{c138}\u{ac8c}"), "A--------");
        assert_eq!(map(&a("AnyWord", 2, true), "\u{c548}\u{b155} \u{d558}\u{c138}\u{c694} \u{c138}\u{ac8c}"), "AAA------");
        assert_eq!(map(&a("AnyWord", 2, true), "abc \u{d55c}\u{ae00} def"), "AAAAA-----");
    }

    #[test]
    fn sentences() {
        let a = |n, inc| vec![item("A", "Sentence", n, inc)];
        assert_eq!(map(&a(1, true), "One two. Three four! Five"), "AAAAAAAAA----------------");
        assert_eq!(map(&a(1, false), "One two. Three four! Five"), "AAAAAAAA-----------------");
        assert_eq!(map(&a(2, true), "One two. Three four! Five"), "AAAAAAAAAAAAAAAAAAAAA----");
        let t = "Mr. Smith went. 3.5 apples? Yes!! Ok... Done";
        assert_eq!(map(&a(1, true), t).matches('A').count(), 4);
        assert_eq!(map(&a(3, true), t).matches('A').count(), 28);
        assert_eq!(map(&a(4, true), t).matches('A').count(), 34);
        assert_eq!(map(&a(5, true), t).matches('A').count(), 40);
        assert_eq!(map(&a(1, true), "He said \"hi.\" Next one. Third").matches('A').count(), 14);
        assert_eq!(map(&a(1, true), "(See this.) Next. Third").matches('A').count(), 12);
        assert_eq!(map(&a(1, true), "Really?\u{201d} Next. Third").matches('A').count(), 9);
        assert_eq!(map(&a(1, true), "End.Next one. Third").matches('A').count(), 14);
        assert_eq!(map(&a(1, true), "End.\tNext one. Third").matches('A').count(), 5);
        assert_eq!(map(&a(1, true), "One.  Two. Three").matches('A').count(), 6);
        assert_eq!(map(&a(1, false), "One.  Two. Three").matches('A').count(), 4);
        assert_eq!(map(&a(1, true), "One two. three four. Five").matches('A').count(), 9);
        assert_eq!(map(&a(1, true), "One\ntwo. Three").matches('A').count(), 9);
        assert_eq!(map(&a(1, true), "One: two; three. Four").matches('A').count(), 17);
        assert_eq!(map(&a(1, true), "Wait\u{2026} then go. Four").matches('A').count(), 15);
        assert_eq!(map(&a(2, false), "One two. Three").matches('A').count(), 14);
        assert_eq!(map(&a(1, true), "\u{c548}\u{b155}\u{3002}\u{bc18}\u{ac11}").matches('A').count(), 3);
        assert_eq!(map(&a(1, true), "\u{c548}\u{b155}\u{ff01}\u{b05d} \u{b610}").matches('A').count(), 3);
        assert_eq!(map(&a(2, true), "\u{4eca}\u{65e5}\u{306f}\u{3002}\u{660e}\u{65e5}\u{ff1f}\u{3042}").matches('A').count(), 7);
    }

    #[test]
    fn letters_digits_and_characters() {
        let a = |d: &str, n, inc| vec![item("A", d, n, inc)];
        assert_eq!(map(&a("Letters", 2, true), "a1b2c3d4e5"), "AAA-------");
        assert_eq!(map(&a("Letters", 2, false), "a1b2c3d4e5"), "AA--------");
        assert_eq!(map(&a("Letters", 3, true), "ab12 cd34"), "AAAAAA---");
        assert_eq!(map(&a("Letters", 1, true), "12ab"), "AAA-");
        assert_eq!(map(&a("Letters", 1, false), "12ab"), "AA--");
        assert_eq!(map(&a("Letters", 3, true), "\u{e9}a\u{ac00}\u{b098}b"), "AAA--");
        assert_eq!(map(&a("Digits", 2, true), "a1b2c3d4e5"), "AAAA------");
        assert_eq!(map(&a("Digits", 2, false), "a1b2c3d4e5"), "AAA-------");
        assert_eq!(map(&a("Digits", 1, true), "abcdef"), "AAAAAA");
        assert_eq!(map(&a("AnyCharacter", 2, true), "a\u{1f600}b"), "AA-");
    }

    #[test]
    fn single_characters_and_user_sets() {
        let a = |d: &str, n, inc| vec![item("A", d, n, inc)];
        assert_eq!(map(&a("Tabs", 1, true), "a\tb\tc"), "AA---");
        assert_eq!(map(&a("Tabs", 1, false), "a\tb\tc"), "A----");
        assert_eq!(map(&a("Tabs", 2, true), "a\tb\tc"), "AAAA-");
        assert_eq!(map(&a("Tabs", 1, true), "abc def"), "AAAAAAA");
        assert_eq!(map(&a("Tabs", 3, true), "a\tb\tc"), "AAAAA");
        assert_eq!(map(&a("ForcedLineBreak", 1, true), "ab\ncd\nef"), "AAA-----");
        assert_eq!(map(&a("ForcedLineBreak", 1, false), "ab\ncd\nef"), "AA------");
        assert_eq!(map(&a("IndentHereTab", 1, true), "ab\u{7}cd"), "AAA--");
        assert_eq!(map(&a("IndentHereTab", 1, false), "ab\u{7}cd"), "AA---");
        assert_eq!(map(&a("EmSpace", 1, true), "ab\u{2003}cd"), "AAA--");
        assert_eq!(map(&a("EmSpace", 1, false), "ab\u{2003}cd"), "AA---");
        assert_eq!(map(&a("EnSpace", 1, true), "ab\u{2002}cd"), "AAA--");
        assert_eq!(map(&a("NonbreakingSpace", 1, true), "ab\u{a0}cd"), "AAA--");
        assert_eq!(map(&a("NonbreakingSpace", 1, false), "ab\u{a0}cd"), "AA---");
        assert_eq!(map(&[custom("A", ":", 1, true)], "Name: value: x"), "AAAAA---------");
        assert_eq!(map(&[custom("A", ":", 1, false)], "Name: value: x"), "AAAA----------");
        assert_eq!(map(&[custom("A", ":", 2, true)], "Name: value: x"), "AAAAAAAAAAAA--");
        assert_eq!(map(&[custom("A", ";:", 1, true)], "a;b:c;d"), "AA-----");
        assert_eq!(map(&[custom("A", ";:", 2, true)], "a;b:c;d"), "AAAA---");
        assert_eq!(map(&[custom("A", "x", 1, true)], "aXbxc"), "AAAA-");
        assert_eq!(map(&[custom("A", " ", 1, true)], "ab cd ef"), "AAA-----");
        assert_eq!(map(&[custom("A", "--", 1, true)], "ab--cd--ef"), "AAA-------");
    }

    #[test]
    fn several_items_and_repeat() {
        let w = |s: &str, n, inc| item(s, "AnyWord", n, inc);
        let c = |s: &str, n, inc| item(s, "AnyCharacter", n, inc);
        let rep = |n| item("$ID/[No character style]", "Repeat", n, true);
        assert_eq!(map(&[w("A", 1, true), c("C", 2, true), w("D", 1, true)], "The quick brown fox jumps."), "AAAACCDDDD----------------");
        assert_eq!(map(&[w("A", 1, false), w("C", 1, true)], "The quick brown fox jumps."), "AAACCCCCCC----------------");
        assert_eq!(map(&[c("A", 1, true), c("C", 5, true), c("D", 1, true)], "ab"), "AC");
        assert_eq!(map(&[item("A", "Tabs", 1, false), item("C", "Tabs", 1, true), w("D", 1, true)], "Name\tvalue here\tend"), "AAAACDDDDDD--------");
        assert_eq!(map(&[w("A", 1, true), w("A", 1, true)], "one two three"), "AAAAAAAA-----");
        let none = NestedStyle { style: NO_STYLE.into(), ..w("A", 1, true) };
        assert_eq!(map(&[none, w("C", 1, true)], "one two three"), "----CCCC-----");
        // [Repeat] n repeats the n items before it to the end of the paragraph
        let t = "w1 w2 w3 w4 w5 w6 w7";
        assert_eq!(map(&[w("A", 1, true), w("C", 1, true), rep(2)], t), "AAACCCAAACCCAAACCCAA");
        assert_eq!(map(&[w("A", 1, true), w("C", 1, true), rep(1)], t), "AAACCCCCCCCCCCCCCCCC");
        assert_eq!(map(&[w("A", 1, true), w("C", 1, true), rep(99)], t), "AAACCC--------------");
        assert_eq!(map(&[w("A", 1, true), rep(2)], "w1 w2 w3 w4 w5"), "AAA-----------");
        assert_eq!(map(&[rep(2)], "w1 w2 w3"), "--------");
        assert_eq!(map(&[w("A", 1, true), rep(2), w("C", 1, true)], t), "AAA-----------------");
        assert_eq!(map(&[w("A", 1, false), w("C", 1, true), rep(1)], "w1 w2 w3 w4 w5 w6"), "AACCCCCCCCCCCCCCC");
    }

    #[test]
    fn drop_cap_item_takes_the_first_characters() {
        // measured (rules/dropcap.ko.md): the drop cap's style covers `DropCapCharacters` characters, the next item starts behind them
        let chars: Vec<char> = "Typography is fun".chars().collect();
        let items = [item("A", DROP_CAP, 1, true), item("B", "AnyWord", 1, true)];
        let out = spans_with(&items, &chars, 1);
        assert_eq!(out.spans, vec![(0..1, 0), (1..11, 1)]);
        assert_eq!(spans_with(&items, &chars, 2).spans, vec![(0..2, 0), (2..11, 1)]);
        // no drop cap characters (or the plain `spans`): the drop cap item covers nothing and the list goes on from the start
        assert_eq!(spans(&items, &chars).spans, vec![(0..11, 1)]);
        assert!(check(&items).is_ok());
        assert!(check(&[item("B", "AnyWord", 1, true), item("A", DROP_CAP, 1, true)]).is_err());
    }

    #[test]
    fn end_nested_style_character_ends_the_item() {
        // measured (rules/specialchars-c1.ko.md): the character ends the item that scans to it and goes into it, through or up to alike
        let e = marker::END_NESTED;
        let chars: Vec<char> = ['a', 'b', e, 'c', 'd'].to_vec();
        let items = [item("A", "EndNestedStyle", 1, false), item("C", "AnyWord", 1, true)];
        assert_eq!(spans(&items, &chars).spans, vec![(0..3, 0), (3..5, 1)]);
        // a word is cut at it; the list goes on
        let items = [item("A", "AnyWord", 1, true), item("C", "AnyWord", 1, true)];
        assert_eq!(spans(&items, &chars).spans, vec![(0..3, 0), (3..5, 1)]);
        // a count of characters cut short by it ends the list
        let items = [item("A", "AnyCharacter", 5, true), item("C", "AnyWord", 1, true)];
        assert_eq!(spans(&items, &chars).spans, vec![(0..3, 0)]);
        // the character after a word's trailing white space is looked at: it goes into the word through, not up to
        let chars: Vec<char> = ['a', ' ', e, 'c'].to_vec();
        assert_eq!(spans(&[item("A", "AnyWord", 1, true)], &chars).spans, vec![(0..3, 0)]);
        assert_eq!(spans(&[item("A", "AnyWord", 1, false)], &chars).spans, vec![(0..1, 0)]);
        // page numbers, section markers, anchored objects are delimiters of their own (a text variable counts as a page number)
        let chars: Vec<char> = ['a', marker::VARIABLE, 'b', marker::SECTION, 'c', '\u{fffc}', 'd'].to_vec();
        assert_eq!(spans(&[item("A", "AutoPageNumber", 1, true)], &chars).spans, vec![(0..2, 0)]);
        assert_eq!(spans(&[item("A", "SectionMarker", 1, false)], &chars).spans, vec![(0..3, 0)]);
        assert_eq!(spans(&[item("A", "InlineGraphic", 1, true)], &chars).spans, vec![(0..6, 0)]);
        assert!(spans(&[item("A", "SectionMarker", 1, true)], &chars[..3]).stopped_at.is_none());
    }

    #[test]
    fn reads_the_idml_list() {
        let xml = r#"<S><ParagraphStyle Self="ParagraphStyle/P" EmptyNestedStyles="false"><Properties><AllNestedStyles type="list">
            <ListItem type="record"><AppliedCharacterStyle type="object">CharacterStyle/A</AppliedCharacterStyle><Delimiter type="enumeration">AnyWord</Delimiter><Repetition type="long">2</Repetition><Inclusive type="boolean">false</Inclusive></ListItem>
            <ListItem type="record"><AppliedCharacterStyle type="object">CharacterStyle/B</AppliedCharacterStyle><Delimiter type="string"> </Delimiter><Repetition type="long">1</Repetition><Inclusive type="boolean">true</Inclusive></ListItem>
            </AllNestedStyles></Properties></ParagraphStyle>
            <ParagraphStyle Self="ParagraphStyle/Q" EmptyNestedStyles="true"/></S>"#;
        let items = rules(xml, "ParagraphStyle/P").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].delimiter.as_str(), items[0].custom, items[0].repetition, items[0].inclusive), ("AnyWord", false, 2, false));
        assert_eq!((items[1].delimiter.as_str(), items[1].custom), (" ", true));
        assert!(rules(xml, "ParagraphStyle/Q").unwrap().is_empty());
        assert!(check(&items).is_ok());
        assert!(check(&[item("A", "Repeat", 1, true)]).is_err());
        assert!(check(&[item("A", "Bogus", 1, true)]).is_err());
    }
}
