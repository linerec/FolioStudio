use folio_model::{BevelStyle, ObjectEffects};

use super::{matte::MAX_BLUR, outer, outer::outset};

// R2, R19
pub(super) fn cap_ppu(ppu: f64, extent: f64) -> f64 {
    if extent > 1e-8 {
        ppu.min(MAX_BLUR as f64 / extent)
    } else {
        ppu
    }
}

pub fn inner_ppu(ppu: f64, fx: &ObjectEffects) -> f64 {
    let shadow = fx.inner_shadow.as_ref().map_or(0.0, |e| e.size);
    let glow = fx.inner_glow.as_ref().map_or(0.0, |e| e.size);
    cap_ppu(ppu, shadow.max(glow))
}

// R20
pub fn reach(fx: &ObjectEffects) -> f64 {
    let bevel = fx
        .bevel
        .as_ref()
        .filter(|b| b.style != BevelStyle::Inner)
        .map_or(0.0, |b| (b.size + b.soften + 2.0).max(0.0));
    outer(fx)
        .map(|(kind, e)| e.x_offset.abs().max(e.y_offset.abs()) + outset(e, kind))
        .fold(bevel, f64::max)
}
