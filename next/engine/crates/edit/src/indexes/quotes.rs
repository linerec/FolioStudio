//! Quotation marks in topic names (index-c5, measured in InDesign 2025 with scenarios `index-c5-quote-*`).
//!
//! - Sorting: InDesign treats the single quotation marks `' ‘ ’ ‚ ‛` as one character and the double quotation
//!   marks `" “ ” „ ‟` as one. Two names that differ only there are one topic (`topics.add` of `a‘b` after
//!   `a'b` returns `a'b`). The angle quotes `‹ › « »` stay four different characters.
//! - Section headings: leading quotation marks (the five single, the five double, `‹ › « »`) are skipped and the
//!   next character decides the section (`«cherry` under C, `'1` under Symbols). Other punctuation (`,` `.`, the
//!   CJK brackets `「 」`, `❝`, `＂`) is not skipped: those names are under Symbols.
//! - The generated text: a straight `'` or `"` becomes a typographic quote like typed text does: opening
//!   (`‘` `“`) at the start of the name and after a space, closing (`’` `”`) after a letter or digit.
//!   Curly quotes in a name are printed as they are.

/// The quotation marks a section heading skips.
pub(super) fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '"' | '\u{2018}'..='\u{201F}' | '\u{2039}' | '\u{203A}' | '\u{AB}' | '\u{BB}')
}

/// The name InDesign compares: the single quotation marks as `'`, the double ones as `"`.
pub(super) fn fold(s: &str) -> String {
    s.chars().map(|c| match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
        c => c,
    }).collect()
}

/// The text InDesign prints for a topic name. `None`: a straight quote after something not measured
/// (punctuation, another quote), which generation refuses.
pub(super) fn printed(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut before: Option<char> = None;
    for c in name.chars() {
        let straight = |open: char, close: char| match before {
            None => Some(open),
            Some(p) if p.is_whitespace() => Some(open),
            Some(p) if p.is_alphanumeric() => Some(close),
            _ => None,
        };
        out.push(match c {
            '\'' => straight('\u{2018}', '\u{2019}')?,
            '"' => straight('\u{201C}', '\u{201D}')?,
            c => c,
        });
        before = Some(c);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{fold, printed};

    #[test]
    fn straight_quotes_become_typographic_ones_by_their_place() {
        assert_eq!(printed("'1").as_deref(), Some("\u{2018}1"));
        assert_eq!(printed("don't").as_deref(), Some("don\u{2019}t"));
        assert_eq!(printed("rock 'n' roll").as_deref(), Some("rock \u{2018}n\u{2019} roll"));
        assert_eq!(printed("He said \"hi\"").as_deref(), Some("He said \u{201C}hi\u{201D}"));
        assert_eq!(printed("'\u{201C}ee").as_deref(), Some("\u{2018}\u{201C}ee"));
        assert_eq!(printed("\u{2018}e\u{2019}").as_deref(), Some("\u{2018}e\u{2019}"));
        assert_eq!(printed("a (\"b\")"), None);
        assert_eq!(fold("a\u{2018}b\u{201E}"), "a'b\"");
    }
}
