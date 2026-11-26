use super::Paragraph;
use crate::cost::LineCost;

pub const MAX_SLOTS: usize = 5;
#[derive(Clone, Copy, Debug)]
pub struct Slots {
    pub n: usize,
    pub w: [f64; MAX_SLOTS],
    pub column_start: bool,
}
impl Slots {
    pub fn one(width: f64) -> Self {
        Self {
            n: 1,
            w: [width, 0.0, 0.0, 0.0, 0.0],
            column_start: false,
        }
    }
    pub fn of(widths: &[f64]) -> Self {
        let mut slots = Self::one(0.0);
        let n = widths.len().min(MAX_SLOTS);
        slots.w[..n].copy_from_slice(&widths[..n]);
        slots.n = n.max(1);
        slots
    }
    pub(super) fn len(self) -> usize {
        self.n.clamp(1, MAX_SLOTS)
    }
}

pub(super) struct Plan {
    counts: [usize; MAX_SLOTS],
    pub score: LineCost,
}
impl Plan {
    pub fn splits(&self, p: &Paragraph, mut start: usize, n: usize) -> Vec<usize> {
        self.counts[..n - 1]
            .iter()
            .map(|&count| {
                start += count;
                p.stops[start].at
            })
            .collect()
    }
}
impl Paragraph<'_> {
    pub(super) fn slot_plan(
        &self,
        start: usize,
        end: usize,
        row: usize,
        slots: Slots,
        justified: bool,
    ) -> Plan {
        let n = slots.len();
        let segments = end - start;
        let mut counts = [0; MAX_SLOTS];
        for (i, count) in counts[..n].iter_mut().enumerate() {
            *count = (i + 1) * segments / n - i * segments / n;
        }
        let score = |counts: &[usize; MAX_SLOTS]| {
            let mut total = LineCost {
                cost: 0.0,
                category: 3,
            };
            let mut pos = start;
            let mut empty_width = None;
            for (&count, &width) in counts[..n].iter().zip(&slots.w) {
                if end == self.end() && !justified && pos == end {
                    break;
                }
                if count == 0 {
                    empty_width = Some(width);
                    if end != self.end() {
                        total.cost += 1333.0 * width;
                    }
                } else {
                    let line = self.base_cost(pos, pos + count, row, width, justified);
                    total.cost += line.cost + empty_width.take().unwrap_or(0.0) * 6665.0;
                    total.category = total.category.max(line.category);
                    pos += count;
                }
            }
            total
        };
        let mut best = Plan {
            counts,
            score: score(&counts),
        };
        for amount in (1..=4).rev() {
            loop {
                let initial = best.counts;
                for from in 0..n {
                    if initial[from] < amount {
                        continue;
                    }
                    for to in 0..n {
                        if from == to {
                            continue;
                        }
                        let mut trial = initial;
                        trial[from] -= amount;
                        trial[to] += amount;
                        let candidate = score(&trial);
                        if candidate.cost < best.score.cost {
                            best = Plan {
                                counts: trial,
                                score: candidate,
                            };
                        }
                    }
                }
                if best.counts == initial {
                    break;
                }
            }
        }
        best
    }
}
