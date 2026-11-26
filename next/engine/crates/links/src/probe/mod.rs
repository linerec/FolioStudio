//! Header-only facts of placed files: pixel size, colour space, bit depth, ICC profile,
//! resolution; for PDF / AI the page count and page boxes and the colour spaces the file
//! declares. No pixels are decoded. Results are cached per file identity (path, size,
//! modification time).

mod eps;
mod jpeg;
mod pdf;
mod png;
mod psd;
mod rasterxmp;
mod svg;
pub use svg::svg_size;
mod tiff;
pub mod xmp;
pub mod metafile;

pub use rasterxmp::{raster_xmp, raster_xmp_at};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Tiff,
    Psd,
    Pdf,
    /// Adobe Illustrator (PDF-compatible or PostScript).
    Ai,
    Eps,
    Svg,
    Gif,
    Bmp,
    /// A Windows metafile (EMF or WMF; build-a).
    Wmf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    Lab,
    Indexed,
    /// Photoshop multichannel / duotone, PDF DeviceN-only, or not declared.
    Other,
}

/// One PDF page's boxes in PDF user space (`[llx lly urx ury]`), as the file states them
/// (inherited from the page tree; not clipped to the MediaBox).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PdfPage {
    pub media: [f64; 4],
    pub crop: Option<[f64; 4]>,
    pub bleed: Option<[f64; 4]>,
    pub trim: Option<[f64; 4]>,
    pub art: Option<[f64; 4]>,
    pub rotate: i32,
}

/// Colour facts of a vector file (PDF, AI, EPS, SVG).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorColors {
    /// Colour spaces declared in the file (resources, images, content operators).
    pub spaces: Vec<ColorSpace>,
    /// Spot colour names (PDF Separation / DeviceN, EPS custom colours).
    pub spots: Vec<String>,
    /// Images inside, by colour space.
    pub image_spaces: Vec<ColorSpace>,
    /// Content streams were examined (false: only dictionaries / comments).
    pub content_scanned: bool,
    /// The file has an output intent (PDF/X).
    pub output_intent: Option<String>,
    /// Transparency groups or soft masks (PDF) / not applicable.
    pub transparency: bool,
}

/// A font used inside a placed PDF / EPS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedFont {
    /// PostScript name without a subset prefix (`ABCDEF+`).
    pub name: String,
    /// PDF font subtype (Type1, TrueType, Type0, Type3 …) or "PostScript" for EPS.
    pub kind: String,
    pub embedded: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageInfo {
    pub format: ImageFormat,
    /// Pixels for raster formats; points (rounded up) for vector formats.
    pub width: u32,
    pub height: u32,
    pub color_space: ColorSpace,
    /// Bits per channel (1 for bitmaps); 0 for vector formats.
    pub bits: u8,
    /// Channels including alpha / extra channels.
    pub channels: u16,
    pub alpha: bool,
    /// Embedded ICC profile bytes (hex in JSON).
    #[serde(default, with = "hex_bytes")]
    pub icc: Option<Vec<u8>>,
    /// Stored resolution (pixels per inch) for raster formats, when the file records one.
    pub ppi: Option<(f64, f64)>,
    /// Vector content (PDF, AI, EPS without image data, SVG).
    pub vector: bool,
    /// Pages (PDF / AI pages, TIFF IFDs).
    pub pages: Option<u32>,
    /// PDF / AI page boxes, one per page.
    #[serde(default)]
    pub page_boxes: Vec<PdfPage>,
    /// EXIF / TIFF orientation (1 = upright) when recorded.
    pub orientation: Option<u16>,
    #[serde(default)]
    pub vector_colors: Option<VectorColors>,
    /// Fonts a PDF / EPS uses and whether the file embeds them.
    #[serde(default)]
    pub fonts: Vec<PlacedFont>,
    /// File identity the facts were read from.
    pub file_size: u64,
    pub file_mtime_unix: Option<f64>,
}

impl ImageInfo {
    pub(crate) fn raster(format: ImageFormat, width: u32, height: u32, color_space: ColorSpace, bits: u8, channels: u16) -> Self {
        ImageInfo {
            format,
            width,
            height,
            color_space,
            bits,
            channels,
            alpha: false,
            icc: None,
            ppi: None,
            vector: false,
            pages: None,
            page_boxes: Vec::new(),
            orientation: None,
            vector_colors: None,
            fonts: Vec::new(),
            file_size: 0,
            file_mtime_unix: None,
        }
    }
}

/// `Option<Vec<u8>>` as a hex string.
mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => {
                let mut out = String::with_capacity(b.len() * 2);
                for x in b {
                    out.push(char::from_digit((x >> 4) as u32, 16).unwrap_or('0'));
                    out.push(char::from_digit((x & 15) as u32, 16).unwrap_or('0'));
                }
                s.serialize_some(&out)
            }
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let Some(h) = Option::<String>::deserialize(d)? else { return Ok(None) };
        let b = h.as_bytes();
        if b.len() % 2 != 0 {
            return Err(serde::de::Error::custom("odd hex length"));
        }
        b.chunks(2)
            .map(|p| {
                let v = |c: u8| (c as char).to_digit(16).ok_or_else(|| serde::de::Error::custom("bad hex digit"));
                Ok((v(p[0])? * 16 + v(p[1])?) as u8)
            })
            .collect::<Result<Vec<u8>, D::Error>>()
            .map(Some)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeError {
    Io(String),
    /// Not one of the formats read here (movies, sound, CSV …).
    Unsupported(String),
    /// The header is damaged or truncated.
    Malformed(String),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::Io(m) => write!(f, "I/O: {m}"),
            ProbeError::Unsupported(m) => write!(f, "unsupported: {m}"),
            ProbeError::Malformed(m) => write!(f, "malformed: {m}"),
        }
    }
}

impl std::error::Error for ProbeError {}

pub type Result<T> = std::result::Result<T, ProbeError>;

pub(crate) fn malformed<T>(m: impl Into<String>) -> Result<T> {
    Err(ProbeError::Malformed(m.into()))
}

/// Bytes read from the start of a file for format detection and most headers.
const HEAD: usize = 256 * 1024;

pub(crate) fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
pub(crate) fn be32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
pub(crate) fn le16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
pub(crate) fn le32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Reads up to `len` bytes at `offset`.
pub(crate) fn read_at(file: &mut std::fs::File, offset: u64, len: usize) -> Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(offset)).map_err(|e| ProbeError::Io(e.to_string()))?;
    let mut buf = Vec::with_capacity(len.min(64 << 20));
    file.take(len as u64).read_to_end(&mut buf).map_err(|e| ProbeError::Io(e.to_string()))?;
    Ok(buf)
}

type Key = (PathBuf, u64, Option<u128>);

type ProbeCache = Mutex<HashMap<PathBuf, (Key, Result<ImageInfo>)>>;

fn cache() -> &'static ProbeCache {
    static C: OnceLock<ProbeCache> = OnceLock::new();
    C.get_or_init(Mutex::default)
}

fn identity(path: &Path) -> Result<(Key, std::fs::Metadata)> {
    let m = std::fs::metadata(path).map_err(|e| ProbeError::Io(format!("{}: {e}", path.display())))?;
    if !m.is_file() {
        return Err(ProbeError::Io(format!("{}: not a file", path.display())));
    }
    let t = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos());
    Ok(((path.to_path_buf(), m.len(), t), m))
}

/// Header facts of `path`, cached by (path, size, modification time).
pub fn probe(path: &Path) -> Result<ImageInfo> {
    let (key, meta) = identity(path)?;
    if let Some((k, r)) = cache().lock().ok().and_then(|c| c.get(path).cloned())
        && k == key
    {
        return r;
    }
    let r = probe_uncached(path, &meta);
    if let Ok(mut c) = cache().lock() {
        c.insert(path.to_path_buf(), (key, r.clone()));
    }
    r
}

/// Number of entries in the in-process probe cache.
pub fn cached_probes() -> usize {
    cache().lock().map(|c| c.len()).unwrap_or(0)
}

/// Bumped when `ImageInfo` changes shape.
// 4: EMF and WMF are read (build-a); a cache that refused them is not used
const PROBE_CACHE_VERSION: u32 = 4;

#[derive(Serialize, Deserialize)]
struct ProbeCacheFile {
    version: u32,
    entries: Vec<(PathBuf, u64, Option<u128>, Result<ImageInfo>)>,
}

/// Loads probe results saved by [`save_probe_cache`]; entries are used only while the file
/// identity still matches. Returns the number of entries loaded.
pub fn load_probe_cache(file: &Path) -> usize {
    let Ok(bytes) = std::fs::read(file) else { return 0 };
    let Ok(c) = serde_json::from_slice::<ProbeCacheFile>(&bytes) else { return 0 };
    if c.version != PROBE_CACHE_VERSION {
        return 0;
    }
    let Ok(mut m) = cache().lock() else { return 0 };
    let n = c.entries.len();
    for (p, size, t, r) in c.entries {
        m.entry(p.clone()).or_insert(((p, size, t), r));
    }
    n
}

/// Writes the in-process probe cache to `file` (atomically).
pub fn save_probe_cache(file: &Path) -> std::io::Result<()> {
    let entries = cache()
        .lock()
        .map(|m| m.values().map(|((p, s, t), r)| (p.clone(), *s, *t, r.clone())).collect())
        .unwrap_or_default();
    let bytes = serde_json::to_vec(&ProbeCacheFile { version: PROBE_CACHE_VERSION, entries })?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, file)
}

/// Whether a GIF's graphic control extension (before its first image) names a transparent colour.
pub fn gif_has_transparency(b: &[u8]) -> bool {
    let Some(&packed) = b.get(10) else { return false };
    let mut o = 13 + if packed & 0x80 != 0 { 3 << ((packed & 7) + 1) } else { 0 };
    while b.get(o) == Some(&0x21) {
        if b.get(o + 1) == Some(&0xF9) && b.get(o + 3).is_some_and(|flags| flags & 1 == 1) {
            return true;
        }
        o += 2;
        while let Some(&n) = b.get(o) {
            o += 1 + n as usize;
            if n == 0 {
                break;
            }
        }
    }
    false
}

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn probe_uncached(path: &Path, meta: &std::fs::Metadata) -> Result<ImageInfo> {
    let mut file = std::fs::File::open(path).map_err(|e| ProbeError::Io(format!("{}: {e}", path.display())))?;
    let head = read_at(&mut file, 0, HEAD)?;
    let e = ext(path);
    let mut info = detect(&head, &e, &mut file, meta.len())?;
    info.file_size = meta.len();
    info.file_mtime_unix = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64());
    Ok(info)
}

fn detect(head: &[u8], ext: &str, file: &mut std::fs::File, len: u64) -> Result<ImageInfo> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return jpeg::probe(head, file);
    }
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png::probe(head, file);
    }
    if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") || head.starts_with(b"II+\0") || head.starts_with(b"MM\0+") {
        return tiff::probe_file(file, len);
    }
    if head.starts_with(b"8BPS") {
        return psd::probe(head, file);
    }
    // PDF header may follow up to 1 KiB of junk.
    if head[..head.len().min(1024)].windows(5).any(|w| w == b"%PDF-") {
        let mut i = pdf::probe(file, len)?;
        if ext == "ai" {
            i.format = ImageFormat::Ai;
        }
        return Ok(i);
    }
    if head.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) || head.starts_with(b"%!PS") {
        let mut i = eps::probe(head, file, len)?;
        if ext == "ai" {
            i.format = ImageFormat::Ai;
        }
        return Ok(i);
    }
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        let w = le16(head, 6).unwrap_or(0) as u32;
        let h = le16(head, 8).unwrap_or(0) as u32;
        let mut i = ImageInfo::raster(ImageFormat::Gif, w, h, ColorSpace::Indexed, 8, 1);
        i.alpha = gif_has_transparency(head);
        return Ok(i);
    }
    if head.starts_with(b"BM") && head.len() >= 54 {
        let w = le32(head, 18).unwrap_or(0) as i32;
        let h = le32(head, 22).unwrap_or(0) as i32;
        let bpp = le16(head, 28).unwrap_or(24);
        let (space, bits, ch) = if bpp <= 8 { (ColorSpace::Indexed, bpp as u8, 1) } else { (ColorSpace::Rgb, 8, bpp / 8) };
        let mut i = ImageInfo::raster(ImageFormat::Bmp, w.unsigned_abs(), h.unsigned_abs(), space, bits, ch);
        let (px, py) = (le32(head, 38).unwrap_or(0), le32(head, 42).unwrap_or(0));
        if px > 0 && py > 0 {
            i.ppi = Some((px as f64 * 0.0254, py as f64 * 0.0254));
        }
        return Ok(i);
    }
    if metafile::looks_like_metafile(head).is_some() {
        return metafile::probe(head);
    }
    if ext == "svg" || svg::looks_like_svg(head) {
        return svg::probe(head);
    }
    Err(ProbeError::Unsupported(if ext.is_empty() { "unknown format".into() } else { format!(".{ext}") }))
}

/// Inflates zlib data, refusing output beyond `limit` bytes.
pub(crate) fn inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut d = flate2::read::ZlibDecoder::new(data).take(limit as u64 + 1);
    match d.read_to_end(&mut out) {
        Ok(_) if out.len() <= limit => Some(out),
        // Truncated or slightly damaged streams: keep what decoded.
        Err(_) if !out.is_empty() && out.len() <= limit => Some(out),
        _ => None,
    }
}

/// Photoshop image resources (`8BIM` blocks): resolution info and ICC profile, as found in
/// PSD files, JPEG APP13 and Photoshop EPS.
pub(crate) fn photoshop_resources(mut b: &[u8], info: &mut ImageInfo) {
    while b.len() >= 12 && &b[..4] == b"8BIM" {
        let id = be16(b, 4).unwrap_or(0);
        let name_len = b[6] as usize;
        let mut o = 7 + name_len;
        if o % 2 == 1 {
            o += 1;
        }
        let Some(size) = be32(b, o) else { break };
        let size = size as usize;
        let start = o + 4;
        let Some(data) = b.get(start..start + size) else { break };
        match id {
            0x03ED if data.len() >= 16 => {
                // Fixed 16.16 pixels per inch (the unit field only changes the display unit).
                let h = be32(data, 0).unwrap_or(0) as f64 / 65536.0;
                let v = be32(data, 8).unwrap_or(0) as f64 / 65536.0;
                if h > 0.0 && v > 0.0 && info.ppi.is_none() {
                    info.ppi = Some((h, v));
                }
            }
            0x040F if info.icc.is_none() && !data.is_empty() => info.icc = Some(data.to_vec()),
            _ => {}
        }
        let mut next = start + size;
        if next % 2 == 1 {
            next += 1;
        }
        if next > b.len() {
            break;
        }
        b = &b[next..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_media_and_text() {
        let dir = std::env::temp_dir().join(format!("folio-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.csv");
        std::fs::write(&p, b"a,b\n1,2\n").unwrap();
        assert!(matches!(probe(&p), Err(ProbeError::Unsupported(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_transparent_colour_is_seen_in_the_graphic_control_extension() {
        // header, 1 x 1 screen, 2-colour table, a graphic control extension (flags 1 = transparent), the image
        let mut g = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00".to_vec();
        let opaque = g.clone();
        g.extend_from_slice(b"\x21\xf9\x04\x01\x00\x00\x00\x00,");
        assert!(gif_has_transparency(&g));
        let mut o = opaque;
        o.extend_from_slice(b"\x21\xf9\x04\x00\x00\x00\x00\x00,");
        assert!(!gif_has_transparency(&o));
    }

    #[test]
    fn a_bmp_is_probed_with_its_bit_depth() {
        let dir = std::env::temp_dir().join(format!("folio-probe-bmp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 54 bytes of header: a 4 x 2, 24-bit BMP at 2835 pixels per metre (72 ppi)
        let mut b = vec![0u8; 54 + 24];
        b[..2].copy_from_slice(b"BM");
        b[10..14].copy_from_slice(&54u32.to_le_bytes());
        b[14..18].copy_from_slice(&40u32.to_le_bytes());
        b[18..22].copy_from_slice(&4i32.to_le_bytes());
        b[22..26].copy_from_slice(&2i32.to_le_bytes());
        b[26..28].copy_from_slice(&1u16.to_le_bytes());
        b[28..30].copy_from_slice(&24u16.to_le_bytes());
        b[38..42].copy_from_slice(&2835u32.to_le_bytes());
        b[42..46].copy_from_slice(&2835u32.to_le_bytes());
        let p = dir.join("a.bmp");
        std::fs::write(&p, &b).unwrap();
        let i = probe(&p).unwrap();
        assert_eq!((i.format, i.width, i.height, i.color_space, i.channels), (ImageFormat::Bmp, 4, 2, ColorSpace::Rgb, 3));
        assert!((i.ppi.unwrap().0 - 72.009).abs() < 1e-3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
