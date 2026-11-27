//! Image XObject data in the image's own colour space (run on worker threads), RGB
//! converted to the destination CMYK for PDF/X-1a ([`Conv`]), and images decoded to ink for
//! the flattener's rasters ([`decode_ink`]) with the same conversion.

mod compression;
mod conversion;
mod decode;

pub use compression::{cmyk_jpeg, flate};
pub use conversion::Conv;
pub use decode::decode_ink;

use crate::graphics::{ImageJob, ImageSource};
use compression::target;
use decode::{bmp, gif, jpeg_converted, png, psd, read, tiff};
use folio_links::ImageFormat;
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ImgSpace {
    Gray,
    Cmyk,
    /// RGB with its embedded profile, or the document RGB profile when untagged.
    Rgb(Option<Arc<Vec<u8>>>),
    /// 1-bit stencil painted with the fill colour (InDesign paints bitmaps in black).
    Mask,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Dct,
    Flate,
}

pub struct Encoded {
    pub width: u32,
    pub height: u32,
    pub bpc: u8,
    pub space: ImgSpace,
    pub data: Vec<u8>,
    pub filter: Filter,
    pub decode: Option<Vec<f32>>,
    /// Flate-compressed 8-bit alpha (soft mask).
    pub smask: Option<Vec<u8>>,
    /// PDF 1.3 output: the alpha of a cut-out as a 1-bit explicit mask (`/Mask`; 1 = not
    /// painted), width, height and Flate data, at up to 8 × the image's resolution.
    pub mask: Option<(u32, u32, Vec<u8>)>,
    pub note: String,
}

/// What an image's alpha is: none (or all opaque), a cut-out (opaque and transparent areas
/// with only anti-aliased edges between them: an explicit 1-bit mask holds it in PDF 1.3),
/// or soft (fades, shadows: transparency the flattener rasterises).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaClass {
    Opaque,
    Cutout,
    Soft,
}

/// Classifies an alpha channel: soft when more than 0.2 % of the pixels are partly
/// transparent and more than two pixels away from any fully opaque or transparent pixel.
pub fn classify_alpha(a: &[u8], w: usize, h: usize) -> AlphaClass {
    if a.iter().all(|v| *v == 255) {
        return AlphaClass::Opaque;
    }
    let part: Vec<bool> = a.iter().map(|v| *v != 0 && *v != 255).collect();
    if !part.iter().any(|p| *p) {
        return AlphaClass::Cutout;
    }
    // hard pixels dilated by 2 (Chebyshev): rows, then columns
    let r = 2usize;
    let mut row = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1));
            row[y * w + x] = (x0..=x1).any(|k| !part[y * w + k]);
        }
    }
    let mut deep = 0usize;
    for y in 0..h {
        for x in 0..w {
            if !part[y * w + x] {
                continue;
            }
            let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1));
            if !(y0..=y1).any(|k| row[k * w + x]) {
                deep += 1;
            }
        }
    }
    if deep as f64 > 0.002 * (w * h) as f64 { AlphaClass::Soft } else { AlphaClass::Cutout }
}

/// The alpha class of an image source (decoded; JPEG has none).
pub fn alpha_class_of(source: &ImageSource) -> AlphaClass {
    let cell = RefCell::new(None);
    let ctx = Ctx { color: Default::default(), image_compression: "legacy", jpeg_quality: 95, level: 0, target: None, convert: None, no_smask: false, proxy: false, capture: Some(&cell) };
    let owned;
    let bytes: &[u8] = match source {
        ImageSource::Jpeg(_) => return AlphaClass::Opaque,
        // a proxy's alpha is a 1-bit mask
        ImageSource::Proxy(_) => return AlphaClass::Cutout,
        ImageSource::Decode(p, _) | ImageSource::Preview(p) => match read(p) {
            Ok(b) => {
                owned = b;
                &owned
            }
            Err(_) => return AlphaClass::Opaque,
        },
        ImageSource::Embedded(d) => d,
    };
    let ok = if bytes.starts_with(b"\x89PNG") {
        png(bytes, &ctx).is_ok()
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        tiff(bytes, &ctx).is_ok()
    } else if bytes.starts_with(b"8BPS") {
        psd(bytes, &ctx).is_ok()
    } else if bytes.starts_with(b"GIF8") {
        gif(bytes, &ctx).is_ok()
    } else if bytes.starts_with(b"BM") {
        bmp(bytes, &ctx).is_ok()
    } else {
        false
    };
    let Some(c) = cell.into_inner().filter(|_| ok) else { return AlphaClass::Opaque };
    if !c.alpha || c.stencil.is_some() {
        return AlphaClass::Opaque;
    }
    let stride = c.n + 1;
    let px = c.w as usize * c.h as usize;
    let a: Vec<u8> = c.samples.chunks_exact(stride).take(px).map(|p| p[c.n]).collect();
    classify_alpha(&a, c.w as usize, c.h as usize)
}

/// A cut-out's alpha as a 1-bit explicit mask: bilinear upsampled by `f`, painted (0) where
/// alpha ≥ 50 %.
/// A proxy's alpha as a 1-bit explicit mask at the image's own resolution: 1 (not painted)
/// where α = 0; `None` without such a pixel.
fn binary_mask(a: &[u8], w: u32, h: u32, level: u32) -> Option<(u32, u32, Vec<u8>)> {
    if !a.contains(&0) {
        return None;
    }
    let (wu, row) = (w as usize, (w as usize).div_ceil(8));
    let mut bits = vec![0u8; row * h as usize];
    for (i, v) in a.iter().enumerate().take(wu * h as usize) {
        if *v == 0 {
            bits[(i / wu) * row + (i % wu) / 8] |= 0x80 >> (i % wu % 8);
        }
    }
    Some((w, h, flate(&bits, level)))
}

pub(crate) fn explicit_mask(a: &[u8], w: u32, h: u32, f: u32, level: u32) -> (u32, u32, Vec<u8>) {
    let (mw, mh) = (w * f, h * f);
    let row = (mw as usize).div_ceil(8);
    let mut bits = vec![0u8; row * mh as usize];
    let at = |x: i64, y: i64| a[(y.clamp(0, h as i64 - 1) as usize) * w as usize + x.clamp(0, w as i64 - 1) as usize] as f32;
    for my in 0..mh as usize {
        let sy = (my as f32 + 0.5) / f as f32 - 0.5;
        let (y0, fy) = (sy.floor() as i64, sy - sy.floor());
        for mx in 0..mw as usize {
            let sx = (mx as f32 + 0.5) / f as f32 - 0.5;
            let (x0, fx) = (sx.floor() as i64, sx - sx.floor());
            let v = (at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx) * (1.0 - fy) + (at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx) * fy;
            if v < 127.5 {
                bits[my * row + mx / 8] |= 0x80 >> (mx % 8);
            }
        }
    }
    (mw, mh, flate(&bits, level))
}

/// Downsampling as InDesign's PDF/X-4 preset: colour and gray images above `above` ppi
/// (at their largest placement) are averaged down to `to` ppi.
#[derive(Clone, Copy, Debug)]
pub struct Downsample {
    pub above: f64,
    pub to: f64,
    pub reuse_rows: bool,
}

/// Encoding settings shared by all images.
#[derive(Clone)]
pub struct Settings {
    pub image_compression: String,
    pub jpeg_quality: u8,
    pub level: u32,
    pub downsample: Option<Downsample>,
    /// RGB to the destination CMYK (PDF/X-1a; PDF/X-3 for profiles PDF 1.3 cannot hold).
    pub convert: Option<Arc<Conv>>,
    /// PDF 1.3: no soft masks (an alpha the flattener did not see is dropped).
    pub no_smask: bool,
    pub doc_rgb: Option<Arc<Vec<u8>>>,
}

pub fn encode(job: &ImageJob, set: &Settings) -> Result<Encoded, String> {
    let proxy = matches!(job.source, ImageSource::Proxy(_));
    let ctx = Ctx { color: crate::image_color::for_source(&job.source, job.color.as_ref(), set.doc_rgb.clone()), image_compression: &set.image_compression, jpeg_quality: set.jpeg_quality, level: set.level, target: target(job, set.downsample), convert: set.convert.as_deref(), no_smask: set.no_smask, proxy, capture: None };
    let mut encoded = match &job.source {
        ImageSource::Jpeg(p) => {
            let bytes = read(p)?;
            jpeg_converted(&bytes, &ctx)
        }
        ImageSource::Decode(p, f) => {
            let bytes = read(p)?;
            match f {
                ImageFormat::Png => png(&bytes, &ctx),
                ImageFormat::Tiff => tiff(&bytes, &ctx),
                ImageFormat::Psd => psd(&bytes, &ctx),
                ImageFormat::Gif => gif(&bytes, &ctx),
                ImageFormat::Bmp => bmp(&bytes, &ctx),
                other => Err(format!("{other:?} not decodable")),
            }
        }
        ImageSource::Embedded(d) => {
            let bytes: &[u8] = d;
            if bytes.starts_with(&[0xFF, 0xD8]) {
                jpeg_converted(bytes, &ctx)
            } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                png(bytes, &ctx)
            } else if bytes.starts_with(&[b'I', b'I', b'*', 0]) || bytes.starts_with(&[b'M', b'M', 0, b'*']) {
                tiff(bytes, &ctx)
            } else if bytes.starts_with(b"8BPS") {
                psd(bytes, &ctx)
            } else if bytes.starts_with(b"GIF8") {
                gif(bytes, &ctx)
            } else if bytes.starts_with(b"BM") {
                bmp(bytes, &ctx)
            } else {
                Err("embedded data format not recognised".into())
            }
        }
        ImageSource::Preview(p) | ImageSource::Proxy(p) => {
            let bytes = read(p)?;
            if bytes.starts_with(b"\x89PNG") {
                png(&bytes, &ctx)
            } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
                tiff(&bytes, &ctx)
            } else if bytes.starts_with(&[0xFF, 0xD8]) {
                jpeg_converted(&bytes, &ctx)
            } else {
                Err("preview format not recognised".into())
            }
        }
    }?;
    if let Some(warning) = &ctx.color.warning {
        encoded.note.push_str(&format!("; {warning}; original image profile retained"));
    }
    Ok(encoded)
}

/// Samples captured instead of encoded ([`decode_ink`]).
struct Captured {
    w: u32,
    h: u32,
    n: usize,
    alpha: bool,
    space: ImgSpace,
    samples: Vec<u8>,
    /// 1-bit stencil rows (packed) and whether bit 1 paints.
    stencil: Option<(Vec<u8>, bool)>,
}

struct Ctx<'a> {
    color: crate::image_color::Resolved,
    image_compression: &'a str,
    jpeg_quality: u8,
    level: u32,
    /// Largest placed size in points and the policy, to size the samples.
    target: Option<((f64, f64), Downsample)>,
    convert: Option<&'a Conv>,
    no_smask: bool,
    /// A placed PDF / EPS proxy: colour as stored, alpha a 1-bit mask (see
    /// [`ImageSource::Proxy`]).
    proxy: bool,
    /// Decode only: the samples go here.
    capture: Option<&'a RefCell<Option<Captured>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::compression::encode_jpeg;
    use crate::graphics::{ImageJob, ImageSource};
    use std::io::Read;

    fn inflate(d: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(d).read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn jpeg_settings_resize_recompress_and_preserve_cmyk_inks() {
        let pixels: Vec<u8> = (0..600 * 300 * 3).map(|i| ((i * 37 + i / 31) % 256) as u8).collect();
        let (source, _) = encode_jpeg(&pixels, 600, 300, &ImgSpace::Rgb(None), 95).unwrap();
        let mut ctx = Ctx { color: Default::default(), image_compression: "zip", jpeg_quality: 95, level: 6,
            target: Some(((72., 36.), Downsample { above: 450., to: 300., reuse_rows: true })),
            convert: None, no_smask: false, proxy: false, capture: None };
        let zip = jpeg_converted(&source, &ctx).unwrap();
        assert_eq!((zip.width, zip.height), (300, 150));
        assert!(matches!(zip.filter, Filter::Flate));
        assert_eq!(inflate(&zip.data).len(), 300 * 150 * 3);
        ctx.image_compression = "jpeg";
        let high = jpeg_converted(&source, &ctx).unwrap();
        ctx.jpeg_quality = 15;
        let low = jpeg_converted(&source, &ctx).unwrap();
        assert!(matches!(low.filter, Filter::Dct));
        assert!(low.data.len() < high.data.len());
        let cmyk: Vec<u8> = [20, 70, 120, 170].repeat(32 * 32);
        let source = cmyk_jpeg(&cmyk, 32, 32).unwrap();
        ctx.image_compression = "zip";
        ctx.target = None;
        let encoded = jpeg_converted(&source, &ctx).unwrap();
        assert!(matches!(encoded.space, ImgSpace::Cmyk));
        for (i, value) in inflate(&encoded.data).iter().enumerate() {
            assert!((*value as i32 - cmyk[i] as i32).abs() < 4, "CMYK ink polarity");
        }
    }

    /// A CMYK image with an assigned profile goes to the output profile when the output converts (PDF/X-1a), as InDesign 2025 does
    /// (`artifacts/imgcolor-c2-20261023/`, `crates/color/tests/cmyk_link.rs`); without the assignment, or when the output keeps
    /// numbers (PDF/X-4), or when the assignment is the output profile itself, the numbers stay.
    #[test]
    fn cmyk_image_with_assigned_profile_converts_to_the_output_profile() {
        let index = folio_color::ProfileIndex::scan_installed();
        let (Some(japan), Some(swop)) = (index.find("Japan Color 2001 Coated"), index.find("U.S. Web Coated (SWOP) v2")) else {
            eprintln!("SKIPPED: the Japan Color 2001 Coated and U.S. Web Coated (SWOP) v2 profiles are not installed");
            return;
        };
        let (japan, swop) = (Arc::new(folio_color::read_profile(&japan).unwrap()), Arc::new(folio_color::read_profile(&swop).unwrap()));
        let ink = [255u8, 28, 100, 0]; // flat patch: the JPEG round trip keeps it within a few levels
        let source = cmyk_jpeg(&ink.repeat(32 * 32), 32, 32).unwrap();
        let run = |assigned: Option<&Arc<Vec<u8>>>, convert: bool| {
            let conv = Conv::new(folio_color::Colors::unmanaged(), swop.clone(), None, convert).with_cmyk_sources(convert);
            let mut color = crate::image_color::Resolved::default();
            color.cmyk = assigned.cloned();
            let ctx = Ctx { color, image_compression: "zip", jpeg_quality: 95, level: 6, target: None, convert: Some(&conv), no_smask: false, proxy: false, capture: None };
            let encoded = jpeg_converted(&source, &ctx).unwrap();
            assert!(matches!(encoded.space, ImgSpace::Cmyk));
            let data = inflate(&encoded.data);
            [data[0], data[1], data[2], data[3]]
        };
        let near = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 4);
        let want = folio_color::cmyk_link_with_intent(&japan, &swop, folio_color::icc::Intent::Relative).unwrap().eval8(ink);
        assert_ne!(want, ink, "the assigned profile changes this colour");
        assert!(near(run(Some(&japan), true), want), "converted");
        assert!(near(run(None, true), ink), "no assignment: numbers kept");
        assert!(near(run(Some(&japan), false), ink), "output that keeps numbers");
        assert!(near(run(Some(&swop), true), ink), "assigned the output profile itself");
    }

    /// Downsampling of a picture drawn with different scales on its two axes, as InDesign 2025 does it (X-1a preset: above 450 ppi to 300;
    /// artifacts/pdfxcheck-c2-20261023/work/indesign/nonuni.result.json, exported with and without sampling): the picture is resampled
    /// only when its higher resolution is above 450 AND its lower one above 300, and then each axis goes to 300 by itself.
    /// Folio used the higher axis for both: a 300 x 600 ppi picture lost half its horizontal detail (150 ppi), a 300 x 750 one 60 % (120 ppi),
    /// where InDesign keeps both as they are.
    #[test]
    fn downsampling_treats_each_axis_by_itself() {
        let ctx = |placed: (f64, f64)| Ctx { color: Default::default(), image_compression: "zip", jpeg_quality: 95, level: 6,
            target: Some((placed, Downsample { above: 450., to: 300., reuse_rows: true })), convert: None, no_smask: false, proxy: false, capture: None };
        // a 600 x 600 px picture; placed (width, height) in points = 144 pt x scale; InDesign's resulting ppi per axis in the comment
        let cases: [((f64, f64), (u32, u32)); 10] = [
            ((72.0, 47.95), (300, 200)),    // 600 x 901 ppi -> 300 x 300
            ((144.0, 57.6), (600, 600)),    // 300 x 750: kept (InDesign 300 x 750)
            ((100.8, 57.6), (420, 240)),    // 429 x 750 -> 300 x 300 (InDesign 300.1 x 301.3)
            ((57.6, 144.0), (600, 600)),    // 750 x 300: kept
            ((57.6, 143.99), (600, 600)),   // 750 x 300.01 (a scale of 0.9999): kept, the lower axis is 300
            ((144.0, 43.2), (600, 600)),    // 300 x 1000: kept
            ((144.0, 144.0), (600, 600)),   // 300 x 300
            ((108.0, 108.0), (600, 600)),   // 400 x 400: below the 450 threshold
            ((98.2, 98.2), (600, 600)),     // 440 x 440: kept
            ((93.6, 93.6), (390, 390)),     // 462 x 462 -> 300 x 300 (InDesign 300.6)
        ];
        for (placed, want) in cases {
            let got = ctx(placed).size(600, 600);
            assert!((got.0 as i64 - want.0 as i64).abs() <= 1 && (got.1 as i64 - want.1 as i64).abs() <= 1, "placed {placed:?}: {got:?}, expected {want:?}");
        }
    }

    /// A bilevel TIFF written without a BitsPerSample tag (the TIFF default is 1 bit; Pillow and other writers omit it) is a stencil:
    /// it used to be read as 8-bit gray, fail with "TIFF data short", and leave the page drawing an XObject that was never written.
    #[test]
    fn bilevel_tiff_without_bits_per_sample_is_a_stencil() {
        let (w, h) = (16u16, 2u16);
        let strip = [0b1010_1010u8, 0xFF, 0x0F, 0xF0];
        for photometric in [0u16, 1] {
            let mut tiff: Vec<u8> = b"II*\0".to_vec();
            tiff.extend(8u32.to_le_bytes());
            let entries: [(u16, u16, u32); 8] = [(256, 3, w as u32), (257, 3, h as u32), (259, 3, 1), (262, 3, photometric as u32),
                (273, 4, 8 + 2 + 8 * 12 + 4), (277, 3, 1), (278, 3, h as u32), (279, 4, strip.len() as u32)];
            tiff.extend((entries.len() as u16).to_le_bytes());
            for (tag, kind, value) in entries {
                tiff.extend(tag.to_le_bytes());
                tiff.extend(kind.to_le_bytes());
                tiff.extend(1u32.to_le_bytes());
                tiff.extend(value.to_le_bytes());
            }
            tiff.extend(0u32.to_le_bytes());
            tiff.extend(strip);
            let dir = std::env::temp_dir().join(format!("folio-pdf-bilevel-{}-{photometric}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("bilevel.tif");
            std::fs::write(&path, &tiff).unwrap();
            let job = ImageJob { source: ImageSource::Decode(path, ImageFormat::Tiff), color: None, owners: Vec::new(), placed: (0.0, 0.0), bytes: 0, decoded: None, uses: 1 };
            let set = Settings { doc_rgb: None, image_compression: "legacy".into(), jpeg_quality: 95, level: 6, downsample: None, convert: None, no_smask: false };
            let e = encode(&job, &set).expect("a bilevel TIFF without BitsPerSample encodes");
            assert_eq!((e.width, e.height, e.bpc), (16, 2, 1));
            assert!(matches!(e.space, ImgSpace::Mask));
            // black pixels are the stored 0 bits for BlackIsZero and the stored 1 bits for WhiteIsZero; the stencil must paint exactly those
            let paints_one = e.decode.is_some();
            let painted: Vec<u8> = inflate(&e.data).iter().map(|b| if paints_one { *b } else { !*b }).collect();
            let black: Vec<u8> = strip.iter().map(|b| if photometric == 0 { *b } else { !*b }).collect();
            assert_eq!(painted, black, "photometric {photometric}");
        }
    }

    /// An 8-bit gray TIFF whose photometric is WhiteIsZero (0 = white, typical of scanners and fax) must print as stored, not as a negative:
    /// the tiff decoder already inverts those samples while reading, and Folio inverted them a second time.
    #[test]
    fn white_is_zero_gray_tiff_is_not_printed_as_a_negative() {
        let (w, h) = (4u16, 1u16);
        let strip = [0u8, 255, 64, 192];
        for (photometric, want) in [(1u16, [0u8, 255, 64, 192]), (0, [255, 0, 191, 63])] {
            let mut tiff: Vec<u8> = b"II*\0".to_vec();
            tiff.extend(8u32.to_le_bytes());
            let entries: [(u16, u16, u32); 9] = [(256, 3, w as u32), (257, 3, h as u32), (258, 3, 8), (259, 3, 1), (262, 3, photometric as u32),
                (273, 4, 8 + 2 + 9 * 12 + 4), (277, 3, 1), (278, 3, h as u32), (279, 4, strip.len() as u32)];
            tiff.extend((entries.len() as u16).to_le_bytes());
            for (tag, kind, value) in entries {
                tiff.extend(tag.to_le_bytes());
                tiff.extend(kind.to_le_bytes());
                tiff.extend(1u32.to_le_bytes());
                tiff.extend(value.to_le_bytes());
            }
            tiff.extend(0u32.to_le_bytes());
            tiff.extend(strip);
            let dir = std::env::temp_dir().join(format!("folio-pdf-gray-{}-{photometric}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("gray.tif");
            std::fs::write(&path, &tiff).unwrap();
            let job = ImageJob { source: ImageSource::Decode(path, ImageFormat::Tiff), color: None, owners: Vec::new(), placed: (0.0, 0.0), bytes: 0, decoded: None, uses: 1 };
            let set = Settings { doc_rgb: None, image_compression: "legacy".into(), jpeg_quality: 95, level: 6, downsample: None, convert: None, no_smask: false };
            let e = encode(&job, &set).expect("an 8-bit gray TIFF encodes");
            assert!(matches!(e.space, ImgSpace::Gray));
            // DeviceGray: 255 is white. BlackIsZero stores it so; WhiteIsZero stores white as 0.
            let samples = inflate(&e.data);
            let shown: Vec<u8> = if e.decode.is_some() { samples.iter().map(|v| 255 - v).collect() } else { samples };
            assert_eq!(shown, want, "photometric {photometric}");
        }
    }

    #[test]
    fn proxy_alpha_is_a_one_bit_mask_over_the_stored_colour() {
        let dir = std::env::temp_dir().join(format!("folio-pdf-proxy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("proxy.png");
        // as the reader's proxies: opaque black, a partly transparent black edge, clear white
        image::RgbaImage::from_raw(3, 1, vec![0, 0, 0, 255, 0, 0, 0, 108, 255, 255, 255, 0]).unwrap().save(&path).unwrap();
        let job = |source| ImageJob { source, color: None, owners: Vec::new(), placed: (0.0, 0.0), bytes: 0, decoded: None, uses: 1 };
        for no_smask in [false, true] {
            let set = Settings { doc_rgb: None, image_compression: "legacy".into(), jpeg_quality: 95, level: 6, downsample: None, convert: None, no_smask };
            let e = encode(&job(ImageSource::Proxy(path.clone())), &set).unwrap();
            assert!(e.smask.is_none(), "a proxy is never transparency");
            let (w, h, bits) = e.mask.expect("explicit mask");
            assert_eq!((w, h), (3, 1));
            // only the clear pixel is masked; the edge prints its stored black, opaque
            assert_eq!(inflate(&bits)[0] & 0xE0, 0x20);
            assert_eq!(inflate(&e.data), vec![0, 0, 0, 0, 0, 0, 255, 255, 255]);
        }
        // an image preview keeps its soft alpha
        let set = Settings { doc_rgb: None, image_compression: "legacy".into(), jpeg_quality: 95, level: 6, downsample: None, convert: None, no_smask: false };
        let e = encode(&job(ImageSource::Preview(path)), &set).unwrap();
        assert!(e.smask.is_some() && e.mask.is_none());
    }
}
