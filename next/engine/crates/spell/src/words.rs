//! Splitting a text into words. Indexes are scalar (`char`) positions, like the editor's.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

pub fn is_hangul(c: char) -> bool {
    matches!(c, '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' | '\u{a960}'..='\u{a97f}' | '\u{ac00}'..='\u{d7ff}')
}

fn is_mark(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036f}')
}

fn joiner(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '-' | '\u{2010}')
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || is_mark(c)
}

/// Words are runs of letters and digits; an apostrophe or hyphen with a letter on both sides stays
/// inside the word (`don't`, `well-known`).
pub fn tokenize(chars: &[char]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !word_char(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() {
            if word_char(chars[i])
                || (joiner(chars[i]) && i > start && chars.get(i + 1).is_some_and(|&n| n.is_alphabetic()) && chars[i - 1].is_alphabetic())
            {
                i += 1;
            } else {
                break;
            }
        }
        out.push(Token { start, end: i, text: chars[start..i].iter().collect() });
    }
    out
}

pub fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Characters that end a sentence for the "uncapitalized sentence" check.
pub fn ends_sentence(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\u{2026}' | '\u{3002}' | '\u{ff01}' | '\u{ff1f}')
}

/// A paragraph break inside a story's text.
pub fn is_break(c: char) -> bool {
    matches!(c, '\r' | '\n' | '\u{2029}')
}
