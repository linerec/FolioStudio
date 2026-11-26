use crate::Raster;
use image::{ImageEncoder, RgbaImage, codecs::png::PngEncoder, imageops};

/// Encode a preview in memory, reducing dimensions if a detailed page exceeds the byte budget.
pub fn bounded_png(raster: Raster, budget: usize) -> Result<(Vec<u8>, u32, u32), String> {
    let mut pixels =
        RgbaImage::from_raw(raster.width, raster.height, raster.rgba).ok_or("invalid raster")?;
    loop {
        let (width, height) = pixels.dimensions();
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(
                pixels.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
        if bytes.len() <= budget {
            return Ok((bytes, width, height));
        }
        if (width, height) == (1, 1) {
            return Err("PNG budget too small".into());
        }
        pixels = imageops::resize(
            &pixels,
            (width * 3 / 4).max(1),
            (height * 3 / 4).max(1),
            imageops::FilterType::Triangle,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_image_is_resized_to_fit_the_wire_budget() {
        let mut value = 37_u32;
        let rgba = (0..128 * 128 * 4)
            .map(|_| {
                value = value.wrapping_mul(1664525).wrapping_add(1013904223);
                (value >> 24) as u8
            })
            .collect();
        let raster = Raster {
            width: 128,
            height: 128,
            rgba,
            stats: Default::default(),
        };
        let (png, width, height) = bounded_png(raster, 1200).unwrap();
        assert!(png.len() <= 1200 && width < 128 && height < 128);
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), (width, height));
    }
}
