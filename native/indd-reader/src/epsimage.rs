//! Lossless wrapper for the observed EPSImage chunky 8-bit gray/RGB/CMYK form.
//! This is a display preview, without an ICC profile or print-color guarantee.
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

#[derive(Debug, PartialEq)]
pub struct Wrapped {
    pub file: String,
    pub bytes: u64,
    pub channels: u32,
}

fn tiff_header(
    raw: &[u8; 28],
    length: u64,
    color_space: Option<u32>,
) -> io::Result<(Vec<u8>, u64, u32)> {
    let field = |n| u32::from_le_bytes(raw[n..n + 4].try_into().unwrap());
    let (stride, width, height, bits, channels) =
        (field(8), field(12), field(16), field(20), field(24));
    let pixels = u64::from(stride) * u64::from(height);
    if &raw[..8] != b"EPSImage"
        || width == 0
        || height == 0
        || bits != 8
        || !matches!(
            (channels, color_space),
            (1, Some(1)) | (3, Some(2)) | (4, Some(4))
        )
        || u64::from(stride) != u64::from(width) * u64::from(channels)
        || pixels + 28 != length
        || pixels > 64 * 1024 * 1024
    {
        return Err(io::Error::other("unsupported or malformed EPSImage layout"));
    }
    let entries = if channels == 4 { 11u16 } else { 10u16 };
    let bits_offset = 8 + 2 + u32::from(entries) * 12 + 4;
    let pixel_offset = bits_offset + if channels == 1 { 0 } else { channels * 2 };
    let mut out = Vec::with_capacity(pixel_offset as usize);
    out.extend_from_slice(b"II\x2a\x00\x08\x00\x00\x00");
    out.extend_from_slice(&entries.to_le_bytes());
    let mut tag = |id: u16, kind: u16, count: u32, value: u32| {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    };
    tag(256, 4, 1, width);
    tag(257, 4, 1, height);
    tag(
        258,
        3,
        channels,
        if channels == 1 { 8 } else { bits_offset },
    );
    tag(259, 3, 1, 1); // Uncompressed.
    tag(
        262,
        3,
        1,
        match channels {
            1 => 1,
            4 => 5,
            _ => 2,
        },
    ); // Gray / CMYK / RGB.
    tag(273, 4, 1, pixel_offset);
    tag(277, 3, 1, channels);
    tag(278, 4, 1, height);
    tag(279, 4, 1, pixels as u32);
    tag(284, 3, 1, 1); // Chunky interleaved samples.
    if channels == 4 {
        tag(332, 3, 1, 1);
    } // InkSet: CMYK.
    out.extend_from_slice(&0u32.to_le_bytes());
    if channels != 1 {
        for _ in 0..channels {
            out.extend_from_slice(&8u16.to_le_bytes());
        }
    }
    Ok((out, pixels, channels))
}

/// Leaves the original raw file intact; only the derived TIFF is advertised.
pub fn wrap(
    folder: &Path,
    raw_name: &str,
    color_space: Option<u32>,
) -> io::Result<Option<Wrapped>> {
    let mut input = fs::File::open(folder.join(raw_name))?;
    let length = input.metadata()?.len();
    if length < 8 {
        return Ok(None);
    }
    let mut raw = [0; 28];
    input.read_exact(&mut raw[..8])?;
    if &raw[..8] != b"EPSImage" {
        return Ok(None);
    }
    input.read_exact(&mut raw[8..])?;
    let (header, pixel_bytes, channels) = tiff_header(&raw, length, color_space)?;
    let name = format!("{}.tif", raw_name.trim_end_matches(".bin"));
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(folder.join(&name))?;
    output.write_all(&header)?;
    let copied = io::copy(&mut input.take(pixel_bytes), &mut output)?;
    if copied != pixel_bytes {
        return Err(io::Error::other("EPSImage pixels truncated"));
    }
    output.flush()?;
    Ok(Some(Wrapped {
        file: name,
        bytes: header.len() as u64 + pixel_bytes,
        channels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(width: u32, height: u32, channels: u32) -> [u8; 28] {
        let mut h = [0; 28];
        h[..8].copy_from_slice(b"EPSImage");
        for (i, n) in [width * channels, width, height, 8, channels]
            .iter()
            .enumerate()
        {
            h[8 + i * 4..12 + i * 4].copy_from_slice(&n.to_le_bytes());
        }
        h
    }
    #[test]
    fn validates_layout_before_writing() {
        for (channels, color) in [(1, 1), (3, 2), (4, 4)] {
            let r = raw(2, 3, channels);
            let (header, pixels, actual) =
                tiff_header(&r, 28 + 6 * u64::from(channels), Some(color)).unwrap();
            assert_eq!(actual, channels);
            assert_eq!(pixels, 6 * u64::from(channels));
            assert_eq!(&header[..4], b"II\x2a\x00");
            assert!(tiff_header(&r, 28 + 6 * u64::from(channels) - 1, Some(color)).is_err());
            assert!(tiff_header(&r, 28 + 6 * u64::from(channels), None).is_err());
        }
        assert!(tiff_header(&raw(0, 2, 3), 28, Some(2)).is_err());
        assert!(tiff_header(&raw(1, 1, 2), 30, Some(2)).is_err());
        assert!(tiff_header(&raw(1, 1, 4), 32, Some(2)).is_err());
        let mut r = raw(2, 3, 4);
        r[20] = 16;
        assert!(tiff_header(&r, 52, Some(4)).is_err());
        let mut r = raw(2, 3, 4);
        r[8] = 9;
        assert!(tiff_header(&r, 55, Some(4)).is_err());
    }
}
