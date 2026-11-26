use super::QrModules;

fn blocks(modules: &QrModules) -> usize {
    let size = modules.size;
    let mut penalty = 0;
    for y in 1..size {
        for x in 1..size {
            let color = modules.get(x, y);
            if modules.get(x - 1, y) == color
                && modules.get(x, y - 1) == color
                && modules.get(x - 1, y - 1) == color
            {
                penalty += 3;
            }
        }
    }
    penalty
}

fn collect_runs(line: impl Iterator<Item = bool>, runs: &mut Vec<(bool, usize)>) {
    runs.clear();
    for color in line {
        match runs.last_mut() {
            Some((last, length)) if *last == color => *length += 1,
            _ => runs.push((color, 1)),
        }
    }
}

// R6, R7
fn run_penalty(runs: &[(bool, usize)]) -> usize {
    let long_runs = runs
        .iter()
        .filter(|(_, length)| *length >= 5)
        .map(|(_, length)| length - 2)
        .sum::<usize>();
    let patterns = runs
        .windows(5)
        .enumerate()
        .filter(|(start, window)| {
            let (dark, width) = window[0];
            if !dark
                || window[1].1 != width
                || window[2].1 != 3 * width
                || window[3].1 != width
                || window[4].1 != width
            {
                return false;
            }
            let end = start + window.len();
            *start <= 1
                || end + 1 >= runs.len()
                || runs[start - 1].1 >= 4 * width
                || runs[end].1 >= 4 * width
        })
        .count();
    long_runs + 40 * patterns
}

fn line_and_balance_penalty(modules: &QrModules) -> usize {
    let size = modules.size;
    let mut runs = Vec::with_capacity(size);
    let mut penalty = 0;
    let mut dark = 0usize;
    for index in 0..size {
        collect_runs((0..size).map(|x| modules.get(x, index)), &mut runs);
        penalty += run_penalty(&runs);
        dark += runs[..runs.len() - 1]
            .iter()
            .filter(|(color, _)| *color)
            .map(|(_, length)| length)
            .sum::<usize>();
        collect_runs((0..size).map(|y| modules.get(index, y)), &mut runs);
        penalty += run_penalty(&runs);
    }
    let percent = dark / (size * size / 100);
    penalty + 10 * (percent.abs_diff(50) / 5)
}

// R7
pub(super) fn select(candidates: Vec<QrModules>) -> Option<QrModules> {
    let mut ranked: Vec<_> = candidates
        .into_iter()
        .enumerate()
        .map(|(id, candidate)| (blocks(&candidate), id, candidate))
        .collect();
    ranked.sort_unstable_by_key(|(blocks, id, _)| (*blocks, *id));
    ranked
        .into_iter()
        .take(3)
        .min_by_key(|(blocks, _, candidate)| blocks + line_and_balance_penalty(candidate))
        .map(|(_, _, candidate)| candidate)
}
