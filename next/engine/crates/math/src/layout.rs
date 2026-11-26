//! Equation layout: TeX's rules (TeXbook appendix G) with the constants of the font's OpenType
//! MATH table, over the elements of MathML presentation markup. All sizes are in em of the
//! equation's root font size; y points up from the baseline.

use crate::font::{Consts, Fonts};
use crate::xml::{Child, Node};
use ttf_parser::GlyphId;

#[derive(Clone, Debug)]
pub enum Seg {
    M(f64, f64),
    L(f64, f64),
    C(f64, f64, f64, f64, f64, f64),
    Z,
}

#[derive(Clone, Debug)]
pub enum Item {
    /// A glyph outline of font `font`, its origin at (x, y), scaled by (sx, sy) em per em.
    Glyph { font: usize, gid: u16, x: f64, y: f64, sx: f64, sy: f64 },
    /// A filled rectangle (a fraction bar, a root's rule); y is its bottom edge.
    Rect { x: f64, y: f64, w: f64, h: f64 },
    /// A filled shape (diagonal strikes, circles).
    Path(Vec<Seg>),
}

impl Item {
    fn shifted(&self, dx: f64, dy: f64) -> Item {
        match self {
            Item::Glyph { font, gid, x, y, sx, sy } => Item::Glyph { font: *font, gid: *gid, x: x + dx, y: y + dy, sx: *sx, sy: *sy },
            Item::Rect { x, y, w, h } => Item::Rect { x: x + dx, y: y + dy, w: *w, h: *h },
            Item::Path(segs) => Item::Path(
                segs.iter()
                    .map(|s| match *s {
                        Seg::M(x, y) => Seg::M(x + dx, y + dy),
                        Seg::L(x, y) => Seg::L(x + dx, y + dy),
                        Seg::C(a, b, c, d, e, f) => Seg::C(a + dx, b + dy, c + dx, d + dy, e + dx, f + dy),
                        Seg::Z => Seg::Z,
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Boxx {
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub items: Vec<Item>,
    /// Italic correction of the last glyph.
    pub ital: f64,
    /// Where an accent attaches over the top.
    pub accent: Option<f64>,
}

impl Boxx {
    fn blank(w: f64, h: f64, d: f64) -> Boxx {
        Boxx { w, h, d, ..Default::default() }
    }
    fn put(&mut self, other: &Boxx, dx: f64, dy: f64) {
        self.items.extend(other.items.iter().map(|i| i.shifted(dx, dy)));
    }
    fn rule(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if w > 0.0 && h > 0.0 {
            self.items.push(Item::Rect { x, y, w, h });
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
    // Retain the eighth class used by the spacing table and class_index.
    #[allow(dead_code)]
    Inner,
}

/// TeX's table of space between two atoms (TeXbook p.170), in thin (3/18 em) steps: 1 thin,
/// 2 medium, 3 thick; negative values apply only outside script styles.
const SPACING: [[i8; 8]; 8] = [
    //  Ord Op  Bin Rel Open Close Punct Inner
    [0, 1, -2, -3, 0, 0, 0, -1],
    [1, 1, 0, -3, 0, 0, 0, -1],
    [-2, -2, 0, 0, -2, 0, 0, -2],
    [-3, -3, 0, 0, -3, 0, 0, -3],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, -2, -3, 0, 0, 0, -1],
    [-1, -1, 0, -1, -1, -1, -1, -1],
    [-1, 1, -2, -3, -1, 0, -1, -1],
];

fn class_index(c: Class) -> usize {
    match c {
        Class::Ord => 0,
        Class::Op => 1,
        Class::Bin => 2,
        Class::Rel => 3,
        Class::Open => 4,
        Class::Close => 5,
        Class::Punct => 6,
        Class::Inner => 7,
    }
}

#[derive(Clone, Copy, Debug)]
struct Style {
    /// 0 display, 1 text, 2 script, 3 script-script.
    level: u8,
    cramped: bool,
}

impl Style {
    fn display(self) -> bool {
        self.level == 0
    }
    fn script(self) -> Style {
        Style { level: if self.level < 2 { 2 } else { 3 }, cramped: self.cramped }
    }
    fn sub(self) -> Style {
        Style { cramped: true, ..self.script() }
    }
    fn sup(self) -> Style {
        self.script()
    }
    fn numerator(self) -> Style {
        Style { level: (self.level + 1).clamp(1, 3), cramped: self.cramped }
    }
    fn denominator(self) -> Style {
        Style { cramped: true, ..self.numerator() }
    }
    fn cramp(self) -> Style {
        Style { cramped: true, ..self }
    }
}

fn is_bin(c: char) -> bool {
    "+\u{2212}-\u{B1}\u{2213}\u{D7}\u{F7}\u{22C5}\u{2217}*\u{2218}\u{2219}\u{2227}\u{2228}\u{2229}\u{222A}\u{2295}\u{2296}\u{2297}\u{2298}\u{2299}\u{228E}\u{2293}\u{2294}\u{2216}\u{22C6}\u{22C4}\u{2020}\u{2021}\u{2240}\u{25CB}\u{25C7}\u{25B3}\u{25BD}".contains(c)
}
fn is_rel(c: char) -> bool {
    "=\u{2260}<>\u{2264}\u{2265}\u{226A}\u{226B}\u{2248}\u{2245}\u{2261}\u{223C}\u{2243}\u{2244}\u{221D}\u{2208}\u{2209}\u{220B}\u{2282}\u{2283}\u{2286}\u{2287}\u{2284}\u{2285}\u{228F}\u{2290}\u{2291}\u{2292}\u{2192}\u{2190}\u{2194}\u{21D2}\u{21D0}\u{21D4}\u{21A6}\u{2191}\u{2193}\u{21D1}\u{21D3}\u{2197}\u{2198}\u{2196}\u{2199}\u{2225}\u{22A5}\u{22A2}\u{22A3}\u{22A8}\u{22A9}\u{227A}\u{227B}\u{2AAF}\u{2AB0}\u{2272}\u{2273}\u{22B2}\u{22B3}\u{22C8}\u{2250}\u{27F6}\u{27F5}\u{27F7}\u{27F9}\u{27F8}\u{27FA}\u{27FC}\u{21A0}\u{21A3}\u{21C0}\u{21C1}\u{21BC}\u{21BD}\u{2223}\u{2224}\u{2226}\u{22A7}\u{2262}\u{2247}\u{226E}\u{226F}\u{2270}\u{2271}:".contains(c)
}
fn is_open(c: char) -> bool {
    "([{\u{27E8}\u{2308}\u{230A}\u{27E6}\u{27EA}\u{2983}\u{2329}".contains(c)
}
fn is_close(c: char) -> bool {
    ")]}\u{27E9}\u{2309}\u{230B}\u{27E7}\u{27EB}\u{2984}\u{232A}".contains(c)
}
fn is_largeop(c: char) -> bool {
    "\u{2211}\u{220F}\u{2210}\u{222B}\u{222C}\u{222D}\u{222E}\u{222F}\u{2230}\u{22C0}\u{22C1}\u{22C2}\u{22C3}\u{2A00}\u{2A01}\u{2A02}\u{2A04}\u{2A06}".contains(c)
}
fn is_integral(c: char) -> bool {
    "\u{222B}\u{222C}\u{222D}\u{222E}\u{222F}\u{2230}".contains(c)
}
fn is_fence(c: char) -> bool {
    is_open(c) || is_close(c) || "|\u{2016}\u{2225}".contains(c)
}
fn is_horizontal_accent(c: char) -> bool {
    "\u{AF}\u{203E}_\u{2190}\u{2192}\u{2194}\u{23DE}\u{23DF}\u{23DC}\u{23DD}\u{2DC}^\u{2C6}\u{2C7}~\u{20D7}\u{23B4}\u{23B5}\u{21D2}\u{21D0}\u{21D4}\u{2015}\u{2500}".contains(c)
}

fn op_char(text: &str) -> char {
    let mut it = text.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => c,
        _ => '\0',
    }
}

fn map_op(c: char) -> char {
    match c {
        '-' => '\u{2212}',
        '*' => '\u{2217}',
        '\'' => '\u{2032}',
        _ => c,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    Normal,
    Italic,
    Bold,
    BoldItalic,
    DoubleStruck,
    Script,
    Fraktur,
    Sans,
    Mono,
}

fn parse_variant(s: &str) -> Option<Variant> {
    Some(match s {
        "normal" => Variant::Normal,
        "italic" => Variant::Italic,
        "bold" => Variant::Bold,
        "bold-italic" => Variant::BoldItalic,
        "double-struck" => Variant::DoubleStruck,
        "script" | "bold-script" => Variant::Script,
        "fraktur" | "bold-fraktur" => Variant::Fraktur,
        "sans-serif" | "bold-sans-serif" | "sans-serif-italic" | "sans-serif-bold-italic" => Variant::Sans,
        "monospace" => Variant::Mono,
        _ => return None,
    })
}

/// A letter or digit in a mathematical alphabet (Unicode "Mathematical Alphanumeric Symbols").
fn styled(c: char, v: Variant) -> char {
    let cp = c as u32;
    let (upper, lower, digit): (u32, u32, Option<u32>) = match v {
        Variant::Normal => return c,
        Variant::Italic => (0x1D434, 0x1D44E, None),
        Variant::Bold => (0x1D400, 0x1D41A, Some(0x1D7CE)),
        Variant::BoldItalic => (0x1D468, 0x1D482, None),
        Variant::DoubleStruck => (0x1D538, 0x1D552, Some(0x1D7D8)),
        Variant::Script => (0x1D49C, 0x1D4B6, None),
        Variant::Fraktur => (0x1D504, 0x1D51E, None),
        Variant::Sans => (0x1D5A0, 0x1D5BA, Some(0x1D7E2)),
        Variant::Mono => (0x1D670, 0x1D68A, Some(0x1D7F6)),
    };
    let mapped = match cp {
        0x41..=0x5A => upper + (cp - 0x41),
        0x61..=0x7A => lower + (cp - 0x61),
        0x30..=0x39 => match digit {
            Some(base) => base + (cp - 0x30),
            None => return c,
        },
        // lower-case Greek in math italic
        0x3B1..=0x3C9 if matches!(v, Variant::Italic) => 0x1D6FC + (cp - 0x3B1),
        _ => return c,
    };
    // holes of the alphabets: the letters Unicode took from the Letterlike Symbols block
    let hole = match (v, c) {
        (Variant::Italic, 'h') => Some('\u{210E}'),
        (Variant::Script, 'B') => Some('\u{212C}'),
        (Variant::Script, 'E') => Some('\u{2130}'),
        (Variant::Script, 'F') => Some('\u{2131}'),
        (Variant::Script, 'H') => Some('\u{210B}'),
        (Variant::Script, 'I') => Some('\u{2110}'),
        (Variant::Script, 'L') => Some('\u{2112}'),
        (Variant::Script, 'M') => Some('\u{2133}'),
        (Variant::Script, 'R') => Some('\u{211B}'),
        (Variant::Script, 'e') => Some('\u{212F}'),
        (Variant::Script, 'g') => Some('\u{210A}'),
        (Variant::Script, 'o') => Some('\u{2134}'),
        (Variant::Fraktur, 'C') => Some('\u{212D}'),
        (Variant::Fraktur, 'H') => Some('\u{210C}'),
        (Variant::Fraktur, 'I') => Some('\u{2111}'),
        (Variant::Fraktur, 'R') => Some('\u{211C}'),
        (Variant::Fraktur, 'Z') => Some('\u{2128}'),
        (Variant::DoubleStruck, 'C') => Some('\u{2102}'),
        (Variant::DoubleStruck, 'H') => Some('\u{210D}'),
        (Variant::DoubleStruck, 'N') => Some('\u{2115}'),
        (Variant::DoubleStruck, 'P') => Some('\u{2119}'),
        (Variant::DoubleStruck, 'Q') => Some('\u{211A}'),
        (Variant::DoubleStruck, 'R') => Some('\u{211D}'),
        (Variant::DoubleStruck, 'Z') => Some('\u{2124}'),
        _ => None,
    };
    hole.or_else(|| char::from_u32(mapped)).unwrap_or(c)
}

/// An engine that could not be laid out: the element it stopped at.
pub struct Layout<'a> {
    pub fonts: &'a Fonts,
    k: Consts,
    pub notes: Vec<String>,
}

fn attr_bool(n: &Node, key: &str) -> Option<bool> {
    match n.attr(key)? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn named_space(s: &str) -> Option<f64> {
    Some(match s {
        "veryverythinmathspace" => 1.0 / 18.0,
        "verythinmathspace" => 2.0 / 18.0,
        "thinmathspace" => 3.0 / 18.0,
        "mediummathspace" => 4.0 / 18.0,
        "thickmathspace" => 5.0 / 18.0,
        "verythickmathspace" => 6.0 / 18.0,
        "veryverythickmathspace" => 7.0 / 18.0,
        "negativeveryverythinmathspace" => -1.0 / 18.0,
        "negativeverythinmathspace" => -2.0 / 18.0,
        "negativethinmathspace" => -3.0 / 18.0,
        "negativemediummathspace" => -4.0 / 18.0,
        "negativethickmathspace" => -5.0 / 18.0,
        "negativeverythickmathspace" => -6.0 / 18.0,
        "negativeveryverythickmathspace" => -7.0 / 18.0,
        _ => return None,
    })
}

/// A MathML length in em of the current font (1em = 10pt for the absolute units: the equation's
/// own size is applied later, so lengths in points scale with it).
fn length(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(v) = named_space(s) {
        return Some(v);
    }
    let split = s.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let v: f64 = num.trim().parse().ok()?;
    Some(match unit.trim() {
        "" | "em" => v,
        "ex" => v * 0.45,
        "pt" => v / 10.0,
        "px" => v / 13.333,
        "pc" => v * 1.2,
        "in" => v * 7.2,
        "cm" => v * 2.8346,
        "mm" => v * 0.28346,
        "mu" => v / 18.0,
        "%" => v / 100.0,
        _ => return None,
    })
}

impl<'a> Layout<'a> {
    pub fn new(fonts: &'a Fonts) -> Layout<'a> {
        // Spacing constants are TeX's (what InDesign's MathJax uses); what belongs to the font itself
        // (the math axis its signs are drawn on, accent height, sizes of the variants) is the font's.
        // FOLIO_MATH_FONTCONSTS=1 takes every constant from the font's MATH table.
        let font_k = fonts.primary().consts();
        let k = if fonts.math && std::env::var_os("FOLIO_MATH_FONTCONSTS").is_none() {
            Consts {
                axis: font_k.axis,
                accent_base: font_k.accent_base,
                display_op_min: font_k.display_op_min,
                delimited_min: font_k.delimited_min,
                min_connector_overlap: font_k.min_connector_overlap,
                ..Consts::tex_like()
            }
        } else {
            font_k
        };
        Layout { fonts, k, notes: Vec::new() }
    }

    fn note(&mut self, text: String) {
        if !self.notes.contains(&text) {
            self.notes.push(text);
        }
    }

    fn scale(&self, style: Style) -> f64 {
        match style.level {
            0 | 1 => 1.0,
            2 => self.k.script_scale,
            _ => self.k.script_script_scale,
        }
    }

    /// Lays out a whole `math` element.
    pub fn math(&mut self, root: &Node, display: bool) -> Boxx {
        let display = attr_bool(root, "displaystyle").unwrap_or(display) || root.attr("display") == Some("block");
        let style = Style { level: if display { 0 } else { 1 }, cramped: false };
        self.element(root, style, None)
    }

    fn children_of<'n>(&self, node: &'n Node) -> Vec<&'n Node> {
        node.elems().collect()
    }

    // ---- glyphs ----

    fn glyph_box(&mut self, ch: char, sc: f64) -> Boxx {
        let Some((font, gid)) = self.fonts.glyph(ch) else {
            if !ch.is_whitespace() && ch != '\u{2061}' && ch != '\u{2062}' && ch != '\u{2063}' && ch != '\u{2064}' {
                self.note(format!("no glyph for U+{:04X}", ch as u32));
            }
            let w = match ch {
                '\u{2009}' => 3.0 / 18.0,
                '\u{2003}' => 1.0,
                '\u{2002}' => 0.5,
                '\u{205F}' => 4.0 / 18.0,
                '\u{2061}' | '\u{2062}' | '\u{2063}' | '\u{2064}' => 0.0,
                c if c.is_whitespace() => 0.25,
                _ => 0.5,
            };
            return Boxx::blank(w * sc, 0.0, 0.0);
        };
        self.glyph_gid(font, gid, sc, sc)
    }

    fn glyph_gid(&self, font: usize, gid: GlyphId, sx: f64, sy: f64) -> Boxx {
        let f = self.fonts.list[font];
        let (h, d) = f.bbox(gid).map_or((0.0, 0.0), |b| (b[3].max(0.0) * sy, (-b[1]).max(0.0) * sy));
        let w = f.advance(gid) * sx;
        let ital = if font == 0 { f.italic_correction(gid) * sx } else { 0.0 };
        let accent = if font == 0 { f.top_accent(gid).map(|a| a * sx) } else { None };
        Boxx { w, h, d, items: vec![Item::Glyph { font, gid: gid.0, x: 0.0, y: 0.0, sx, sy }], ital, accent }
    }

    fn text_box(&mut self, text: &str, sc: f64, map: impl Fn(char) -> char) -> Boxx {
        let mut out = Boxx::default();
        for ch in text.chars() {
            let b = self.glyph_box(map(ch), sc);
            out.put(&b, out.w, 0.0);
            out.w += b.w;
            out.h = out.h.max(b.h);
            out.d = out.d.max(b.d);
            out.ital = b.ital;
            out.accent = b.accent.map(|a| out.w - b.w + a);
        }
        out
    }

    // ---- stretching ----

    /// A delimiter or operator glyph for `ch` stretched to `target` em along the axis
    /// (`vertical`), its size at least the glyph's own. Centred on the math axis when vertical.
    fn stretched(&mut self, ch: char, target: f64, vertical: bool, sc: f64) -> Boxx {
        let natural = self.glyph_box(ch, sc);
        let Some((font, base)) = self.fonts.glyph(ch).filter(|(f, _)| *f == 0) else { return natural };
        let f = self.fonts.primary();
        let size_of = |b: &Boxx| if vertical { b.h + b.d } else { b.w };
        if size_of(&natural) >= target {
            return if vertical { self.center_on_axis(natural, sc) } else { natural };
        }
        let mut variants = f.variants(base, vertical);
        variants.sort_by(|a, b| a.1.total_cmp(&b.1));
        let want = target / sc;
        if let Some((gid, _)) = variants.iter().find(|(_, adv)| *adv >= want - 1e-9) {
            let b = self.glyph_gid(font, *gid, sc, sc);
            return if vertical { self.center_on_axis(b, sc) } else { b };
        }
        let parts = f.assembly(base, vertical);
        if !parts.is_empty()
            && let Some(b) = self.assemble(&parts, want, vertical, sc) {
            return b;
        }
        // no larger variant: the largest one, stretched to the target
        let (gid, _) = variants.last().copied().unwrap_or((base, 0.0));
        let b = self.glyph_gid(font, gid, sc, sc);
        let have = size_of(&b);
        if have <= 0.0 {
            return b;
        }
        let k = (target / have).min(6.0);
        let stretched = if vertical { self.glyph_gid(font, gid, sc, sc * k) } else { self.glyph_gid(font, gid, sc * k, sc) };
        if vertical { self.center_on_axis(stretched, sc) } else { stretched }
    }

    /// The lowest point of the shapes in a box (em; negative below the baseline).
    fn ink_bottom(&self, b: &Boxx) -> f64 {
        let mut low = f64::INFINITY;
        for it in &b.items {
            match it {
                Item::Glyph { font, gid, y, sy, .. } => {
                    if let Some(bb) = self.fonts.list[*font].bbox(GlyphId(*gid)) {
                        low = low.min(y + bb[1] * sy);
                    }
                }
                Item::Rect { y, .. } => low = low.min(*y),
                Item::Path(segs) => {
                    for sg in segs {
                        if let Seg::M(_, y) | Seg::L(_, y) | Seg::C(_, _, _, _, _, y) = sg {
                            low = low.min(*y);
                        }
                    }
                }
            }
        }
        if low.is_finite() { low } else { 0.0 }
    }

    /// Moves a glyph box so that its ink is centred on the math axis.
    fn center_on_axis(&self, mut b: Boxx, sc: f64) -> Boxx {
        let axis = self.k.axis * sc;
        let dy = axis - (b.h - b.d) / 2.0;
        for it in &mut b.items {
            *it = it.shifted(0.0, dy);
        }
        b.h += dy;
        b.d -= dy;
        if b.d < 0.0 {
            b.d = 0.0;
        }
        b
    }

    /// An assembly of glyph parts of total size `want` (in em of the font, before `sc`).
    fn assemble(&self, parts: &[crate::font::Part], want: f64, vertical: bool, sc: f64) -> Option<Boxx> {
        let f = self.fonts.primary();
        let min_overlap = self.k.min_connector_overlap;
        // repeat the extenders until the assembly can reach the wanted size at minimum overlap
        let extenders = parts.iter().filter(|p| p.extender).count();
        let mut reps = 0usize;
        let sequence = |reps: usize| -> Vec<&crate::font::Part> {
            let mut s = Vec::new();
            for p in parts {
                let n = if p.extender { reps } else { 1 };
                for _ in 0..n {
                    s.push(p);
                }
            }
            s
        };
        let total_at = |seq: &[&crate::font::Part], overlap: f64| -> f64 {
            seq.iter().map(|p| p.full_advance).sum::<f64>() - overlap * (seq.len().saturating_sub(1)) as f64
        };
        let mut seq = sequence(reps);
        if extenders == 0 {
            if total_at(&seq, min_overlap) < want * 0.5 {
                return None;
            }
        } else {
            reps = 1;
            seq = sequence(reps);
            while total_at(&seq, min_overlap) < want && reps < 64 {
                reps += 1;
                seq = sequence(reps);
            }
        }
        if seq.len() < 2 {
            return None;
        }
        let joints = seq.len() - 1;
        let max_overlap = (0..joints).map(|i| seq[i].end_overlap.min(seq[i + 1].start_overlap)).fold(f64::INFINITY, f64::min).max(min_overlap);
        let sum: f64 = seq.iter().map(|p| p.full_advance).sum();
        let overlap = ((sum - want) / joints as f64).clamp(min_overlap, max_overlap);
        let mut out = Boxx::default();
        let mut pos = 0.0; // along the axis, from the start (bottom / left)
        for (i, p) in seq.iter().enumerate() {
            let b = f.bbox(p.gid)?;
            let adv = f.advance(p.gid);
            if vertical {
                let y = pos - b[1];
                out.items.push(Item::Glyph { font: 0, gid: p.gid.0, x: 0.0, y: y * sc, sx: sc, sy: sc });
                out.w = out.w.max(adv * sc);
            } else {
                let x = pos - b[0];
                out.items.push(Item::Glyph { font: 0, gid: p.gid.0, x: x * sc, y: 0.0, sx: sc, sy: sc });
                out.h = out.h.max(b[3].max(0.0) * sc);
                out.d = out.d.max((-b[1]).max(0.0) * sc);
            }
            pos += p.full_advance - if i + 1 < seq.len() { overlap } else { 0.0 };
        }
        if vertical {
            let total = pos * sc;
            out.h = total;
            out.d = 0.0;
            // centre on the axis
            let dy = self.k.axis * sc - total / 2.0;
            for it in &mut out.items {
                *it = it.shifted(0.0, dy);
            }
            out.h += dy;
            out.d = (-dy).max(0.0);
        } else {
            out.w = pos * sc;
        }
        Some(out)
    }

    // ---- elements ----

    fn element(&mut self, n: &Node, style: Style, _parent: Option<&Node>) -> Boxx {
        match n.name.as_str() {
            "math" | "mrow" | "mstyle" | "mpadded" | "merror" | "mphantom" | "maction" | "semantics" | "mlongdiv" => {
                let mut style = style;
                if n.name == "mstyle" || n.name == "math" {
                    if let Some(d) = attr_bool(n, "displaystyle") {
                        style.level = if d { 0 } else { style.level.max(1) };
                    }
                    if let Some(s) = n.attr("scriptlevel") {
                        if let Some(rest) = s.strip_prefix('+') {
                            for _ in 0..rest.parse::<u8>().unwrap_or(0) {
                                style = style.script();
                            }
                        } else if let Ok(v) = s.parse::<i32>() {
                            style.level = match v {
                                i32::MIN..=0 => style.level.min(1),
                                1 => 2,
                                _ => 3,
                            };
                        }
                    }
                }
                let kids: Vec<&Node> = match n.name.as_str() {
                    "maction" | "semantics" => self.children_of(n).into_iter().take(1).collect(),
                    _ => self.children_of(n),
                };
                let mut b = self.row(&kids, style);
                if n.name == "mphantom" {
                    b.items.clear();
                }
                if n.name == "mpadded" {
                    self.padded(n, &mut b);
                }
                b
            }
            "mi" | "mn" | "mtext" | "ms" | "mo" => self.token(n, style),
            "mspace" => {
                let w = n.attr("width").and_then(length).unwrap_or(0.0) * self.scale(style);
                let h = n.attr("height").and_then(length).unwrap_or(0.0) * self.scale(style);
                let d = n.attr("depth").and_then(length).unwrap_or(0.0) * self.scale(style);
                Boxx::blank(w, h, d)
            }
            "mfrac" => self.frac(n, style),
            "msqrt" => {
                let kids = self.children_of(n);
                let inner = self.row(&kids, style.cramp());
                self.radical(inner, None, style)
            }
            "mroot" => {
                let kids = self.children_of(n);
                if kids.len() < 2 {
                    self.note("mroot needs a base and an index".to_owned());
                    return Boxx::default();
                }
                let base = self.element(kids[0], style.cramp(), None);
                let index = self.element(kids[1], Style { level: 3, cramped: false }, None);
                self.radical(base, Some(index), style)
            }
            "msub" | "msup" | "msubsup" => self.sub_sup(n, style),
            "munder" | "mover" | "munderover" => self.under_over(n, style),
            "mmultiscripts" => self.multiscripts(n, style),
            "mtable" => self.table(n, style),
            "mfenced" => self.fenced(n, style),
            "menclose" => self.enclose(n, style),
            "mglyph" | "annotation" | "annotation-xml" | "none" | "mprescripts" => Boxx::default(),
            other => {
                self.note(format!("unsupported element <{other}> laid out as a row"));
                let kids = self.children_of(n);
                self.row(&kids, style)
            }
        }
    }

    fn padded(&mut self, n: &Node, b: &mut Boxx) {
        let size = |key: &str, from: f64| -> Option<f64> {
            let v = n.attr(key)?;
            let (sign, rest) = match v.as_bytes().first()? {
                b'+' => (1, &v[1..]),
                b'-' => (-1, &v[1..]),
                _ => (0, v),
            };
            let rel = rest.ends_with('%');
            let value = if rel { rest.trim_end_matches('%').trim().parse::<f64>().ok()? / 100.0 * from } else { length(rest)? };
            Some(match sign { 0 => value, s => from + f64::from(s) * value })
        };
        let (w0, h0, d0) = (b.w, b.h, b.d);
        let lspace = size("lspace", w0).unwrap_or(0.0);
        let voffset = size("voffset", 0.0).unwrap_or(0.0);
        if let Some(w) = size("width", w0) {
            b.w = w;
        }
        if let Some(h) = size("height", h0) {
            b.h = h;
        }
        if let Some(d) = size("depth", d0) {
            b.d = d;
        }
        if lspace != 0.0 || voffset != 0.0 {
            for it in &mut b.items {
                *it = it.shifted(lspace, voffset);
            }
            b.w += lspace;
        }
    }

    fn fenced(&mut self, n: &Node, style: Style) -> Boxx {
        let open = n.attr("open").unwrap_or("(").to_owned();
        let close = n.attr("close").unwrap_or(")").to_owned();
        let seps: Vec<char> = n.attr("separators").unwrap_or(",").chars().filter(|c| !c.is_whitespace()).collect();
        let kids = self.children_of(n);
        let mk = |name: &str, text: &str, fence: bool| Node {
            name: name.to_owned(),
            attrs: if fence { vec![("fence".to_owned(), "true".to_owned())] } else { Vec::new() },
            children: vec![Child::Text(text.to_owned())],
        };
        let mut row = Node { name: "mrow".to_owned(), attrs: Vec::new(), children: Vec::new() };
        if !open.is_empty() {
            row.children.push(Child::Elem(mk("mo", &open, true)));
        }
        for (i, k) in kids.iter().enumerate() {
            if i > 0
                && let Some(s) = seps.get(i - 1).or(seps.last()) {
                row.children.push(Child::Elem(mk("mo", &s.to_string(), false)));
            }
            row.children.push(Child::Elem((*k).clone()));
        }
        if !close.is_empty() {
            row.children.push(Child::Elem(mk("mo", &close, true)));
        }
        self.element(&row, style, None)
    }

    // ---- tokens ----

    fn token(&mut self, n: &Node, style: Style) -> Boxx {
        let sc = self.scale(style);
        let mut text = n.text();
        let variant = n.attr("mathvariant").and_then(parse_variant);
        match n.name.as_str() {
            "mi" => {
                let single = text.chars().count() == 1;
                let v = variant.unwrap_or_else(|| {
                    let c = text.chars().next().unwrap_or(' ');
                    if single && (c.is_ascii_alphabetic() || ('\u{3B1}'..='\u{3C9}').contains(&c)) { Variant::Italic } else { Variant::Normal }
                });
                self.text_box(&text, sc, |c| styled(c, v))
            }
            "mn" => {
                let v = variant.unwrap_or(Variant::Normal);
                self.text_box(&text, sc, |c| styled(c, v))
            }
            "mtext" => {
                let v = variant.unwrap_or(Variant::Normal);
                text = n.text();
                self.text_box(&text, sc, |c| styled(c, v))
            }
            "ms" => {
                let l = n.attr("lquote").unwrap_or("\"");
                let r = n.attr("rquote").unwrap_or("\"");
                let all = format!("{l}{text}{r}");
                self.text_box(&all, sc, |c| c)
            }
            _ => self.operator(n, style, None),
        }
    }

    /// An `mo`, large or stretched when asked to (`stretch` is the target size along the axis).
    fn operator(&mut self, n: &Node, style: Style, stretch: Option<f64>) -> Boxx {
        let sc = self.scale(style);
        let c = op_char(&n.text());
        if c == '\0' {
            let text = n.text();
            return self.text_box(&text, sc, map_op);
        }
        let c = map_op(c);
        let large = attr_bool(n, "largeop").unwrap_or(is_largeop(c));
        if let Some(target) = stretch {
            return self.stretched(c, target, true, sc);
        }
        if large && style.display() {
            let target = self.k.display_op_min * sc;
            let mut b = self.stretched(c, target, true, sc);
            if is_integral(c) {
                // an integral's limits sit beside it; nothing more to do here
                b.ital = b.ital.max(0.0);
            }
            return b;
        }
        if large {
            // text style: the operator's own glyph centred on the axis
            let b = self.glyph_box(c, sc);
            return self.center_on_axis(b, sc);
        }
        self.glyph_box(c, sc)
    }

    // ---- rows ----

    fn class_of(&self, n: &Node, first: bool) -> Class {
        let _ = first;
        match n.name.as_str() {
            "mo" => {
                let c = map_op(op_char(&n.text()));
                if is_largeop(c) {
                    Class::Op
                } else if is_open(c) {
                    Class::Open
                } else if is_close(c) {
                    Class::Close
                } else if c == ',' || c == ';' {
                    Class::Punct
                } else if is_bin(c) {
                    Class::Bin
                } else if is_rel(c) {
                    Class::Rel
                } else {
                    Class::Ord
                }
            }
            "mi" => {
                let t = n.text();
                if t.chars().count() > 1 && t.chars().all(|c| c.is_alphabetic()) { Class::Op } else { Class::Ord }
            }
            "msub" | "msup" | "msubsup" | "munder" | "mover" | "munderover" | "mmultiscripts" => {
                n.elems().next().map_or(Class::Ord, |b| self.class_of(b, false))
            }
            "mrow" | "mstyle" if n.elems().count() == 1 => n.elems().next().map_or(Class::Ord, |b| self.class_of(b, false)),
            _ => Class::Ord,
        }
    }

    fn is_stretchy(&self, n: &Node, count: usize) -> bool {
        if n.name != "mo" || count < 2 {
            return false;
        }
        let c = map_op(op_char(&n.text()));
        match attr_bool(n, "stretchy") {
            Some(v) => v && c != '\0',
            None => is_fence(c) && c != '\0',
        }
    }

    fn row(&mut self, kids: &[&Node], style: Style) -> Boxx {
        // a lone child is the row
        if kids.len() == 1 {
            return self.element(kids[0], style, None);
        }
        let n = kids.len();
        let sc = self.scale(style);
        let stretchy: Vec<bool> = kids.iter().map(|k| self.is_stretchy(k, n)).collect();
        let mut boxes: Vec<Option<Boxx>> = Vec::with_capacity(n);
        for (k, s) in kids.iter().zip(&stretchy) {
            boxes.push(if *s { None } else { Some(self.element(k, style, None)) });
        }
        // the size a delimiter stretches to: the most the children it encloses reach above and
        // below the axis. Open and close fences pair up; a lone one reaches to the row's end.
        let axis = self.k.axis * sc;
        let ranges = fence_ranges(kids, &stretchy);
        let mut targets = vec![0.0f64; n];
        for i in 0..n {
            if !stretchy[i] {
                continue;
            }
            let (lo, hi) = ranges[i];
            let (mut up, mut down) = (0.0f64, 0.0f64);
            for entry in boxes.iter().take(hi + 1).skip(lo) {
                if let Some(b) = entry.as_ref()
                    && (b.w > 0.0 || b.h > 0.0 || b.d > 0.0) {
                    up = up.max(b.h - axis);
                    down = down.max(b.d + axis);
                }
            }
            let m = up.max(down).max(0.0);
            targets[i] = (2.0 * m * 0.901).max(2.0 * m - 0.5 * sc);
        }
        for i in 0..n {
            if stretchy[i] {
                let b = self.operator(kids[i], style, Some(targets[i]));
                boxes[i] = Some(b);
            }
        }
        // classes, with TeX's rules turning a binary operator into an ordinary atom
        let mut classes: Vec<Class> = kids.iter().enumerate().map(|(i, k)| self.class_of(k, i == 0)).collect();
        for i in 0..n {
            if classes[i] == Class::Bin {
                let before = if i == 0 { None } else { Some(classes[i - 1]) };
                let after = if i + 1 == n { None } else { Some(classes[i + 1]) };
                let lonely = matches!(before, None | Some(Class::Bin | Class::Op | Class::Rel | Class::Open | Class::Punct));
                let trailing = matches!(after, None | Some(Class::Rel | Class::Close | Class::Punct));
                if lonely || trailing {
                    classes[i] = Class::Ord;
                }
            }
        }
        let mut out = Boxx::default();
        let mut first = true;
        for i in 0..n {
            let Some(b) = boxes[i].take() else { continue };
            let attrs = kids[i];
            let mut space = if first { 0.0 } else { self.pair_space(classes[i - 1], classes[i], style) };
            if kids[i].name == "mo" {
                // explicit spacing of the operator replaces the table's
                if let Some(l) = kids[i].attr("lspace").and_then(length) {
                    space = l * sc;
                }
            }
            if i > 0 && kids[i - 1].name == "mo"
                && let Some(r) = kids[i - 1].attr("rspace").and_then(length) {
                space = r * sc + if kids[i].name == "mo" { kids[i].attr("lspace").and_then(length).unwrap_or(0.0) * sc } else { 0.0 };
            }
            let _ = attrs;
            out.w += space;
            let x = out.w;
            out.put(&b, x, 0.0);
            out.w += b.w;
            out.h = out.h.max(b.h);
            out.d = out.d.max(b.d);
            out.ital = b.ital;
            out.accent = b.accent.map(|a| x + a);
            first = false;
        }
        out
    }

    fn pair_space(&self, left: Class, right: Class, style: Style) -> f64 {
        let v = SPACING[class_index(left)][class_index(right)];
        if v < 0 && style.level >= 2 {
            return 0.0;
        }
        f64::from(v.abs()) * (3.0 / 18.0) * if v.abs() == 2 { 4.0 / 6.0 } else if v.abs() == 3 { 5.0 / 9.0 } else { 1.0 } * self.scale(style)
    }

    // ---- fractions ----

    fn frac(&mut self, n: &Node, style: Style) -> Boxx {
        let kids = self.children_of(n);
        if kids.len() < 2 {
            self.note("mfrac needs a numerator and a denominator".to_owned());
            return Boxx::default();
        }
        let sc = self.scale(style);
        let k = self.k.clone();
        let num = self.element(kids[0], style.numerator(), None);
        let den = self.element(kids[1], style.denominator(), None);
        let display = style.display();
        let default_rule = k.frac_rule * sc;
        let rule = match n.attr("linethickness") {
            None => default_rule,
            Some("thin") => default_rule * 0.5,
            Some("medium") => default_rule,
            Some("thick") => default_rule * 2.0,
            Some(v) => {
                let has_unit = v.chars().any(|c| c.is_ascii_alphabetic() || c == '%');
                match length(v) {
                    Some(l) if has_unit => l * sc,
                    Some(l) => l * default_rule,
                    None => default_rule,
                }
            }
        };
        let axis = k.axis * sc;
        let (num_up, den_down) = if rule > 0.0 {
            if display { (k.frac_num_up_display * sc, k.frac_den_down_display * sc) } else { (k.frac_num_up * sc, k.frac_den_down * sc) }
        } else if display {
            (k.stack_top_up_display * sc, k.stack_bottom_down_display * sc)
        } else {
            (k.stack_top_up * sc, k.stack_bottom_down * sc)
        };
        let (mut up, mut down) = (num_up, den_down);
        if rule > 0.0 {
            let (gap_n, gap_d) = if display { (k.frac_num_gap_display * sc, k.frac_den_gap_display * sc) } else { (k.frac_num_gap * sc, k.frac_den_gap * sc) };
            up = up.max(axis + rule / 2.0 + gap_n + num.d);
            down = down.max(den.h + gap_d - axis + rule / 2.0);
        } else {
            let gap_min = if display { k.stack_gap_min_display * sc } else { k.stack_gap_min * sc };
            let gap = (up - num.d) - (den.h - down);
            if gap < gap_min {
                let extra = (gap_min - gap) / 2.0;
                up += extra;
                down += extra;
            }
        }
        let pad = if rule > 0.0 { 0.1 * sc } else { 0.0 };
        let side = 0.12 * sc;
        let inner = num.w.max(den.w) + 2.0 * pad;
        let mut out = Boxx::blank(inner + 2.0 * side, up + num.h, down + den.d);
        out.put(&num, side + pad + (inner - 2.0 * pad - num.w) / 2.0, up);
        out.put(&den, side + pad + (inner - 2.0 * pad - den.w) / 2.0, -down);
        out.rule(side, axis - rule / 2.0, inner, rule);
        out
    }

    // ---- radicals ----

    fn radical(&mut self, inner: Boxx, index: Option<Boxx>, style: Style) -> Boxx {
        let sc = self.scale(style);
        let k = self.k.clone();
        let rule = k.radical_rule * sc;
        let mut gap = if style.display() { k.radical_gap_display * sc } else { k.radical_gap * sc };
        let need = inner.h + inner.d + gap + rule;
        let font = self.fonts.primary();
        let sqrt = self.fonts.glyph('\u{221A}').filter(|(f, _)| *f == 0);
        // the sign: its items (baseline at 0), width, and ink top and bottom
        let (mut sign_items, sign_w, mut top, mut bottom): (Vec<Item>, f64, f64, f64);
        if let Some((_, base)) = sqrt {
            let mut variants = font.variants(base, true);
            variants.sort_by(|a, b| a.1.total_cmp(&b.1));
            let want = need / sc;
            let single = |g: GlyphId| -> (Vec<Item>, f64, f64, f64) {
                let b = font.bbox(g).unwrap_or([0.0, -0.1, 0.9, 0.9]);
                (vec![Item::Glyph { font: 0, gid: g.0, x: 0.0, y: 0.0, sx: sc, sy: sc }], font.advance(g) * sc, b[3] * sc, b[1] * sc)
            };
            let pick = variants.iter().find(|(_, adv)| *adv >= want - 1e-9).map(|(g, _)| *g);
            let parts = font.assembly(base, true);
            let built = match pick {
                Some(g) => Some(single(g)),
                None if variants.is_empty() && parts.is_empty() => Some(single(base)),
                None => self.assemble(&parts, want, true, sc).map(|a| {
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for it in &a.items {
                        if let Item::Glyph { gid, y, sy, .. } = it
                            && let Some(bb) = font.bbox(GlyphId(*gid)) {
                            lo = lo.min(y + bb[1] * sy);
                            hi = hi.max(y + bb[3] * sy);
                        }
                    }
                    (a.items, a.w, hi, lo)
                }),
            };
            let (items, gw, gtop, gbottom) = built.unwrap_or_else(|| single(variants.last().map_or(base, |v| v.0)));
            let glyph_h = gtop - gbottom;
            if glyph_h > need {
                gap += (glyph_h - need) / 2.0;
            }
            // the sign's top meets the rule's top
            let rule_top = inner.h + gap + rule;
            let dy = rule_top - gtop;
            sign_items = items.iter().map(|i| i.shifted(0.0, dy)).collect();
            sign_w = gw;
            top = rule_top;
            bottom = gbottom + dy;
        } else {
            // no radical sign in the font: draw one
            let h = need;
            let w = 0.6 * sc;
            let t = 0.045 * sc;
            let y0 = -inner.d;
            let seg = vec![
                Seg::M(0.0, y0 + h * 0.45),
                Seg::L(0.08 * sc, y0 + h * 0.48),
                Seg::L(0.25 * sc, y0),
                Seg::L(w, y0 + h),
                Seg::L(w, y0 + h - t),
                Seg::L(0.25 * sc, y0 + t),
                Seg::L(0.08 * sc, y0 + h * 0.48 - t),
                Seg::Z,
            ];
            sign_items = vec![Item::Path(seg)];
            sign_w = w;
            top = y0 + h;
            bottom = y0;
        }
        let _ = (&mut top, &mut bottom, &mut sign_items);
        let (ix_x, sign_x) = match &index {
            Some(ix) => {
                let before = (k.radical_kern_before * sc).max(0.0);
                (before, (before + ix.w + k.radical_kern_after * sc).max(0.0))
            }
            None => (0.0, 0.0),
        };
        let mut result = Boxx::default();
        result.items.extend(sign_items.iter().map(|i| i.shifted(sign_x, 0.0)));
        let mut h = top + k.radical_extra * sc;
        if let Some(ix) = &index {
            let raise = bottom + k.radical_raise * (top - bottom);
            result.put(ix, ix_x, raise + ix.d);
            h = h.max(raise + ix.d + ix.h);
        }
        let start = sign_x + sign_w;
        result.put(&inner, start, 0.0);
        result.rule(start, top - rule, inner.w, rule);
        result.w = start + inner.w;
        result.h = h;
        result.d = inner.d.max(-bottom).max(0.0);
        result
    }

    // ---- scripts ----

    fn scripts(&mut self, base: Boxx, sub: Option<Boxx>, sup: Option<Boxx>, style: Style) -> Boxx {
        let sc = self.scale(style);
        let k = self.k.clone();
        let mut u = base.h - k.sup_baseline_drop_max * sc;
        let mut v = base.d + k.sub_baseline_drop_min * sc;
        let sup_up = if style.cramped { k.sup_shift_up_cramped } else if style.display() { k.sup_shift_up_display } else { k.sup_shift_up } * sc;
        match (&sub, &sup) {
            (None, Some(sp)) => {
                u = u.max(sup_up).max(sp.d + k.sup_bottom_min * sc);
            }
            (Some(sb), None) => {
                v = v.max(k.sub_shift_down * sc).max(sb.h - k.sub_top_max * sc);
            }
            (Some(sb), Some(sp)) => {
                u = u.max(sup_up).max(sp.d + k.sup_bottom_min * sc);
                v = v.max(k.sub_shift_down * sc).max(sb.h - k.sub_top_max * sc);
                let gap = (u - sp.d) - (sb.h - v);
                if gap < k.sub_sup_gap_min * sc {
                    v += k.sub_sup_gap_min * sc - gap;
                    let psi = k.sup_bottom_max_with_sub * sc - (u - sp.d);
                    if psi > 0.0 {
                        u += psi;
                        v -= psi;
                    }
                }
            }
            (None, None) => {}
        }
        let mut out = Boxx::blank(base.w, base.h, base.d);
        out.put(&base, 0.0, 0.0);
        let mut width = base.w;
        if let Some(sp) = &sup {
            out.put(sp, base.w + base.ital, u);
            width = width.max(base.w + base.ital + sp.w);
            out.h = out.h.max(u + sp.h);
        }
        if let Some(sb) = &sub {
            out.put(sb, base.w, -v);
            width = width.max(base.w + sb.w);
            out.d = out.d.max(v + sb.d);
        }
        out.w = width + k.space_after_script * sc;
        out.ital = 0.0;
        out
    }

    fn sub_sup(&mut self, n: &Node, style: Style) -> Boxx {
        let kids = self.children_of(n);
        let need = if n.name == "msubsup" { 3 } else { 2 };
        if kids.len() < need {
            self.note(format!("<{}> needs {need} children", n.name));
            return kids.first().map_or(Boxx::default(), |k| self.element(k, style, None));
        }
        // a large operator with limits in display style takes them above and below
        let base = self.element(kids[0], style, None);
        let (sub_n, sup_n) = match n.name.as_str() {
            "msub" => (Some(kids[1]), None),
            "msup" => (None, Some(kids[1])),
            _ => (Some(kids[1]), Some(kids[2])),
        };
        let sub = sub_n.map(|s| self.element(s, style.sub(), None));
        let sup = sup_n.map(|s| self.element(s, style.sup(), None));
        self.scripts(base, sub, sup, style)
    }

    fn multiscripts(&mut self, n: &Node, style: Style) -> Boxx {
        let kids = self.children_of(n);
        let Some((base_n, rest)) = kids.split_first() else { return Boxx::default() };
        let base = self.element(base_n, style, None);
        let split = rest.iter().position(|k| k.name == "mprescripts").unwrap_or(rest.len());
        let (post, pre) = (&rest[..split], rest.get(split + 1..).unwrap_or(&[]));
        let lay = |this: &mut Self, list: &[&Node]| -> Vec<(Option<Boxx>, Option<Boxx>)> {
            list.chunks(2)
                .map(|c| {
                    let one = |this: &mut Self, nd: Option<&&Node>, st: Style| nd.filter(|x| x.name != "none").map(|x| this.element(x, st, None));
                    (one(this, c.first(), style.sub()), one(this, c.get(1), style.sup()))
                })
                .collect()
        };
        let post_boxes = lay(self, post);
        let pre_boxes = lay(self, pre);
        let mut cur = base;
        for (sub, sup) in post_boxes {
            cur = self.scripts(cur, sub, sup, style);
        }
        if !pre_boxes.is_empty() {
            // pre-scripts: a script block laid on an empty base, placed to the left
            let sc = self.scale(style);
            let mut left = Boxx::default();
            for (sub, sup) in pre_boxes {
                let empty = Boxx::blank(0.0, cur.h.min(0.45 * sc), 0.0);
                let mut block = self.scripts(empty, sub, sup, style);
                block.w -= self.k.space_after_script * sc * 0.0;
                let x = left.w;
                left.put(&block, x, 0.0);
                left.w += block.w;
                left.h = left.h.max(block.h);
                left.d = left.d.max(block.d);
            }
            let mut out = Boxx::blank(left.w + cur.w, left.h.max(cur.h), left.d.max(cur.d));
            out.put(&left, 0.0, 0.0);
            out.put(&cur, left.w, 0.0);
            return out;
        }
        cur
    }

    // ---- limits and accents ----

    fn under_over(&mut self, n: &Node, style: Style) -> Boxx {
        let kids = self.children_of(n);
        let need = if n.name == "munderover" { 3 } else { 2 };
        if kids.len() < need {
            self.note(format!("<{}> needs {need} children", n.name));
            return kids.first().map_or(Boxx::default(), |k| self.element(k, style, None));
        }
        let base_n = kids[0];
        let (under_n, over_n) = match n.name.as_str() {
            "munder" => (Some(kids[1]), None),
            "mover" => (None, Some(kids[1])),
            _ => (Some(kids[1]), Some(kids[2])),
        };
        let base_text = base_n.text();
        let base_char = op_char(&base_text);
        let movable = base_n.name == "mo" && attr_bool(base_n, "movablelimits").unwrap_or(is_largeop(map_op(base_char)) && !is_integral(map_op(base_char)))
            || (base_n.name == "mi" && matches!(base_text.as_str(), "lim" | "max" | "min" | "sup" | "inf" | "limsup" | "liminf" | "det" | "gcd" | "Pr"));
        let is_accent = |nd: Node_, attr: &str| nd.0.attr(attr).map(|v| v == "true").unwrap_or_else(|| nd.0.name == "mo" && is_horizontal_accent(map_op(op_char(&nd.0.text()))));
        let over_accent = over_n.is_some_and(|o| is_accent(Node_(o), "accent"));
        let under_accent = under_n.is_some_and(|o| is_accent(Node_(o), "accentunder"));
        if movable && !style.display() {
            // text style: the limits become scripts
            let base = self.element(base_n, style, None);
            let sub = under_n.map(|s| self.element(s, style.sub(), None));
            let sup = over_n.map(|s| self.element(s, style.sup(), None));
            return self.scripts(base, sub, sup, style);
        }
        let sc = self.scale(style);
        let k = self.k.clone();
        let base = self.element(base_n, style, None);
        let script_style = style.script();
        let mut over = over_n.map(|o| {
            if over_accent { self.accent_box(o, base.w, style, true) } else { (self.element(o, script_style, None), false) }
        });
        let mut under = under_n.map(|u| {
            if under_accent { self.accent_box(u, base.w, style, false) } else { (self.element(u, Style { cramped: true, ..script_style }, None), false) }
        });
        let ital = if movable || is_largeop(map_op(base_char)) { base.ital } else { 0.0 };
        let mut width = base.w;
        for (b, _) in [&over, &under].into_iter().flatten() {
            width = width.max(b.w);
        }
        let base_x = (width - base.w) / 2.0;
        let mut out = Boxx::blank(width, base.h, base.d);
        out.put(&base, base_x, 0.0);
        if let Some((o, bar)) = over.take() {
            let (x, y) = if over_accent && bar {
                (base_x, base.h)
            } else if over_accent {
                // TeX: raise the accent by what the base reaches above the accent base height, and keep it clear of the base
                let shift = (base.h - base.h.min(k.accent_base * sc)).max(0.0);
                let ink_bottom = self.ink_bottom(&o);
                let y = shift.max(base.h + 0.02 * sc - ink_bottom);
                let single = base.items.len() == 1 && matches!(base.items[0], Item::Glyph { .. });
                let attach = if single { base.accent.unwrap_or(base.w / 2.0) } else { base.w / 2.0 };
                (base_x + attach - o.accent.unwrap_or(o.w / 2.0), y)
            } else {
                ((width - o.w) / 2.0 + ital / 2.0, base.h + (k.upper_limit_gap_min * sc + o.d).max(k.upper_limit_rise_min * sc))
            };
            out.put(&o, x, y);
            out.h = out.h.max(y + o.h);
        }
        if let Some((u, bar)) = under.take() {
            let (x, y) = if under_accent && bar {
                (base_x, -(base.d + u.h))
            } else if under_accent {
                ((width - u.w) / 2.0, -(base.d + k.underbar_gap * sc + u.h))
            } else {
                ((width - u.w) / 2.0 - ital / 2.0, -(base.d + (k.lower_limit_gap_min * sc + u.h).max(k.lower_limit_drop_min * sc)))
            };
            out.put(&u, x, y);
            out.d = out.d.max(-y + u.d);
        }
        out
    }

    /// An accent over or under a base of width `w`: stretched along the base when it is a
    /// stretchy character. The flag is true for a plain bar, whose box already holds its gap.
    fn accent_box(&mut self, n: &Node, w: f64, style: Style, over: bool) -> (Boxx, bool) {
        let c = map_op(op_char(&n.text()));
        let c = match c {
            '^' | '\u{302}' => '\u{2C6}',
            '~' | '\u{303}' => '\u{2DC}',
            other => other,
        };
        let sc = self.scale(style);
        if n.name == "mo" && c != '\0' {
            let stretch = attr_bool(n, "stretchy").unwrap_or(is_horizontal_accent(c));
            if stretch && w > 0.0 {
                if matches!(c, '\u{AF}' | '\u{203E}' | '_' | '\u{2015}' | '\u{2500}') {
                    // a plain bar over or under the base, with the font's gap and thickness
                    let (t, gap) = if over { (self.k.overbar_rule * sc, self.k.overbar_gap * sc) } else { (self.k.underbar_rule * sc, self.k.underbar_gap * sc) };
                    let mut b = Boxx::blank(w, 0.0, 0.0);
                    if over {
                        b.items.push(Item::Rect { x: 0.0, y: gap, w, h: t });
                        b.h = gap + t;
                    } else {
                        b.items.push(Item::Rect { x: 0.0, y: -gap - t, w, h: t });
                        b.d = gap + t;
                    }
                    b.accent = Some(w / 2.0);
                    return (b, true);
                }
                let mut b = self.stretched(c, w, false, sc);
                if b.accent.is_none() {
                    b.accent = Some(b.w / 2.0);
                }
                return (b, false);
            }
        }
        let mut b = self.glyph_box(c, sc);
        if n.name != "mo" || c == '\0' {
            b = self.element(n, style, None);
        }
        if b.accent.is_none() {
            b.accent = Some(b.w / 2.0);
        }
        (b, false)
    }

    // ---- tables ----

    fn table(&mut self, n: &Node, style: Style) -> Boxx {
        let sc = self.scale(style);
        let mut style = style;
        if let Some(d) = attr_bool(n, "displaystyle") {
            style.level = if d { 0 } else { style.level.max(1) };
        } else if style.level == 0 {
            style.level = 1;
        }
        let align: Vec<String> = n.attr("columnalign").unwrap_or("center").split_whitespace().map(str::to_owned).collect();
        let rows: Vec<&Node> = n.elems().filter(|r| r.name == "mtr" || r.name == "mlabeledtr").collect();
        let mut grid: Vec<Vec<(Boxx, String)>> = Vec::new();
        for (ri, r) in rows.iter().enumerate() {
            let mut cells = Vec::new();
            let row_align: Vec<String> = r.attr("columnalign").map(|a| a.split_whitespace().map(str::to_owned).collect()).unwrap_or_default();
            let skip = usize::from(r.name == "mlabeledtr");
            for (ci, c) in r.elems().skip(skip).enumerate() {
                let b = if c.name == "mtd" {
                    let kids = self.children_of(c);
                    self.row(&kids, style)
                } else {
                    self.element(c, style, None)
                };
                let a = c
                    .attr("columnalign")
                    .map(str::to_owned)
                    .or_else(|| row_align.get(ci).or(row_align.last()).cloned())
                    .or_else(|| align.get(ci).or(align.last()).cloned())
                    .unwrap_or_else(|| "center".to_owned());
                cells.push((b, a));
            }
            let _ = ri;
            grid.push(cells);
        }
        if grid.is_empty() {
            return Boxx::default();
        }
        let ncols = grid.iter().map(Vec::len).max().unwrap_or(0);
        let mut col_w = vec![0.0f64; ncols];
        for row in &grid {
            for (i, (b, _)) in row.iter().enumerate() {
                col_w[i] = col_w[i].max(b.w);
            }
        }
        let col_gap = length(n.attr("columnspacing").and_then(|s| s.split_whitespace().next()).unwrap_or("0.8em")).unwrap_or(0.8) * sc;
        let row_gap = length(n.attr("rowspacing").and_then(|s| s.split_whitespace().next()).unwrap_or("1ex")).unwrap_or(0.45) * sc;
        let (strut_h, strut_d) = (0.7 * sc, 0.25 * sc);
        let heights: Vec<(f64, f64)> = grid
            .iter()
            .map(|row| row.iter().fold((strut_h, strut_d), |(h, d), (b, _)| (h.max(b.h), d.max(b.d))))
            .collect();
        let total_h: f64 = heights.iter().map(|(h, d)| h + d).sum::<f64>() + row_gap * (grid.len() - 1) as f64;
        let total_w: f64 = col_w.iter().sum::<f64>() + col_gap * ncols.saturating_sub(1) as f64;
        let axis = self.k.axis * sc;
        let mut out = Boxx::blank(total_w, total_h / 2.0 + axis, total_h / 2.0 - axis);
        let mut y = total_h / 2.0 + axis;
        for (ri, row) in grid.iter().enumerate() {
            let (h, d) = heights[ri];
            y -= h;
            let mut x = 0.0;
            for (ci, (b, a)) in row.iter().enumerate() {
                let dx = match a.as_str() {
                    "left" => 0.0,
                    "right" => col_w[ci] - b.w,
                    _ => (col_w[ci] - b.w) / 2.0,
                };
                out.put(b, x + dx, y);
                x += col_w[ci] + col_gap;
            }
            y -= d + row_gap;
        }
        out
    }

    // ---- enclosures ----

    fn enclose(&mut self, n: &Node, style: Style) -> Boxx {
        let sc = self.scale(style);
        let kids = self.children_of(n);
        let inner = self.row(&kids, style);
        let notation: Vec<&str> = n.attr("notation").unwrap_or("longdiv").split_whitespace().collect();
        if notation.contains(&"radical") {
            return self.radical(inner, None, style);
        }
        let t = 0.04 * sc;
        let pad = 0.15 * sc;
        let has = |s: &str| notation.contains(&s);
        let boxed = has("box") || has("roundedbox");
        let circle = has("circle");
        let (l, r, tp, bt) = (
            boxed || has("left") || has("longdiv") || has("actuarial") || circle,
            boxed || has("right") || circle,
            boxed || has("top") || has("longdiv") || has("actuarial") || circle,
            boxed || has("bottom") || circle,
        );
        let (pl, pr, pt, pb) = (
            if l || circle { pad } else { 0.0 },
            if r || circle { pad } else { 0.0 },
            if tp || circle { pad } else { 0.0 },
            if bt || circle { pad } else { 0.0 },
        );
        let (pl, pr, pt, pb) = if has("longdiv") { (0.3 * sc, 0.0, pad / 2.0, 0.0) } else { (pl, pr, pt, pb) };
        let mut out = Boxx::blank(inner.w + pl + pr, inner.h + pt, inner.d + pb);
        out.put(&inner, pl, 0.0);
        let (w, top, bot) = (out.w, out.h, -out.d);
        if circle {
            let (cx, cy) = (w / 2.0, (top + bot) / 2.0);
            let (rx, ry) = (w / 2.0 * 1.2, (top - bot) / 2.0 * 1.2);
            out.items.push(Item::Path(ellipse_ring(cx, cy, rx, ry, t)));
            out.w = w + (rx - w / 2.0) * 2.0;
            let shift = rx - w / 2.0;
            for it in &mut out.items {
                *it = it.shifted(shift, 0.0);
            }
            out.h = cy + ry;
            out.d = -(cy - ry);
        } else {
            if tp {
                out.rule(0.0, top - t, w, t);
            }
            if bt {
                out.rule(0.0, bot, w, t);
            }
            if l {
                out.rule(0.0, bot, t, top - bot);
            }
            if r {
                out.rule(w - t, bot, t, top - bot);
            }
        }
        if has("updiagonalstrike") {
            out.items.push(Item::Path(line_poly(0.0, bot, w, top, t)));
        }
        if has("downdiagonalstrike") {
            out.items.push(Item::Path(line_poly(0.0, top, w, bot, t)));
        }
        if has("horizontalstrike") {
            out.rule(0.0, (top + bot) / 2.0 - t / 2.0, w, t);
        }
        if has("verticalstrike") {
            out.rule(w / 2.0 - t / 2.0, bot, t, top - bot);
        }
        if has("mathmlfolio-none") {
            return inner;
        }
        out
    }
}

struct Node_<'a>(&'a Node);

/// For each stretchy operator of a row, the children (first, last) whose height it stretches to.
fn fence_ranges(kids: &[&Node], stretchy: &[bool]) -> Vec<(usize, usize)> {
    let n = kids.len();
    let ch = |i: usize| map_op(op_char(&kids[i].text()));
    let mut out = vec![(0, n.saturating_sub(1)); n];
    let mut stack: Vec<usize> = Vec::new();
    let mut open_bar: Option<usize> = None;
    let mut paired = vec![false; n];
    for i in 0..n {
        if !stretchy[i] {
            continue;
        }
        let c = ch(i);
        if is_open(c) {
            stack.push(i);
        } else if is_close(c) {
            if let Some(o) = stack.pop() {
                out[o] = (o + 1, i.saturating_sub(1).max(o + 1).min(n - 1));
                out[i] = out[o];
                if i == o + 1 {
                    out[o] = (o, o);
                    out[i] = (o, o);
                }
                paired[o] = true;
                paired[i] = true;
            } else {
                out[i] = (0, i.saturating_sub(1));
            }
        } else if "|\u{2016}\u{2225}".contains(c) {
            match open_bar.take() {
                Some(o) if ch(o) == c => {
                    out[o] = (o + 1, i.saturating_sub(1).max(o + 1).min(n - 1));
                    out[i] = out[o];
                    paired[o] = true;
                    paired[i] = true;
                }
                Some(o) => {
                    open_bar = Some(i);
                    let _ = o;
                }
                None => open_bar = Some(i),
            }
        }
    }
    for o in stack {
        out[o] = ((o + 1).min(n - 1), n - 1);
    }
    out
}

fn line_poly(x0: f64, y0: f64, x1: f64, y1: f64, t: f64) -> Vec<Seg> {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len = dx.hypot(dy).max(1e-9);
    let (nx, ny) = (-dy / len * t / 2.0, dx / len * t / 2.0);
    vec![Seg::M(x0 + nx, y0 + ny), Seg::L(x1 + nx, y1 + ny), Seg::L(x1 - nx, y1 - ny), Seg::L(x0 - nx, y0 - ny), Seg::Z]
}

/// A ring: the outer ellipse counter-clockwise, the inner one clockwise, so that it fills as a ring.
fn ellipse_ring(cx: f64, cy: f64, rx: f64, ry: f64, t: f64) -> Vec<Seg> {
    const K: f64 = 0.552_284_749_8;
    let ring = |rx: f64, ry: f64, ccw: bool| -> Vec<Seg> {
        let s = if ccw { 1.0 } else { -1.0 };
        vec![
            Seg::M(cx + rx, cy),
            Seg::C(cx + rx, cy + s * ry * K, cx + rx * K, cy + s * ry, cx, cy + s * ry),
            Seg::C(cx - rx * K, cy + s * ry, cx - rx, cy + s * ry * K, cx - rx, cy),
            Seg::C(cx - rx, cy - s * ry * K, cx - rx * K, cy - s * ry, cx, cy - s * ry),
            Seg::C(cx + rx * K, cy - s * ry, cx + rx, cy - s * ry * K, cx + rx, cy),
            Seg::Z,
        ]
    };
    let mut out = ring(rx, ry, true);
    out.extend(ring((rx - t).max(0.01), (ry - t).max(0.01), false));
    out
}
