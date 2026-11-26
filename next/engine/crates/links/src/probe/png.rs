//! PNG: IHDR, pHYs, iCCP (inflated). Stops at the first IDAT.

use super::{ColorSpace, ImageFormat, ImageInfo, Result, be32, inflate, malformed, read_at};

pub(crate) fn probe(head: &[u8], file: &mut std::fs::File) -> Result<ImageInfo> {
    let mut buf = head.to_vec();
    let mut o = 8;
    let mut info: Option<ImageInfo> = None;
    loop {
        if o + 8 > buf.len() {
            let extra = read_at(file, buf.len() as u64, 4 << 20)?;
            if extra.is_empty() {
                break;
            }
            buf.extend_from_slice(&extra);
            continue;
        }
        let len = be32(&buf, o).unwrap_or(0) as usize;
        let typ = &buf[o + 4..o + 8].to_vec();
        if typ == b"IDAT" || typ == b"IEND" {
            break;
        }
        if o + 12 + len > buf.len() {
            if len > 64 << 20 {
                return malformed("PNG chunk too large");
            }
            let extra = read_at(file, buf.len() as u64, (o + 12 + len - buf.len()).max(1 << 16))?;
            if extra.is_empty() {
                break;
            }
            buf.extend_from_slice(&extra);
            continue;
        }
        let data = &buf[o + 8..o + 8 + len];
        match typ.as_slice() {
            b"IHDR" if len >= 13 => {
                let w = be32(data, 0).unwrap_or(0);
                let h = be32(data, 4).unwrap_or(0);
                let bits = data[8];
                let (space, ch, alpha) = match data[9] {
                    0 => (ColorSpace::Gray, 1, false),
                    2 => (ColorSpace::Rgb, 3, false),
                    3 => (ColorSpace::Indexed, 1, false),
                    4 => (ColorSpace::Gray, 2, true),
                    6 => (ColorSpace::Rgb, 4, true),
                    _ => return malformed("PNG colour type"),
                };
                let mut i = ImageInfo::raster(ImageFormat::Png, w, h, space, bits, ch);
                i.alpha = alpha;
                info = Some(i);
            }
            b"pHYs" if len >= 9 => {
                if let Some(i) = info.as_mut()
                    && data[8] == 1
                {
                    let (x, y) = (be32(data, 0).unwrap_or(0) as f64, be32(data, 4).unwrap_or(0) as f64);
                    if x > 0.0 && y > 0.0 {
                        i.ppi = Some((x * 0.0254, y * 0.0254));
                    }
                }
            }
            b"iCCP" => {
                if let Some(i) = info.as_mut()
                    && let Some(z) = data.iter().position(|&c| c == 0)
                    && z + 2 <= data.len()
                {
                    i.icc = inflate(&data[z + 2..], 16 << 20);
                }
            }
            b"tRNS" => {
                if let Some(i) = info.as_mut() {
                    i.alpha = true;
                }
            }
            _ => {}
        }
        o += 12 + len;
    }
    info.ok_or_else(|| super::ProbeError::Malformed("PNG without IHDR".into()))
}
