//! Export options and the JSON report.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Which PDF/X flavour to write, after InDesign 2025's built-in presets (read from InDesign
/// by script, artifacts/next-20261018/pdfx1a/indesign/presets.json):
///
/// | | "[PDF/X-4:2008]" | "[PDF/X-1a:2001]" | "[PDF/X-3:2002]" |
/// |---|---|---|---|
/// | PDF version (compatibility) | 1.6 (Acrobat 7) | 1.3 (Acrobat 4) | 1.3 (Acrobat 4) |
/// | colour conversion | none | Convert to Destination (Preserve Numbers), destination the document CMYK | none |
/// | profiles included | tagged RGB | none | tagged RGB |
/// | transparency | live | flattened, "[High Resolution]" | flattened, "[High Resolution]" |
/// | output intent | document CMYK | document CMYK | document CMYK |
///
/// All three downsample colour and gray images above 450 ppi to 300 (bicubic) and compress
/// them "Automatic (JPEG), Maximum" (JPEG quality 95). "[High Resolution]": raster/vector
/// balance 100, line art and text 1200 ppi, gradients and mesh 300 ppi, complex regions
/// clipped, text and strokes not outlined.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
    #[default]
    X4,
    X1a,
    X3,
    High,
    Press,
    Small,
}

/// The transparency flattener's resolutions (InDesign's "[High Resolution]" preset).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FlattenSettings {
    /// Areas where vector edges (line art, text) are rasterised, pixels per inch.
    pub line_art_ppi: f64,
    /// Areas of smooth content (images, gradients, effects), pixels per inch; effect masks
    /// are computed at this resolution.
    pub smooth_ppi: f64,
}

impl Default for FlattenSettings {
    fn default() -> Self {
        FlattenSettings { line_art_ppi: 1200.0, smooth_ppi: 300.0 }
    }
}

/// Printer's marks, drawn in the Registration colour (Separation /All).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Marks {
    pub crop: bool,
    pub bleed: bool,
    pub registration: bool,
    pub page_info: bool,
}

impl Marks {
    pub fn any(&self) -> bool {
        self.crop || self.bleed || self.registration || self.page_info
    }
}

/// Export Layers (InDesign's `exportWhichLayers`, measured in InDesign 2025: oracle/indesign/buildc2-ocg-probe.jsx). Without
/// Acrobat layers it picks the layers whose content is written; with them every layer's content is written and it picks the
/// layers that are on in the PDF's default view (the others are off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportLayers {
    /// Visible & Printable Layers (InDesign's default).
    #[default]
    VisiblePrintable,
    Visible,
    All,
}

impl ExportLayers {
    pub fn includes(self, visible: bool, printable: bool) -> bool {
        match self {
            ExportLayers::VisiblePrintable => visible && printable,
            ExportLayers::Visible => visible,
            ExportLayers::All => true,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "visiblePrintable" => Some(ExportLayers::VisiblePrintable),
            "visible" => Some(ExportLayers::Visible),
            "all" => Some(ExportLayers::All),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub preset: Preset,
    /// Create Acrobat Layers: each document layer becomes an optional content group (PDF 1.5 and later; PDF/X-1a and PDF/X-3 are
    /// PDF 1.3 and leave it off). [High Quality Print] and [Press Quality] are written as PDF 1.5 then.
    pub acrobat_layers: bool,
    pub export_layers: ExportLayers,
    /// Use the document bleed (BleedBox = page + bleed). Off in InDesign's X-4 preset.
    pub bleed: bool,
    pub marks: Marks,
    /// Top, bottom, inside, outside in points; overrides the document bleed when set.
    pub custom_bleed: Option<[f64; 4]>,
    /// Distance of crop/bleed marks from the trim, points (InDesign: 0.0833 in).
    pub mark_offset: f64,
    /// Line weight of marks, points (InDesign default 0.25 pt).
    pub mark_weight: f64,
    /// One PDF page per spread instead of per page.
    pub spreads: bool,
    /// The output intent profile (the document CMYK profile). Required: never bundled.
    pub output_profile: Option<PathBuf>,
    /// RGB profile for untagged RGB content (the document RGB profile, usually sRGB).
    pub rgb_profile: Option<PathBuf>,
    /// Folders searched for linked files the document's paths do not reach.
    pub search_roots: Vec<PathBuf>,
    /// Folder of the source document (relative links, "Links" folder).
    pub document_dir: Option<PathBuf>,
    /// Shown in the report and page information mark.
    pub document_name: String,
    /// The identity of the document (its XMP `xmpMM:OriginalDocumentID`), written into every page as `PieceInfo/InDesign/OriginalDocumentID`
    /// as InDesign does: File > Import PDF Comments checks that the PDF came from the document it is imported into.
    pub document_id: Option<String>,
    pub title: Option<String>,
    /// Only these document page indices (0-based); `None` = all.
    pub pages: Option<Vec<usize>>,
    /// Preserve the requested order (printer jobs); ordinary PDF export stays in document order.
    pub ordered_pages: bool,
    /// Output pages of hidden spreads too. InDesign's Print does (measured); its PDF export leaves them out.
    pub include_hidden_spreads: bool,
    /// Print Booklet: the sheets (`printing::booklet::layout`) the pages are placed on, one PDF page per sheet. Each page is drawn at its
    /// place, clipped to the page. `pages`, `spreads` and the marks are not used then. PDF/X-4 only (no flattening, no marks).
    pub booklet: Option<Vec<crate::printing::booklet::LaidSheet>>,
    /// Worker threads for encoding (0 = available parallelism).
    pub threads: usize,
    /// Flate level for content streams and images (0–9).
    pub compression: u32,
    pub image_compression: String,
    pub jpeg_quality: u8,
    pub cmyk_override: Option<bool>,
    /// Downsample colour and gray images above the first ppi to the second (InDesign's
    /// PDF/X-4 preset: above 450 to 300). `None` keeps every pixel.
    pub downsample: Option<(f64, f64)>,
    /// Memory allowed for images being decoded at once (bytes).
    pub image_memory: u64,
    /// Transparency flattening (PDF/X-1a and PDF/X-3).
    pub flatten: FlattenSettings,
    /// The document's colour conversions (`folio_color::document_colors`), used by the
    /// flattener and the CMYK conversion of PDF/X-1a; built from the output and RGB
    /// profiles when absent.
    pub colors: Option<folio_color::Colors>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            preset: Preset::X4,
            acrobat_layers: false,
            export_layers: ExportLayers::VisiblePrintable,
            bleed: false,
            marks: Marks::default(),
            custom_bleed: None,
            mark_offset: 6.0,
            mark_weight: 0.25,
            spreads: false,
            output_profile: None,
            rgb_profile: None,
            search_roots: Vec::new(),
            document_dir: None,
            document_name: String::new(),
            document_id: None,
            title: None,
            pages: None,
            ordered_pages: false,
            include_hidden_spreads: false,
            booklet: None,
            threads: 0,
            compression: 6,
            image_compression: "legacy".into(),
            jpeg_quality: 95,
            cmyk_override: None,
            downsample: Some((450.0, 300.0)),
            image_memory: 1 << 30,
            flatten: FlattenSettings::default(),
            colors: None,
        }
    }
}

/// An aggregated problem: one entry per kind and message, with where it happened.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Warning {
    /// font, link, effect, colour, text, image, structure, …
    pub kind: String,
    pub message: String,
    pub count: usize,
    /// Document page indices (0-based), first few.
    pub pages: Vec<usize>,
    /// Item ids, first few.
    pub items: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FontReport {
    /// What the document asked for.
    pub family: String,
    pub style: String,
    /// Face embedded instead (or exactly).
    pub embedded: Option<String>,
    pub exact: bool,
    pub glyphs: usize,
    pub characters: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct EmbeddedFont {
    pub postscript: String,
    pub file: String,
    /// CIDFontType0C (CFF) or CIDFontType2 (TrueType).
    pub format: String,
    pub glyphs: usize,
    pub bytes: usize,
    /// Glyphs drawn as .notdef (missing from the font).
    pub notdef: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LinkUsage {
    pub uri: String,
    /// original, preview, none
    pub used: String,
    /// How the original was found (folio-links), or why the preview was used.
    pub how: String,
    pub path: Option<String>,
    pub pages: Vec<usize>,
    pub placements: usize,
    /// Image format written: DCT passthrough, Flate, Form XObject (PDF), …
    pub written: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct InkReport {
    pub name: String,
    /// process, spot
    pub kind: String,
    /// "own plate", "converted to process", "alias of …", "not printed"
    pub output: String,
    pub uses: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct OutputIntentReport {
    pub profile: String,
    pub description: String,
    pub identifier: String,
    pub bytes: usize,
    /// ICC profile class and version.
    pub class: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Counts {
    pub pages: usize,
    pub items: usize,
    pub glyphs: usize,
    /// Glyphs with a K-only fill (black-tint text separates on the black plate alone).
    pub black_only_glyphs: usize,
    pub black_overprint_glyphs: usize,
    /// Fills / strokes drawn with overprint: the object's own setting or [Black] at 100 %.
    pub overprint_fills: usize,
    pub overprint_strokes: usize,
    /// Of those, the ones whose object sets Overprint Fill / Overprint Stroke itself.
    pub explicit_overprint_fills: usize,
    pub explicit_overprint_strokes: usize,
    pub transparency_groups: usize,
    pub pages_with_transparency: usize,
    pub images: usize,
    pub image_placements: usize,
    pub imported_pdf_pages: usize,
    pub shadings: usize,
    pub separation_spaces: usize,
    pub devicen_spaces: usize,
    /// Glyphs the layout placed in all frames (drawn or not).
    pub layout_glyphs: usize,
    pub glyphs_without_paint: usize,
    pub glyphs_without_font: usize,
    pub text_frames_drawn: usize,
    pub items_culled: usize,
    /// Drop shadows, glows and feathers written as soft masks.
    pub effects: usize,
}

/// What the transparency flattener did.
#[derive(Clone, Debug, Default, Serialize)]
pub struct FlattenReport {
    /// Output pages that had transparency and were flattened.
    pub pages: usize,
    /// Their document page indices (first pages of spreads), first 200.
    pub page_list: Vec<usize>,
    /// Transparent objects (shadows, glows, feathers, opacity, blend modes, inner effects,
    /// partial transparency, images with alpha, placed PDFs with transparency).
    pub objects: usize,
    /// Connected areas rasterised.
    pub regions: usize,
    /// Raster images written for them, and how many are at the line-art resolution.
    pub images: usize,
    pub line_art_images: usize,
    /// Rasterised area, square points, at each resolution.
    pub smooth_area_pt2: f64,
    pub line_art_area_pt2: f64,
    pub line_art_ppi: f64,
    pub smooth_ppi: f64,
    /// Compressed raster bytes.
    pub bytes: u64,
    /// Units of paint drawn as vector under a clip that leaves out the rasterised areas.
    pub clipped_units: usize,
    /// Placed graphics drawn in rasters from their preview (placed PDF / EPS vector content
    /// cannot be rasterised here).
    pub preview_graphics: usize,
    /// Rendering time summed over threads.
    pub render_ms: f64,
    /// Transparent objects by kind.
    pub kinds: BTreeMap<String, usize>,
    /// Per flattened output page: (document page index, objects, raster bytes).
    pub per_page: Vec<(usize, usize, u64)>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub ok: bool,
    /// "x4", "x1a" or "x3".
    pub preset: String,
    pub standard: String,
    pub output: String,
    pub bytes: u64,
    pub output_intent: OutputIntentReport,
    /// Image XObjects by how they were written (JPEG passthrough, decoded format, …).
    pub images_written: BTreeMap<String, usize>,
    pub counts: Counts,
    pub inks: Vec<InkReport>,
    pub fonts_missing: Vec<FontReport>,
    pub fonts_embedded: Vec<EmbeddedFont>,
    pub links: Vec<LinkUsage>,
    pub links_summary: BTreeMap<String, usize>,
    pub warnings: Vec<Warning>,
    pub timings_ms: BTreeMap<String, f64>,
    /// Transparency flattening (PDF/X-1a, PDF/X-3).
    pub flatten: Option<FlattenReport>,
    /// Colours and images converted to the destination CMYK (PDF/X-1a).
    pub converted: Option<ConvertReport>,
}

/// What PDF/X-1a's conversion to the destination CMYK changed.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ConvertReport {
    pub destination: String,
    /// RGB images converted (document RGB profile or their own).
    pub rgb_images: usize,
    pub rgb_images_own_profile: usize,
    /// Placed PDFs whose RGB, Lab or ICC-based colour was converted.
    pub placed_pdfs: usize,
}
