//! PDF (and PDF-compatible Illustrator files): page count, page boxes, colour spaces, spot
//! colours, output intent and transparency, read from the cross-reference table / stream
//! (with object streams), falling back to a scan of `n g obj` headers when the table is
//! damaged. Content streams (Flate only) are tokenised for colour operators; image data is
//! never decoded.

use super::{ColorSpace, ImageFormat, ImageInfo, ProbeError, Result, VectorColors, inflate};
use std::collections::{HashMap, HashSet};

/// Largest decoded object / xref stream (bytes).
const MAX_STREAM: usize = 64 << 20;
/// Content bytes tokenised per file.
const MAX_CONTENT: usize = 96 << 20;
const MAX_PAGES: usize = 100_000;

#[derive(Clone, Debug, PartialEq)]
enum Obj {
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

type Dict = Vec<(String, Obj)>;

fn get<'a>(d: &'a Dict, k: &str) -> Option<&'a Obj> {
    d.iter().find(|(n, _)| n == k).map(|(_, v)| v)
}

impl Obj {
    fn dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _, _) => Some(d),
            _ => None,
        }
    }
    fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }
}

fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}
fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

struct Lexer<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn skip_ws(&mut self) {
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

    fn word(&mut self) -> &'a [u8] {
        let s = self.pos;
        while self.pos < self.b.len() && !is_ws(self.b[self.pos]) && !is_delim(self.b[self.pos]) {
            self.pos += 1;
        }
        &self.b[s..self.pos]
    }

    fn int_ahead(&self, at: usize) -> Option<(u64, usize)> {
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

    fn obj(&mut self, depth: usize) -> Option<Obj> {
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

    fn literal(&mut self) -> Vec<u8> {
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

fn decode_name(raw: &[u8]) -> String {
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

#[derive(Clone, Copy)]
enum Loc {
    Offset(usize),
    InStream(u32, u32),
}

type ObjectStream = std::rc::Rc<(Vec<u8>, Vec<(u32, usize)>)>;
type PageNode = (Obj, Option<[f64; 4]>, Option<[f64; 4]>, i32, Option<Obj>, usize);

struct Pdf<'a> {
    b: &'a [u8],
    xref: HashMap<u32, Loc>,
    trailer: Dict,
    objstm: HashMap<u32, Option<ObjectStream>>,
    cache: HashMap<u32, Obj>,
    encrypted: bool,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

impl<'a> Pdf<'a> {
    /// Indirect object at `offset` (`n g obj …`).
    fn parse_at(&self, offset: usize) -> Option<Obj> {
        let mut lx = Lexer { b: self.b, pos: offset };
        lx.skip_ws();
        lx.word(); // n
        lx.skip_ws();
        lx.word(); // g
        lx.skip_ws();
        if lx.word() != b"obj" {
            return None;
        }
        let o = lx.obj(0)?;
        if let Obj::Dict(d) = &o {
            lx.skip_ws();
            if self.b[lx.pos..].starts_with(b"stream") {
                let mut p = lx.pos + 6;
                if self.b.get(p) == Some(&b'\r') {
                    p += 1;
                }
                if self.b.get(p) == Some(&b'\n') {
                    p += 1;
                }
                let declared = match get(d, "Length") {
                    Some(Obj::Num(n)) => Some(*n as usize),
                    Some(Obj::Ref(r)) => self.direct_length(*r),
                    _ => None,
                };
                let end = match declared {
                    Some(n) if p + n <= self.b.len() && find(&self.b[..(p + n + 32).min(self.b.len())], b"endstream", p + n).is_some() => p + n,
                    _ => find(self.b, b"endstream", p).unwrap_or(self.b.len()),
                };
                return Some(Obj::Stream(d.clone(), p, end));
            }
        }
        Some(o)
    }

    fn direct_length(&self, r: u32) -> Option<usize> {
        match self.xref.get(&r)? {
            Loc::Offset(o) => self.parse_at(*o)?.num().map(|n| n as usize),
            Loc::InStream(..) => None,
        }
    }

    fn resolve(&mut self, o: &Obj) -> Option<Obj> {
        match o {
            Obj::Ref(r) => self.get_obj(*r),
            other => Some(other.clone()),
        }
    }

    fn get_obj(&mut self, num: u32) -> Option<Obj> {
        if let Some(o) = self.cache.get(&num) {
            return Some(o.clone());
        }
        let o = match *self.xref.get(&num)? {
            Loc::Offset(off) => self.parse_at(off)?,
            Loc::InStream(stm, idx) => {
                let s = self.object_stream(stm)?;
                let (_, off) = *s.1.get(idx as usize).filter(|e| e.0 == num).or_else(|| s.1.iter().find(|e| e.0 == num))?;
                let mut lx = Lexer { b: &s.0, pos: off };
                lx.obj(0)?
            }
        };
        if self.cache.len() < 200_000 {
            self.cache.insert(num, o.clone());
        }
        Some(o)
    }

    fn object_stream(&mut self, num: u32) -> Option<ObjectStream> {
        if let Some(s) = self.objstm.get(&num) {
            return s.clone();
        }
        let parsed = (|| {
            let Loc::Offset(off) = *self.xref.get(&num)? else { return None };
            let Obj::Stream(d, s, e) = self.parse_at(off)? else { return None };
            let data = self.decode(&d, s, e)?;
            let n = get(&d, "N")?.num()? as usize;
            let first = get(&d, "First")?.num()? as usize;
            let mut lx = Lexer { b: &data, pos: 0 };
            let mut entries = Vec::with_capacity(n.min(100_000));
            for _ in 0..n.min(100_000) {
                let a = lx.obj(0)?.num()? as u32;
                let o = lx.obj(0)?.num()? as usize;
                entries.push((a, first + o));
            }
            Some(std::rc::Rc::new((data, entries)))
        })();
        self.objstm.insert(num, parsed.clone());
        parsed
    }

    /// Stream data with its filters applied (Flate with PNG/TIFF predictors only).
    fn decode(&self, d: &Dict, s: usize, e: usize) -> Option<Vec<u8>> {
        let raw = self.b.get(s..e)?;
        let filters: Vec<String> = match get(d, "Filter") {
            None => Vec::new(),
            Some(Obj::Name(n)) => vec![n.clone()],
            Some(Obj::Array(a)) => a.iter().filter_map(|f| f.name().map(str::to_owned)).collect(),
            _ => return None,
        };
        let mut data = raw.to_vec();
        for f in &filters {
            data = match f.as_str() {
                "FlateDecode" | "Fl" => inflate(&data, MAX_STREAM)?,
                "ASCII85Decode" | "A85" => ascii85(&data)?,
                "ASCIIHexDecode" | "AHx" => ascii_hex(&data),
                _ => return None,
            };
        }
        let parms = match get(d, "DecodeParms") {
            Some(Obj::Dict(p)) => Some(p.clone()),
            Some(Obj::Array(a)) => a.iter().find_map(|x| x.dict().cloned()),
            _ => None,
        };
        if let Some(p) = parms {
            let pred = get(&p, "Predictor").and_then(Obj::num).unwrap_or(1.0) as u32;
            if pred >= 10 {
                let cols = get(&p, "Columns").and_then(Obj::num).unwrap_or(1.0) as usize;
                let colors = get(&p, "Colors").and_then(Obj::num).unwrap_or(1.0) as usize;
                let bpc = get(&p, "BitsPerComponent").and_then(Obj::num).unwrap_or(8.0) as usize;
                data = png_unpredict(&data, (cols * colors * bpc).div_ceil(8), (colors * bpc).div_ceil(8).max(1))?;
            }
        }
        Some(data)
    }
}

fn ascii85(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() * 4 / 5);
    let mut group = [0u32; 5];
    let mut n = 0;
    let mut it = data.iter().copied().skip_while(|c| is_ws(*c));
    // optional "<~" prefix
    let mut pending: Vec<u8> = Vec::new();
    if let Some(a) = it.next() {
        pending.push(a);
    }
    if pending == [b'<'] {
        let _ = it.next();
        pending.clear();
    }
    for c in pending.into_iter().chain(it) {
        match c {
            b'~' => break,
            b'z' if n == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                group[n] = (c - b'!') as u32;
                n += 1;
                if n == 5 {
                    let v = group.iter().fold(0u64, |a, &d| a * 85 + d as u64);
                    out.extend_from_slice(&(v as u32).to_be_bytes());
                    n = 0;
                }
            }
            c if is_ws(c) => {}
            _ => return None,
        }
        if out.len() > MAX_STREAM {
            return None;
        }
    }
    if n > 1 {
        for g in group.iter_mut().skip(n) {
            *g = 84;
        }
        let v = group.iter().fold(0u64, |a, &d| a * 85 + d as u64);
        out.extend_from_slice(&(v as u32).to_be_bytes()[..n - 1]);
    }
    Some(out)
}

fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = data.iter().copied().take_while(|c| *c != b'>').filter(u8::is_ascii_hexdigit).collect();
    digits
        .chunks(2)
        .map(|p| {
            let h = (p[0] as char).to_digit(16).unwrap_or(0) as u8;
            let l = p.get(1).and_then(|c| (*c as char).to_digit(16)).unwrap_or(0) as u8;
            h * 16 + l
        })
        .collect()
}

fn png_unpredict(data: &[u8], row: usize, bpp: usize) -> Option<Vec<u8>> {
    if row == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        if chunk.len() < row + 1 {
            break;
        }
        let ft = chunk[0];
        let mut cur = chunk[1..].to_vec();
        for i in 0..row {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            cur[i] = match ft {
                0 => cur[i],
                1 => cur[i].wrapping_add(a),
                2 => cur[i].wrapping_add(b),
                3 => cur[i].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => {
                    let p = a as i16 + b as i16 - c as i16;
                    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
                    cur[i].wrapping_add(if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c })
                }
                _ => return None,
            };
        }
        out.extend_from_slice(&cur);
        prev = cur;
    }
    Some(out)
}

/// Reads the cross-reference chain starting at `start`.
fn read_xref(pdf: &mut Pdf<'_>, start: usize) -> bool {
    let mut next = Some(start);
    let mut seen = HashSet::new();
    let mut any = false;
    while let Some(off) = next.take() {
        if off >= pdf.b.len() || !seen.insert(off) || seen.len() > 512 {
            break;
        }
        let mut lx = Lexer { b: pdf.b, pos: off };
        lx.skip_ws();
        let trailer: Dict;
        if pdf.b[lx.pos..].starts_with(b"xref") {
            lx.pos += 4;
            loop {
                lx.skip_ws();
                let Some((first, p)) = lx.int_ahead(lx.pos) else { break };
                let Some((count, p2)) = lx.int_ahead(p) else { break };
                lx.pos = p2;
                for i in 0..count.min(10_000_000) {
                    lx.skip_ws();
                    let w1 = lx.word();
                    lx.skip_ws();
                    let _g = lx.word();
                    lx.skip_ws();
                    let t = lx.word();
                    let num = (first + i) as u32;
                    if t == b"n"
                        && let Ok(o) = std::str::from_utf8(w1).unwrap_or("x").parse::<usize>()
                    {
                        pdf.xref.entry(num).or_insert(Loc::Offset(o));
                    }
                }
            }
            lx.skip_ws();
            if !pdf.b[lx.pos..].starts_with(b"trailer") {
                break;
            }
            lx.pos += 7;
            let Some(Obj::Dict(d)) = lx.obj(0) else { break };
            trailer = d;
            if let Some(Obj::Num(x)) = get(&trailer, "XRefStm") {
                read_xref_stream(pdf, *x as usize);
            }
        } else {
            match read_xref_stream(pdf, off) {
                Some(d) => trailer = d,
                None => break,
            }
        }
        any = true;
        for (k, v) in &trailer {
            if !pdf.trailer.iter().any(|(n, _)| n == k) {
                pdf.trailer.push((k.clone(), v.clone()));
            }
        }
        next = get(&trailer, "Prev").and_then(Obj::num).map(|n| n as usize);
    }
    any
}

fn read_xref_stream(pdf: &mut Pdf<'_>, off: usize) -> Option<Dict> {
    let Obj::Stream(d, s, e) = pdf.parse_at(off)? else { return None };
    if get(&d, "Type").and_then(Obj::name) != Some("XRef") {
        return None;
    }
    let data = pdf.decode(&d, s, e)?;
    let w: Vec<usize> = match get(&d, "W")? {
        Obj::Array(a) => a.iter().filter_map(Obj::num).map(|n| n as usize).collect(),
        _ => return None,
    };
    if w.len() != 3 || w.iter().any(|&x| x > 8) {
        return None;
    }
    let size = get(&d, "Size").and_then(Obj::num).unwrap_or(0.0) as u64;
    let index: Vec<u64> = match get(&d, "Index") {
        Some(Obj::Array(a)) => a.iter().filter_map(Obj::num).map(|n| n as u64).collect(),
        _ => vec![0, size],
    };
    let rec = w[0] + w[1] + w[2];
    if rec == 0 {
        return None;
    }
    let field = |b: &[u8]| b.iter().fold(0u64, |a, &c| (a << 8) | c as u64);
    let mut pos = 0;
    for pair in index.chunks(2) {
        let (first, count) = (pair[0], *pair.get(1).unwrap_or(&0));
        for i in 0..count {
            let Some(r) = data.get(pos..pos + rec) else { break };
            pos += rec;
            let t = if w[0] == 0 { 1 } else { field(&r[..w[0]]) };
            let f2 = field(&r[w[0]..w[0] + w[1]]);
            let f3 = field(&r[w[0] + w[1]..]);
            let num = (first + i) as u32;
            match t {
                1 => {
                    pdf.xref.entry(num).or_insert(Loc::Offset(f2 as usize));
                }
                2 => {
                    pdf.xref.entry(num).or_insert(Loc::InStream(f2 as u32, f3 as u32));
                }
                _ => {}
            }
        }
    }
    Some(d)
}

/// Fallback: every `n g obj` header in the file, last definition wins.
fn scan_objects(pdf: &mut Pdf<'_>) {
    let b = pdf.b;
    let mut i = 0;
    while let Some(p) = find(b, b" obj", i) {
        i = p + 4;
        // walk back over "n g"
        let mut s = p;
        let mut fields = 0;
        while fields < 2 && s > 0 {
            let mut e = s;
            while e > 0 && b[e - 1] == b' ' {
                e -= 1;
            }
            let mut st = e;
            while st > 0 && b[st - 1].is_ascii_digit() {
                st -= 1;
            }
            if st == e {
                break;
            }
            fields += 1;
            s = st;
            if fields == 1 {
                continue;
            }
            if let Ok(n) = std::str::from_utf8(&b[st..e]).unwrap_or("x").parse::<u32>() {
                pdf.xref.insert(n, Loc::Offset(st));
            }
        }
    }
    // trailer dictionaries, if any
    let mut j = 0;
    while let Some(t) = find(b, b"trailer", j) {
        j = t + 7;
        let mut lx = Lexer { b, pos: t + 7 };
        if let Some(Obj::Dict(d)) = lx.obj(0) {
            for (k, v) in d {
                if let Some(slot) = pdf.trailer.iter_mut().find(|(n, _)| *n == k) {
                    slot.1 = v;
                } else {
                    pdf.trailer.push((k, v));
                }
            }
        }
    }
}

fn rect(o: Option<Obj>) -> Option<[f64; 4]> {
    match o? {
        Obj::Array(a) if a.len() == 4 => {
            let v: Vec<f64> = a.iter().filter_map(Obj::num).collect();
            (v.len() == 4).then(|| [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
        }
        _ => None,
    }
}

const PROCESS_NAMES: [&str; 4] = ["Cyan", "Magenta", "Yellow", "Black"];

struct Colors {
    spaces: HashSet<ColorSpace>,
    spots: HashSet<String>,
    images: HashSet<ColorSpace>,
    transparency: bool,
}

impl Colors {
    fn add(&mut self, s: ColorSpace) {
        self.spaces.insert(s);
    }
}

/// Colour space of a /ColorSpace value (name or array), recording spots.
fn color_space(pdf: &mut Pdf<'_>, o: &Obj, c: &mut Colors, depth: usize) -> Option<ColorSpace> {
    if depth > 8 {
        return None;
    }
    let o = pdf.resolve(o)?;
    match &o {
        Obj::Name(n) => match n.as_str() {
            "DeviceRGB" | "CalRGB" | "RGB" => Some(ColorSpace::Rgb),
            "DeviceCMYK" | "CMYK" => Some(ColorSpace::Cmyk),
            "DeviceGray" | "CalGray" | "G" => Some(ColorSpace::Gray),
            "Lab" => Some(ColorSpace::Lab),
            "Pattern" => None,
            _ => None,
        },
        Obj::Array(a) => {
            let kind = a.first()?.name()?;
            match kind {
                "ICCBased" => {
                    let s = pdf.resolve(a.get(1)?)?;
                    let d = s.dict()?.clone();
                    match get(&d, "N").and_then(Obj::num).map(|n| n as u32) {
                        Some(1) => Some(ColorSpace::Gray),
                        Some(3) => {
                            // Lab-based ICC profiles are rare; treat N=3 as RGB unless the
                            // alternate says Lab.
                            match get(&d, "Alternate").and_then(Obj::name) {
                                Some("Lab") => Some(ColorSpace::Lab),
                                _ => Some(ColorSpace::Rgb),
                            }
                        }
                        Some(4) => Some(ColorSpace::Cmyk),
                        _ => get(&d, "Alternate").cloned().and_then(|alt| color_space(pdf, &alt, c, depth + 1)),
                    }
                }
                "Indexed" | "I" => {
                    let base = a.get(1).cloned().and_then(|b| color_space(pdf, &b, c, depth + 1));
                    if let Some(b) = base {
                        c.add(b);
                    }
                    Some(ColorSpace::Indexed)
                }
                "Separation" => {
                    let name = a.get(1).and_then(Obj::name).unwrap_or("").to_owned();
                    if PROCESS_NAMES.contains(&name.as_str()) {
                        return Some(ColorSpace::Cmyk);
                    }
                    if name != "All" && name != "None" && !name.is_empty() {
                        c.spots.insert(name);
                    }
                    Some(ColorSpace::Other)
                }
                "DeviceN" | "NChannel" => {
                    let names = a.get(1).cloned().and_then(|n| pdf.resolve(&n));
                    let mut process = false;
                    if let Some(Obj::Array(n)) = names {
                        for x in n.iter().filter_map(Obj::name) {
                            if PROCESS_NAMES.contains(&x) {
                                process = true;
                            } else if x != "None" && x != "All" {
                                c.spots.insert(x.to_owned());
                            }
                        }
                    }
                    Some(if process { ColorSpace::Cmyk } else { ColorSpace::Other })
                }
                "Pattern" => a.get(1).cloned().and_then(|b| color_space(pdf, &b, c, depth + 1)),
                "CalRGB" => Some(ColorSpace::Rgb),
                "CalGray" => Some(ColorSpace::Gray),
                "Lab" => Some(ColorSpace::Lab),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Colour operators of a content stream (`rg`, `k`, `g`, `/DeviceRGB cs`, inline image
/// `/CS /RGB`).
fn scan_content(data: &[u8], c: &mut Colors) {
    let mut lx = Lexer { b: data, pos: 0 };
    let mut last_name: Option<String> = None;
    let named = |n: &str| match n {
        "DeviceRGB" | "RGB" => Some(ColorSpace::Rgb),
        "DeviceCMYK" | "CMYK" => Some(ColorSpace::Cmyk),
        "DeviceGray" | "G" => Some(ColorSpace::Gray),
        _ => None,
    };
    loop {
        lx.skip_ws();
        let Some(&ch) = data.get(lx.pos) else { break };
        let mut name = None;
        match ch {
            b'(' => {
                lx.literal();
            }
            b'<' if data.get(lx.pos + 1) == Some(&b'<') => lx.pos += 2,
            b'>' if data.get(lx.pos + 1) == Some(&b'>') => lx.pos += 2,
            b'<' => {
                while lx.pos < data.len() && data[lx.pos] != b'>' {
                    lx.pos += 1;
                }
                lx.pos += 1;
            }
            b'[' | b']' | b'{' | b'}' | b')' | b'>' => lx.pos += 1,
            b'/' => {
                lx.pos += 1;
                let n = decode_name(lx.word());
                // inline image: /CS /RGB
                if matches!(last_name.as_deref(), Some("CS" | "ColorSpace"))
                    && let Some(s) = named(&n)
                {
                    c.add(s);
                }
                name = Some(n);
            }
            _ => {
                let w = lx.word();
                if w.is_empty() {
                    lx.pos += 1;
                } else {
                    match w {
                        b"rg" | b"RG" => c.add(ColorSpace::Rgb),
                        b"k" | b"K" => c.add(ColorSpace::Cmyk),
                        b"g" | b"G" => c.add(ColorSpace::Gray),
                        b"cs" | b"CS" => {
                            if let Some(s) = last_name.as_deref().and_then(named) {
                                c.add(s);
                            }
                        }
                        b"ID" => match find(data, b"EI", lx.pos) {
                            // inline image data: skip to EI
                            Some(p) => lx.pos = p + 2,
                            None => break,
                        },
                        _ => {}
                    }
                }
            }
        }
        last_name = name;
    }
}

pub(crate) fn probe(file: &mut std::fs::File, len: u64) -> Result<ImageInfo> {
    // SAFETY: the map is read-only and dropped before returning; the file may change
    // underneath only if another program truncates it, in which case reads fault — the
    // same risk every reader of a shared file has. Files are opened read-only.
    let map = unsafe { memmap2::Mmap::map(&*file) }.map_err(|e| ProbeError::Io(e.to_string()))?;
    let b: &[u8] = &map;
    if len == 0 || b.is_empty() {
        return Err(ProbeError::Malformed("empty PDF".into()));
    }
    let mut pdf = Pdf { b, xref: HashMap::new(), trailer: Dict::new(), objstm: HashMap::new(), cache: HashMap::new(), encrypted: false };
    let tail_from = b.len().saturating_sub(4096);
    let ok = rfind(&b[tail_from..], b"startxref")
        .and_then(|p| Lexer { b, pos: tail_from + p + 9 }.int_ahead(tail_from + p + 9))
        .is_some_and(|(off, _)| read_xref(&mut pdf, off as usize));
    let mut repaired = false;
    let mut root = get(&pdf.trailer, "Root").cloned();
    if !ok || root.as_ref().and_then(|r| pdf.resolve(r)).is_none() {
        repaired = true;
        pdf.xref.clear();
        pdf.cache.clear();
        scan_objects(&mut pdf);
        root = get(&pdf.trailer, "Root").cloned();
        if root.as_ref().and_then(|r| pdf.resolve(r)).is_none() {
            // find the catalog by type
            let nums: Vec<u32> = pdf.xref.keys().copied().collect();
            root = nums.into_iter().find(|n| pdf.get_obj(*n).and_then(|o| o.dict().and_then(|d| get(d, "Type").and_then(Obj::name).map(|t| t == "Catalog"))).unwrap_or(false)).map(Obj::Ref);
        }
    }
    pdf.encrypted = get(&pdf.trailer, "Encrypt").is_some();
    let catalog = root.as_ref().and_then(|r| pdf.resolve(r)).and_then(|o| o.dict().cloned()).ok_or_else(|| ProbeError::Malformed("PDF without catalog".into()))?;

    let mut colors = Colors { spaces: HashSet::new(), spots: HashSet::new(), images: HashSet::new(), transparency: false };
    let mut output_intent = None;
    if let Some(oi) = get(&catalog, "OutputIntents").cloned().and_then(|o| pdf.resolve(&o))
        && let Obj::Array(list) = oi
    {
        for i in list {
            if let Some(d) = pdf.resolve(&i).and_then(|o| o.dict().cloned()) {
                let id = get(&d, "OutputConditionIdentifier").or_else(|| get(&d, "OutputCondition"));
                if let Some(Obj::Str(s)) = id {
                    output_intent = Some(String::from_utf8_lossy(s).into_owned());
                }
            }
        }
    }

    // Page tree with inherited boxes.
    let mut pages = Vec::new();
    let mut contents: Vec<Obj> = Vec::new();
    let mut stack: Vec<PageNode> =
        get(&catalog, "Pages").cloned().map(|p| vec![(p, None, None, 0, None, 0)]).unwrap_or_default();
    let mut visited = HashSet::new();
    while let Some((node, media, crop, rot, res, depth)) = stack.pop() {
        if pages.len() >= MAX_PAGES || depth > 64 {
            break;
        }
        if let Obj::Ref(r) = node
            && !visited.insert(r)
        {
            continue;
        }
        let Some(d) = pdf.resolve(&node).and_then(|o| o.dict().cloned()) else { continue };
        let media = rect(get(&d, "MediaBox").cloned().and_then(|o| pdf.resolve(&o))).or(media);
        let crop = rect(get(&d, "CropBox").cloned().and_then(|o| pdf.resolve(&o))).or(crop);
        let rot = get(&d, "Rotate").and_then(Obj::num).map(|r| r as i32).unwrap_or(rot);
        let res = get(&d, "Resources").cloned().or(res);
        if let Some(Obj::Array(kids)) = get(&d, "Kids").cloned().and_then(|k| pdf.resolve(&k)) {
            for k in kids.into_iter().rev() {
                stack.push((k, media, crop, rot, res.clone(), depth + 1));
            }
            continue;
        }
        let b = |k: &str, pdf: &mut Pdf<'_>| rect(get(&d, k).cloned().and_then(|o| pdf.resolve(&o)));
        pages.push(super::PdfPage {
            media: media.unwrap_or([0.0, 0.0, 612.0, 792.0]),
            crop,
            bleed: b("BleedBox", &mut pdf),
            trim: b("TrimBox", &mut pdf),
            art: b("ArtBox", &mut pdf),
            rotate: rot.rem_euclid(360),
        });
        if let Some(c) = get(&d, "Contents") {
            contents.push(c.clone());
        }
        let _ = res;
        if get(&d, "Group").is_some() {
            colors.transparency = true;
        }
    }
    if pages.is_empty() {
        return Err(ProbeError::Malformed("PDF without pages".into()));
    }

    // Colour spaces of every object (resources, images, shadings), bounded.
    let nums: Vec<u32> = {
        let mut v: Vec<u32> = pdf.xref.keys().copied().collect();
        v.sort_unstable();
        v
    };
    let mut content_budget = MAX_CONTENT;
    let mut content_ok = !pdf.encrypted;
    let mut form_streams: Vec<(Dict, usize, usize)> = Vec::new();
    let mut fonts: Vec<super::PlacedFont> = Vec::new();
    for n in nums.iter().take(500_000) {
        let Some(o) = pdf.get_obj(*n) else { continue };
        let Some(d) = o.dict().cloned() else { continue };
        if get(&d, "Type").and_then(Obj::name) == Some("Font")
            && let Some(f) = placed_font(&mut pdf, &d)
            && !fonts.contains(&f)
        {
            fonts.push(f);
        }
        examine(&mut pdf, &d, &mut colors, 0);
        if let Obj::Stream(sd, s, e) = &o
            && (get(sd, "Subtype").and_then(Obj::name) == Some("Form") || get(sd, "PatternType").and_then(Obj::num) == Some(1.0))
        {
            form_streams.push((sd.clone(), *s, *e));
        }
    }
    // Content streams of pages and forms.
    let mut streams: Vec<(Dict, usize, usize)> = form_streams;
    for c in &contents {
        let items = match pdf.resolve(c) {
            Some(Obj::Array(a)) => a,
            Some(other) => vec![other],
            None => Vec::new(),
        };
        for it in items {
            if let Some(Obj::Stream(d, s, e)) = pdf.resolve(&it) {
                streams.push((d, s, e));
            }
        }
    }
    for (d, s, e) in streams {
        if content_budget == 0 {
            content_ok = false;
            break;
        }
        match pdf.decode(&d, s, e) {
            Some(data) => {
                content_budget = content_budget.saturating_sub(data.len());
                scan_content(&data, &mut colors);
            }
            None => content_ok = false,
        }
    }
    let mut spaces: Vec<ColorSpace> = colors.spaces.into_iter().collect();
    spaces.sort_by_key(|s| *s as u8);
    let mut image_spaces: Vec<ColorSpace> = colors.images.into_iter().collect();
    image_spaces.sort_by_key(|s| *s as u8);
    let mut spots: Vec<String> = colors.spots.into_iter().collect();
    spots.sort();
    let first = &pages[0];
    let shown = first.crop.unwrap_or(first.media);
    let mut info = ImageInfo::raster(
        ImageFormat::Pdf,
        (shown[2] - shown[0]).abs().ceil() as u32,
        (shown[3] - shown[1]).abs().ceil() as u32,
        dominant(&spaces, &image_spaces),
        0,
        0,
    );
    info.vector = true;
    fonts.sort_by(|a, b| a.name.cmp(&b.name));
    info.fonts = fonts;
    info.pages = Some(pages.len() as u32);
    info.page_boxes = pages;
    info.vector_colors = Some(VectorColors {
        spaces,
        spots,
        image_spaces,
        content_scanned: content_ok,
        output_intent,
        transparency: colors.transparency,
    });
    let _ = repaired;
    Ok(info)
}

/// Colour spaces and transparency of one dictionary, and of the dictionaries written
/// inline in it (a page's `Resources`, their `ExtGState` entries, a `Group`); referenced
/// objects are examined on their own.
fn examine(pdf: &mut Pdf<'_>, d: &Dict, colors: &mut Colors, depth: usize) {
    if depth > 4 {
        return;
    }
    if let Some(cs) = get(d, "ColorSpace").cloned() {
        match pdf.resolve(&cs) {
            // resource dictionary of named colour spaces
            Some(Obj::Dict(named)) => {
                for (_, v) in named {
                    if let Some(s) = color_space(pdf, &v, colors, 0) {
                        colors.add(s);
                    }
                }
            }
            Some(v) => {
                let s = color_space(pdf, &v, colors, 0);
                if get(d, "Subtype").and_then(Obj::name) == Some("Image") {
                    if let Some(s) = s {
                        colors.images.insert(s);
                    }
                } else if let Some(s) = s {
                    colors.add(s);
                }
            }
            None => {}
        }
    }
    if get(d, "SMask").is_some_and(|m| m.name() != Some("None")) || get(d, "S").and_then(Obj::name) == Some("Transparency") {
        colors.transparency = true;
    }
    for k in ["CA", "ca"] {
        if get(d, k).and_then(Obj::num).is_some_and(|v| v < 1.0) {
            colors.transparency = true;
        }
    }
    for k in ["Resources", "ExtGState", "Group", "Shading", "Pattern"] {
        if let Some(Obj::Dict(inner)) = get(d, k) {
            let inner = inner.clone();
            examine(pdf, &inner, colors, depth + 1);
            // dictionaries of named resources (ExtGState << /GS0 << … >> >>)
            for (_, v) in &inner {
                if let Obj::Dict(x) = v {
                    examine(pdf, &x.clone(), colors, depth + 2);
                }
            }
        }
    }
}

/// A font dictionary: name, subtype and whether its program is in the file. CID fonts
/// are reached through their Type0 parent; Type3 glyphs are always in the file.
fn placed_font(pdf: &mut Pdf<'_>, d: &Dict) -> Option<super::PlacedFont> {
    let kind = get(d, "Subtype").and_then(Obj::name)?.to_owned();
    if kind.starts_with("CIDFontType") {
        return None;
    }
    let raw = get(d, "BaseFont").and_then(Obj::name).unwrap_or(if kind == "Type3" { "Type3" } else { "" });
    let name = match raw.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|c| c.is_ascii_uppercase()) => rest.to_owned(),
        _ => raw.to_owned(),
    };
    let descriptor_of = |pdf: &mut Pdf<'_>, f: &Dict| -> Option<Dict> {
        let fd = get(f, "FontDescriptor").cloned()?;
        pdf.resolve(&fd).and_then(|o| o.dict().cloned())
    };
    let embedded = if kind == "Type3" {
        true
    } else {
        let desc = if kind == "Type0" {
            let desc_fonts = get(d, "DescendantFonts").cloned().and_then(|x| pdf.resolve(&x));
            match desc_fonts {
                Some(Obj::Array(a)) => a.first().cloned().and_then(|f| pdf.resolve(&f)).and_then(|f| f.dict().cloned()).and_then(|f| descriptor_of(pdf, &f)),
                _ => None,
            }
        } else {
            descriptor_of(pdf, d)
        };
        desc.is_some_and(|fd| ["FontFile", "FontFile2", "FontFile3"].iter().any(|k| get(&fd, k).is_some()))
    };
    Some(super::PlacedFont { name, kind, embedded })
}

/// The colour space a PDF mostly is: CMYK if it uses CMYK at all and nothing else
/// non-gray; RGB / Lab when present.
fn dominant(spaces: &[ColorSpace], images: &[ColorSpace]) -> ColorSpace {
    let all: Vec<ColorSpace> = spaces.iter().chain(images).copied().collect();
    for s in [ColorSpace::Rgb, ColorSpace::Lab, ColorSpace::Cmyk, ColorSpace::Gray, ColorSpace::Indexed] {
        if all.contains(&s) {
            return s;
        }
    }
    ColorSpace::Other
}
