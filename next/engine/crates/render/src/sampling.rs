//! Area sampling for axis-aligned images, with weights shared by the ink renderer.

use folio_model::Affine;
use tiny_skia::Pixmap;

/// Per output pixel along one axis: the first source pixel and the share of the output
/// pixel each source pixel from there covers (shares sum to 1 inside the image, less at
/// its edges).
pub(crate) struct AxisTaps {
    pub(crate) start: Vec<u32>,
    pub(crate) range: Vec<(u32, u32)>,
    pub(crate) weights: Vec<f32>,
}

/// Taps for output pixels `from..to` of an axis where source coordinate u maps to output
/// `scale · u + offset`, the source having `n` pixels.
pub(crate) fn axis_taps(scale: f64, offset: f64, n: u32, from: i64, to: i64) -> AxisTaps {
    let count = (to - from).max(0) as usize;
    let mut t = AxisTaps { start: Vec::with_capacity(count), range: Vec::with_capacity(count), weights: Vec::new() };
    let k = scale.abs();
    for i in from..to {
        let (u0, u1) = ((i as f64 - offset) / scale, (i as f64 + 1.0 - offset) / scale);
        let (lo, hi) = (u0.min(u1).max(0.0), u0.max(u1).min(n as f64));
        let first = t.weights.len() as u32;
        let j0 = lo.floor().max(0.0) as u32;
        let mut j = j0;
        while (j as f64) < hi && j < n {
            let overlap = (hi.min(j as f64 + 1.0) - lo.max(j as f64)).max(0.0);
            t.weights.push((overlap * k) as f32);
            j += 1;
        }
        t.start.push(j0);
        t.range.push((first, t.weights.len() as u32));
    }
    t
}

/// Exact area (box) resampling of premultiplied `src` through the axis-aligned
/// `to_device`, for the device pixels `area` (x0, y0, x1, y1) the image covers. The result
/// has a transparent one-pixel border; returns it with its device origin.
pub(crate) fn resample_box(src: &Pixmap, to_device: &Affine, area: (i64, i64, i64, i64)) -> Option<(Pixmap, i64, i64)> {
    let (sw, sh) = (src.width(), src.height());
    let (a, d, tx, ty) = (to_device.a, to_device.d, to_device.tx, to_device.ty);
    if a.abs() < 1e-12 || d.abs() < 1e-12 {
        return None;
    }
    // The image's device rectangle, cut to `area`.
    let (l, r) = (tx.min(tx + a * sw as f64), tx.max(tx + a * sw as f64));
    let (t, b) = (ty.min(ty + d * sh as f64), ty.max(ty + d * sh as f64));
    let x0 = (l.floor() as i64).max(area.0);
    let y0 = (t.floor() as i64).max(area.1);
    let x1 = (r.ceil() as i64).min(area.2);
    let y1 = (b.ceil() as i64).min(area.3);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let cols = axis_taps(a, tx, sw, x0, x1);
    let rows = axis_taps(d, ty, sh, y0, y1);
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    // Source rows used, then the horizontal pass over them.
    let r_lo = rows.start.iter().copied().min().unwrap_or(0) as usize;
    let r_hi = rows.range.iter().zip(&rows.start).map(|(&(f, e), &s)| s as usize + (e - f) as usize).max().unwrap_or(0).min(sh as usize);
    if r_hi <= r_lo {
        return None;
    }
    let data = src.data();
    let stride = sw as usize * 4;
    let mut tmp = vec![0f32; (r_hi - r_lo) * w * 4];
    for sr in r_lo..r_hi {
        let row = &data[sr * stride..(sr + 1) * stride];
        let out = &mut tmp[(sr - r_lo) * w * 4..(sr - r_lo + 1) * w * 4];
        for (i, (&(f, e), &j0)) in cols.range.iter().zip(&cols.start).enumerate() {
            let mut acc = [0f32; 4];
            for (k, &wt) in cols.weights[f as usize..e as usize].iter().enumerate() {
                let px = &row[(j0 as usize + k) * 4..(j0 as usize + k) * 4 + 4];
                for c in 0..4 {
                    acc[c] += wt * px[c] as f32;
                }
            }
            out[i * 4..i * 4 + 4].copy_from_slice(&acc);
        }
    }
    let mut dst = Pixmap::new(w as u32 + 2, h as u32 + 2)?;
    let dw = (w + 2) * 4;
    let dd = dst.data_mut();
    for (y, (&(f, e), &r0)) in rows.range.iter().zip(&rows.start).enumerate() {
        let out = &mut dd[(y + 1) * dw + 4..(y + 1) * dw + 4 + w * 4];
        for i in 0..w {
            let mut acc = [0f32; 4];
            for (k, &wt) in rows.weights[f as usize..e as usize].iter().enumerate() {
                let sr = r0 as usize + k - r_lo;
                let px = &tmp[(sr * w + i) * 4..(sr * w + i) * 4 + 4];
                for c in 0..4 {
                    acc[c] += wt * px[c];
                }
            }
            let alpha = acc[3].round().clamp(0.0, 255.0);
            out[i * 4 + 3] = alpha as u8;
            for c in 0..3 {
                out[i * 4 + c] = acc[c].round().clamp(0.0, alpha) as u8;
            }
        }
    }
    Some((dst, x0 - 1, y0 - 1))
}
