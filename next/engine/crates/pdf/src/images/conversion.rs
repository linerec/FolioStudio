//! Resolves and applies image color profile conversions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// RGB → output CMYK for images: the document RGB profile for untagged images (exactly the
/// separations renderer's pixel conversion, `Colors::rgb_to_cmyk8`), an image's own profile
/// when it has one (`folio_color::rgb_link`, same intent and black point compensation).
/// CMYK and gray keep their numbers (InDesign's "Preserve Numbers").
pub struct Conv {
    pub colors: folio_color::Colors,
    /// Output (destination) CMYK profile data.
    pub cmyk_icc: Arc<Vec<u8>>,
    /// Document RGB profile data: images tagged with it convert as untagged ones.
    pub doc_rgb: Option<Arc<Vec<u8>>>,
    /// Every RGB image is converted (PDF/X-1a); otherwise only those whose profile is newer
    /// than ICC version 2 (PDF/X-3, PDF 1.3).
    pub all_rgb: bool,
    links: Mutex<HashMap<(u64, folio_color::icc::Intent), Option<folio_color::RgbLink>>>,
    /// Convert to destination: a CMYK image with an assigned profile other than the output profile is converted to it.
    pub cmyk_sources: bool,
    cmyk_links: Mutex<HashMap<(u64, folio_color::icc::Intent), Option<folio_color::CmykLink>>>,
    pub converted: std::sync::atomic::AtomicUsize,
    pub own_profile: std::sync::atomic::AtomicUsize,
}

impl Conv {
    pub fn new(colors: folio_color::Colors, cmyk_icc: Arc<Vec<u8>>, doc_rgb: Option<Arc<Vec<u8>>>, all_rgb: bool) -> Conv {
        Conv {
            colors,
            cmyk_icc,
            doc_rgb,
            all_rgb,
            links: Mutex::new(HashMap::new()),
            cmyk_sources: false,
            cmyk_links: Mutex::new(HashMap::new()),
            converted: Default::default(),
            own_profile: Default::default(),
        }
    }

    /// Converts CMYK images that carry an assigned profile (PDF/X-1a: "Convert to Destination").
    pub fn with_cmyk_sources(mut self, on: bool) -> Conv {
        self.cmyk_sources = on;
        self
    }

    /// Whether a CMYK image whose numbers are in the profile `source` is converted to the output profile.
    pub fn wants_cmyk(&self, source: &Arc<Vec<u8>>) -> bool {
        self.cmyk_sources && source.as_slice() != self.cmyk_icc.as_slice()
    }

    fn cmyk_link(&self, source: &Arc<Vec<u8>>, intent: folio_color::icc::Intent) -> Option<folio_color::CmykLink> {
        let key = (crate::colors::fnv(source), intent);
        if let Some(hit) = self.cmyk_links.lock().expect("PDF worker/cache lock is not poisoned").get(&key) {
            return hit.clone();
        }
        let link = folio_color::cmyk_link_with_intent(source, &self.cmyk_icc, intent).ok();
        self.cmyk_links.lock().expect("PDF worker/cache lock is not poisoned").insert(key, link.clone());
        link
    }

    /// Interleaved CMYK (`stride` bytes per pixel, the first four ink) of the profile `source` → output CMYK, the bytes
    /// behind the ink (alpha) kept. Unchanged when the link cannot be made.
    pub(super) fn cmyk_pixels(&self, source: &Arc<Vec<u8>>, samples: &[u8], stride: usize, intent: folio_color::icc::Intent) -> Vec<u8> {
        let Some(link) = self.cmyk_link(source, intent) else { return samples.to_vec() };
        self.own_profile.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut out = Vec::with_capacity(samples.len());
        let mut last: Option<([u8; 4], [u8; 4])> = None;
        for p in samples.chunks_exact(stride) {
            let ink = [p[0], p[1], p[2], p[3]];
            let c = match last {
                Some((i, o)) if i == ink => o,
                _ => {
                    let o = link.eval8(ink);
                    last = Some((ink, o));
                    o
                }
            };
            out.extend_from_slice(&c);
            out.extend_from_slice(&p[4..]);
        }
        out
    }

    /// The same conversion on ink + alpha pixels.
    pub(super) fn cmyk_ink5(&self, source: &Arc<Vec<u8>>, pixels: &mut [[u8; 5]], intent: folio_color::icc::Intent) {
        let Some(link) = self.cmyk_link(source, intent) else { return };
        self.own_profile.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut last: Option<([u8; 4], [u8; 4])> = None;
        for p in pixels {
            let ink = [p[0], p[1], p[2], p[3]];
            let out = match last {
                Some((i, o)) if i == ink => o,
                _ => {
                    let o = link.eval8(ink);
                    last = Some((ink, o));
                    o
                }
            };
            p[..4].copy_from_slice(&out);
        }
    }

    /// Whether an RGB image with this profile is converted in the vector output.
    pub fn wants(&self, icc: &Option<Arc<Vec<u8>>>) -> bool {
        self.all_rgb || icc.as_ref().is_some_and(|p| p.get(8).is_some_and(|v| *v >= 4))
    }

    /// The link of an image's own profile; `None`: the document RGB conversion.
    fn link(&self, icc: &Option<Arc<Vec<u8>>>, intent: folio_color::icc::Intent) -> Option<folio_color::RgbLink> {
        let p = icc.as_ref().or(self.doc_rgb.as_ref())?;
        if intent == folio_color::icc::Intent::Relative && self.doc_rgb.as_ref().is_some_and(|d| d.as_slice() == p.as_slice()) {
            return None;
        }
        let key = (crate::colors::fnv(p), intent);
        if let Some(hit) = self.links.lock().expect("PDF worker/cache lock is not poisoned").get(&key) {
            return hit.clone();
        }
        let l = folio_color::rgb_link_with_intent(p, &self.cmyk_icc, intent).ok();
        self.links.lock().expect("PDF worker/cache lock is not poisoned").insert(key, l.clone());
        l
    }

    /// Interleaved RGB (`stride` bytes per pixel, the first three RGB) → CMYK, keeping the
    /// bytes after the colour (alpha) behind it.
    pub(super) fn rgb_pixels_intent(&self, icc: &Option<Arc<Vec<u8>>>, samples: &[u8], stride: usize, intent: folio_color::icc::Intent) -> Vec<u8> {
        let link = self.link(icc, intent);
        if link.is_some() {
            self.own_profile.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let out_stride = stride + 1;
        let mut out = Vec::with_capacity(samples.len() / stride * out_stride);
        let mut last: Option<([u8; 3], [u8; 4])> = None;
        for p in samples.chunks_exact(stride) {
            let rgb = [p[0], p[1], p[2]];
            let c = match last {
                Some((i, o)) if i == rgb => o,
                _ => {
                    let o = match &link {
                        Some(l) => l.eval8(rgb),
                        None => self.colors.rgb_to_cmyk8(rgb),
                    };
                    last = Some((rgb, o));
                    o
                }
            };
            out.extend_from_slice(&c);
            out.extend_from_slice(&p[3..]);
        }
        out
    }
}
