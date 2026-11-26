//! Edit ▸ Convert to Profile: document colours (swatch values) converted from one ICC profile to another.
//!
//! Measured in InDesign 2026 (2025 not checked), engine Adobe (ACE), see `artifacts/profileconv-c1-20261023/STATUS.ko.md` and
//! `tests/convert_reference.rs` (13 settings, 16–96 swatches each):
//! - RGB values go from the document RGB profile to the destination RGB profile, CMYK ink from the document CMYK profile to the
//!   destination CMYK profile; each space is converted on its own, through the PCS (`to_xyz` of the source with the intent's
//!   table, black point compensation, `from_xyz` of the destination with the intent's table).
//! - The result is 8-bit: RGB integers 0–255, CMYK in steps of 100/255 percent.
//! - Black point compensation is the ACE flag "Use Black Point Compensation"; it acts on relative colorimetric only (perceptual and saturation
//!   tables already map the black, absolute colorimetric keeps it): with the flag on, those three intents agree with the table-only conversion.

use crate::icc::{Bpc, IccError, Intent, Profile, Space, destination_black_point, source_black_point};

/// The Conversion Options of the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertSettings {
    pub intent: Intent,
    pub black_point_compensation: bool,
}

impl Default for ConvertSettings {
    /// The dialog's defaults: Relative Colorimetric with black point compensation.
    fn default() -> Self {
        ConvertSettings { intent: Intent::Relative, black_point_compensation: true }
    }
}

/// One space's conversion (RGB → RGB or CMYK → CMYK).
pub struct ColorConverter {
    space: Space,
    src: Profile,
    dst: Profile,
    bpc: Bpc,
}

impl ColorConverter {
    /// Converter from the profile file `source` to the profile file `destination` (both RGB or both CMYK).
    pub fn new(source: &[u8], destination: &[u8], settings: ConvertSettings) -> Result<ColorConverter, IccError> {
        let src = Profile::parse_with_intent(source, settings.intent)?;
        let dst = Profile::parse_with_intent(destination, settings.intent)?;
        if src.space != dst.space || !matches!(src.space, Space::Rgb | Space::Cmyk) {
            return Err(IccError::Unsupported("the two profiles do not have the same RGB or CMYK colour space".into()));
        }
        // the destination needs a way back from the PCS: a table, or for RGB the matrix and curves
        if !(dst.can_convert_from_pcs() || src.space == Space::Rgb && dst.from_xyz_shaper([0.5; 3]).is_some()) {
            return Err(IccError::Unsupported("the destination profile has no PCS to device table or matrix model".into()));
        }
        let bpc = if settings.black_point_compensation && settings.intent == Intent::Relative {
            // black points are those of the relative colorimetric tables, like the proof conversions
            let s = Profile::parse_with_intent(source, Intent::Relative)?;
            let d = Profile::parse_with_intent(destination, Intent::Relative)?;
            Bpc::new(source_black_point(&s), destination_black_point(&d))
        } else {
            Bpc::NONE
        };
        Ok(ColorConverter { space: src.space, src, dst, bpc })
    }

    pub fn space(&self) -> Space {
        self.space
    }

    /// CMYK ink (0–1) of the source profile → ink (0–1) of the destination, not quantised (what a sampled image link is made of).
    pub fn cmyk_unit(&self, ink: [f64; 4]) -> Option<[f64; 4]> {
        if self.space != Space::Cmyk {
            return None;
        }
        let xyz = self.bpc.apply(self.src.to_xyz(&ink));
        let out = self.dst.from_xyz(xyz)?;
        Some([out[0], out[1], out[2], out[3]])
    }

    /// Converts IDML values (CMYK 0–100, RGB 0–255) to the destination profile's, quantised to 8 bits.
    pub fn convert(&self, values: &[f64]) -> Option<Vec<f64>> {
        match self.space {
            Space::Cmyk => {
                let ink: Vec<f64> = values.get(..4)?.iter().map(|v| (v / 100.0).clamp(0.0, 1.0)).collect();
                let xyz = self.bpc.apply(self.src.to_xyz(&ink));
                let out = self.dst.from_xyz(xyz)?;
                Some(out.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() * 100.0 / 255.0).collect())
            }
            Space::Rgb => {
                let rgb: Vec<f64> = values.get(..3)?.iter().map(|v| (v / 255.0).clamp(0.0, 1.0)).collect();
                let xyz = self.bpc.apply(self.src.to_xyz(&rgb));
                let out = match self.dst.from_xyz_shaper(xyz) {
                    Some(o) => o,
                    None => {
                        let o = self.dst.from_xyz(xyz)?;
                        [o[0], o[1], o[2]]
                    }
                };
                Some(out.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round()).collect())
            }
            Space::Gray => None,
        }
    }
}

/// InDesign's automatic swatch name of CMYK ink percentages: `C=10 M=20 Y=30 K=40` (each value rounded).
pub fn auto_name_cmyk(v: &[f64]) -> Option<String> {
    let r = |i: usize| v.get(i).map(|x| x.round() as i64);
    Some(format!("C={} M={} Y={} K={}", r(0)?, r(1)?, r(2)?, r(3)?))
}

/// InDesign's automatic swatch name of RGB values: `R=10 G=20 B=30`.
pub fn auto_name_rgb(v: &[f64]) -> Option<String> {
    let r = |i: usize| v.get(i).map(|x| x.round() as i64);
    Some(format!("R={} G={} B={}", r(0)?, r(1)?, r(2)?))
}
