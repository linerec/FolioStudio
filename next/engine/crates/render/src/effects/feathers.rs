mod basic;
mod diffusion;
mod directional;

use folio_model::{ObjectEffects, PageItem, Rect};

use super::{Alpha, Part, Shapes, gradient_feather, grow, silhouette};

// R16, R18
fn square(v: u32) -> u8 {
    (v * v / 255) as u8
}

// R14
pub fn feather_mask(
    item: &PageItem,
    shapes: &Shapes<'_, '_>,
    ext: &Rect,
    ppu: f64,
    fx: &ObjectEffects,
    part: Part,
) -> Option<Alpha> {
    if fx.feather.is_none() && fx.directional_feather.is_none() && fx.gradient_feather.is_none() {
        return None;
    }
    let mut alpha = silhouette(item, shapes, &grow(*ext, 2.0), ppu, (0.0, 0.0), part)?;
    if !alpha.any() {
        return Some(alpha);
    }
    if let Some(f) = &fx.feather {
        basic::apply(&mut alpha, ppu, f);
    }
    if let Some(d) = &fx.directional_feather {
        directional::apply(item, shapes, &mut alpha, ext, ppu, d, part);
    }
    if let Some(g) = &fx.gradient_feather {
        gradient_feather(&mut alpha, ppu, ext, g, &item.transform);
    }
    Some(alpha)
}
