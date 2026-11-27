//! Indirect objects, cross-reference tables and streams, and catalog recovery.

use std::collections::{HashMap, HashSet};

use super::filters;
use super::syntax::{Dict, Lexer, Obj, find, get};
use crate::probe::{ProbeError, Result};

#[derive(Clone, Copy)]
enum Loc {
    Offset(usize),
    InStream(u32, u32),
}

type ObjectStream = std::rc::Rc<(Vec<u8>, Vec<(u32, usize)>)>;

pub(super) struct Pdf<'a> {
    b: &'a [u8],
    xref: HashMap<u32, Loc>,
    trailer: Dict,
    objstm: HashMap<u32, Option<ObjectStream>>,
    cache: HashMap<u32, Obj>,
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

impl<'a> Pdf<'a> {
    pub(super) fn open(b: &'a [u8]) -> Result<(Self, Dict)> {
        let mut pdf = Self {
            b,
            xref: HashMap::new(),
            trailer: Dict::new(),
            objstm: HashMap::new(),
            cache: HashMap::new(),
        };
        let tail_from = b.len().saturating_sub(4096);
        let ok = rfind(&b[tail_from..], b"startxref")
            .and_then(|p| Lexer { b, pos: tail_from + p + 9 }.int_ahead(tail_from + p + 9))
            .is_some_and(|(off, _)| read_xref(&mut pdf, off as usize));
        let mut root = get(&pdf.trailer, "Root").cloned();
        if !ok || root.as_ref().and_then(|r| pdf.resolve(r)).is_none() {
            pdf.xref.clear();
            pdf.cache.clear();
            scan_objects(&mut pdf);
            root = get(&pdf.trailer, "Root").cloned();
            if root.as_ref().and_then(|r| pdf.resolve(r)).is_none() {
                // find the catalog by type
                let nums: Vec<u32> = pdf.xref.keys().copied().collect();
                root = nums.into_iter().find(|n| {
                    pdf.get_obj(*n).is_some_and(|o| {
                        o.dict().and_then(|d| get(d, "Type")).and_then(Obj::name) == Some("Catalog")
                    })
                }).map(Obj::Ref);
            }
        }
        let catalog = root
            .as_ref()
            .and_then(|r| pdf.resolve(r))
            .and_then(|o| o.dict().cloned())
            .ok_or_else(|| ProbeError::Malformed("PDF without catalog".into()))?;
        Ok((pdf, catalog))
    }

    pub(super) fn is_encrypted(&self) -> bool {
        get(&self.trailer, "Encrypt").is_some()
    }

    pub(super) fn object_numbers(&self) -> Vec<u32> {
        let mut numbers: Vec<_> = self.xref.keys().copied().collect();
        numbers.sort_unstable();
        numbers
    }

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

    pub(super) fn resolve(&mut self, o: &Obj) -> Option<Obj> {
        match o {
            Obj::Ref(r) => self.get_obj(*r),
            other => Some(other.clone()),
        }
    }

    pub(super) fn get_obj(&mut self, num: u32) -> Option<Obj> {
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

    pub(super) fn decode(&self, dict: &Dict, start: usize, end: usize) -> Option<Vec<u8>> {
        filters::decode(dict, self.b.get(start..end)?)
    }
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
