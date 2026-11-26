//! Sampled conversions and their interpolation.
//!
//! Display conversions are sampled once per document profile on regular grids (C-order,
//! last index fastest).
//!
//! - [`Link16`]: a conversion sampled and interpolated exactly as LittleCMS precalculates
//!   its transforms for 8- and 16-bit pixels, which is how the reference renderer (MuPDF)
//!   converts CMYK pixels and colours, and pixels composited in a CMYK group: nodes at the
//!   16-bit inputs round(i·65535/(N−1)) holding 16-bit outputs, white fixed to white,
//!   tetrahedral interpolation in 16.16 fixed point for three inputs, linear in the first
//!   channel between two tetrahedral evaluations for four. CMYK → RGB uses 17⁴ nodes and
//!   RGB → CMYK 33³, LittleCMS's grids for those spaces. [`Cmyk4`] agrees with LittleCMS's
//!   own CMYK → sRGB transform to 0.002 per channel on average.
//! - [`Rgb3`]: 8-bit RGB on 52³ nodes at multiples of 5 (so every node is an 8-bit input
//!   that can be evaluated exactly, e.g. through two 8-bit links), tetrahedral in 8.8 fixed
//!   point.

/// Nodes per axis of the CMYK → RGB link.
pub const CMYK_N: usize = 17;
/// Nodes per axis of the RGB → CMYK link.
pub const RGB_CMYK_N: usize = 33;
/// Nodes per axis of the RGB grid (node i is the 8-bit input 5·i).
pub const RGB_N: usize = 52;

/// Values in a CMYK → RGB table.
pub const CMYK_LEN: usize = CMYK_N * CMYK_N * CMYK_N * CMYK_N * 3;
/// Values in an RGB → CMYK link.
pub const RGB_CMYK_LEN: usize = RGB_CMYK_N * RGB_CMYK_N * RGB_CMYK_N * 4;
/// Bytes in an RGB grid.
pub const RGB_LEN: usize = RGB_N * RGB_N * RGB_N * 3;

/// Values of a matrix/TRC RGB conversion: three 256-level linearisation tables, a 3×3
/// matrix (row-major) and an offset.
pub const SHAPER_LEN: usize = 3 * 256 + 12;

/// Full scale of the 8.8 fixed-point channel inputs (255.0).
const FULL: u32 = 255 << 8;

/// A conversion of `IN` channels to `OUT` channels sampled as LittleCMS precalculates it,
/// on `N` nodes per axis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link16<const IN: usize, const OUT: usize, const N: usize> {
    pub(crate) data: Box<[u16]>,
}

/// CMYK ink → display RGB.
pub type Cmyk4 = Link16<4, 3, CMYK_N>;
/// RGB → CMYK ink.
pub type RgbCmyk = Link16<3, 4, RGB_CMYK_N>;

/// Input (0–1) of node `i` on an `n`-point axis, quantised to 16 bits as LittleCMS does.
#[inline]
fn node16(i: usize, n: usize) -> f64 {
    (i as f64 * 65535.0 / (n - 1) as f64 + 0.5).floor() / 65535.0
}

/// One axis of a cell: offset of the lower node, 16-bit fraction, step to the upper node
/// (0 at the top, where there is none).
#[inline(always)]
fn cell<const N: usize>(x: u16, stride: usize) -> (usize, i64, usize) {
    let v = x as u32 * (N as u32 - 1);
    let fixed = v + (v + 0x7FFF) / 0xFFFF;
    ((fixed >> 16) as usize * stride, (fixed & 0xFFFF) as i64, if x == 0xFFFF { 0 } else { stride })
}

impl<const IN: usize, const OUT: usize, const N: usize> Link16<IN, OUT, N> {
    const LEN: usize = N.pow(IN as u32) * OUT;

    /// Samples `f` (inputs 0–1 → outputs 0–1) on the nodes; `white` is the input node forced
    /// to `white_out` (LittleCMS's white fix-up).
    pub fn sample(f: &(dyn Fn([f64; IN]) -> [f64; OUT] + Sync), white: [usize; IN], white_out: [u16; OUT]) -> Self {
        let nodes = Self::LEN / OUT;
        let mut data = vec![0u16; Self::LEN].into_boxed_slice();
        let threads = std::thread::available_parallelism().map_or(4, |t| t.get()).clamp(1, 8);
        let per = nodes.div_ceil(threads).max(1);
        std::thread::scope(|s| {
            for (t, chunk) in data.chunks_mut(per * OUT).enumerate() {
                s.spawn(move || {
                    for (k, px) in chunk.chunks_exact_mut(OUT).enumerate() {
                        let mut rest = t * per + k;
                        let mut input = [0.0f64; IN];
                        for d in (0..IN).rev() {
                            input[d] = node16(rest % N, N);
                            rest /= N;
                        }
                        for (o, x) in px.iter_mut().zip(f(input)) {
                            *o = (x * 65535.0).round().clamp(0.0, 65535.0) as u16;
                        }
                    }
                });
            }
        });
        let at = white.iter().fold(0, |acc, &i| acc * N + i) * OUT;
        data[at..at + OUT].copy_from_slice(&white_out);
        Link16 { data }
    }

    pub fn from_box(data: Box<[u16]>) -> Option<Self> {
        (data.len() == Self::LEN).then_some(Link16 { data })
    }

    /// 16-bit inputs → 16-bit outputs.
    #[inline]
    pub fn eval16(&self, x: [u16; IN]) -> [u16; OUT] {
        // strides of the last three axes
        let (s2, s1, s0) = (OUT, OUT * N, OUT * N * N);
        let out = if IN == 4 {
            let (c0, rc, sc) = cell::<N>(x[0], OUT * N * N * N);
            let axes = [cell::<N>(x[1], s0), cell::<N>(x[2], s1), cell::<N>(x[3], s2)];
            let lo = self.tetra(c0, axes);
            if rc == 0 {
                lo
            } else {
                let hi = self.tetra(c0 + sc, axes);
                std::array::from_fn(|ch| lo[ch] + (((hi[ch] - lo[ch]) * rc + 0x8000) >> 16))
            }
        } else {
            self.tetra(0, [cell::<N>(x[0], s0), cell::<N>(x[1], s1), cell::<N>(x[2], s2)])
        };
        out.map(|v| v.clamp(0, 0xFFFF) as u16)
    }

    /// LittleCMS's 16-bit tetrahedral interpolation over three axes from `slice`.
    #[inline(always)]
    fn tetra(&self, slice: usize, axes: [(usize, i64, usize); 3]) -> [i64; OUT] {
        let [(x0, rx, sx), (y0, ry, sy), (z0, rz, sz)] = axes;
        let base = slice + x0 + y0 + z0;
        // The corners between the first and the last, walked in decreasing fraction order
        // (ties broken as LittleCMS breaks them), and the fractions in that order.
        let (p, q, f) = if rx >= ry && ry >= rz {
            (sx, sx + sy, [rx, ry, rz])
        } else if rx >= rz && rz >= ry {
            (sx, sx + sz, [rx, rz, ry])
        } else if rz >= rx && rx >= ry {
            (sz, sx + sz, [rz, rx, ry])
        } else if ry >= rx && rx >= rz {
            (sy, sx + sy, [ry, rx, rz])
        } else if ry >= rz && rz >= rx {
            (sy, sy + sz, [ry, rz, rx])
        } else {
            (sz, sy + sz, [rz, ry, rx])
        };
        let last = sx + sy + sz;
        let d = &self.data[base..base + last + OUT];
        std::array::from_fn(|ch| {
            let (v0, vp, vq, v3) = (d[ch] as i64, d[p + ch] as i64, d[q + ch] as i64, d[last + ch] as i64);
            let rest = (vp - v0) * f[0] + (vq - vp) * f[1] + (v3 - vq) * f[2];
            // C semantics: the division truncates, the shift floors.
            let scaled = rest + (rest + 0x7FFF) / 0xFFFF;
            v0 + ((scaled + 0x8000) >> 16)
        })
    }
}

impl Cmyk4 {
    /// Samples ink (0–1) → RGB (0–1) on the CMYK grid; no ink is white.
    pub fn sample_cmyk(f: &(dyn Fn([f64; 4]) -> [f64; 3] + Sync)) -> Cmyk4 {
        Link16::sample(f, [0; 4], [0xFFFF; 3])
    }

    /// 8-bit ink (0 = none, 255 = 100%).
    #[inline]
    pub fn eval8(&self, c: u8, m: u8, y: u8, k: u8) -> [u8; 3] {
        self.eval16([c, m, y, k].map(|v| v as u16 * 257)).map(to8)
    }

    /// Ink percentages (0–100).
    pub fn eval_percent(&self, c: f64, m: f64, y: f64, k: f64) -> [u8; 3] {
        self.eval16([c, m, y, k].map(|v| ((v / 100.0).clamp(0.0, 1.0) * 65535.0).round() as u16)).map(to8)
    }
}

/// CMYK ink → CMYK ink (a placed image's own profile → the output profile).
pub type CmykCmyk = Link16<4, 4, CMYK_N>;

impl CmykCmyk {
    /// Samples ink (0–1) → ink (0–1); unlike RGB → CMYK, no node is forced: the paper white
    /// is whatever the two profiles and the intent make of it.
    pub fn sample_ink(f: &(dyn Fn([f64; 4]) -> [f64; 4] + Sync)) -> CmykCmyk {
        let white = f([0.0; 4]).map(|x| (x * 65535.0).round().clamp(0.0, 65535.0) as u16);
        Link16::sample(f, [0; 4], white)
    }

    /// 8-bit ink → 8-bit ink (0 = none, 255 = 100 %).
    #[inline]
    pub fn eval8(&self, ink: [u8; 4]) -> [u8; 4] {
        self.eval16(ink.map(|v| v as u16 * 257)).map(to8)
    }
}

impl RgbCmyk {
    /// Samples RGB (0–1) → ink (0–1) on the RGB → CMYK grid; white is no ink.
    pub fn sample_rgb(f: &(dyn Fn([f64; 3]) -> [f64; 4] + Sync)) -> RgbCmyk {
        Link16::sample(f, [RGB_CMYK_N - 1; 3], [0; 4])
    }

    /// 8-bit RGB → 8-bit ink.
    #[inline]
    pub fn eval8(&self, rgb: [u8; 3]) -> [u8; 4] {
        self.eval16(rgb.map(|v| v as u16 * 257)).map(to8)
    }
}

/// 16-bit → 8-bit with rounding, as LittleCMS reduces its outputs.
#[inline]
pub fn to8(v: u16) -> u8 {
    ((v as u32 * 65281 + 8_388_608) >> 24) as u8
}

/// A sampled RGB → RGB conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgb3 {
    pub(crate) rgb: Box<[u8]>,
}

impl Rgb3 {
    /// Samples `f(rgb)` at the grid's 8-bit nodes.
    pub fn sample(f: &(dyn Fn([u8; 3]) -> [u8; 3] + Sync)) -> Rgb3 {
        let mut rgb = vec![0u8; RGB_LEN].into_boxed_slice();
        let n = RGB_N;
        let step = 255 / (n - 1);
        let nodes = n * n * n;
        let threads = std::thread::available_parallelism().map_or(4, |t| t.get()).clamp(1, 8);
        let per = nodes.div_ceil(threads).max(1);
        std::thread::scope(|s| {
            for (t, chunk) in rgb.chunks_mut(per * 3).enumerate() {
                s.spawn(move || {
                    for (k, px) in chunk.chunks_exact_mut(3).enumerate() {
                        let i = t * per + k;
                        let v = [i / (n * n), i / n % n, i % n].map(|c| (c * step) as u8);
                        px.copy_from_slice(&f(v));
                    }
                });
            }
        });
        Rgb3 { rgb }
    }

    pub fn from_box(rgb: Box<[u8]>) -> Option<Rgb3> {
        (rgb.len() == RGB_LEN).then_some(Rgb3 { rgb })
    }

    #[inline]
    pub fn eval(&self, rgb: [u8; 3]) -> [u8; 3] {
        const SB: usize = 3;
        const SG: usize = RGB_N * SB;
        const SR: usize = RGB_N * SG;
        let lut = &self.rgb[..RGB_LEN];
        let (ri, rt) = axis((rgb[0] as u32) << 8, RGB_N);
        let (gi, gt) = axis((rgb[1] as u32) << 8, RGB_N);
        let (bi, bt) = axis((rgb[2] as u32) << 8, RGB_N);
        let base = ri * SR + gi * SG + bi * SB;
        tetra(
            |dr, dg, db| {
                let o = base + dr * SR + dg * SG + db * SB;
                [lut[o] as i32, lut[o + 1] as i32, lut[o + 2] as i32]
            },
            [rt, gt, bt],
        )
        .map(|v| to_u8(v, 8))
    }
}

/// An RGB → display conversion through a matrix/TRC model, exact per pixel: each channel
/// linearised by table, one affine map to linear sRGB (the profile's matrix, black point
/// compensation and the sRGB matrix together), clipped and encoded. A sampled grid would
/// bend where the conversion clips out-of-gamut colours.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaper3 {
    pub(crate) v: Box<[f32]>,
}

impl Shaper3 {
    pub fn new(lin: &[[f64; 256]; 3], m: [[f64; 3]; 3], offset: [f64; 3]) -> Shaper3 {
        let mut v = Vec::with_capacity(SHAPER_LEN);
        v.extend(lin.iter().flatten().map(|&x| x as f32));
        v.extend(m.iter().flatten().map(|&x| x as f32));
        v.extend(offset.map(|x| x as f32));
        Shaper3 { v: v.into_boxed_slice() }
    }

    pub fn from_box(v: Box<[f32]>) -> Option<Shaper3> {
        (v.len() == SHAPER_LEN && v.iter().all(|x| x.is_finite())).then_some(Shaper3 { v })
    }

    #[inline]
    pub fn eval(&self, rgb: [u8; 3]) -> [u8; 3] {
        static ENCODE: std::sync::OnceLock<Box<[u8]>> = std::sync::OnceLock::new();
        let enc = ENCODE.get_or_init(|| {
            (0..=65535u32).map(|i| (crate::icc::srgb_encode(i as f64 / 65535.0) * 255.0).round().clamp(0.0, 255.0) as u8).collect()
        });
        let v = &self.v[..SHAPER_LEN];
        let l = [v[rgb[0] as usize], v[256 + rgb[1] as usize], v[512 + rgb[2] as usize]];
        let (m, o) = (&v[768..777], &v[777..780]);
        std::array::from_fn(|r| {
            let x = m[3 * r] * l[0] + m[3 * r + 1] * l[1] + m[3 * r + 2] * l[2] + o[r];
            enc[(x.clamp(0.0, 1.0) * 65535.0 + 0.5) as usize]
        })
    }
}

/// Document RGB → display: exact for matrix/TRC profiles, sampled otherwise.
#[derive(Clone, Debug, PartialEq)]
pub enum RgbDisplay {
    Shaper(Shaper3),
    Grid(Rgb3),
}

impl RgbDisplay {
    #[inline]
    pub fn eval(&self, rgb: [u8; 3]) -> [u8; 3] {
        match self {
            RgbDisplay::Shaper(s) => s.eval(rgb),
            RgbDisplay::Grid(g) => g.eval(rgb),
        }
    }
}

/// Grid cell and weight (0–256) of a channel in 8.8 fixed point on an `n`-point axis.
#[inline]
fn axis(x: u32, n: usize) -> (usize, i32) {
    let x = x.min(FULL);
    let num = x * (n as u32 - 1);
    let (mut i, mut rem) = ((num / FULL) as usize, num % FULL);
    if i >= n - 1 {
        i = n - 2;
        rem = FULL;
    }
    (i, (rem * 256 / FULL) as i32)
}

/// Tetrahedral interpolation in a 3-D cell. `at(dx, dy, dz)` reads a corner; weights are
/// 0–256. Returns channel values × 256.
#[inline]
fn tetra(at: impl Fn(usize, usize, usize) -> [i32; 3], t: [i32; 3]) -> [i32; 3] {
    let [a, b, c] = t;
    let c000 = at(0, 0, 0);
    let c111 = at(1, 1, 1);
    // The tetrahedron is chosen by the order of the fractions; walk from c000 to c111
    // along the axes in decreasing fraction order.
    let (w, p, q) = if a >= b {
        if b >= c {
            ([256 - a, a - b, b - c, c], at(1, 0, 0), at(1, 1, 0))
        } else if a >= c {
            ([256 - a, a - c, c - b, b], at(1, 0, 0), at(1, 0, 1))
        } else {
            ([256 - c, c - a, a - b, b], at(0, 0, 1), at(1, 0, 1))
        }
    } else if c > b {
        ([256 - c, c - b, b - a, a], at(0, 0, 1), at(0, 1, 1))
    } else if c > a {
        ([256 - b, b - c, c - a, a], at(0, 1, 0), at(0, 1, 1))
    } else {
        ([256 - b, b - a, a - c, c], at(0, 1, 0), at(1, 1, 0))
    };
    let mut out = [0i32; 3];
    for (k, o) in out.iter_mut().enumerate() {
        *o = c000[k] * w[0] + p[k] * w[1] + q[k] * w[2] + c111[k] * w[3];
    }
    out
}

#[inline]
fn to_u8(v: i32, scale_bits: u32) -> u8 {
    let half = 1 << (scale_bits - 1);
    ((v + half) >> scale_bits).clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_grid_is_exact_at_nodes_and_close_between() {
        let id = Rgb3::sample(&|v| v);
        for v in [0u8, 8, 100, 128, 200, 255] {
            let o = id.eval([v, 255 - v, v / 2]);
            for (a, b) in o.iter().zip([v, 255 - v, v / 2]) {
                assert!((*a as i32 - b as i32).abs() <= 1, "{v}: {o:?}");
            }
        }
        // Node order: last index fastest; node 1 is input 5.
        let n = RGB_N;
        assert_eq!(&id.rgb[3 * (n * n)..3 * (n * n) + 3], &[5, 0, 0]);
        assert_eq!(id.eval([5, 250, 255]), [5, 250, 255]);
    }

    #[test]
    fn links_interpolate_linear_tables_exactly() {
        let k = Cmyk4::sample(&|v| [v[0], v[1] * 0.5 + v[2] * 0.5, v[3]], [0; 4], [0; 3]);
        assert_eq!(k.eval8(255, 0, 0, 128), [255, 0, 128]);
        assert_eq!(k.eval8(17, 100, 50, 3), [17, 75, 3]);
        assert_eq!(k.eval16([65535; 4]), [65535; 3]);
        let r = RgbCmyk::sample_rgb(&|v| [1.0 - v[0], 1.0 - v[1], 1.0 - v[2], 0.25]);
        assert_eq!(r.eval8([10, 128, 250]), [245, 127, 5, 64]);
        // White fix-up: white is no ink.
        assert_eq!(r.eval8([255, 255, 255]), [0, 0, 0, 0]);
        let c = Cmyk4::sample_cmyk(&|v| [1.0 - v[0], 1.0 - v[1], 1.0 - v[2]]);
        assert_eq!(c.eval8(0, 0, 0, 0), [255, 255, 255]);
    }
}
