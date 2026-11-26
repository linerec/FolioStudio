//! The composition of a PSD's shown layers (`flatten`'s picture): layers bottom to top onto a
//! transparent canvas, each with its blend mode, opacity × fill and pixel mask; a clipping
//! group (a layer and the clipped layers above it) is composed on its own and then blended
//! with the base layer's mode; a layer group is either composed on its own (a mode of its
//! own) or, for pass-through, composed directly onto what is below. Rules measured against
//! InDesign 2025 (psdlayers-c1).
use super::blend::Mode;
use super::{Node, Psd, Rec, Rgb, plane};

pub(super) struct Canvas {
    pub w: usize,
    pub h: usize,
    pub nc: usize,
    /// Interleaved colour, `nc` bytes per pixel (not premultiplied).
    pub color: Vec<u8>,
    pub alpha: Vec<u8>,
}

impl Canvas {
    fn empty(w: usize, h: usize, nc: usize) -> Canvas {
        Canvas { w, h, nc, color: vec![0; w * h * nc], alpha: vec![0; w * h] }
    }
    fn rgb(&self, p: usize) -> Rgb {
        let c = |i: usize| self.color[p * self.nc + i.min(self.nc - 1)] as f32 / 255.0;
        [c(0), c(1), c(2)]
    }
    fn set(&mut self, p: usize, c: Rgb, a: f32) {
        for (i, channel) in c[..self.nc].iter().enumerate() {
            self.color[p * self.nc + i] = (channel * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
        self.alpha[p] = (a * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
}

/// A layer mask: the value at a pixel, or the default colour outside its rectangle.
struct Mask {
    rect: [i32; 4],
    default: u8,
    plane: Option<Vec<u16>>,
    /// "Invert mask when blending": the values inside the rectangle are 1 - value.
    invert: bool,
}

impl Mask {
    fn at(&self, x: usize, y: usize) -> f32 {
        let [top, left, bottom, right] = self.rect;
        let (x, y) = (x as i32, y as i32);
        if y >= top && y < bottom && x >= left && x < right
            && let Some(p) = &self.plane
        {
            let v = p[(y - top) as usize * (right - left) as usize + (x - left) as usize] as f32 / 65535.0;
            return if self.invert { 1.0 - v } else { v };
        }
        self.default as f32 / 255.0
    }
}

/// The pixels of one layer (planes of its rectangle).
struct Pixels {
    rect: [i32; 4],
    colors: Vec<Vec<u16>>,
    alpha: Option<Vec<u16>>,
}

struct Ctx<'a, 'b> {
    psd: &'a Psd<'b>,
    shown: &'a [bool],
    nc: usize,
}

pub(super) fn compose(psd: &Psd, nodes: &[Node], shown: &[bool]) -> Result<Canvas, String> {
    // 16-bit files are refused: InDesign drew an own 16-bit test file with its layers stretched sideways by about 12 % (objlayer-c5, unexplained)
    if psd.depth != 8 || !matches!(psd.mode, 1 | 3) || psd.w == 0 || psd.h == 0 || psd.w.checked_mul(psd.h).is_none_or(|n| n > 1 << 28) {
        return Err("only 8-bit RGB and grayscale PSD layers can be composed".into());
    }
    let nc = if psd.mode == 3 { 3 } else { 1 };
    let cx = Ctx { psd, shown, nc };
    let mut canvas = Canvas::empty(psd.w, psd.h, nc);
    cx.level(nodes, &mut canvas)?;
    Ok(canvas)
}

/// The layer's opacity and, apart, its fill when the mode uses it before clamping
/// (`Mode::fill_before_clamp`); otherwise both are one factor.
fn strength(rec: &Rec, mode: Mode) -> (f32, Option<f32>) {
    let (opacity, fill) = (rec.opacity as f32 / 255.0, rec.fill as f32 / 255.0);
    if mode.fill_before_clamp() && rec.fill < 255 { (opacity, Some(fill)) } else { (opacity * fill, None) }
}

impl Ctx<'_, '_> {
    fn rec(&self, node: &Node) -> &Rec {
        &self.psd.layers[node.rec()]
    }

    fn check(&self, rec: &Rec) -> Result<(), String> {
        match &rec.unsupported {
            Some(why) => Err(format!("PSD layer \"{}\" has {why}, which cannot be composed yet", rec.name)),
            None => Ok(()),
        }
    }

    fn mode(&self, rec: &Rec) -> Result<Mode, String> {
        let mode = Mode::from_key(&rec.blend).ok_or_else(|| format!("PSD layer \"{}\" uses the \"{}\" blend mode, which cannot be composed yet", rec.name, String::from_utf8_lossy(&rec.blend)))?;
        if self.nc == 1 && mode.mixes_channels() {
            return Err(format!("PSD layer \"{}\" mixes colour channels in a grayscale file, which cannot be composed yet", rec.name));
        }
        Ok(mode)
    }

    /// Layers of one level, bottom to top: a clipping group is a layer and the clipped layers above it.
    fn level(&self, nodes: &[Node], dst: &mut Canvas) -> Result<(), String> {
        let mut i = 0;
        while i < nodes.len() {
            let base = &nodes[i];
            i += 1;
            let first = i;
            while i < nodes.len() && self.rec(&nodes[i]).clip {
                i += 1;
            }
            let clipped = &nodes[first..i];
            if !self.shown[base.rec()] {
                continue; // a hidden base takes the layers clipped to it along
            }
            if clipped.is_empty() {
                self.node(base, dst)?;
            } else {
                self.clipping_group(base, clipped, dst)?;
            }
        }
        Ok(())
    }

    fn node(&self, node: &Node, dst: &mut Canvas) -> Result<(), String> {
        let rec = self.rec(node);
        self.check(rec)?;
        match node {
            Node::Layer(_) => {
                let mode = self.mode(rec)?;
                let (k, fill) = strength(rec, mode);
                self.paint_layer(rec, dst, mode, k, fill, false)
            }
            Node::Group(_, kids) => {
                let mask = self.mask(rec)?;
                if &rec.blend == b"pass" {
                    if rec.opacity == 255 && mask.is_none() {
                        return self.level(kids, dst);
                    }
                    let mut with = Canvas { w: dst.w, h: dst.h, nc: dst.nc, color: dst.color.clone(), alpha: dst.alpha.clone() };
                    self.level(kids, &mut with)?;
                    mix(dst, &with, mask.as_ref(), rec.opacity as f32 / 255.0);
                    return Ok(());
                }
                let mode = self.mode(rec)?;
                let mut own = Canvas::empty(dst.w, dst.h, dst.nc);
                self.level(kids, &mut own)?;
                let (k, fill) = strength(rec, mode);
                paint_canvas(dst, &own, mask.as_ref(), mode, k, fill);
                Ok(())
            }
        }
    }

    fn clipping_group(&self, base: &Node, clipped: &[Node], dst: &mut Canvas) -> Result<(), String> {
        let Node::Layer(_) = base else {
            return Err("PSD layers clipped to a layer group cannot be composed yet".into());
        };
        let rec = self.rec(base);
        self.check(rec)?;
        let mode = self.mode(rec)?;
        let mut own = Canvas::empty(dst.w, dst.h, dst.nc);
        self.paint_layer(rec, &mut own, Mode::Normal, 1.0, None, false)?;
        for node in clipped.iter().filter(|n| self.shown[n.rec()]) {
            let layer = self.rec(node);
            self.check(layer)?;
            let Node::Layer(_) = node else {
                return Err("a layer group clipped to a layer cannot be composed yet".into());
            };
            let layer_mode = self.mode(layer)?;
            let (k, fill) = strength(layer, layer_mode);
            self.paint_layer(layer, &mut own, layer_mode, k, fill, true)?;
        }
        let (k, fill) = strength(rec, mode);
        paint_canvas(dst, &own, None, mode, k, fill);
        Ok(())
    }

    fn mask(&self, rec: &Rec) -> Result<Option<Mask>, String> {
        let Some(m) = &rec.mask else { return Ok(None) };
        if m.flags & 2 != 0 {
            return Ok(None); // the mask is switched off
        }
        let invert = m.flags & 4 != 0;
        // an inverted mask is measured inside its rectangle only (InDesign, psdlayers-c1)
        if invert && !(m.rect[0] <= rec.rect[0] && m.rect[1] <= rec.rect[1] && m.rect[2] >= rec.rect[2] && m.rect[3] >= rec.rect[3]) {
            return Err(format!("PSD layer \"{}\" has an inverted mask that does not cover it, which cannot be composed yet", rec.name));
        }
        let [top, left, bottom, right] = m.rect;
        let (w, h) = ((right - left).max(0) as usize, (bottom - top).max(0) as usize);
        let channel = rec.channels.iter().find(|(c, _)| *c == -2);
        let plane = match channel {
            Some((_, range)) if w * h > 0 => Some(plane(&self.psd.b[range.clone()], w, h, self.psd.depth)?),
            _ => None,
        };
        Ok(Some(Mask { rect: m.rect, default: m.default, plane, invert }))
    }

    fn pixels(&self, rec: &Rec) -> Result<Pixels, String> {
        let [top, left, bottom, right] = rec.rect;
        let (w, h) = ((right - left).max(0) as usize, (bottom - top).max(0) as usize);
        let chan = |id: i16| -> Result<Option<Vec<u16>>, String> {
            rec.channels.iter().find(|(c, _)| *c == id).map(|(_, r)| plane(&self.psd.b[r.clone()], w, h, self.psd.depth)).transpose()
        };
        let colors = (0..self.nc as i16).map(|c| chan(c)?.ok_or_else(|| "PSD layer lacks a colour channel".to_string())).collect::<Result<Vec<_>, _>>()?;
        Ok(Pixels { rect: rec.rect, colors, alpha: chan(-1)? })
    }

    /// The vector mask of a layer as coverage over the canvas.
    fn vector(&self, rec: &Rec) -> Result<Option<Vec<f32>>, String> {
        let Some(data) = &rec.vmask else { return Ok(None) };
        let shape = super::vector::parse(data).map_err(|e| format!("PSD layer \"{}\" has {e}, which cannot be composed yet", rec.name))?;
        // measured (objlayer-c5): InDesign shows a layer beyond its own pixels where a larger vector mask lets it, in a way not understood yet
        if rec.adjust.is_none() {
            let (w, h) = (self.psd.w as f32, self.psd.h as f32);
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for e in &shape.edges {
                for p in e {
                    (x0, y0, x1, y1) = (x0.min(p.0 * w), y0.min(p.1 * h), x1.max(p.0 * w), y1.max(p.1 * h));
                }
            }
            let [top, left, bottom, right] = rec.rect;
            if shape.flags & super::vector::DISABLED == 0 && (x0 < left as f32 - 0.01 || y0 < top as f32 - 0.01 || x1 > right as f32 + 0.01 || y1 > bottom as f32 + 0.01) {
                return Err(format!("PSD layer \"{}\" has a vector mask larger than its pixels, which cannot be composed yet", rec.name));
            }
        }
        Ok(Some(shape.coverage(self.psd.w, self.psd.h)))
    }

    /// One pixel layer onto `dst`; `atop` keeps the canvas's own transparency (clipped layers).
    fn paint_layer(&self, rec: &Rec, dst: &mut Canvas, mode: Mode, k: f32, fill: Option<f32>, atop: bool) -> Result<(), String> {
        let mask = self.mask(rec)?;
        let vector = self.vector(rec)?;
        if let Some(adjust) = &rec.adjust {
            // no pixels of its own: the colour is made from what is below (an adjustment) or is one colour (a fill)
            for y in 0..dst.h {
                for x in 0..dst.w {
                    let p = y * dst.w + x;
                    let a = mask.as_ref().map_or(1.0, |m| m.at(x, y)) * vector.as_ref().map_or(1.0, |v| v[p]);
                    if a <= 0.0 || (!adjust.is_fill() && dst.alpha[p] == 0) {
                        continue;
                    }
                    let cs = adjust.apply(dst.rgb(p));
                    composite(dst, p, cs, a * k, mode, fill, atop);
                }
            }
            return Ok(());
        }
        let px = self.pixels(rec)?;
        let [top, left, bottom, right] = px.rect;
        let (lw, lh) = ((right - left).max(0) as usize, (bottom - top).max(0) as usize);
        if lw * lh == 0 {
            return Ok(());
        }
        let (x0, x1) = (left.max(0) as usize, (right.max(0) as usize).min(dst.w));
        let (y0, y1) = (top.max(0) as usize, (bottom.max(0) as usize).min(dst.h));
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y - top as usize) * lw + (x - left as usize);
                let la = px.alpha.as_ref().map_or(1.0, |a| a[i] as f32 / 65535.0);
                let a = la * mask.as_ref().map_or(1.0, |m| m.at(x, y)) * vector.as_ref().map_or(1.0, |v| v[y * dst.w + x]);
                let c = |n: usize| px.colors[n.min(self.nc - 1)][i] as f32 / 65535.0;
                composite(dst, y * dst.w + x, [c(0), c(1), c(2)], a * k, mode, fill, atop);
            }
        }
        Ok(())
    }
}

/// One source pixel (colour `cs`, alpha `a`) over the pixel `p` of `dst`.
fn composite(dst: &mut Canvas, p: usize, cs: Rgb, a: f32, mode: Mode, fill: Option<f32>, atop: bool) {
    if a <= 0.0 {
        return;
    }
    let ab = dst.alpha[p] as f32 / 255.0;
    if atop {
        if ab <= 0.0 {
            return;
        }
        // the base's transparency stays; the source mixes with the base's colour as if it were opaque
        let cb = dst.rgb(p);
        let b = blended(mode, cb, cs, fill);
        let mixed = [cb[0] + (b[0] - cb[0]) * a, cb[1] + (b[1] - cb[1]) * a, cb[2] + (b[2] - cb[2]) * a];
        dst.set(p, mixed, ab);
        return;
    }
    // over nothing the layer is its own colour; fill, used before clamping, still thins it out (not measured)
    if ab <= 0.0 {
        dst.set(p, cs, a * fill.unwrap_or(1.0));
        return;
    }
    let cb = dst.rgb(p);
    let ar = a + ab - a * ab;
    let b = blended(mode, cb, cs, fill);
    let t = a / ar;
    let mut out = [0.0; 3];
    for i in 0..3 {
        out[i] = (1.0 - t) * cb[i] + t * ((1.0 - ab) * cs[i] + ab * b[i]);
    }
    dst.set(p, out, ar);
}

fn blended(mode: Mode, cb: Rgb, cs: Rgb, fill: Option<f32>) -> Rgb {
    match fill {
        Some(f) => mode.blend_fill(cb, cs, f),
        None => mode.blend(cb, cs),
    }
}

/// A composed canvas (a group, a clipping group) onto `dst`.
fn paint_canvas(dst: &mut Canvas, src: &Canvas, mask: Option<&Mask>, mode: Mode, k: f32, fill: Option<f32>) {
    for y in 0..dst.h {
        for x in 0..dst.w {
            let p = y * dst.w + x;
            let a = src.alpha[p] as f32 / 255.0 * mask.map_or(1.0, |m| m.at(x, y));
            composite(dst, p, src.rgb(p), a * k, mode, fill, false);
        }
    }
}

/// A pass-through group of less than full strength: `dst` moves towards `with` (the canvas with
/// the group's layers on it) by opacity × mask.
fn mix(dst: &mut Canvas, with: &Canvas, mask: Option<&Mask>, opacity: f32) {
    for y in 0..dst.h {
        for x in 0..dst.w {
            let p = y * dst.w + x;
            let f = opacity * mask.map_or(1.0, |m| m.at(x, y));
            let (a0, a1) = (dst.alpha[p] as f32 / 255.0, with.alpha[p] as f32 / 255.0);
            let a = a0 + (a1 - a0) * f;
            if a <= 0.0 {
                dst.set(p, [0.0; 3], 0.0);
                continue;
            }
            let (c0, c1) = (dst.rgb(p), with.rgb(p));
            let mut out = [0.0; 3];
            for i in 0..3 {
                out[i] = (c0[i] * a0 * (1.0 - f) + c1[i] * a1 * f) / a;
            }
            dst.set(p, out, a);
        }
    }
}
