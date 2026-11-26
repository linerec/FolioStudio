//! Blend modes of a Photoshop layer (the key in the layer record) on 0..1 colour values.
//! Dissolve (random) and Hard Mix (InDesign's result is not the 0/255 threshold; psdlayers-c1) are
//! not offered; pass-through is a property of groups only.
use super::Rgb;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    ColorDodge,
    ColorBurn,
    Darken,
    Lighten,
    Difference,
    Exclusion,
    LinearBurn,
    LinearDodge,
    VividLight,
    LinearLight,
    PinLight,
    Hue,
    Saturation,
    Color,
    Luminosity,
    DarkerColor,
    LighterColor,
    Subtract,
    Divide,
}

impl Mode {
    pub fn from_key(key: &[u8]) -> Option<Mode> {
        use Mode::*;
        Some(match key {
            b"norm" => Normal,
            b"mul " => Multiply,
            b"scrn" => Screen,
            b"over" => Overlay,
            b"sLit" => SoftLight,
            b"hLit" => HardLight,
            b"div " => ColorDodge,
            b"idiv" => ColorBurn,
            b"dark" => Darken,
            b"lite" => Lighten,
            b"diff" => Difference,
            b"smud" => Exclusion,
            b"lbrn" => LinearBurn,
            b"lddg" => LinearDodge,
            b"vLit" => VividLight,
            b"lLit" => LinearLight,
            b"pLit" => PinLight,
            b"hue " => Hue,
            b"sat " => Saturation,
            b"colr" => Color,
            b"lum " => Luminosity,
            b"dkCl" => DarkerColor,
            b"lgCl" => LighterColor,
            b"fsub" => Subtract,
            b"fdiv" => Divide,
            _ => return None,
        })
    }

    /// Modes that mix the three colour channels (they need an RGB image).
    pub fn mixes_channels(self) -> bool {
        use Mode::*;
        matches!(self, Hue | Saturation | Color | Luminosity | DarkerColor | LighterColor)
    }

    /// Modes for which the layer's fill opacity acts before the result is clamped: the layer
    /// adds `fill` × its effect to the backdrop (measured, psdlayers-c1).
    pub fn fill_before_clamp(self) -> bool {
        use Mode::*;
        matches!(self, ColorDodge | ColorBurn | LinearBurn | LinearDodge | VividLight | LinearLight)
    }

    /// The blend of `source` over `backdrop` with fill `f` in the modes of `fill_before_clamp`.
    pub fn blend_fill(self, cb: Rgb, cs: Rgb, f: f32) -> Rgb {
        let ch = |b: f32, s: f32| (b + f * (self.unclamped(b, s) - b)).clamp(0.0, 1.0);
        [ch(cb[0], cs[0]), ch(cb[1], cs[1]), ch(cb[2], cs[2])]
    }

    fn unclamped(self, b: f32, s: f32) -> f32 {
        use Mode::*;
        const BIG: f32 = 1.0e4;
        let dodge = |b: f32, s: f32| if b <= 0.0 { 0.0 } else if s >= 1.0 { BIG } else { b / (1.0 - s) };
        let burn = |b: f32, s: f32| if b >= 1.0 { 1.0 } else if s <= 0.0 { -BIG } else { 1.0 - (1.0 - b) / s };
        match self {
            ColorDodge => dodge(b, s),
            ColorBurn => burn(b, s),
            LinearBurn => b + s - 1.0,
            LinearDodge => b + s,
            VividLight => if s < 0.5 { burn(b, 2.0 * s) } else { dodge(b, 2.0 * (s - 0.5)) },
            LinearLight => b + 2.0 * s - 1.0,
            _ => self.channel(b, s),
        }
    }

    /// The colour of `source` blended with `backdrop`, both 0..1 per channel.
    pub fn blend(self, cb: Rgb, cs: Rgb) -> Rgb {
        use Mode::*;
        match self {
            Normal => cs,
            Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
            Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
            Color => set_lum(cs, lum(cb)),
            Luminosity => set_lum(cb, lum(cs)),
            DarkerColor => {
                if lum(cs) < lum(cb) { cs } else { cb }
            }
            LighterColor => {
                if lum(cs) > lum(cb) { cs } else { cb }
            }
            _ => [self.channel(cb[0], cs[0]), self.channel(cb[1], cs[1]), self.channel(cb[2], cs[2])],
        }
    }

    fn channel(self, b: f32, s: f32) -> f32 {
        use Mode::*;
        let v = match self {
            Multiply => b * s,
            Screen => b + s - b * s,
            Overlay => hard_light(s, b),
            SoftLight => soft_light(b, s),
            HardLight => hard_light(b, s),
            ColorDodge => color_dodge(b, s),
            ColorBurn => color_burn(b, s),
            Darken => b.min(s),
            Lighten => b.max(s),
            Difference => (b - s).abs(),
            Exclusion => b + s - 2.0 * b * s,
            LinearBurn => b + s - 1.0,
            LinearDodge => b + s,
            VividLight => {
                if s < 0.5 { color_burn(b, 2.0 * s) } else { color_dodge(b, 2.0 * (s - 0.5)) }
            }
            LinearLight => b + 2.0 * s - 1.0,
            PinLight => {
                if s < 0.5 { b.min(2.0 * s) } else { b.max(2.0 * s - 1.0) }
            }
            Subtract => b - s,
            Divide => {
                if s <= 0.0 { 1.0 } else { b / s }
            }
            Normal | Hue | Saturation | Color | Luminosity | DarkerColor | LighterColor => s,
        };
        v.clamp(0.0, 1.0)
    }
}

fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 { b * 2.0 * s } else { let t = 2.0 * s - 1.0; b + t - b * t }
}

fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 { 0.0 } else if s >= 1.0 { 1.0 } else { (b / (1.0 - s)).min(1.0) }
}

fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 { 1.0 } else if s <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / s).min(1.0) }
}

fn lum(c: Rgb) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn sat(c: Rgb) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn clip_color(c: Rgb) -> Rgb {
    let l = lum(c);
    let (n, x) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    let mut c = c;
    if n < 0.0 {
        for v in &mut c {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1.0 {
        for v in &mut c {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    c
}

fn set_lum(c: Rgb, l: f32) -> Rgb {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

fn set_sat(c: Rgb, s: f32) -> Rgb {
    // order the channel indices by value: hi >= mid >= lo
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|a, b| c[*b].partial_cmp(&c[*a]).unwrap_or(std::cmp::Ordering::Equal));
    let (hi, mid, lo) = (idx[0], idx[1], idx[2]);
    let mut out = [0.0; 3];
    if c[hi] > c[lo] {
        out[mid] = (c[mid] - c[lo]) * s / (c[hi] - c[lo]);
        out[hi] = s;
    }
    out
}
