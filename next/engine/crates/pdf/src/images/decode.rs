//! Decodes the supported raster image formats for PDF and flattening.

use super::compression::{captured_stub, flate, raster};
use super::{Captured, Conv, Ctx, Encoded, Filter, ImgSpace};
use crate::graphics::ImageSource;
use std::cell::RefCell;
use std::sync::Arc;

/// A JPEG as it is, or, when it is RGB the output converts, decoded, converted and
/// re-encoded.
pub(super) fn jpeg_converted(bytes: &[u8], ctx: &Ctx) -> Result<Encoded, String> {
    let mut e = jpeg(bytes)?;
    ctx.color.apply(&mut e.space);
    let converting = match (&e.space, ctx.convert) {
        (ImgSpace::Rgb(icc), Some(conv)) => conv.wants(icc),
        (ImgSpace::Cmyk, Some(conv)) => ctx.color.cmyk.as_ref().is_some_and(|source| conv.wants_cmyk(source)),
        _ => false,
    };
    if converting || ctx.size(e.width, e.height) != (e.width, e.height) || ctx.image_compression != "legacy" {
        let (w, h) = (e.width, e.height);
        if matches!(e.space, ImgSpace::Cmyk) {
            let mut samples = Vec::new();
            folio_render::cmyk_jpeg_inks(bytes, |c| samples.extend_from_slice(&c)).ok_or("cannot decode CMYK JPEG")?;
            return Ok(raster(w, h, &samples, 4, false, ImgSpace::Cmyk, ctx, "JPEG CMYK"));
        }
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).map_err(|x| x.to_string())?;
        if matches!(e.space, ImgSpace::Gray) {
            Ok(raster(w, h, img.to_luma8().as_raw(), 1, false, e.space, ctx, "JPEG gray"))
        } else {
            Ok(raster(w, h, img.to_rgb8().as_raw(), 3, false, e.space, ctx, "JPEG RGB"))
        }
    } else { Ok(e) }
}

pub(super) fn read(p: &std::path::Path) -> Result<Vec<u8>, String> {
    std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))
}

// -- JPEG ------------------------------------------------------------------------------

/// A JPEG written as it is: size, components and Adobe / ICC markers from its headers.
fn jpeg(bytes: &[u8]) -> Result<Encoded, String> {
    let mut i = 2usize;
    let (mut w, mut h, mut comps, mut bits) = (0u32, 0u32, 0u8, 8u8);
    let mut adobe: Option<u8> = None;
    let mut icc: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut arithmetic = false;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            return Err("JPEG marker expected".into());
        }
        let m = bytes[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        let seg = bytes.get(i + 4..i + 2 + len).ok_or("truncated JPEG")?;
        match m {
            0xC0..=0xCF if !matches!(m, 0xC4 | 0xC8 | 0xCC) => {
                if m >= 0xC9 {
                    arithmetic = true;
                }
                bits = seg[0];
                h = u16::from_be_bytes([seg[1], seg[2]]) as u32;
                w = u16::from_be_bytes([seg[3], seg[4]]) as u32;
                comps = seg[5];
            }
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe = Some(seg[11]),
            0xE2 if seg.starts_with(b"ICC_PROFILE\0") && seg.len() > 14 => icc.push((seg[12], seg[14..].to_vec())),
            0xDA => break,
            _ => {}
        }
        i += 2 + len;
    }
    if w == 0 || h == 0 {
        return Err("JPEG without frame header".into());
    }
    if bits != 8 || arithmetic {
        return Err(format!("JPEG with {bits}-bit samples or arithmetic coding cannot pass through"));
    }
    icc.sort_by_key(|(seq, _)| *seq);
    let icc: Vec<u8> = icc.into_iter().flat_map(|(_, d)| d).collect();
    let (space, decode, note) = match comps {
        1 => (ImgSpace::Gray, None, "JPEG gray"),
        3 => (ImgSpace::Rgb((!icc.is_empty()).then(|| Arc::new(icc))), None, "JPEG RGB"),
        // Adobe CMYK JPEGs store inverted ink.
        4 if adobe.is_some() => (ImgSpace::Cmyk, Some(vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]), "JPEG CMYK (Adobe, inverted)"),
        4 => (ImgSpace::Cmyk, None, "JPEG CMYK"),
        n => return Err(format!("JPEG with {n} components")),
    };
    Ok(Encoded { width: w, height: h, bpc: 8, space, data: bytes.to_vec(), filter: Filter::Dct, decode, smask: None, mask: None, note: note.into() })
}

/// An image decoded to ink for the flattener: straight C, M, Y, K (0 = no ink) and alpha
/// per pixel, RGB converted as the PDF/X-1a vector output converts it, gray as black ink,
/// 1-bit as a black stencil.
pub fn decode_ink(source: &ImageSource, conv: &Conv, color: Option<&folio_model::image_color::ImageColor>) -> Result<(u32, u32, Vec<[u8; 5]>), String> {
    let cell = RefCell::new(None);
    let proxy = matches!(source, ImageSource::Proxy(_));
    let ctx = Ctx { color: crate::image_color::for_source(source, color, conv.doc_rgb.clone()), image_compression: "legacy", jpeg_quality: 95, level: 0, target: None, convert: None, no_smask: false, proxy, capture: Some(&cell) };
    let owned;
    let bytes: &[u8] = match source {
        ImageSource::Jpeg(p) | ImageSource::Decode(p, _) | ImageSource::Preview(p) | ImageSource::Proxy(p) => {
            owned = read(p)?;
            &owned
        }
        ImageSource::Embedded(d) => d,
    };
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut out = Vec::new();
        if let Some((w, h)) = folio_render::cmyk_jpeg_inks(bytes, |c| out.push([c[0], c[1], c[2], c[3], 255])) {
            if let Some(source) = ctx.color.cmyk.as_ref().filter(|source| conv.wants_cmyk(source)) {
                conv.cmyk_ink5(source, &mut out, ctx.color.intent);
            }
            return Ok((w, h, out));
        }
        let mut e = jpeg(bytes)?;
        ctx.color.apply(&mut e.space);
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).map_err(|x| x.to_string())?;
        let (w, h) = (img.width(), img.height());
        return Ok(match e.space {
            ImgSpace::Gray => (w, h, img.to_luma8().as_raw().iter().map(|g| [0, 0, 0, 255 - g, 255]).collect()),
            ImgSpace::Rgb(icc) => {
                let c = conv.rgb_pixels_intent(&icc, img.to_rgb8().as_raw(), 3, ctx.color.intent);
                (w, h, c.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3], 255]).collect())
            }
            _ => return Err("JPEG colour space".into()),
        });
    }
    if bytes.starts_with(b"\x89PNG") {
        png(bytes, &ctx)?;
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        tiff(bytes, &ctx)?;
    } else if bytes.starts_with(b"8BPS") {
        psd(bytes, &ctx)?;
    } else if bytes.starts_with(b"GIF8") {
        gif(bytes, &ctx)?;
    } else if bytes.starts_with(b"BM") {
        bmp(bytes, &ctx)?;
    } else {
        return Err("image format not recognised".into());
    }
    let intent = ctx.color.intent;
    let cmyk_source = ctx.color.cmyk.clone().filter(|source| conv.wants_cmyk(source));
    let c = cell.into_inner().ok_or("nothing decoded")?;
    let px = c.w as usize * c.h as usize;
    if let Some((rows, one_paints)) = &c.stencil {
        let row = (c.w as usize).div_ceil(8);
        let mut out = Vec::with_capacity(px);
        for y in 0..c.h as usize {
            for x in 0..c.w as usize {
                let bit = (rows[y * row + x / 8] >> (7 - x % 8)) & 1 == 1;
                out.push([0, 0, 0, 255, if bit == *one_paints { 255 } else { 0 }]);
            }
        }
        return Ok((c.w, c.h, out));
    }
    let stride = c.n + usize::from(c.alpha);
    // a proxy's alpha as a 1-bit mask
    let al = |v: u8| if proxy && v > 0 { 255 } else if proxy { 0 } else { v };
    let a = |p: &[u8]| if c.alpha { al(p[c.n]) } else { 255 };
    let out: Vec<[u8; 5]> = match &c.space {
        ImgSpace::Gray => c.samples.chunks_exact(stride).take(px).map(|p| [0, 0, 0, 255 - p[0], a(p)]).collect(),
        ImgSpace::Cmyk => {
            let mut ink: Vec<[u8; 5]> = c.samples.chunks_exact(stride).take(px).map(|p| [p[0], p[1], p[2], p[3], a(p)]).collect();
            if let Some(source) = &cmyk_source {
                conv.cmyk_ink5(source, &mut ink, intent);
            }
            ink
        }
        ImgSpace::Rgb(icc) => {
            let cm = conv.rgb_pixels_intent(icc, &c.samples[..px.min(c.samples.len() / stride) * stride], stride, intent);
            cm.chunks_exact(stride + 1).map(|p| [p[0], p[1], p[2], p[3], if c.alpha { al(p[4]) } else { 255 }]).collect()
        }
        ImgSpace::Mask => return Err("unexpected mask".into()),
    };
    Ok((c.w, c.h, out))
}

// -- PNG -------------------------------------------------------------------------------

pub(super) fn png(bytes: &[u8], ctx: &Ctx) -> Result<Encoded, String> {
    use image::{ColorType, ImageDecoder};
    let mut dec = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let icc = dec.icc_profile().ok().flatten().filter(|p| p.len() > 128);
    let ct = dec.color_type();
    let img = image::DynamicImage::from_decoder(dec).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    Ok(match ct {
        ColorType::L8 | ColorType::L16 => raster(w, h, img.to_luma8().as_raw(), 1, false, ImgSpace::Gray, ctx, "PNG gray"),
        ColorType::La8 | ColorType::La16 => raster(w, h, img.to_luma_alpha8().as_raw(), 1, true, ImgSpace::Gray, ctx, "PNG gray+alpha"),
        ColorType::Rgb8 | ColorType::Rgb16 | ColorType::Rgb32F => raster(w, h, img.to_rgb8().as_raw(), 3, false, ImgSpace::Rgb(icc.map(Arc::new)), ctx, "PNG RGB"),
        _ => raster(w, h, img.to_rgba8().as_raw(), 3, true, ImgSpace::Rgb(icc.map(Arc::new)), ctx, "PNG RGBA"),
    })
}

// -- GIF -------------------------------------------------------------------------------

/// The first image of a GIF in the document RGB; the pixels of its transparent colour are a soft mask (screens-import).
pub(super) fn gif(bytes: &[u8], ctx: &Ctx) -> Result<Encoded, String> {
    let (w, h, rgba, transparent) = folio_render::gif::decode_rgba(bytes).ok_or("GIF could not be decoded")?;
    if transparent {
        return Ok(raster(w, h, &rgba, 3, true, ImgSpace::Rgb(None), ctx, "GIF transparent"));
    }
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    Ok(raster(w, h, &rgb, 3, false, ImgSpace::Rgb(None), ctx, "GIF"))
}

// -- BMP -------------------------------------------------------------------------------

/// A BMP (1 to 32 bits, palette or direct colour) as RGB in the document RGB; the alpha of a 32-bit BMP that has one is kept.
pub(super) fn bmp(bytes: &[u8], ctx: &Ctx) -> Result<Encoded, String> {
    use image::ColorType;
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Bmp).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    Ok(match img.color() {
        ColorType::Rgba8 | ColorType::Rgba16 | ColorType::La8 | ColorType::La16 => raster(w, h, img.to_rgba8().as_raw(), 3, true, ImgSpace::Rgb(None), ctx, "BMP RGBA"),
        _ => raster(w, h, img.to_rgb8().as_raw(), 3, false, ImgSpace::Rgb(None), ctx, "BMP"),
    })
}

// -- TIFF ------------------------------------------------------------------------------

pub(super) fn tiff(bytes: &[u8], ctx: &Ctx) -> Result<Encoded, String> {
    let level = ctx.level;
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;
    let mut d = Decoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let (w, h) = d.dimensions().map_err(|e| e.to_string())?;
    let photometric = d.get_tag_u32(Tag::PhotometricInterpretation).map_err(|e| e.to_string())?;
    let spp = d.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(1) as usize;
    // TIFF 6.0: BitsPerSample defaults to 1, which is what a bilevel (photometric 0/1, one sample) file without the tag is.
    let bits = d.get_tag_u32_vec(Tag::BitsPerSample).ok().and_then(|v| v.first().copied()).unwrap_or(if matches!(photometric, 0 | 1) && spp == 1 { 1 } else { 8 });
    let extra = d.get_tag_u32_vec(Tag::ExtraSamples).ok().and_then(|v| v.first().copied());
    let icc = d.get_tag_u8_vec(Tag::Unknown(34675)).ok().filter(|p| p.len() > 128);
    let map = d.get_tag_u16_vec(Tag::ColorMap).ok();
    if bits == 1 && spp == 1 {
        let data = match d.read_image().map_err(|e| e.to_string())? {
            DecodingResult::U8(v) => v,
            _ => return Err("1-bit TIFF decoded unexpectedly".into()),
        };
        let row = (w as usize).div_ceil(8);
        let need = row * h as usize;
        if data.len() < need {
            return Err("1-bit TIFF data short".into());
        }
        // Stencil: sample 0 paints. The tiff decoder already turns WhiteIsZero samples into BlackIsZero ones (it inverts them
        // while reading), so black is 0 for both photometrics; inverting again here printed WhiteIsZero bitmaps as negatives.
        if let Some(c) = ctx.capture {
            *c.borrow_mut() = Some(Captured { w, h, n: 1, alpha: false, space: ImgSpace::Mask, samples: Vec::new(), stencil: Some((data[..need].to_vec(), false)) });
            return Ok(captured_stub());
        }
        let decode = None;
        return Ok(Encoded {
            width: w,
            height: h,
            bpc: 1,
            space: ImgSpace::Mask,
            data: flate(&data[..need], level),
            filter: Filter::Flate,
            decode,
            smask: None,
            mask: None,
            note: "TIFF bitmap (stencil)".into(),
        });
    }
    // The tiff decoder turns single-sample WhiteIsZero gray into BlackIsZero while reading; samples from our own strip reader are as stored.
    let mut as_stored = false;
    let samples: Vec<u8> = match d.read_image() {
        Ok(DecodingResult::U8(v)) => v,
        Ok(DecodingResult::U16(v)) => v.iter().map(|x| (x >> 8) as u8).collect(),
        Ok(_) => return Err("TIFF sample format not supported".into()),
        // e.g. palette + alpha, which the tiff crate refuses: read the strips ourselves
        Err(e) => {
            as_stored = true;
            // a 16-bit sample (CIELab, which the tiff crate refuses) as two bytes, its high byte kept below
            read_strips(&mut d, w, h, spp * if bits == 16 { 2 } else { 1 }).ok_or_else(|| format!("TIFF: {e}"))?
        }
    };
    let px = (w * h) as usize;
    // a 16-bit CIELab image comes from the tiff crate as its raw bytes: the high byte of each sample (build-a)
    let samples = folio_render::high_bytes_of_raw16(samples, bits, px * spp, bytes.starts_with(b"II"));
    if samples.len() < px * spp {
        return Err("TIFF data short".into());
    }
    let alpha_ok = matches!(extra, Some(1 | 2));
    match photometric {
        0 | 1 => {
            let mut s = samples;
            // gray with alpha is not inverted by the decoder (its color type is GrayA); plain gray was, a second inversion printed a negative
            if photometric == 0 && (as_stored || spp >= 2) {
                for (k, v) in s.iter_mut().enumerate() {
                    if k % spp == 0 {
                        *v = 255 - *v;
                    }
                }
            }
            let alpha = spp >= 2 && alpha_ok;
            let packed = pick(&s, spp, if alpha { &[0, 1] } else { &[0] });
            let packed = if alpha && extra == Some(1) && !ctx.proxy { unpremultiply(packed, 1) } else { packed };
            Ok(raster(w, h, &packed, 1, alpha, ImgSpace::Gray, ctx, "TIFF gray"))
        }
        2 => {
            let alpha = spp >= 4 && alpha_ok;
            let packed = pick(&samples, spp, if alpha { &[0, 1, 2, 3] } else { &[0, 1, 2] });
            let packed = if alpha && extra == Some(1) && !ctx.proxy { unpremultiply(packed, 3) } else { packed };
            Ok(raster(w, h, &packed, 3, alpha, ImgSpace::Rgb(icc.map(Arc::new)), ctx, "TIFF RGB"))
        }
        5 if spp >= 4 => {
            let alpha = spp >= 5 && alpha_ok;
            let packed = pick(&samples, spp, if alpha { &[0, 1, 2, 3, 4] } else { &[0, 1, 2, 3] });
            let packed = if alpha && extra == Some(1) && !ctx.proxy { unpremultiply(packed, 4) } else { packed };
            Ok(raster(w, h, &packed, 4, alpha, ImgSpace::Cmyk, ctx, "TIFF CMYK"))
        }
        8 if spp >= 3 => {
            // CIELab: converted to sRGB, the document RGB (placeimg-c1: place-tiff-lab)
            let mut rgb = Vec::with_capacity(px * 3);
            for p in samples.chunks_exact(spp).take(px) {
                rgb.extend_from_slice(&folio_render::lab8_to_srgb(p[0], p[1], p[2]));
            }
            Ok(raster(w, h, &rgb, 3, false, ImgSpace::Rgb(None), ctx, "TIFF Lab"))
        }
        3 => {
            let m = map.ok_or("palette TIFF without ColorMap")?;
            let size = m.len() / 3;
            let alpha = spp >= 2 && alpha_ok;
            let mut out = Vec::with_capacity(px * (3 + usize::from(alpha)));
            for p in samples.chunks_exact(spp).take(px) {
                let i = p[0] as usize;
                out.extend_from_slice(&[(m[i] >> 8) as u8, (m[size + i] >> 8) as u8, (m[2 * size + i] >> 8) as u8]);
                if alpha {
                    out.push(p[1]);
                }
            }
            Ok(raster(w, h, &out, 3, alpha, ImgSpace::Rgb(None), ctx, "TIFF palette"))
        }
        p => Err(format!("TIFF photometric {p} not supported")),
    }
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

/// The listed channels of each pixel, interleaved.
fn pick(s: &[u8], spp: usize, channels: &[usize]) -> Vec<u8> {
    if channels.len() == spp {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len() / spp * channels.len());
    for p in s.chunks_exact(spp) {
        for &c in channels {
            out.push(p[c]);
        }
    }
    out
}

/// Associated (premultiplied) alpha → straight colour, for a soft mask.
fn unpremultiply(mut v: Vec<u8>, n: usize) -> Vec<u8> {
    for p in v.chunks_exact_mut(n + 1) {
        let a = p[n] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..n] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    v
}

// -- PSD -------------------------------------------------------------------------------

fn be16(b: &[u8], i: usize) -> Result<u16, String> {
    b.get(i..i + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or_else(|| "truncated PSD".into())
}
fn be32(b: &[u8], i: usize) -> Result<u32, String> {
    b.get(i..i + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "truncated PSD".into())
}

/// The composite image of a Photoshop file (raw or PackBits), its ICC profile, and the
/// merged transparency when the file has one.
pub(super) fn psd(b: &[u8], ctx: &Ctx) -> Result<Encoded, String> {
    if !b.starts_with(b"8BPS") || be16(b, 4)? != 1 {
        return Err("not a PSD (or PSB not supported)".into());
    }
    let channels = be16(b, 12)? as usize;
    let h = be32(b, 14)?;
    let w = be32(b, 18)?;
    let depth = be16(b, 22)?;
    let mode = be16(b, 24)?;
    let mut i = 26usize;
    let cm_len = be32(b, i)? as usize;
    i += 4 + cm_len;
    let res_len = be32(b, i)? as usize;
    let res = b.get(i + 4..i + 4 + res_len).ok_or("truncated PSD resources")?;
    i += 4 + res_len;
    let mut icc = None;
    let mut r = 0usize;
    while r + 12 <= res.len() && &res[r..r + 4] == b"8BIM" {
        let id = u16::from_be_bytes([res[r + 4], res[r + 5]]);
        let nlen = res[r + 6] as usize;
        let mut p = r + 6 + 1 + nlen;
        if p % 2 == 1 {
            p += 1;
        }
        let size = be32(res, p)? as usize;
        let data = res.get(p + 4..p + 4 + size).ok_or("truncated PSD resource")?;
        if id == 1039 {
            icc = Some(data.to_vec());
        }
        r = p + 4 + size + (size % 2);
    }
    let lm_len = be32(b, i)? as usize;
    // Layer count < 0: the first alpha channel is the merged transparency.
    let mut merged_alpha = false;
    if lm_len >= 8 {
        let layer_info_len = be32(b, i + 4)? as usize;
        if layer_info_len >= 2 {
            let count = be16(b, i + 8)? as i16;
            merged_alpha = count < 0;
        }
    }
    i += 4 + lm_len;
    let compression = be16(b, i)?;
    i += 2;
    let (cw, chh) = (w as usize, h as usize);
    let bpc = (depth as usize).div_ceil(8).max(1);
    if depth != 8 && depth != 16 {
        return Err(format!("PSD depth {depth} not supported"));
    }
    let row_bytes = cw * bpc;
    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(channels);
    match compression {
        0 => {
            for c in 0..channels {
                let start = i + c * row_bytes * chh;
                planes.push(b.get(start..start + row_bytes * chh).ok_or("truncated PSD image data")?.to_vec());
            }
        }
        1 => {
            let counts_at = i;
            let mut pos = i + channels * chh * 2;
            for c in 0..channels {
                let mut plane = Vec::with_capacity(row_bytes * chh);
                for y in 0..chh {
                    let n = be16(b, counts_at + (c * chh + y) * 2)? as usize;
                    let src = b.get(pos..pos + n).ok_or("truncated PSD RLE")?;
                    packbits(src, row_bytes, &mut plane);
                    pos += n;
                }
                planes.push(plane);
            }
        }
        c => return Err(format!("PSD compression {c} not supported")),
    }
    let to8 = |p: &Vec<u8>| -> Vec<u8> { if bpc == 2 { p.chunks_exact(2).map(|s| s[0]).collect() } else { p.clone() } };
    let (n, space, note) = match mode {
        1 => (1, ImgSpace::Gray, "PSD gray"),
        3 => (3, ImgSpace::Rgb(icc.filter(|p| p.len() > 128).map(Arc::new)), "PSD RGB"),
        4 => (4, ImgSpace::Cmyk, "PSD CMYK"),
        // Lab (build-a2): converted to sRGB as a CIELab TIFF is (placeimg-c1: place-tiff-lab), the document RGB
        9 => (3, ImgSpace::Rgb(None), "PSD Lab"),
        m => return Err(format!("PSD colour mode {m} not supported")),
    };
    if channels < n {
        return Err("PSD with too few channels".into());
    }
    let alpha = merged_alpha && channels > n;
    let used: Vec<Vec<u8>> = planes.iter().take(n + usize::from(alpha)).map(to8).collect();
    let px = cw * chh;
    let stride = used.len();
    let mut inter = vec![0u8; px * stride];
    for (c, p) in used.iter().enumerate() {
        for (k, v) in p.iter().take(px).enumerate() {
            inter[k * stride + c] = *v;
        }
    }
    if mode == 4 {
        // Photoshop stores CMYK as 255 − ink.
        for p in inter.chunks_exact_mut(stride) {
            for v in &mut p[..4] {
                *v = 255 - *v;
            }
        }
    }
    if mode == 9 {
        // Photoshop's Lab: a and b offset by 128 (a CIELab TIFF's are signed)
        for p in inter.chunks_exact_mut(stride) {
            let rgb = folio_render::lab8_to_srgb(p[0], p[1] ^ 0x80, p[2] ^ 0x80);
            p[..3].copy_from_slice(&rgb);
        }
    }
    Ok(raster(w, h, &inter, n, alpha, space, ctx, note))
}

fn packbits(src: &[u8], want: usize, out: &mut Vec<u8>) {
    let start = out.len();
    let mut i = 0;
    while i < src.len() && out.len() - start < want {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            out.extend_from_slice(&src[i..(i + k).min(src.len())]);
            i += k;
        } else if n != -128 {
            let k = (-(n as i16)) as usize + 1;
            if let Some(&v) = src.get(i) {
                out.extend(std::iter::repeat_n(v, k));
            }
            i += 1;
        }
    }
    out.resize(start + want, 0);
}
