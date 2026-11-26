//! Autocorrect word pairs (Edit > Spelling > Autocorrect; Preferences > Autocorrect).
//!
//! Table rules measured in InDesign 2025 (DOM `app.autoCorrectTables[i].autoCorrectWordPairList`,
//! `next/oracle/rules/preferences.ko.md`): the list only grows through the setter; a typo that is
//! already in the list keeps its first correction. One table per language (38 languages in
//! InDesign; none for Korean).
//!
//! What typing does, measured in InDesign 2025 with real key presses (`next/oracle/rules/autocorrect-c2.ko.md`, the cases are
//! `next/oracle/rules/autocorrect-typing.json`): text put in by a script or pasted is never corrected, only typed text. A typed character finishes
//! a word only when it is a space, `.`, `,`, `?`, `/`, `<`, `>`, a return, a tab or a forced line break. The word is everything back to the
//! last separator (a space or other white space, `< > / \ + = | ~ ^` and a backtick, the single quotes, `-`-like dashes `–` `—`, `©`, and a change
//! between CJK and other text): punctuation like `,` `.` `;` `:` `!` `(` `-` a digit, a letter or a straight double quote stays inside it, so
//! `a-zqteh` and `(zqteh)` are not the word `zqteh`. A word is corrected only when some of its characters were typed in the current run.
//! A typo that has no capital letter matches whatever its case (the correction gets a capital first letter when the typed word starts with a
//! capital); a typo with a capital letter matches only exactly. With "autocorrect capitalization errors" the word that starts a sentence
//! (after `. ? !` and a space, or a paragraph start) gets a capital first letter when the dictionary knows it.
use std::collections::BTreeMap;

pub type Pair = (String, String);
pub type Tables = BTreeMap<String, Vec<Pair>>;

fn has_upper(s: &str) -> bool {
    s.chars().any(char::is_uppercase)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The correction for `word`: an exact match, or (for a typo without a capital letter) a match whatever the case of `word`, where the
/// correction starts with a capital letter when `word` does (InDesign 2025: `Zqteh` and `ZQTEH` become `The`, `zqTeh` becomes `the`).
pub fn lookup(table: &[Pair], word: &str) -> Option<String> {
    if let Some((_, fix)) = table.iter().find(|(typo, _)| typo == word) {
        return Some(fix.clone());
    }
    if !has_upper(word) {
        return None;
    }
    let lower = word.to_lowercase();
    let (_, fix) = table.iter().find(|(typo, _)| *typo == lower && !has_upper(typo))?;
    Some(if word.chars().next().is_some_and(char::is_uppercase) { capitalize_first(fix) } else { fix.clone() })
}

/// Add pairs the way InDesign's setter does: existing typos keep their first correction. The list
/// stays sorted by typo (Folio's list shows it that way).
pub fn add_pairs(table: &mut Vec<Pair>, pairs: &[Pair]) {
    for (typo, fix) in pairs {
        if !table.iter().any(|(t, _)| t == typo) {
            table.push((typo.clone(), fix.clone()));
        }
    }
    table.sort_by(|a, b| a.0.cmp(&b.0));
}

/// Sentence ends that a following word starts a sentence after. The two abbreviations are the ones measured not to end one in
/// InDesign 2025 (`e.g. it`, `dr. smith` keep the lower case; `mr. smith` does not, so InDesign's list is longer than this one).
const ABBREVIATIONS: [&str; 2] = ["e.g.", "dr."];

/// What a finished word becomes, or `None` to leave it. `known(word)` says whether the language's dictionary calls a word a word:
/// capitalization needs it (InDesign 2025 leaves `zqfoo` and `mr` alone but turns `hello` into `Hello`).
pub fn apply_word(table: &[Pair], word: &str, capitalization_errors: bool, starts_sentence: bool, known: impl Fn(&str) -> bool) -> Option<String> {
    let mut result = lookup(table, word);
    if capitalization_errors && starts_sentence {
        let current = result.as_deref().unwrap_or(word);
        if current.chars().next().is_some_and(|c| c.is_lowercase() && c.is_alphabetic()) {
            let head = current.split(char::is_whitespace).next().unwrap_or("");
            if known(head) || head == "i" {
                result = Some(capitalize_first(current));
            }
        }
    }
    result.filter(|r| r != word)
}

/// `apply_word` for a caller that has no dictionary: every word counts as known.
pub fn apply(table: &[Pair], word: &str, capitalization_errors: bool, starts_sentence: bool) -> Option<String> {
    apply_word(table, word, capitalization_errors, starts_sentence, |_| true)
}

/// What typing changed in a story: the characters `start..end` become `replacement`, which already holds the typed text.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Fix {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Text in CJK scripts (Hangul, kana, ideographs, CJK punctuation, full-width forms): InDesign ends the Latin word before it.
fn is_cjk(c: char) -> bool {
    crate::words::is_hangul(c) || matches!(c, '\u{3000}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}')
}

/// A character the word before the caret does not reach past (measured with 100 characters in front of `zqteh` in InDesign 2025): white
/// space (not the zero width space), `< > / \ + = | ~ ^`, the backtick, `‘ ’ ‹ › – — ©` and CJK text. Letters, digits and the other
/// punctuation (`, . ; : ! ? ( [ { - _ @ # & * $ %`, `“ ” « » …` ...) belong to the word.
fn is_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(c, '<' | '>' | '/' | '\\' | '+' | '=' | '|' | '~' | '^' | '`' | '\u{2018}' | '\u{2019}' | '\u{2039}' | '\u{203a}' | '\u{2013}' | '\u{2014}' | '\u{a9}')
        || is_cjk(c)
}

/// The typed character finishes the word before it (measured with every key of a US keyboard, the numeric keypad and the return, tab and
/// shift-return keys): a space, `.`, `,`, `?`, `/`, `<`, `>`, a return, a tab, a forced line break. `; : ! ) ] } - " '` and the others do not.
pub fn ends_word(typed: char) -> bool {
    matches!(typed, ' ' | '.' | ',' | '?' | '/' | '<' | '>' | '\r' | '\t' | '\u{2028}')
}

fn abbreviation_before(text: &[char], dot: usize) -> bool {
    let mut from = dot;
    while from > 0 && (text[from - 1].is_alphabetic() || text[from - 1] == '.') {
        from -= 1;
    }
    let run: String = text[from..=dot].iter().collect::<String>().to_lowercase();
    ABBREVIATIONS.contains(&run.as_str())
}

/// The word that ends at `at` in `text`: where it starts and whether it starts a sentence. A word starts a sentence when the
/// first character before it that is not blank (a return is not blank) is a sentence end (not after an abbreviation) or a paragraph end, or there is none.
pub fn word_before(text: &[char], at: usize) -> Option<(usize, bool)> {
    let at = at.min(text.len());
    let mut start = at;
    while start > 0 && !is_separator(text[start - 1]) {
        start -= 1;
    }
    if start == at {
        return None;
    }
    let mut before = start;
    while before > 0 && text[before - 1].is_whitespace() && text[before - 1] != '\r' {
        before -= 1;
    }
    let starts_sentence = before == 0
        || match text[before - 1] {
            '\r' | '\u{2029}' => true,
            '.' => !abbreviation_before(text, before - 1),
            '!' | '?' | '\u{2026}' => true,
            _ => false,
        };
    Some((start, starts_sentence))
}

/// The switches that decide what typing does.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// Edit > Spelling > Autocorrect (the word pairs and the capital rule need it).
    pub auto_correct: bool,
    /// Preferences > Autocorrect > Autocorrect Capitalization Errors.
    pub capitalization_errors: bool,
    /// The document's "Use Typographer's Quotes": `Some(f)` makes a typed straight quote curly with `f(character before, quote)` (whether
    /// autocorrect is on or off); the rule itself is `folio_textimport::smart_quote`, the one place that knows it.
    pub quotes: Option<fn(Option<char>, char) -> char>,
}

/// A straight quote after `prefix` made curly, as typing it does; used for the quotes inside a correction too (`zqdont` -> `don’t`).
fn curly(quote: fn(Option<char>, char) -> char, prefix: &[char], text: &str) -> String {
    let mut prev = prefix.last().copied();
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        out.push(quote(prev, c));
        prev = Some(c);
    }
    out
}

/// Type `typed` into `story` at `at`, one character after the other, and say what the text around the caret becomes:
/// `None` when nothing changes (the text goes in as it is). `table_of(p)` is the table of the language of the story's character `p`
/// (`None` = the caret's language, for text that was typed) and `known(p, word)` asks that language's dictionary. `typed_run` says that a
/// character of the word before the caret was typed just before (a word that a script or a paste brought is not corrected).
pub fn type_text<'a>(
    story: &[char],
    at: usize,
    typed: &str,
    table_of: impl Fn(Option<usize>) -> Option<&'a [Pair]>,
    known: impl Fn(Option<usize>, &str) -> bool,
    rules: Rules,
    typed_run: bool,
) -> Option<Fix> {
    let at = at.min(story.len());
    let mut buf: Vec<char> = story[..at].to_vec();
    // `buf[typed_from..]` is typed text (it has the caret's language); a fix that reaches back into the story moves it
    let mut typed_from = at;
    let mut first_change: Option<usize> = None;
    let mut run = typed_run;
    for c in typed.chars() {
        let mut shown = c;
        if ends_word(c) {
            if rules.auto_correct && run
                && let Some((start, starts_sentence)) = word_before(&buf, buf.len())
            {
                let place = (start < typed_from).then_some(start);
                let word: String = buf[start..].iter().collect();
                let table = table_of(place).unwrap_or(&[]);
                if let Some(fixed) = apply_word(table, &word, rules.capitalization_errors, starts_sentence, |w| known(place, w)) {
                    let fixed = rules.quotes.map_or(fixed.clone(), |q| curly(q, &buf[..start], &fixed));
                    if fixed != word {
                        buf.truncate(start);
                        buf.extend(fixed.chars());
                        first_change = Some(first_change.map_or(start, |f| f.min(start)));
                        if start < typed_from {
                            typed_from = buf.len();
                        }
                    }
                }
            }
            run = false;
        } else {
            run = true;
            if let (Some(quote), true) = (rules.quotes, matches!(c, '"' | '\'')) {
                shown = quote(buf.last().copied(), c);
                if shown != c {
                    first_change = Some(first_change.map_or(buf.len(), |f| f.min(buf.len())));
                }
            }
        }
        buf.push(shown);
    }
    first_change.map(|start| {
        let start = start.min(at);
        Fix { start, end: at, replacement: buf[start..].iter().collect() }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(a: &str, b: &str) -> Pair {
        (a.into(), b.into())
    }
    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }
    const ON: Rules = Rules { auto_correct: true, capitalization_errors: false, quotes: None };
    fn quote(prev: Option<char>, c: char) -> char {
        let opening = prev.is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '[' | '{'));
        match (c, opening) {
            ('"', true) => '\u{201c}',
            ('"', false) => '\u{201d}',
            ('\'', true) => '\u{2018}',
            ('\'', false) => '\u{2019}',
            _ => c,
        }
    }

    fn typed(story: &str, at: usize, text: &str, table: &[Pair], rules: Rules, run: bool) -> Option<Fix> {
        type_text(&chars(story), at, text, |_| Some(table), |_, w| w == "hello" || w == "hi" || w == "the", rules, run)
    }

    #[test]
    fn a_typo_keeps_its_first_correction_and_case_counts() {
        let mut t = Vec::new();
        add_pairs(&mut t, &[p("teh", "the"), p("adn", "and")]);
        add_pairs(&mut t, &[p("teh", "tea"), p("Teh", "The")]);
        assert_eq!(t, [p("Teh", "The"), p("adn", "and"), p("teh", "the")]);
        assert_eq!(lookup(&t, "teh").as_deref(), Some("the"));
        assert_eq!(lookup(&t, "TEH").as_deref(), Some("The")); // `teh` has no capital letter, so any case of it matches
    }

    #[test]
    fn a_lower_case_typo_matches_any_case_and_a_capital_one_only_exactly() {
        let t = vec![p("zqteh", "the"), p("zqbtw", "by the way"), p("Zqcap", "Capital"), p("ZQUP", "UP")];
        assert_eq!(lookup(&t, "Zqteh").as_deref(), Some("The"));
        assert_eq!(lookup(&t, "ZQTEH").as_deref(), Some("The"));
        assert_eq!(lookup(&t, "zqTeh").as_deref(), Some("the"));
        assert_eq!(lookup(&t, "ZqBTW").as_deref(), Some("By the way"));
        assert_eq!(lookup(&t, "zQBTW").as_deref(), Some("by the way"));
        assert_eq!(lookup(&t, "Zqcap").as_deref(), Some("Capital"));
        assert_eq!(lookup(&t, "zqcap"), None);
        assert_eq!(lookup(&t, "ZQCAP"), None);
        assert_eq!(lookup(&t, "ZQUP").as_deref(), Some("UP"));
        assert_eq!(lookup(&t, "zqup"), None);
        assert_eq!(lookup(&t, "Zqup"), None);
    }

    #[test]
    fn the_word_before_the_caret_reaches_back_to_a_separator() {
        let t = chars("Hi. it's teh");
        assert_eq!(word_before(&t, 12), Some((9, false)));
        assert_eq!(word_before(&t, 3), Some((0, true))); // `Hi.` is one word, the full stop belongs to it
        assert_eq!(word_before(&chars("abc123"), 6), Some((0, true))); // a digit belongs to the word
        assert_eq!(word_before(&chars("one\rteh"), 7), Some((4, true)));
        assert_eq!(word_before(&chars("teh"), 3), Some((0, true)));
        // separators measured in InDesign 2025
        for sep in ['/', '<', '>', '\\', '+', '=', '|', '~', '^', '`', '\u{2018}', '\u{2013}', '\u{a9}', ' ', '\t', '\u{a0}', '\u{ac00}'] {
            assert_eq!(word_before(&chars(&format!("a{sep}teh")), 5), Some((2, false)), "{sep:?}");
        }
        // everything else stays in the word
        for inside in [',', '.', ';', ':', '!', '?', '(', '[', '{', '-', '_', '@', '#', '&', '*', '$', '%', '\u{201c}', '\u{2026}', '\u{200b}', 'x', '1', '\u{e9}'] {
            assert_eq!(word_before(&chars(&format!("a{inside}teh")), 5), Some((0, true)), "{inside:?}");
        }
        assert!(ends_word(' ') && ends_word('.') && ends_word(',') && ends_word('?') && ends_word('/') && ends_word('<') && ends_word('>'));
        assert!(ends_word('\t') && ends_word('\r') && ends_word('\u{2028}'));
        assert!(!ends_word('a') && !ends_word('7') && !ends_word('\'') && !ends_word(';') && !ends_word('!') && !ends_word('-') && !ends_word(')') && !ends_word('"'));
    }

    #[test]
    fn typing_a_space_after_a_listed_word_corrects_it() {
        let t = vec![p("teh", "the")];
        // the word is in the story already and one space is typed after a typed letter
        let fix = typed("see teh", 7, " ", &t, ON, true).unwrap();
        assert_eq!(fix, Fix { start: 4, end: 7, replacement: "the ".into() });
        // a word that a script or a paste brought is left alone
        assert_eq!(typed("see teh", 7, " ", &t, ON, false), None);
        // a letter after it finishes nothing
        assert_eq!(typed("see teh", 7, "s", &t, ON, true), None);
        // text typed in one go is judged character by character
        let fix = typed("a ", 2, "teh. teh ", &t, ON, false).unwrap();
        assert_eq!(fix, Fix { start: 2, end: 2, replacement: "the. the ".into() });
        let fix = typed("a ", 2, "teh.teh ", &t, ON, false).unwrap();
        assert_eq!(fix, Fix { start: 2, end: 2, replacement: "the.teh ".into() }); // `the.teh` is one word: the full stop belongs to it
        // the word's language has no table: nothing changes
        assert_eq!(type_text(&chars("see teh"), 7, " ", |_| None, |_, _| false, ON, true), None);
        // a punctuation mark inside the word spoils it
        assert_eq!(typed("see a-teh", 9, " ", &t, ON, true), None);
        assert_eq!(typed("see (teh", 8, " ", &t, ON, true), None);
        assert_eq!(typed("see teh", 7, ";", &t, ON, true), None);
        // the switch off: nothing
        assert_eq!(typed("see teh", 7, " ", &t, Rules { auto_correct: false, ..ON }, true), None);
    }

    #[test]
    fn capitalization_errors_only_at_a_sentence_start_and_for_a_known_word() {
        let t = vec![p("teh", "the")];
        let caps = Rules { capitalization_errors: true, ..ON };
        let known = |w: &str| matches!(w, "word" | "the" | "it" | "hello" | "smith");
        assert_eq!(apply_word(&t, "teh", false, false, known).as_deref(), Some("the"));
        assert_eq!(apply_word(&t, "teh", true, true, known).as_deref(), Some("The"));
        assert_eq!(apply_word(&t, "word", true, true, known).as_deref(), Some("Word"));
        assert_eq!(apply_word(&t, "zqfoo", true, true, known), None); // the dictionary does not know it
        assert_eq!(apply_word(&t, "hello!", true, true, known), None);
        assert_eq!(apply_word(&t, "word", true, false, known), None);
        assert_eq!(apply_word(&t, "Word", true, true, known), None);
        assert_eq!(apply_word(&t, "i", true, true, known).as_deref(), Some("I"));
        assert_eq!(apply_word(&t, "\u{d55c}\u{ae00}", true, true, known), None);
        // the capital rule needs no table
        let fix = type_text(&chars("it is. word"), 11, " ", |_| None, |_, w| known(w), caps, true).unwrap();
        assert_eq!(fix, Fix { start: 7, end: 11, replacement: "Word ".into() });
    }

    #[test]
    fn sentence_starts_follow_the_end_mark_and_skip_two_abbreviations() {
        for (text, starts) in [("hi. word", true), ("hi? word", true), ("hi! word", true), ("hi... word", true), ("hi\rword", true), ("hi, word", false), ("hi: word", false),
            ("hi\u{2028}word", false), ("hi\tword", false), ("hi. (word", true), ("hi. \u{201c}word", true), ("e.g. word", false), ("dr. word", false), ("mr. word", true), ("1. word", true)] {
            let t = chars(text);
            let start = text.rfind(' ').or_else(|| text.rfind(['\r', '\t', '\u{2028}'])).map_or(0, |i| text[..i].chars().count() + 1);
            assert_eq!(word_before(&t, t.len()), Some((start, starts)), "{text:?}");
        }
    }

    #[test]
    fn quotes_follow_the_character_before_them_and_reach_into_a_correction() {
        let q = Rules { quotes: Some(quote), ..ON };
        let fix = typed("", 0, "\"a\"", &[], q, false).unwrap();
        assert_eq!(fix, Fix { start: 0, end: 0, replacement: "\u{201c}a\u{201d}".into() });
        assert_eq!(typed("", 0, "\"a\"", &[], ON, false), None);
        let t = vec![p("zqdont", "don't")];
        assert_eq!(typed("", 0, "zqdont ", &t, q, false).unwrap().replacement, "don\u{2019}t ");
        assert_eq!(typed("", 0, "zqdont ", &t, ON, false).unwrap().replacement, "don't ");
    }
}
