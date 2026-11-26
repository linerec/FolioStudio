//! Swatches and gradients from Resources/Graphic.xml, and paint resolution.
//!
//! `display_rgb` is a profile-free approximation of the swatch (`folio_color::Colors::
//! unmanaged`): CMYK as 1 − ink, RGB as stored, Lab converted (D50), gray as ink, tints
//! applied in ink space for CMYK; Paper is white. The colour InDesign shows depends on the
//! document's profiles on this machine and is resolved for display (`folio_color::Colors`
//! from `document_colors`), not here.
//!
//! Spot colours display through their own `Space`/`ColorValue` (the alternate InDesign uses).

use crate::props::{Props, clean_name, is_none_ref, nums};
use crate::warn::Warnings;
use crate::xml::Element;
use folio_color::BlendSpace;
use folio_model::{ColorModel, ColorSpace, Gradient, GradientKind, GradientStop, Id, Paint, Swatch};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Colors {
    pub swatches: BTreeMap<Id, Swatch>,
    pub gradients: BTreeMap<Id, Gradient>,
    /// Flat stand-in for each gradient (mean of its stop colours).
    gradient_rgb: BTreeMap<Id, [u8; 3]>,
}

fn clamp_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Whether `values` are usable for `space`.
fn usable(space: ColorSpace, v: &[f64]) -> bool {
    match space {
        ColorSpace::Cmyk => v.len() >= 4,
        ColorSpace::Rgb | ColorSpace::Lab => v.len() >= 3,
        ColorSpace::Gray => !v.is_empty(),
    }
}

/// Display colour of a swatch from its own values (see the module comment).
fn display_of(s: &Swatch) -> [u8; 3] {
    folio_color::Colors::unmanaged().swatch_rgb(s, 100.0, BlendSpace::Rgb).unwrap_or([255, 255, 255])
}

fn space_of(s: &str) -> Option<ColorSpace> {
    match s {
        "CMYK" => Some(ColorSpace::Cmyk),
        "RGB" => Some(ColorSpace::Rgb),
        "LAB" => Some(ColorSpace::Lab),
        "Gray" | "GRAY" => Some(ColorSpace::Gray),
        _ => None,
    }
}

fn process_ink(name: &str) -> Option<usize> {
    match clean_name(name.strip_prefix("Ink/").unwrap_or(name)) {
        "Process Cyan" => Some(0),
        "Process Magenta" => Some(1),
        "Process Yellow" => Some(2),
        "Process Black" => Some(3),
        _ => None,
    }
}

impl Colors {
    pub fn read(roots: &[&Element<'_>], w: &mut Warnings) -> Colors {
        let mut c = Colors::default();
        let mut all: Vec<&Element<'_>> = Vec::new();
        for r in roots {
            all.extend(r.elements());
        }
        for e in &all {
            match e.name {
                "Color" => c.read_color(e, w),
                "Swatch" => {
                    if let Some(id) = e.attr("Self") {
                        let none = id == "Swatch/None";
                        if !none {
                            w.add(format!("swatch {id} is not a colour; kept as None"));
                        }
                        c.swatches.insert(
                            id.to_owned(),
                            Swatch {
                                id: id.to_owned(),
                                name: clean_name(e.attr("Name").unwrap_or("None")).to_owned(),
                                model: ColorModel::None,
                                space: ColorSpace::Rgb,
                                values: Vec::new(),
                                base: None,
                                tint: None,
                                display_rgb: [255, 255, 255],
                            },
                        );
                    }
                }
                _ => {}
            }
        }
        // Mixed inks and tints refer to colours read above.
        for e in &all {
            if e.name == "MixedInk" {
                c.read_mixed_ink(e, w);
            }
        }
        for e in &all {
            if e.name == "Tint" {
                c.read_tint(e, w);
            }
        }
        for e in &all {
            match e.name {
                "Gradient" => c.read_gradient(e, w),
                "Color" | "Swatch" | "Tint" | "MixedInk" | "Ink" | "StrokeStyle" | "DashedStrokeStyle"
                | "DottedStrokeStyle" | "StripedStrokeStyle" | "ColorGroup" | "Properties" | "MixedInkGroup" => {}
                "PastedSmoothShade" => w.add(format!("{} swatch not modelled", e.name)),
                other => w.add(format!("Graphic.xml element {other} not read")),
            }
        }
        c
    }

    fn read_color(&mut self, e: &Element<'_>, w: &mut Warnings) {
        let Some(id) = e.attr("Self") else { return };
        let name = clean_name(e.attr("Name").unwrap_or(id)).to_owned();
        let paper = id == "Color/Paper" || e.attr("ColorOverride") == Some("Specialpaper");
        let model = match e.attr("Model") {
            _ if paper => ColorModel::Paper,
            Some("Registration") => ColorModel::Registration,
            Some("Spot") => ColorModel::Spot,
            Some("MixedInkModel") => ColorModel::Mixed,
            Some("Process") | None => ColorModel::Process,
            Some(other) => {
                w.add(format!("colour model {other} read as Process"));
                ColorModel::Process
            }
        };
        let values = e.str("ColorValue").and_then(nums).unwrap_or_default();
        let space = match e.attr("Space").and_then(space_of) {
            Some(s) => s,
            None => {
                w.add(format!("colour space {:?} not supported (read as CMYK)", e.attr("Space").unwrap_or("")));
                ColorSpace::Cmyk
            }
        };
        if !matches!(model, ColorModel::Paper | ColorModel::Registration) && !usable(space, &values) {
            w.add(format!("colour {id} has no usable values; shown grey"));
        }
        let mut sw = Swatch { id: id.to_owned(), name, model, space, values, base: None, tint: None, display_rgb: [128, 128, 128] };
        sw.display_rgb = display_of(&sw);
        self.swatches.insert(id.to_owned(), sw);
    }

    fn read_mixed_ink(&mut self, e: &Element<'_>, w: &mut Warnings) {
        let Some(id) = e.attr("Self") else { return };
        let name = clean_name(e.attr("Name").unwrap_or(id)).to_owned();
        let own = e.str("ColorValue").and_then(nums);
        let (space, values) = match (e.attr("Space").and_then(space_of), own) {
            (Some(s), Some(v)) if usable(s, &v) => (s, v),
            _ => {
                // Combine the inks: process inks add to CMYK, spot inks add their CMYK alternate.
                let inks: Vec<&str> = e
                    .get("InkList")
                    .map(|v| match v {
                        crate::props::Val::Str(s) => s.split_whitespace().collect(),
                        crate::props::Val::Elem(l) => {
                            l.children_named("ListItem").filter_map(|i| i.children.first()).filter_map(|n| match n {
                                crate::xml::Node::Text(t) => Some(t.as_ref()),
                                _ => None,
                            })
                            .collect()
                        }
                    })
                    .unwrap_or_default();
                let amounts = e.get("InkValues").and_then(|v| v.f64s()).unwrap_or_default();
                let mut cmyk = [0.0f64; 4];
                for (ink, amount) in inks.iter().zip(amounts.iter()) {
                    if let Some(ch) = process_ink(ink) {
                        cmyk[ch] += amount;
                    } else {
                        let spot = clean_name(ink.strip_prefix("Ink/").unwrap_or(ink));
                        match self.swatches.values().find(|s| s.name == spot && s.space == ColorSpace::Cmyk) {
                            Some(s) => {
                                for (k, v) in s.values.iter().enumerate().take(4) {
                                    cmyk[k] += v * amount / 100.0;
                                }
                            }
                            None => w.add(format!("mixed ink {id}: ink {ink} has no CMYK alternate")),
                        }
                    }
                }
                (ColorSpace::Cmyk, cmyk.iter().map(|v| v.min(100.0)).collect())
            }
        };
        let mut sw =
            Swatch { id: id.to_owned(), name, model: ColorModel::Mixed, space, values, base: None, tint: None, display_rgb: [128, 128, 128] };
        sw.display_rgb = display_of(&sw);
        self.swatches.insert(id.to_owned(), sw);
    }

    fn read_tint(&mut self, e: &Element<'_>, w: &mut Warnings) {
        let Some(id) = e.attr("Self") else { return };
        let base_id = e.attr("BaseColor").unwrap_or("");
        let tint = e.f64("TintValue").filter(|t| (0.0..=100.0).contains(t)).unwrap_or(100.0);
        let Some(base) = self.swatches.get(base_id) else {
            w.add(format!("tint {id}: base colour {base_id} missing"));
            return;
        };
        let mut s = Swatch {
            id: id.to_owned(),
            name: clean_name(e.attr("Name").unwrap_or(id)).to_owned(),
            model: base.model,
            space: base.space,
            values: base.values.clone(),
            base: Some(base_id.to_owned()),
            tint: Some(tint),
            display_rgb: base.display_rgb,
        };
        s.display_rgb = display_of(&s);
        self.swatches.insert(id.to_owned(), s);
    }

    fn read_gradient(&mut self, e: &Element<'_>, w: &mut Warnings) {
        let Some(id) = e.attr("Self") else { return };
        let kind = match e.attr("Type") {
            Some("Radial") => GradientKind::Radial,
            Some("Linear") | None => GradientKind::Linear,
            Some(other) => {
                w.add(format!("gradient type {other} read as Linear"));
                GradientKind::Linear
            }
        };
        let mut stops = Vec::new();
        let mut sum = [0.0f64; 3];
        for s in e.children_named("GradientStop") {
            let color = s.attr("StopColor").unwrap_or("").to_owned();
            match self.swatches.get(&color) {
                Some(sw) => {
                    for (k, c) in sw.display_rgb.iter().enumerate() {
                        sum[k] += *c as f64;
                    }
                }
                None => w.add(format!("gradient {id}: stop colour {color} missing")),
            }
            stops.push(GradientStop {
                color,
                location: s.f64("Location").unwrap_or(0.0),
                midpoint: s.f64("Midpoint").unwrap_or(50.0),
            });
        }
        let n = stops.len().max(1) as f64;
        self.gradient_rgb.insert(id.to_owned(), sum.map(|v| clamp_u8(v / n)));
        self.gradients.insert(
            id.to_owned(),
            Gradient { id: id.to_owned(), name: clean_name(e.attr("Name").unwrap_or(id)).to_owned(), kind, stops },
        );
    }

    /// Paint for a `FillColor`/`StrokeColor` reference and its tint (−1 or absent = 100).
    pub fn paint(&self, color: Option<&str>, tint: Option<f64>, w: &mut Warnings) -> Paint {
        let Some(id) = color else { return Paint::None };
        if is_none_ref(id) || id == "Swatch/None" {
            return Paint::None;
        }
        if self.gradients.contains_key(id) {
            let fallback_rgb = self.gradient_rgb.get(id).copied().unwrap_or([128, 128, 128]);
            return Paint::Gradient { id: id.to_owned(), fallback_rgb };
        }
        if self.swatches.contains_key(id) {
            let tint = tint.filter(|t| (0.0..=100.0).contains(t)).unwrap_or(100.0);
            return Paint::Swatch { id: id.to_owned(), tint };
        }
        w.add(format!("missing swatch {id} (painted as None)"));
        Paint::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swatch(space: ColorSpace, values: &[f64], tint: Option<f64>) -> Swatch {
        Swatch {
            id: "c".into(),
            name: "c".into(),
            model: ColorModel::Process,
            space,
            values: values.to_vec(),
            base: None,
            tint,
            display_rgb: [0, 0, 0],
        }
    }

    #[test]
    fn conversions() {
        // CMYK without a profile: 1 − ink.
        assert_eq!(display_of(&swatch(ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0], None)), [0, 0, 0]);
        assert_eq!(display_of(&swatch(ColorSpace::Cmyk, &[100.0, 0.0, 0.0, 0.0], None)), [0, 255, 255]);
        // A CMYK tint scales the ink: 50 % black is K50, not black blended with white.
        assert_eq!(
            display_of(&swatch(ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 100.0], Some(50.0))),
            display_of(&swatch(ColorSpace::Cmyk, &[0.0, 0.0, 0.0, 50.0], None))
        );
        // Lab white and black, mid grey L=50 about 119 in sRGB.
        assert_eq!(display_of(&swatch(ColorSpace::Lab, &[100.0, 0.0, 0.0], None)), [255, 255, 255]);
        assert_eq!(display_of(&swatch(ColorSpace::Lab, &[0.0, 0.0, 0.0], None)), [0, 0, 0]);
        let g = display_of(&swatch(ColorSpace::Lab, &[50.0, 0.0, 0.0], None));
        assert!((g[0] as i32 - 119).abs() <= 1 && g[0] == g[1] && g[1] == g[2], "{g:?}");
        // RGB tints blend towards white.
        assert_eq!(display_of(&swatch(ColorSpace::Rgb, &[0.0, 0.0, 0.0], Some(50.0))), [128, 128, 128]);
    }
}
