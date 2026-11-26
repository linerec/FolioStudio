//! Windows metafiles (EMF and WMF) as the static SVG the SVG reader draws (build-a): the common drawing records become paths with a
//! solid fill and stroke, so the screen and the PDF draw them as vectors through the SVG path (`svg::read`). Read:
//! - EMF: the header's frame (0.01 mm) and device size; map modes (text, isotropic, anisotropic, metric, English, twips), window and
//!   viewport origin and extent, world transform (set, modify), save and restore; pens (CREATEPEN, EXTCREATEPEN) and solid brushes,
//!   stock objects; polygons, polylines, Béziers (16- and 32-bit points, poly-poly), rectangles, rounded rectangles, ellipses, move and
//!   line, paths (BEGINPATH … FILLPATH / STROKEPATH / STROKEANDFILLPATH), fill mode.
//! - WMF: the placeable header (bounding box and units per inch) or the window; the same shapes and objects in 16 bits.
//!
//! - Text (EMF EXTTEXTOUTW/A, WMF TEXTOUT/EXTTEXTOUT) with its font (LOGFONT), colour, alignment, opaque background and glyph
//!   advances as `<text>`; bitmaps (EMF STRETCHDIBITS, SETDIBITSTODEVICE, BITBLT, STRETCHBLT, ALPHABLEND; WMF STRETCHDIB,
//!   DIBSTRETCHBLT, DIBBITBLT) as PNG `<image>`s (build-a2, `content.rs`). resvg draws those (svg/rich.rs).
//!
//! Clipping, hatches, patterns and raster operations other than a plain copy are not drawn; [`Converted::skipped`] names them
//! and how many records there were.
mod content;
mod emf;
mod wmf;

use emf::emf;
use wmf::wmf;

/// The SVG of a metafile, its size in points and the records not drawn (record name, count).
#[derive(Clone, Debug)]
pub struct Converted {
    pub svg: String,
    pub size: [f64; 2],
    pub skipped: Vec<(String, usize)>,
}

/// Whether `b` starts like an EMF (record 1 with the ` EMF` signature) or a WMF (placeable key, or a memory or disk metafile header).
pub fn is_metafile(b: &[u8]) -> bool {
    is_emf(b) || is_wmf(b)
}

fn is_emf(b: &[u8]) -> bool {
    b.len() >= 88 && u32le(b, 0) == Some(1) && b.get(40..44) == Some(b" EMF")
}

fn is_wmf(b: &[u8]) -> bool {
    b.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]) || (b.len() >= 18 && matches!(u16le(b, 0), Some(1 | 2)) && u16le(b, 2) == Some(9) && matches!(u16le(b, 4), Some(0x0100 | 0x0300)))
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}
fn i16le(b: &[u8], at: usize) -> Option<i16> {
    u16le(b, at).map(|v| v as i16)
}
fn u32le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
fn i32le(b: &[u8], at: usize) -> Option<i32> {
    u32le(b, at).map(|v| v as i32)
}
fn f32le(b: &[u8], at: usize) -> Option<f64> {
    u32le(b, at).map(|v| f32::from_bits(v) as f64)
}

pub fn convert(b: &[u8]) -> Result<Converted, String> {
    if is_emf(b) {
        emf(b)
    } else if is_wmf(b) {
        wmf(b)
    } else {
        Err("metafile: not an EMF or WMF file".into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Pen {
    color: Option<[u8; 3]>,
    width: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Brush {
    color: Option<[u8; 3]>,
}
#[derive(Clone, Copy, Debug)]
enum Obj {
    Pen(Pen),
    Brush(Brush),
    /// A font: its index in `Out::fonts`.
    Font(usize),
    Other,
}

/// x' = a x + c y + e, y' = b x + d y + f
#[derive(Clone, Copy, Debug)]
struct M([f64; 6]);
impl M {
    const ID: M = M([1., 0., 0., 1., 0., 0.]);
    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
    /// `self` then `o`.
    fn then(&self, o: &M) -> M {
        let (a, b) = (self.0, o.0);
        M([
            a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3],
            a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3],
            a[4] * b[0] + a[5] * b[2] + b[4], a[4] * b[1] + a[5] * b[3] + b[5],
        ])
    }
}

#[derive(Clone, Copy, Debug)]
struct State {
    pen: Pen,
    brush: Brush,
    evenodd: bool,
    world: M,
    mode: u32,
    win_org: (f64, f64),
    win_ext: (f64, f64),
    vp_org: (f64, f64),
    vp_ext: (f64, f64),
    at: (f64, f64),
    text_color: [u8; 3],
    text_align: u32,
    /// SETBKMODE: 2 opaque (the text's box filled with `bk_color`), 1 transparent.
    bk_mode: u32,
    bk_color: [u8; 3],
    /// The selected font (`Out::fonts`), none: the system font.
    font: Option<usize>,
}

/// The drawing being made: shapes as SVG elements, the path bracket, what was skipped.
struct Out {
    shapes: String,
    path: Option<String>,
    skipped: std::collections::BTreeMap<String, usize>,
    /// Device units per logical unit of a fixed map mode (EMF), or 1.
    metric: f64,
    fonts: Vec<content::Font>,
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

impl State {
    fn new() -> Self {
        State {
            pen: Pen { color: Some([0, 0, 0]), width: 0.0 },
            brush: Brush { color: Some([255, 255, 255]) },
            evenodd: true,
            world: M::ID,
            mode: 1,
            win_org: (0., 0.),
            win_ext: (1., 1.),
            vp_org: (0., 0.),
            vp_ext: (1., 1.),
            at: (0., 0.),
            text_color: [0, 0, 0],
            text_align: 0,
            bk_mode: 2,
            bk_color: [255, 255, 255],
            font: None,
        }
    }

    /// Logical → device (EMF: world transform, then the map mode's page → device).
    fn device(&self, x: f64, y: f64, metric: f64) -> (f64, f64) {
        let (x, y) = self.world.apply(x, y);
        let (sx, sy) = match self.mode {
            7 | 8 => {
                let sx = if self.win_ext.0 != 0.0 { self.vp_ext.0 / self.win_ext.0 } else { 1.0 };
                let sy = if self.win_ext.1 != 0.0 { self.vp_ext.1 / self.win_ext.1 } else { 1.0 };
                if self.mode == 7 {
                    // isotropic: the smaller scale on both axes, signs kept
                    let s = sx.abs().min(sy.abs());
                    (s * sx.signum(), s * sy.signum())
                } else {
                    (sx, sy)
                }
            }
            2..=6 => (metric, -metric),
            _ => (1.0, 1.0),
        };
        ((x - self.win_org.0) * sx + self.vp_org.0, (y - self.win_org.1) * sy + self.vp_org.1)
    }

    /// The device size of a length of the logical space (a pen width).
    fn device_len(&self, v: f64, metric: f64) -> f64 {
        let (x0, y0) = self.device(0.0, 0.0, metric);
        let (x1, y1) = self.device(v, 0.0, metric);
        let (x2, y2) = self.device(0.0, v, metric);
        (((x1 - x0).hypot(y1 - y0)) * ((x2 - x0).hypot(y2 - y0))).sqrt()
    }
}

impl Out {
    fn new(metric: f64) -> Self {
        Out { shapes: String::new(), path: None, skipped: Default::default(), metric, fonts: Vec::new() }
    }

    /// Text at the logical reference point `at` (or the current position with TA_UPDATECP), `dx` the logical advance of each
    /// character (empty: the font's own); `opaque` the logical box ETO_OPAQUE fills with the background colour.
    fn text(&mut self, s: &mut State, at: (f64, f64), chars: &str, dx: &[f64], opaque: Option<[f64; 4]>) {
        if let Some([l, t, r, b]) = opaque {
            let d = self.poly(s, &[(l, t), (r, t), (r, b), (l, b)], true);
            self.shapes.push_str(&format!("<path d=\"{}\" fill=\"{}\" stroke=\"none\"/>\n", d.trim(), hex(s.bk_color)));
        }
        let at = if s.text_align & 1 != 0 { s.at } else { at };
        let system = content::Font { face: String::new(), height: -12.0, weight: 400, ..Default::default() };
        let font = s.font.and_then(|i| self.fonts.get(i)).cloned().unwrap_or(system);
        let em = s.device_len(font.height.abs(), self.metric) * if font.height > 0.0 { 0.8 } else { 1.0 };
        let (x, y) = s.device(at.0, at.1, self.metric);
        let mut xs = vec![x];
        let mut run = 0.0;
        for d in dx.iter().take(chars.chars().count().saturating_sub(1)) {
            run += d;
            xs.push(s.device(at.0 + run, at.1, self.metric).0);
        }
        self.shapes.push_str(&content::text_element(content::Text { font: &font, size: em, color: s.text_color, align: s.text_align, xs: &xs, y }, chars));
        if s.text_align & 1 != 0 {
            s.at = (at.0 + dx.iter().sum::<f64>(), at.1);
        }
    }

    /// A bitmap onto the logical rectangle from (x, y), `w` × `h`.
    fn bitmap(&mut self, s: &State, picture: Option<(&'static str, Vec<u8>)>, (x, y, w, h): (f64, f64, f64, f64), opacity: f64) {
        let Some((mime, data)) = picture else {
            self.skip("bitmap (unreadable format)");
            return;
        };
        let a = s.device(x, y, self.metric);
        let b = s.device(x + w, y + h, self.metric);
        self.shapes.push_str(&content::image_element(mime, &data, a, b, opacity));
    }

    fn skip(&mut self, what: &str) {
        *self.skipped.entry(what.to_owned()).or_default() += 1;
    }

    /// Path data of points (device units); `close` ends with Z.
    fn poly(&self, s: &State, pts: &[(f64, f64)], close: bool) -> String {
        let mut d = String::new();
        for (k, (x, y)) in pts.iter().enumerate() {
            let (x, y) = s.device(*x, *y, self.metric);
            d.push_str(if k == 0 { "M" } else { "L" });
            d.push_str(&format!("{} {} ", num(x), num(y)));
        }
        if close && !d.is_empty() {
            d.push('Z');
        }
        d
    }

    /// Bézier runs: `start` then groups of three points.
    fn bezier(&self, s: &State, start: Option<(f64, f64)>, pts: &[(f64, f64)]) -> String {
        let mut d = String::new();
        let mut it = pts.iter();
        let first = start.or_else(|| it.next().copied());
        if let Some((x, y)) = first {
            let (x, y) = s.device(x, y, self.metric);
            d.push_str(&format!("M{} {} ", num(x), num(y)));
        }
        let rest: Vec<(f64, f64)> = it.copied().collect();
        for c in rest.chunks_exact(3) {
            d.push('C');
            for (x, y) in c {
                let (x, y) = s.device(*x, *y, self.metric);
                d.push_str(&format!("{} {} ", num(x), num(y)));
            }
        }
        d
    }

    fn ellipse(&self, s: &State, l: f64, t: f64, r: f64, b: f64) -> String {
        // four Béziers (k = 0.5523)
        let (cx, cy, rx, ry) = ((l + r) / 2.0, (t + b) / 2.0, (r - l) / 2.0, (b - t) / 2.0);
        let k = 0.552_284_75;
        let p = [
            (cx + rx, cy), (cx + rx, cy + ry * k), (cx + rx * k, cy + ry), (cx, cy + ry), (cx - rx * k, cy + ry), (cx - rx, cy + ry * k), (cx - rx, cy),
            (cx - rx, cy - ry * k), (cx - rx * k, cy - ry), (cx, cy - ry), (cx + rx * k, cy - ry), (cx + rx, cy - ry * k), (cx + rx, cy),
        ];
        let mut d = self.bezier(s, None, &p);
        d.push('Z');
        d
    }

    fn round_rect(&self, s: &State, l: f64, t: f64, r: f64, b: f64, w: f64, h: f64) -> String {
        let (rx, ry) = ((w / 2.0).min((r - l).abs() / 2.0), (h / 2.0).min((b - t).abs() / 2.0));
        if rx <= 0.0 || ry <= 0.0 {
            return self.poly(s, &[(l, t), (r, t), (r, b), (l, b)], true);
        }
        let k = 0.552_284_75;
        let p = [
            (l + rx, t), (r - rx, t), (r - rx + rx * k, t), (r, t + ry - ry * k), (r, t + ry), (r, b - ry), (r, b - ry + ry * k), (r - rx + rx * k, b), (r - rx, b),
            (l + rx, b), (l + rx - rx * k, b), (l, b - ry + ry * k), (l, b - ry), (l, t + ry), (l, t + ry - ry * k), (l + rx - rx * k, t), (l + rx, t),
        ];
        let dev = |i: usize| { let (x, y) = s.device(p[i].0, p[i].1, self.metric); format!("{} {}", num(x), num(y)) };
        format!("M{} L{} C{} {} {} L{} C{} {} {} L{} C{} {} {} L{} C{} {} {} Z", dev(0), dev(1), dev(2), dev(3), dev(4), dev(5), dev(6), dev(7), dev(8), dev(9), dev(10), dev(11), dev(12), dev(13), dev(14), dev(15), dev(16))
    }

    /// A figure: kept in the path bracket, else drawn now with the pen (and the brush when `fill`).
    fn figure(&mut self, s: &State, d: String, fill: bool) {
        if d.is_empty() {
            return;
        }
        if let Some(p) = self.path.as_mut() {
            p.push_str(&d);
            return;
        }
        self.draw(s, &d, fill, true);
    }

    fn draw(&mut self, s: &State, d: &str, fill: bool, stroke: bool) {
        let fill = if fill { s.brush.color } else { None };
        let pen = if stroke { s.pen.color } else { None };
        if fill.is_none() && pen.is_none() {
            return;
        }
        // a pen of no width is one device unit wide (a cosmetic pen)
        let width = s.device_len(s.pen.width, self.metric).max(1.0);
        self.shapes.push_str(&format!(
            "<path d=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" fill-rule=\"{}\"/>\n",
            d.trim(),
            fill.map_or("none".into(), hex),
            pen.map_or("none".into(), hex),
            num(width),
            if s.evenodd { "evenodd" } else { "nonzero" }
        ));
    }
}

/// Polygons and lines of points (kinds as EMF's 16-bit records: 86 polygon, 87 polyline, 85 Bézier, 88 Bézier to, 89 polyline to).
fn draw_poly(out: &mut Out, s: &mut State, kind: u32, pts: &[(f64, f64)]) {
    match kind {
        86 => {
            let d = out.poly(s, pts, true);
            out.figure(s, d, true);
        }
        87 => {
            let d = out.poly(s, pts, false);
            out.figure(s, d, false);
        }
        85 => {
            let d = out.bezier(s, None, pts);
            out.figure(s, d, false);
        }
        88 | 89 => {
            // from the current position
            if let Some(path) = out.path.as_mut() {
                for c in if kind == 88 { pts.chunks(3).collect::<Vec<_>>() } else { pts.chunks(1).collect() } {
                    path.push(if kind == 88 { 'C' } else { 'L' });
                    for (x, y) in c {
                        let (x, y) = s.device(*x, *y, out.metric);
                        path.push_str(&format!("{} {} ", num(x), num(y)));
                    }
                }
            } else {
                let d = if kind == 88 { out.bezier(s, Some(s.at), pts) } else {
                    let mut all = vec![s.at];
                    all.extend_from_slice(pts);
                    out.poly(s, &all, false)
                };
                out.draw(s, &d, false, true);
            }
            if let Some(last) = pts.last() {
                s.at = *last;
            }
        }
        _ => {}
    }
}

fn colorref(v: u32) -> [u8; 3] {
    [(v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8]
}

/// Stock objects (index with the high bit set).
fn stock(i: u32) -> Option<Obj> {
    Some(match i & 0x7FFF_FFFF {
        0 => Obj::Brush(Brush { color: Some([255, 255, 255]) }),
        1 => Obj::Brush(Brush { color: Some([192, 192, 192]) }),
        2 => Obj::Brush(Brush { color: Some([128, 128, 128]) }),
        3 => Obj::Brush(Brush { color: Some([64, 64, 64]) }),
        4 => Obj::Brush(Brush { color: Some([0, 0, 0]) }),
        5 => Obj::Brush(Brush { color: None }),
        6 => Obj::Pen(Pen { color: Some([255, 255, 255]), width: 0.0 }),
        7 => Obj::Pen(Pen { color: Some([0, 0, 0]), width: 0.0 }),
        8 => Obj::Pen(Pen { color: None, width: 0.0 }),
        // stock fonts: the system font
        10..=17 => Obj::Font(usize::MAX),
        _ => return None,
    })
}

fn select(s: &mut State, o: Obj) {
    match o {
        Obj::Pen(p) => s.pen = p,
        Obj::Brush(b) => s.brush = b,
        Obj::Font(f) => s.font = Some(f),
        Obj::Other => {}
    }
}

fn pen_of(style: u32, width: f64, color: u32) -> Pen {
    // PS_NULL (5): no line
    Pen { color: (style & 0xF != 5).then(|| colorref(color)), width }
}

fn brush_of(style: u32, color: u32) -> (Brush, bool) {
    match style {
        0 => (Brush { color: Some(colorref(color)) }, false),
        1 => (Brush { color: None }, false),
        // hatched and pattern brushes: their colour, flat (named in the skipped list)
        _ => (Brush { color: Some(colorref(color)) }, true),
    }
}

fn svg_text(shapes: &str, view: [f64; 4], size: [f64; 2]) -> String {
    format!(
        // a user unit is a point when placed (svgplace: `width="100px"` is 100 pt)
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\">\n{shapes}</svg>\n",
        num(size[0]), num(size[1]), num(view[0]), num(view[1]), num(view[2]), num(view[3])
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(kind: u32, params: &[u8]) -> Vec<u8> {
        let mut r = kind.to_le_bytes().to_vec();
        r.extend(((8 + params.len()) as u32).to_le_bytes());
        r.extend_from_slice(params);
        r
    }
    fn ints(v: &[i32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// An EMF of 100 × 50 mm on a 1000 × 500 device (10 px per mm) with a red filled rectangle and a blue polyline.
    pub(crate) fn sample_emf() -> Vec<u8> {
        let mut header = ints(&[0, 0, 999, 499, 0, 0, 10000, 5000]);
        header.extend(b" EMF");
        header.extend(ints(&[0x10000, 0, 0, 0, 0, 0, 0, 1000, 500, 100, 50]));
        let mut b = rec(1, &header);
        b.extend(rec(39, &ints(&[1, 0, 0x0000FF, 0])));
        b.extend(rec(37, &ints(&[1])));
        b.extend(rec(37, &ints(&[0x8000_0008u32 as i32])));
        b.extend(rec(43, &ints(&[100, 100, 300, 200])));
        b.extend(rec(38, &ints(&[2, 0, 5, 0, 0xFF0000])));
        b.extend(rec(37, &ints(&[2])));
        let mut poly = ints(&[0, 0, 0, 0, 3]);
        for (x, y) in [(10i16, 10i16), (500, 400), (900, 10)] {
            poly.extend(x.to_le_bytes());
            poly.extend(y.to_le_bytes());
        }
        b.extend(rec(87, &poly));
        b.extend(rec(84, &[0; 40]));
        b.extend(rec(14, &ints(&[0, 0, 0])));
        b
    }

    #[test]
    fn an_emf_becomes_paths_the_svg_reader_draws() {
        let b = sample_emf();
        assert!(is_metafile(&b));
        let c = convert(&b).unwrap();
        assert!((c.size[0] - 283.4646).abs() < 0.01 && (c.size[1] - 141.7323).abs() < 0.01, "{:?}", c.size);
        assert!(c.svg.contains("fill=\"#ff0000\" stroke=\"none\""), "{}", c.svg);
        assert!(c.svg.contains("M10 10 L500 400 L900 10") && c.svg.contains("stroke=\"#0000ff\" stroke-width=\"5\""), "{}", c.svg);
        // an empty text record draws nothing and is not left out
        assert!(c.skipped.is_empty(), "{:?}", c.skipped);
        let scene = crate::svg::parse(&c.svg).unwrap();
        assert_eq!(scene.shapes.len(), 2);
        assert!((scene.width - c.size[0]).abs() < 0.01);
    }

    #[test]
    fn emf_text_and_bitmaps_are_drawn() {
        let mut header = ints(&[0, 0, 999, 499, 0, 0, 10000, 5000]);
        header.extend(b" EMF");
        header.extend(ints(&[0x10000, 0, 0, 0, 0, 0, 0, 1000, 500, 100, 50]));
        let mut b = rec(1, &header);
        // EXTCREATEFONTINDIRECTW: object 1, height -40, weight 700, face "Arial"
        let mut font = ints(&[1, -40, 0, 0, 0, 700]);
        font.extend([0u8; 8]);
        let face: Vec<u8> = "Arial".encode_utf16().chain(std::iter::repeat(0)).take(32).flat_map(|c| c.to_le_bytes()).collect();
        font.extend(face);
        b.extend(rec(82, &font));
        b.extend(rec(37, &ints(&[1])));
        b.extend(rec(24, &ints(&[0x0000C0])));
        b.extend(rec(22, &ints(&[24])));
        // EXTTEXTOUTW "Hi 한글" at (100, 300) with advances of 30
        let text: Vec<u16> = "Hi 한글".encode_utf16().collect();
        let n = text.len() as i32;
        // after the record header (8) and the 17 fixed fields
        let off_string = 8 + 68;
        #[allow(clippy::manual_div_ceil, reason = "align the fixture offset in usize before encoding it as i32")]
        let off_dx = off_string + ((text.len() * 2 + 3) / 4 * 4) as i32;
        let mut t = ints(&[0, 0, 0, 0, 1, 0, 0, 100, 300, n, off_string, 0, 0, 0, 0, 0, off_dx]);
        t.extend(text.iter().flat_map(|c| c.to_le_bytes()));
        while t.len() + 8 < off_dx as usize { t.push(0); }
        t.extend(ints(&vec![30; text.len()]));
        b.extend(rec(84, &t));
        // STRETCHDIBITS: a 2 × 2 24-bit picture onto (600, 100) 200 × 200
        let mut bmi = ints(&[40, 2, 2]);
        bmi.extend(1u16.to_le_bytes());
        bmi.extend(24u16.to_le_bytes());
        bmi.extend(ints(&[0, 0, 0, 0, 0, 0]));
        let bits = [255u8, 0, 0, 0, 255, 0, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0];
        let fixed = 8 + 72;
        let mut d = ints(&[0, 0, 0, 0, 600, 100, 0, 0, 2, 2, fixed, bmi.len() as i32, fixed + bmi.len() as i32, bits.len() as i32, 0, 0x00CC_0020, 200, 200]);
        d.extend(&bmi);
        d.extend(bits);
        b.extend(rec(81, &d));
        b.extend(rec(14, &ints(&[0, 0, 0])));
        let c = convert(&b).unwrap();
        assert!(c.skipped.is_empty(), "{:?}", c.skipped);
        assert!(c.svg.contains(">Hi 한글</text>") && c.svg.contains("font-family=\"'Arial', sans-serif\"") && c.svg.contains("font-weight=\"700\""), "{}", c.svg);
        assert!(c.svg.contains("x=\"100 130 160 190 220\"") && c.svg.contains("fill=\"#c00000\""), "{}", c.svg);
        assert!(c.svg.contains("<image ") && c.svg.contains("data:image/png;base64,"), "{}", c.svg);
        let scene = crate::svg::parse_any(&c.svg).unwrap();
        assert!(scene.rich.is_some());
        let parts = scene.parts(1.0);
        assert!(parts.iter().any(|p| matches!(p, crate::svg::Part::Shape(_))), "glyph outlines");
        assert!(parts.iter().any(|p| matches!(p, crate::svg::Part::Raster { .. })), "the bitmap");
    }

    #[test]
    fn a_placeable_wmf_becomes_paths() {
        // placeable header: box 0 0 1440 720 at 1440 per inch (1 × 0.5 inch)
        let mut b = vec![0xD7, 0xCD, 0xC6, 0x9A, 0, 0];
        for v in [0i16, 0, 1440, 720] {
            b.extend(v.to_le_bytes());
        }
        b.extend(1440u16.to_le_bytes());
        b.extend([0; 6]);
        b.extend([1, 0, 9, 0, 0, 3, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
        let rec16 = |func: u16, params: &[i16]| {
            let mut r = ((3 + params.len()) as u32).to_le_bytes().to_vec();
            r.extend(func.to_le_bytes());
            for p in params {
                r.extend(p.to_le_bytes());
            }
            r
        };
        b.extend(rec16(0x02FC, &[0, 0x00FF, 0x0000, 0]));
        b.extend(rec16(0x012D, &[0]));
        b.extend(rec16(0x0324, &[3, 0, 0, 1440, 0, 720, 720]));
        b.extend(rec16(0x0418, &[720, 1440, 360, 720]));
        b.extend(rec16(0x0000, &[]));
        let c = convert(&b).unwrap();
        assert_eq!(c.size, [72.0, 36.0]);
        assert!(c.svg.contains("viewBox=\"0 0 1440 720\"") && c.svg.contains("fill=\"#ff0000\""), "{}", c.svg);
        assert_eq!(crate::svg::parse(&c.svg).unwrap().shapes.len(), 2);
    }
}
