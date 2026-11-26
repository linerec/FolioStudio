use crate::{
    Item, Settings,
    breaks::opportunities,
    cost::{TOL, character_allowance, justified},
    measure::{Measures, natural_width, visible_length},
};

pub(super) fn unit_width(
    items: &[Item],
    start: usize,
    mut end: usize,
    s: &Settings,
    width: impl FnOnce(usize, usize) -> f64,
) -> f64 {
    if end <= start {
        return 0.0;
    }
    if end - start >= 2 && items[end - 1].ch == '\t' {
        end -= 1;
    }
    let natural = width(start, end);
    if s.alignment.is_justified() {
        natural * (1.0 - character_allowance(s, true))
    } else {
        natural
    }
}
pub fn first_unit(items: &[Item], start: usize, ops: &[usize], s: &Settings) -> f64 {
    let end = ops
        .get(ops.partition_point(|&p| p <= start))
        .copied()
        .unwrap_or(items.len())
        .min(items.len());
    unit_width(items, start, end, s, |a, b| {
        let line = &items[a..b];
        natural_width(&line[..visible_length(line)])
    })
}

pub fn first_fit(items: &[Item], s: &Settings, width: &dyn Fn(usize) -> f64) -> Vec<usize> {
    let ops = opportunities(items, s);
    let widths = Measures::new(items, s);
    let mut breaks = Vec::new();
    let mut start = 0;
    let mut next = 0;
    let mut forced = ops
        .iter()
        .copied()
        .filter(|&j| items[j - 1].ch == '\n')
        .peekable();
    while next < ops.len() {
        let available = width(breaks.len());
        if forced.peek().is_none() && widths.line(start, items.len()).natural <= available + TOL {
            break;
        }
        let mut chosen = next;
        for j in next..ops.len() {
            let metrics = widths.line(start, ops[j]);
            if j > next && metrics.natural > available + TOL {
                if s.alignment.is_justified()
                    && justified(&metrics, available, s).cost
                        <= justified(&widths.line(start, ops[chosen]), available, s).cost
                {
                    chosen = j;
                }
                break;
            }
            chosen = j;
            if items[ops[j] - 1].ch == '\n' {
                break;
            }
        }
        start = ops[chosen];
        if forced.peek() == Some(&start) {
            forced.next();
        }
        breaks.push(start);
        next = chosen + 1;
    }
    breaks
}
