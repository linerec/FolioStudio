//! Compose line breaks into rows, including both sides of a text wrap.

use super::*;

pub(super) struct ComposedRows {
    pub(super) count: usize,
    pub(super) parts: Vec<(usize, usize, Part)>,
}

impl Prepared {
    pub(super) fn compose_rows(
        &self,
        pos: usize,
        settings: &Settings,
        single_line: bool,
        saved: Option<&[usize]>,
        pin_items: &[usize],
        measure_with: impl Fn(usize, Option<f64>) -> folio_compose::compose::Slots,
    ) -> ComposedRows {
        let n = self.items.len();
        let measure = |k: usize| measure_with(k, None).w[0];
        let (rel, rel_splits): (Vec<usize>, Vec<Vec<usize>>) = if let Some(forced_breaks) = saved {
            (forced_breaks.to_vec(), Vec::new())
        } else if pos >= n {
            (Vec::new(), Vec::new())
        } else if single_line {
            (folio_compose::compose::first_fit(&self.items[pos..], settings, &measure), Vec::new())
        } else {
            // Lines pinned by nested line styles are kept; the rest of the paragraph is composed from the last one.
            let pinned: Vec<usize> = pin_items.iter().copied().filter(|&b| b > pos && b < n).collect();
            match pinned.last().copied() {
                None => folio_compose::compose::compose_slots(&self.items[pos..], settings, &|k, unit| measure_with(k, Some(unit))),
                Some(last) => {
                    let shift = pinned.len();
                    let (tail, tail_splits) = folio_compose::compose::compose_slots(&self.items[last..], settings, &|k, unit| measure_with(k + shift, Some(unit)));
                    let rel = pinned.iter().map(|b| b - pos).chain(tail.iter().map(|b| b + last - pos)).collect();
                    let splits = (0..shift).map(|_| Vec::new()).chain(tail_splits.into_iter().map(|v| v.into_iter().map(|x| x + last - pos).collect())).collect();
                    (rel, splits)
                }
            }
        };
        let mut bounds: Vec<usize> = vec![pos];
        bounds.extend(rel.iter().map(|b| b + pos));
        bounds.push(n);
        bounds.dedup();
        if bounds.len() == 1 {
            bounds.push(n); // empty paragraph: one empty line
        }
        let rows = bounds.len() - 1;
        // A row split by a both-sides wrap is set as its left part, then its right part
        // (the composer divided its text between the two slots, rule R2).
        let mut row_split: HashMap<usize, usize> = HashMap::new();
        let mut at = pos;
        for (k, sp) in rel_splits.iter().enumerate() {
            if let Some(&p) = sp.first() {
                row_split.insert(at, p + pos);
            }
            at = rel.get(k).map_or(n, |b| b + pos);
        }
        let mut parts: Vec<(usize, usize, Part)> = Vec::new();
        for w in bounds.windows(2) {
            match row_split.get(&w[0]) {
                Some(&p) if p > w[0] && p < w[1] => {
                    parts.push((w[0], p, Part::Left));
                    parts.push((p, w[1], Part::Right));
                }
                _ => parts.push((w[0], w[1], Part::Whole)),
            }
        }
        ComposedRows { count: rows, parts }
    }
}

#[derive(PartialEq, Eq, Hash)]
struct FirstBand {
    frame: usize,
    column: usize,
    top_bits: u64,
    height_bits: u64,
    required_width_10000ths: i64,
}

/// Column and wrap widths available to the composer during one attempt at a paragraph.
pub(super) struct RowWidths<'r, 'f> {
    rooms: &'r [Room],
    ahead: Vec<Ahead>,
    geometry: Vec<(Option<&'f Shape>, Vec<Column>)>,
    first_widths: std::cell::RefCell<HashMap<FirstBand, Option<f64>>>,
    attrs: &'r ParagraphAttrs,
    default_room: f64,
}

impl<'r, 'f> RowWidths<'r, 'f> {
    pub(super) fn new(flow: &Flow<'f>, rooms: &'r [Room], attrs: &'r ParagraphAttrs, default_room: f64) -> Self {
        Self {
            rooms,
            ahead: rooms_ahead(flow, rooms),
            geometry: flow.frames.iter().map(|f| (f.shape, f.cols.clone())).collect(),
            first_widths: Default::default(),
            attrs,
            default_room,
        }
    }

    fn first_width(&self, fi: usize, ci: usize, top: f64, height: f64, need: f64) -> Option<f64> {
        let key = FirstBand {
            frame: fi,
            column: ci,
            top_bits: top.to_bits(),
            height_bits: height.to_bits(),
            required_width_10000ths: (need * 1e4).round() as i64,
        };
        *self.first_widths.borrow_mut().entry(key)
            .or_insert_with(|| first_room(&self.geometry, fi, ci, top, height, need))
    }

    pub(super) fn column_start(&self, k: usize) -> bool {
        match self.rooms.get(k) {
            Some(room) => room.column_start,
            None => self.ahead.get(k - self.rooms.len()).is_some_and(|room| room.first.is_some()),
        }
    }

    pub(super) fn room(&self, k: usize, unit: Option<f64>, extra: f64) -> (f64, Option<f64>) {
        let a = self.attrs;
        let need = unit.map(|u| u + a.left_indent + a.right_indent + extra + if k == 0 { a.first_line_indent } else { 0.0 });
        if let Some(r) = self.rooms.get(k) {
            if let (Some(need), Some((fi, ci, ..)), Some((top, height))) = (need, r.at, r.first)
                && let Some(w) = self.first_width(fi, ci, top, height, need)
            {
                return (w, None);
            }
            let next = r.next.filter(|&w| unit.is_none_or(|u| w - a.left_indent - a.right_indent - extra >= u - TOL));
            return (r.width, next);
        }
        if let Some(x) = self.ahead.get(k - self.rooms.len()) {
            if let (Some(need), Some((fi, ci, top, height))) = (need, x.first)
                && let Some(w) = self.first_width(fi, ci, top, height, need)
            {
                return (w, None);
            }
            return (x.width, None);
        }
        (self.rooms.last().map(|r| r.width).unwrap_or(self.default_room), None)
    }

    pub(super) fn slots(&self, row: usize, unit: Option<f64>, extra: f64) -> folio_compose::compose::Slots {
        let a = self.attrs;
        let first = if row == 0 { a.first_line_indent } else { 0.0 };
        let (w, next) = self.room(row, unit, extra);
        let w = (w - a.left_indent - a.right_indent - first - extra).max(1.0);
        let slots = match next {
            Some(nw) => folio_compose::compose::Slots::of(&[w, (nw - a.left_indent - a.right_indent - extra).max(1.0)]),
            None => folio_compose::compose::Slots::one(w),
        };
        folio_compose::compose::Slots { column_start: self.column_start(row), ..slots }
    }
}
