use folio_model::{ObjectEffects, PageItem, Rect, Satin};

use super::{
    Alpha, INNER_SIGMA, InnerLayer, Part, Shapes, bevel, blur, grow, matte, mul8, shifted,
    silhouette,
};

// R6
fn margin(fx: &ObjectEffects) -> f64 {
    let shadow = fx.inner_shadow.as_ref().map_or(0.0, |e| {
        e.x_offset.abs().max(e.y_offset.abs()) + 2.0 * e.size.max(0.0) + 2.0
    });
    let glow = fx
        .inner_glow
        .as_ref()
        .map_or(0.0, |g| 2.0 * g.size.max(0.0) + 2.0);
    let satin = fx
        .satin
        .as_ref()
        .map_or(0.0, |s| s.distance.abs() + 2.0 * s.size.max(0.0) + 2.0);
    let bevel = fx.bevel.as_ref().map_or(0.0, |b| b.size + b.soften + 4.0);
    2.0_f64.max(shadow).max(glow).max(satin).max(bevel)
}

// R8
fn clip_to_shape(a: &mut Alpha, shape: &Alpha) {
    for (value, &coverage) in a.data.iter_mut().zip(&shape.data) {
        *value = mul8(*value, coverage);
    }
}

// R9
fn inner_matte(a: &mut Alpha, size_px: f32, choke: f64, precise: bool, invert: bool, noise: f64) {
    let (spread, radius) = matte::split(size_px, choke.clamp(0.0, 1.0) as f32 * 0.99 * 100.0);
    matte::spread(a, spread, false);
    let mut values = matte::values(a, radius, precise);
    if invert {
        for value in &mut values {
            *value = matte::FULL - *value;
        }
    }
    matte::finish(a, values, noise);
}

// R12
fn satin_mask(shape: &Alpha, ppu: f64, s: &Satin) -> Alpha {
    let mut mask = shape.clone();
    blur(&mut mask, INNER_SIGMA * s.size.max(0.0) * ppu);
    let (sin, cos) = s.angle.to_radians().sin_cos();
    let (dx, dy) = (s.distance * cos * ppu, -s.distance * sin * ppu);
    let mut positive = shifted(&mask, dx, dy);
    let negative = shifted(&mask, -dx, -dy);
    for (value, other) in positive.iter_mut().zip(negative) {
        let diff = value.abs_diff(other);
        *value = if s.invert { 255 - diff } else { diff };
    }
    mask.data = positive;
    clip_to_shape(&mut mask, shape);
    mask
}

/// The inner effects of an item in painting order.
pub fn inner_layers<'d>(
    item: &PageItem,
    shapes: &Shapes<'_, '_>,
    ext: &Rect,
    ppu: f64,
    fx: &'d ObjectEffects,
    part: Part,
) -> Vec<InnerLayer<'d>> {
    let mut layers = Vec::new();
    if fx.inner_shadow.is_none()
        && fx.inner_glow.is_none()
        && fx.satin.is_none()
        && fx.bevel.is_none()
    {
        return layers;
    }
    // R7
    let rect = grow(*ext, margin(fx));
    let Some(shape) = silhouette(item, shapes, &rect, ppu, (0.0, 0.0), part).filter(Alpha::any)
    else {
        return layers;
    };
    // R10
    if let Some(e) = &fx.inner_shadow {
        let mut mask = shape.like(shifted(&shape, e.x_offset * ppu, e.y_offset * ppu));
        inner_matte(
            &mut mask,
            e.size as f32 * ppu as f32,
            e.spread,
            false,
            true,
            e.noise,
        );
        clip_to_shape(&mut mask, &shape);
        layers.push(InnerLayer {
            mask,
            color: &e.color,
            blend: e.blend,
            opacity: e.opacity,
        });
    }
    // R11
    if let Some(g) = &fx.inner_glow {
        let mut mask = shape.clone();
        inner_matte(
            &mut mask,
            g.size as f32 * ppu as f32,
            g.choke,
            g.precise,
            !g.center,
            g.noise,
        );
        clip_to_shape(&mut mask, &shape);
        layers.push(InnerLayer {
            mask,
            color: &g.color,
            blend: g.blend,
            opacity: g.opacity,
        });
    }
    if let Some(s) = &fx.satin {
        layers.push(InnerLayer {
            mask: satin_mask(&shape, ppu, s),
            color: &s.color,
            blend: s.blend,
            opacity: s.opacity,
        });
    }
    // R13
    if let Some(b) = &fx.bevel {
        let (highlight, shadow) = bevel(&shape, ppu, b);
        layers.push(InnerLayer {
            mask: highlight,
            color: &b.highlight,
            blend: b.highlight_blend,
            opacity: b.highlight_opacity,
        });
        layers.push(InnerLayer {
            mask: shadow,
            color: &b.shadow,
            blend: b.shadow_blend,
            opacity: b.shadow_opacity,
        });
    }
    layers
}
