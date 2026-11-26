//! Windows metafiles (EMF, WMF; build-a): the size in points from the header (EMF: the frame in 0.01 mm; WMF: the placeable box and its
//! units per inch, else the window of the records at 1440 per inch). The drawing itself is `folio_render::metafile`.
use super::{ColorSpace, ImageFormat, ImageInfo, Result};

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}
fn i16le(b: &[u8], at: usize) -> Option<f64> {
    u16le(b, at).map(|v| f64::from(v as i16))
}
fn i32le(b: &[u8], at: usize) -> Option<f64> {
    b.get(at..at + 4).map(|x| f64::from(i32::from_le_bytes([x[0], x[1], x[2], x[3]])))
}

/// Whether the header is an EMF's or a WMF's.
pub fn looks_like_metafile(head: &[u8]) -> Option<&'static str> {
    if head.len() >= 44 && head[..4] == [1, 0, 0, 0] && &head[40..44] == b" EMF" {
        return Some("EMF");
    }
    if head.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]) || (head.len() >= 6 && matches!(u16le(head, 0), Some(1 | 2)) && u16le(head, 2) == Some(9) && matches!(u16le(head, 4), Some(0x0100 | 0x0300))) {
        return Some("WMF");
    }
    None
}

/// The size in points of the metafile `b` (its whole bytes).
pub fn size(b: &[u8]) -> Option<[f64; 2]> {
    match looks_like_metafile(b)? {
        "EMF" => {
            let (l, t, r, bt) = (i32le(b, 24)?, i32le(b, 28)?, i32le(b, 32)?, i32le(b, 36)?);
            let mm = |v: f64| v / 100.0 * 72.0 / 25.4;
            (r > l && bt > t).then(|| [mm(r - l), mm(bt - t)])
        }
        _ if b.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]) => {
            let (l, t, r, bt) = (i16le(b, 6)?, i16le(b, 8)?, i16le(b, 10)?, i16le(b, 12)?);
            let inch = f64::from(u16le(b, 14)?.max(1));
            (r > l && bt > t).then(|| [(r - l) / inch * 72.0, (bt - t) / inch * 72.0])
        }
        _ => {
            // the window extent of the records (SETWINDOWEXT 0x020C), at 1440 per inch
            let mut at = usize::from(u16le(b, 2)?) * 2;
            while at + 6 <= b.len() {
                let words = b.get(at..at + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize)?;
                if words < 3 {
                    return None;
                }
                if u16le(b, at + 4)? == 0x020C {
                    let (h, w) = (i16le(b, at + 6)?.abs(), i16le(b, at + 8)?.abs());
                    return (w > 0.0 && h > 0.0).then(|| [w / 1440.0 * 72.0, h / 1440.0 * 72.0]);
                }
                at += words * 2;
            }
            None
        }
    }
}

pub(crate) fn probe(b: &[u8]) -> Result<ImageInfo> {
    // a metafile whose size cannot be read is refused by name (the place query and the command say "EMF·WMF")
    let Some(s) = size(b) else {
        return Err(super::ProbeError::Unsupported(if looks_like_metafile(b) == Some("EMF") { ".emf" } else { ".wmf" }.into()));
    };
    let mut info = ImageInfo::raster(ImageFormat::Wmf, s[0].ceil() as u32, s[1].ceil() as u32, ColorSpace::Rgb, 0, 0);
    info.vector = true;
    Ok(info)
}
