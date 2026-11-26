//! The Color Picker dialog's colour conversions: one colour given in RGB, CMYK or Lab, shown in all
//! three through the document's own profiles ([`Colors`]), and whether the document's CMYK can print it.
//!
//! The picker's screen maths that needs no profile (HSB, hex) stays in the renderer; everything that does
//! need the profiles comes from here so that the dialog, a script and an LLM get the same numbers
//! (the engine query `colorPick`). Measured in InDesign 2025 (`Color.space` of a colour in a document with
//! sRGB and U.S. Web Coated SWOP v2, `artifacts/colorpicker-c1-20261023/measure/convert.json`, kept as
//! `tests/colorpicker_convert.json`): RGB → CMYK is the CMYK blend-space link (black point compensation, so
//! RGB black is a rich black), CMYK → RGB and Lab → RGB are the display conversions. InDesign gives
//! fractions (8-bit table sampling aside, it also shifts Lab a and b by −0.5 for neutral colours); the
//! numbers here are 8-bit RGB and ink in 8-bit steps.
//!
//! The out-of-gamut test is not measured (the picker's warning is not scriptable): a colour is out of the
//! document CMYK's gamut when converting it to CMYK and back to the screen changes any channel by more than
//! [`GAMUT_TOLERANCE`] levels, or when a Lab colour lies outside sRGB. `fixed` is the colour the warning
//! swatch offers (the CMYK round trip).
use crate::{Colors, icc, lab_to_srgb};

/// The most a channel may change (8-bit levels) in the round trip through CMYK for the colour to count as in gamut.
pub const GAMUT_TOLERANCE: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Rgb,
    Cmyk,
    Lab,
}

impl Space {
    pub fn parse(name: &str) -> Option<Space> {
        match name.to_ascii_uppercase().as_str() {
            "RGB" => Some(Space::Rgb),
            "CMYK" => Some(Space::Cmyk),
            "LAB" => Some(Space::Lab),
            _ => None,
        }
    }
}

/// The colour a gamut warning offers instead.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixed {
    pub cmyk: [f64; 4],
    pub display: [u8; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
    /// Document RGB (0–255); for CMYK and Lab colours the screen colour they show as.
    pub rgb: [f64; 3],
    /// Ink percentages (0–100).
    pub cmyk: [f64; 4],
    /// CIE L*a*b* (D50).
    pub lab: [f64; 3],
    /// The colour as the screen shows it (sRGB).
    pub display: [u8; 3],
    pub in_gamut: bool,
    pub fixed: Option<Fixed>,
}

fn pct(ink8: u8) -> f64 {
    (ink8 as f64 * 10000.0 / 255.0).round() / 100.0
}

fn lab_of(display: [u8; 3]) -> [f64; 3] {
    static SRGB: std::sync::OnceLock<icc::Srgb> = std::sync::OnceLock::new();
    icc::xyz_to_lab(SRGB.get_or_init(icc::Srgb::new).to_xyz(display.map(|c| c as f64 / 255.0)))
}

/// `value` in `space` as all three; `None` when it has the wrong number of components or one is not finite.
pub fn pick(colors: &Colors, space: Space, value: &[f64]) -> Option<Picked> {
    let want = if space == Space::Cmyk { 4 } else { 3 };
    if value.len() != want || value.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (rgb, cmyk8, display, lab, clipped) = match space {
        Space::Rgb => {
            let rgb8 = [0, 1, 2].map(|i| value[i].round().clamp(0.0, 255.0) as u8);
            let display = colors.rgb_to_srgb(rgb8);
            (rgb8.map(f64::from), colors.rgb_to_cmyk8(rgb8), display, lab_of(display), false)
        }
        Space::Cmyk => {
            let ink = [0, 1, 2, 3].map(|i| value[i].clamp(0.0, 100.0));
            let display = colors.cmyk_to_srgb(ink[0], ink[1], ink[2], ink[3]);
            let cmyk8 = ink.map(|v| (v * 2.55).round() as u8);
            (display.map(f64::from), cmyk8, display, lab_of(display), false)
        }
        Space::Lab => {
            let lab = [value[0].clamp(0.0, 100.0), value[1].clamp(-128.0, 127.0), value[2].clamp(-128.0, 127.0)];
            let display = lab_to_srgb(lab);
            let linear = icc::Srgb::new().linear_from_xyz(icc::lab_to_xyz(lab));
            let clipped = linear.iter().any(|c| !(-0.0005..=1.0005).contains(c));
            (display.map(f64::from), colors.srgb_to_cmyk8(display), display, lab, clipped)
        }
    };
    let cmyk = match space {
        Space::Cmyk => [0, 1, 2, 3].map(|i| value[i].clamp(0.0, 100.0)),
        _ => cmyk8.map(pct),
    };
    let (in_gamut, fixed) = if space == Space::Cmyk {
        (true, None)
    } else {
        let back = colors.cmyk8_to_srgb(cmyk8[0], cmyk8[1], cmyk8[2], cmyk8[3]);
        let worst = (0..3).map(|i| (display[i] as i32 - back[i] as i32).abs()).max().unwrap_or(0);
        if clipped || worst > GAMUT_TOLERANCE { (false, Some(Fixed { cmyk, display: back })) } else { (true, None) }
    };
    Some(Picked { rgb, cmyk, lab, display, in_gamut, fixed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmanaged_round_trips() {
        let colors = Colors::unmanaged();
        let p = pick(&colors, Space::Rgb, &[255.0, 0.0, 0.0]).unwrap();
        assert_eq!(p.display, [255, 0, 0]);
        assert_eq!(p.cmyk, [0.0, 100.0, 100.0, 0.0]);
        assert!(p.in_gamut && p.fixed.is_none());
        let c = pick(&colors, Space::Cmyk, &[0.0, 0.0, 0.0, 100.0]).unwrap();
        assert_eq!(c.rgb, [0.0, 0.0, 0.0]);
        assert!(c.lab[0].abs() < 0.01);
        let w = pick(&colors, Space::Lab, &[100.0, 0.0, 0.0]).unwrap();
        assert_eq!(w.display, [255, 255, 255]);
        assert!(pick(&colors, Space::Cmyk, &[1.0, 2.0, 3.0]).is_none());
        assert!(pick(&colors, Space::Rgb, &[f64::NAN, 0.0, 0.0]).is_none());
    }

    #[test]
    fn a_lab_colour_outside_srgb_is_out_of_gamut() {
        let p = pick(&Colors::unmanaged(), Space::Lab, &[50.0, 80.0, -80.0]).unwrap();
        assert!(!p.in_gamut);
        assert!(p.fixed.is_some());
    }
}
