//! Photoshop PSD / PSB: header, colour mode, image resources (resolution, ICC).

use super::{ColorSpace, ImageFormat, ImageInfo, Result, be16, be32, malformed, photoshop_resources, read_at};

pub(crate) fn probe(head: &[u8], file: &mut std::fs::File) -> Result<ImageInfo> {
    if head.len() < 30 {
        return malformed("PSD header");
    }
    let channels = be16(head, 12).unwrap_or(0);
    let height = be32(head, 14).unwrap_or(0);
    let width = be32(head, 18).unwrap_or(0);
    let depth = be16(head, 22).unwrap_or(8) as u8;
    let mode = be16(head, 24).unwrap_or(3);
    let (space, base) = match mode {
        0 => (ColorSpace::Gray, 1), // bitmap (1 bit)
        1 => (ColorSpace::Gray, 1),
        2 => (ColorSpace::Indexed, 1),
        3 => (ColorSpace::Rgb, 3),
        4 => (ColorSpace::Cmyk, 4),
        9 => (ColorSpace::Lab, 3),
        _ => (ColorSpace::Other, channels), // multichannel, duotone
    };
    let mut info = ImageInfo::raster(ImageFormat::Psd, width, height, space, depth, channels);
    info.alpha = channels > base;
    let mode_len = be32(head, 26).unwrap_or(0) as u64;
    let res_at = 30 + mode_len;
    let res_len_bytes = read_at(file, res_at, 4)?;
    let res_len = be32(&res_len_bytes, 0).unwrap_or(0) as usize;
    if res_len > 0 && res_len <= 64 << 20 {
        let res = read_at(file, res_at + 4, res_len)?;
        photoshop_resources(&res, &mut info);
    }
    Ok(info)
}
