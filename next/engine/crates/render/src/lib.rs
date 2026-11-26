//! Page rasteriser: draws one document page (its master items, then its spread items) into
//! an RGBA bitmap with tiny-skia.
//!
//! Draw order follows InDesign: layers bottom to top, items in their stacking order; a frame
//! draws its fill, then its placed graphic clipped to the frame, then its text and inline
//! objects, then its stroke. An item with opacity or a blend mode is drawn into a layer and
//! composited as a whole. Colours follow InDesign's display through the document's profiles
//! (`folio_color::Colors`, given in the `Scene`): CMYK through the CMYK profile, RGB through
//! the RGB profile, and on pages with transparency RGB content shown through the CMYK blend
//! space. Axis-aligned images are resampled by exact pixel area.

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub mod clipextract;
pub mod effects;
pub mod gif;
mod psd;
pub mod inks;
pub mod runs;
pub mod sep;
mod display;
pub mod mathsvg;
pub mod svg;
pub mod eps;
pub mod metafile;
pub use display::{DisplayOptions, ViewDisplay};

mod cache;
mod geometry;
mod image_decode;
mod page;
mod paint;
mod region;
mod preview_png;
pub use preview_png::bounded_png;
mod sampling;

pub use cache::{CachedImage, GlyphCache, ImageCache};
pub use image_decode::{
    ImageMode, binary_alpha, cmyk_jpeg_inks, graphic_is_proxy, header_has_alpha,
    high_bytes_of_raw16, is_proxy, lab8_to_srgb,
};
pub use page::{MAX_PIXELS, Raster, RenderStats, Scene, locate_page, page_count, render_page, render_page_display};
pub use paint::color;
pub use region::page_region;

use geometry::{build_path, item_extent, skia_stroke, to_skia};
use image_decode::{decode_rgba, tiff_samples};
#[cfg(test)]
use image_decode::{test_tiff, tiff_photometric};
use page::with_clip_layer;
use paint::solid;
use sampling::axis_taps;
