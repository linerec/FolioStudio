//! The outline of a pixel mask: grow or shrink it (exact distance transform), follow the pixel
//! edges into loops (outer loops clockwise on the page, holes the other way), then either keep the
//! pixel outline (tolerance 0) or fit cubic curves within the tolerance (Schneider's curve fitting,
//! corners kept where the outline turns sharply).

use super::Mask;

type P = (f64, f64);

/// Squared distance of each cell to the nearest cell where `target` holds (Felzenszwalb–Huttenlocher).
fn distance2(target: &[bool], w: usize, h: usize) -> Vec<f64> {
    const INF: f64 = 1e20;
    let mut g: Vec<f64> = target.iter().map(|&t| if t { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
    let mut pass = |g: &mut [f64], len: usize, at: &dyn Fn(usize) -> usize| {
        for q in 0..len { f[q] = g[at(q)]; }
        let mut k = 0;
        v[0] = 0;
        z[0] = -INF;
        z[1] = INF;
        for q in 1..len {
            let mut s;
            loop {
                let p = v[k];
                s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
                if s <= z[k] && k > 0 { k -= 1; } else { break; }
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = INF;
        }
        k = 0;
        #[allow(clippy::needless_range_loop, reason = "the row or column length bounds the distance pass")]
        for q in 0..len {
            while z[k + 1] < q as f64 { k += 1; }
            let p = v[k];
            d[q] = (q as f64 - p as f64).powi(2) + f[p];
        }
        for q in 0..len { g[at(q)] = d[q]; }
    };
    for x in 0..w { pass(&mut g, h, &|q| q * w + x); }
    for y in 0..h { pass(&mut g, w, &|q| y * w + q); }
    g
}

/// The mask with a border of `pad` hidden cells, shrunk (`inset` > 0) or grown, and cut to the frame.
fn prepared(m: &Mask, inset: f64, frame: Option<[f64; 4]>) -> (Vec<bool>, usize, usize, usize) {
    let pad = inset.min(0.0).abs().ceil() as usize + 1;
    let (w, h) = (m.w + 2 * pad, m.h + 2 * pad);
    let mut on = vec![false; w * h];
    for y in 0..m.h {
        for x in 0..m.w {
            let keep = frame.is_none_or(|[l, t, r, b]| {
                let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                cx >= l && cx <= r && cy >= t && cy <= b
            });
            on[(y + pad) * w + x + pad] = m.on[y * m.w + x] && keep;
        }
    }
    if inset > 0.0 {
        let hidden: Vec<bool> = on.iter().map(|o| !o).collect();
        let d = distance2(&hidden, w, h);
        let r = (inset + 0.5).powi(2);
        on.iter_mut().zip(d).for_each(|(o, d)| *o = *o && d >= r);
    } else if inset < 0.0 {
        let d = distance2(&on, w, h);
        let r = (-inset + 0.5).powi(2);
        on.iter_mut().zip(d).for_each(|(o, d)| *o = *o || d <= r);
    }
    (on, w, h, pad)
}

// edge directions out of a vertex: right, down, left, up
const STEP: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The boundary loops by their turning vertices, the kept cells on the right of each edge.
fn loops(on: &[bool], w: usize, h: usize) -> Vec<Vec<(i64, i64)>> {
    let vw = w + 1;
    let mut out_edges = vec![0u8; vw * (h + 1)];
    let cell = |x: i64, y: i64| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && on[y as usize * w + x as usize];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !cell(x, y) { continue; }
            let v = |x: i64, y: i64| y as usize * vw + x as usize;
            if !cell(x, y - 1) { out_edges[v(x, y)] |= 1; }
            if !cell(x + 1, y) { out_edges[v(x + 1, y)] |= 2; }
            if !cell(x, y + 1) { out_edges[v(x + 1, y + 1)] |= 4; }
            if !cell(x - 1, y) { out_edges[v(x, y + 1)] |= 8; }
        }
    }
    let mut result = Vec::new();
    for start in 0..out_edges.len() {
        while out_edges[start] != 0 {
            let (sx, sy) = ((start % vw) as i64, (start / vw) as i64);
            let mut dir = out_edges[start].trailing_zeros() as usize;
            let (mut x, mut y) = (sx, sy);
            let mut pts = vec![(sx, sy)];
            loop {
                out_edges[y as usize * vw + x as usize] &= !(1 << dir);
                x += STEP[dir].0;
                y += STEP[dir].1;
                if (x, y) == (sx, sy) { break; }
                let bits = out_edges[y as usize * vw + x as usize];
                // a right turn first (cells touching at a corner stay apart), then straight on, then left
                let Some(next) = [(dir + 1) % 4, dir, (dir + 3) % 4].into_iter().find(|d| bits & (1 << d) != 0) else { break };
                if next != dir { pts.push((x, y)); }
                dir = next;
            }
            // the start is a corner unless the loop goes straight through it
            if pts.len() > 2 {
                let (a, b, c) = (pts[pts.len() - 1], pts[0], pts[1]);
                if (b.0 - a.0) * (c.1 - b.1) == (b.1 - a.1) * (c.0 - b.0) { pts.remove(0); }
            }
            if pts.len() >= 3 { result.push(pts); }
        }
    }
    result
}

fn area(p: &[P]) -> f64 {
    (0..p.len()).map(|i| { let (a, b) = (p[i], p[(i + 1) % p.len()]); a.0 * b.1 - b.0 * a.1 }).sum::<f64>() / 2.0
}

fn inside(pt: P, poly: &[P]) -> bool {
    let mut c = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a.1 > pt.1) != (b.1 > pt.1) && pt.0 < a.0 + (pt.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) { c = !c; }
    }
    c
}

fn sub(a: P, b: P) -> P { (a.0 - b.0, a.1 - b.1) }
fn add(a: P, b: P) -> P { (a.0 + b.0, a.1 + b.1) }
fn mul(a: P, k: f64) -> P { (a.0 * k, a.1 * k) }
fn dot(a: P, b: P) -> f64 { a.0 * b.0 + a.1 * b.1 }
fn len(a: P) -> f64 { dot(a, a).sqrt() }
fn unit(a: P) -> P { let l = len(a); if l > 0.0 { mul(a, 1.0 / l) } else { (0.0, 0.0) } }

/// Distance of `p` from the segment a–b.
fn seg_dist(p: P, a: P, b: P) -> f64 {
    let ab = sub(b, a);
    let t = if dot(ab, ab) > 0.0 { (dot(sub(p, a), ab) / dot(ab, ab)).clamp(0.0, 1.0) } else { 0.0 };
    len(sub(p, add(a, mul(ab, t))))
}

/// Douglas–Peucker on an open run: the kept indices.
fn simplify(p: &[P], eps: f64, from: usize, to: usize, keep: &mut Vec<usize>) {
    let (mut far, mut at) = (0.0, from);
    for i in from + 1..to {
        let d = seg_dist(p[i], p[from], p[to]);
        if d > far { far = d; at = i; }
    }
    if far > eps {
        simplify(p, eps, from, at, keep);
        keep.push(at);
        simplify(p, eps, at, to, keep);
    }
}

fn bezier(b: &[P; 4], t: f64) -> P {
    let u = 1.0 - t;
    let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    (0..4).fold((0.0, 0.0), |s, i| add(s, mul(b[i], k[i])))
}

fn generate(d: &[P], u: &[f64], t1: P, t2: P) -> [P; 4] {
    let (first, last) = (d[0], d[d.len() - 1]);
    let (mut c, mut x) = ([[0.0; 2]; 2], [0.0; 2]);
    for (i, &t) in u.iter().enumerate() {
        let s = 1.0 - t;
        let a0 = mul(t1, 3.0 * s * s * t);
        let a1 = mul(t2, 3.0 * s * t * t);
        c[0][0] += dot(a0, a0);
        c[0][1] += dot(a0, a1);
        c[1][1] += dot(a1, a1);
        let base = add(mul(first, s * s * s + 3.0 * s * s * t), mul(last, 3.0 * s * t * t + t * t * t));
        let tmp = sub(d[i], base);
        x[0] += dot(a0, tmp);
        x[1] += dot(a1, tmp);
    }
    c[1][0] = c[0][1];
    let det = c[0][0] * c[1][1] - c[1][0] * c[0][1];
    let (mut al, mut ar) = if det.abs() > 1e-12 {
        ((x[0] * c[1][1] - c[0][1] * x[1]) / det, (c[0][0] * x[1] - c[1][0] * x[0]) / det)
    } else { (0.0, 0.0) };
    let seg = len(sub(last, first));
    if al < 1e-6 * seg || ar < 1e-6 * seg { al = seg / 3.0; ar = seg / 3.0; }
    [first, add(first, mul(t1, al)), add(last, mul(t2, ar)), last]
}

fn max_error(d: &[P], b: &[P; 4], u: &[f64]) -> (f64, usize) {
    let (mut worst, mut at) = (0.0, d.len() / 2);
    for i in 1..d.len() - 1 {
        let e = dot(sub(bezier(b, u[i]), d[i]), sub(bezier(b, u[i]), d[i]));
        if e >= worst { worst = e; at = i; }
    }
    (worst, at)
}

fn reparameterize(d: &[P], u: &[f64], b: &[P; 4]) -> Vec<f64> {
    let q1 = [mul(sub(b[1], b[0]), 3.0), mul(sub(b[2], b[1]), 3.0), mul(sub(b[3], b[2]), 3.0)];
    let q2 = [mul(sub(q1[1], q1[0]), 2.0), mul(sub(q1[2], q1[1]), 2.0)];
    d.iter().zip(u).map(|(&p, &t)| {
        let s = 1.0 - t;
        let q = bezier(b, t);
        let d1 = add(add(mul(q1[0], s * s), mul(q1[1], 2.0 * s * t)), mul(q1[2], t * t));
        let d2 = add(mul(q2[0], s), mul(q2[1], t));
        let num = dot(sub(q, p), d1);
        let den = dot(d1, d1) + dot(sub(q, p), d2);
        if den.abs() < 1e-12 { t } else { (t - num / den).clamp(0.0, 1.0) }
    }).collect()
}

/// Cubic curves through `d` (tangent `t1` leaving the first point, `t2` entering the last, pointing back).
fn fit(d: &[P], t1: P, t2: P, err: f64, out: &mut Vec<[P; 4]>) {
    if d.len() == 2 {
        let dist = len(sub(d[1], d[0])) / 3.0;
        out.push([d[0], add(d[0], mul(t1, dist)), add(d[1], mul(t2, dist)), d[1]]);
        return;
    }
    let mut u = vec![0.0];
    for i in 1..d.len() { u.push(u[i - 1] + len(sub(d[i], d[i - 1]))); }
    let total = u[d.len() - 1].max(f64::MIN_POSITIVE);
    u.iter_mut().for_each(|t| *t /= total);
    let mut b = generate(d, &u, t1, t2);
    let (mut worst, mut split) = max_error(d, &b, &u);
    if worst < err { out.push(b); return; }
    if worst < err * 4.0 {
        for _ in 0..4 {
            u = reparameterize(d, &u, &b);
            b = generate(d, &u, t1, t2);
            (worst, split) = max_error(d, &b, &u);
            if worst < err { out.push(b); return; }
        }
    }
    let split = split.clamp(1, d.len() - 2);
    let tc = unit(sub(d[split - 1], d[split + 1]));
    fit(&d[..=split], t1, tc, err, out);
    fit(&d[split..], mul(tc, -1.0), t2, err, out);
}

/// The direction along `run` from its start, over about `reach` of its length.
fn tangent(run: &[P], reach: f64) -> P {
    let mut far = run[1];
    for &p in &run[1..] {
        far = p;
        if len(sub(p, run[0])) >= reach { break; }
    }
    unit(sub(far, run[0]))
}

/// A closed loop of turning vertices as (left, anchor, right) points.
fn curves(p: &[P], tol: f64) -> Vec<[P; 3]> {
    let n = p.len();
    if tol <= 0.0 {
        return p.iter().map(|&a| [a, a, a]).collect();
    }
    // corners: where the outline (its pixel steps smoothed away) turns by 60° or more
    let far = (1..n).max_by(|&a, &b| len(sub(p[a], p[0])).total_cmp(&len(sub(p[b], p[0])))).unwrap_or(n / 2);
    let ring: Vec<P> = p.iter().chain(std::iter::once(&p[0])).copied().collect();
    let mut keep = vec![0];
    simplify(&ring, 1.0, 0, far, &mut keep);
    keep.push(far);
    simplify(&ring, 1.0, far, n, &mut keep);
    keep.sort_unstable();
    keep.dedup();
    let corners: Vec<usize> = keep.iter().enumerate().filter(|&(k, &i)| {
        let (a, b) = (ring[keep[(k + keep.len() - 1) % keep.len()]], ring[keep[(k + 1) % keep.len()]]);
        let (d1, d2) = (unit(sub(ring[i], a)), unit(sub(b, ring[i])));
        dot(d1, d2) <= 0.5
    }).map(|(_, &i)| i).collect();
    let reach = (2.0 * tol).max(3.0);
    let mut segs: Vec<[P; 4]> = Vec::new();
    let piece = |run: &[P], t1: Option<P>, t2: Option<P>, segs: &mut Vec<[P; 4]>| {
        let mut idx = Vec::new();
        simplify(run, tol, 0, run.len() - 1, &mut idx);
        if idx.is_empty() {
            let (a, b) = (run[0], run[run.len() - 1]);
            segs.push([a, a, b, b]);
            return;
        }
        let rev: Vec<P> = run.iter().rev().copied().collect();
        fit(run, t1.unwrap_or_else(|| tangent(run, reach)), t2.unwrap_or_else(|| tangent(&rev, reach)), tol, segs);
    };
    if corners.is_empty() {
        // a smooth loop: start where the outline is lowest (as InDesign does), the same tangent both ways
        let start = (0..n).max_by(|&a, &b| p[a].1.total_cmp(&p[b].1).then(p[b].0.total_cmp(&p[a].0))).unwrap_or(0);
        let run: Vec<P> = (0..=n).map(|k| p[(start + k) % n]).collect();
        let back: Vec<P> = (0..=n).map(|k| p[(start + n - k) % n]).collect();
        let t = unit(sub(tangent(&run, reach), tangent(&back, reach)));
        piece(&run, Some(t), Some(mul(t, -1.0)), &mut segs);
    } else {
        for k in 0..corners.len() {
            let (from, to) = (corners[k], if k + 1 < corners.len() { corners[k + 1] } else { corners[0] + n });
            let run: Vec<P> = (from..=to).map(|i| p[i % n]).collect();
            if run.len() >= 2 { piece(&run, None, None, &mut segs); }
        }
    }
    let m = segs.len();
    (0..m).map(|k| [segs[(k + m - 1) % m][2], segs[k][0], segs[k][1]]).collect()
}

/// The outline of the kept cells in pixel coordinates of the mask (before any border).
pub(crate) fn outline(m: &Mask, tolerance: f64, inset: f64, inside_edges: bool, frame: Option<[f64; 4]>) -> Vec<Vec<[P; 3]>> {
    let (on, w, h, pad) = prepared(m, inset, frame);
    let off = pad as f64;
    let all: Vec<Vec<P>> = loops(&on, w, h).into_iter().map(|l| l.into_iter().map(|(x, y)| (x as f64 - off, y as f64 - off)).collect()).collect();
    let specks = tolerance * tolerance;
    let outer: Vec<&Vec<P>> = all.iter().filter(|l| area(l) > 0.0).collect();
    let mut kept: Vec<&Vec<P>> = Vec::new();
    for l in &all {
        let a = area(l);
        if a.abs() <= specks { continue; }
        // a point just beside the loop's first edge, outside what it keeps
        let (p0, p1) = (l[0], l[1]);
        let d = unit(sub(p1, p0));
        let probe = add(add(p0, mul(d, 0.5)), mul((-d.1, d.0), -0.25));
        let within = outer.iter().filter(|o| !std::ptr::eq(**o, l) && area(o) > a.abs() && inside(probe, o)).count();
        // without inside edges only the outermost loops; with them every loop
        if inside_edges || (a > 0.0 && within == 0) { kept.push(l); }
    }
    kept.into_iter().map(|l| curves(l, tolerance)).collect()
}
