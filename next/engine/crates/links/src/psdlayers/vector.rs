//! Vector masks of a PSD layer (`vmsk`): one closed shape (straight or Bézier edges) that is the layer's
//! visible area. `coverage` rasterises it over the canvas with anti-aliased edges (four sub-scanlines
//! per pixel row, exact coverage along each). Measured against InDesign (objlayer-c5): a rectangle on
//! pixel edges is exactly 0 or 1, the "disable" flag shows the whole layer, the "invert" flag shows
//! the outside. Several shapes (path operations) are not composed.
use super::{be16, be32};

pub(super) const INVERT: u32 = 1;
pub(super) const DISABLED: u32 = 4;

/// A path in canvas fractions (x, y): the outline as straight edges after flattening.
pub(super) struct Shape {
    pub flags: u32,
    pub edges: Vec<[(f32, f32); 2]>,
}

fn fixed(b: &[u8], at: usize) -> Option<f32> {
    Some(be32(b, at)? as i32 as f32 / (1 << 24) as f32)
}

/// Reads `vmsk` (version 3, flags, then 26-byte path records) as a single closed shape.
pub(super) fn parse(data: &[u8]) -> Result<Shape, String> {
    let bad = || "unreadable vector mask".to_string();
    if be32(data, 0).ok_or_else(bad)? != 3 {
        return Err("a vector mask of an unknown version".into());
    }
    let flags = be32(data, 4).ok_or_else(bad)?;
    let mut subpaths: Vec<Vec<[(f32, f32); 3]>> = Vec::new();
    let mut at = 8;
    let mut want = 0usize;
    while at + 26 <= data.len() {
        let kind = be16(data, at).ok_or_else(bad)?;
        match kind {
            0 => {
                if want != 0 {
                    return Err(bad());
                }
                want = be16(data, at + 2).ok_or_else(bad)? as usize;
                subpaths.push(Vec::new());
            }
            1 | 2 => {
                // closed knot: control before, anchor, control after (each vertical then horizontal)
                let point = |i: usize| -> Option<(f32, f32)> { Some((fixed(data, at + 2 + i * 8 + 4)?, fixed(data, at + 2 + i * 8)?)) };
                let knot = [point(0).ok_or_else(bad)?, point(1).ok_or_else(bad)?, point(2).ok_or_else(bad)?];
                subpaths.last_mut().ok_or_else(bad)?.push(knot);
                want = want.checked_sub(1).ok_or_else(bad)?;
            }
            3..=5 => return Err("a vector mask with an open path".into()),
            // 6 fill rule, 7 clipboard, 8 initial fill rule
            6..=8 => {
                if kind == 8 && be16(data, at + 2) == Some(1) {
                    return Err("a vector mask that starts with every pixel included".into());
                }
            }
            _ => return Err(bad()),
        }
        at += 26;
    }
    subpaths.retain(|s| !s.is_empty());
    if subpaths.len() != 1 {
        return Err("a vector mask with several shapes".into());
    }
    let knots = &subpaths[0];
    let mut edges = Vec::new();
    for i in 0..knots.len() {
        let (a, b) = (&knots[i], &knots[(i + 1) % knots.len()]);
        let (p0, p1, p2, p3) = (a[1], a[2], b[0], b[1]);
        if p0 == p1 && p2 == p3 {
            edges.push([p0, p3]);
        } else {
            let mut prev = p0;
            const STEPS: usize = 24;
            for s in 1..=STEPS {
                let t = s as f32 / STEPS as f32;
                let u = 1.0 - t;
                let mix = |f: fn((f32, f32)) -> f32| u * u * u * f(p0) + 3.0 * u * u * t * f(p1) + 3.0 * u * t * t * f(p2) + t * t * t * f(p3);
                let next = (mix(|p| p.0), mix(|p| p.1));
                edges.push([prev, next]);
                prev = next;
            }
        }
    }
    Ok(Shape { flags, edges })
}

impl Shape {
    /// The area the layer shows, 0..1 per pixel of a `w` × `h` canvas (the "disable" flag shows everything, "invert" the outside).
    pub(super) fn coverage(&self, w: usize, h: usize) -> Vec<f32> {
        if self.flags & DISABLED != 0 {
            return vec![1.0; w * h];
        }
        const SUB: usize = 4;
        let mut cov = vec![0.0f32; w * h];
        let edges: Vec<[(f32, f32); 2]> = self.edges.iter().map(|e| [(e[0].0 * w as f32, e[0].1 * h as f32), (e[1].0 * w as f32, e[1].1 * h as f32)]).collect();
        for y in 0..h {
            for s in 0..SUB {
                let sy = y as f32 + (s as f32 + 0.5) / SUB as f32;
                // crossings of the sub-scanline: x and winding direction
                let mut hits: Vec<(f32, i32)> = Vec::new();
                for [a, b] in &edges {
                    if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                        let t = (sy - a.1) / (b.1 - a.1);
                        hits.push((a.0 + t * (b.0 - a.0), if b.1 > a.1 { 1 } else { -1 }));
                    }
                }
                hits.sort_by(|p, q| p.0.total_cmp(&q.0));
                let mut winding = 0;
                for i in 0..hits.len() {
                    let before = winding;
                    winding += hits[i].1;
                    if before == 0 && winding != 0 {
                        // an inside span starts here and runs to the crossing where the winding returns to zero
                        let start = hits[i].0;
                        let mut end = start;
                        let mut wnd = winding;
                        for next in &hits[i + 1..] {
                            wnd += next.1;
                            end = next.0;
                            if wnd == 0 {
                                break;
                            }
                        }
                        let (x0, x1) = (start.clamp(0.0, w as f32), end.clamp(0.0, w as f32));
                        if x1 > x0 {
                            let (c0, c1) = (x0.floor() as usize, ((x1.ceil() as usize).max(1) - 1).min(w - 1));
                            for x in c0..=c1 {
                                let part = (x1.min(x as f32 + 1.0) - x0.max(x as f32)).max(0.0);
                                cov[y * w + x] += part / SUB as f32;
                            }
                        }
                    }
                }
            }
        }
        for v in &mut cov {
            *v = v.clamp(0.0, 1.0);
            if self.flags & INVERT != 0 {
                *v = 1.0 - *v;
            }
        }
        cov
    }
}
