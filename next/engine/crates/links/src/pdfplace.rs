//! PDF source selection and previews. The document renderer consumes the resulting bitmap;
//! print export keeps using the original PDF page.
mod bounds;
mod metadata;
pub mod previews;
pub mod layers;

use crate::probe::{ImageInfo, PdfPage};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::{Arc, atomic::{AtomicBool, Ordering}}};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Crop {
    CropContentVisibleLayers,
    CropContentAllLayers,
    CropArt,
    #[default]
    CropPDF,
    CropTrim,
    CropBleed,
    CropMedia,
}

impl Crop {
    pub fn name(self) -> &'static str {
        match self {
            Self::CropContentVisibleLayers => "CropContentVisibleLayers",
            Self::CropContentAllLayers => "CropContentAllLayers",
            Self::CropArt => "CropArt", Self::CropPDF => "CropPDF", Self::CropTrim => "CropTrim",
            Self::CropBleed => "CropBleed", Self::CropMedia => "CropMedia",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub page_number: u32,
    pub crop: Crop,
    pub transparent_background: bool,
}

impl Default for Options {
    fn default() -> Self { Self { page_number: 1, crop: Crop::CropPDF, transparent_background: true } }
}

impl TryFrom<&folio_model::PlacedPdf> for Options {
    type Error = String;
    fn try_from(p: &folio_model::PlacedPdf) -> Result<Self, String> {
        Ok(Self { page_number: p.page, crop: serde_json::from_value(serde_json::json!(p.crop)).map_err(|e| e.to_string())?, transparent_background: p.transparent_background })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(flatten)]
    pub options: Options,
    /// Graphic bounds measured from the PDF CropBox's lower left.
    pub bounds: [f64; 4],
    #[serde(default)]
    pub metadata: String,
    #[serde(default)]
    pub layers: Option<folio_model::graphic_layers::Options>,
}

fn valid_box(b: [f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite()) && b[2] > b[0] && b[3] > b[1]
}

/// A page's `/Rotate` (clockwise, a multiple of 90) turns the page before its boxes are measured: the graphic bounds are in the
/// turned page's y-up coordinates from the lower left of the turned crop box (InDesign 2025, placeimg-c1: pdf-rot90/180/270-click,
/// pdf-rot90-trim-click). `b` is a box in the page's own user space, `crop` the page's crop box in the same space.
pub fn turn(rotate: i32, crop: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let (wc, hc) = (crop[2] - crop[0], crop[3] - crop[1]);
    let (u0, v0, u1, v1) = (b[0] - crop[0], b[1] - crop[1], b[2] - crop[0], b[3] - crop[1]);
    match rotate.rem_euclid(360) {
        90 => [v0, wc - u1, v1, wc - u0],
        180 => [wc - u1, hc - v1, wc - u0, hc - v0],
        270 => [hc - v1, u0, hc - v0, u1],
        _ => [u0, v0, u1, v1],
    }
}

/// The inverse of [`turn`]: graphic bounds `b` as a box in the page's own user space.
pub fn unturn(rotate: i32, crop: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let (wc, hc) = (crop[2] - crop[0], crop[3] - crop[1]);
    let (x, y) = (crop[0], crop[1]);
    match rotate.rem_euclid(360) {
        90 => [x + wc - b[3], y + b[0], x + wc - b[1], y + b[2]],
        180 => [x + wc - b[2], y + hc - b[3], x + wc - b[0], y + hc - b[1]],
        270 => [x + b[1], y + hc - b[2], x + b[3], y + hc - b[0]],
        _ => [x + b[0], y + b[1], x + b[2], y + b[3]],
    }
}

/// The matrix of the form that carries the page's content into graphic space (y down), given the page's `/Rotate`, crop box and
/// the graphic bounds `[left, top, right, bottom]` (see [`turn`]): a point of the page goes to (x', top + bottom − y') where
/// (x', y') is its place in the turned page measured from the turned crop box's lower left.
pub fn form_matrix(rotate: i32, crop: [f64; 4], bounds: [f64; 4]) -> [f64; 6] {
    let (wc, hc) = (crop[2] - crop[0], crop[3] - crop[1]);
    let tb = bounds[1] + bounds[3];
    match rotate.rem_euclid(360) {
        90 => [0., 1., 1., 0., -crop[1], tb - wc - crop[0]],
        180 => [-1., 0., 0., 1., crop[0] + wc, tb - hc - crop[1]],
        270 => [0., -1., -1., 0., hc + crop[1], tb + crop[0]],
        _ => [1., 0., 0., -1., -crop[0], crop[1] + tb],
    }
}

fn page(info: &ImageInfo, number: u32) -> Result<&PdfPage, String> {
    let p = number.checked_sub(1).and_then(|n| info.page_boxes.get(n as usize))
        .ok_or_else(|| format!("PDF page {number} is outside 1..{}", info.page_boxes.len()))?;
    if p.rotate.rem_euclid(90) != 0 { return Err("PDF pages turned by something other than a multiple of 90 degrees cannot be placed".into()); }
    if !valid_box(p.media) || !valid_box(p.crop.unwrap_or(p.media)) { return Err("Invalid PDF page box".into()); }
    Ok(p)
}

pub fn inspect(path: &Path, info: &ImageInfo, options: Options) -> Result<Source, String> {
    let doc = load(path)?;
    let p = page(info, options.page_number)?;
    let crop = p.crop.unwrap_or(p.media);
    let selected = match options.crop {
        Crop::CropPDF => crop,
        Crop::CropMedia => p.media,
        Crop::CropArt => p.art.ok_or("PDF page has no ArtBox")?,
        Crop::CropTrim => p.trim.ok_or("PDF page has no TrimBox")?,
        Crop::CropBleed => p.bleed.ok_or("PDF page has no BleedBox")?,
        Crop::CropContentVisibleLayers | Crop::CropContentAllLayers => {
            if doc.catalog().is_ok_and(|c| c.has(b"OCProperties")) {
                return Err("Content cropping of PDF layers is not measured yet; choose a page box".into());
            }
            bounds::measure(&std::fs::read(path).map_err(|e| e.to_string())?, options.page_number, crop)?
        }
    };
    if !valid_box(selected) { return Err("Invalid PDF crop box".into()); }
    Ok(Source { options, bounds: turn(p.rotate, crop, selected), metadata: metadata::read(&doc)?, layers: layers::read(&doc)? })
}

fn load(path: &Path) -> Result<lopdf::Document, String> {
    let doc = lopdf::Document::load(path).map_err(|e| format!("PDF: {e}"))?;
    if doc.is_encrypted() { return Err("Encrypted PDF cannot be placed".into()); }
    for id in doc.get_pages().values() {
        if let Ok(p) = doc.get_dictionary(*id)
            && p.get(b"UserUnit").is_ok_and(|n| n.as_float().unwrap_or(0.) != 1.)
        {
            return Err("PDF UserUnit other than 1 is not supported for placement yet".into());
        }
    }
    Ok(doc)
}

pub(super) fn settings() -> (InterpreterSettings, Arc<AtomicBool>) {
    let failed = Arc::new(AtomicBool::new(false));
    let font_failed = failed.clone();
    let cmap_failed = failed.clone();
    let warn_failed = failed.clone();
    (InterpreterSettings {
        render_annotations: false,
        font_resolver: Arc::new(move |_| { font_failed.store(true, Ordering::Relaxed); None }),
        cmap_resolver: Arc::new(move |_| { cmap_failed.store(true, Ordering::Relaxed); None }),
        warning_sink: Arc::new(move |_| { warn_failed.store(true, Ordering::Relaxed); }),
    }, failed)
}

/// A bounded preview of the chosen box. Rebuilt from the portable original, never required
/// to make the project portable. Unsupported fonts/images fail instead of disappearing.
pub fn preview(path: &Path, info: &ImageInfo, source: &Source, output: &Path) -> Result<(), String> {
    let p = page(info, source.options.page_number)?;
    let crop = p.crop.unwrap_or(p.media);
    let b = source.bounds;
    if !valid_box(b) { return Err("Invalid PDF graphic bounds".into()); }
    let selected = unturn(p.rotate, crop, b);
    let mut doc = load(path)?;
    layers::apply(&mut doc,source.layers.as_ref())?;
    let id = *doc.get_pages().get(&source.options.page_number).ok_or("PDF page is missing")?;
    let dict = doc.get_dictionary_mut(id).map_err(|e| e.to_string())?;
    for key in [b"MediaBox".as_slice(), b"CropBox"] {
        dict.set(key, selected.iter().map(|v| lopdf::Object::Real(*v as f32)).collect::<Vec<_>>());
    }
    let mut data = Vec::new();
    doc.save_to(&mut data).map_err(|e| e.to_string())?;
    let pdf = Pdf::new(data).map_err(|e| format!("PDF preview: {e:?}"))?;
    let page = &pdf.pages()[source.options.page_number as usize - 1];
    let scale = 2.0_f64.min(2400. / (b[2]-b[0]).max(b[3]-b[1]));
    if scale <= 0. || (b[2]-b[0])*scale < 1. || (b[3]-b[1])*scale < 1. { return Err("PDF page is too small or too large".into()); }
    let (settings, failed) = settings();
    let bg = if source.options.transparent_background {
        hayro::vello_cpu::color::palette::css::TRANSPARENT
    } else { hayro::vello_cpu::color::palette::css::WHITE };
    let image = hayro::render(page, &hayro::RenderCache::new(), &settings, &hayro::RenderSettings::default(),
        &hayro::PixmapSettings { x_scale: scale as f32, y_scale: scale as f32, bg_color: bg });
    if failed.load(Ordering::Relaxed) { return Err("PDF preview needs embedded fonts and supported images/CMaps".into()); }
    image.into_png().map_err(|e| e.to_string()).and_then(|png| std::fs::write(output, png).map_err(|e| e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// InDesign 2025 (placeimg-c1): pdfplace-single.pdf, CropBox 10 30 230 290, TrimBox 20 60 220 280, /Rotate 90: the trim crop's
    /// bounds are L30 T10 R250 B210; the whole crop box gives 260 × 220.
    #[test]
    fn a_turned_page_gives_the_bounds_indesign_measured() {
        let (crop, trim) = ([10., 30., 230., 290.], [20., 60., 220., 280.]);
        assert_eq!(turn(90, crop, trim), [30., 10., 250., 210.]);
        assert_eq!(turn(90, crop, crop), [0., 0., 260., 220.]);
        assert_eq!(turn(180, crop, crop), [0., 0., 220., 260.]);
        assert_eq!(turn(270, crop, crop), [0., 0., 260., 220.]);
        assert_eq!(turn(0, crop, trim), [10., 30., 210., 250.]);
    }

    #[test]
    fn bounds_turn_back_to_the_page_box_and_the_form_matrix_maps_it_onto_them() {
        let (crop, trim) = ([10., 30., 230., 290.], [20., 60., 220., 280.]);
        for rotate in [0, 90, 180, 270] {
            let bounds = turn(rotate, crop, trim);
            assert_eq!(unturn(rotate, crop, bounds), trim, "rotate {rotate}");
            // the form's matrix sends the shown box (page space) onto the bounds, y down: corners map to the bounds' corners
            let m = form_matrix(rotate, crop, bounds);
            let at = |x: f64, y: f64| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
            let corners = [at(trim[0], trim[1]), at(trim[2], trim[3])];
            let (x0, x1) = (corners[0].0.min(corners[1].0), corners[0].0.max(corners[1].0));
            let (y0, y1) = (corners[0].1.min(corners[1].1), corners[0].1.max(corners[1].1));
            assert_eq!([x0, y0, x1, y1], bounds, "rotate {rotate}");
        }
    }
}
