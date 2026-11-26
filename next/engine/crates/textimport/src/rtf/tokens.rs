//! Lexical tokens of an RTF byte stream. Text and control words borrow their original bytes.

#[derive(Debug)]
pub(super) enum Tok<'a> {
    Open,
    Close,
    Word(&'a [u8], Option<i32>),
    Symbol(u8),
    Hex(u8),
    Text(&'a [u8]),
}

pub(super) fn tokenize(b: &[u8]) -> Vec<Tok<'_>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => {
                out.push(Tok::Open);
                i += 1;
            }
            b'}' => {
                out.push(Tok::Close);
                i += 1;
            }
            b'\r' | b'\n' => i += 1,
            b'\\' => {
                i += 1;
                let Some(&c) = b.get(i) else { break };
                if c.is_ascii_alphabetic() {
                    let start = i;
                    while i < b.len() && b[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let name = &b[start..i];
                    let mut param = None;
                    let negative = b.get(i) == Some(&b'-');
                    let digits = i + usize::from(negative);
                    let mut end = digits;
                    while end < b.len() && b[end].is_ascii_digit() {
                        end += 1;
                    }
                    if end > digits {
                        let v: i64 = std::str::from_utf8(&b[digits..end]).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
                        param = Some((if negative { -v } else { v }).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
                        i = end;
                    }
                    if b.get(i) == Some(&b' ') {
                        i += 1;
                    }
                    out.push(Tok::Word(name, param));
                } else if c == b'\'' {
                    let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                    out.push(hex.map_or(Tok::Symbol(b'\''), Tok::Hex));
                    i += if hex.is_some() { 3 } else { 1 };
                } else {
                    out.push(Tok::Symbol(c));
                    i += 1;
                }
            }
            _ => {
                let start = i;
                while i < b.len() && !matches!(b[i], b'{' | b'}' | b'\\' | b'\r' | b'\n') {
                    i += 1;
                }
                out.push(Tok::Text(&b[start..i]));
            }
        }
    }
    out
}
