use super::{Paragraph, Slots, State};
use crate::{Alignment, Search};

#[derive(Clone, Copy)]
struct Path {
    prev: usize,
    total: f64,
    state: State,
    slots: Slots,
}
#[derive(Clone, Copy)]
struct Choice {
    prev: usize,
    total: f64,
    state: State,
    category: u8,
}

fn choose(
    p: &Paragraph,
    paths: &[Path],
    window: &mut usize,
    end: usize,
    justified: bool,
) -> Choice {
    let evaluate = |prev: usize| {
        let path = paths[prev];
        let (line, state) = p.score(prev, end, path.state, path.slots, justified);
        Choice {
            prev,
            total: path.total + line.cost,
            state,
            category: line.category,
        }
    };
    let mut best = evaluate(*window);
    for prev in *window + 1..end {
        let candidate = evaluate(prev);
        if candidate.total > best.total {
            if matches!(candidate.category, 1 | 2) {
                break;
            }
        } else {
            if best.prev == *window && best.category >= 4 {
                *window += 1;
            }
            best = candidate;
        }
    }
    best
}

pub(super) fn line_ends(p: &Paragraph, room: &dyn Fn(usize, f64) -> Slots) -> Vec<usize> {
    if p.settings.search == Search::Exhaustive {
        return exhaustive(p, room);
    }
    let mut paths = Vec::with_capacity(p.stops.len());
    paths.push(Path {
        prev: 0,
        total: 0.0,
        state: State::default(),
        slots: room(0, p.stops[0].unit),
    });
    let justified = p.settings.alignment.is_justified();
    let mut window = 0;
    for end in 1..=p.end() {
        let mut ragged_window = window;
        let mut best = choose(p, &paths, &mut window, end, justified);
        if justified && end == p.end() && p.settings.alignment != Alignment::FullyJustified {
            let ragged = choose(p, &paths, &mut ragged_window, end, false);
            if ragged.prev == best.prev || ragged.total <= best.total {
                best = ragged;
            }
        }
        let slots = if end < p.end() {
            room(best.state.row, p.stops[end].unit)
        } else {
            Slots::one(0.0)
        };
        let total = if p.stops[end].forced { 0.0 } else { best.total };
        paths.push(Path {
            prev: best.prev,
            total,
            state: best.state,
            slots,
        });
        if p.stops[end].forced {
            window = end;
        }
    }
    let mut end = p.end();
    let mut ends = Vec::new();
    while end != 0 {
        ends.push(end);
        end = paths[end].prev;
    }
    ends.reverse();
    ends
}

fn exhaustive(p: &Paragraph, room: &dyn Fn(usize, f64) -> Slots) -> Vec<usize> {
    use std::collections::BTreeMap;
    type Key = (usize, u8, u8);
    struct Entry {
        total: f64,
        state: State,
        prev: (usize, Key),
    }
    let initial = State::default();
    let mut paths = vec![BTreeMap::from([(
        (0, 0, 0),
        Entry {
            total: 0.0,
            state: initial,
            prev: (0, (0, 0, 0)),
        },
    )])];
    let mut segment_start = 0;
    for end in 1..=p.end() {
        let mut choices = BTreeMap::new();
        for (start, previous) in paths.iter().enumerate().skip(segment_start) {
            for (&key, path) in previous {
                let slots = room(path.state.row, p.stops[start].unit);
                let (line, state) = p.best_line_mode(start, end, path.state, slots, true);
                let total = path.total + line.cost;
                let new_key = (state.row, state.grade, state.history & 15);
                if choices
                    .get(&new_key)
                    .is_none_or(|old: &Entry| total < old.total)
                {
                    choices.insert(
                        new_key,
                        Entry {
                            total,
                            state,
                            prev: (start, key),
                        },
                    );
                }
            }
        }
        paths.push(choices);
        if p.stops[end].forced {
            segment_start = end;
        }
    }
    let mut end = p.end();
    let Some((&mut_key, _)) = paths[end]
        .iter()
        .min_by(|a, b| a.1.total.total_cmp(&b.1.total))
    else {
        return Vec::new();
    };
    let mut key = mut_key;
    let mut ends = Vec::new();
    while end > 0 {
        ends.push(end);
        (end, key) = paths[end][&key].prev;
    }
    ends.reverse();
    ends
}
