//! ICC profile evaluation in f64: device ↔ PCS (relative colorimetric), black point
//! compensation and the display encoding (sRGB).
//!
//! Profiles are parsed by moxcms (tags, LUT and curve data); evaluation is ours, because
//! moxcms resamples CMYK pipelines on a coarse grid and has no black point compensation.
//! The conventions follow ICC.1 and LittleCMS 2, the engine the reference PDF renderer
//! (MuPDF) uses:
//!
//! - Tag choice per intent: A2B1/B2A1 for relative colorimetric, falling back to the
//!   A2B0/B2A0 default and then to the matrix/TRC model.
//! - Multidimensional tables: tetrahedral interpolation for three inputs; for four inputs
//!   linear in the first channel between two tetrahedral evaluations of the other three.
//!   PCS → device tables of Lab-based profiles interpolate trilinearly (Lab is not
//!   uniform enough for tetrahedra; LittleCMS does the same).
//! - PCS Lab encoding: legacy 16-bit (L 0xFF00 = 100) in `lut16Type`, ICC v4 in `lut8Type`
//!   and `lutAtoBType`/`lutBtoAType`; PCS XYZ is u1Fixed15 (0x8000 = 1.0).
//! - Black point compensation (ISO 18619, Adobe's published algorithm): see
//!   [`source_black_point`], [`destination_black_point`] and [`Bpc`].
//! - Display: sRGB IEC 61966-2.1 primaries and D65 white adapted to the D50 PCS with the
//!   Bradford transform, as LittleCMS builds its sRGB profile.

use moxcms::{ColorProfile, DataColorSpace, LutStore, LutType, LutWarehouse, ProfileClass, ToneReprCurve};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Intent {
    Perceptual,
    #[default]
    Relative,
    Saturation,
    Absolute,
}

impl Intent {
    pub fn from_name(name: &str) -> Self {
        match name {
            "Perceptual" => Self::Perceptual,
            "Saturation" => Self::Saturation,
            "AbsoluteColorimetric" => Self::Absolute,
            _ => Self::Relative,
        }
    }
}

/// PCS white (D50) as LittleCMS and ICC.1 use it.
pub const D50: [f64; 3] = [0.9642, 1.0, 0.8249];

type M3 = [[f64; 3]; 3];

/// Why a profile cannot be used for display conversion.
#[derive(Debug, Clone, PartialEq)]
pub enum IccError {
    /// Not a well-formed ICC profile.
    Parse(String),
    /// A well-formed profile this converter does not handle (named here).
    Unsupported(String),
}

impl fmt::Display for IccError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IccError::Parse(m) => write!(f, "not a usable ICC profile ({m})"),
            IccError::Unsupported(m) => write!(f, "unsupported profile: {m}"),
        }
    }
}

impl std::error::Error for IccError {}

/// Device colour space of a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Gray,
    Rgb,
    Cmyk,
}

impl Space {
    pub fn channels(self) -> usize {
        match self {
            Space::Gray => 1,
            Space::Rgb => 3,
            Space::Cmyk => 4,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Curves and tables

#[derive(Clone, Debug)]
enum Curve {
    Identity,
    Gamma(f64),
    /// Samples spanning [0, 1], values normalised to [0, 1].
    Table(Vec<f64>),
    /// ICC parametricCurveType parameters g, a, b, c, d, e, f (absent ones per type).
    Param(Vec<f64>),
}

impl Curve {
    fn from_icc(c: &ToneReprCurve) -> Result<Curve, IccError> {
        Ok(match c {
            ToneReprCurve::Lut(t) if t.is_empty() => Curve::Identity,
            ToneReprCurve::Lut(t) if t.len() == 1 => Curve::Gamma(t[0] as f64 / 256.0),
            ToneReprCurve::Lut(t) => Curve::Table(t.iter().map(|&v| v as f64 / 65535.0).collect()),
            ToneReprCurve::Parametric(p) if matches!(p.len(), 1 | 3 | 4 | 5 | 7) => Curve::Param(p.iter().map(|&v| v as f64).collect()),
            ToneReprCurve::Parametric(p) => return Err(IccError::Unsupported(format!("parametric curve with {} parameters", p.len()))),
        })
    }

    fn from_store(store: &LutStore, channel: usize, entries: usize) -> Curve {
        let t: Vec<f64> = match store {
            LutStore::Store8(v) => v[channel * entries..(channel + 1) * entries].iter().map(|&x| x as f64 / 255.0).collect(),
            LutStore::Store16(v) => v[channel * entries..(channel + 1) * entries].iter().map(|&x| x as f64 / 65535.0).collect(),
        };
        Curve::Table(t)
    }

    /// The curve at `x` (clamped to [0, 1]).
    fn eval(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Curve::Identity => x,
            Curve::Gamma(g) => x.powf(*g),
            Curve::Table(t) => interp1(t, x),
            Curve::Param(p) => {
                let pw = |v: f64| v.max(0.0).powf(p[0]);
                match p.len() {
                    1 => pw(x),
                    3 => {
                        let (a, b) = (p[1], p[2]);
                        if x >= -b / a { pw(a * x + b) } else { 0.0 }
                    }
                    4 => {
                        let (a, b, c) = (p[1], p[2], p[3]);
                        if x >= -b / a { pw(a * x + b) + c } else { c }
                    }
                    5 => {
                        let (a, b, c, d) = (p[1], p[2], p[3], p[4]);
                        if x >= d { pw(a * x + b) } else { c * x }
                    }
                    _ => {
                        let (a, b, c, d, e, f) = (p[1], p[2], p[3], p[4], p[5], p[6]);
                        if x >= d { pw(a * x + b) + e } else { c * x + f }
                    }
                }
            }
        }
    }
}

/// Linear interpolation in samples spanning [0, 1].
fn interp1(t: &[f64], x: f64) -> f64 {
    match t.len() {
        0 => x,
        1 => t[0],
        n => {
            let p = x.clamp(0.0, 1.0) * (n - 1) as f64;
            let i = (p.floor() as usize).min(n - 2);
            let f = p - i as f64;
            t[i] + (t[i + 1] - t[i]) * f
        }
    }
}

/// A multidimensional table (first input varies slowest), values normalised to [0, 1].
#[derive(Clone, Debug)]
struct Clut {
    grid: Vec<usize>,
    strides: Vec<usize>,
    outputs: usize,
    data: Vec<f64>,
    /// Multilinear instead of tetrahedral interpolation.
    trilinear: bool,
}

impl Clut {
    fn new(grid: Vec<usize>, outputs: usize, store: &LutStore) -> Result<Clut, IccError> {
        if grid.is_empty() || grid.contains(&0) || outputs == 0 {
            return Err(IccError::Parse("empty colour table".into()));
        }
        let mut strides = vec![outputs; grid.len()];
        for i in (0..grid.len() - 1).rev() {
            strides[i] = strides[i + 1] * grid[i + 1];
        }
        let len = strides[0] * grid[0];
        let data: Vec<f64> = match store {
            LutStore::Store8(v) => v.iter().take(len).map(|&x| x as f64 / 255.0).collect(),
            LutStore::Store16(v) => v.iter().take(len).map(|&x| x as f64 / 65535.0).collect(),
        };
        if data.len() < len {
            return Err(IccError::Parse("colour table shorter than its grid".into()));
        }
        Ok(Clut { grid, strides, outputs, data, trilinear: false })
    }

    /// Cell index and fraction of input `x` on axis `d`.
    fn cell(&self, d: usize, x: f64) -> (usize, f64) {
        let g = self.grid[d];
        if g < 2 {
            return (0, 0.0);
        }
        let p = x.clamp(0.0, 1.0) * (g - 1) as f64;
        let i = (p.floor() as usize).min(g - 2);
        (i, p - i as f64)
    }

    /// Tetrahedral interpolation over three axes from `base`: walk from the cell's first
    /// corner to its opposite one along the axes in decreasing fraction order.
    fn tetra(&self, base: usize, axes: [usize; 3], x: [f64; 3], out: &mut [f64]) {
        let mut idx = [(0usize, 0.0f64, 0usize); 3];
        let mut offset = base;
        for k in 0..3 {
            let d = axes[k];
            let (i, f) = self.cell(d, x[k]);
            offset += i * self.strides[d];
            let step = if self.grid[d] < 2 { 0 } else { self.strides[d] };
            idx[k] = (step, f, d);
        }
        idx.sort_by(|a, b| b.1.total_cmp(&a.1));
        let n = self.outputs;
        let v0 = offset;
        let v1 = v0 + idx[0].0;
        let v2 = v1 + idx[1].0;
        let v3 = v2 + idx[2].0;
        let (f0, f1, f2) = (idx[0].1, idx[1].1, idx[2].1);
        let d = &self.data;
        for o in 0..n {
            let (c0, c1, c2, c3) = (d[v0 + o], d[v1 + o], d[v2 + o], d[v3 + o]);
            out[o] = c0 + f0 * (c1 - c0) + f1 * (c2 - c1) + f2 * (c3 - c2);
        }
    }

    fn eval(&self, x: &[f64], out: &mut [f64]) {
        if self.trilinear {
            return self.multilinear(x, out);
        }
        match self.grid.len() {
            3 => self.tetra(0, [0, 1, 2], [x[0], x[1], x[2]], out),
            4 => {
                let (i, f) = self.cell(0, x[0]);
                let base = i * self.strides[0];
                self.tetra(base, [1, 2, 3], [x[1], x[2], x[3]], out);
                if f > 0.0 && self.grid[0] >= 2 {
                    let mut hi = [0.0f64; 16];
                    self.tetra(base + self.strides[0], [1, 2, 3], [x[1], x[2], x[3]], &mut hi);
                    for (o, h) in out.iter_mut().zip(&hi).take(self.outputs) {
                        *o += f * (h - *o);
                    }
                }
            }
            _ => self.multilinear(x, out),
        }
    }

    /// Multilinear interpolation (1, 2 or more than 4 inputs).
    fn multilinear(&self, x: &[f64], out: &mut [f64]) {
        let dims = self.grid.len().min(16);
        let mut cells = [(0usize, 0.0f64); 16];
        for (d, c) in cells.iter_mut().enumerate().take(dims) {
            *c = self.cell(d, x[d]);
        }
        let cells = &cells[..dims];
        out[..self.outputs].fill(0.0);
        for corner in 0..(1usize << dims) {
            let mut w = 1.0;
            let mut offset = 0;
            for (d, &(i, f)) in cells.iter().enumerate() {
                let up = corner >> (dims - 1 - d) & 1 == 1;
                if up && self.grid[d] < 2 {
                    w = 0.0;
                    break;
                }
                w *= if up { f } else { 1.0 - f };
                offset += (i + up as usize) * self.strides[d];
            }
            if w == 0.0 {
                continue;
            }
            for (o, v) in out[..self.outputs].iter_mut().zip(&self.data[offset..offset + self.outputs]) {
                *o += w * v;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// PCS encodings

/// How a table encodes the PCS side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pcs {
    /// Lab, legacy 16-bit encoding of `lut16Type`.
    LabLegacy,
    /// Lab, ICC v4 encoding (and the 8-bit encoding, which is the same).
    LabV4,
    /// XYZ, u1Fixed15.
    Xyz,
}

const XYZ_SCALE: f64 = 65535.0 / 32768.0;

impl Pcs {
    fn decode(self, v: [f64; 3]) -> [f64; 3] {
        match self {
            Pcs::Xyz => v.map(|c| c * XYZ_SCALE),
            Pcs::LabLegacy => {
                let s = 65535.0 / 65280.0;
                lab_to_xyz([v[0] * s * 100.0, v[1] * 65535.0 / 256.0 - 128.0, v[2] * 65535.0 / 256.0 - 128.0])
            }
            Pcs::LabV4 => lab_to_xyz([v[0] * 100.0, v[1] * 255.0 - 128.0, v[2] * 255.0 - 128.0]),
        }
    }

    fn encode(self, xyz: [f64; 3]) -> [f64; 3] {
        let v = match self {
            Pcs::Xyz => xyz.map(|c| c / XYZ_SCALE),
            Pcs::LabLegacy => {
                let [l, a, b] = xyz_to_lab(xyz);
                [l / 100.0 * 65280.0 / 65535.0, (a + 128.0) * 256.0 / 65535.0, (b + 128.0) * 256.0 / 65535.0]
            }
            Pcs::LabV4 => {
                let [l, a, b] = xyz_to_lab(xyz);
                [l / 100.0, (a + 128.0) / 255.0, (b + 128.0) / 255.0]
            }
        };
        v.map(|c| c.clamp(0.0, 1.0))
    }
}

/// CIE L*a*b* (D50) → XYZ (D50, white Y = 1).
pub fn lab_to_xyz(lab: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = lab;
    let fy = (l + 16.0) / 116.0;
    let inv = |t: f64| {
        let d = 6.0 / 29.0;
        if t > d { t * t * t } else { 3.0 * d * d * (t - 4.0 / 29.0) }
    };
    [inv(fy + a / 500.0) * D50[0], inv(fy) * D50[1], inv(fy - b / 200.0) * D50[2]]
}

/// XYZ (D50) → CIE L*a*b* (D50).
pub fn xyz_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    let f = |t: f64| {
        let d = 6.0 / 29.0;
        if t > d * d * d { t.cbrt() } else { t / (3.0 * d * d) + 4.0 / 29.0 }
    };
    let (fx, fy, fz) = (f(xyz[0] / D50[0]), f(xyz[1] / D50[1]), f(xyz[2] / D50[2]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

// ---------------------------------------------------------------------------------------
// Pipelines

#[derive(Clone, Debug)]
enum Stages {
    /// `lut8Type` / `lut16Type`: [matrix, PCS XYZ input only] → input curves → table →
    /// output curves.
    Mft { matrix: Option<M3>, input: Vec<Curve>, clut: Clut, output: Vec<Curve> },
    /// `lutAtoBType`: A curves → table → M curves → matrix → B curves.
    AToB { a: Vec<Curve>, clut: Option<Clut>, m: Vec<Curve>, matrix: Option<(M3, [f64; 3])>, b: Vec<Curve> },
    /// `lutBtoAType`: B curves → matrix → M curves → table → A curves.
    BToA { b: Vec<Curve>, matrix: Option<(M3, [f64; 3])>, m: Vec<Curve>, clut: Option<Clut>, a: Vec<Curve> },
    /// Matrix/TRC model, device → PCS XYZ (colorants as columns).
    Shaper { curves: Vec<Curve>, matrix: M3 },
}

#[derive(Clone, Debug)]
struct Pipeline {
    inputs: usize,
    outputs: usize,
    pcs: Pcs,
    stages: Stages,
}

fn apply_curves(curves: &[Curve], v: &mut [f64]) {
    for (c, x) in curves.iter().zip(v.iter_mut()) {
        *x = c.eval(*x);
    }
}

fn mul(m: &M3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

/// The inverse of a 3x3 matrix (`None` when singular).
fn invert3(m: &M3) -> Option<M3> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let c = |r0: usize, c0: usize, r1: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    Some([
        [c(1, 1, 2, 2) / det, -c(0, 1, 2, 2) / det, c(0, 1, 1, 2) / det],
        [-c(1, 0, 2, 2) / det, c(0, 0, 2, 2) / det, -c(0, 0, 1, 2) / det],
        [c(1, 0, 2, 1) / det, -c(0, 0, 2, 1) / det, c(0, 0, 1, 1) / det],
    ])
}

fn is_identity(m: &M3) -> bool {
    (0..3).all(|r| (0..3).all(|c| (m[r][c] - if r == c { 1.0 } else { 0.0 }).abs() < 1e-9))
}

impl Pipeline {
    fn from_lut(lut: &LutWarehouse, pcs_lab: bool, to_pcs: bool) -> Result<Pipeline, IccError> {
        match lut {
            LutWarehouse::Lut(l) => {
                let (nin, nout) = (l.num_input_channels as usize, l.num_output_channels as usize);
                let pcs = match (pcs_lab, l.lut_type) {
                    (false, _) => Pcs::Xyz,
                    (true, LutType::Lut16) => Pcs::LabLegacy,
                    (true, _) => Pcs::LabV4,
                };
                let ni = l.num_input_table_entries as usize;
                let no = l.num_output_table_entries as usize;
                let len = |s: &LutStore| match s {
                    LutStore::Store8(v) => v.len(),
                    LutStore::Store16(v) => v.len(),
                };
                if nin == 0 || nout == 0 || len(&l.input_table) < ni * nin || len(&l.output_table) < no * nout {
                    return Err(IccError::Parse("truncated lut tables".into()));
                }
                let input = (0..nin).map(|c| Curve::from_store(&l.input_table, c, ni)).collect();
                let output = (0..nout).map(|c| Curve::from_store(&l.output_table, c, no)).collect();
                let mut clut = Clut::new(vec![l.num_clut_grid_points as usize; nin], nout, &l.clut_table)?;
                clut.trilinear = pcs_lab && !to_pcs;
                // The matrix applies only to XYZ input (PCS side of a PCS → device table).
                let matrix = (!to_pcs && pcs == Pcs::Xyz && !is_identity(&l.matrix.v)).then_some(l.matrix.v);
                Ok(Pipeline { inputs: nin, outputs: nout, pcs, stages: Stages::Mft { matrix, input, clut, output } })
            }
            LutWarehouse::Multidimensional(m) => {
                let (nin, nout) = (m.num_input_channels as usize, m.num_output_channels as usize);
                let pcs = if pcs_lab { Pcs::LabV4 } else { Pcs::Xyz };
                let curves = |v: &[ToneReprCurve]| v.iter().map(Curve::from_icc).collect::<Result<Vec<_>, _>>();
                let (a, b, mc) = (curves(&m.a_curves)?, curves(&m.b_curves)?, curves(&m.m_curves)?);
                if nin > 16 || nout > 16 {
                    return Err(IccError::Unsupported(format!("table with {nin} inputs and {nout} outputs")));
                }
                // The table maps the tag's inputs to its outputs in both directions.
                let clut = match &m.clut {
                    Some(store) => {
                        let mut c = Clut::new(m.grid_points[..nin].iter().map(|&g| g as usize).collect(), nout, store)?;
                        c.trilinear = pcs_lab && !to_pcs;
                        Some(c)
                    }
                    None if nin == nout => None,
                    None => return Err(IccError::Unsupported("table without CLUT changes the channel count".into())),
                };
                let matrix = (!mc.is_empty() || !is_identity(&m.matrix.v) || m.bias.v.iter().any(|&x| x != 0.0))
                    .then_some((m.matrix.v, m.bias.v));
                let stages = if to_pcs {
                    Stages::AToB { a, clut, m: mc, matrix, b }
                } else {
                    Stages::BToA { b, matrix, m: mc, clut, a }
                };
                Ok(Pipeline { inputs: nin, outputs: nout, pcs, stages })
            }
        }
    }

    /// Evaluates normalised inputs; returns normalised outputs (up to 16 channels).
    fn eval(&self, input: &[f64]) -> [f64; 16] {
        let mut v = [0.0f64; 16];
        v[..self.inputs].copy_from_slice(&input[..self.inputs]);
        let mut out = [0.0f64; 16];
        match &self.stages {
            Stages::Mft { matrix, input: curves_in, clut, output } => {
                if let Some(m) = matrix {
                    let r = mul(m, [v[0], v[1], v[2]]);
                    v[..3].copy_from_slice(&r.map(|c| c.clamp(0.0, 1.0)));
                }
                apply_curves(curves_in, &mut v[..self.inputs]);
                clut.eval(&v[..self.inputs], &mut out);
                apply_curves(output, &mut out[..self.outputs]);
            }
            Stages::AToB { a, clut, m, matrix, b } => {
                apply_curves(a, &mut v[..self.inputs]);
                match clut {
                    Some(c) => c.eval(&v[..self.inputs], &mut out),
                    None => out[..self.inputs].copy_from_slice(&v[..self.inputs]),
                }
                apply_curves(m, &mut out[..3]);
                if let Some((mx, bias)) = matrix {
                    let r = mul(mx, [out[0], out[1], out[2]]);
                    for k in 0..3 {
                        out[k] = (r[k] + bias[k]).clamp(0.0, 1.0);
                    }
                }
                apply_curves(b, &mut out[..3]);
            }
            Stages::BToA { b, matrix, m, clut, a } => {
                apply_curves(b, &mut v[..3]);
                if let Some((mx, bias)) = matrix {
                    let r = mul(mx, [v[0], v[1], v[2]]);
                    for k in 0..3 {
                        v[k] = (r[k] + bias[k]).clamp(0.0, 1.0);
                    }
                }
                apply_curves(m, &mut v[..3]);
                match clut {
                    Some(c) => c.eval(&v[..self.inputs], &mut out),
                    None => out[..self.inputs].copy_from_slice(&v[..self.inputs]),
                }
                apply_curves(a, &mut out[..self.outputs]);
            }
            Stages::Shaper { curves, matrix } => {
                apply_curves(curves, &mut v[..self.inputs]);
                let lin = if self.inputs == 1 { [v[0]; 3] } else { [v[0], v[1], v[2]] };
                let r = mul(matrix, lin);
                // Shaper output is XYZ itself, not an encoding.
                out[..3].copy_from_slice(&r);
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------------------
// Profiles

/// An RGB matrix/TRC model: per-channel linearisation of the 8-bit levels and the matrix
/// from linear RGB to PCS XYZ.
#[derive(Clone, Debug)]
pub struct MatrixShaper {
    pub lin: [[f64; 256]; 3],
    pub matrix: [[f64; 3]; 3],
}

/// A device profile prepared for evaluation.
#[derive(Clone, Debug)]
pub struct Profile {
    pub space: Space,
    output_class: bool,
    /// A2B1 (or A2B0, or matrix/TRC): device → PCS, relative colorimetric.
    to_pcs: Pipeline,
    /// B2A0: PCS → device, perceptual (black point detection only).
    from_pcs_perceptual: Option<Pipeline>,
    /// B2A1 (or B2A0): PCS → device, relative colorimetric.
    from_pcs: Option<Pipeline>,
    /// Whether the profile has a B2A1 table (Adobe's destination black point needs one).
    has_b2a1: bool,
    absolute_white: Option<[f64; 3]>,
}

impl Profile {
    pub fn parse(bytes: &[u8]) -> Result<Profile, IccError> {
        Self::parse_with_intent(bytes, Intent::Relative)
    }

    pub fn parse_with_intent(bytes: &[u8], intent: Intent) -> Result<Profile, IccError> {
        let p = ColorProfile::new_from_slice(bytes).map_err(|e| IccError::Parse(format!("{e:?}")))?;
        Profile::from_moxcms(&p, intent)
    }

    fn from_moxcms(p: &ColorProfile, intent: Intent) -> Result<Profile, IccError> {
        let space = match p.color_space {
            DataColorSpace::Cmyk => Space::Cmyk,
            DataColorSpace::Rgb => Space::Rgb,
            DataColorSpace::Gray => Space::Gray,
            other => return Err(IccError::Unsupported(format!("colour space {other:?}"))),
        };
        let pcs_lab = match p.pcs {
            DataColorSpace::Lab => true,
            DataColorSpace::Xyz => false,
            other => return Err(IccError::Unsupported(format!("connection space {other:?}"))),
        };
        if !matches!(p.profile_class, ProfileClass::InputDevice | ProfileClass::DisplayDevice | ProfileClass::OutputDevice | ProfileClass::ColorSpace) {
            return Err(IccError::Unsupported(format!("profile class {:?}", p.profile_class)));
        }
        let lut = |l: Option<&LutWarehouse>, to_pcs: bool| -> Result<Option<Pipeline>, IccError> {
            let Some(l) = l else { return Ok(None) };
            let pl = Pipeline::from_lut(l, pcs_lab, to_pcs)?;
            let (dev, pcs) = if to_pcs { (pl.inputs, pl.outputs) } else { (pl.outputs, pl.inputs) };
            if dev != space.channels() || pcs != 3 {
                return Err(IccError::Unsupported(format!("table with {} inputs and {} outputs", pl.inputs, pl.outputs)));
            }
            Ok(Some(pl))
        };
        let a2b0 = lut(p.lut_a_to_b_perceptual.as_ref(), true)?;
        let a2b1 = lut(p.lut_a_to_b_colorimetric.as_ref(), true)?;
        let b2a0 = lut(p.lut_b_to_a_perceptual.as_ref(), false)?;
        let b2a1 = lut(p.lut_b_to_a_colorimetric.as_ref(), false)?;
        let a2b2 = lut(p.lut_a_to_b_saturation.as_ref(), true)?;
        let b2a2 = lut(p.lut_b_to_a_saturation.as_ref(), false)?;
        let shaper = || -> Result<Option<Pipeline>, IccError> {
            match space {
                Space::Rgb => {
                    let (Some(r), Some(g), Some(b)) = (&p.red_trc, &p.green_trc, &p.blue_trc) else { return Ok(None) };
                    let (rc, gc, bc) = (p.red_colorant, p.green_colorant, p.blue_colorant);
                    let matrix = [[rc.x, gc.x, bc.x], [rc.y, gc.y, bc.y], [rc.z, gc.z, bc.z]];
                    let curves = vec![Curve::from_icc(r)?, Curve::from_icc(g)?, Curve::from_icc(b)?];
                    Ok(Some(Pipeline { inputs: 3, outputs: 3, pcs: Pcs::Xyz, stages: Stages::Shaper { curves, matrix } }))
                }
                Space::Gray => {
                    let Some(k) = &p.gray_trc else { return Ok(None) };
                    let matrix = [[D50[0], 0.0, 0.0], [0.0, D50[1], 0.0], [0.0, 0.0, D50[2]]];
                    Ok(Some(Pipeline { inputs: 1, outputs: 3, pcs: Pcs::Xyz, stages: Stages::Shaper { curves: vec![Curve::from_icc(k)?], matrix } }))
                }
                Space::Cmyk => Ok(None),
            }
        };
        let selected_a = match intent {
            Intent::Perceptual => a2b0.or(a2b1),
            Intent::Saturation => a2b2.or(a2b0).or(a2b1),
            _ => a2b1.or(a2b0),
        };
        let to_pcs = match selected_a {
            Some(pl) => pl,
            None => shaper()?.ok_or_else(|| IccError::Unsupported("no device-to-PCS table or matrix/TRC model".into()))?,
        };
        let has_b2a1 = b2a1.is_some();
        let from_pcs = match intent {
            Intent::Perceptual => b2a0.clone().or(b2a1),
            Intent::Saturation => b2a2.or_else(|| b2a0.clone()).or(b2a1),
            _ => b2a1.or_else(|| b2a0.clone()),
        };
        Ok(Profile {
            space,
            output_class: p.profile_class == ProfileClass::OutputDevice,
            to_pcs,
            from_pcs_perceptual: b2a0,
            from_pcs,
            has_b2a1,
            absolute_white: if intent == Intent::Absolute && p.profile_class == ProfileClass::OutputDevice { p.media_white_point.map(|w| [w.x, w.y, w.z]) } else { None },
        })
    }

    /// Device values (0–1, CMYK as ink) → PCS XYZ with the selected rendering intent.
    pub fn to_xyz(&self, device: &[f64]) -> [f64; 3] {
        let out = self.to_pcs.eval(device);
        let xyz = match self.to_pcs.stages {
            Stages::Shaper { .. } => [out[0], out[1], out[2]],
            _ => self.to_pcs.pcs.decode([out[0], out[1], out[2]]),
        };
        self.absolute_white.map_or(xyz, |w| std::array::from_fn(|i| xyz[i] * w[i] / D50[i]))
    }

    /// PCS XYZ → device values with the selected intent; `None` without a PCS → device table.
    pub fn from_xyz(&self, xyz: [f64; 3]) -> Option<[f64; 4]> {
        let pl = self.from_pcs.as_ref()?;
        let xyz = self.absolute_white.map_or(xyz, |w| std::array::from_fn(|i| xyz[i] * D50[i] / w[i]));
        Some(eval_from_pcs(pl, xyz))
    }

    pub fn can_convert_from_pcs(&self) -> bool {
        self.from_pcs.is_some()
    }

    /// PCS XYZ → device values (0–1) of an RGB matrix/TRC profile: the inverse of the matrix, then the inverse of each curve
    /// (found by bisection; out-of-range linear values are clipped first). `None` for other profiles.
    pub fn from_xyz_shaper(&self, xyz: [f64; 3]) -> Option<[f64; 3]> {
        let Stages::Shaper { curves, matrix } = &self.to_pcs.stages else { return None };
        if self.space != Space::Rgb || curves.len() != 3 {
            return None;
        }
        let inv = invert3(matrix)?;
        let lin = mul(&inv, xyz).map(|v| v.clamp(0.0, 1.0));
        Some([0, 1, 2].map(|c| {
            let (mut lo, mut hi) = (0.0f64, 1.0f64);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if curves[c].eval(mid) < lin[c] { lo = mid } else { hi = mid }
            }
            0.5 * (lo + hi)
        }))
    }

    /// PCS XYZ → device value (0–1) of a gray profile: through its PCS → device table when it has one, else the luminance relative
    /// to the PCS white, clipped, through the inverse of the curve (bisection). `None` for other profiles.
    pub fn from_xyz_gray(&self, xyz: [f64; 3]) -> Option<f64> {
        if self.space != Space::Gray {
            return None;
        }
        if let Some(pl) = &self.from_pcs {
            return Some(eval_from_pcs(pl, xyz)[0]);
        }
        let Stages::Shaper { curves, .. } = &self.to_pcs.stages else { return None };
        if curves.len() != 1 {
            return None;
        }
        let y = (xyz[1] / D50[1]).clamp(0.0, 1.0);
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if curves[0].eval(mid) < y { lo = mid } else { hi = mid }
        }
        Some(0.5 * (lo + hi))
    }

    /// For an RGB matrix/TRC profile: each channel's 8-bit levels linearised, and the
    /// matrix from linear RGB to PCS XYZ (colorants as columns).
    pub fn matrix_shaper(&self) -> Option<MatrixShaper> {
        let Stages::Shaper { curves, matrix } = &self.to_pcs.stages else { return None };
        if self.space != Space::Rgb || curves.len() != 3 {
            return None;
        }
        let lin = std::array::from_fn(|c| std::array::from_fn(|v| curves[c].eval(v as f64 / 255.0)));
        Some(MatrixShaper { lin, matrix: *matrix })
    }
}

fn eval_from_pcs(pl: &Pipeline, xyz: [f64; 3]) -> [f64; 4] {
    let out = pl.eval(&pl.pcs.encode(xyz));
    [out[0], out[1], out[2], out[3]]
}

// ---------------------------------------------------------------------------------------
// Black point compensation

/// L* clipped to [0, 50], neutral → XYZ.
fn neutral_black(l: f64) -> [f64; 3] {
    lab_to_xyz([l, 0.0, 0.0])
}

/// Black point of a profile used as the source of a relative colorimetric conversion.
///
/// A CMYK output profile: PCS black (L* 0) through the perceptual PCS → device table and
/// back through the colorimetric device → PCS table, made neutral, L* at most 50 (this
/// discounts ink limiting). Other profiles: the darkest colorant (RGB 0, CMYK 100 % of
/// each ink, gray 0) through the colorimetric table, made neutral; L* outside 0–50 → 0.
pub fn source_black_point(p: &Profile) -> [f64; 3] {
    if p.space == Space::Cmyk && p.output_class {
        let Some(perceptual) = &p.from_pcs_perceptual else { return [0.0; 3] };
        let device = eval_from_pcs(perceptual, [0.0; 3]);
        let lab = xyz_to_lab(p.to_xyz(&device));
        return neutral_black(lab[0].min(50.0));
    }
    let dark: &[f64] = match p.space {
        Space::Gray | Space::Rgb => &[0.0, 0.0, 0.0],
        Space::Cmyk => &[1.0, 1.0, 1.0, 1.0],
    };
    let l = xyz_to_lab(p.to_xyz(dark))[0];
    neutral_black(if (0.0..=50.0).contains(&l) { l } else { 0.0 })
}

/// Black point of a profile used as the destination of a relative colorimetric conversion
/// (Adobe's algorithm, ISO 18619 annex): a table-based gray, RGB or CMYK profile is
/// probed with an L* ramp through PCS → device → PCS. When the round trip is nearly
/// straight above the shadows, the source black point is used; otherwise the shadow
/// section (normalised output 0.1–0.5) is fitted with a least-squares quadratic whose
/// root is the black point L*. Other profiles use their source black point.
pub fn destination_black_point(p: &Profile) -> [f64; 3] {
    let Some(b2a) = p.from_pcs.as_ref().filter(|_| p.has_b2a1) else { return source_black_point(p) };
    let initial = xyz_to_lab(source_black_point(p));
    let (a0, b0) = (initial[1].clamp(-50.0, 50.0), initial[2].clamp(-50.0, 50.0));
    let mut input = [0.0f64; 256];
    let mut output = [0.0f64; 256];
    for l in 0..256 {
        let lab = [l as f64 * 100.0 / 255.0, a0, b0];
        let device = eval_from_pcs(b2a, lab_to_xyz(lab));
        input[l] = lab[0];
        output[l] = xyz_to_lab(p.to_xyz(&device))[0];
    }
    for l in (1..255).rev() {
        output[l] = output[l].min(output[l + 1]);
    }
    if output[0] >= output[255] {
        return [0.0; 3];
    }
    let (min_l, max_l) = (output[0], output[255]);
    let straight = (0..256).all(|l| input[l] <= min_l + 0.2 * (max_l - min_l) || (input[l] - output[l]).abs() < 4.0);
    if straight {
        return lab_to_xyz(initial);
    }
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for l in 0..256 {
        let y = (output[l] - min_l) / (max_l - min_l);
        if (0.1..0.5).contains(&y) {
            xs.push(input[l]);
            ys.push(y);
        }
    }
    if xs.len() < 3 {
        return [0.0; 3];
    }
    let l = quadratic_root(&xs, &ys).max(0.0);
    lab_to_xyz([l, initial[1], initial[2]])
}

/// Least-squares fit y = a·x² + b·x + c; the root (−b + √(b² − 4ac)) / 2a clamped to 0–50
/// (−c/b for a straight line), 0 when there is none.
fn quadratic_root(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mut sx, mut sx2, mut sx3, mut sx4, mut sy, mut syx, mut syx2) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (&xi, &yi) in x.iter().zip(y) {
        sx += xi;
        sx2 += xi * xi;
        sx3 += xi * xi * xi;
        sx4 += xi * xi * xi * xi;
        sy += yi;
        syx += yi * xi;
        syx2 += yi * xi * xi;
    }
    let Some([c, b, a]) = solve3([[n, sx, sx2], [sx, sx2, sx3], [sx2, sx3, sx4]], [sy, syx, syx2]) else { return 0.0 };
    if a.abs() < 1e-10 {
        if b.abs() < 1e-10 {
            return 0.0;
        }
        return (-c / b).clamp(0.0, 50.0);
    }
    let d = b * b - 4.0 * a * c;
    if d <= 0.0 {
        return 0.0;
    }
    ((-b + d.sqrt()) / (2.0 * a)).clamp(0.0, 50.0)
}

/// Solves m · x = v (Cramer's rule); `None` when singular.
fn solve3(m: M3, v: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: &M3| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(&m);
    if d.abs() < 1e-300 {
        return None;
    }
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let mut mk = m;
        for r in 0..3 {
            mk[r][k] = v[r];
        }
        *o = det(&mk) / d;
    }
    Some(out)
}

/// Black point compensation: a linear scaling in XYZ that maps the source black point to
/// the destination one and keeps the PCS white.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bpc {
    scale: [f64; 3],
    offset: [f64; 3],
}

impl Bpc {
    pub const NONE: Bpc = Bpc { scale: [1.0; 3], offset: [0.0; 3] };

    pub fn new(src: [f64; 3], dst: [f64; 3]) -> Bpc {
        if src == dst {
            return Bpc::NONE;
        }
        let mut b = Bpc::NONE;
        for k in 0..3 {
            let t = src[k] - D50[k];
            b.scale[k] = (dst[k] - D50[k]) / t;
            b.offset[k] = -D50[k] * (dst[k] - src[k]) / t;
        }
        b
    }

    #[inline]
    pub fn apply(&self, xyz: [f64; 3]) -> [f64; 3] {
        [0, 1, 2].map(|k| xyz[k] * self.scale[k] + self.offset[k])
    }
}

// ---------------------------------------------------------------------------------------
// Display: sRGB

/// sRGB (IEC 61966-2.1) as the display space: D65 primaries adapted to D50 with Bradford.
#[derive(Clone, Copy, Debug)]
pub struct Srgb {
    to_xyz: M3,
    from_xyz: M3,
}

fn inverse(m: &M3) -> Option<M3> {
    let mut out = [[0.0; 3]; 3];
    for c in 0..3 {
        let mut e = [0.0; 3];
        e[c] = 1.0;
        let col = solve3(*m, e)?;
        for r in 0..3 {
            out[r][c] = col[r];
        }
    }
    Some(out)
}

fn matmul(a: &M3, b: &M3) -> M3 {
    let mut out = [[0.0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}

impl Default for Srgb {
    fn default() -> Self {
        Srgb::new()
    }
}

impl Srgb {
    pub fn new() -> Srgb {
        // Primaries and white (x, y).
        let (r, g, b, w) = ((0.64, 0.33), (0.30, 0.60), (0.15, 0.06), (0.3127, 0.3290));
        let prim: M3 = [[r.0, g.0, b.0], [r.1, g.1, b.1], [1.0 - r.0 - r.1, 1.0 - g.0 - g.1, 1.0 - b.0 - b.1]];
        let white = [w.0 / w.1, 1.0, (1.0 - w.0 - w.1) / w.1];
        let coef = mul(&inverse(&prim).expect("sRGB primaries"), white);
        let rgb_xyz: M3 = [0, 1, 2].map(|row| [0, 1, 2].map(|c| prim[row][c] * coef[c]));
        // Bradford adaptation D65 → D50.
        let brad: M3 = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
        let (cs, cd) = (mul(&brad, white), mul(&brad, D50));
        let diag: M3 = [[cd[0] / cs[0], 0.0, 0.0], [0.0, cd[1] / cs[1], 0.0], [0.0, 0.0, cd[2] / cs[2]]];
        let adapt = matmul(&inverse(&brad).expect("Bradford"), &matmul(&diag, &brad));
        let to_xyz = matmul(&adapt, &rgb_xyz);
        Srgb { to_xyz, from_xyz: inverse(&to_xyz).expect("sRGB matrix") }
    }

    /// Display RGB (0–1) → XYZ (D50).
    pub fn to_xyz(&self, rgb: [f64; 3]) -> [f64; 3] {
        mul(&self.to_xyz, rgb.map(srgb_decode))
    }

    /// XYZ (D50) → linear display RGB, not clipped.
    pub fn linear_from_xyz(&self, xyz: [f64; 3]) -> [f64; 3] {
        mul(&self.from_xyz, xyz)
    }

    /// XYZ (D50) → display RGB (0–1), clipped to the gamut.
    pub fn from_xyz(&self, xyz: [f64; 3]) -> [f64; 3] {
        mul(&self.from_xyz, xyz).map(|c| srgb_encode(c.clamp(0.0, 1.0)))
    }

    /// XYZ (D50) → 8-bit display RGB.
    pub fn from_xyz8(&self, xyz: [f64; 3]) -> [u8; 3] {
        self.from_xyz(xyz).map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
    }
}

pub fn srgb_decode(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

pub fn srgb_encode(l: f64) -> f64 {
    if l <= 0.0031308 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_profiles_and_colour_tables_return_errors() {
        for len in 0..=132 {
            assert!(Profile::parse(&vec![0; len]).is_err(), "zero ICC header of length {len}");
        }
        let empty = LutStore::Store16(vec![]);
        for (grid, outputs) in [(vec![], 1), (vec![0], 1), (vec![2], 0), (vec![2, 2, 2], 3)] {
            assert!(Clut::new(grid, outputs, &empty).is_err());
        }
        assert!(Clut::new(vec![2, 2, 2], 3, &LutStore::Store8(vec![0; 23])).is_err());
    }

    #[test]
    fn lab_round_trip_and_white() {
        let xyz = lab_to_xyz([50.0, 20.0, -30.0]);
        let lab = xyz_to_lab(xyz);
        assert!((lab[0] - 50.0).abs() < 1e-9 && (lab[1] - 20.0).abs() < 1e-9 && (lab[2] + 30.0).abs() < 1e-9);
        let s = Srgb::new();
        assert_eq!(s.from_xyz8(D50), [255, 255, 255]);
        assert_eq!(s.from_xyz8([0.0; 3]), [0, 0, 0]);
        let w = s.to_xyz([1.0; 3]);
        assert!((0..3).all(|k| (w[k] - D50[k]).abs() < 1e-6), "{w:?}");
    }

    #[test]
    fn bpc_maps_black_and_keeps_white() {
        let src = neutral_black(20.0);
        let b = Bpc::new(src, [0.0; 3]);
        let o = b.apply(src);
        assert!(o.iter().all(|v| v.abs() < 1e-12), "{o:?}");
        let w = b.apply(D50);
        assert!((0..3).all(|k| (w[k] - D50[k]).abs() < 1e-12));
    }

    #[test]
    fn quadratic_root_of_a_known_curve() {
        // y = (x² − 100) / 1000: roots at ±10, the larger one is the black point.
        let xs: Vec<f64> = (12..30).map(|v| v as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| (x * x - 100.0) / 1000.0).collect();
        assert!((quadratic_root(&xs, &ys) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn tetrahedral_is_exact_on_a_linear_table() {
        // 3 inputs, 2 grid points, output = (x + 2y + 3z) / 6.
        let mut data = Vec::new();
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    data.push(((x + 2 * y + 3 * z) as f64 / 6.0 * 65535.0).round() as u16);
                }
            }
        }
        let clut = Clut::new(vec![2, 2, 2], 1, &LutStore::Store16(data)).unwrap();
        let mut out = [0.0; 16];
        clut.eval(&[0.3, 0.7, 0.1], &mut out);
        assert!((out[0] - (0.3 + 1.4 + 0.3) / 6.0).abs() < 1e-4, "{}", out[0]);
    }
}
