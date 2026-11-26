//! JPEG: SOF size and components, JFIF / EXIF / Photoshop resolution, ICC (APP2 chunks),
//! Adobe APP14 (CMYK / YCCK). Stops at the first scan.

use super::{ColorSpace, ImageFormat, ImageInfo, Result, be16, malformed, photoshop_resources, read_at};

pub(crate) fn probe(head: &[u8], file: &mut std::fs::File) -> Result<ImageInfo> {
    // Markers before the first scan are normally within the head; large EXIF thumbnails or
    // ICC profiles may push SOF further, so read on when needed.
    let mut buf = head.to_vec();
    let mut more = true;
    loop {
        match parse(&buf) {
            Ok(Some(info)) => return Ok(info),
            Ok(None) if more => {
                let extra = read_at(file, buf.len() as u64, 4 << 20)?;
                more = extra.len() == 4 << 20;
                if extra.is_empty() {
                    return malformed("JPEG without frame header");
                }
                buf.extend_from_slice(&extra);
            }
            Ok(None) => return malformed("JPEG without frame header"),
            Err(e) => return Err(e),
        }
    }
}

/// `Ok(None)`: more bytes are needed.
fn parse(b: &[u8]) -> Result<Option<ImageInfo>> {
    let mut o = 2;
    let mut jfif: Option<(f64, f64)> = None;
    let mut exif: (Option<(f64, f64)>, Option<u16>) = (None, None);
    let mut icc_chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut adobe_transform: Option<u8> = None;
    let mut ps = ImageInfo::raster(ImageFormat::Jpeg, 0, 0, ColorSpace::Other, 8, 0);
    loop {
        // skip fill bytes
        while o < b.len() && b[o] == 0xFF && b.get(o + 1) == Some(&0xFF) {
            o += 1;
        }
        if o + 4 > b.len() {
            return Ok(None);
        }
        if b[o] != 0xFF {
            return malformed("JPEG marker expected");
        }
        let m = b[o + 1];
        if m == 0xD8 || (0xD0..=0xD7).contains(&m) || m == 0x01 {
            o += 2;
            continue;
        }
        if m == 0xD9 || m == 0xDA {
            return malformed("JPEG scan before frame header");
        }
        let len = be16(b, o + 2).unwrap_or(0) as usize;
        if len < 2 {
            return malformed("JPEG segment length");
        }
        let Some(seg) = b.get(o + 4..o + 2 + len) else { return Ok(None) };
        match m {
            0xE0 if seg.starts_with(b"JFIF\0") && seg.len() >= 12 => {
                let unit = seg[7];
                let (x, y) = (be16(seg, 8).unwrap_or(0) as f64, be16(seg, 10).unwrap_or(0) as f64);
                if x > 0.0 && y > 0.0 {
                    jfif = match unit {
                        1 => Some((x, y)),
                        2 => Some((x * 2.54, y * 2.54)),
                        _ => None,
                    };
                }
            }
            0xE1 if seg.starts_with(b"Exif\0\0") => exif = super::tiff::exif(&seg[6..]),
            0xE2 if seg.starts_with(b"ICC_PROFILE\0") && seg.len() > 14 => {
                icc_chunks.push((seg[12], seg[14..].to_vec()));
            }
            0xED if seg.starts_with(b"Photoshop 3.0\0") => photoshop_resources(&seg[14..], &mut ps),
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe_transform = Some(seg[11]),
            0xC0..=0xCF if m != 0xC4 && m != 0xC8 && m != 0xCC => {
                if seg.len() < 6 {
                    return malformed("JPEG frame header");
                }
                let bits = seg[0];
                let height = be16(seg, 1).unwrap_or(0) as u32;
                let width = be16(seg, 3).unwrap_or(0) as u32;
                let comps = seg[5] as u16;
                let space = match comps {
                    1 => ColorSpace::Gray,
                    3 => ColorSpace::Rgb,
                    4 => ColorSpace::Cmyk,
                    _ => ColorSpace::Other,
                };
                let _ = adobe_transform; // CMYK either way (YCCK is CMYK after decoding)
                let mut info = ImageInfo::raster(ImageFormat::Jpeg, width, height, space, bits, comps);
                // Photoshop's own resolution wins over JFIF (as InDesign reads it), then EXIF.
                info.ppi = ps.ppi.or(jfif).or(exif.0);
                info.orientation = exif.1;
                if !icc_chunks.is_empty() {
                    icc_chunks.sort_by_key(|c| c.0);
                    info.icc = Some(icc_chunks.into_iter().flat_map(|c| c.1).collect());
                } else {
                    info.icc = ps.icc;
                }
                return Ok(Some(info));
            }
            _ => {}
        }
        o += 2 + len;
    }
}
