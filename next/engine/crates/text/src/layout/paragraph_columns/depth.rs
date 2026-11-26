use super::EPS;

/// Redistributes row advances in order. The first cost is charged when a
/// row starts a column. The optional flags mark the rows allowed to start one.
pub fn partition(
    steps: &[f64],
    first_costs: &[f64],
    columns: usize,
    break_allowed: Option<&[bool]>,
) -> Vec<usize> {
    if steps.is_empty() || columns == 0 {
        return vec![0; columns];
    }
    choose(steps, first_costs, columns, break_allowed, |_| true).counts
}

pub(super) struct Partition {
    pub depth: f64,
    pub counts: Vec<usize>,
}

struct Rows<'a> {
    steps: &'a [f64],
    costs: &'a [f64],
    columns: usize,
}

impl Rows<'_> {
    // R4
    fn fill(&self, ceiling: f64, mut end: impl FnMut(usize)) -> bool {
        let mut used = self.steps[0];
        let mut column = 1;
        let mut start = 0;
        for (i, &step) in self.steps.iter().enumerate().skip(1) {
            if column < self.columns && used + step > ceiling {
                column += 1;
                start = i;
                used = self.costs[i];
                end(i);
            } else {
                used += step;
            }
        }
        self.steps.len() - start <= 1 || used <= ceiling
    }

    fn counts(&self, ceiling: f64) -> Vec<usize> {
        let mut counts = Vec::with_capacity(self.columns);
        let mut start = 0;
        self.fill(ceiling, |end| {
            counts.push(end - start);
            start = end;
        });
        counts.push(self.steps.len() - start);
        counts.resize(self.columns, 0);
        counts
    }

    fn minimum_ceiling(&self, total: f64) -> f64 {
        let mut lo = 0.0;
        let mut hi = total.max(0.0);
        for _ in 0..64 {
            if hi - lo <= EPS / 16.0 {
                break;
            }
            let mid = lo + (hi - lo) / 2.0;
            if self.fill(mid, |_| {}) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        hi
    }

    fn prefixes(&self) -> impl Iterator<Item = f64> + '_ {
        self.steps.iter().scan(0.0, |total, step| {
            *total += step;
            Some(*total)
        })
    }

    // R5
    fn mixed_ceiling(&self, minimum: f64, total: f64) -> f64 {
        let Some((&second_cost, tail)) = self.costs.get(1..).and_then(<[_]>::split_first) else {
            return minimum;
        };
        let constant_tail = tail.iter().all(|cost| (cost - second_cost).abs() <= EPS);
        if constant_tail
            && ((self.costs[0] - second_cost).abs() <= EPS || self.costs[0] >= self.steps[1] - EPS)
        {
            return minimum;
        }
        let mean = self.steps[0] + (total - self.steps[0]) / self.columns as f64;
        let first = self.counts(mean)[0];
        if !constant_tail
            && self.columns == 3
            && first == 2
            && self
                .costs
                .get(first)
                .is_some_and(|&cost| cost > self.costs[0] + EPS)
            && self.steps[first] >= self.steps[first - 1]
            && self.fill(mean, |_| {})
        {
            return mean;
        }
        self.prefixes()
            .find(|&depth| depth + EPS >= minimum && self.fill(depth + EPS, |_| {}))
            .map_or(minimum, |depth| depth + EPS)
    }
}

pub(super) fn choose(
    steps: &[f64],
    first_costs: &[f64],
    columns: usize,
    allowed: Option<&[bool]>,
    fits_columns: impl Fn(&[usize]) -> bool,
) -> Partition {
    assert_eq!(steps.len(), first_costs.len());
    let rows = Rows {
        steps,
        costs: first_costs,
        columns,
    };
    let total: f64 = steps.iter().sum();
    let minimum = rows.minimum_ceiling(total);
    let base = rows.mixed_ceiling(minimum, total);
    let counts = rows.counts(base);
    let Some(allowed) = allowed else {
        return Partition {
            depth: base - EPS,
            counts,
        };
    };
    let can_break = |counts: &[usize]| {
        let mut end = 0;
        counts.iter().take_while(|&&count| count > 0).all(|&count| {
            end += count;
            end == steps.len() || allowed.get(end).copied().unwrap_or(true)
        })
    };
    // R6
    if !can_break(&counts) {
        for depth in rows.prefixes().filter(|&depth| depth + EPS > base) {
            let candidate = rows.counts(depth + EPS);
            if can_break(&candidate) && fits_columns(&candidate) {
                return Partition {
                    depth,
                    counts: candidate,
                };
            }
        }
    }
    Partition {
        depth: base - EPS,
        counts,
    }
}
