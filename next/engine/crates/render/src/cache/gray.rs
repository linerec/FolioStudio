//! Keep large opaque grayscale PNG previews in their native one-byte representation until zoomed.
use crate::ImageMode;
use folio_color::{BlendSpace, Colors};
use image::{DynamicImage, GrayImage, ImageDecoder};
use tiny_skia::Pixmap;

pub(super) struct GrayPreview {
    pixels: GrayImage,
    colors: Box<[[u8; 4]; 256]>,
}

impl GrayPreview {
    pub fn decode(bytes: &[u8], allowance: u64, mode: ImageMode, colors: &Colors) -> Option<Self> {
        // Only attempt the compact path for an 8-bit grayscale IHDR over the worker's allowance.
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR") || bytes.get(24..26)? != [8, 0] {
            return None;
        }
        let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
        let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
        let rgba_bytes = u64::from(width)
            .checked_mul(u64::from(height))?
            .checked_mul(4)?;
        if rgba_bytes <= allowance
            || width > i32::MAX as u32 / 4
            || usize::try_from(rgba_bytes).is_err()
        {
            return None;
        }
        // A grayscale PNG with a transparency key expands to gray+alpha and uses the usual decoder.
        let mut limits = image::Limits::default();
        let mut decoder = image::codecs::png::PngDecoder::with_limits(
            std::io::Cursor::new(bytes),
            limits.clone(),
        )
        .ok()?;
        if decoder.color_type() != image::ColorType::L8 {
            return None;
        }
        limits.reserve(decoder.total_bytes()).ok()?;
        decoder.set_limits(limits).ok()?;
        let DynamicImage::ImageLuma8(pixels) = DynamicImage::from_decoder(decoder).ok()? else {
            return None;
        };
        let mut ramp = GrayImage::from_fn(256, 1, |x, _| image::Luma([x as u8]));
        ramp.set_color_space(pixels.color_space()).ok()?;
        let rgba = DynamicImage::ImageLuma8(ramp).to_rgba8();
        let colors = Box::new(std::array::from_fn(|i| {
            let p = rgba.get_pixel(i as u32, 0).0;
            if mode.blend == BlendSpace::Cmyk {
                let [r, g, b] = colors.gray_through_cmyk(p[0]);
                [r, g, b, 255]
            } else {
                p
            }
        }));
        Some(Self { pixels, colors })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.pixels.dimensions()
    }

    pub fn original(&self) -> Pixmap {
        let (w, h) = self.dimensions();
        let mut out = Pixmap::new(w, h).expect("validated grayscale preview dimensions");
        for (dst, &sample) in out.data_mut().chunks_exact_mut(4).zip(self.pixels.as_raw()) {
            dst.copy_from_slice(&self.colors[usize::from(sample)]);
        }
        out
    }

    pub fn half(&self) -> Option<Pixmap> {
        let (w, h) = self.dimensions();
        let mut out = Pixmap::new(w / 2, h / 2)?;
        let stride = w as usize;
        let mut dst = out.data_mut().chunks_exact_mut(4);
        for pair in self.pixels.as_raw().chunks_exact(stride * 2) {
            let (top, bottom) = pair.split_at(stride);
            for (a, b) in top.chunks_exact(2).zip(bottom.chunks_exact(2)) {
                let [p, q, r, s] = [a[0], a[1], b[0], b[1]].map(|i| self.colors[usize::from(i)]);
                let pixel = dst.next().expect("one destination per complete 2x2 block");
                for c in 0..3 {
                    pixel[c] = super::box_mean(p[c], q[c], r[c], s[c]);
                }
                pixel[3] = 255;
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CachedImage, Original};
    use super::*;
    use image::{ImageEncoder, codecs::png::PngEncoder};

    fn png(w: u32, h: u32, gray_alpha: bool) -> Vec<u8> {
        let (channels, ty) = if gray_alpha {
            (2, image::ExtendedColorType::La8)
        } else {
            (1, image::ExtendedColorType::L8)
        };
        let samples = (0..w * h * channels)
            .map(|i| i.wrapping_mul(113) as u8)
            .collect::<Vec<_>>();
        let mut encoded = Vec::new();
        PngEncoder::new(&mut encoded)
            .write_image(&samples, w, h, ty)
            .unwrap();
        encoded
    }

    #[test]
    fn compact_levels_match_reference_in_both_zoom_directions() {
        for (w, h) in [(61, 31), (1, 17), (28, 2), (32, 32)] {
            let bytes = png(w, h, false);
            for blend in [BlendSpace::Rgb, BlendSpace::Cmyk] {
                let mode = ImageMode {
                    blend,
                    proxy: false,
                };
                let colors = Colors::unmanaged();
                for initial in [0.01, 1.0] {
                    let compact = CachedImage::decode(&bytes, mode, &colors, Some(0)).unwrap();
                    let reference = CachedImage::decode(&bytes, mode, &colors, None).unwrap();
                    assert!(matches!(
                        compact.levels.lock().unwrap().original,
                        Original::Gray(_)
                    ));
                    for scale in [initial, 0.25, 0.03125, 0.75, 0.5, 0.499, 0.25] {
                        let (got, k) = compact.level_for(scale);
                        let (want, m) = reference.level_for(scale);
                        assert_eq!(
                            (k, got.width(), got.height(), got.data()),
                            (m, want.width(), want.height(), want.data())
                        );
                    }
                    assert_eq!(compact.original().data(), reference.original().data());
                }
            }
        }
    }

    #[test]
    fn respects_allowance_and_uses_fallback_for_alpha_and_invalid_data() {
        let mode = ImageMode {
            blend: BlendSpace::Rgb,
            proxy: true,
        };
        let colors = Colors::unmanaged();
        let opaque = png(19, 17, false);
        let threshold = 19 * 17 * 4;
        assert!(GrayPreview::decode(&opaque, threshold - 1, mode, &colors).is_some());
        assert!(GrayPreview::decode(&opaque, threshold, mode, &colors).is_none());
        let mut keyed = opaque.clone();
        // tRNS before the first IDAT, with a transparency key of zero and its CRC.
        keyed.splice(
            33..33,
            [
                0, 0, 0, 2, b't', b'R', b'N', b'S', 0, 0, 0x76, 0x93, 0xcd, 0x38,
            ],
        );
        for data in [png(19, 17, true), keyed] {
            let got = CachedImage::decode(&data, mode, &colors, Some(0)).unwrap();
            assert!(matches!(
                got.levels.lock().unwrap().original,
                Original::Rgba(_)
            ));
            assert_eq!(
                got.original().data(),
                CachedImage::decode(&data, mode, &colors, None)
                    .unwrap()
                    .original()
                    .data()
            );
        }
        assert!(CachedImage::decode(&opaque[..28], mode, &colors, Some(0)).is_none());
    }
}
