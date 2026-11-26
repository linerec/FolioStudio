//! GIF (first image): colour table, LZW, interlacing, and the transparent colour of the graphic control extension
//! (screens-import: a transparent index is not painted; `decode` keeps the opaque RGB of placeimg-c1, `decode_rgba` the alpha).

/// Width, height and RGB samples of the first image of a GIF, on a logical screen of its size (a transparent colour is painted
/// with its colour-table entry).
pub fn decode(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h, rgba, _) = decode_rgba(b)?;
    Some((w, h, rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()))
}

/// Width, height and RGBA samples of the first image of a GIF, and whether a transparent colour is in the picture (a pixel of the
/// transparent index has alpha 0; the screen outside the image is the background colour, opaque).
pub fn decode_rgba(b: &[u8]) -> Option<(u32, u32, Vec<u8>, bool)> {
    decode_rgba_checked(b).ok()
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeErrorKind { InvalidData, InvalidLzwCodeSize }

#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub kind: DecodeErrorKind,
    pub offset: usize,
}

/// Decode with an error category and byte offset; the Option helpers retain their existing contract.
pub fn decode_rgba_checked(b: &[u8]) -> Result<(u32, u32, Vec<u8>, bool), DecodeError> {
    let invalid = |offset| DecodeError { kind: DecodeErrorKind::InvalidData, offset };
    if !(b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) || b.len() < 13 {
        return Err(invalid(0));
    }
    let u16_at = |o: usize| b.get(o..o + 2).map(|v| u16::from_le_bytes([v[0], v[1]]) as usize);
    let (sw, sh) = (u16_at(6).ok_or_else(|| invalid(6))?, u16_at(8).ok_or_else(|| invalid(8))?);
    let packed = *b.get(10).ok_or_else(|| invalid(10))?;
    let mut o = 13;
    let mut global: &[u8] = &[];
    if packed & 0x80 != 0 {
        let n = 3 << ((packed & 7) + 1);
        global = b.get(o..o + n).ok_or_else(|| invalid(o))?;
        o += n;
    }
    let bg = *b.get(11).ok_or_else(|| invalid(11))? as usize;
    let mut transparent: Option<usize> = None;
    loop {
        match *b.get(o).ok_or_else(|| invalid(o))? {
            0x21 => {
                // a graphic control extension (0xF9, 4 bytes): flags bit 0 = a transparent colour, its index in byte 3
                if b.get(o + 1) == Some(&0xF9) && b.get(o + 2) == Some(&4) && b.get(o + 3).is_some_and(|f| f & 1 == 1) {
                    transparent = b.get(o + 6).map(|&i| i as usize);
                }
                o += 2;
                loop {
                    let n = *b.get(o).ok_or_else(|| invalid(o))? as usize;
                    o += 1 + n;
                    if n == 0 {
                        break;
                    }
                }
            }
            0x2C => {
                let (left, top, w, h) = (u16_at(o + 1).ok_or_else(|| invalid(o + 1))?, u16_at(o + 3).ok_or_else(|| invalid(o + 3))?, u16_at(o + 5).ok_or_else(|| invalid(o + 5))?, u16_at(o + 7).ok_or_else(|| invalid(o + 7))?);
                let ip = *b.get(o + 9).ok_or_else(|| invalid(o + 9))?;
                o += 10;
                let mut table = global;
                if ip & 0x80 != 0 {
                    let n = 3 << ((ip & 7) + 1);
                    table = b.get(o..o + n).ok_or_else(|| invalid(o))?;
                    o += n;
                }
                let min = *b.get(o).ok_or_else(|| invalid(o))?;
                if !(2..=8).contains(&min) {
                    return Err(DecodeError { kind: DecodeErrorKind::InvalidLzwCodeSize, offset: o });
                }
                o += 1;
                let mut data = Vec::new();
                loop {
                    let n = *b.get(o).ok_or_else(|| invalid(o))? as usize;
                    o += 1;
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(b.get(o..o + n).ok_or_else(|| invalid(o))?);
                    o += n;
                }
                let idx = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, min).decode(&data).map_err(|_| invalid(o))?;
                if idx.len() < w * h || w == 0 || h == 0 {
                    return Err(invalid(o));
                }
                // row order of an interlaced image: every 8th from 0, every 8th from 4, every 4th from 2, every 2nd from 1
                let rows: Vec<usize> = if ip & 0x40 != 0 {
                    [(0, 8), (4, 8), (2, 4), (1, 2)].iter().flat_map(|&(s, st)| (s..h).step_by(st)).collect()
                } else {
                    (0..h).collect()
                };
                let colour = |i: usize| table.get(i * 3..i * 3 + 3).map(|c| [c[0], c[1], c[2]]).unwrap_or([0, 0, 0]);
                let background = colour(bg);
                let mut out: Vec<u8> = (0..sw * sh).flat_map(|_| [background[0], background[1], background[2], 255]).collect();
                let mut any_transparent = false;
                for (k, &y) in rows.iter().enumerate() {
                    for x in 0..w {
                        let (dx, dy) = (left + x, top + y);
                        if dx < sw && dy < sh {
                            let index = idx[k * w + x] as usize;
                            let c = colour(index);
                            let alpha = if transparent == Some(index) { any_transparent = true; 0 } else { 255 };
                            out[(dy * sw + dx) * 4..][..4].copy_from_slice(&[c[0], c[1], c[2], alpha]);
                        }
                    }
                }
                return Ok((sw as u32, sh as u32, out, any_transparent));
            }
            _ => return Err(invalid(o)),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bugfind_invalid_lzw_size_is_rejected() {
        let mut gif = vec![71,73,70,56,57,97,1,0,1,0,128,0,0,255,255,255,0,0,0,44,0,0,0,0,1,0,1,0,0,2,2,68,1,0,59];
        for size in [13, 255, 0, 1, 9, 12] {
            gif[29] = size;
            assert_eq!(super::decode(&gif), None, "minimum code size {size}");
            assert_eq!(super::decode_rgba_checked(&gif).unwrap_err(), super::DecodeError {
                kind: super::DecodeErrorKind::InvalidLzwCodeSize, offset: 29,
            });
        }
    }
    #[test]
    fn a_one_pixel_gif_decodes_to_its_palette_entry() {
        // the classic 1 × 1 GIF: palette white, black; index 0
        let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
        assert_eq!(super::decode(gif), Some((1, 1, vec![255, 255, 255])));
        assert_eq!(super::decode(b"GIF89a"), None);
        for end in 0..gif.len() - 1 {
            assert_eq!(super::decode(&gif[..end]), None, "GIF prefix {end}");
        }
    }

    #[test]
    fn the_transparent_index_has_no_alpha() {
        // a 1 x 1 GIF whose only pixel (index 0) is the transparent colour
        let mut g = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00".to_vec();
        g.extend_from_slice(b"\x21\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;");
        assert_eq!(super::decode_rgba(&g), Some((1, 1, vec![255, 255, 255, 0], true)));
        // without the extension the same picture is opaque
        let opaque = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
        assert_eq!(super::decode_rgba(opaque), Some((1, 1, vec![255, 255, 255, 255], false)));
    }
}
