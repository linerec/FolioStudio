//! Stream filters and predictors used for object lookup and metadata.

use super::syntax::{Dict, Obj, get, is_ws};
use crate::probe::inflate;

/// Largest decoded object / xref stream (bytes).
const MAX_STREAM: usize = 64 << 20;

pub(super) fn decode(d: &Dict, raw: &[u8]) -> Option<Vec<u8>> {
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
