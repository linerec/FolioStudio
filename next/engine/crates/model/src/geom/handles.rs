use std::sync::OnceLock;

pub fn directions() -> [(f64, f64); 3] {
    let (s5, s2) = (5f64.sqrt(), 2f64.sqrt());
    [
        (1.0 / s5, -2.0 / s5),
        (2.0 / s5, -1.0 / s5),
        (1.0 / s2, -1.0 / s2),
    ]
}

pub(super) fn select_handles(
    support: [f64; 3],
    weights: [f64; 3],
    tolerance: f64,
) -> Option<(f64, f64, f64)> {
    static TABLE: OnceLock<FitTable> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let w = directions();
        FitTable::build(0.25, |a, b| FitTable::support(&w, a, b))
    });
    table.fit(support, weights, tolerance)
}

pub struct FitTable {
    r2_0: i32,
    rowscale: i32,
    /// (first entry, count, scale of r1 − r0, r2 of the row)
    rows: Vec<(usize, i32, i32, i32)>,
    /// k1, k2, r0, r1 and the unit normal of the support surface
    ents: Vec<[i32; 7]>,
}

const S30: f64 = 1073741824.0;

fn i32w(v: i64) -> i32 {
    v as i32
}

impl FitTable {
    /// Support values of the cubic (1,0) (k1,0) (0,k2) (0,1) along the three directions: the
    /// curve's extreme towards V, where its tangent is parallel to the direction.
    fn support(w: &[(f64, f64); 3], k1: f64, k2: f64) -> [f64; 3] {
        w.map(|(x, y)| {
            let p = [-y, -y * k1, x * k2, x];
            let [a, b, c] = [p[1] - p[0], p[2] - p[1], p[3] - p[2]];
            let den = a - 2.0 * b + c;
            let disc = (b * b - a * c).max(0.0).sqrt();
            let linear = 2.0 * (b - a);
            let roots = if den.abs() < 1e-14 {
                [-a / linear, f64::NAN]
            } else {
                [(a - b - disc) / den, (a - b + disc) / den]
            };
            roots
                .into_iter()
                .filter(|t| (0.0..1.0).contains(t))
                .fold(p[0].min(p[3]), |best, t| {
                    let u = 1.0 - t;
                    best.min(
                        p[0] * u * u * u
                            + 3.0 * p[1] * u * u * t
                            + 3.0 * p[2] * u * t * t
                            + p[3] * t * t * t,
                    )
                })
        })
    }

    pub fn build(column_rounding: f64, sup: impl Fn(f64, f64) -> [f64; 3]) -> FitTable {
        let s2 = 2f64.sqrt();
        let row_step = 3.0 / (107.0 * 4.0 * s2);
        // (k1, k2) with support r2 and r1 − r0 = dr, Newton from a guess
        let solve = |r2: f64, dr: f64, mut k1: f64, mut k2: f64| -> (f64, f64) {
            for _ in 0..60 {
                let r = sup(k1, k2);
                let f = (r[2] - r2, (r[1] - r[0]) - dr);
                if f.0.abs() < 1e-15 && f.1.abs() < 1e-15 {
                    break;
                }
                let h = 1e-7;
                let (ra, rb) = (sup(k1 + h, k2), sup(k1, k2 + h));
                let j = [
                    [(ra[2] - r[2]) / h, (rb[2] - r[2]) / h],
                    [
                        ((ra[1] - ra[0]) - (r[1] - r[0])) / h,
                        ((rb[1] - rb[0]) - (r[1] - r[0])) / h,
                    ],
                ];
                let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
                if det.abs() < 1e-30 {
                    break;
                }
                k1 -= (f.0 * j[1][1] - f.1 * j[0][1]) / det;
                k2 -= (j[0][0] * f.1 - j[1][0] * f.0) / det;
            }
            (k1, k2)
        };
        let normal = |k1: f64, k2: f64| -> [f64; 3] {
            let h = 1e-7;
            let (r, ra, rb) = (sup(k1, k2), sup(k1 + h, k2), sup(k1, k2 + h));
            let a = [(ra[0] - r[0]) / h, (ra[1] - r[1]) / h, (ra[2] - r[2]) / h];
            let b = [(rb[0] - r[0]) / h, (rb[1] - r[1]) / h, (rb[2] - r[2]) / h];
            let n = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if l > 1e-12 {
                n.map(|x| x / l)
            } else {
                [0.0; 3]
            }
        };
        // the row's far end: along the iso line to the side where r1 − r0 grows
        let end_point = |j: usize| -> (f64, f64, f64) {
            let k = j as f64 / 107.0;
            let r2 = sup(k, k)[2];
            let mut best = (k, k, 0.0);
            for sign in [-1.0, 1.0] {
                let (mut k1, mut k2, mut prev) = (k, k, 0.0f64);
                loop {
                    let n1 = (k1 + sign * 1e-3).clamp(0.0, 0.99);
                    let mut nk2 = k2;
                    for _ in 0..50 {
                        let r = sup(n1, nk2)[2];
                        let g = (sup(n1, nk2 + 1e-7)[2] - r) / 1e-7;
                        if g.abs() < 1e-30 {
                            break;
                        }
                        let step = (r - r2) / g;
                        nk2 -= step;
                        if step.abs() < 1e-15 {
                            break;
                        }
                    }
                    let r = sup(n1, nk2);
                    let dr = r[1] - r[0];
                    if dr <= prev {
                        break;
                    }
                    (k1, k2, prev) = (n1, nk2, dr);
                    if k1 == 0.0 || k1 == 0.99 {
                        break;
                    }
                }
                if prev > best.2 {
                    best = (k1, k2, prev);
                }
            }
            best
        };
        let q = |x: f64| (x * S30) as i64 as i32;
        let mut rows = Vec::new();
        let mut ents: Vec<[i32; 7]> = Vec::new();
        for j in 0..108 {
            let k = j as f64 / 107.0;
            let r = sup(k, k);
            let sym = normal(k, k);
            let start = ents.len();
            let push = |ents: &mut Vec<[i32; 7]>, k1: f64, k2: f64, r: [f64; 3], n: [f64; 3]| {
                let n = if n[0] * sym[0] + n[1] * sym[1] + n[2] * sym[2] < 0.0 {
                    n.map(|x| -x)
                } else {
                    n
                };
                ents.push([q(k1), q(k2), q(r[0]), q(r[1]), q(n[0]), q(n[1]), q(n[2])]);
            };
            push(&mut ents, k, k, r, sym);
            let (k1e, k2e, dre) = if j > 0 && j < 107 {
                end_point(j)
            } else {
                (k, k, 0.0)
            };
            // steps of about one row's r2 spacing (rounded column count)
            let n = if dre > 1e-6 {
                ((dre / row_step - column_rounding).ceil() as usize).max(1)
            } else {
                0
            };
            let (mut k1, mut k2) = (k, k);
            for i in 1..=n {
                (k1, k2) = solve(
                    r[2],
                    dre * i as f64 / n as f64,
                    k1 + (k1e - k) / n as f64,
                    k2 + (k2e - k) / n as f64,
                );
                push(&mut ents, k1, k2, sup(k1, k2), normal(k1, k2));
            }
            let scale = if n > 0 {
                (65536.0 * n as f64 / dre).round().min(2147483647.0) as i32
            } else {
                0
            };
            rows.push((start, (n + 1) as i32, scale, q(r[2])));
        }
        FitTable {
            r2_0: q(sup(0.0, 0.0)[2]),
            rowscale: (65536.0 / row_step).round() as i32,
            rows,
            ents,
        }
    }

    fn row(&self, x: i32) -> usize {
        let row = (i32w((x as i64 * self.rowscale as i64) >> 30) as i64 + 0x8000) >> 16;
        if row < 0 {
            0
        } else {
            (row as usize).min(self.rows.len() - 1)
        }
    }

    fn resid(&self, ei: usize, sw: bool, r: [i32; 3], r2ref: i32) -> [i32; 3] {
        let e = &self.ents[ei];
        if sw {
            [
                r[0].wrapping_sub(e[3]),
                r[1].wrapping_sub(e[2]),
                r[2].wrapping_sub(r2ref),
            ]
        } else {
            [
                r[0].wrapping_sub(e[2]),
                r[1].wrapping_sub(e[3]),
                r[2].wrapping_sub(r2ref),
            ]
        }
    }

    fn out(&self, err: i32, ei: usize, sw: bool) -> (i32, i32, i32) {
        let e = &self.ents[ei];
        if sw {
            (err, e[1], e[0])
        } else {
            (err, e[0], e[1])
        }
    }

    fn err(d: [i32; 3], w: [i32; 3]) -> i32 {
        let m = |a: i32, b: i32| i32w((a as i64 * b as i64) >> 30).wrapping_abs();
        m(d[0], w[0]).max(m(d[1], w[1])).max(m(w[2], d[2]))
    }

    fn entry_of(&self, row: usize, dr: i32) -> (usize, bool, i32) {
        let (start, count, scale, r2ref) = self.rows[row];
        let e = (i32w((dr as i64 * scale as i64) >> 30) as i64 + 0x8000) >> 16;
        (
            start + (e.unsigned_abs() as usize).min(count as usize - 1),
            e < 0,
            r2ref,
        )
    }

    /// Start at the row of r2 and the step of r1 − r0,
    /// then a walk along entry normals while the weighted residual shrinks.
    fn search(&self, r: [i32; 3], w: [i32; 3], tolint: i32) -> Option<(i32, i32, i32)> {
        let row = self.row(r[2].wrapping_sub(self.r2_0));
        let (ei, sw, r2ref) = self.entry_of(row, r[1].wrapping_sub(r[0]));
        let mut d = self.resid(ei, sw, r, r2ref);
        let mut err = FitTable::err(d, w);
        if err <= tolint {
            return Some(self.out(err, ei, sw));
        }
        let cdiv = |a: i64, b: i64| a / b;
        let (m0, m1, m2) = if w[0] <= w[1] && w[0] <= w[2] {
            (
                0x4000_0000,
                i32w(cdiv((w[0] as i64) << 30, w[1] as i64)),
                i32w(cdiv((w[0] as i64) << 30, w[2] as i64)),
            )
        } else if w[1] <= w[2] {
            (
                i32w(cdiv((w[1] as i64) << 30, w[0] as i64)),
                0x4000_0000,
                i32w(cdiv((w[1] as i64) << 30, w[2] as i64)),
            )
        } else {
            (
                i32w(cdiv((w[2] as i64) << 30, w[0] as i64)),
                i32w(cdiv((w[2] as i64) << 30, w[1] as i64)),
                0x4000_0000,
            )
        };
        let mul30 = |a: i32, b: i32| i32w((a as i64 * b as i64) >> 30);
        let (mut best, mut cur, mut cursw) = (ei, ei, sw);
        for _ in 0..1000 {
            let g = &self.ents[cur];
            let (g4, g5, g6) = (g[4], g[5], g[6]);
            let (t_d2, t_m2) = (mul30(d[2], g6), mul30(g6, m2.wrapping_neg()));
            let (p, qv) = if cursw {
                (
                    mul30(d[1], g4)
                        .wrapping_add(t_d2)
                        .wrapping_add(mul30(d[0], g5)),
                    mul30(m1, g4).wrapping_add(mul30(m0, g5)).wrapping_add(t_m2),
                )
            } else {
                (
                    mul30(d[1], g5)
                        .wrapping_add(t_d2)
                        .wrapping_add(mul30(d[0], g4)),
                    mul30(m0, g4).wrapping_add(t_m2).wrapping_add(mul30(g5, m1)),
                )
            };
            let aq = qv.wrapping_abs();
            let ps = if qv >= 0 { p } else { p.wrapping_neg() };
            if aq < 0x4000_0000 && (ps as i64).abs() >= 2 * aq as i64 {
                return None;
            }
            if aq == 0 {
                return Some(self.out(err, best, cursw));
            }
            let s = i32w(cdiv((ps as i64) << 30, aq as i64)).wrapping_neg();
            let row = self.row(
                mul30(s, m2.wrapping_neg())
                    .wrapping_sub(self.r2_0)
                    .wrapping_add(r[2]),
            );
            let dr = mul30(m1.wrapping_sub(m0), s)
                .wrapping_sub(r[0])
                .wrapping_add(r[1]);
            let (ne, nsw, r2ref) = self.entry_of(row, dr);
            if ne == best {
                return Some(self.out(err, ne, nsw));
            }
            d = self.resid(ne, nsw, r, r2ref);
            let nerr = FitTable::err(d, w);
            if nerr <= tolint {
                return Some(self.out(nerr, ne, nsw));
            }
            if err <= nerr {
                return Some(self.out(err, best, nsw));
            }
            (err, best, cur, cursw) = (nerr, ne, ne, nsw);
        }
        Some(self.out(err, best, cursw))
    }

    /// Handle factors for support values `r` with weights `w`, and the fit's error (pt).
    pub fn fit(&self, r: [f64; 3], w: [f64; 3], tol: f64) -> Option<(f64, f64, f64)> {
        let ri = r.map(|x| (x * S30) as i64 as i32);
        let wi = w.map(|x| (x * 65536.0) as i64 as i32);
        if wi.iter().any(|&w| w <= 0) {
            return None;
        }
        let (err, k1, k2) = self.search(ri, wi, (0.25 * tol * 65536.0) as i32)?;
        Some((k1 as f64 / S30, k2 as f64 / S30, err as f64 / 65536.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_symmetric_rows() {
        let directions = directions();
        let t = FitTable::build(0.25, |a, b| FitTable::support(&directions, a, b));
        assert_eq!(t.rows.len(), 108);
        assert_eq!(t.rowscale, 13222618);
        assert_eq!(t.r2_0, 189812531);
        // R3
        let e = t.ents[t.rows[48].0];
        assert!((e[0] as f64 / S30 - 48.0 / 107.0).abs() < 1e-9);
    }
}
