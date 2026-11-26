//! A finished PDF drawn to pixels, page by page, for devices that take bitmaps (the Windows print path sends the print PDF's sheets this way).
//! The PDF is the one print export writes (fonts embedded), so nothing here looks anything up on the computer.
use hayro::hayro_syntax::Pdf;

/// One page as RGBA8 (opaque, white paper), top row first.
pub struct Raster { pub width: u32, pub height: u32, pub rgba: Vec<u8> }

pub struct RasterDocument { pdf: Pdf }

impl RasterDocument {
    pub fn new(data: Vec<u8>) -> Result<Self, String> {
        Ok(Self { pdf: Pdf::new(data).map_err(|e| format!("cannot read the print PDF: {e:?}"))? })
    }
    pub fn pages(&self) -> usize { self.pdf.pages().len() }
    /// The page's size in points (rotation applied).
    pub fn page_size(&self, index: usize) -> Option<[f64; 2]> {
        self.pdf.pages().get(index).map(|p| { let (w, h) = p.render_dimensions(); [f64::from(w), f64::from(h)] })
    }
    /// Page `index` at `scale` pixels per point.
    pub fn render(&self, index: usize, scale: f32) -> Result<Raster, String> {
        let page = self.pdf.pages().get(index).ok_or_else(|| format!("the print PDF has no page {}", index + 1))?;
        let (w, h) = page.render_dimensions();
        if scale <= 0.0 || w * scale >= f32::from(u16::MAX) || h * scale >= f32::from(u16::MAX) { return Err("the page is too large for this resolution".into()); }
        let (settings, failed) = crate::pdfplace::settings();
        let image = hayro::render(page, &hayro::RenderCache::new(), &settings, &hayro::RenderSettings::default(),
            &hayro::PixmapSettings { x_scale: scale, y_scale: scale, bg_color: hayro::vello_cpu::color::palette::css::WHITE });
        if failed.load(std::sync::atomic::Ordering::Relaxed) { return Err("the print PDF needs embedded fonts and supported images".into()); }
        Ok(Raster { width: u32::from(image.width()), height: u32::from(image.height()), rgba: image.data_as_u8_slice().to_vec() })
    }
}
