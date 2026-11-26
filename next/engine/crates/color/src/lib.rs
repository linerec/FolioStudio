//! Display colour as InDesign shows it, through the document's own ICC profiles.
//!
//! - CMYK ink goes through the document CMYK profile to sRGB, relative colorimetric with
//!   black point compensation (InDesign's default colour settings, Adobe ACE).
//! - RGB content is in the document RGB profile; when that is not sRGB it is converted to
//!   sRGB the same way.
//! - On a page that uses the transparency blend space "Document CMYK" (InDesign's default,
//!   in effect on a page with any transparency), RGB and gray content shows as it looks
//!   after conversion to that CMYK: RGB goes document RGB → CMYK → sRGB (same intent),
//!   gray becomes black ink. CMYK content is the same in both cases.
//!
//! The profiles are the user's installed files, found by description ([`ProfileIndex`]);
//! nothing derived from them ships with the program. [`document_colors`] resolves a
//! document's profiles and builds (or loads from the on-disk cache) the sampled
//! conversions of [`Colors`]. A missing profile falls back to a conversion without colour
//! management ([`Colors::unmanaged`]) and a warning naming it.
//!
//! Profiles are evaluated exactly ([`icc`]); the conversions are then sampled and
//! interpolated the way LittleCMS precalculates its 8- and 16-bit transforms, because that
//! is how the reference renderer (MuPDF, on InDesign's exported PDF) converts colours and
//! pixels. Against LittleCMS 2.18 (Pillow, same intent and flags) CMYK → sRGB agrees to
//! 0.002–0.004 per channel on average (at most 1) for the six installed CMYK profiles
//! tried, and the CMYK blend space round trip to 0.004–0.007 (at most 4).

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

mod cache;
mod convert;
mod document;
mod grid;
mod gradient;
mod picker;
mod proof;
pub use convert::{ColorConverter, ConvertSettings, auto_name_cmyk, auto_name_rgb};
pub use gradient::GradientCurve;
pub use picker::{Fixed, GAMUT_TOLERANCE, Picked, Space as PickSpace, pick as pick_color};
pub use proof::ProofSpec;
pub mod icc;
pub mod profiles;

pub use document::{ColorReport, DocumentColors, ProfileStatus, ProfileUse, SRGB_NAME, document_colors};
pub use profiles::{ProfileEntry, ProfileIndex, read_profile};

use folio_model::{ColorModel, ColorSpace, Gradient, Id, Swatch};
use grid::{Cmyk4, Rgb3, RgbCmyk, RgbDisplay};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The space content is composited in on a page: RGB, or InDesign's transparency blend
/// space "Document CMYK" (pages with transparency).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendSpace {
    #[default]
    Rgb,
    Cmyk,
}

/// A document's display conversions (cheap to share: tables are reference counted).
///
/// Every table is optional; a missing one means the mapping needs no table:
/// - `cmyk`: CMYK ink → sRGB; `None`: without colour management (1 − ink).
/// - `rgb`: document RGB → sRGB; `None`: the document RGB is sRGB.
/// - `rgb_cmyk`: document RGB → 8-bit CMYK (the CMYK blend space); `None`: CMYK is
///   unmanaged, and such content shows as on an RGB page. Colours go through it and `cmyk`
///   as LittleCMS would; pixels through `rgb_cmyk_px`, that result sampled at every fifth
///   8-bit level and interpolated (0.3 levels off on average, a few at most in saturated
///   colours) because two links per pixel would make a photo several times slower.
/// - `srgb_cmyk`: sRGB → 8-bit CMYK (Lab content); `None`: `rgb_cmyk` serves when the
///   document RGB is sRGB.
#[derive(Clone, Default)]
pub struct Colors {
    cmyk: Option<Arc<Cmyk4>>,
    rgb: Option<Arc<RgbDisplay>>,
    rgb_cmyk: Option<Arc<RgbCmyk>>,
    rgb_cmyk_px: Option<Arc<Rgb3>>,
    srgb_cmyk: Option<Arc<RgbCmyk>>,
    proof_paper: Option<[u8; 3]>,
    /// Identity of the conversion (0 = unmanaged), for caches of converted content.
    key: u64,
}

impl std::fmt::Debug for Colors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Colors")
            .field("key", &format_args!("{:016x}", self.key))
            .field("cmyk", &self.cmyk.is_some())
            .field("rgb", &self.rgb.is_some())
            .field("rgb_cmyk", &self.rgb_cmyk.is_some())
            .field("srgb_cmyk", &self.srgb_cmyk.is_some())
            .finish()
    }
}

impl Colors {
    /// Conversion without profiles: CMYK as 1 − ink, RGB as sRGB, and the CMYK blend space
    /// changes nothing.
    pub fn unmanaged() -> Colors {
        Colors::default()
    }

    /// Identity of the conversion: equal keys convert alike (0 for [`Colors::unmanaged`]).
    pub fn key(&self) -> u64 {
        self.key
    }

    /// The unpainted page color in a proof preview.
    pub fn paper_display(&self) -> [u8; 3] {
        self.proof_paper.unwrap_or([255; 3])
    }

    /// Whether CMYK goes through a profile.
    pub fn cmyk_managed(&self) -> bool {
        self.cmyk.is_some()
    }

    /// Whether document RGB goes through a profile other than sRGB.
    pub fn rgb_managed(&self) -> bool {
        self.rgb.is_some()
    }

    /// Display colour of CMYK ink percentages (0–100).
    pub fn cmyk_to_srgb(&self, c: f64, m: f64, y: f64, k: f64) -> [u8; 3] {
        match &self.cmyk {
            Some(t) => t.eval_percent(c, m, y, k),
            None => {
                let f = |v: f64| 1.0 - (v / 100.0).clamp(0.0, 1.0);
                let kk = f(k);
                [c, m, y].map(|v| (255.0 * f(v) * kk).round() as u8)
            }
        }
    }

    /// Display colour of 8-bit CMYK ink (0 = none, 255 = 100%), e.g. a CMYK image pixel.
    #[inline]
    pub fn cmyk8_to_srgb(&self, c: u8, m: u8, y: u8, k: u8) -> [u8; 3] {
        match &self.cmyk {
            Some(t) => t.eval8(c, m, y, k),
            None => {
                let kk = 255 - k as u32;
                [c, m, y].map(|v| (((255 - v as u32) * kk + 127) / 255) as u8)
            }
        }
    }

    /// Display colour of a document RGB colour.
    #[inline]
    pub fn rgb_to_srgb(&self, rgb: [u8; 3]) -> [u8; 3] {
        match &self.rgb {
            Some(t) => t.eval(rgb),
            None => rgb,
        }
    }

    /// A document RGB colour as shown in the CMYK blend space: converted to 8-bit CMYK and
    /// shown from there, as a CMYK transparency group composites it.
    #[inline]
    pub fn rgb_through_cmyk(&self, rgb: [u8; 3]) -> [u8; 3] {
        match (&self.rgb_cmyk, &self.cmyk) {
            (Some(link), Some(cmyk)) => {
                let [c, m, y, k] = link.eval8(rgb);
                cmyk.eval8(c, m, y, k)
            }
            _ => self.rgb_to_srgb(rgb),
        }
    }

    /// An sRGB colour (e.g. converted Lab) as shown in the CMYK blend space.
    #[inline]
    pub fn srgb_through_cmyk(&self, rgb: [u8; 3]) -> [u8; 3] {
        let link = match (&self.srgb_cmyk, &self.rgb) {
            (Some(l), _) => Some(l),
            (None, None) => self.rgb_cmyk.as_ref(),
            (None, Some(_)) => None,
        };
        match (link, &self.cmyk) {
            (Some(link), Some(cmyk)) => {
                let [c, m, y, k] = link.eval8(rgb);
                cmyk.eval8(c, m, y, k)
            }
            _ => rgb,
        }
    }

    /// A document RGB colour as 8-bit CMYK ink (0 = none, 255 = 100 %), converted the way the
    /// CMYK blend space converts it (document RGB → document CMYK, same intent). Without a
    /// managed CMYK, the inverse of the unmanaged display (black from the lightest channel).
    #[inline]
    pub fn rgb_to_cmyk8(&self, rgb: [u8; 3]) -> [u8; 4] {
        match &self.rgb_cmyk {
            Some(link) => link.eval8(rgb),
            None => naive_cmyk8(rgb),
        }
    }

    /// An sRGB colour (e.g. converted Lab) as 8-bit CMYK ink, like [`Colors::rgb_to_cmyk8`].
    #[inline]
    pub fn srgb_to_cmyk8(&self, rgb: [u8; 3]) -> [u8; 4] {
        match (&self.srgb_cmyk, &self.rgb) {
            (Some(link), _) => link.eval8(rgb),
            (None, None) => self.rgb_to_cmyk8(rgb),
            (None, Some(_)) => naive_cmyk8(rgb),
        }
    }

    /// A gray level (0 = black) as shown in the CMYK blend space: black ink only.
    #[inline]
    pub fn gray_through_cmyk(&self, gray: u8) -> [u8; 3] {
        self.cmyk8_to_srgb(0, 0, 0, 255 - gray)
    }

    /// Converts straight-alpha RGBA pixels of document RGB in place to display colour in a
    /// page's blend space. Runs of equal colours reuse the previous result.
    pub fn rgba_to_display(&self, px: &mut [u8], blend: BlendSpace) {
        match (blend, &self.rgb_cmyk_px, &self.rgb) {
            (BlendSpace::Cmyk, Some(t), _) => map_pixels(px, |rgb| t.eval(rgb)),
            (_, _, Some(t)) => map_pixels(px, |rgb| t.eval(rgb)),
            _ => {}
        }
    }

    /// Display colour of a swatch at `tint` percent (applied on top of a tint swatch's own
    /// tint), `None` for the swatch "None". Tints scale ink for CMYK and gray, move Lab
    /// towards white in Lab, and blend RGB towards white, as InDesign writes them to PDF.
    pub fn swatch_rgb(&self, sw: &Swatch, tint: f64, blend: BlendSpace) -> Option<[u8; 3]> {
        let t = (tint / 100.0).clamp(0.0, 1.0) * sw.tint.map_or(1.0, |s| (s / 100.0).clamp(0.0, 1.0));
        let v = |i: usize| sw.values.get(i).copied().unwrap_or(0.0);
        let srgb = |rgb: [u8; 3]| if blend == BlendSpace::Cmyk { self.srgb_through_cmyk(rgb) } else { rgb };
        let toward_white = |rgb: [u8; 3]| rgb.map(|c| (255.0 - (255.0 - c as f64) * t).round() as u8);
        Some(match sw.model {
            ColorModel::None => return None,
            ColorModel::Paper => [255, 255, 255],
            ColorModel::Registration => self.cmyk_to_srgb(100.0 * t, 100.0 * t, 100.0 * t, 100.0 * t),
            _ => match sw.space {
                ColorSpace::Cmyk if sw.values.len() >= 4 => self.cmyk_to_srgb(v(0) * t, v(1) * t, v(2) * t, v(3) * t),
                ColorSpace::Rgb if sw.values.len() >= 3 => {
                    let rgb = toward_white([v(0), v(1), v(2)].map(|c| c.round().clamp(0.0, 255.0) as u8));
                    match blend {
                        BlendSpace::Cmyk => self.rgb_through_cmyk(rgb),
                        BlendSpace::Rgb => self.rgb_to_srgb(rgb),
                    }
                }
                ColorSpace::Lab if sw.values.len() >= 3 => srgb(lab_to_srgb([100.0 + (v(0) - 100.0) * t, v(1) * t, v(2) * t])),
                ColorSpace::Gray if !sw.values.is_empty() => {
                    let ink = (v(0) * t).clamp(0.0, 100.0);
                    match blend {
                        BlendSpace::Cmyk => self.cmyk_to_srgb(0.0, 0.0, 0.0, ink),
                        BlendSpace::Rgb => [(255.0 * (1.0 - ink / 100.0)).round() as u8; 3],
                    }
                }
                // No usable values: the stored approximation, with the paint's tint only.
                _ => srgb(sw.display_rgb.map(|c| (255.0 - (255.0 - c as f64) * (tint / 100.0).clamp(0.0, 1.0)).round() as u8)),
            },
        })
    }

    /// The colour a fraction `t` (0–1) of the way from swatch `a` to swatch `b`, as a
    /// gradient between them shows it: inks are interpolated when both are CMYK (InDesign
    /// blends a gradient in the document's CMYK; Paper is no ink), display colours otherwise.
    pub fn swatch_mix(&self, a: &Swatch, b: &Swatch, t: f64, blend: BlendSpace) -> Option<[u8; 3]> {
        let ink = |s: &Swatch| -> Option<[f64; 4]> {
            match s.model {
                ColorModel::Paper => Some([0.0; 4]),
                ColorModel::None => None,
                _ if s.space == ColorSpace::Cmyk && s.values.len() >= 4 => {
                    let k = s.tint.map_or(1.0, |v| (v / 100.0).clamp(0.0, 1.0));
                    Some([s.values[0] * k, s.values[1] * k, s.values[2] * k, s.values[3] * k])
                }
                _ => None,
            }
        };
        let t = t.clamp(0.0, 1.0);
        if let (Some(x), Some(y)) = (ink(a), ink(b)) {
            let m = |i: usize| x[i] + (y[i] - x[i]) * t;
            return Some(self.cmyk_to_srgb(m(0), m(1), m(2), m(3)));
        }
        let (x, y) = (self.swatch_rgb(a, 100.0, blend)?, self.swatch_rgb(b, 100.0, blend)?);
        Some([0, 1, 2].map(|i| (x[i] as f64 + (y[i] as f64 - x[i] as f64) * t).round() as u8))
    }

    /// Flat stand-in for a gradient where it is not drawn as one (e.g. text): the mean of
    /// its stop colours on an RGB page, shown through the blend space. `None` when no stop
    /// colour exists.
    pub fn gradient_flat(&self, g: &Gradient, swatches: &BTreeMap<Id, Swatch>, blend: BlendSpace) -> Option<[u8; 3]> {
        let mut sum = [0.0f64; 3];
        let mut n = 0usize;
        for s in &g.stops {
            if let Some(rgb) = swatches.get(&s.color).and_then(|sw| self.swatch_rgb(sw, 100.0, BlendSpace::Rgb)) {
                for k in 0..3 {
                    sum[k] += rgb[k] as f64;
                }
                n += 1;
            }
        }
        if n == 0 {
            return None;
        }
        let mean = sum.map(|v| (v / n as f64).round().clamp(0.0, 255.0) as u8);
        Some(if blend == BlendSpace::Cmyk { self.srgb_through_cmyk(mean) } else { mean })
    }
}

/// RGB → CMYK as the inverse of the unmanaged display `(1 − ink) × (1 − black)`.
fn naive_cmyk8(rgb: [u8; 3]) -> [u8; 4] {
    let max = rgb.iter().copied().max().unwrap_or(0) as u32;
    if max == 0 {
        return [0, 0, 0, 255];
    }
    let ink = |v: u8| (255 - (v as u32 * 255 + max / 2) / max) as u8;
    [ink(rgb[0]), ink(rgb[1]), ink(rgb[2]), (255 - max) as u8]
}

/// Converts the colour of straight-alpha RGBA pixels in place; runs of equal colours reuse
/// the previous result.
fn map_pixels(px: &mut [u8], convert: impl Fn([u8; 3]) -> [u8; 3]) {
    let mut last: Option<([u8; 3], [u8; 3])> = None;
    for p in px.chunks_exact_mut(4) {
        let rgb = [p[0], p[1], p[2]];
        let out = match last {
            Some((i, o)) if i == rgb => o,
            _ => {
                let o = convert(rgb);
                last = Some((rgb, o));
                o
            }
        };
        p[..3].copy_from_slice(&out);
    }
}

/// RGB in a source profile (e.g. one embedded in an image) → CMYK of an output profile,
/// sampled and interpolated like the document RGB → CMYK link of [`Colors::rgb_to_cmyk8`]
/// (relative colorimetric with black point compensation, Adobe ACE's default and InDesign's
/// "Convert to Destination (Preserve Numbers)" of tagged RGB).
#[derive(Clone)]
pub struct RgbLink(Arc<RgbCmyk>);

impl RgbLink {
    /// 8-bit RGB → 8-bit ink (0 = none, 255 = 100 %).
    #[inline]
    pub fn eval8(&self, rgb: [u8; 3]) -> [u8; 4] {
        self.0.eval8(rgb)
    }
}

/// CMYK ink in a source profile (the profile assigned to a placed CMYK image) → ink of the
/// output profile: the same conversion as Edit ▸ Convert to Profile ([`ColorConverter`],
/// black point compensation on relative colorimetric), sampled like [`RgbLink`]. Measured
/// in InDesign 2025's PDF/X-1a output of CMYK TIFF and JPEG images with an assigned
/// profile (`artifacts/imgcolor-c2-20261023/`): all four intents.
#[derive(Clone)]
pub struct CmykLink(Arc<grid::CmykCmyk>);

impl CmykLink {
    /// 8-bit ink → 8-bit ink (0 = none, 255 = 100 %).
    #[inline]
    pub fn eval8(&self, ink: [u8; 4]) -> [u8; 4] {
        self.0.eval8(ink)
    }
}

/// The link from CMYK profile `src` to CMYK profile `dst` (both ICC data).
pub fn cmyk_link_with_intent(src: &[u8], dst: &[u8], intent: icc::Intent) -> Result<CmykLink, icc::IccError> {
    let converter = ColorConverter::new(src, dst, ConvertSettings { intent, black_point_compensation: true })?;
    if converter.space() != icc::Space::Cmyk {
        return Err(icc::IccError::Unsupported("both profiles must be CMYK profiles".into()));
    }
    Ok(CmykLink(Arc::new(grid::CmykCmyk::sample_ink(&|ink| converter.cmyk_unit(ink).unwrap_or([0.0; 4])))))
}

/// The link from RGB profile `src` to CMYK profile `cmyk` (both ICC data).
pub fn rgb_link(src: &[u8], cmyk: &[u8]) -> Result<RgbLink, icc::IccError> {
    rgb_link_with_intent(src, cmyk, icc::Intent::Relative)
}

pub fn rgb_link_with_intent(src: &[u8], cmyk: &[u8], intent: icc::Intent) -> Result<RgbLink, icc::IccError> {
    let s = icc::Profile::parse_with_intent(src, intent)?;
    let c = icc::Profile::parse_with_intent(cmyk, intent)?;
    if s.space != icc::Space::Rgb {
        return Err(icc::IccError::Unsupported("source is not an RGB profile".into()));
    }
    if c.space != icc::Space::Cmyk || !c.can_convert_from_pcs() {
        return Err(icc::IccError::Unsupported("output is not a CMYK profile with a PCS → device table".into()));
    }
    let bpc = if intent == icc::Intent::Relative {
        icc::Bpc::new(icc::source_black_point(&s), icc::destination_black_point(&c))
    } else { icc::Bpc::new([0.0; 3], [0.0; 3]) };
    let link = RgbCmyk::sample_rgb(&|v| c.from_xyz(bpc.apply(s.to_xyz(&v))).unwrap_or([0.0; 4]));
    Ok(RgbLink(Arc::new(link)))
}

/// CIE L*a*b* (D50, as InDesign stores it) → sRGB, relative colorimetric.
pub fn lab_to_srgb(lab: [f64; 3]) -> [u8; 3] {
    static SRGB: std::sync::OnceLock<icc::Srgb> = std::sync::OnceLock::new();
    SRGB.get_or_init(icc::Srgb::new).from_xyz8(icc::lab_to_xyz(lab))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmanaged_conversions() {
        let u = Colors::unmanaged();
        assert_eq!(u.cmyk_to_srgb(0.0, 0.0, 0.0, 0.0), [255, 255, 255]);
        assert_eq!(u.cmyk_to_srgb(0.0, 0.0, 0.0, 100.0), [0, 0, 0]);
        assert_eq!(u.cmyk_to_srgb(100.0, 0.0, 0.0, 0.0), [0, 255, 255]);
        assert_eq!(u.cmyk8_to_srgb(0, 255, 255, 0), [255, 0, 0]);
        assert_eq!(u.srgb_through_cmyk([0, 255, 0]), [0, 255, 0]);
        assert_eq!(u.rgb_through_cmyk([10, 20, 30]), [10, 20, 30]);
        assert_eq!(u.gray_through_cmyk(100), [100, 100, 100]);
        assert_eq!(u.key(), 0);
        assert_eq!(u.rgb_to_cmyk8([255, 255, 255]), [0, 0, 0, 0]);
        assert_eq!(u.rgb_to_cmyk8([0, 0, 0]), [0, 0, 0, 255]);
        assert_eq!(u.rgb_to_cmyk8([0, 255, 255]), [255, 0, 0, 0]);
        assert_eq!(u.srgb_to_cmyk8([255, 0, 255]), [0, 255, 0, 0]);
        // the unmanaged display of the result is the colour again
        let [c, m, y, k] = u.rgb_to_cmyk8([200, 100, 50]);
        assert_eq!(u.cmyk8_to_srgb(c, m, y, k), [200, 100, 50]);
    }

    #[test]
    fn lab_white_black_and_gray() {
        assert_eq!(lab_to_srgb([100.0, 0.0, 0.0]), [255, 255, 255]);
        assert_eq!(lab_to_srgb([0.0, 0.0, 0.0]), [0, 0, 0]);
        let g = lab_to_srgb([50.0, 0.0, 0.0]);
        assert!((g[0] as i32 - 119).abs() <= 1 && g[0] == g[1] && g[1] == g[2], "{g:?}");
    }
}
