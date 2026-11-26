//! Ink planes shown on screen, as InDesign's Overprint Preview and Separations Preview show
//! them: every plate combined through the document CMYK profile (spot inks simulated by
//! their CMYK equivalent laid over the process inks), chosen plates only (a single plate in
//! black, InDesign's default "Show Single Plates in Black"; with that off, a single plate in
//! its own ink colour), or the ink limit (areas whose total ink exceeds the limit in red over
//! the page in gray).

use super::{DocInks, InkPlanes};
use folio_color::Colors;

#[derive(Clone, Debug, PartialEq)]
pub enum View {
    /// All plates as printed.
    Overprint,
    /// Only the plates marked visible.
    Separations { visible: Vec<bool> },
    /// Total area coverage above `limit` percent highlighted.
    InkLimit { limit: f64 },
}

/// The Separations Preview panel menu's switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// "Show Single Plates in Black" (on by default): a single visible plate is drawn as
    /// black ink on white; off, in its own ink colour.
    pub single_plates_black: bool,
    /// "Desaturate Black" (off by default): in the Separations view the black plate is shown at 80 % of its ink, as InDesign 2026 draws
    /// it (2025 not checked): K 100 % shows as K 80 % does, a 60/40/40/100 rich black as 60/40/40/80. Measured with every plate shown;
    /// a single shown plate is drawn as before.
    pub desaturate_black: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look { single_plates_black: true, desaturate_black: false }
    }
}

/// The black plate's share when Desaturate Black is on.
const DESATURATED_BLACK: f32 = 0.8;

/// The planes as display RGBA (opaque), with the panel menu's defaults.
pub fn composite(planes: &InkPlanes, inks: &DocInks, colors: &Colors, view: &View) -> Vec<u8> {
    composite_with(planes, inks, colors, view, Look::default())
}

/// The planes as display RGBA (opaque).
pub fn composite_with(planes: &InkPlanes, inks: &DocInks, colors: &Colors, view: &View, look: Look) -> Vec<u8> {
    let n = planes.count;
    let wh = planes.width as usize * planes.height as usize;
    let mut out = vec![255u8; wh * 4];
    match view {
        View::Overprint => combine(planes, inks, colors, &vec![true; n], 1.0, &mut out),
        View::Separations { visible } => {
            let shown: Vec<bool> = (0..n).map(|p| visible.get(p).copied().unwrap_or(false)).collect();
            match shown.iter().filter(|v| **v).count() {
                0 => {}
                1 if look.single_plates_black => {
                    let plane = planes.plane(shown.iter().position(|v| *v).unwrap_or(0));
                    for (px, &v) in out.chunks_exact_mut(4).zip(plane) {
                        let g = 255 - v;
                        px[..3].copy_from_slice(&[g, g, g]);
                    }
                }
                _ => combine(planes, inks, colors, &shown, if look.desaturate_black { DESATURATED_BLACK } else { 1.0 }, &mut out),
            }
        }
        View::InkLimit { limit } => {
            combine(planes, inks, colors, &vec![true; n], 1.0, &mut out);
            let over = (limit.max(0.0) * 255.0 / 100.0).round() as u32;
            for (i, px) in out.chunks_exact_mut(4).enumerate() {
                let total: u32 = (0..n).map(|p| planes.data[p * wh + i] as u32).sum();
                if total > over {
                    // light red just above the limit, darker the further above (100 % over: dark)
                    let f = ((total - over) as f32 / 255.0).min(1.0);
                    px[0] = (255.0 - 95.0 * f) as u8;
                    px[1] = (110.0 * (1.0 - f)) as u8;
                    px[2] = (110.0 * (1.0 - f)) as u8;
                } else {
                    let y = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32).round() as u8;
                    px[..3].copy_from_slice(&[y, y, y]);
                }
            }
        }
    }
    out
}

/// The `shown` plates combined: spot coverage laid over the process inks with the spot's
/// CMYK equivalent (1 − (1 − ink) · (1 − equivalent · coverage)), then CMYK through the
/// document profile.
fn combine(planes: &InkPlanes, inks: &DocInks, colors: &Colors, shown: &[bool], black: f32, out: &mut [u8]) {
    let n = planes.count;
    let wh = planes.width as usize * planes.height as usize;
    let d = &planes.data;
    let spots: Vec<(usize, [f32; 4])> = (4..n).filter(|p| shown[*p]).map(|p| (p, inks.inks.get(p).map_or([0.0, 0.0, 0.0, 1.0], |k| k.cmyk))).collect();
    let mut last: Option<([u8; 4], [u8; 3])> = None;
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        let mut c = [0u8; 4];
        for k in 0..4 {
            if shown[k] {
                c[k] = d[k * wh + i];
            }
        }
        if black != 1.0 {
            c[3] = (c[3] as f32 * black).round() as u8;
        }
        if spots.iter().any(|(p, _)| d[p * wh + i] > 0) {
            let mut f = c.map(|v| v as f32 / 255.0);
            for (p, eq) in &spots {
                let t = d[p * wh + i] as f32 / 255.0;
                if t > 0.0 {
                    for k in 0..4 {
                        f[k] = 1.0 - (1.0 - f[k]) * (1.0 - eq[k] * t);
                    }
                }
            }
            c = f.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        let rgb = match last {
            Some((k, rgb)) if k == c => rgb,
            _ => {
                let rgb = if c == [0; 4] { [255; 3] } else { colors.cmyk8_to_srgb(c[0], c[1], c[2], c[3]) };
                last = Some((c, rgb));
                rgb
            }
        };
        px[..3].copy_from_slice(&rgb);
    }
}
