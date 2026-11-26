//! SVG: root element size (width / height with units, else the viewBox). RGB, vector.

use super::{ColorSpace, ImageFormat, ImageInfo, Result, VectorColors, malformed};

pub(crate) fn looks_like_svg(head: &[u8]) -> bool {
    let n = head.len().min(4096);
    head[..n].windows(4).any(|w| w == b"<svg")
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(i) = rest.find(name) {
        let before_ok = i == 0 || rest.as_bytes()[i - 1].is_ascii_whitespace();
        let after = rest[i + name.len()..].trim_start();
        if before_ok && let Some(v) = after.strip_prefix('=') {
            let v = v.trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let end = v[1..].find(q)?;
                return Some(&v[1..1 + end]);
            }
        }
        rest = &rest[i + name.len()..];
    }
    None
}

/// Length in points (CSS: 1px = 0.75pt).
fn length(v: &str) -> Option<f64> {
    let v = v.trim();
    let split = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')).unwrap_or(v.len());
    let n: f64 = v[..split].parse().ok()?;
    let f = match v[split..].trim() {
        "" | "px" => 0.75,
        "pt" => 1.0,
        "pc" => 12.0,
        "mm" => 72.0 / 25.4,
        "cm" => 72.0 / 2.54,
        "in" => 72.0,
        _ => return None, // %, em: depends on the placing context
    };
    Some(n * f)
}

pub fn svg_size(path: &std::path::Path) -> Result<[f64; 2]> {
    let text = std::fs::read_to_string(path).map_err(|e|super::ProbeError::Io(e.to_string()))?;
    let Some(start) = text.find("<svg") else { return malformed("SVG root") };
    let Some(end) = text[start..].find('>') else { return malformed("SVG root tag") };
    let tag = &text[start..start + end];
    let vb: Vec<f64> = attr(tag,"viewBox").unwrap_or("").split(|c:char|c==',' || c.is_whitespace()).filter(|s|!s.is_empty()).filter_map(|s|s.parse().ok()).collect();
    let axis = |name, index| attr(tag,name).and_then(length).map(|v|v/0.75).or_else(||if vb.len()==4 {Some(vb[index])} else {None});
    match (axis("width",2),axis("height",3)) {
        (Some(w),Some(h)) if w.is_finite() && h.is_finite() && w>0. && h>0. => Ok([w,h]),
        _ => malformed("SVG dimensions"),
    }
}

pub(crate) fn probe(head: &[u8]) -> Result<ImageInfo> {
    let text = String::from_utf8_lossy(head);
    let Some(start) = text.find("<svg") else { return malformed("SVG root") };
    let Some(end) = text[start..].find('>') else { return malformed("SVG root tag") };
    let tag = &text[start..start + end];
    let vb: Option<Vec<f64>> = attr(tag, "viewBox").map(|v| v.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect());
    let vb = vb.filter(|v| v.len() == 4);
    let w = attr(tag, "width").and_then(length).or(vb.as_ref().map(|v| v[2] * 0.75));
    let h = attr(tag, "height").and_then(length).or(vb.as_ref().map(|v| v[3] * 0.75));
    let mut info = ImageInfo::raster(ImageFormat::Svg, w.unwrap_or(0.0).ceil() as u32, h.unwrap_or(0.0).ceil() as u32, ColorSpace::Rgb, 0, 3);
    info.vector = true;
    info.vector_colors = Some(VectorColors { spaces: vec![ColorSpace::Rgb], ..VectorColors::default() });
    Ok(info)
}
