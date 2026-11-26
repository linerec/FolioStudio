//! What a paint puts on press: process inks, spot inks (with the Ink Manager's
//! convert-to-process and alias), registration, paper, or RGB / Lab that must be converted.

use folio_model::{ColorModel, ColorSpace, Document, Ink, Paint, Swatch};
use std::collections::HashMap;

/// Ink a paint separates into.
#[derive(Clone, Debug, PartialEq)]
pub enum Inks {
    None,
    /// Paper (white): prints nothing.
    Paper,
    /// Registration: every plate at 100 % (× tint).
    Registration(f64),
    /// Process CMYK percentages (after tints).
    Process([f64; 4]),
    /// A spot ink at `tint` %; `process` = its CMYK when the Ink Manager converts it.
    Spot { name: String, tint: f64, process: Option<[f64; 4]> },
    /// Process RGB / Lab colour (converted on output); `values` as stored, `tint` applied.
    Device { space: ColorSpace, values: Vec<f64>, tint: f64 },
    /// Several inks (mixed ink swatch): process part + spots.
    Mixed { process: [f64; 4], spots: Vec<(String, f64)> },
    /// Gradient: the stops' inks.
    Gradient(Vec<Inks>),
    /// Swatch not found.
    Unknown,
}

impl Inks {
    /// Total area coverage (percent) when it can be computed in CMYK + spots.
    pub fn coverage(&self) -> Option<f64> {
        match self {
            Inks::None | Inks::Paper => Some(0.0),
            Inks::Registration(t) => Some(4.0 * t),
            Inks::Process(v) => Some(v.iter().sum()),
            Inks::Spot { tint, process: Some(p), .. } => Some(p.iter().sum::<f64>() * tint / 100.0),
            Inks::Spot { tint, process: None, .. } => Some(*tint),
            Inks::Mixed { process, spots } => Some(process.iter().sum::<f64>() + spots.iter().map(|s| s.1).sum::<f64>()),
            Inks::Gradient(stops) => stops.iter().filter_map(Inks::coverage).reduce(f64::max),
            Inks::Device { .. } | Inks::Unknown => None,
        }
    }

    /// Paints white: paper, CMYK 0, RGB white, Lab L100, or a 0 % tint.
    pub fn is_white(&self) -> bool {
        match self {
            Inks::Paper => true,
            Inks::Process(v) => v.iter().all(|c| *c <= 0.0),
            Inks::Spot { tint, .. } => *tint <= 0.0,
            Inks::Device { space: ColorSpace::Rgb, values, tint } => *tint <= 0.0 || values.iter().take(3).all(|v| *v >= 254.5),
            Inks::Device { space: ColorSpace::Lab, values, tint } => *tint <= 0.0 || values.first().is_some_and(|l| *l >= 99.5),
            Inks::Device { space: ColorSpace::Gray, values, tint } => *tint <= 0.0 || values.first().is_some_and(|g| *g <= 0.0),
            _ => false,
        }
    }

    /// Only black ink (process K, any amount > 0; gray swatches are K).
    pub fn k_only(&self) -> bool {
        match self {
            Inks::Process(v) => v[0] <= 0.0 && v[1] <= 0.0 && v[2] <= 0.0 && v[3] > 0.0,
            Inks::Device { space: ColorSpace::Gray, values, tint } => *tint > 0.0 && values.first().is_some_and(|g| *g > 0.0),
            _ => false,
        }
    }

    /// Process inks with a non-zero amount (C, M, Y, K), plus spots.
    pub fn ink_count(&self) -> usize {
        match self {
            Inks::Process(v) => v.iter().filter(|c| **c > 0.0).count(),
            Inks::Registration(_) => 4,
            Inks::Spot { process: Some(p), tint, .. } if *tint > 0.0 => p.iter().filter(|c| **c > 0.0).count(),
            Inks::Spot { process: Some(_), .. } => 0,
            Inks::Spot { tint, .. } => usize::from(*tint > 0.0),
            Inks::Mixed { process, spots } => process.iter().filter(|c| **c > 0.0).count() + spots.len(),
            // converted on output: separates into several process inks
            Inks::Device { space, tint, .. } if *space != ColorSpace::Gray && *tint > 0.0 => 4,
            _ => 0,
        }
    }

    /// Spot ink names used.
    pub fn spots(&self, out: &mut Vec<String>) {
        match self {
            Inks::Spot { name, .. } => out.push(name.clone()),
            Inks::Mixed { spots, .. } => out.extend(spots.iter().map(|s| s.0.clone())),
            Inks::Gradient(g) => g.iter().for_each(|i| i.spots(out)),
            _ => {}
        }
    }

    /// RGB / Lab / other device space that needs conversion for CMYK output (including a
    /// converted spot defined in RGB / Lab).
    pub fn device_space(&self) -> Option<ColorSpace> {
        match self {
            Inks::Device { space, .. } if *space != ColorSpace::Gray && *space != ColorSpace::Cmyk => Some(*space),
            Inks::Gradient(g) => g.iter().find_map(Inks::device_space),
            _ => None,
        }
    }
}

/// Ink resolution for one document.
pub struct InkTable<'d> {
    pub doc: &'d Document,
    inks: HashMap<String, &'d Ink>,
}

fn cmyk(v: &[f64]) -> [f64; 4] {
    [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0), v.get(3).copied().unwrap_or(0.0)]
}

impl<'d> InkTable<'d> {
    pub fn new(doc: &'d Document) -> Self {
        InkTable { doc, inks: doc.print.inks.iter().map(|i| (i.name.clone(), i)).collect() }
    }

    pub fn ink(&self, name: &str) -> Option<&'d Ink> {
        self.inks.get(name).copied()
    }

    /// The spot's own name after Ink Manager aliasing.
    fn spot_name(&self, name: &str) -> String {
        let mut n = name.to_owned();
        for _ in 0..8 {
            match self.inks.get(n.as_str()).and_then(|i| i.alias.clone()) {
                Some(a) if a != n => n = a,
                _ => break,
            }
        }
        n
    }

    /// Process values of a swatch (its own values when CMYK; `None` otherwise).
    fn process_values(s: &Swatch) -> Option<[f64; 4]> {
        match s.space {
            ColorSpace::Cmyk => Some(cmyk(&s.values)),
            ColorSpace::Gray => Some([0.0, 0.0, 0.0, s.values.first().copied().unwrap_or(0.0)]),
            _ => None,
        }
    }

    pub fn swatch_inks(&self, s: &Swatch, extra_tint: f64) -> Inks {
        // A tint swatch keeps the base's values; `tint` scales them.
        let tint = s.tint.unwrap_or(100.0) * extra_tint / 100.0;
        let scale = |v: [f64; 4]| v.map(|c| c * tint / 100.0);
        match s.model {
            ColorModel::None => Inks::None,
            ColorModel::Paper => Inks::Paper,
            ColorModel::Registration => Inks::Registration(tint),
            ColorModel::Spot => {
                // A tint of a spot separates on the base colour's plate.
                let base = s.base.as_ref().and_then(|b| self.doc.swatches.get(b)).map_or(s.name.as_str(), |b| b.name.as_str());
                let convert = self.ink(base).is_some_and(|i| i.convert_to_process);
                let name = self.spot_name(base);
                if convert {
                    match Self::process_values(s) {
                        Some(p) => Inks::Spot { name, tint, process: Some(p) },
                        None => Inks::Device { space: s.space, values: s.values.clone(), tint },
                    }
                } else {
                    Inks::Spot { name, tint, process: None }
                }
            }
            ColorModel::Mixed => match Self::process_values(s) {
                Some(p) => Inks::Process(scale(p)),
                None => Inks::Device { space: s.space, values: s.values.clone(), tint },
            },
            ColorModel::Process => match s.space {
                ColorSpace::Cmyk => Inks::Process(scale(cmyk(&s.values))),
                ColorSpace::Gray => Inks::Process(scale([0.0, 0.0, 0.0, s.values.first().copied().unwrap_or(0.0)])),
                other => Inks::Device { space: other, values: s.values.clone(), tint },
            },
        }
    }

    pub fn paint(&self, p: &Paint) -> Inks {
        match p {
            Paint::None => Inks::None,
            Paint::Swatch { id, tint } => match self.doc.swatches.get(id) {
                Some(s) => self.swatch_inks(s, *tint),
                None => Inks::Unknown,
            },
            Paint::Gradient { id, .. } => match self.doc.gradients.get(id) {
                Some(g) => Inks::Gradient(
                    g.stops.iter().map(|st| self.doc.swatches.get(&st.color).map(|s| self.swatch_inks(s, 100.0)).unwrap_or(Inks::Unknown)).collect(),
                ),
                None => Inks::Unknown,
            },
        }
    }

}

/// Display name of a paint for messages.
pub fn paint_name(doc: &Document, p: &Paint) -> String {
    match p {
        Paint::None => "없음".into(),
        Paint::Swatch { id, tint } => {
            let n = doc.swatches.get(id).map(|s| s.name.clone()).unwrap_or_else(|| id.clone());
            if (*tint - 100.0).abs() > 0.01 { format!("{n} {tint:.0}%") } else { n }
        }
        Paint::Gradient { id, .. } => doc.gradients.get(id).map(|g| g.name.clone()).unwrap_or_else(|| id.clone()),
    }
}
