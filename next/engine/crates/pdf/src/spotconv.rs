//! The document's Ink Manager applied to placed PDFs: a Separation whose ink the Ink Manager
//! converts to process is replaced by its alternate space, with every colour value that
//! used it recomputed through the tint transform (content streams, images, shadings); an
//! aliased ink is renamed. Without this, a placed PDF would add plates the document says
//! not to print.

use lopdf::Object;
use std::collections::{HashMap, HashSet};

/// What the Ink Manager says about the inks of placed PDFs.
#[derive(Clone, Debug, Default)]
pub struct InkRules {
    pub convert: HashSet<String>,
    pub alias: HashMap<String, String>,
}

impl InkRules {
    pub fn from_doc(doc: &folio_model::Document) -> Self {
        let mut r = InkRules::default();
        for ink in &doc.print.inks {
            if ink.process {
                continue;
            }
            if let Some(a) = &ink.alias {
                let target = match a.as_str() {
                    "Process Cyan" => "Cyan",
                    "Process Magenta" => "Magenta",
                    "Process Yellow" => "Yellow",
                    "Process Black" => "Black",
                    other => other,
                };
                r.alias.insert(ink.name.clone(), target.to_owned());
            } else if ink.convert_to_process {
                r.convert.insert(ink.name.clone());
            }
        }
        r
    }
}

// -- functions ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Func {
    Exp { c0: Vec<f64>, c1: Vec<f64>, n: f64 },
    Sampled { size: usize, outs: usize, samples: Vec<f64>, encode: [f64; 2], decode: Vec<f64> },
    Stitch { domain: [f64; 2], bounds: Vec<f64>, encode: Vec<f64>, fns: Vec<Func> },
    PostScript(Vec<Tok>),
}

#[derive(Clone, Debug)]
pub enum Tok {
    Num(f64),
    Op(String),
    Block(Vec<Tok>),
}

fn deref<'a>(doc: &'a lopdf::Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn nums(doc: &lopdf::Document, o: Option<&Object>) -> Option<Vec<f64>> {
    let a = deref(doc, o?).as_array().ok()?;
    a.iter().map(|x| num(deref(doc, x))).collect()
}

pub fn parse_function(doc: &lopdf::Document, o: &Object) -> Option<Func> {
    let o = deref(doc, o);
    let (dict, data) = match o {
        Object::Dictionary(d) => (d, None),
        Object::Stream(s) => (&s.dict, Some(s.decompressed_content().ok().unwrap_or_else(|| s.content.clone()))),
        _ => return None,
    };
    let get = |k: &[u8]| dict.get(k).ok();
    match get(b"FunctionType").and_then(num)? as i64 {
        2 => Some(Func::Exp {
            c0: nums(doc, get(b"C0")).unwrap_or_else(|| vec![0.0]),
            c1: nums(doc, get(b"C1")).unwrap_or_else(|| vec![1.0]),
            n: get(b"N").and_then(num).unwrap_or(1.0),
        }),
        0 => {
            let size = nums(doc, get(b"Size"))?.first().copied()? as usize;
            let bps = get(b"BitsPerSample").and_then(num)? as u32;
            let range = nums(doc, get(b"Range"))?;
            let outs = range.len() / 2;
            let encode = nums(doc, get(b"Encode")).map(|e| [e[0], e[1]]).unwrap_or([0.0, (size.max(1) - 1) as f64]);
            let decode = nums(doc, get(b"Decode")).unwrap_or_else(|| range.clone());
            let data = data?;
            let max = ((1u64 << bps) - 1) as f64;
            let mut samples = Vec::with_capacity(size * outs);
            let mut bitpos = 0usize;
            for _ in 0..size * outs {
                let mut v = 0u64;
                for _ in 0..bps {
                    let byte = *data.get(bitpos / 8)? as u64;
                    v = (v << 1) | ((byte >> (7 - bitpos % 8)) & 1);
                    bitpos += 1;
                }
                samples.push(v as f64 / max);
            }
            Some(Func::Sampled { size, outs, samples, encode, decode })
        }
        3 => {
            let fns = deref(doc, get(b"Functions")?).as_array().ok()?.iter().map(|f| parse_function(doc, f)).collect::<Option<Vec<_>>>()?;
            let d = nums(doc, get(b"Domain")).unwrap_or_else(|| vec![0.0, 1.0]);
            Some(Func::Stitch {
                domain: [d[0], d[1]],
                bounds: nums(doc, get(b"Bounds")).unwrap_or_default(),
                encode: nums(doc, get(b"Encode")).unwrap_or_default(),
                fns,
            })
        }
        4 => {
            let text = String::from_utf8_lossy(&data?).into_owned();
            let mut it = text.split(|c: char| c.is_whitespace() || c == '{' || c == '}').peekable();
            let _ = &mut it;
            Some(Func::PostScript(ps_parse(&text)))
        }
        _ => None,
    }
}

fn ps_parse(text: &str) -> Vec<Tok> {
    fn block(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Vec<Tok> {
        let mut out = Vec::new();
        let mut word = String::new();
        let flush = |w: &mut String, out: &mut Vec<Tok>| {
            if !w.is_empty() {
                out.push(match w.parse::<f64>() {
                    Ok(v) => Tok::Num(v),
                    Err(_) => Tok::Op(std::mem::take(w)),
                });
                w.clear();
            }
        };
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    flush(&mut word, &mut out);
                    out.push(Tok::Block(block(chars)));
                }
                '}' => {
                    flush(&mut word, &mut out);
                    return out;
                }
                c if c.is_whitespace() => flush(&mut word, &mut out),
                c => word.push(c),
            }
        }
        flush(&mut word, &mut out);
        out
    }
    let mut chars = text.trim().chars().peekable();
    // the outer braces
    let toks = block(&mut chars);
    match toks.as_slice() {
        [Tok::Block(b)] => b.clone(),
        _ => toks,
    }
}

fn ps_run(toks: &[Tok], st: &mut Vec<f64>) -> Option<()> {
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Num(v) => st.push(*v),
            Tok::Block(_) => {
                // blocks are consumed by if / ifelse
                let a = match &toks[i] {
                    Tok::Block(b) => b,
                    _ => unreachable!(),
                };
                match toks.get(i + 1) {
                    Some(Tok::Op(op)) if op == "if" => {
                        if st.pop()? != 0.0 {
                            ps_run(a, st)?;
                        }
                        i += 1;
                    }
                    Some(Tok::Block(b)) if matches!(toks.get(i + 2), Some(Tok::Op(op)) if op == "ifelse") => {
                        if st.pop()? != 0.0 {
                            ps_run(a, st)?;
                        } else {
                            ps_run(b, st)?;
                        }
                        i += 2;
                    }
                    _ => return None,
                }
            }
            Tok::Op(op) => {
                let b1 = |st: &mut Vec<f64>, f: fn(f64) -> f64| -> Option<()> {
                    let a = st.pop()?;
                    st.push(f(a));
                    Some(())
                };
                let b2 = |st: &mut Vec<f64>, f: fn(f64, f64) -> f64| -> Option<()> {
                    let b = st.pop()?;
                    let a = st.pop()?;
                    st.push(f(a, b));
                    Some(())
                };
                let t = |c: bool| if c { 1.0 } else { 0.0 };
                match op.as_str() {
                    "add" => b2(st, |a, b| a + b)?,
                    "sub" => b2(st, |a, b| a - b)?,
                    "mul" => b2(st, |a, b| a * b)?,
                    "div" => b2(st, |a, b| if b == 0.0 { 0.0 } else { a / b })?,
                    "idiv" => b2(st, |a, b| if b == 0.0 { 0.0 } else { (a as i64 / b as i64) as f64 })?,
                    "mod" => b2(st, |a, b| if b == 0.0 { 0.0 } else { (a as i64 % b as i64) as f64 })?,
                    "exp" => b2(st, f64::powf)?,
                    "atan" => b2(st, |a, b| {
                        let d = b.atan2(a).to_degrees();
                        if d < 0.0 { d + 360.0 } else { d }
                    })?,
                    "neg" => b1(st, |a| -a)?,
                    "abs" => b1(st, f64::abs)?,
                    "ceiling" => b1(st, f64::ceil)?,
                    "floor" => b1(st, f64::floor)?,
                    "round" => b1(st, |a| (a + 0.5).floor())?,
                    "truncate" | "cvi" => b1(st, f64::trunc)?,
                    "cvr" => {}
                    "sqrt" => b1(st, |a| a.max(0.0).sqrt())?,
                    "sin" => b1(st, |a| a.to_radians().sin())?,
                    "cos" => b1(st, |a| a.to_radians().cos())?,
                    "ln" => b1(st, |a| a.max(1e-30).ln())?,
                    "log" => b1(st, |a| a.max(1e-30).log10())?,
                    "dup" => st.push(*st.last()?),
                    "pop" => {
                        st.pop()?;
                    }
                    "exch" => {
                        let n = st.len();
                        if n < 2 {
                            return None;
                        }
                        st.swap(n - 1, n - 2);
                    }
                    "index" => {
                        let k = st.pop()? as usize;
                        let v = *st.get(st.len().checked_sub(k + 1)?)?;
                        st.push(v);
                    }
                    "copy" => {
                        let k = st.pop()? as usize;
                        let n = st.len();
                        let from = n.checked_sub(k)?;
                        for j in from..n {
                            st.push(st[j]);
                        }
                    }
                    "roll" => {
                        let j = st.pop()? as i64;
                        let n = st.pop()? as usize;
                        let len = st.len();
                        if n > len || n == 0 {
                            if n == 0 {
                                i += 1;
                                continue;
                            }
                            return None;
                        }
                        let s = &mut st[len - n..];
                        let j = j.rem_euclid(n as i64) as usize;
                        s.rotate_right(j);
                    }
                    "eq" => b2(st, |a, b| if a == b { 1.0 } else { 0.0 })?,
                    "ne" => b2(st, |a, b| if a != b { 1.0 } else { 0.0 })?,
                    "gt" => b2(st, |a, b| if a > b { 1.0 } else { 0.0 })?,
                    "ge" => b2(st, |a, b| if a >= b { 1.0 } else { 0.0 })?,
                    "lt" => b2(st, |a, b| if a < b { 1.0 } else { 0.0 })?,
                    "le" => b2(st, |a, b| if a <= b { 1.0 } else { 0.0 })?,
                    "and" => b2(st, |a, b| ((a as i64) & (b as i64)) as f64)?,
                    "or" => b2(st, |a, b| ((a as i64) | (b as i64)) as f64)?,
                    "xor" => b2(st, |a, b| ((a as i64) ^ (b as i64)) as f64)?,
                    "not" => b1(st, |a| if a == 0.0 { 1.0 } else if a == 1.0 { 0.0 } else { !(a as i64) as f64 })?,
                    "true" => st.push(t(true)),
                    "false" => st.push(t(false)),
                    _ => return None,
                }
            }
        }
        i += 1;
    }
    Some(())
}

impl Func {
    pub fn eval(&self, x: &[f64]) -> Option<Vec<f64>> {
        match self {
            Func::Exp { c0, c1, n } => {
                let t = x.first().copied().unwrap_or(0.0).clamp(0.0, 1.0).powf(*n);
                Some(c0.iter().zip(c1).map(|(a, b)| a + t * (b - a)).collect())
            }
            Func::Sampled { size, outs, samples, encode, decode } => {
                let t = x.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
                let e = (encode[0] + t * (encode[1] - encode[0])).clamp(0.0, (*size - 1) as f64);
                let (i0, f) = (e.floor() as usize, e - e.floor());
                let i1 = (i0 + 1).min(size - 1);
                Some(
                    (0..*outs)
                        .map(|k| {
                            let s = samples[i0 * outs + k] * (1.0 - f) + samples[i1 * outs + k] * f;
                            decode[2 * k] + s * (decode[2 * k + 1] - decode[2 * k])
                        })
                        .collect(),
                )
            }
            Func::Stitch { domain, bounds, encode, fns } => {
                let t = x.first().copied().unwrap_or(0.0).clamp(domain[0], domain[1]);
                let k = bounds.iter().position(|b| t < *b).unwrap_or(bounds.len());
                let lo = if k == 0 { domain[0] } else { bounds[k - 1] };
                let hi = if k == bounds.len() { domain[1] } else { bounds[k] };
                let (e0, e1) = (encode.get(2 * k).copied().unwrap_or(0.0), encode.get(2 * k + 1).copied().unwrap_or(1.0));
                let u = if hi > lo { e0 + (t - lo) / (hi - lo) * (e1 - e0) } else { e0 };
                fns.get(k)?.eval(&[u])
            }
            Func::PostScript(toks) => {
                let mut st = x.to_vec();
                ps_run(toks, &mut st)?;
                Some(st)
            }
        }
    }
}

// -- colour spaces -----------------------------------------------------------------------

/// A Separation the Ink Manager converts: its alternate space and tint transform.
#[derive(Clone, Debug)]
pub struct Conversion {
    pub ink: String,
    /// The alternate colour space object (copied in place of the Separation).
    pub alternate: Object,
    pub components: usize,
    pub tint: Func,
}

impl Conversion {
    pub fn values(&self, tint: f64) -> Vec<f64> {
        self.tint.eval(&[tint]).unwrap_or_else(|| vec![0.0; self.components])
    }
}

fn components(doc: &lopdf::Document, cs: &Object) -> usize {
    match deref(doc, cs) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceCMYK" => 4,
            b"DeviceRGB" | b"CalRGB" | b"Lab" => 3,
            _ => 1,
        },
        Object::Array(a) => match a.first().and_then(|x| x.as_name().ok()) {
            Some(b"Lab") | Some(b"CalRGB") => 3,
            Some(b"CalGray") => 1,
            Some(b"ICCBased") => a.get(1).map(|s| deref(doc, s)).and_then(|s| s.as_stream().ok()).and_then(|s| s.dict.get(b"N").ok()).and_then(num).unwrap_or(3.0) as usize,
            _ => 1,
        },
        _ => 1,
    }
}

// Keep conversions inline: boxing changes allocation and the existing conversion representation.
#[allow(clippy::large_enum_variant)]
pub enum CsAction {
    Keep,
    /// Rename the Separation's colorant (alias).
    Rename(String),
    Convert(Conversion),
}

/// What the Ink Manager does to a colour space of a placed PDF.
pub fn classify(doc: &lopdf::Document, cs: &Object, rules: &InkRules) -> CsAction {
    let Object::Array(a) = deref(doc, cs) else { return CsAction::Keep };
    if a.first().and_then(|x| x.as_name().ok()) != Some(b"Separation") || a.len() < 4 {
        return CsAction::Keep;
    }
    let Some(name) = a[1].as_name().ok().map(|n| String::from_utf8_lossy(n).into_owned()) else { return CsAction::Keep };
    if let Some(to) = rules.alias.get(&name) {
        return CsAction::Rename(to.clone());
    }
    if !rules.convert.contains(&name) {
        return CsAction::Keep;
    }
    let alt = deref(doc, &a[2]).clone();
    match parse_function(doc, &a[3]) {
        Some(f) => CsAction::Convert(Conversion { ink: name, components: components(doc, &alt), alternate: alt, tint: f }),
        None => CsAction::Keep,
    }
}

// -- content streams ---------------------------------------------------------------------

/// DeviceRGB (0–1) → DeviceCMYK (0–1), the conversion of CMYK-only output.
pub type RgbToCmyk<'a> = &'a dyn Fn([f32; 3]) -> [f32; 4];

/// What the current fill or stroke colour space of a content stream becomes.
#[derive(Clone, Copy)]
enum Paint<'a> {
    Kept,
    /// A converted Separation: tints become the alternate's values.
    Spot(&'a Conversion),
    /// DeviceRGB written as DeviceCMYK (CMYK-only output).
    Rgb,
}

/// Rewrites colour operators of a content stream: `/CSn cs` / `CS` naming a converted space
/// keep their name (the resource now holds the alternate), and the tint operands of
/// `sc`/`scn`/`SC`/`SCN` become the alternate's values. With `rgb` (CMYK-only output),
/// DeviceRGB paint becomes DeviceCMYK: `rg`/`RG` → `k`/`K`, `/DeviceRGB cs` → `/DeviceCMYK cs`
/// and its `sc` operands. RGB this does not reach (inline images, resource colour spaces) is
/// left for the caller to detect. Returns `None` when nothing changed.
pub fn rewrite_content(data: &[u8], spaces: &HashMap<Vec<u8>, Conversion>, rgb: Option<RgbToCmyk<'_>>) -> Option<Vec<u8>> {
    if spaces.is_empty() && rgb.is_none() {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() + 64);
    let mut operands: Vec<(usize, usize)> = Vec::new(); // spans of the current operands
    let (mut fill, mut stroke) = (Paint::Kept, Paint::Kept);
    let mut stack: Vec<(Paint<'_>, Paint<'_>)> = Vec::new();
    let number = |(a, b): (usize, usize)| std::str::from_utf8(&data[a..b]).ok().and_then(|s| s.parse::<f32>().ok());
    // the last three operands as an RGB colour
    let rgb_operands = |operands: &[(usize, usize)]| -> Option<(usize, [f32; 3])> {
        let s = operands.get(operands.len().checked_sub(3)?..)?;
        Some((s[0].0, [number(s[0])?, number(s[1])?, number(s[2])?]))
    };
    let mut changed = false;
    let mut i = 0usize;
    let mut last = 0usize; // copied up to here
    let n = data.len();
    while i < n {
        let c = data[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'%' {
            while i < n && data[i] != b'\n' && data[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        let start = i;
        match c {
            b'(' => {
                let mut depth = 0;
                while i < n {
                    match data[i] {
                        b'\\' => i += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                operands.push((start, i));
            }
            b'<' if data.get(i + 1) == Some(&b'<') => {
                // dictionary operand (inline image params, marked content): skip to >>
                let mut depth = 0;
                while i + 1 < n {
                    if data[i] == b'<' && data[i + 1] == b'<' {
                        depth += 1;
                        i += 2;
                        continue;
                    }
                    if data[i] == b'>' && data[i + 1] == b'>' {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    i += 1;
                }
                operands.push((start, i));
            }
            b'<' => {
                while i < n && data[i] != b'>' {
                    i += 1;
                }
                i += 1;
                operands.push((start, i));
            }
            b'[' | b']' | b'{' | b'}' => {
                i += 1;
                operands.push((start, i));
            }
            b'/' => {
                i += 1;
                while i < n && !data[i].is_ascii_whitespace() && !b"/[]()<>{}%".contains(&data[i]) {
                    i += 1;
                }
                operands.push((start, i));
            }
            _ => {
                while i < n && !data[i].is_ascii_whitespace() && !b"/[]()<>{}%".contains(&data[i]) {
                    i += 1;
                }
                if i == start {
                    // a stray delimiter (')' or '>'): skip it
                    i += 1;
                    continue;
                }
                let tok = &data[start..i];
                let is_num = tok.iter().all(|b| b.is_ascii_digit() || *b == b'.' || *b == b'-' || *b == b'+');
                if is_num {
                    operands.push((start, i));
                    continue;
                }
                match tok {
                    b"q" => stack.push((fill, stroke)),
                    b"Q" => {
                        if let Some((f, s)) = stack.pop() {
                            fill = f;
                            stroke = s;
                        }
                    }
                    b"cs" | b"CS" => {
                        let set_fill = tok == b"cs";
                        let name = operands.last().map(|&(a, b)| (a, b, &data[a + 1..b]));
                        let paint = match (name, rgb) {
                            (Some((_, _, b"DeviceRGB")), Some(_)) => Paint::Rgb,
                            (Some((_, _, n)), _) => spaces.get(n).map_or(Paint::Kept, Paint::Spot),
                            (None, _) => Paint::Kept,
                        };
                        if set_fill {
                            fill = paint;
                        } else {
                            stroke = paint;
                        }
                        // the new space's initial colour differs from the old one's: set it
                        let initial = match paint {
                            Paint::Spot(cv) => Some(cv.values(1.0)),
                            Paint::Rgb => rgb.map(|f| f([0.0; 3]).iter().map(|v| f64::from(*v)).collect()),
                            Paint::Kept => None,
                        };
                        if let Some(v) = initial {
                            match (paint, name) {
                                (Paint::Rgb, Some((a, b, _))) => {
                                    out.extend_from_slice(&data[last..a]);
                                    out.extend_from_slice(b"/DeviceCMYK");
                                    out.extend_from_slice(&data[b..i]);
                                }
                                _ => out.extend_from_slice(&data[last..i]),
                            }
                            last = i;
                            out.push(b'\n');
                            for x in &v {
                                out.extend_from_slice(format!("{} ", fmt(*x)).as_bytes());
                            }
                            out.extend_from_slice(if set_fill { b"sc" } else { b"SC" });
                            changed = true;
                        }
                    }
                    b"sc" | b"scn" | b"SC" | b"SCN" => {
                        let paint = if tok[0] == b's' { fill } else { stroke };
                        let replaced = match (paint, rgb) {
                            (Paint::Spot(cv), _) => operands
                                .last()
                                .and_then(|&(a, b)| Some((a, b, std::str::from_utf8(&data[a..b]).ok()?.parse::<f64>().ok()?)))
                                .map(|(a, b, t)| (a, b, cv.values(t))),
                            (Paint::Rgb, Some(f)) => rgb_operands(&operands)
                                .map(|(a, c)| (a, operands[operands.len() - 1].1, f(c).iter().map(|v| f64::from(*v)).collect())),
                            _ => None,
                        };
                        if let Some((a, b, v)) = replaced {
                            out.extend_from_slice(&data[last..a]);
                            let text: Vec<String> = v.iter().map(|x| fmt(*x)).collect();
                            out.extend_from_slice(text.join(" ").as_bytes());
                            out.extend_from_slice(&data[b..i]);
                            last = i;
                            changed = true;
                        }
                    }
                    b"rg" | b"RG" => {
                        if let Some(f) = rgb
                            && let Some((a, c)) = rgb_operands(&operands)
                        {
                            out.extend_from_slice(&data[last..a]);
                            let text: Vec<String> = f(c).iter().map(|v| fmt(f64::from(*v))).collect();
                            out.extend_from_slice(text.join(" ").as_bytes());
                            out.extend_from_slice(if tok == b"rg" { b" k" } else { b" K" });
                            last = i;
                            changed = true;
                            // DeviceCMYK now, where the source's later `sc` would be RGB
                            if tok == b"rg" {
                                fill = Paint::Rgb;
                            } else {
                                stroke = Paint::Rgb;
                            }
                        }
                    }
                    b"g" | b"k" => fill = Paint::Kept,
                    b"G" | b"K" => stroke = Paint::Kept,
                    b"BI" => {
                        // inline image: skip to EI (its colour space is not converted)
                        if let Some(p) = find(data, b"EI", i) {
                            i = p + 2;
                        }
                    }
                    _ => {}
                }
                operands.clear();
                continue;
            }
        }
    }
    if !changed {
        return None;
    }
    out.extend_from_slice(&data[last..]);
    Some(out)
}

fn find(h: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    h.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn fmt(v: f64) -> String {
    let s = format!("{:.5}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".into() } else { s.to_owned() }
}

/// Image samples (8-bit, one tint per pixel) converted to the alternate space.
pub fn convert_samples(cv: &Conversion, samples: &[u8], decode_inverted: bool) -> Vec<u8> {
    let table: Vec<Vec<u8>> = (0..256)
        .map(|s| {
            let t = if decode_inverted { 1.0 - s as f64 / 255.0 } else { s as f64 / 255.0 };
            cv.values(t).iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
        })
        .collect();
    let mut out = Vec::with_capacity(samples.len() * cv.components);
    for s in samples {
        out.extend_from_slice(&table[*s as usize]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postscript_and_exponential_functions() {
        let f = Func::PostScript(ps_parse("{ dup 0.5 mul exch 0 exch 1 }"));
        assert_eq!(f.eval(&[0.8]).unwrap(), vec![0.4, 0.0, 0.8, 1.0]);
        let f = Func::PostScript(ps_parse("{ dup 0.5 gt { pop 1 } { 2 mul } ifelse }"));
        assert_eq!(f.eval(&[0.7]).unwrap(), vec![1.0]);
        assert_eq!(f.eval(&[0.2]).unwrap(), vec![0.4]);
        let e = Func::Exp { c0: vec![0.0, 0.0, 0.0, 0.0], c1: vec![0.0, 0.6, 1.0, 0.0], n: 1.0 };
        assert_eq!(e.eval(&[0.5]).unwrap(), vec![0.0, 0.3, 0.5, 0.0]);
    }

    #[test]
    fn content_rewrite_converts_tints() {
        let cv = Conversion {
            ink: "PANTONE X".into(),
            alternate: Object::Name(b"DeviceCMYK".to_vec()),
            components: 4,
            tint: Func::Exp { c0: vec![0.0; 4], c1: vec![1.0, 0.5, 0.0, 0.0], n: 1.0 },
        };
        let spaces: HashMap<Vec<u8>, Conversion> = [(b"CS0".to_vec(), cv)].into_iter().collect();
        let src = b"q /CS0 cs 0.5 scn 0 0 10 10 re f Q (a) Tj 1 0 0 1 k";
        let out = String::from_utf8(rewrite_content(src, &spaces, None).unwrap()).unwrap();
        assert!(out.contains("/CS0 cs\n1 0.5 0 0 sc"), "{out}");
        assert!(out.contains("0.5 0.25 0 0 scn"), "{out}");
        assert!(out.ends_with("(a) Tj 1 0 0 1 k"), "{out}");
    }

    #[test]
    fn content_rewrite_converts_device_rgb() {
        let f = |c: [f32; 3]| [1.0 - c[0], 1.0 - c[1], 1.0 - c[2], 0.5];
        let src = b"q .25 .5 1 rg 0 0 1 RG 0 0 10 10 re B Q /DeviceRGB cs 1 1 1 sc (rg) Tj 0.5 g 1 0 0 0 K";
        let out = String::from_utf8(rewrite_content(src, &HashMap::new(), Some(&f)).unwrap()).unwrap();
        assert!(out.starts_with("q 0.75 0.5 0 0.5 k 1 1 0 0.5 K 0 0 10 10 re B Q"), "{out}");
        assert!(out.contains("/DeviceCMYK cs\n1 1 1 0.5 sc 0 0 0 0.5 sc (rg) Tj 0.5 g 1 0 0 0 K"), "{out}");
        assert_eq!(rewrite_content(b"0.5 g 0 0 1 1 re f", &HashMap::new(), Some(&f)), None);
    }
}
