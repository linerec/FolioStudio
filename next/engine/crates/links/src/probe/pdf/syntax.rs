//! PDF values, names and lexical scanning.

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Name(String),
    Str(Vec<u8>),
    Array(Vec<Obj>),
    Dict(Dict),
    Ref(u32),
    /// Dictionary and the raw data range in the file.
    Stream(Dict, usize, usize),
}

pub(super) type Dict = Vec<(String, Obj)>;

pub(super) fn get<'a>(d: &'a Dict, k: &str) -> Option<&'a Obj> {
    d.iter().find(|(n, _)| n == k).map(|(_, v)| v)
}

impl Obj {
    pub(super) fn dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _, _) => Some(d),
            _ => None,
        }
    }
    pub(super) fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
    pub(super) fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }
}

pub(super) fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}
fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

pub(super) struct Lexer<'a> {
    pub(super) b: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> Lexer<'a> {
    pub(super) fn skip_ws(&mut self) {
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            if is_ws(c) {
                self.pos += 1;
            } else if c == b'%' {
                while self.pos < self.b.len() && self.b[self.pos] != b'\n' && self.b[self.pos] != b'\r' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    pub(super) fn word(&mut self) -> &'a [u8] {
        let s = self.pos;
        while self.pos < self.b.len() && !is_ws(self.b[self.pos]) && !is_delim(self.b[self.pos]) {
            self.pos += 1;
        }
        &self.b[s..self.pos]
    }

    pub(super) fn int_ahead(&self, at: usize) -> Option<(u64, usize)> {
        let mut p = at;
        while p < self.b.len() && is_ws(self.b[p]) {
            p += 1;
        }
        let s = p;
        while p < self.b.len() && self.b[p].is_ascii_digit() {
            p += 1;
        }
        if p == s || p - s > 10 {
            return None;
        }
        Some((std::str::from_utf8(&self.b[s..p]).ok()?.parse().ok()?, p))
    }

    pub(super) fn obj(&mut self, depth: usize) -> Option<Obj> {
        if depth > 64 {
            return None;
        }
        self.skip_ws();
        let c = *self.b.get(self.pos)?;
        match c {
            b'/' => {
                self.pos += 1;
                let raw = self.word();
                Some(Obj::Name(decode_name(raw)))
            }
            b'(' => Some(Obj::Str(self.literal())),
            b'<' if self.b.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                let mut d = Dict::new();
                loop {
                    self.skip_ws();
                    match self.b.get(self.pos)? {
                        b'>' => {
                            self.pos += 2;
                            return Some(Obj::Dict(d));
                        }
                        b'/' => {
                            self.pos += 1;
                            let k = decode_name(self.word());
                            let v = self.obj(depth + 1)?;
                            d.push((k, v));
                        }
                        _ => {
                            // junk key: skip a token
                            self.obj(depth + 1)?;
                        }
                    }
                }
            }
            b'<' => {
                self.pos += 1;
                let s = self.pos;
                while self.pos < self.b.len() && self.b[self.pos] != b'>' {
                    self.pos += 1;
                }
                let hex: Vec<u8> = self.b[s..self.pos].iter().copied().filter(|c| c.is_ascii_hexdigit()).collect();
                self.pos += 1;
                Some(Obj::Str(hex.chunks(2).map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap_or("0"), 16).unwrap_or(0) << if p.len() == 1 { 4 } else { 0 }).collect()))
            }
            b'[' => {
                self.pos += 1;
                let mut a = Vec::new();
                loop {
                    self.skip_ws();
                    if *self.b.get(self.pos)? == b']' {
                        self.pos += 1;
                        return Some(Obj::Array(a));
                    }
                    a.push(self.obj(depth + 1)?);
                }
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                let w = self.word();
                let n: f64 = std::str::from_utf8(w).ok()?.parse().unwrap_or(0.0);
                // `n g R`
                if w.iter().all(u8::is_ascii_digit)
                    && let Some((_, p1)) = self.int_ahead(self.pos)
                {
                    let mut p = p1;
                    while p < self.b.len() && is_ws(self.b[p]) {
                        p += 1;
                    }
                    if self.b.get(p) == Some(&b'R') && self.b.get(p + 1).is_none_or(|c| is_ws(*c) || is_delim(*c)) {
                        self.pos = p + 1;
                        return Some(Obj::Ref(n as u32));
                    }
                }
                Some(Obj::Num(n))
            }
            _ => {
                let w = self.word();
                match w {
                    b"true" => Some(Obj::Bool(true)),
                    b"false" => Some(Obj::Bool(false)),
                    b"null" => Some(Obj::Null),
                    [] => {
                        self.pos += 1;
                        Some(Obj::Null)
                    }
                    _ => Some(Obj::Null),
                }
            }
        }
    }

    pub(super) fn literal(&mut self) -> Vec<u8> {
        self.pos += 1;
        let mut depth = 1;
        let mut out = Vec::new();
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            self.pos += 1;
            match c {
                b'\\' => {
                    if let Some(&n) = self.b.get(self.pos) {
                        self.pos += 1;
                        out.push(match n {
                            b'n' => b'\n',
                            b'r' => b'\r',
                            b't' => b'\t',
                            b'b' => 8,
                            b'f' => 12,
                            other => other,
                        });
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
        out
    }
}

pub(super) fn decode_name(raw: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'#'
            && let Some(h) = raw.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(h).unwrap_or("zz"), 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(super) fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}
