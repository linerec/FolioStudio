//! Portable PDF export settings. Runtime paths belong only to ExportOptions.
use crate::{ExportLayers, ExportOptions, Marks, Preset};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdfPreset {
    pub version: u32,
    pub name: String,
    pub standard: Preset,
    pub spreads: bool,
    pub use_document_bleed: bool,
    /// Top, bottom, inside, outside, in points.
    pub bleed: [f64; 4],
    pub marks: Marks,
    pub mark_offset: f64,
    pub mark_weight: f64,
    pub downsample: Option<Sampling>,
    pub compression: u32,
    #[serde(default = "default_image_compression")]
    pub image_compression: String,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u32,
    #[serde(default = "default_color_conversion")]
    pub color_conversion: String,
    pub subset_fonts_below: u32,
    pub flatten_line_art_ppi: f64,
    pub flatten_smooth_ppi: f64,
    /// Create Acrobat Layers (PDF 1.5 and later; not PDF/X-1a or PDF/X-3).
    #[serde(default)]
    pub acrobat_layers: bool,
    /// Export Layers: visiblePrintable (default), visible or all.
    #[serde(default)]
    pub export_layers: ExportLayers,
}

fn default_image_compression() -> String { "legacy".into() }
fn default_jpeg_quality() -> u32 { 95 }
fn default_color_conversion() -> String { "preset".into() }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sampling {
    pub above: f64,
    pub to: f64,
}

impl PdfPreset {
    pub fn builtin(standard: Preset) -> Self {
        Self {
            version: 1, name: standard.indesign_preset().into(), standard,
            spreads: false, use_document_bleed: false, bleed: [0.0; 4],
            marks: Marks::default(), mark_offset: 6.0, mark_weight: 0.25,
            downsample: Some(Sampling { above: 450.0, to: 300.0 }), compression: 6,
            image_compression: default_image_compression(), jpeg_quality: 95, color_conversion: default_color_conversion(),
            subset_fonts_below: 100, flatten_line_art_ppi: 1200.0, flatten_smooth_ppi: 300.0,
            acrobat_layers: false, export_layers: ExportLayers::VisiblePrintable,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 { return Err("unsupported PDF preset version".into()); }
        if self.name.trim().is_empty() || self.name.len() > 200 || self.name.chars().any(char::is_control) {
            return Err("invalid PDF preset name".into());
        }
        let range = |v: f64, lo: f64, hi: f64| v.is_finite() && v >= lo && v <= hi;
        if self.bleed.iter().any(|v| !range(*v, 0.0, 432.0)) || !range(self.mark_offset, 0.0, 72.0)
            || ![0.125, 0.25, 0.5].contains(&self.mark_weight) {
            return Err("invalid PDF bleed or printer marks".into());
        }
        if let Some(s) = &self.downsample
            && (!range(s.to, 9.0, 2400.0) || !range(s.above, s.to, 10000.0)) {
            return Err("invalid PDF image resolution".into());
        }
        if self.compression > 9 { return Err("invalid PDF compression level".into()); }
        if !["legacy", "zip", "jpeg"].contains(&self.image_compression.as_str()) || !(1..=100).contains(&self.jpeg_quality) {
            return Err("invalid PDF image compression".into());
        }
        if !["preset", "none", "documentCmyk"].contains(&self.color_conversion.as_str()) || (self.standard == Preset::X1a && self.color_conversion == "none") {
            return Err("invalid PDF color conversion (PDF/X-1a requires CMYK)".into());
        }
        if self.subset_fonts_below != 100 { return Err("font subset thresholds other than 100 are not supported".into()); }
        if self.flatten_line_art_ppi != 1200.0 || self.flatten_smooth_ppi != 300.0 {
            return Err("only the high resolution transparency flattener is supported".into());
        }
        if self.acrobat_layers && !self.standard.layers_allowed() {
            return Err("Acrobat layers need PDF 1.5 or later (PDF/X-1a and PDF/X-3 are PDF 1.3)".into());
        }
        Ok(())
    }

    pub fn apply(&self, mut base: ExportOptions) -> Result<ExportOptions, String> {
        self.validate()?;
        if self.standard == Preset::Small { return Err("smallest-file RGB conversion and separate sampling are not supported".into()); }
        base.preset = self.standard;
        base.spreads = self.spreads;
        base.bleed = self.use_document_bleed;
        base.custom_bleed = (!self.use_document_bleed).then_some(self.bleed);
        base.marks = self.marks;
        base.mark_offset = self.mark_offset;
        base.mark_weight = self.mark_weight;
        base.downsample = self.downsample.as_ref().map(|s| (s.above, s.to));
        base.compression = self.compression;
        base.image_compression = self.image_compression.clone();
        base.jpeg_quality = self.jpeg_quality as u8;
        base.acrobat_layers = self.acrobat_layers;
        base.export_layers = self.export_layers;
        base.cmyk_override = match self.color_conversion.as_str() { "none" => Some(false), "documentCmyk" => Some(true), _ => None };
        Ok(base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_round_trip_and_strict_boundary() {
        let p = PdfPreset::builtin(Preset::X4);
        let mut value = serde_json::to_value(&p).unwrap();
        assert_eq!(serde_json::from_value::<PdfPreset>(value.clone()).unwrap(), p);
        value["outputProfile"] = "C:/machine/profile.icc".into();
        assert!(serde_json::from_value::<PdfPreset>(value).is_err());
        let mut bad = p.clone(); bad.mark_offset = f64::NAN;
        assert!(bad.validate().is_err());
        bad = p.clone(); bad.subset_fonts_below = 0;
        assert!(bad.validate().is_err());
        bad = p.clone(); bad.downsample = Some(Sampling { above: 100.0, to: 300.0 });
        assert!(bad.validate().is_err());
        // Acrobat layers: PDF 1.5 and later only; presets saved before them read as off, visible & printable
        let mut layered = p.clone(); layered.acrobat_layers = true; layered.export_layers = ExportLayers::All;
        assert!(layered.validate().is_ok());
        let opts = layered.apply(ExportOptions::default()).unwrap();
        assert!(opts.acrobat_layers && opts.export_layers == ExportLayers::All);
        layered.standard = Preset::X1a;
        assert!(layered.validate().is_err());
        let mut old = serde_json::to_value(&p).unwrap();
        old.as_object_mut().unwrap().remove("acrobatLayers");
        old.as_object_mut().unwrap().remove("exportLayers");
        let read: PdfPreset = serde_json::from_value(old).unwrap();
        assert!(!read.acrobat_layers && read.export_layers == ExportLayers::VisiblePrintable);
    }
}
