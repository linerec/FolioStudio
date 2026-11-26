//! Resolves screen paints through the document's colour profiles and blend space.

use crate::geometry::gradient_shader;
use folio_color::{BlendSpace, Colors};
use folio_model::{BlendMode, Document, GradientGeometry, Paint};
use tiny_skia::Transform;

pub(crate) fn skia_blend(b: BlendMode) -> tiny_skia::BlendMode {
    use tiny_skia::BlendMode as S;
    match b {
        BlendMode::Normal => S::SourceOver,
        BlendMode::Multiply => S::Multiply,
        BlendMode::Screen => S::Screen,
        BlendMode::Overlay => S::Overlay,
        BlendMode::SoftLight => S::SoftLight,
        BlendMode::HardLight => S::HardLight,
        BlendMode::ColorDodge => S::ColorDodge,
        BlendMode::ColorBurn => S::ColorBurn,
        BlendMode::Darken => S::Darken,
        BlendMode::Lighten => S::Lighten,
        BlendMode::Difference => S::Difference,
        BlendMode::Exclusion => S::Exclusion,
        BlendMode::Hue => S::Hue,
        BlendMode::Saturation => S::Saturation,
        BlendMode::Color => S::Color,
        BlendMode::Luminosity => S::Luminosity,
    }
}

/// Paint for a fill or stroke: a solid colour, or a linear/radial gradient placed by the
/// item's gradient geometry (inner coordinates).
pub(super) fn paint_for(
    doc: &Document,
    colors: &Colors,
    paint: &Paint,
    geometry: Option<&GradientGeometry>,
    blend: BlendSpace,
) -> Option<tiny_skia::Paint<'static>> {
    if let Paint::Gradient { id, .. } = paint
        && let (Some(gradient), Some(geo)) = (doc.gradients.get(id), geometry)
        && let Some(shader) = screen_gradient(doc, colors, gradient, geo, blend)
    {
        return Some(tiny_skia::Paint { shader, anti_alias: true, ..Default::default() });
    }
    color(doc, colors, paint, blend).map(solid)
}

fn screen_gradient(
    doc: &Document,
    colors: &Colors,
    g: &folio_model::Gradient,
    geo: &GradientGeometry,
    blend: BlendSpace,
) -> Option<tiny_skia::Shader<'static>> {
    // InDesign blends between stops in ink and bends the blend so the colours are half
    // mixed at the stop's midpoint: each pair of stops becomes `STEPS` linear pieces.
    const STEPS: usize = 16;
    let mut stops: Vec<tiny_skia::GradientStop> = Vec::new();
    let mut push = |pos: f64, rgb: [u8; 3]| {
        stops.push(tiny_skia::GradientStop::new((pos / 100.0).clamp(0.0, 1.0) as f32, tiny_skia::Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255)));
    };
    let swatch = |id: &str| doc.swatches.get(id);
    for (i, s) in g.stops.iter().enumerate() {
        let Some(next) = g.stops.get(i + 1) else {
            push(s.location, color(doc, colors, &Paint::Swatch { id: s.color.clone(), tint: 100.0 }, blend).unwrap_or([255, 255, 255]));
            break;
        };
        let (Some(a), Some(b)) = (swatch(&s.color), swatch(&next.color)) else { continue };
        let curve = folio_color::GradientCurve::new(next.midpoint);
        for step in 0..STEPS {
            let f = step as f64 / STEPS as f64;
            let rgb = colors.swatch_mix(a, b, curve.sample(f), blend).unwrap_or([255, 255, 255]);
            push(s.location + (next.location - s.location) * f, rgb);
        }
    }
    if stops.len() < 2 {
        return None;
    }
    gradient_shader(g.kind, geo, stops, Transform::identity())
}

pub(super) fn solid(rgb: [u8; 3]) -> tiny_skia::Paint<'static> {
    let mut p = tiny_skia::Paint::default();
    p.set_color_rgba8(rgb[0], rgb[1], rgb[2], 255);
    p.anti_alias = true;
    p
}

/// Display colour of a paint in a page's blend space, `None` when nothing is painted. A
/// gradient where it is not drawn as one shows its flat stand-in.
pub fn color(doc: &Document, colors: &Colors, paint: &Paint, blend: BlendSpace) -> Option<[u8; 3]> {
    match paint {
        Paint::None => None,
        Paint::Gradient { id, fallback_rgb } => doc
            .gradients
            .get(id)
            .and_then(|g| colors.gradient_flat(g, &doc.swatches, blend))
            .or_else(|| Some(if blend == BlendSpace::Cmyk { colors.srgb_through_cmyk(*fallback_rgb) } else { *fallback_rgb })),
        Paint::Swatch { id, tint } => colors.swatch_rgb(doc.swatches.get(id)?, *tint, blend),
    }
}
