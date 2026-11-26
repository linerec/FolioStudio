//! Resolves PDF presets, output intent profiles, and colour conversion.

use crate::{ExportError, ExportOptions, Preset, images, meta};
use folio_model::Document;
use std::path::Path;
use std::sync::Arc;

pub(super) struct OutputSettings<'a> {
    pub(super) profile_path: &'a Path,
    pub(super) profile: Vec<u8>,
    pub(super) facts: meta::IccFacts,
    pub(super) rgb_profile: Option<Vec<u8>>,
    pub(super) preset: Preset,
    pub(super) flattening: bool,
    pub(super) cmyk_only: bool,
    pub(super) extra_warnings: Vec<(String, String)>,
    pub(super) colors: Option<folio_color::Colors>,
    pub(super) conv: Option<Arc<images::Conv>>,
}

impl<'a> OutputSettings<'a> {
    pub(super) fn resolve(doc: &Document, opts: &'a ExportOptions) -> Result<Self, ExportError> {
        // Output intent: the document CMYK profile from this machine.
        let profile_path = opts.output_profile.as_ref().ok_or_else(|| {
            ExportError::OutputProfile(format!(
                "no CMYK profile for {:?}: pass --output-profile",
                doc.print.cmyk_profile.as_deref().unwrap_or("(none)")
            ))
        })?;
        let profile = std::fs::read(profile_path).map_err(|e| ExportError::OutputProfile(format!("{}: {e}", profile_path.display())))?;
        let facts = meta::icc_facts(&profile).ok_or_else(|| ExportError::OutputProfile(format!("{} is not an ICC profile", profile_path.display())))?;
        if &facts.space != b"CMYK" {
            return Err(ExportError::OutputProfile(format!("{} is not a CMYK profile", profile_path.display())));
        }
        let rgb_profile: Option<Vec<u8>> = opts.rgb_profile.as_ref().and_then(|p| std::fs::read(p).ok());
        let preset = opts.preset;
        let flattening = preset.flattens();
        let mut extra_warnings: Vec<(String, String)> = Vec::new();
        // PDF 1.3 holds ICC profiles up to version 2: PDF/X-3 with a newer document RGB
        // profile converts RGB colours as PDF/X-1a does.
        let rgb_v4 = rgb_profile.as_ref().is_some_and(|p| p.get(8).is_some_and(|v| *v >= 4));
        let cmyk_only = opts.cmyk_override.unwrap_or(preset.cmyk_only()) || (preset == Preset::X3 && rgb_v4);
        if preset == Preset::X3 && rgb_v4 {
            extra_warnings.push(("colour".into(), "the document RGB profile is newer than ICC version 2 (PDF 1.3 cannot hold it): RGB colour converted to CMYK".into()));
        }
        // The document's colour conversions: the flattener's blend space and PDF/X-1a's CMYK.
        let colors: Option<folio_color::Colors> = (flattening || cmyk_only).then(|| match &opts.colors {
            Some(c) => c.clone(),
            None => folio_color::Colors::from_profiles(Some(&profile), rgb_profile.as_deref()).unwrap_or_else(|e| {
                extra_warnings.push(("colour".into(), format!("profiles not usable for conversion ({e}); converted without colour management")));
                folio_color::Colors::unmanaged()
            }),
        });
        let conv: Option<Arc<images::Conv>> =
            colors.as_ref().map(|c| Arc::new(images::Conv::new(c.clone(), Arc::new(profile.clone()), rgb_profile.clone().map(Arc::new), cmyk_only)
                .with_cmyk_sources(opts.cmyk_override.unwrap_or(preset.cmyk_only()))));
        Ok(Self {
            profile_path,
            profile,
            facts,
            rgb_profile,
            preset,
            flattening,
            cmyk_only,
            extra_warnings,
            colors,
            conv,
        })
    }
}
