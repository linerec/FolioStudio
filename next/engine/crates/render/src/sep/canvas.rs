//! Ink planes and how painted objects combine on them.
//!
//! Each plane holds one ink's coverage (0 = none, 255 = 100 %): what its plate carries.
//! Objects composite per plane in the subtractive blend space: separable blend modes act on
//! complemented values (PDF 11.3.3), so Normal, Multiply, Screen … work per ink as in the
//! document's CMYK transparency blend space; Normal mixes inks linearly by alpha.
//!
//! The page itself is a group with a transparent backdrop (paper is no ink): planes keep
//! premultiplied ink and alpha, and a blend mode only acts where something is painted below,
//! as in a PDF page group; over paper the result is the ink itself.
//!
//! An overprinting object paints only some planes; the others are left as they are. A
//! transparent layer (a group with opacity or a blend mode, a feathered object, content
//! opacity) keeps an alpha per plane, so an object that overprints inside it covers only the
//! planes it paints and the composited layer leaves the backdrop's other inks unchanged — the
//! result PDF transparency gives overprint in a non-isolated group.

use folio_model::{Affine, BlendMode, Rect};
use tiny_skia::{FillRule, Mask, Pixmap, PixmapPaint, Transform};

/// One page (or layer) of ink planes, plane-major.
pub(crate) struct Canvas {
    pub w: usize,
    pub h: usize,
    pub n: usize,
    /// `n` planes of `w × h`; premultiplied by `alpha` in a layer.
    pub ink: Vec<u8>,
    /// Per-plane alpha of a transparent layer; `None` on the page (opaque paper).
    pub alpha: Option<Vec<u8>>,
    /// Hard edges (flattening): pixels a vector shape covers partly (anti-aliased edge of a
    /// fill, stroke or glyph), 1 per pixel; `None` when not tracked.
    pub edges: Option<Vec<u8>>,
    /// The paint in progress has soft edges (an effect mask): no hard edges are recorded.
    pub soft: bool,
    /// Coverage mode (Flattener Preview): every paint lands as full ink, Normal, opaque.
    pub mask: bool,
    /// Coverage mode, an object that is not selected: what it paints takes away what lies below it (by its coverage and opacity).
    pub erase: bool,
}

/// Coverage over a rectangle of a canvas, in that canvas's pixels; outside it is 0.
#[derive(Clone, Debug)]
pub(crate) struct Cov {
    pub x0: usize,
    pub y0: usize,
    pub w: usize,
    pub h: usize,
    pub a: Vec<u8>,
}

/// What an object paints: one ink value per plane, or ink and alpha per pixel.
pub(crate) enum Src<'a> {
    Solid(&'a [u8]),
    /// Same rectangle as the coverage it is painted through.
    Pixels(&'a Pixels),
}

/// Per-pixel ink (straight, not premultiplied) and alpha over a coverage's rectangle.
pub(crate) struct Pixels {
    pub w: usize,
    pub h: usize,
    /// Per plane; `None` = no ink on that plane.
    pub planes: Vec<Option<Vec<u8>>>,
    pub alpha: Option<Vec<u8>>,
}

#[inline]
fn mul(a: u32, b: u32) -> u32 {
    (a * b + 127) / 255
}

/// Straight ink of premultiplied ink `v` with alpha `a` (0 where nothing is painted).
#[inline]
fn unpremultiply(v: u8, a: u32) -> u8 {
    (v as u32 * 255 + a / 2).checked_div(a).map_or(0, |x| x.min(255) as u8)
}

impl Canvas {
    /// An opaque canvas (paper as backdrop for blending).
    #[cfg(test)]
    pub fn page(w: usize, h: usize, n: usize) -> Canvas {
        Canvas { w, h, n, ink: vec![0; n * w * h], alpha: None, edges: None, soft: false, mask: false, erase: false }
    }

    pub fn layer(w: usize, h: usize, n: usize) -> Canvas {
        Canvas { w, h, n, ink: vec![0; n * w * h], alpha: Some(vec![0; n * w * h]), edges: None, soft: false, mask: false, erase: false }
    }

    /// A transparent layer of `w × h` that tracks edges when this canvas does.
    pub fn sublayer(&self, w: usize, h: usize) -> Canvas {
        let mut l = Canvas::layer(w, h, self.n);
        l.mask = self.mask; // a layer of an unselected object records coverage (erase off); the parent takes it away
        if self.edges.is_some() {
            l.edges = Some(vec![0; w * h]);
        }
        l
    }

    /// Records the hard edges of `cov`: partly covered pixels (anti-aliased edges) and
    /// covered pixels next to uncovered ones (edges that fall on the pixel grid).
    fn mark_edges(&mut self, cov: &Cov) {
        let (w, h) = (self.w, self.h);
        let Some(e) = self.edges.as_mut() else { return };
        if self.soft {
            return;
        }
        let at = |x: usize, y: usize| cov.a[y * cov.w + x];
        for row in 0..cov.h {
            let y = cov.y0 + row;
            if y >= h {
                break;
            }
            let cw = cov.w.min(w.saturating_sub(cov.x0));
            for col in 0..cw {
                let c = at(col, row);
                let edge = (c != 0 && c != 255)
                    || (c == 255
                        && ((col > 0 && at(col - 1, row) == 0) || (col + 1 < cov.w && at(col + 1, row) == 0) || (row > 0 && at(col, row - 1) == 0) || (row + 1 < cov.h && at(col, row + 1) == 0)));
                if edge {
                    e[y * w + cov.x0 + col] = 1;
                }
            }
        }
    }

    #[cfg(test)]
    pub fn plane(&self, p: usize) -> &[u8] {
        &self.ink[p * self.w * self.h..(p + 1) * self.w * self.h]
    }

    /// Paints `src` through `cov` on the planes marked in `touched`, with `mode` and
    /// `opacity` (0–1).
    pub fn paint(&mut self, cov: &Cov, src: &Src, touched: &[bool], mode: BlendMode, opacity: f64) {
        if self.mask && self.erase {
            self.erase_cov(cov, opacity);
            return;
        }
        if self.mask {
            // coverage: every paint is full ink, Normal, opaque; the shape (and its soft edge) is all that counts
            let full = vec![255u8; self.n];
            let all = vec![true; self.n];
            self.mask = false;
            self.paint(cov, &Src::Solid(&full), &all, BlendMode::Normal, 1.0);
            self.mask = true;
            return;
        }
        let op = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        if op == 0 || cov.w == 0 || cov.h == 0 {
            return;
        }
        if self.edges.is_some() {
            self.mark_edges(cov);
        }
        if let Src::Pixels(px) = src {
            debug_assert!(px.w == cov.w && px.h == cov.h);
        }
        if nonseparable(mode) {
            self.paint_nonseparable(cov, src, touched, mode, op);
            return;
        }
        for p in 0..self.n {
            if !touched.get(p).copied().unwrap_or(false) {
                continue;
            }
            // A spot ink the source does not paint keeps its backdrop under blend modes other
            // than Normal (as InDesign and Ghostscript composite it): blended as "no ink", a
            // Difference or Exclusion would print the spot at full strength, a Screen or
            // Lighten erase it. Normal knocks it out.
            let unpainted_spot = p >= 4
                && mode != BlendMode::Normal
                && match src {
                    Src::Solid(v) => v.get(p).copied().unwrap_or(0) == 0,
                    Src::Pixels(px) => px.planes.get(p).is_none_or(|pl| pl.is_none()),
                };
            if unpainted_spot {
                continue;
            }
            match src {
                Src::Solid(v) => {
                    let s = v.get(p).copied().unwrap_or(0);
                    self.plane_op(p, cov, mode, |_, c| (s, mul(c as u32, op)));
                }
                Src::Pixels(px) => {
                    let plane = px.planes.get(p).and_then(|x| x.as_deref());
                    let alpha = px.alpha.as_deref();
                    self.plane_op(p, cov, mode, |i, c| {
                        let s = plane.map_or(0, |pl| pl[i]);
                        let a = alpha.map_or(255, |al| al[i] as u32);
                        (s, mul(mul(c as u32, a), op))
                    });
                }
            }
        }
    }

    /// One plane over `cov`: `f(index in cov, coverage)` gives the straight ink and alpha.
    fn plane_op(&mut self, p: usize, cov: &Cov, mode: BlendMode, f: impl Fn(usize, u8) -> (u8, u32)) {
        let (w, h) = (self.w, self.h);
        let base = p * w * h;
        let Canvas { ink, alpha, .. } = self;
        for row in 0..cov.h {
            let y = cov.y0 + row;
            if y >= h {
                break;
            }
            let cw = cov.w.min(w.saturating_sub(cov.x0));
            let off = base + y * w + cov.x0;
            for col in 0..cw {
                let ci = row * cov.w + col;
                let c = cov.a[ci];
                if c == 0 {
                    continue;
                }
                let (s, a) = f(ci, c);
                if a == 0 {
                    continue;
                }
                let i = off + col;
                if a >= 255 && mode == BlendMode::Normal {
                    // opaque: the source replaces what is there
                    ink[i] = s;
                    if let Some(al) = alpha {
                        al[i] = 255;
                    }
                    continue;
                }
                match alpha {
                    None => ink[i] = over_opaque(ink[i], s, a, mode),
                    Some(al) => {
                        let (ni, na) = over_layer(ink[i], al[i], s, a, mode);
                        ink[i] = ni;
                        al[i] = na;
                    }
                }
            }
        }
    }

    /// Hue, saturation, colour and luminosity: C, M, Y as complemented R, G, B; black from
    /// the backdrop (luminosity: from the source); spot inks blend Normal (PDF 11.3.5).
    fn paint_nonseparable(&mut self, cov: &Cov, src: &Src, touched: &[bool], mode: BlendMode, op: u32) {
        let (w, h, n) = (self.w, self.h, self.n);
        let wh = w * h;
        let mut sv = vec![0u8; n];
        let mut bv = vec![0u8; n];
        for row in 0..cov.h {
            let y = cov.y0 + row;
            if y >= h {
                break;
            }
            for col in 0..cov.w.min(w.saturating_sub(cov.x0)) {
                let ci = row * cov.w + col;
                let c = cov.a[ci] as u32;
                if c == 0 {
                    continue;
                }
                let a = match src {
                    Src::Solid(v) => {
                        for (p, out) in sv.iter_mut().enumerate() {
                            *out = v.get(p).copied().unwrap_or(0);
                        }
                        mul(c, op)
                    }
                    Src::Pixels(px) => {
                        for (p, out) in sv.iter_mut().enumerate() {
                            *out = px.planes.get(p).and_then(|x| x.as_deref()).map_or(0, |pl| pl[ci]);
                        }
                        mul(mul(c, px.alpha.as_deref().map_or(255, |al| al[ci] as u32)), op)
                    }
                };
                if a == 0 {
                    continue;
                }
                let i = y * w + cov.x0 + col;
                for (p, out) in bv.iter_mut().enumerate() {
                    *out = match &self.alpha {
                        None => self.ink[p * wh + i],
                        Some(al) => {
                            let da = al[p * wh + i] as u32;
                            unpremultiply(self.ink[p * wh + i], da)
                        }
                    };
                }
                let rgb = |v: &[u8]| [0, 1, 2].map(|p| 1.0 - v.get(p).copied().unwrap_or(0) as f32 / 255.0);
                let mixed = nonsep(mode, rgb(&bv), rgb(&sv));
                for p in 0..n {
                    if !touched.get(p).copied().unwrap_or(false) {
                        continue;
                    }
                    let target = match p {
                        0..=2 => ((1.0 - mixed[p]) * 255.0).round().clamp(0.0, 255.0) as u8,
                        3 if mode == BlendMode::Luminosity => sv[3],
                        3 => bv[3],
                        _ => sv[p],
                    };
                    let k = p * wh + i;
                    match &mut self.alpha {
                        None => self.ink[k] = over_opaque(self.ink[k], target, a, BlendMode::Normal),
                        Some(al) => {
                            let (ni, na) = over_layer_result(self.ink[k], al[k], sv[p], target, a);
                            self.ink[k] = ni;
                            al[k] = na;
                        }
                    }
                }
            }
        }
    }

    /// Takes away coverage under `cov` (coverage mode): the share `cov × opacity` of every plane.
    fn erase_cov(&mut self, cov: &Cov, opacity: f64) {
        let op = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        let (w, h, n) = (self.w, self.h, self.n);
        for row in 0..cov.h {
            let y = cov.y0 + row;
            if y >= h {
                break;
            }
            for col in 0..cov.w.min(w.saturating_sub(cov.x0)) {
                let c = cov.a[row * cov.w + col] as u32;
                if c == 0 {
                    continue;
                }
                let keep = 255 - mul(c, op);
                for p in 0..n {
                    let i = p * w * h + y * w + cov.x0 + col;
                    self.ink[i] = mul(self.ink[i] as u32, keep) as u8;
                    if let Some(a) = self.alpha.as_mut() {
                        a[i] = mul(a[i] as u32, keep) as u8;
                    }
                }
            }
        }
    }

    /// Composites `layer` (a transparent canvas at `(lx, ly)` in this canvas) with `mode`,
    /// `opacity` and an optional soft mask in the layer's pixels.
    pub fn composite(&mut self, layer: &Canvas, lx: usize, ly: usize, mode: BlendMode, opacity: f64, mask: Option<&Cov>) {
        if self.mask && self.erase {
            // an unselected group: its coverage takes away what lies below, by the group's opacity
            let (lw, lh) = (layer.w, layer.h);
            let wh = lw * lh;
            if let Some(lal) = layer.alpha.as_deref() {
                let cov = Cov { x0: lx, y0: ly, w: lw, h: lh, a: lal[..wh].to_vec() };
                self.erase_cov(&cov, opacity);
            }
            return;
        }
        let (mode, opacity) = if self.mask { (BlendMode::Normal, 1.0) } else { (mode, opacity) };
        let Some(lal) = layer.alpha.as_deref() else { return };
        let op = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        if op == 0 {
            return;
        }
        let (lw, lh) = (layer.w, layer.h);
        let cw = lw.min(self.w.saturating_sub(lx));
        let ch = lh.min(self.h.saturating_sub(ly));
        if cw == 0 || ch == 0 {
            return;
        }
        // the layer's hard edges stay hard edges of this canvas
        if let (Some(dst), Some(src)) = (self.edges.as_mut(), layer.edges.as_ref()) {
            for row in 0..ch {
                for col in 0..cw {
                    if src[row * lw + col] != 0 {
                        dst[(ly + row) * self.w + lx + col] = 1;
                    }
                }
            }
        }
        // coverage = the layer's own rectangle (its alphas carry the shapes) times the mask
        let cov = Cov { x0: lx, y0: ly, w: cw, h: ch, a: (0..cw * ch).map(|i| mask.map_or(255, |m| m.at(i % cw, i / cw))).collect() };
        if nonseparable(mode) {
            // straight layer values as a per-pixel source with the layer's black alpha
            let mut planes = Vec::with_capacity(self.n);
            for p in 0..self.n {
                let mut v = vec![0u8; cw * ch];
                for (i, out) in v.iter_mut().enumerate() {
                    let li = p * lw * lh + (i / cw) * lw + i % cw;
                    let a = lal[li] as u32;
                    *out = unpremultiply(layer.ink[li], a);
                }
                planes.push(Some(v));
            }
            let alpha = (0..cw * ch).map(|i| (0..self.n).map(|p| lal[p * lw * lh + (i / cw) * lw + i % cw]).max().unwrap_or(0)).collect();
            let px = Pixels { w: cw, h: ch, planes, alpha: Some(alpha) };
            let all = vec![true; self.n];
            self.paint_nonseparable(&cov, &Src::Pixels(&px), &all, mode, op);
            return;
        }
        for p in 0..self.n {
            let lb = p * lw * lh;
            let (lin, lal) = (&layer.ink, lal);
            // a spot ink the layer does not paint keeps its backdrop (see `paint`)
            let keep_unpainted = p >= 4 && mode != BlendMode::Normal;
            self.plane_op(p, &cov, mode, |ci, c| {
                let li = lb + (ci / cw) * lw + ci % cw;
                let la = lal[li] as u32;
                if la == 0 || (keep_unpainted && lin[li] == 0) {
                    return (0, 0);
                }
                let s = ((lin[li] as u32 * 255 + la / 2) / la).min(255) as u8;
                (s, mul(mul(la, c as u32), op))
            });
        }
    }
}

/// Normal blending on an opaque backdrop: `d + (B(d, s) − d) · a`.
#[inline]
fn over_opaque(d: u8, s: u8, a: u32, mode: BlendMode) -> u8 {
    let r = if mode == BlendMode::Normal { s as u32 } else { blend(mode, d, s) as u32 };
    ((d as u32 * (255 - a) + r * a + 127) / 255) as u8
}

/// Premultiplied ink `dp` with alpha `da`, source `s` (straight) with alpha `a`.
#[inline]
fn over_layer(dp: u8, da: u8, s: u8, a: u32, mode: BlendMode) -> (u8, u8) {
    if mode == BlendMode::Normal || da == 0 {
        let ink = (s as u32 * a + dp as u32 * (255 - a) + 127) / 255;
        let alpha = a + (da as u32 * (255 - a) + 127) / 255;
        return (ink.min(255) as u8, alpha.min(255) as u8);
    }
    let (af, daf) = (a as f32 / 255.0, da as f32 / 255.0);
    let (sf, dpf) = (s as f32 / 255.0, dp as f32 / 255.0);
    let cb = (dpf / daf).min(1.0);
    let b = blend_f(mode, cb, sf);
    let co = sf * af * (1.0 - daf) + dpf * (1.0 - af) + af * daf * b;
    let ao = af + daf - af * daf;
    ((co * 255.0).round().clamp(0.0, 255.0) as u8, (ao * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// Premultiplied compositing with a blend result `b` already computed from the backdrop's
/// straight value: `s` alone where the backdrop is empty, `b` where it is fully painted.
#[inline]
fn over_layer_result(dp: u8, da: u8, s: u8, b: u8, a: u32) -> (u8, u8) {
    let (af, daf) = (a as f32 / 255.0, da as f32 / 255.0);
    let co = s as f32 / 255.0 * af * (1.0 - daf) + dp as f32 / 255.0 * (1.0 - af) + af * daf * (b as f32 / 255.0);
    let ao = af + daf - af * daf;
    ((co * 255.0).round().clamp(0.0, 255.0) as u8, (ao * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn nonseparable(mode: BlendMode) -> bool {
    matches!(mode, BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity)
}

#[inline]
fn blend(mode: BlendMode, d: u8, s: u8) -> u8 {
    (blend_f(mode, d as f32 / 255.0, s as f32 / 255.0) * 255.0).round().clamp(0.0, 255.0) as u8
}

/// A separable blend mode on ink values (0–1): applied to the complements (the additive
/// values), the result complemented back.
fn blend_f(mode: BlendMode, ink_b: f32, ink_s: f32) -> f32 {
    let (b, s) = (1.0 - ink_b, 1.0 - ink_s);
    let hard = |b: f32, s: f32| if s <= 0.5 { b * 2.0 * s } else { let t = 2.0 * s - 1.0; b + t - b * t };
    let r = match mode {
        BlendMode::Normal => s,
        BlendMode::Multiply => b * s,
        BlendMode::Screen => b + s - b * s,
        BlendMode::Overlay => hard(s, b),
        BlendMode::HardLight => hard(b, s),
        BlendMode::Darken => b.min(s),
        BlendMode::Lighten => b.max(s),
        BlendMode::ColorDodge => {
            if b == 0.0 { 0.0 } else if s >= 1.0 { 1.0 } else { (b / (1.0 - s)).min(1.0) }
        }
        BlendMode::ColorBurn => {
            if b >= 1.0 { 1.0 } else if s <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / s).min(1.0) }
        }
        BlendMode::SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        BlendMode::Difference => (b - s).abs(),
        BlendMode::Exclusion => b + s - 2.0 * b * s,
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => s,
    };
    1.0 - r
}

/// Non-separable blend modes on RGB (0–1), W3C compositing definitions.
fn nonsep(mode: BlendMode, b: [f32; 3], s: [f32; 3]) -> [f32; 3] {
    let lum = |c: [f32; 3]| 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    let clip = |c: [f32; 3]| {
        let l = lum(c);
        let n = c.iter().cloned().fold(f32::MAX, f32::min);
        let x = c.iter().cloned().fold(f32::MIN, f32::max);
        let mut c = c;
        if n < 0.0 && l - n > 1e-6 {
            c = c.map(|v| l + (v - l) * l / (l - n));
        }
        if x > 1.0 && x - l > 1e-6 {
            c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
        }
        c
    };
    let set_lum = |c: [f32; 3], l: f32| {
        let d = l - lum(c);
        clip(c.map(|v| v + d))
    };
    let sat = |c: [f32; 3]| c.iter().cloned().fold(f32::MIN, f32::max) - c.iter().cloned().fold(f32::MAX, f32::min);
    let set_sat = |c: [f32; 3], s: f32| {
        let (mut idx, mut out) = ([0usize, 1, 2], [0f32; 3]);
        idx.sort_by(|&i, &j| c[i].total_cmp(&c[j]));
        let (lo, mid, hi) = (idx[0], idx[1], idx[2]);
        if c[hi] > c[lo] {
            out[mid] = (c[mid] - c[lo]) * s / (c[hi] - c[lo]);
            out[hi] = s;
        }
        out[lo] = 0.0;
        out
    };
    match mode {
        BlendMode::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        BlendMode::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        BlendMode::Color => set_lum(s, lum(b)),
        BlendMode::Luminosity => set_lum(b, lum(s)),
        _ => s,
    }
}

impl Cov {
    /// Coverage at `(x, y)` of the canvas.
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> u8 {
        if x < self.x0 || y < self.y0 || x >= self.x0 + self.w || y >= self.y0 + self.h {
            return 0;
        }
        self.a[(y - self.y0) * self.w + (x - self.x0)]
    }

    /// This coverage times `other` (0 outside `other`).
    pub fn multiply(&mut self, other: &Cov) {
        for row in 0..self.h {
            let y = self.y0 + row;
            for col in 0..self.w {
                let i = row * self.w + col;
                if self.a[i] != 0 {
                    self.a[i] = mul(self.a[i] as u32, other.at(self.x0 + col, y) as u32) as u8;
                }
            }
        }
    }

    /// The part inside the rectangle `x0, y0, w, h` of this canvas, in that rectangle's
    /// own pixels (for a layer placed there).
    pub fn rebase(&self, x0: usize, y0: usize, w: usize, h: usize) -> Cov {
        let mut out = Cov { x0: 0, y0: 0, w, h, a: vec![0; w * h] };
        for row in 0..h {
            for col in 0..w {
                out.a[row * w + col] = self.at(x0 + col, y0 + row);
            }
        }
        out
    }
}

/// The device pixels a rectangle touches on a `w × h` canvas, grown by `margin`, inside
/// `clip`'s rectangle when there is one: `(x0, y0, x1, y1)`.
pub(crate) fn pixel_box(r: &Rect, w: usize, h: usize, margin: f64, clip: Option<&Cov>) -> Option<(usize, usize, usize, usize)> {
    if !(r.left.is_finite() && r.top.is_finite() && r.right.is_finite() && r.bottom.is_finite()) {
        return None;
    }
    let mut x0 = (r.left - margin).floor().max(0.0) as i64;
    let mut y0 = (r.top - margin).floor().max(0.0) as i64;
    let mut x1 = ((r.right + margin).ceil() as i64).min(w as i64);
    let mut y1 = ((r.bottom + margin).ceil() as i64).min(h as i64);
    if let Some(c) = clip {
        x0 = x0.max(c.x0 as i64);
        y0 = y0.max(c.y0 as i64);
        x1 = x1.min((c.x0 + c.w) as i64);
        y1 = y1.min((c.y0 + c.h) as i64);
    }
    (x1 > x0 && y1 > y0).then_some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
}

fn path_rect(path: &tiny_skia::Path, t: &Affine) -> Rect {
    let b = path.bounds();
    t.apply_rect(&Rect { left: b.left() as f64, top: b.top() as f64, right: b.right() as f64, bottom: b.bottom() as f64 })
}

fn offset(x0: usize, y0: usize, t: &Affine) -> Transform {
    crate::to_skia(&Affine::translate(-(x0 as f64), -(y0 as f64)).then_inner(t))
}

/// Anti-aliased coverage of filling `path` (device transform `t`) on a `w × h` canvas.
pub(crate) fn fill_cov(w: usize, h: usize, path: &tiny_skia::Path, rule: FillRule, t: &Affine, clip: Option<&Cov>) -> Option<Cov> {
    let (x0, y0, x1, y1) = pixel_box(&path_rect(path, t), w, h, 1.0, clip)?;
    let mut m = Mask::new((x1 - x0) as u32, (y1 - y0) as u32)?;
    m.fill_path(path, rule, true, offset(x0, y0, t));
    let mut cov = Cov { x0, y0, w: x1 - x0, h: y1 - y0, a: m.take() };
    if let Some(c) = clip {
        cov.multiply(c);
    }
    Some(cov)
}

/// Coverage of stroking `path`, as tiny-skia strokes (hairlines below one pixel included).
pub(crate) fn stroke_cov(w: usize, h: usize, path: &tiny_skia::Path, stroke: &tiny_skia::Stroke, t: &Affine, clip: Option<&Cov>) -> Option<Cov> {
    let unit = (t.a * t.d - t.b * t.c).abs().sqrt();
    let reach = stroke.width as f64 * unit * 0.5 * (stroke.miter_limit as f64).max(1.5) + 2.0;
    let (x0, y0, x1, y1) = pixel_box(&path_rect(path, t), w, h, reach, clip)?;
    let mut pm = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32)?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    pm.stroke_path(path, &paint, stroke, offset(x0, y0, t), None);
    let mut cov = Cov { x0, y0, w: x1 - x0, h: y1 - y0, a: pm.data().chunks_exact(4).map(|p| p[3]).collect() };
    if let Some(c) = clip {
        cov.multiply(c);
    }
    Some(cov)
}

/// Coverage of an effect raster (`ppu` pixels per unit of the user space `t` maps to the
/// device), bilinear as the screen draws effect masks.
pub(crate) fn alpha_cov(w: usize, h: usize, a: &crate::effects::Alpha, t: &Affine, ppu: f64, clip: Option<&Cov>) -> Option<Cov> {
    let to_dev = t.then_inner(&Affine::translate(a.rect.left, a.rect.top)).then_inner(&Affine::scale(1.0 / ppu, 1.0 / ppu));
    let r = to_dev.apply_rect(&Rect { left: 0.0, top: 0.0, right: a.w as f64, bottom: a.h as f64 });
    let (x0, y0, x1, y1) = pixel_box(&r, w, h, 1.0, clip)?;
    let mut src = Pixmap::new(a.w as u32, a.h as u32)?;
    for (px, al) in src.data_mut().chunks_exact_mut(4).zip(&a.data) {
        px.copy_from_slice(&[*al, *al, *al, *al]);
    }
    let mut pm = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32)?;
    let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
    pm.draw_pixmap(0, 0, src.as_ref(), &paint, offset(x0, y0, &to_dev), None);
    let mut cov = Cov { x0, y0, w: x1 - x0, h: y1 - y0, a: pm.data().chunks_exact(4).map(|p| p[3]).collect() };
    if let Some(c) = clip {
        cov.multiply(c);
    }
    Some(cov)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(w: usize, h: usize, v: u8) -> Cov {
        Cov { x0: 0, y0: 0, w, h, a: vec![v; w * h] }
    }

    #[test]
    fn knockout_overprint_and_multiply() {
        let mut c = Canvas::page(2, 1, 5);
        // cyan 100 % over everything
        c.paint(&full(2, 1, 255), &Src::Solid(&[255, 0, 0, 0, 0]), &[true; 5], BlendMode::Normal, 1.0);
        // knockout magenta 50 %: cyan gone
        c.paint(&Cov { x0: 0, y0: 0, w: 1, h: 1, a: vec![255] }, &Src::Solid(&[0, 128, 0, 0, 0]), &[true; 5], BlendMode::Normal, 1.0);
        assert_eq!((c.plane(0)[0], c.plane(1)[0]), (0, 128));
        // overprinting black: cyan stays
        c.paint(&Cov { x0: 1, y0: 0, w: 1, h: 1, a: vec![255] }, &Src::Solid(&[0, 0, 0, 255, 0]), &[false, false, false, true, false], BlendMode::Normal, 1.0);
        assert_eq!((c.plane(0)[1], c.plane(3)[1]), (255, 255));
        // multiply 50 % yellow over 50 % yellow: 1 − 0.5 · 0.5 = 75 %
        let mut m = Canvas::page(1, 1, 4);
        m.paint(&full(1, 1, 255), &Src::Solid(&[0, 0, 128, 0]), &[true; 4], BlendMode::Normal, 1.0);
        m.paint(&full(1, 1, 255), &Src::Solid(&[0, 0, 128, 0]), &[true; 4], BlendMode::Multiply, 1.0);
        assert!((m.plane(2)[0] as i32 - 192).abs() <= 1, "{}", m.plane(2)[0]);
    }

    #[test]
    fn blend_modes_leave_spot_inks_the_source_does_not_paint() {
        // cyan, then a process orange with Difference: no spot ink appears (it would print
        // at 100 % blended as "no ink"); over a spot backdrop the spot stays as it was
        let mut c = Canvas::page(2, 1, 5);
        c.paint(&full(2, 1, 255), &Src::Solid(&[255, 0, 0, 0, 0]), &[true; 5], BlendMode::Normal, 1.0);
        c.paint(&Cov { x0: 1, y0: 0, w: 1, h: 1, a: vec![255] }, &Src::Solid(&[0, 0, 0, 0, 200]), &[true; 5], BlendMode::Normal, 1.0);
        for mode in [BlendMode::Difference, BlendMode::Exclusion, BlendMode::Screen, BlendMode::Lighten] {
            let mut d = Canvas::page(2, 1, 5);
            d.ink.copy_from_slice(&c.ink);
            d.paint(&full(2, 1, 255), &Src::Solid(&[0, 166, 242, 0, 0]), &[true; 5], mode, 1.0);
            assert_eq!((d.plane(4)[0], d.plane(4)[1]), (0, 200), "{mode:?}");
        }
        // the same through a layer (an object with a blend mode is composited as a group)
        let mut layer = Canvas::layer(2, 1, 5);
        layer.paint(&full(2, 1, 255), &Src::Solid(&[0, 166, 242, 0, 0]), &[true; 5], BlendMode::Normal, 1.0);
        let mut d = Canvas::page(2, 1, 5);
        d.ink.copy_from_slice(&c.ink);
        d.composite(&layer, 0, 0, BlendMode::Difference, 1.0, None);
        assert_eq!((d.plane(4)[0], d.plane(4)[1]), (0, 200));
        // Normal knocks the spot out
        c.paint(&full(2, 1, 255), &Src::Solid(&[0, 166, 242, 0, 0]), &[true; 5], BlendMode::Normal, 1.0);
        assert_eq!(c.plane(4)[1], 0);
    }

    #[test]
    fn layer_keeps_overprinted_inks_of_the_backdrop() {
        // cyan background, a 50 % opaque layer with overprinting black: cyan untouched
        let mut page = Canvas::page(1, 1, 4);
        page.paint(&full(1, 1, 255), &Src::Solid(&[255, 0, 0, 0]), &[true; 4], BlendMode::Normal, 1.0);
        let mut layer = Canvas::layer(1, 1, 4);
        layer.paint(&full(1, 1, 255), &Src::Solid(&[0, 0, 0, 255]), &[false, false, false, true], BlendMode::Normal, 1.0);
        page.composite(&layer, 0, 0, BlendMode::Normal, 0.5, None);
        assert_eq!(page.plane(0)[0], 255);
        assert_eq!(page.plane(3)[0], 128);
        // a knockout object in the layer reduces the cyan by the opacity
        let mut layer = Canvas::layer(1, 1, 4);
        layer.paint(&full(1, 1, 255), &Src::Solid(&[0, 0, 0, 255]), &[true; 4], BlendMode::Normal, 1.0);
        page.composite(&layer, 0, 0, BlendMode::Normal, 0.5, None);
        assert!((page.plane(0)[0] as i32 - 128).abs() <= 1, "{}", page.plane(0)[0]);
    }

    #[test]
    fn blend_modes_act_only_over_painted_ink() {
        // the page group: Screen over empty paper is the object; over cyan it lightens
        let mut g = Canvas::layer(2, 1, 4);
        g.paint(&Cov { x0: 1, y0: 0, w: 1, h: 1, a: vec![255] }, &Src::Solid(&[255, 0, 0, 0]), &[true; 4], BlendMode::Normal, 1.0);
        g.paint(&full(2, 1, 255), &Src::Solid(&[0, 160, 255, 0]), &[true; 4], BlendMode::Screen, 1.0);
        assert_eq!((g.plane(1)[0], g.plane(2)[0]), (160, 255));
        // screen in CMYK multiplies inks: over 100 % cyan, magenta and yellow vanish
        assert_eq!((g.plane(0)[1], g.plane(1)[1], g.plane(2)[1]), (0, 0, 0));
    }

    #[test]
    fn coverage_of_a_filled_rectangle() {
        let path = tiny_skia::PathBuilder::from_rect(tiny_skia::Rect::from_ltrb(1.0, 1.0, 3.0, 2.5).unwrap());
        let cov = fill_cov(8, 8, &path, FillRule::Winding, &Affine::IDENTITY, None).unwrap();
        assert_eq!(cov.at(1, 1), 255);
        assert_eq!(cov.at(2, 2), 128);
        assert_eq!(cov.at(3, 1), 0);
        assert_eq!(cov.at(5, 5), 0);
    }
}
