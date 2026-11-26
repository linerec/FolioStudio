//! Placed graphics: which file each graphic prints from, and how it is written.
//!
//! The original linked file is used whenever `folio-links` finds it: JPEG passes through
//! as DCT data, PNG / TIFF / PSD are decoded to Flate in their own colour space (alpha as
//! a soft mask, embedded ICC kept for RGB), PDF and AI pages are copied as Form XObjects.
//! EPS cannot be placed in PDF/X natively, and missing or embedded links have no file: the
//! INDD reader's preview is used and reported.

use crate::colors::Resolver;
use crate::pdfimport::{ImportCache, Imported};
use crate::registry::{FormDef, Warn};
use folio_links::{ImageFormat, LinkEntry, LinkReport, LinkStatus};
use folio_model::{Affine, Graphic};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub enum Placement {
    /// Draw image `index` through `unit` (unit square → graphic inner coordinates).
    Image { index: usize, unit: Affine, transparency: bool },
    /// Draw imported page `index` (its form maps PDF user space to graphic coordinates).
    Imported { index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageSource {
    /// Original JPEG, written as it is (DCTDecode).
    Jpeg(PathBuf),
    /// Original PNG / TIFF / PSD, decoded to its native samples.
    Decode(PathBuf, ImageFormat),
    /// The INDD reader's preview of a missing or unusable image, with its alpha.
    Preview(PathBuf),
    /// The reader's proxy of a placed PDF / EPS ([`folio_render::is_proxy`]): its colour as
    /// stored, its alpha a 1-bit explicit mask (α > 0 painted) — what InDesign prints, except
    /// that fully transparent pixels are not painted white. Never transparency.
    Proxy(PathBuf),
    /// File data embedded in the document (IDML `Contents`).
    Embedded(std::sync::Arc<Vec<u8>>),
}

pub struct ImageJob {
    pub source: ImageSource,
    pub color: Option<folio_model::image_color::ImageColor>,
    /// Graphic ids using it, for reports.
    pub owners: Vec<String>,
    /// Largest placed size (points) of the image's width and height axes.
    pub placed: (f64, f64),
    /// File size, for the decoding memory budget.
    pub bytes: u64,
    /// Decoded size when the header told us (width × height × channels).
    pub decoded: Option<u64>,
    /// Placements drawn (an image only the flattener's rasters show is not written).
    pub uses: usize,
}

#[derive(Default)]
pub struct LinkUse {
    pub uri: String,
    pub used: String,
    pub how: String,
    pub path: Option<String>,
    pub pages: Vec<usize>,
    pub placements: usize,
    pub written: String,
}

pub struct Graphics<'d> {
    entries: HashMap<String, LinkEntry>,
    pub images: Vec<ImageJob>,
    image_index: HashMap<(ImageSource, Option<folio_model::image_color::ImageColor>), usize>,
    pub imported: Vec<Imported>,
    imported_index: HashMap<(PathBuf, u32, [i64; 4], bool, String), usize>,
    imported_form: HashMap<usize, usize>,
    pdfs: ImportCache,
    pub links: BTreeMap<String, LinkUse>,
    /// Faces for fonts placed PDFs reference without embedding them.
    fonts: &'d folio_text::FontStore,
    /// The Ink Manager, for spot inks inside placed PDFs.
    rules: crate::spotconv::InkRules,
    /// Flattening (PDF 1.3): images with alpha and placed PDFs with transparency are
    /// transparency the flattener rasterises.
    pub flatten: bool,
    /// CMYK-only output (PDF/X-1a): DeviceRGB in placed PDFs is converted; a placed PDF with
    /// other RGB, Lab or ICC-based colour cannot be copied as it is.
    pub cmyk_only: bool,
    /// Import files a drawn placement uses (others are not written).
    used_files: std::collections::HashSet<usize>,
    alpha_classes: HashMap<ImageSource, crate::images::AlphaClass>,
}

/// Where the flattener's raster takes a graphic's image from.
pub enum RasterSource {
    /// A found original raster or embedded data, decoded like the vector output. Its samples
    /// are used as stored: InDesign turns an EXIF-oriented image through the graphic's
    /// transform, not its pixels (basic-place-formats: place-jpeg-exif6).
    Image(ImageSource),
    /// The reader's preview (missing links, placed PDF / EPS / AI vector content).
    Preview,
}

/// The source of a graphic's image for the flattener's rasters: the original raster file the
/// vector output would use (same choice as [`Graphics::place`]), else the preview.
pub fn raster_source(entries: &HashMap<String, LinkEntry>, g: &Graphic) -> RasterSource {
    let entry = entries.get(&g.id);
    let found = entry.and_then(|e| match &e.status {
        Some(LinkStatus::Found { path, .. }) => Some(path.clone()),
        _ => None,
    });
    let info = entry.and_then(|e| e.info.as_ref()).and_then(|i| i.as_ref().ok());
    if let Some(path) = &found {
        let format = info.map(|i| i.format).or_else(|| sniff(path));
        match (g.element.as_str(), format) {
            ("EPS", _) | (_, Some(ImageFormat::Eps | ImageFormat::Pdf | ImageFormat::Ai)) => return RasterSource::Preview,
            (_, Some(ImageFormat::Jpeg)) => return RasterSource::Image(ImageSource::Jpeg(path.clone())),
            (_, Some(f @ (ImageFormat::Png | ImageFormat::Tiff | ImageFormat::Psd | ImageFormat::Gif | ImageFormat::Bmp))) => return RasterSource::Image(ImageSource::Decode(path.clone(), f)),
            _ => {}
        }
    }
    if found.is_none()
        && let Some(data) = g.embedded_data.clone()
        && !data.starts_with(b"%PDF")
    {
        return RasterSource::Image(ImageSource::Embedded(data));
    }
    RasterSource::Preview
}


/// Whether embedded image data (PNG, TIFF, PSD) has an alpha channel.
fn embedded_alpha(d: &[u8]) -> bool {
    if d.starts_with(b"\x89PNG") {
        return d.get(25).is_some_and(|t| *t == 4 || *t == 6) || d.windows(4).take(4096).any(|w| w == b"tRNS");
    }
    if d.starts_with(b"II*\0") || d.starts_with(b"MM\0*") {
        let little = d[0] == b'I';
        let u16_at = |i: usize| d.get(i..i + 2).map(|b| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
        let u32_at = |i: usize| d.get(i..i + 4).map(|b| if little { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) });
        let Some(ifd) = u32_at(4) else { return false };
        let Some(n) = u16_at(ifd as usize) else { return false };
        return (0..n as usize).any(|k| u16_at(ifd as usize + 2 + 12 * k) == Some(338));
    }
    false
}

pub fn sniff(path: &Path) -> Option<ImageFormat> {
    use std::io::Read;
    let mut head = [0u8; 8];
    let mut f = std::fs::File::open(path).ok()?;
    let n = f.read(&mut head).ok()?;
    let h = &head[..n];
    Some(if h.starts_with(&[0xFF, 0xD8]) {
        ImageFormat::Jpeg
    } else if h.starts_with(b"\x89PNG") {
        ImageFormat::Png
    } else if h.starts_with(b"II*\0") || h.starts_with(b"MM\0*") {
        ImageFormat::Tiff
    } else if h.starts_with(b"8BPS") {
        ImageFormat::Psd
    } else if h.starts_with(b"GIF8") {
        ImageFormat::Gif
    } else if h.starts_with(b"BM") {
        ImageFormat::Bmp
    } else if h.starts_with(b"%PDF") {
        ImageFormat::Pdf
    } else {
        return None;
    })
}

impl<'d> Graphics<'d> {
    pub fn new(report: Option<LinkReport>, fonts: &'d folio_text::FontStore, rules: crate::spotconv::InkRules) -> Self {
        let entries = report.map(|r| r.links.into_iter().map(|e| (e.owner.clone(), e)).collect()).unwrap_or_default();
        Graphics {
            entries,
            images: Vec::new(),
            image_index: HashMap::new(),
            imported: Vec::new(),
            imported_index: HashMap::new(),
            imported_form: HashMap::new(),
            pdfs: ImportCache::default(),
            links: BTreeMap::new(),
            fonts,
            rules,
            flatten: false,
            cmyk_only: false,
            used_files: Default::default(),
            alpha_classes: HashMap::new(),
        }
    }

    /// Whether an image's alpha is soft (fades: transparency to flatten) rather than a cut-out
    /// (an explicit mask in PDF 1.3); decoded once per source.
    fn soft_alpha(&mut self, source: &ImageSource) -> bool {
        if let Some(&c) = self.alpha_classes.get(source) {
            return c == crate::images::AlphaClass::Soft;
        }
        let c = crate::images::alpha_class_of(source);
        self.alpha_classes.insert(source.clone(), c);
        c == crate::images::AlphaClass::Soft
    }

    /// The link entries by graphic id (for the flattener's image source).
    pub fn entries(&self) -> &HashMap<String, LinkEntry> {
        &self.entries
    }

    /// Flattening / CMYK-only output settings; `cmyk`: the conversion of CMYK-only output.
    pub fn set_output(&mut self, flatten: bool, cmyk: Option<folio_color::Colors>) {
        self.flatten = flatten;
        self.cmyk_only = cmyk.is_some();
        self.pdfs.cmyk_output = cmyk;
        self.pdfs.pdf13 = flatten;
    }

    /// Whether the found original of graphic `id` has an alpha channel (its header says so).
    pub fn link_has_alpha(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| {
            matches!(e.status, Some(LinkStatus::Found { .. })) && e.info.as_ref().and_then(|i| i.as_ref().ok()).is_some_and(|i| i.alpha)
        })
    }

    /// Whether a drawn placement uses import file `file`.
    pub fn file_used(&self, file: usize) -> bool {
        self.used_files.contains(&file)
    }

    /// Whether imported page `index` cannot be copied into PDF 1.3 output as it is:
    /// transparency, or (CMYK-only output) colour that is not CMYK, gray or spot, or
    /// features newer than PDF 1.3.
    pub fn imported_needs_flattening(&self, index: usize) -> Option<&'static str> {
        let imp = self.imported.get(index)?;
        let f = self.pdfs.files.get(imp.file)?;
        if imp.transparency {
            Some("placed PDF with transparency")
        } else if f.newer_than_13 {
            Some("placed PDF with features newer than PDF 1.3")
        } else if self.cmyk_only && f.non_cmyk {
            Some("placed PDF with RGB, Lab or ICC-based colour")
        } else {
            None
        }
    }

    fn image(&mut self, source: ImageSource, graphic: &Graphic) -> usize {
        let owner = &graphic.id;
        let key = (source.clone(), graphic.image_color.clone());
        if let Some(&i) = self.image_index.get(&key) {
            if self.images[i].owners.len() < 8 {
                self.images[i].owners.push(owner.to_owned());
            }
            return i;
        }
        let bytes = match &source {
            ImageSource::Jpeg(p) | ImageSource::Decode(p, _) | ImageSource::Preview(p) | ImageSource::Proxy(p) => std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            ImageSource::Embedded(d) => d.len() as u64,
        };
        self.images.push(ImageJob { source: source.clone(), color: graphic.image_color.clone(), owners: vec![owner.to_owned()], placed: (0.0, 0.0), bytes, decoded: None, uses: 0 });
        self.image_index.insert(key, self.images.len() - 1);
        self.images.len() - 1
    }

    pub fn svg_source(&self, g: &Graphic) -> Option<PathBuf> {
        match self.entries.get(&g.id).and_then(|e| e.status.as_ref()) {
            Some(LinkStatus::Found { path, .. }) => Some(path.clone()),
            _ => g.preview.as_ref().map(PathBuf::from).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"))),
        }
    }

    /// Pixels of an SVG (its filters, masks, gradients, opacity, bitmaps: folio_render svg/rich.rs) as a PNG image (alpha as
    /// a soft mask), registered for graphic `g`.
    pub fn svg_pixels(&mut self, g: &Graphic, png: Vec<u8>) -> usize {
        self.image(ImageSource::Embedded(std::sync::Arc::new(png)), g)
    }

    pub fn note_svg(&mut self, g: &Graphic, page: usize, path: &Path) {
        self.note(g, page, "source", "SVG vector".into(), Some(path), "vector paths");
    }

    fn note(&mut self, g: &Graphic, page: usize, used: &str, how: String, path: Option<&Path>, written: &str) {
        let uri = g.link_uri.clone().or_else(|| g.link.as_ref().map(|l| l.uri.clone())).unwrap_or_else(|| format!("(no link: {})", g.id));
        let e = self.links.entry(format!("{uri}\u{1}{used}")).or_default();
        e.uri = uri;
        e.used = used.to_owned();
        e.how = how;
        e.path = path.map(|p| p.display().to_string());
        e.placements += 1;
        e.written = written.to_owned();
        if !e.pages.contains(&page) && e.pages.len() < 16 {
            e.pages.push(page);
        }
    }

    /// The unit square mapped onto the graphic's bounds (y down).
    fn unit(g: &Graphic) -> Affine {
        let b = &g.bounds;
        Affine::new(b.width(), 0.0, 0.0, -b.height(), b.left, b.bottom)
    }

    pub fn place(&mut self, g: &Graphic, page: usize, res: &mut Resolver<'_>, warn: &mut Warn, frame: &str) -> Option<Placement> {
        let _ = res;
        if g.bounds.width().abs() < 1e-9 || g.bounds.height().abs() < 1e-9 {
            return None;
        }
        let entry = self.entries.get(&g.id).cloned();
        let found: Option<PathBuf> = entry.as_ref().and_then(|e| match &e.status {
            Some(LinkStatus::Found { path, .. }) => Some(path.clone()),
            _ => None,
        });
        let how = match entry.as_ref().and_then(|e| e.status.as_ref()) {
            Some(LinkStatus::Found { how, .. }) => format!("{how:?}"),
            Some(LinkStatus::Missing) => "missing".into(),
            Some(LinkStatus::Ambiguous(c)) => format!("ambiguous ({} candidates)", c.len()),
            None if entry.as_ref().is_some_and(|e| e.embedded) => "embedded in the document".into(),
            None => "no link".into(),
        };
        let info = entry.as_ref().and_then(|e| e.info.as_ref()).and_then(|i| i.as_ref().ok());
        let element = g.element.as_str();

        if let Some(path) = &found {
            let format = info.map(|i| i.format).or_else(|| sniff(path));
            match (element, format) {
                ("EPS", _) | (_, Some(ImageFormat::Eps)) => {
                    // a Photoshop EPS: its own picture (`%ImageData`, full resolution; build-a2), not a preview
                    if !info.is_some_and(|i| i.width > 0 && i.height > 0) {
                        warn.add("link", "EPS cannot be placed in PDF/X natively; its preview was used", Some(page), Some(&g.id));
                    }
                }
                (_, Some(ImageFormat::Pdf | ImageFormat::Ai)) => {
                    let pdf = g.pdf.clone().unwrap_or_default();
                    let pg = pdf.page.max(1);
                    let key_b = [g.bounds.left, g.bounds.top, g.bounds.right, g.bounds.bottom].map(|v| (v * 1000.0).round() as i64);
                    let key = (path.clone(), pg, key_b, pdf.transparent_background, serde_json::to_string(&g.layer_options).unwrap_or_default());
                    if let Some(&i) = self.imported_index.get(&key) {
                        self.note(g, page, "original", how, Some(path), "Form XObject (PDF page)");
                        return Some(Placement::Imported { index: i });
                    }
                    match self.pdfs.import_layers(path, pg, &g.bounds, self.fonts, &self.rules, pdf.transparent_background, g.layer_options.as_ref()) {
                        Ok(imp) => {
                            for w in &imp.warnings {
                                warn.add("link", w.clone(), Some(page), Some(&g.id));
                            }
                            self.imported.push(imp);
                            let i = self.imported.len() - 1;
                            self.imported_index.insert(key, i);
                            self.note(g, page, "original", how, Some(path), "Form XObject (PDF page)");
                            return Some(Placement::Imported { index: i });
                        }
                        Err(e) => warn.add("link", format!("placed PDF could not be copied ({e}); its preview was used"), Some(page), Some(&g.id)),
                    }
                }
                (_, Some(ImageFormat::Jpeg)) => {
                    let i = self.image(ImageSource::Jpeg(path.clone()), g);
                    self.note(g, page, "original", how, Some(path), "DCT passthrough");
                    return Some(Placement::Image { index: i, unit: Self::unit(g), transparency: false });
                }
                (_, Some(f @ (ImageFormat::Png | ImageFormat::Tiff | ImageFormat::Psd | ImageFormat::Gif | ImageFormat::Bmp))) => {
                    // a PSD with layers switched: the composition of the layers shown (objlayer-c4)
                    if matches!(f, ImageFormat::Psd)
                        && let Some(layers) = g.layer_options.as_ref().filter(|o| folio_links::psdlayers::overrides(o))
                    {
                        match std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| folio_links::psdlayers::flatten(&b, layers)) {
                            Ok(Some(bytes)) => {
                                let has_alpha = bytes.get(12..14).is_some_and(|c| u16::from_be_bytes([c[0], c[1]]) > if bytes.get(24..26) == Some(&[0, 1]) { 1 } else { 3 });
                                let source = ImageSource::Embedded(std::sync::Arc::new(bytes));
                                let alpha = has_alpha && (!self.flatten || self.soft_alpha(&source));
                                let i = self.image(source, g);
                                self.note(g, page, "original", how, Some(path), "Flate (PSD layers composed)");
                                return Some(Placement::Image { index: i, unit: Self::unit(g), transparency: alpha });
                            }
                            Ok(None) => {}
                            Err(e) => warn.add("link", format!("PSD layers not applied ({e}); the saved picture was used"), Some(page), Some(&g.id)),
                        }
                    }
                    let i = self.image(ImageSource::Decode(path.clone(), f), g);
                    if let Some(info) = info {
                        self.images[i].decoded = Some(info.width as u64 * info.height as u64 * info.channels.max(1) as u64 * (info.bits.max(8) as u64 / 8));
                    }
                    self.note(g, page, "original", how, Some(path), "Flate");
                    let alpha = info.is_some_and(|i| i.alpha) && (!self.flatten || self.soft_alpha(&ImageSource::Decode(path.clone(), f)));
                    return Some(Placement::Image { index: i, unit: Self::unit(g), transparency: alpha });
                }
                (_, f) => {
                    warn.add("link", format!("linked format {f:?} not written natively; its preview was used"), Some(page), Some(&g.id));
                }
            }
        }

        // Data embedded in the document (raster formats; PDF data is not copied).
        if found.is_none()
            && let Some(data) = g.embedded_data.clone()
            && !data.starts_with(b"%PDF")
        {
            let alpha = self.flatten && embedded_alpha(&data) && self.soft_alpha(&ImageSource::Embedded(data.clone()));
            let i = self.image(ImageSource::Embedded(data), g);
            self.note(g, page, "embedded", "embedded in the document".into(), None, "embedded data");
            return Some(Placement::Image { index: i, unit: Self::unit(g), transparency: alpha });
        }

        // The reader's preview.
        let Some(preview) = g.preview.as_deref().map(PathBuf::from) else {
            warn.add("link", "graphic without original file or preview: not printed", Some(page), Some(&g.id));
            self.note(g, page, "none", how, None, "nothing");
            return None;
        };
        let reason = if found.is_some() { "original not usable" } else { how.as_str() };
        if found.is_none() {
            let what = if entry.as_ref().is_some_and(|e| e.embedded) { "embedded link" } else { "missing link" };
            warn.add("link", format!("{what}: low-resolution preview printed"), Some(page), Some(frame));
        }
        let fmt = sniff(&preview);
        let source = match fmt {
            Some(ImageFormat::Jpeg) => ImageSource::Jpeg(preview.clone()),
            _ if folio_render::graphic_is_proxy(g) => ImageSource::Proxy(preview.clone()),
            _ => ImageSource::Preview(preview.clone()),
        };
        // an image preview's soft alpha is transparency to flatten (a cut-out prints through
        // an explicit mask); a placed PDF / EPS proxy's alpha is always a 1-bit mask
        let alpha = self.flatten
            && matches!(source, ImageSource::Preview(_))
            && folio_render::header_has_alpha(&preview.to_string_lossy()).unwrap_or(false)
            && self.soft_alpha(&source);
        let written = if matches!(source, ImageSource::Jpeg(_)) { "preview, DCT" } else { "preview, Flate" };
        let i = self.image(source, g);
        let reason = reason.to_owned();
        self.note(g, page, "preview", reason, Some(&preview), written);
        Some(Placement::Image { index: i, unit: Self::unit(g), transparency: alpha })
    }

    /// The form index drawing imported page `index` (registered on first use).
    pub fn form_of(&mut self, index: usize, forms: &mut Vec<FormDef>) -> usize {
        if let Some(imp) = self.imported.get(index) {
            self.used_files.insert(imp.file);
        }
        if let Some(&f) = self.imported_form.get(&index) {
            return f;
        }
        forms.push(FormDef::Imported(index));
        let f = forms.len() - 1;
        self.imported_form.insert(index, f);
        f
    }

    /// Records a placement of image `index` whose unit square maps through `t` (points).
    pub fn placed(&mut self, index: usize, t: &Affine) {
        if let Some(j) = self.images.get_mut(index) {
            j.uses += 1;
            let w = t.a.hypot(t.b);
            let h = t.c.hypot(t.d);
            j.placed = (j.placed.0.max(w), j.placed.1.max(h));
        }
    }

    /// Frees the parsed source PDFs once every page is drawn.
    pub fn release_sources(&mut self) {
        self.pdfs.release();
        if self.pdfs.pdf13 {
            for (file, entry) in self.pdfs.files.iter_mut().enumerate() {
                if !self.used_files.contains(&file) {
                    entry.chunk = pdf_writer::Chunk::new();
                }
            }
        }
    }

    /// Chunks of the placed PDF files (one per source file, shared by its placements).
    pub fn take_import_files(&mut self) -> Vec<crate::pdfimport::FileChunk> {
        std::mem::take(&mut self.pdfs.files)
    }

    pub fn imported_transparency(&self, index: usize) -> bool {
        self.imported.get(index).is_some_and(|i| i.transparency)
    }
}

#[cfg(test)]
mod image_color_tests {
    use super::*;

    #[test]
    fn placements_share_only_when_source_and_color_settings_match() {
        let fonts = folio_text::FontStore::empty();
        let mut graphics = Graphics::new(None, &fonts, Default::default());
        let source = ImageSource::Embedded(std::sync::Arc::new(vec![1, 2, 3]));
        let mut graphic = Graphic { id: "a".into(), image_color: Some(Default::default()), ..Default::default() };
        let first = graphics.image(source.clone(), &graphic);
        graphic.id = "b".into();
        assert_eq!(graphics.image(source.clone(), &graphic), first);
        graphic.image_color.as_mut().unwrap().intent = "Saturation".into();
        let saturation = graphics.image(source.clone(), &graphic);
        assert_ne!(saturation, first);
        graphic.image_color.as_mut().unwrap().profile = "Another RGB profile".into();
        let assigned = graphics.image(source.clone(), &graphic);
        assert_ne!(assigned, saturation);
        assert_eq!(graphics.image(source, &graphic), assigned);
        assert_eq!(graphics.images.len(), 3);
    }
}
