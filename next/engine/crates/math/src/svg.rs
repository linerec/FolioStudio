//! A laid-out equation as SVG in the form InDesign stores with a MathML object: one `svg` whose
//! `viewBox` counts thousandths of an em, y turned by `scale(1,-1)`, every shape a filled
//! `path` or `rect` in `currentColor`. No font is referenced: the outlines are in the file.

use crate::font::Fonts;
use crate::layout::{Boxx, Item, Seg};
use ttf_parser::{GlyphId, OutlineBuilder};

/// Units per em of the stored drawing.
const UNITS: f64 = 1000.0;
/// x-height (em) the `ex` sizes of the SVG header are counted in, as InDesign's own drawing does.
const EX: f64 = 0.442;

/// How the header gives the drawing's size.
#[derive(Clone, Copy)]
pub enum Dim {
    /// Relative sizes (`ex`), as stored in the document.
    Ex,
    /// Points, for showing the drawing at once; `em_pt` points to an em.
    Pt(f64),
}

pub fn num(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r == 0.0 {
        return "0".to_owned();
    }
    let s = format!("{r:.1}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

struct Path<'a> {
    out: &'a mut String,
    x: f64,
    y: f64,
    sx: f64,
    sy: f64,
}

impl Path<'_> {
    fn pt(&self, x: f32, y: f32) -> String {
        format!("{} {}", num((self.x + f64::from(x) * self.sx) * UNITS), num((self.y + f64::from(y) * self.sy) * UNITS))
    }
}

impl OutlineBuilder for Path<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.out.push_str(&format!("M{p}"));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.out.push_str(&format!("L{p}"));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (a, b) = (self.pt(x1, y1), self.pt(x, y));
        self.out.push_str(&format!("Q{a} {b}"));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b, c) = (self.pt(x1, y1), self.pt(x2, y2), self.pt(x, y));
        self.out.push_str(&format!("C{a} {b} {c}"));
    }
    fn close(&mut self) {
        self.out.push('Z');
    }
}

/// The ink and box extent in em: (left, bottom, right, top).
fn extent(b: &Boxx, fonts: &Fonts) -> [f64; 4] {
    // a negative space (mspace width="-3em") must not make the drawing negative
    let (mut x0, mut y0, mut x1, mut y1) = (0.0f64, (-b.d).min(0.0), b.w.max(0.0), b.h.max(0.0));
    let mut grow = |x: f64, y: f64| {
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    };
    for it in &b.items {
        match it {
            Item::Glyph { font, gid, x, y, sx, sy } => {
                if let Some(bb) = fonts.list[*font].bbox(GlyphId(*gid)) {
                    grow(x + bb[0] * sx, y + bb[1] * sy);
                    grow(x + bb[2] * sx, y + bb[3] * sy);
                }
            }
            Item::Rect { x, y, w, h } => {
                grow(*x, *y);
                grow(x + w, y + h);
            }
            Item::Path(segs) => {
                for s in segs {
                    match *s {
                        Seg::M(x, y) | Seg::L(x, y) => grow(x, y),
                        Seg::C(a, b2, c, d, e, f) => {
                            grow(a, b2);
                            grow(c, d);
                            grow(e, f);
                        }
                        Seg::Z => {}
                    }
                }
            }
        }
    }
    [x0, y0, x1, y1]
}

/// The drawing, and its extent in em (left, bottom, right, top), baseline at 0.
/// `color`: the equation's colour written on the drawing the way InDesign writes it (`fill` and `stroke` on the
/// `math` group); `None` leaves `currentColor`, the colour of the object (black unless set).
pub fn render(b: &Boxx, fonts: &Fonts, dim: Dim, color: Option<[u8; 3]>) -> (String, [f64; 4]) {
    let ext = extent(b, fonts);
    let [x0, y0, x1, y1] = ext;
    let mut body = String::new();
    for it in &b.items {
        match it {
            Item::Glyph { font, gid, x, y, sx, sy } => {
                let mut d = String::new();
                let ok = fonts.list[*font].outline(GlyphId(*gid), &mut Path { out: &mut d, x: *x, y: *y, sx: *sx, sy: *sy });
                if ok && !d.is_empty() {
                    body.push_str(&format!("<path d=\"{d}\"></path>"));
                }
            }
            Item::Rect { x, y, w, h } => {
                body.push_str(&format!(
                    "<rect width=\"{}\" height=\"{}\" x=\"{}\" y=\"{}\"></rect>",
                    num(w * UNITS),
                    num(h * UNITS),
                    num(x * UNITS),
                    num(y * UNITS)
                ));
            }
            Item::Path(segs) => {
                let mut d = String::new();
                for s in segs {
                    match *s {
                        Seg::M(x, y) => d.push_str(&format!("M{} {}", num(x * UNITS), num(y * UNITS))),
                        Seg::L(x, y) => d.push_str(&format!("L{} {}", num(x * UNITS), num(y * UNITS))),
                        Seg::C(a, b2, c, e, f, g) => d.push_str(&format!(
                            "C{} {} {} {} {} {}",
                            num(a * UNITS),
                            num(b2 * UNITS),
                            num(c * UNITS),
                            num(e * UNITS),
                            num(f * UNITS),
                            num(g * UNITS)
                        )),
                        Seg::Z => d.push('Z'),
                    }
                }
                body.push_str(&format!("<path d=\"{d}\"></path>"));
            }
        }
    }
    let (w_em, h_em) = (x1 - x0, y1 - y0);
    let (width, height, align) = match dim {
        Dim::Ex => (format!("{}ex", ex(w_em)), format!("{}ex", ex(h_em)), format!(" style=\"vertical-align: {}ex;\"", ex(y0))),
        Dim::Pt(em_pt) => (format!("{}pt", pt(w_em * em_pt)), format!("{}pt", pt(h_em * em_pt)), String::new()),
    };
    let paint = color.map_or(String::new(), |[r, g, b]| format!(" fill=\"#{r:02x}{g:02x}{b:02x}\" stroke=\"#{r:02x}{g:02x}{b:02x}\""));
    let svg = format!(
        "<svg{align} xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" role=\"img\" focusable=\"false\" viewBox=\"{} {} {} {}\"><g stroke=\"currentColor\" fill=\"currentColor\" stroke-width=\"0\" transform=\"scale(1,-1)\"><g data-mml-node=\"math\"{paint}>{body}</g></g></svg>",
        num(x0 * UNITS),
        num(-y1 * UNITS),
        num(w_em * UNITS),
        num(h_em * UNITS),
    );
    (svg, ext)
}

fn ex(em: f64) -> String {
    let v = (em / EX * 1000.0).round() / 1000.0;
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".to_owned() } else { s.to_owned() }
}

fn pt(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}
