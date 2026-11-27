//! Resamples image data and writes compressed PDF image streams and masks.

use super::{Captured, Conv, Ctx, Downsample, Encoded, Filter, ImgSpace, binary_mask, explicit_mask};
use crate::graphics::ImageJob;
use std::io::Write;
use std::sync::Arc;

pub fn flate(data: &[u8], level: u32) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::with_capacity(data.len() / 2 + 64), flate2::Compression::new(level.min(9)));
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

pub(super) fn target(job: &ImageJob, d: Option<Downsample>) -> Option<((f64, f64), Downsample)> {
    let d = d?;
    (job.placed.0 > 0.0 && job.placed.1 > 0.0).then_some((job.placed, d))
}

impl Ctx<'_> {
    /// Output size for a `w`x`h` image.
    pub(super) fn size(&self, w: u32, h: u32) -> (u32, u32) {
        let Some(((pw, ph), d)) = self.target else { return (w, h) };
        let (ppi_x, ppi_y) = (w as f64 / (pw / 72.0), h as f64 / (ph / 72.0));
        // InDesign 2025 (artifacts/pdfxcheck-c2-20261023, `downsampling_treats_each_axis_by_itself`): resample only when the higher
        // resolution is above the threshold and the lower one above the target; then each axis goes to the target by itself.
        // (whole numbers, as InDesign shows and decides them: a 300.0001 ppi axis is 300)
        if ppi_x.max(ppi_y).round() <= d.above || ppi_x.min(ppi_y).round() <= d.to {
            return (w, h);
        }
        let (fx, fy) = ((d.to / ppi_x).min(1.0), (d.to / ppi_y).min(1.0));
        (((w as f64 * fx).round() as u32).max(1), ((h as f64 * fy).round() as u32).max(1))
    }
}

/// Area-average resampling of interleaved 8-bit samples.
fn box_resample(src: &[u8], w: u32, h: u32, stride: usize, size: (u32, u32), reuse_rows: bool) -> Vec<u8> {
    let (nw, nh) = size;
    let taps = |n: u32, m: u32| -> Vec<Vec<(usize, f32)>> {
        let scale = n as f64 / m as f64;
        (0..m)
            .map(|i| {
                let (a, b) = (i as f64 * scale, ((i + 1) as f64 * scale).min(n as f64));
                let mut v = Vec::new();
                let mut j = a.floor() as usize;
                while (j as f64) < b && j < n as usize {
                    let o = (b.min(j as f64 + 1.0) - a.max(j as f64)).max(0.0);
                    if o > 0.0 {
                        v.push((j, (o / (b - a)) as f32));
                    }
                    j += 1;
                }
                v
            })
            .collect()
    };
    let (cx, cy) = (taps(w, nw), taps(h, nh));
    let row_out = nw as usize * stride;
    let mut tmp = vec![0f32; row_out * h as usize];
    let row_bytes = w as usize * stride;
    for y in 0..h as usize {
        let row = &src[y * row_bytes..(y + 1) * row_bytes];
        if reuse_rows && w > nw && y > 0 {
            let previous = &src[(y - 1) * row_bytes..y * row_bytes];
            if previous.last() == row.last() && previous == row {
                tmp.copy_within((y - 1) * row_out..y * row_out, y * row_out);
                continue;
            }
        }
        let out = &mut tmp[y * row_out..(y + 1) * row_out];
        for (x, t) in cx.iter().enumerate() {
            for &(j, wt) in t {
                for c in 0..stride {
                    out[x * stride + c] += wt * row[j * stride + c] as f32;
                }
            }
        }
    }
    let mut acc = vec![0f32; row_out];
    let mut dst = vec![0u8; row_out * nh as usize];
    for (y, t) in cy.iter().enumerate() {
        acc.iter_mut().for_each(|v| *v = 0.0);
        for &(j, wt) in t {
            let row = &tmp[j * row_out..(j + 1) * row_out];
            for (o, v) in acc.iter_mut().zip(row) {
                *o += wt * v;
            }
        }
        for (d, v) in dst[y * row_out..(y + 1) * row_out].iter_mut().zip(&acc) {
            *d = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    dst
}

pub(super) fn captured_stub() -> Encoded {
    Encoded { width: 0, height: 0, bpc: 8, space: ImgSpace::Gray, data: Vec::new(), filter: Filter::Flate, decode: None, smask: None, mask: None, note: String::new() }
}

impl Ctx<'_> {
    /// What alpha `a` becomes: nothing when all opaque; for a proxy a 1-bit mask at its own
    /// resolution (α > 0 painted); else a soft mask, or in PDF 1.3 the explicit mask of a
    /// cut-out ([`Ctx::mask_of`]).
    // Keep the alpha result as the existing optional soft mask and explicit mask pair.
    #[allow(clippy::type_complexity)]
    fn alpha_out(&self, a: &[u8], w: u32, h: u32) -> (Option<Vec<u8>>, Option<(u32, u32, Vec<u8>)>) {
        if a.iter().all(|v| *v == 255) {
            (None, None)
        } else if self.proxy {
            (None, binary_mask(a, w, h, self.level))
        } else if self.no_smask {
            (None, self.mask_of(a, w, h))
        } else {
            (Some(flate(a, self.level)), None)
        }
    }

    /// The explicit mask of alpha `a` (PDF 1.3) at about 600 ppi of the largest placement,
    /// at most 8 × the image and 64 million mask pixels; `None` when all is opaque.
    fn mask_of(&self, a: &[u8], w: u32, h: u32) -> Option<(u32, u32, Vec<u8>)> {
        if a.iter().all(|v| *v == 255) {
            return None;
        }
        let ppi = self.target.map(|((pw, ph), _)| (w as f64 / (pw / 72.0)).max(h as f64 / (ph / 72.0))).unwrap_or(300.0).max(1.0);
        let mut f = (600.0 / ppi).round().clamp(1.0, 8.0) as u32;
        while f > 1 && (w * f) as u64 * (h * f) as u64 > 64_000_000 {
            f -= 1;
        }
        Some(explicit_mask(a, w, h, f, self.level))
    }
}

/// Interleaved 8-bit samples (`n` colour channels, optional trailing alpha) → Encoded.
// Preserve the encoder boundary with independent dimensions, samples and image options.
#[allow(clippy::too_many_arguments)]
pub(super) fn raster(w: u32, h: u32, samples: &[u8], n: usize, alpha: bool, mut space: ImgSpace, ctx: &Ctx, note: &str) -> Encoded {
    ctx.color.apply(&mut space);
    if let Some(c) = ctx.capture {
        *c.borrow_mut() = Some(Captured { w, h, n, alpha, space, samples: samples.to_vec(), stencil: None });
        return captured_stub();
    }
    if let (Some(conv), ImgSpace::Rgb(icc)) = (ctx.convert, &space)
        && n == 3
        && conv.wants(icc)
    {
        return raster_cmyk(w, h, samples, alpha, icc, conv, ctx, note);
    }
    // a CMYK image with an assigned profile: its ink goes to the output profile before it is resized and compressed
    let cmyk_converted;
    let (samples, note) = match (&space, ctx.convert, &ctx.color.cmyk) {
        (ImgSpace::Cmyk, Some(conv), Some(source)) if n == 4 && conv.wants_cmyk(source) => {
            cmyk_converted = (conv.cmyk_pixels(source, samples, n + usize::from(alpha), ctx.color.intent), format!("{note}, CMYK profile converted"));
            (cmyk_converted.0.as_slice(), cmyk_converted.1.as_str())
        }
        _ => (samples, note),
    };
    let level = ctx.level;
    let stride = n + usize::from(alpha);
    let (nw, nh) = ctx.size(w, h);
    let resampled;
    let (w, h, samples) = if (nw, nh) != (w, h) && samples.len() >= w as usize * h as usize * stride {
        let reuse = ctx.target.is_some_and(|(_, d)| d.reuse_rows);
        resampled = box_resample(samples, w, h, stride, (nw, nh), reuse);
        (nw, nh, resampled.as_slice())
    } else {
        (w, h, samples)
    };
    let px = (w as usize) * (h as usize);
    let color = if alpha {
        let mut c = Vec::with_capacity(px * n);
        let mut a = Vec::with_capacity(px);
        for p in samples.chunks_exact(stride).take(px) {
            c.extend_from_slice(&p[..n]);
            a.push(p[n]);
        }
        let (smask, mask) = ctx.alpha_out(&a, w, h);
        (c, smask, mask)
    } else {
        (samples[..px * n].to_vec(), None, None)
    };
    let (color, smask, mask) = (color.0, color.1, color.2);
    if ctx.image_compression == "jpeg"
        && let Some((data, decode)) = encode_jpeg(&color, w, h, &space, ctx.jpeg_quality) {
        return Encoded { width: w, height: h, bpc: 8, space, data, filter: Filter::Dct, decode, smask, mask, note: note.into() };
    }
    Encoded { width: w, height: h, bpc: 8, space, data: flate(&color, level), filter: Filter::Flate, decode: None, smask, mask, note: note.into() }
}

/// RGB samples converted to the destination CMYK, downsampled like other images, written
/// as baseline JPEG at InDesign's "Maximum" quality (95) — Flate for palette images.
// Preserve the encoder boundary with independent dimensions, samples and image options.
#[allow(clippy::too_many_arguments)]
fn raster_cmyk(w: u32, h: u32, samples: &[u8], alpha: bool, icc: &Option<Arc<Vec<u8>>>, conv: &Conv, ctx: &Ctx, note: &str) -> Encoded {
    conv.converted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stride = 3 + usize::from(alpha);
    let px = w as usize * h as usize;
    let cmyk = conv.rgb_pixels_intent(icc, &samples[..px.min(samples.len() / stride) * stride], stride, ctx.color.intent);
    let out_stride = stride + 1;
    let (nw, nh) = ctx.size(w, h);
    let (w, h, cmyk) = if (nw, nh) != (w, h) && cmyk.len() >= px * out_stride {
        let reuse = ctx.target.is_some_and(|(_, d)| d.reuse_rows);
        (nw, nh, box_resample(&cmyk, w, h, out_stride, (nw, nh), reuse))
    } else {
        (w, h, cmyk)
    };
    let px = w as usize * h as usize;
    let (color, smask, mask) = if alpha {
        let mut c = Vec::with_capacity(px * 4);
        let mut a = Vec::with_capacity(px);
        for p in cmyk.chunks_exact(5).take(px) {
            c.extend_from_slice(&p[..4]);
            a.push(p[4]);
        }
        let (smask, mask) = ctx.alpha_out(&a, w, h);
        (c, smask, mask)
    } else {
        (cmyk, None, None)
    };
    let note = format!("{note} → CMYK");
    if ctx.image_compression == "zip" || note.starts_with("TIFF palette") || w < 16 || h < 16 || w > 65_500 || h > 65_500 {
        return Encoded { width: w, height: h, bpc: 8, space: ImgSpace::Cmyk, data: flate(&color, ctx.level), filter: Filter::Flate, decode: None, smask, mask, note };
    }
    match cmyk_jpeg_quality(&color, w, h, ctx.jpeg_quality) {
        Some(data) => Encoded {
            width: w,
            height: h,
            bpc: 8,
            space: ImgSpace::Cmyk,
            data,
            filter: Filter::Dct,
            decode: Some(vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]),
            smask,
            mask,
            note: format!("{note}, JPEG"),
        },
        None => Encoded { width: w, height: h, bpc: 8, space: ImgSpace::Cmyk, data: flate(&color, ctx.level), filter: Filter::Flate, decode: None, smask, mask, note },
    }
}

/// 8-bit CMYK (0 = no ink) as a quality-95 JPEG the way InDesign writes converted CMYK
/// images: YCCK (Adobe transform 2, far smaller than plain CMYK), optimised Huffman tables,
/// progressive; the samples are stored inverted with an Adobe marker, so the image needs
/// `/Decode [1 0 1 0 1 0 1 0]`. `None`: not encodable (size limits).
pub fn cmyk_jpeg(cmyk: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    cmyk_jpeg_quality(cmyk, w, h, 95)
}

fn cmyk_jpeg_quality(cmyk: &[u8], w: u32, h: u32, quality: u8) -> Option<Vec<u8>> {
    if w > 65_500 || h > 65_500 {
        return None;
    }
    let mut data = Vec::with_capacity(cmyk.len() / 8);
    let mut enc = jpeg_encoder::Encoder::new(&mut data, quality);
    enc.set_optimized_huffman_tables(true);
    enc.set_progressive(true);
    enc.encode(cmyk, w as u16, h as u16, jpeg_encoder::ColorType::CmykAsYcck).ok()?;
    Some(data)
}

pub(super) fn encode_jpeg(color: &[u8], w: u32, h: u32, space: &ImgSpace, quality: u8) -> Option<(Vec<u8>, Option<Vec<f32>>)> {
    if w > 65_500 || h > 65_500 { return None; }
    if matches!(space, ImgSpace::Cmyk) {
        return cmyk_jpeg_quality(color, w, h, quality).map(|data| (data, Some(vec![1.,0.,1.,0.,1.,0.,1.,0.])));
    }
    let kind = match space { ImgSpace::Rgb(_) => jpeg_encoder::ColorType::Rgb, ImgSpace::Gray => jpeg_encoder::ColorType::Luma, _ => return None };
    let mut data = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut data, quality);
    encoder.set_optimized_huffman_tables(true);
    encoder.encode(color, w as u16, h as u16, kind).ok()?;
    Some((data, None))
}

#[cfg(test)]
mod row_tests {
    use super::box_resample;

    #[test]
    fn repeated_scanlines_match_the_scalar_filter() {
        for stride in 1..=5 {
            let width = 17;
            let mut row: Vec<u8> = (0..width * stride).map(|i| (i * 23) as u8).collect();
            let mut src = Vec::new();
            for index in [0, row.len() / 2, row.len() - 1, 0] {
                src.extend_from_slice(&row);
                src.extend_from_slice(&row);
                row[index] ^= 255;
            }
            for size in [(1, 1), (8, 4), (12, 7), (17, 3), (9, 8)] {
                let scalar = box_resample(&src, width as u32, 8, stride, size, false);
                assert_eq!(box_resample(&src, width as u32, 8, stride, size, true), scalar);
            }
        }
    }
}
