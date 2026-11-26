//! Converts calculated-field expressions into tokens with character spans.

use super::{ExprError, err};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Tok { Num(f64), Str(String), Field(String), Ident(String), Plus, Minus, Star, Slash, Amp, Eq, Ne, Lt, Gt, Le, Ge, LParen, RParen, Comma, End }

pub(super) struct Lexed { pub(super) tok: Tok, pub(super) pos: usize, pub(super) end: usize }

pub(super) fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(_) => "a number".into(), Tok::Str(_) => "a text".into(), Tok::Field(n) => format!("{{{n}}}"), Tok::Ident(n) => n.clone(),
        Tok::Plus => "+".into(), Tok::Minus => "-".into(), Tok::Star => "*".into(), Tok::Slash => "/".into(), Tok::Amp => "&".into(),
        Tok::Eq => "=".into(), Tok::Ne => "<>".into(), Tok::Lt => "<".into(), Tok::Gt => ">".into(), Tok::Le => "<=".into(), Tok::Ge => ">=".into(),
        Tok::LParen => "(".into(), Tok::RParen => ")".into(), Tok::Comma => ",".into(), Tok::End => "the end".into(),
    }
}

pub(super) fn lex(chars: &[char]) -> Result<Vec<Lexed>, ExprError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() { i += 1; continue; }
        let start = i;
        let tok = match c {
            '"' => {
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= chars.len() { return Err(err("unterminatedString", start, chars.len(), vec![])); }
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') { s.push('"'); i += 2; continue; }
                        i += 1;
                        break;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                Tok::Str(s)
            }
            '{' => {
                i += 1;
                let from = i;
                while i < chars.len() && chars[i] != '}' { i += 1; }
                if i >= chars.len() { return Err(err("unterminatedField", start, chars.len(), vec![])); }
                let name: String = chars[from..i].iter().collect();
                i += 1;
                if name.trim().is_empty() { return Err(err("emptyField", start, i, vec![])); }
                Tok::Field(name.trim().to_owned())
            }
            '0'..='9' | '.' => {
                if c == '.' && !chars.get(i + 1).is_some_and(char::is_ascii_digit) { return Err(err("unexpectedChar", i, i + 1, vec![".".into()])); }
                while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
                if i < chars.len() && chars[i] == '.' {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
                }
                if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                    let mut j = i + 1;
                    if j < chars.len() && matches!(chars[j], '+' | '-') { j += 1; }
                    if j < chars.len() && chars[j].is_ascii_digit() {
                        while j < chars.len() && chars[j].is_ascii_digit() { j += 1; }
                        i = j;
                    }
                }
                if i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '_') {
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
                    let text: String = chars[start..i].iter().collect();
                    return Err(err("badNumber", start, i, vec![text]));
                }
                let text: String = chars[start..i].iter().collect();
                match text.parse::<f64>() {
                    Ok(x) if x.is_finite() => Tok::Num(x),
                    _ => return Err(err("badNumber", start, i, vec![text])),
                }
            }
            '&' => { i += 1; Tok::Amp }
            '+' => { i += 1; Tok::Plus }
            '-' => { i += 1; Tok::Minus }
            '*' => { i += 1; Tok::Star }
            '/' => { i += 1; Tok::Slash }
            '=' => { i += 1; Tok::Eq }
            '(' => { i += 1; Tok::LParen }
            ')' => { i += 1; Tok::RParen }
            ',' | ';' => { i += 1; Tok::Comma }
            '<' => {
                i += 1;
                match chars.get(i) { Some('=') => { i += 1; Tok::Le } Some('>') => { i += 1; Tok::Ne } _ => Tok::Lt }
            }
            '>' => { i += 1; if chars.get(i) == Some(&'=') { i += 1; Tok::Ge } else { Tok::Gt } }
            '!' if chars.get(i + 1) == Some(&'=') => { i += 2; Tok::Ne }
            c if c.is_alphabetic() || c == '_' => {
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') { i += 1; }
                Tok::Ident(chars[start..i].iter().collect())
            }
            other => return Err(err("unexpectedChar", i, i + 1, vec![other.to_string()])),
        };
        out.push(Lexed { tok, pos: start, end: i });
    }
    out.push(Lexed { tok: Tok::End, pos: chars.len(), end: chars.len() });
    Ok(out)
}
