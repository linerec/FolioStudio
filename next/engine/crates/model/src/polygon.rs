//! Polygon and star vertices in page coordinates.
/// The corners of the regular polygon of `sides` sides on the unit circle (an odd one points up, an even one stands on a side).
fn outer(sides: u32) -> Vec<(f64, f64)> {
    let start = -std::f64::consts::FRAC_PI_2 + if sides.is_multiple_of(2) { std::f64::consts::PI / sides as f64 } else { 0.0 };
    (0..sides).map(|i| {
        let angle = start - i as f64 * std::f64::consts::TAU / sides as f64;
        (angle.cos(), angle.sin())
    }).collect()
}
fn outer_bounds(outer: &[(f64, f64)]) -> [f64; 4] {
    outer.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)])
}
/// Width over height of the regular polygon's own box (the Shift drag draws a polygon of this proportion: a hexagon is 2 : sqrt(3) wide).
pub fn aspect(sides: u32) -> f64 {
    let b = outer_bounds(&outer(sides.clamp(3, 100)));
    (b[2] - b[0]) / (b[3] - b[1])
}
pub fn points(rect: [f64; 4], sides: u32, inset: f64) -> Result<Vec<(f64, f64)>, &'static str> {
    if !(3..=100).contains(&sides) || !inset.is_finite() || !(0.0..=100.0).contains(&inset)
        || rect.iter().any(|v| !v.is_finite()) || rect[2] <= rect[0] || rect[3] <= rect[1] {
        return Err("invalid polygon geometry");
    }
    let outer = outer(sides);
    let bounds = outer_bounds(&outer);
    let mut vertices = Vec::with_capacity(sides as usize * 2);
    for (i, &a) in outer.iter().enumerate() {
        vertices.push(a);
        if inset > 0.0 {
            let b = outer[(i + 1) % outer.len()];
            let scale = (1.0 - inset / 100.0) / 2.0;
            vertices.push(((a.0 + b.0) * scale, (a.1 + b.1) * scale));
        }
    }
    Ok(vertices.into_iter().map(|p| (
        rect[0] + (p.0 - bounds[0]) / (bounds[2] - bounds[0]) * (rect[2] - rect[0]),
        rect[1] + (p.1 - bounds[1]) / (bounds[3] - bounds[1]) * (rect[3] - rect[1]),
    )).collect())
}
