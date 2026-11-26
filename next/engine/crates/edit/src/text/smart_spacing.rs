//! Preferences ▸ Type ▸ Adjust Spacing Automatically When Cutting and Pasting Words (`textEditingPreferences.smartCutAndPaste`, tools-c2).
//! The screen runs `adjustSpacing` in the same edit as the cut or the paste, so one Undo takes both back. The rules (InDesign's help, not
//! measured with real input; next/oracle/rules/limits.toml `tools-c2-smart-spacing`):
//! - after a cut at `at` (start = end): a space on both sides loses the one after; a space left before punctuation or before the paragraph
//!   end loses the one before; a space left at the start of a paragraph goes;
//! - after a paste of [start, end): a pasted word that touches a word before or after it gets one space between them.
use super::*;

fn word(c: char) -> bool { c.is_alphanumeric() || c == '_' }
fn space(c: char) -> bool { c == ' ' || c == '\u{3000}' }
fn closing(c: char) -> bool { matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '…' | '。' | '、') }
fn paragraph_end(c: char) -> bool { matches!(c, '\r' | '\n' | '\u{2029}') }

/// What to change: deletions `(start, end)` and insertions `(at, " ")`, applied from the end of the story back.
fn plan(chars: &[char], start: usize, end: usize) -> (Option<(usize, usize)>, Vec<usize>) {
    let len = chars.len();
    let at = |i: usize| chars.get(i).copied();
    if start == end {
        let p = start.min(len);
        let before = p.checked_sub(1).and_then(at);
        let after = at(p);
        if before.is_some_and(space) && after.is_some_and(space) { return (Some((p, p + 1)), vec![]); }
        if before.is_some_and(space) && (after.is_none_or(|c| closing(c) || paragraph_end(c))) { return (Some((p - 1, p)), vec![]); }
        if (p == 0 || before.is_some_and(paragraph_end)) && after.is_some_and(space) { return (Some((p, p + 1)), vec![]); }
        return (None, vec![]);
    }
    if end > len || start > end { return (None, vec![]); }
    let (first, last) = (chars[start], chars[end - 1]);
    let mut add = Vec::new();
    if end < len && word(last) && word(chars[end]) { add.push(end); }
    if start > 0 && word(first) && word(chars[start - 1]) { add.push(start); }
    (None, add)
}

pub(crate) fn adjust(w: &mut Work<'_>, story: &str, start: &usize, end: &usize) -> Result<(), EditError> {
    let (part, ..) = locate(w, story)?;
    let chars: Vec<char> = contents(w.text(&part)?, story)?.chars().collect();
    if *start > chars.len() || *end > chars.len() || start > end { return Err(EditError::new("invalid", "range outside the story")); }
    let (cut, add) = plan(&chars, *start, *end);
    if let Some((a, b)) = cut { delete(w, story, a, b)?; }
    // `add` is from the end back: the second insertion point is not moved by the first
    for at in add { insert(w, story, at, " ")?; }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::plan;
    fn p(s: &str, a: usize, b: usize) -> (Option<(usize, usize)>, Vec<usize>) { plan(&s.chars().collect::<Vec<_>>(), a, b) }
    #[test]
    fn cut_leaves_one_space() {
        // "one two three" with "two" cut: "one  three", the space after goes
        assert_eq!(p("one  three", 4, 4), (Some((4, 5)), vec![]));
        // the last word cut before the full stop: "one two ." loses the space before the stop
        assert_eq!(p("one two .", 8, 8), (Some((7, 8)), vec![]));
        // the first word of a paragraph cut: " two"
        assert_eq!(p("a\r two", 2, 2), (Some((2, 3)), vec![]));
        assert_eq!(p("one two", 3, 3), (None, vec![]));
    }
    #[test]
    fn paste_gets_its_spaces() {
        // "onetwothree" with "two" pasted at 3: spaces on both sides (end first)
        assert_eq!(p("onetwothree", 3, 6), (None, vec![6, 3]));
        assert_eq!(p("one two three", 4, 7), (None, vec![]));
        assert_eq!(p("one, two", 5, 8), (None, vec![]));
    }
}
