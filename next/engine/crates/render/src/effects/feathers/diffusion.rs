use super::super::Alpha;

const FULL: i32 = (1 << 29) - 1;

struct Grid {
    w: usize,
    h: usize,
    values: Vec<i32>,
    fixed: Vec<bool>,
}

// R17
pub(super) fn apply(a: &mut Alpha, radius: usize) {
    let mut grid = Grid {
        w: a.w,
        h: a.h,
        values: vec![0; a.data.len()],
        fixed: vec![true; a.data.len()],
    };
    for y in 1..a.h.saturating_sub(1) {
        for x in 1..a.w.saturating_sub(1) {
            let i = y * a.w + x;
            let coverage = a.data[i];
            grid.fixed[i] = coverage == 0;
            grid.values[i] = (i32::from(coverage) * 257) << 13;
        }
    }
    let scale = 2.5 * radius as f64;
    let source = if scale < 1.4143 {
        1.0
    } else {
        2.0 / (scale * scale)
    };
    grid.solve(source);
    for (pixel, value) in a.data.iter_mut().zip(grid.values) {
        *pixel = (value >> 21) as u8;
    }
}

impl Grid {
    fn coarse(&self) -> Self {
        let pairs_x = self.w / 2;
        let pairs_y = self.h / 2;
        let padded = |n: usize| if n > 1 { n.next_multiple_of(2) } else { n };
        let (w, h) = (padded(pairs_x), padded(pairs_y));
        let mut coarse = Self {
            w,
            h,
            fixed: vec![false; w * h],
            values: vec![0; w * h],
        };
        for y in 0..h {
            let row = 2 * y.min(pairs_y - 1) * self.w;
            for x in 0..w {
                let i = row + 2 * x.min(pairs_x - 1);
                let mut sum = 0u32;
                let mut count = 0;
                for j in [i, i + 1, i + self.w, i + self.w + 1] {
                    if self.fixed[j] {
                        sum += self.values[j] as u32;
                        count += 1;
                    }
                }
                #[allow(clippy::manual_checked_ops, reason = "the count guard protects the rounded division and the fixed flag")]
                if count > 0 {
                    coarse.fixed[y * w + x] = true;
                    coarse.values[y * w + x] = ((sum + count / 2) / count) as i32;
                }
            }
        }
        coarse
    }

    fn solve(&mut self, source: f64) {
        if self.w > 1 && self.h > 1 && source.abs() < 1.0 {
            let mut coarse = self.coarse();
            if coarse.fixed.iter().any(|fixed| !fixed) {
                coarse.solve(source * 4.0);
                for (i, (value, fixed)) in self.values.iter_mut().zip(&self.fixed).enumerate() {
                    if !fixed {
                        let x = (i % self.w / 2).min(coarse.w - 1);
                        let y = (i / self.w / 2).min(coarse.h - 1);
                        *value = coarse.values[y * coarse.w + x];
                    }
                }
            }
        }
        self.relax(source);
    }

    fn relax(&mut self, source: f64) {
        let source = (source * f64::from(FULL)).floor() as i64 as i32;
        let len = self.values.len();
        for pass in 0..7 {
            for i in 0..len {
                let i = if pass % 2 == 0 { i } else { len - 1 - i };
                if self.fixed[i] {
                    continue;
                }
                let x = i % self.w;
                let y = i / self.w;
                let north = y.saturating_sub(1) * self.w + x;
                let south = (y + 1).min(self.h - 1) * self.w + x;
                let west = y * self.w + x.saturating_sub(1);
                let east = y * self.w + (x + 1).min(self.w - 1);
                let sum = self.values[north] as u32
                    + self.values[south] as u32
                    + self.values[west] as u32
                    + self.values[east] as u32;
                let estimate = ((sum >> 2) as i32).wrapping_add(source);
                let delta = estimate.wrapping_sub(self.values[i]) >> 1;
                self.values[i] = estimate.wrapping_add(delta).clamp(0, FULL);
            }
        }
    }
}
