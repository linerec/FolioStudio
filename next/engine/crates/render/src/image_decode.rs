//! Decodes preview images and reads the sample and alpha information used by renderers.

use crate::{gif, psd};
use folio_color::{BlendSpace, Colors};
use tiny_skia::Pixmap;

/// How a preview is drawn: the page's blend space (the same file shows differently on a
/// page composited in CMYK) and whether it is the proxy of a placed PDF or EPS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageMode {
    pub blend: BlendSpace,
    /// A placed PDF / EPS proxy ([`is_proxy`]): its alpha as a 1-bit mask (α > 0 opaque)
    /// over its colour as stored. InDesign draws the stored colour opaque; the fully
    /// transparent pixels (white in the reader's proxies) are left out, so a placed file's
    /// clear background shows what is below.
    pub proxy: bool,
}

/// Whether graphic element `element` is drawn from a proxy whose alpha is a 1-bit mask
/// (placed PDF and EPS; image previews keep their soft alpha).
pub fn is_proxy(element: &str) -> bool {
    matches!(element, "PDF" | "EPS")
}

pub fn graphic_is_proxy(g: &folio_model::Graphic) -> bool {
    !g.preview_from_source && is_proxy(&g.element)
}

/// A proxy's alpha (byte `alpha` of `stride`-byte pixels) as a 1-bit mask: α > 0 → opaque.
pub fn binary_alpha(px: &mut [u8], stride: usize, alpha: usize) {
    for p in px.chunks_exact_mut(stride) {
        p[alpha] = if p[alpha] > 0 { 255 } else { 0 };
    }
}

/// The colour model of an image's samples.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Samples {
    Gray,
    Rgb,
    /// CMYK, already converted to display RGB.
    Cmyk,
}

pub(super) fn decode(path: &str, mode: ImageMode, colors: &Colors) -> Option<Pixmap> {
    let bytes = std::fs::read(path).ok()?;
    let (w, h, mut data, samples) = decode_rgba(&bytes, colors, !mode.proxy)?;
    if mode.proxy {
        binary_alpha(&mut data, 4, 3);
    }
    // RGB samples are in the document RGB space (InDesign's policy for untagged RGB).
    match samples {
        Samples::Rgb => colors.rgba_to_display(&mut data, mode.blend),
        Samples::Gray if mode.blend == BlendSpace::Cmyk => {
            for px in data.chunks_exact_mut(4) {
                let rgb = colors.gray_through_cmyk(px[0]);
                px[..3].copy_from_slice(&rgb);
            }
        }
        Samples::Gray | Samples::Cmyk => {}
    }
    // tiny-skia wants premultiplied RGBA
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u16;
        if a < 255 {
            px[0] = ((px[0] as u16 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u16 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u16 * a + 127) / 255) as u8;
        }
    }
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
}

/// RGBA of an image file and the model of its samples. CMYK (TIFF, JPEG) is converted with
/// the document CMYK profile; everything else goes through the `image` crate, with our own
/// TIFF reader for layouts it refuses. `straight`: associated (premultiplied) TIFF alpha is
/// undone; otherwise the colour is returned as stored.
pub(super) fn decode_rgba(bytes: &[u8], colors: &Colors, straight: bool) -> Option<(u32, u32, Vec<u8>, Samples)> {
    let tiff = bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*");
    if bytes.starts_with(b"8BPS") {
        let (w, h, image, alpha) = psd::composite(bytes)?;
        let mut out = Vec::with_capacity(w as usize * h as usize * 4);
        let samples = match image {
            psd::Composite::Gray(px) => { px.iter().for_each(|&v| out.extend_from_slice(&[v, v, v, 255])); Samples::Gray }
            psd::Composite::Rgb(px) => { px.chunks_exact(3).for_each(|p| out.extend_from_slice(&[p[0], p[1], p[2], 255])); Samples::Rgb }
            psd::Composite::Cmyk(px) => {
                px.chunks_exact(4).for_each(|p| { let rgb = colors.cmyk8_to_srgb(p[0], p[1], p[2], p[3]); out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]); });
                Samples::Cmyk
            }
        };
        if let Some(alpha) = alpha {
            // merged transparency: straight alpha, as for any image with an alpha channel
            out.chunks_exact_mut(4).zip(alpha).for_each(|(px, a)| px[3] = a);
        }
        return Some((w, h, out, samples));
    }
    if tiff && tiff_photometric(bytes) == Some(5) {
        return decode_tiff(bytes, colors, straight).map(|(w, h, d)| (w, h, d, Samples::Cmyk));
    }
    if let Some((w, h, rgba, _)) = gif::decode_rgba(bytes) {
        return Some((w, h, rgba, Samples::Rgb));
    }
    if tiff && tiff_photometric(bytes) == Some(8) {
        return decode_tiff(bytes, colors, straight).map(|(w, h, d)| (w, h, d, Samples::Rgb));
    }
    if bytes.starts_with(&[0xFF, 0xD8])
        && let Some((w, h, d)) = decode_cmyk_jpeg(bytes, colors)
    {
        return Some((w, h, d, Samples::Cmyk));
    }
    match image::load_from_memory(bytes) {
        Ok(img) => {
            use image::ColorType;
            let gray = matches!(img.color(), ColorType::L8 | ColorType::La8 | ColorType::L16 | ColorType::La16);
            let img = img.to_rgba8();
            let (w, h) = img.dimensions();
            let mut data = img.into_raw();
            // a TIFF with associated (premultiplied) alpha: straight colour for compositing
            if straight && tiff && tiff_associated_alpha(bytes) {
                unpremultiply(&mut data, 4);
            }
            Some((w, h, data, if gray { Samples::Gray } else { Samples::Rgb }))
        }
        Err(_) => {
            let gray = tiff && matches!(tiff_photometric(bytes), Some(0 | 1));
            decode_tiff(bytes, colors, straight).map(|(w, h, d)| (w, h, d, if gray { Samples::Gray } else { Samples::Rgb }))
        }
    }
}

/// Whether a TIFF's extra sample is associated (premultiplied) alpha (ExtraSamples = 1).
pub(crate) fn tiff_associated_alpha(bytes: &[u8]) -> bool {
    let Ok(mut d) = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)) else { return false };
    d.get_tag_u32_vec(tiff::tags::Tag::ExtraSamples).ok().and_then(|v| v.first().copied()) == Some(1)
}

/// Premultiplied colour → straight, in pixels of `stride` bytes whose last byte is alpha.
pub(crate) fn unpremultiply(px: &mut [u8], stride: usize) {
    for p in px.chunks_exact_mut(stride) {
        let a = p[stride - 1] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..stride - 1] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

pub(crate) fn tiff_photometric(bytes: &[u8]) -> Option<u32> {
    let mut d = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).ok()?;
    d.get_tag_u32(tiff::tags::Tag::PhotometricInterpretation).ok()
}

/// A CMYK or YCCK JPEG (Adobe convention) as display RGBA;
/// `None` for other JPEGs.
fn decode_cmyk_jpeg(bytes: &[u8], colors: &Colors) -> Option<(u32, u32, Vec<u8>)> {
    let mut out = Vec::new();
    let (w, h) = cmyk_jpeg_inks(bytes, |[c, m, y, k]| {
        let rgb = colors.cmyk8_to_srgb(c, m, y, k);
        out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    })?;
    Some((w, h, out))
}

/// The ink (0 = none, 255 = 100 %) of every pixel of a CMYK or YCCK JPEG (Adobe convention),
/// in order, passed to `pixel`; the image size, or `None` for other JPEGs.
pub fn cmyk_jpeg_inks(bytes: &[u8], mut pixel: impl FnMut([u8; 4])) -> Option<(u32, u32)> {
    use zune_jpeg::JpegDecoder;
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let mut probe = JpegDecoder::new(ZCursor::new(bytes));
    probe.decode_headers().ok()?;
    let input = probe.input_colorspace()?;
    if !matches!(input, ColorSpace::CMYK | ColorSpace::YCCK) {
        return None;
    }
    let mut d = JpegDecoder::new_with_options(ZCursor::new(bytes), DecoderOptions::default().jpeg_set_out_colorspace(input));
    let px = d.decode().ok()?;
    let (w, h) = d.dimensions()?;
    if px.len() < w * h * 4 {
        return None;
    }
    for s in px.chunks_exact(4).take(w * h) {
        // Ink amounts: Adobe stores 255 − ink; YCCK encodes 255 − stored C, M, Y as YCbCr
        // (libjpeg's YCCK → CMYK), so its decoded R, G, B are the ink itself.
        let (c, m, y) = if input == ColorSpace::YCCK {
            let (yy, cb, cr) = (s[0] as f32, s[1] as f32 - 128.0, s[2] as f32 - 128.0);
            let v = |x: f32| x.round().clamp(0.0, 255.0) as u8;
            (v(yy + 1.402 * cr), v(yy - 0.344_136 * cb - 0.714_136 * cr), v(yy + 1.772 * cb))
        } else {
            (255 - s[0], 255 - s[1], 255 - s[2])
        };
        pixel([c, m, y, 255 - s[3]]);
    }
    Some((w as u32, h as u32))
}

/// Whether an image file has an alpha channel, reading only its header: PNG colour type
/// or transparency chunk, TIFF extra samples; JPEG never.
pub fn header_has_alpha(path: &str) -> Option<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 16];
    let n = f.read(&mut head).ok()?;
    let head = &head[..n];
    if head.starts_with(&[0xFF, 0xD8]) {
        return Some(false);
    }
    if head.starts_with(b"\x89PNG") {
        let mut ihdr = [0u8; 26];
        f.seek(SeekFrom::Start(0)).ok()?;
        f.read_exact(&mut ihdr).ok()?;
        if ihdr[25] == 4 || ihdr[25] == 6 {
            return Some(true);
        }
        // chunks up to the first IDAT: a tRNS chunk makes colours transparent
        let mut pos = 8u64;
        loop {
            let mut ch = [0u8; 8];
            f.seek(SeekFrom::Start(pos)).ok()?;
            f.read_exact(&mut ch).ok()?;
            let len = u32::from_be_bytes([ch[0], ch[1], ch[2], ch[3]]) as u64;
            match &ch[4..8] {
                b"tRNS" => return Some(true),
                b"IDAT" | b"IEND" => return Some(false),
                _ => pos += 12 + len,
            }
        }
    }
    let little = head.starts_with(b"II*\0");
    if !(little || head.starts_with(b"MM\0*")) {
        return None;
    }
    let u16_at = |b: &[u8]| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
    let ifd = if little {
        u32::from_le_bytes([head[4], head[5], head[6], head[7]])
    } else {
        u32::from_be_bytes([head[4], head[5], head[6], head[7]])
    } as u64;
    let mut count = [0u8; 2];
    f.seek(SeekFrom::Start(ifd)).ok()?;
    f.read_exact(&mut count).ok()?;
    let mut entries = vec![0u8; u16_at(&count) as usize * 12];
    f.read_exact(&mut entries).ok()?;
    // Tag 338 ExtraSamples: an alpha (or other extra) sample per pixel.
    Some(entries.chunks_exact(12).any(|e| u16_at(&e[0..2]) == 338))
}

/// An 8-bit TIFF CIELab sample (L 0–255 for 0–100, a and b signed bytes, D50) as sRGB: plain colorimetric,
/// clipped. InDesign's own gamut mapping differs by up to 12 steps in a clipped channel (placeimg-c1: place-tiff-lab).
pub fn lab8_to_srgb(l: u8, a: u8, b: u8) -> [u8; 3] {
    let (l, a, b) = (l as f64 / 255.0 * 100.0, a as i8 as f64, b as i8 as f64);
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let inv = |t: f64| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f64 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.9642 * inv(fx), inv(fy), 0.8249 * inv(fz));
    // XYZ D50 → linear sRGB (Bradford-adapted)
    let lin = [
        3.1338561 * x - 1.6168667 * y - 0.4906146 * z,
        -0.9787684 * x + 1.9161415 * y + 0.0334540 * z,
        0.0719453 * x - 0.2289914 * y + 1.4052427 * z,
    ];
    lin.map(|v| {
        let v = v.clamp(0.0, 1.0);
        let e = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (e * 255.0).round() as u8
    })
}

/// TIFF layouts the `image` crate refuses: CMYK with alpha and palette with alpha.
/// Returns RGBA (see [`tiff_samples`] for `straight`); CMYK goes through the document CMYK
/// profile.
fn decode_tiff(bytes: &[u8], colors: &Colors, straight: bool) -> Option<(u32, u32, Vec<u8>)> {
    let TiffSamples { w, h, photometric, spp, map, src, .. } = tiff_samples(bytes, straight)?;
    let n = (w * h) as usize;
    let mut out = Vec::with_capacity(n * 4);
    for px in src.chunks_exact(spp).take(n) {
        let (rgb, alpha) = match photometric {
            5 if spp >= 4 => (colors.cmyk8_to_srgb(px[0], px[1], px[2], px[3]), if spp >= 5 { px[4] } else { 255 }),
            8 if spp >= 3 => (lab8_to_srgb(px[0], px[1], px[2]), if spp >= 4 { px[3] } else { 255 }),
            3 => {
                let m = map.as_ref()?;
                let size = m.len() / 3;
                let i = px[0] as usize;
                ([(m[i] >> 8) as u8, (m[size + i] >> 8) as u8, (m[2 * size + i] >> 8) as u8], if spp >= 2 { px[1] } else { 255 })
            }
            0 | 1 => {
                let v = if photometric == 0 { 255 - px[0] } else { px[0] };
                ([v, v, v], if spp >= 2 { px[1] } else { 255 })
            }
            _ if spp >= 3 => ([px[0], px[1], px[2]], if spp >= 4 { px[3] } else { 255 }),
            _ => return None,
        };
        out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
    }
    Some((w, h, out))
}

/// 8-bit TIFF samples as stored (chunky, `spp` per pixel, at least `w × h` pixels).
pub(crate) struct TiffSamples {
    pub w: u32,
    pub h: u32,
    pub photometric: u32,
    pub spp: usize,
    pub bits: u32,
    pub map: Option<Vec<u16>>,
    pub src: Vec<u8>,
}

/// `straight`: associated (premultiplied) alpha is undone; otherwise samples as stored.
pub(crate) fn tiff_samples(bytes: &[u8], straight: bool) -> Option<TiffSamples> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;
    let mut d = Decoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (w, h) = d.dimensions().ok()?;
    let photometric = d.get_tag_u32(Tag::PhotometricInterpretation).ok()?;
    let spp = d.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(1) as usize;
    // one value per sample: the first says it (a single u32 read fails when there are several)
    let bits = d.get_tag_u32_vec(Tag::BitsPerSample).ok().and_then(|v| v.first().copied()).unwrap_or(8);
    let map = d.get_tag_u16_vec(Tag::ColorMap).ok();
    let associated = d.get_tag_u32_vec(Tag::ExtraSamples).ok().and_then(|v| v.first().copied()) == Some(1);
    let mut bits = bits;
    let src = match d.read_image() {
        Ok(DecodingResult::U8(src)) => src,
        // 16 bits per sample (CMYK, CIELab, ...): the high byte, as the print output keeps it; a signed CIELab a/b keeps its sign
        Ok(DecodingResult::U16(src)) => {
            bits = 8;
            src.iter().map(|v| (v >> 8) as u8).collect()
        }
        Ok(_) => return None,
        // e.g. palette + alpha, or 16-bit CIELab, which the tiff crate refuses: read the strips ourselves (a 16-bit sample as two bytes)
        Err(_) => read_strips(&mut d, w, h, spp * if bits == 16 { 2 } else { 1 })?,
    };
    // 16-bit samples read as bytes: the high byte of each sample
    let src = high_bytes_of_raw16(src, bits, (w * h) as usize * spp, bytes.starts_with(b"II"));
    if bits == 16 {
        bits = 8;
    }
    if spp == 0 || src.len() < (w * h) as usize * spp {
        return None;
    }
    let mut src = src;
    // associated alpha: straight colour (a palette index is not premultiplied)
    if straight && associated && spp >= 2 && bits == 8 && photometric != 3 {
        unpremultiply(&mut src[..(w * h) as usize * spp], spp);
    }
    Some(TiffSamples { w, h, photometric, spp, bits, map, src })
}

/// 16-bit samples the tiff crate returned as bytes (`samples` is how many samples the image has): the high byte of each, in the file's
/// byte order (`little`: Intel). Anything else is returned as it is.
pub fn high_bytes_of_raw16(src: Vec<u8>, bits: u32, samples: usize, little: bool) -> Vec<u8> {
    if bits != 16 || src.len() < samples * 2 {
        return src;
    }
    src.chunks_exact(2).take(samples).map(|p| if little { p[1] } else { p[0] }).collect()
}

/// Chunky 8-bit strips, uncompressed or LZW, with optional horizontal predictor.
fn read_strips(d: &mut tiff::decoder::Decoder<std::io::Cursor<&[u8]>>, w: u32, h: u32, spp: usize) -> Option<Vec<u8>> {
    use tiff::tags::Tag;
    let compression = d.get_tag_u32(Tag::Compression).unwrap_or(1);
    let predictor = d.get_tag_u32(Tag::Predictor).unwrap_or(1);
    let offsets = d.get_tag_u64_vec(Tag::StripOffsets).ok()?;
    let counts = d.get_tag_u64_vec(Tag::StripByteCounts).ok()?;
    let bytes = d.inner().get_ref();
    let row = w as usize * spp;
    let mut out = Vec::with_capacity(row * h as usize);
    for (&o, &c) in offsets.iter().zip(&counts) {
        let raw = bytes.get(o as usize..(o + c) as usize)?;
        match compression {
            1 => out.extend_from_slice(raw),
            5 => {
                let mut dec = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
                out.extend(dec.decode(raw).ok()?);
            }
            _ => return None,
        }
    }
    out.truncate(row * h as usize);
    if out.len() < row * h as usize {
        return None;
    }
    if predictor == 2 {
        for r in out.chunks_exact_mut(row) {
            for i in spp..row {
                r[i] = r[i].wrapping_add(r[i - spp]);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod lab_tests {
    use super::lab8_to_srgb;

    /// InDesign 2025 (placeimg-c1: place-tiff-lab) prints L 136, a 80, b 67 as (250, 7, 8) and L 200, a −70, b 60 as (12, 224, 66):
    /// the plain conversion is within 12 steps in a clipped channel.
    #[test]
    fn lab_samples_convert_near_indesigns_values() {
        assert_eq!(lab8_to_srgb(255, 0, 0), [255, 255, 255]);
        assert_eq!(lab8_to_srgb(0, 0, 0), [0, 0, 0]);
        let close = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 12);
        assert!(close(lab8_to_srgb(136, 80, 67), [250, 7, 8]));
        assert!(close(lab8_to_srgb(200, (-70i8) as u8, 60), [12, 224, 66]));
    }
}

/// A baseline uncompressed little-endian TIFF of one pixel row (tests): `samples` of `bits` per sample, chunky.
#[cfg(test)]
pub(crate) fn test_tiff(w: u32, photometric: u16, spp: u16, bits: u16, samples: &[u8]) -> Vec<u8> {
    let entries: Vec<(u16, u16, u32, u32)> = vec![
        (256, 4, 1, w), (257, 4, 1, 1), (258, 3, spp as u32, 0), (259, 3, 1, 1), (262, 3, 1, photometric as u32),
        (273, 4, 1, 0), (277, 3, 1, spp as u32), (278, 4, 1, 1), (279, 4, 1, samples.len() as u32), (284, 3, 1, 1),
    ];
    let ifd_at = 8u32;
    let ifd_len = 2 + entries.len() as u32 * 12 + 4;
    let bits_at = ifd_at + ifd_len;
    let data_at = bits_at + spp as u32 * 2;
    let mut out = b"II*\0".to_vec();
    out.extend(ifd_at.to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in entries {
        let value = match tag { 258 if spp > 2 => bits_at, 258 => bits as u32, 273 => data_at, _ => value };
        out.extend(tag.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(value.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes());
    for _ in 0..spp {
        out.extend(bits.to_le_bytes());
    }
    out.extend_from_slice(samples);
    out
}

#[cfg(test)]
mod deep_tests {
    use super::*;

    /// 16-bit CMYK and CIELab TIFF (build-a): drawn from the high byte of each sample, the same colour as the 8-bit picture.
    #[test]
    fn sixteen_bit_cmyk_and_lab_tiffs_draw_like_their_eight_bit_twins() {
        let colors = Colors::unmanaged();
        let wide = |v: &[u8]| v.iter().flat_map(|b| ((*b as u16) << 8 | 0x7F).to_le_bytes()).collect::<Vec<u8>>();
        let cmyk = [0u8, 255, 255, 0, 128, 0, 0, 64];
        let (w, h, deep, samples) = decode_rgba(&test_tiff(2, 5, 4, 16, &wide(&cmyk)), &colors, true).unwrap();
        let (_, _, flat, _) = decode_rgba(&test_tiff(2, 5, 4, 8, &cmyk), &colors, true).unwrap();
        assert_eq!((w, h, matches!(samples, Samples::Cmyk)), (2, 1, true));
        assert_eq!(deep, flat);
        // L 136, a 80, b 67 and L 200, a -70, b 60 (a and b signed)
        let lab = [136u8, 80, 67, 200, (-70i8) as u8, 60];
        let s = tiff_samples(&test_tiff(2, 8, 3, 16, &wide(&lab)), true).unwrap();
        assert_eq!((s.photometric, s.spp, &s.src[..6]), (8, 3, &lab[..]));
        assert_eq!(high_bytes_of_raw16(vec![1, 2, 3, 4], 16, 2, false), [1, 3]);
        let (_, _, deep, _) = decode_rgba(&test_tiff(2, 8, 3, 16, &wide(&lab)), &colors, true).unwrap();
        assert_eq!(&deep[..3], &lab8_to_srgb(136, 80, 67));
        assert_eq!(&deep[4..7], &lab8_to_srgb(200, (-70i8) as u8, 60));
    }
}

#[cfg(test)]
mod proxy_tests {
    use super::*;

    #[test]
    fn a_proxy_draws_its_stored_colour_where_alpha_is_not_zero() {
        let dir = std::env::temp_dir().join(format!("folio-render-proxy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("proxy.png");
        image::RgbaImage::from_raw(3, 1, vec![0, 0, 0, 255, 0, 0, 0, 108, 255, 255, 255, 0]).unwrap().save(&path).unwrap();
        let path = path.to_string_lossy().into_owned();
        let colors = Colors::unmanaged();
        let proxy = decode(&path, ImageMode { blend: BlendSpace::Rgb, proxy: true }, &colors).unwrap();
        assert_eq!(&proxy.data()[4..], &[0, 0, 0, 255, 0, 0, 0, 0]);
        // an image preview keeps its soft alpha
        let image = decode(&path, ImageMode { blend: BlendSpace::Rgb, proxy: false }, &colors).unwrap();
        assert_eq!(image.data()[7], 108);
        assert!(is_proxy("PDF") && is_proxy("EPS") && !is_proxy("Image"));
    }
}
