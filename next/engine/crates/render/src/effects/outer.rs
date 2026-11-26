use folio_model::{PageItem, Rect, ShadowEffect};

use super::{Alpha, Outer, Part, Shapes, grow, matte, offset, silhouette};

// R1
fn blur_extent(e: &ShadowEffect, kind: Outer) -> (f32, f32) {
    let size = e.size.max(0.0) as f32;
    let spread = e.spread.clamp(0.0, 1.0) as f32;
    let (extent, share) = match kind {
        Outer::DropShadow => {
            let choke = spread * size;
            let extent = choke + (size - choke) * 1.2;
            (
                extent,
                if extent > 1e-8 {
                    choke / extent
                } else {
                    spread
                },
            )
        }
        Outer::OuterGlow => (size, spread),
    };
    (extent, share * 0.99 * 100.0)
}

pub(super) fn outset(e: &ShadowEffect, kind: Outer) -> f64 {
    f64::from(blur_extent(e, kind).0) * 1.05 + 2.0
}

// R2
pub fn shadow_ppu(ppu: f64, e: &ShadowEffect, kind: Outer) -> f64 {
    super::reach::cap_ppu(ppu, f64::from(blur_extent(e, kind).0))
}

// R3
pub fn shadow_rect(ext: &Rect, e: &ShadowEffect, kind: Outer) -> Rect {
    grow(offset(ext, e.x_offset, e.y_offset), outset(e, kind))
}

// R5
pub fn shadow(
    item: &PageItem,
    shapes: &Shapes<'_, '_>,
    rect: &Rect,
    ppu: f64,
    e: &ShadowEffect,
    kind: Outer,
    part: Part,
) -> Option<Alpha> {
    let mut alpha = silhouette(item, shapes, rect, ppu, (e.x_offset, e.y_offset), part)?;
    if !alpha.any() {
        return Some(alpha);
    }
    let (extent, share) = blur_extent(e, kind);
    let (spread, radius) = matte::split(extent * ppu as f32, share);
    matte::spread(&mut alpha, spread, true);
    let values = if radius != 0 && kind == Outer::OuterGlow && e.precise {
        matte::precise_glow(&mut alpha, radius)
    } else {
        matte::values(&alpha, radius, false)
    };
    matte::finish(&mut alpha, values, e.noise);
    Some(alpha)
}
