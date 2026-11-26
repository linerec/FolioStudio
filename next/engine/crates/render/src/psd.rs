//! The merged (composite) image of a Photoshop file, for drawing a placed PSD: 8-bit grayscale,
//! RGB or CMYK, raw or RLE (PackBits) compressed, channels stored one after another. The
//! layers are not read: InDesign also draws a placed PSD from its composite unless layer
//! visibility is overridden.

/// What the composite holds.
pub(crate) enum Composite {
    Gray(Vec<u8>),
    Rgb(Vec<u8>),
    /// Ink per channel (0 = none, 255 = 100 %), C M Y K per pixel.
    Cmyk(Vec<u8>),
}

fn be16(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn be32(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// Width, height, the composite of a PSD (version 1, 8 or 16 bits per channel; 16 bits keep the high byte, as the print output does) and its merged
/// transparency: the first alpha channel when the layer count is negative (what Photoshop
/// writes for a transparent composite, and what a composition of switched layers writes).
pub(crate) fn composite(b: &[u8]) -> Option<(u32, u32, Composite, Option<Vec<u8>>)> {
    if !b.starts_with(b"8BPS") || be16(b, 4)? != 1 {
        return None;
    }
    let channels = be16(b, 12)?;
    let (h, w) = (be32(b, 14)?, be32(b, 18)?);
    let (depth, mode) = (be16(b, 22)?, be16(b, 24)?);
    let used = match mode {
        1 => 1, // grayscale
        3 | 9 => 3, // RGB, Lab
        4 => 4, // CMYK
        _ => return None,
    };
    if !matches!(depth, 8 | 16) || channels < used || w == 0 || h == 0 || w.checked_mul(h)? > 1 << 28 {
        return None;
    }
    // colour mode data, image resources, layer and mask information: skipped
    let mut at: usize = 26;
    let mut merged_alpha = false;
    for section in 0..3 {
        if section == 2 && be32(b, at)? >= 8 && be32(b, at + 4)? >= 2 {
            merged_alpha = (be16(b, at + 8)? as u16 as i16) < 0;
        }
        at = at.checked_add(4)?.checked_add(be32(b, at)?)?;
    }
    let compression = be16(b, at)?;
    at += 2;
    let plane = w * h;
    let alpha_channel = (merged_alpha && channels > used).then_some(used);
    let stored = used + usize::from(alpha_channel.is_some());
    let bpc = depth / 8;
    let mut planes = vec![0u8; plane * bpc * stored];
    match compression {
        0 => {
            let data = b.get(at..at + plane * bpc * stored)?;
            planes.copy_from_slice(data);
        }
        1 => {
            // byte counts of every row of every channel, then the PackBits rows
            let counts = at;
            let mut src = at + channels * h * 2;
            let row_len = w * bpc;
            for c in 0..channels {
                for y in 0..h {
                    let n = be16(b, counts + (c * h + y) * 2)?;
                    let row = b.get(src..src + n)?;
                    src += n;
                    if c < stored {
                        unpack(row, &mut planes[(c * h + y) * row_len..(c * h + y + 1) * row_len])?;
                    }
                }
            }
        }
        _ => return None,
    }
    if bpc == 2 {
        planes = planes.chunks_exact(2).map(|s| s[0]).collect();
    }
    let alpha = alpha_channel.map(|c| planes[c * plane..(c + 1) * plane].to_vec());
    planes.truncate(plane * used);
    let interleave = |k: usize| -> Vec<u8> {
        let mut out = vec![0u8; plane * k];
        for p in 0..plane {
            for c in 0..k {
                out[p * k + c] = planes[c * plane + p];
            }
        }
        out
    };
    let image = match mode {
        1 => Composite::Gray(planes),
        3 => Composite::Rgb(interleave(3)),
        // Lab (8 or 16 bits; build-a2): L 0–255 for 0–100, a and b offset by 128, shown in sRGB as a CIELab TIFF is (D50)
        9 => Composite::Rgb(interleave(3).chunks_exact(3).flat_map(|p| crate::lab8_to_srgb(p[0], p[1] ^ 0x80, p[2] ^ 0x80)).collect()),
        // Photoshop stores CMYK as 255 − ink
        _ => Composite::Cmyk(interleave(4).into_iter().map(|v| 255 - v).collect()),
    };
    Some((w as u32, h as u32, image, alpha))
}

/// Every channel of a PSD's image data as 8-bit planes (16 bits keep the high byte), for clipping
/// path extraction (`clipextract`): width, height, the number of colour channels,
/// whether the first extra channel is the merged transparency, the planes, and the image resources.
pub(crate) struct Planes<'a> {
    pub w: usize,
    pub h: usize,
    pub colors: usize,
    pub merged_alpha: bool,
    pub planes: Vec<Vec<u8>>,
    pub resources: &'a [u8],
}

pub(crate) fn planes(b: &[u8]) -> Option<Planes<'_>> {
    if !b.starts_with(b"8BPS") || be16(b, 4)? != 1 {
        return None;
    }
    let channels = be16(b, 12)?;
    let (h, w) = (be32(b, 14)?, be32(b, 18)?);
    let (depth, mode) = (be16(b, 22)?, be16(b, 24)?);
    let colors = match mode { 1 => 1, 3 => 3, 4 => 4, _ => return None };
    if !matches!(depth, 8 | 16) || channels < colors || w == 0 || h == 0 || w.checked_mul(h)? > 1 << 28 {
        return None;
    }
    let mut at: usize = 26;
    at = at.checked_add(4)?.checked_add(be32(b, at)?)?;
    let resources = b.get(at + 4..at + 4 + be32(b, at)?)?;
    at = at.checked_add(4)?.checked_add(be32(b, at)?)?;
    let merged_alpha = be32(b, at)? >= 8 && be32(b, at + 4)? >= 2 && (be16(b, at + 8)? as u16 as i16) < 0;
    at = at.checked_add(4)?.checked_add(be32(b, at)?)?;
    let compression = be16(b, at)?;
    at += 2;
    let bpc = depth / 8;
    let row_len = w * bpc;
    let mut planes = Vec::with_capacity(channels);
    match compression {
        0 => {
            for c in 0..channels {
                planes.push(b.get(at + c * h * row_len..at + (c + 1) * h * row_len)?.to_vec());
            }
        }
        1 => {
            let counts = at;
            let mut src = at + channels * h * 2;
            for c in 0..channels {
                let mut plane = vec![0u8; h * row_len];
                for y in 0..h {
                    let n = be16(b, counts + (c * h + y) * 2)?;
                    unpack(b.get(src..src + n)?, &mut plane[y * row_len..(y + 1) * row_len])?;
                    src += n;
                }
                planes.push(plane);
            }
        }
        _ => return None,
    }
    if bpc == 2 {
        planes = planes.into_iter().map(|p| p.chunks_exact(2).map(|s| s[0]).collect()).collect();
    }
    Some(Planes { w, h, colors, merged_alpha, planes, resources })
}

/// PackBits into `out` (exactly filled).
fn unpack(src: &[u8], out: &mut [u8]) -> Option<()> {
    let (mut i, mut o) = (0, 0);
    while o < out.len() {
        let n = *src.get(i)? as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            out.get_mut(o..o + k)?.copy_from_slice(src.get(i..i + k)?);
            i += k;
            o += k;
        } else if n != -128 {
            let k = (1 - n as isize) as usize;
            let v = *src.get(i)?;
            i += 1;
            out.get_mut(o..o + k)?.fill(v);
            o += k;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 x 1 RGB PSD with a fourth channel; `count` is the layer count written.
    fn tiny(count: Option<i16>) -> Vec<u8> {
        let mut b = b"8BPS".to_vec();
        b.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 0, 2, 0, 8, 0, 3]);
        b.extend_from_slice(&[0; 8]); // colour mode data, image resources
        match count {
            Some(c) => {
                b.extend_from_slice(&10u32.to_be_bytes());
                b.extend_from_slice(&2u32.to_be_bytes());
                b.extend_from_slice(&c.to_be_bytes());
                b.extend_from_slice(&0u32.to_be_bytes());
            }
            None => b.extend_from_slice(&0u32.to_be_bytes()),
        }
        b.extend_from_slice(&[0, 0]); // raw
        b.extend_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80]); // R G B A, one row each
        b
    }

    #[test]
    fn truncated_composites_and_packbits_return_none() {
        let full = tiny(Some(-1));
        for end in 0..full.len() {
            assert!(composite(&full[..end]).is_none(), "PSD prefix {end}");
        }
        for bad in [&[][..], &[2, 1, 2][..], &[0xFE][..]] {
            assert!(unpack(bad, &mut [0; 3]).is_none());
        }
        let mut no_channels = full.clone();
        no_channels[12..14].fill(0);
        assert!(composite(&no_channels).is_none());
        for offset in [14, 18, 22, 24] {
            let mut bad = full.clone();
            bad[offset..offset + 2].fill(0xff);
            assert!(composite(&bad).is_none(), "invalid PSD field at {offset}");
        }
    }

    #[test]
    fn merged_transparency_is_the_first_alpha_only_when_announced() {
        let (_, _, image, alpha) = composite(&tiny(Some(-1))).unwrap();
        assert!(matches!(image, Composite::Rgb(ref p) if p == &[10, 30, 50, 20, 40, 60]));
        assert_eq!(alpha, Some(vec![70, 80]));
        // an alpha channel without the announcement is a saved selection, not transparency
        assert_eq!(composite(&tiny(None)).unwrap().3, None);
    }

    /// A 1 x 1 Lab PSD (8 or 16 bits) of L, a, b as Photoshop stores them (a and b offset by half the range).
    fn lab(depth: u8, l: u16, a: u16, b_: u16) -> Vec<u8> {
        let mut b = b"8BPS".to_vec();
        b.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 1, 0, depth, 0, 9]);
        b.extend_from_slice(&[0; 12]); // colour mode data, image resources, layers
        b.extend_from_slice(&[0, 0]); // raw
        for v in [l, a, b_] {
            if depth == 16 { b.extend_from_slice(&v.to_be_bytes()) } else { b.push(v as u8) }
        }
        b
    }

    #[test]
    fn lab_composites_in_8_and_16_bits_are_shown_like_a_lab_tiff() {
        // L 136, a +80, b +67 (TIFF: signed bytes): InDesign prints (250, 7, 8) (lab_tests)
        let want = crate::lab8_to_srgb(136, 80, 67);
        for psd in [lab(8, 136, 128 + 80, 128 + 67), lab(16, 136 << 8 | 0x7F, (128 + 80) << 8, (128 + 67) << 8 | 0xFF)] {
            let (w, h, image, alpha) = composite(&psd).unwrap();
            assert_eq!((w, h, alpha), (1, 1, None));
            assert!(matches!(image, Composite::Rgb(ref p) if p[..] == want[..]));
        }
    }

    #[test]
    fn packbits_runs_and_literals() {
        let mut out = [0u8; 6];
        unpack(&[0xFE, 7, 2, 1, 2, 3], &mut out).unwrap();
        assert_eq!(out, [7, 7, 7, 1, 2, 3]);
    }
}
