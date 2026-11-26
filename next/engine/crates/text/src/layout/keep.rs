//! Keep groups and forward-only column retries.
use super::*;

type Key = (usize, usize);
type Area = (usize, usize);

#[derive(Default)]
pub(super) struct State {
    pub active: bool,
    pub counts: BTreeMap<usize, usize>,
    pub placed: Vec<(Key, Area, bool)>,
    pub targets: BTreeMap<Key, Area>,
    pub offset: usize,
    tables: std::collections::BTreeSet<Key>,
    /// The areas of paragraphs that span the columns (`spancols::begin` adds one such column per frame): they cover every
    /// column of their frame, so they do not break a group inside the frame, and the area after one is the next frame.
    pub spans: std::collections::BTreeSet<Key>,
    /// The sub-column of a line that stands in a parcel of a split paragraph (`Flow::keep_sub`; 0 outside such parcels).
    /// Two lines of a keep group in different sub-columns of one column are in different columns for the group.
    pub subs: BTreeMap<Key, i64>,
    /// Split paragraphs whose lines are all joined by their keep options: InDesign sets them in one column, not split.
    pub unsplit: std::collections::BTreeSet<usize>,
}

/// What a keep pass asks of the next attempt.
pub(super) enum Retry {
    /// Start the group whose first line is the key in the area (first line of the next column).
    Move(Key, Area),
    /// Set the split paragraph in one column.
    Unsplit(usize),
}

impl State {
    pub(super) fn table(&mut self, pi: usize, rows: usize) -> usize {
        let start = self.counts.get(&pi).copied().unwrap_or(0);
        if self.active {
            self.counts.insert(pi, start + rows);
            self.tables.extend((start..start + rows).map(|li| (pi, li)));
        }
        start
    }

    fn retry(&self, paragraphs: &[Paragraph], columns: &[usize]) -> Option<Retry> {
        // A split paragraph whose keep options join every one of its lines cannot break between its sub-columns: it is set
        // in one column (keepspan3: the keep options of a split paragraph).
        for (pi, p) in paragraphs.iter().enumerate() {
            let a = &p.attrs;
            if a.columns.layout != folio_model::paragraph_columns::ColumnLayout::Split || !a.keep_lines_together || self.unsplit.contains(&pi) { continue; }
            let count = self.counts.get(&pi).copied().unwrap_or(1);
            if (1..count).all(|li| a.keep_all_lines || li < a.keep_first_lines as usize || li > count.saturating_sub(a.keep_last_lines as usize)) {
                return Some(Retry::Unsplit(pi));
            }
        }
        let mut rows = Vec::new();
        let mut joined = Vec::new();
        let mut spans = Vec::new();
        for (pi, p) in paragraphs.iter().enumerate() {
            let start = rows.len();
            let count = self.counts.get(&pi).copied().unwrap_or(1);
            rows.extend((0..count).map(|li| (pi, li)));
            joined.resize(rows.len(), false);
            let a = &p.attrs;
            if a.keep_lines_together {
                for li in 1..count {
                    joined[start + li] = a.keep_all_lines || li < a.keep_first_lines as usize
                        || li > count.saturating_sub(a.keep_last_lines as usize);
                }
            }
            spans.push((start, count));
        }
        for (pi, p) in paragraphs.iter().enumerate() {
            let (start, count) = spans[pi];
            let end = (start + count + p.attrs.keep_with_next as usize).min(rows.len());
            for i in start + count..end {
                joined[i] = true;
                if self.tables.contains(&rows[i]) { break; }
            }
        }
        // An explicit paragraph start breaks a keep chain.
        for (pi, p) in paragraphs.iter().enumerate() {
            if p.attrs.start != folio_model::ParagraphStart::Anywhere {
                joined[spans[pi].0] = false;
            }
        }
        let positions: BTreeMap<_, _> = self.placed.iter().map(|&(key, area, empty)| (key, (area, empty))).collect();
        let mut start = 0;
        while start < rows.len() {
            let mut end = start + 1;
            while end < rows.len() && joined[end] { end += 1; }
            if end > start + 1 {
                if let Some(&(area, false)) = positions.get(&rows[start]) {
                    // The group stands in one column when its lines do: a line of a paragraph that spans the columns fits any
                    // column of its frame, so the other lines are compared with the first one that is not (keepspan3-mid-next2: a
                    // heading's last line and the two lines after it), and lines in different sub-columns of a split paragraph are
                    // in different columns (keepspan3-lead-next2).
                    let at_of = |key: &Key| positions.get(key).map(|&(at, _)| (at, self.subs.get(key).copied().unwrap_or(0)));
                    let reference = rows[start..end].iter().filter_map(at_of).find(|(at, _)| !self.spans.contains(at)).map_or(area, |(at, _)| at);
                    let mut subs = rows[start..end].iter().filter_map(at_of).filter(|(at, sub)| !self.spans.contains(at) && *sub != 0).map(|(_, sub)| sub).collect::<Vec<_>>();
                    subs.sort_unstable();
                    subs.dedup();
                    let split = subs.len() > 1 || rows[start..end].iter().any(|key| match at_of(key) {
                        None => true,
                        Some((at, _)) if self.spans.contains(&at) => at.0 != reference.0,
                        Some((at, _)) => at != reference,
                    });
                    if split {
                        let next = if !self.spans.contains(&area) && area.1 + 1 < columns[area.0] { (area.0, area.1 + 1) }
                            else { ((area.0 + 1..columns.len()).find(|&fi| columns[fi] > 0).unwrap_or(columns.len()), 0) };
                        return Some(Retry::Move(rows[start], next));
                    }
                }
            }
            start = end;
        }
        None
    }
}

impl Ctx<'_, '_> {
    pub(super) fn lay_kept_text(&self, flow: &mut Flow<'_>, paragraphs: &[Paragraph], st: &mut TextState) -> bool {
        let saved = flow.checkpoint();
        let initial = st.clone();
        let columns: Vec<_> = flow.frames.iter().map(|f| f.cols.len()).collect();
        flow.keep.active = true;
        // Every retry advances a group's first line to a later area. A group starting
        // in an empty area may split; even a group taller than every frame terminates.
        let rows = paragraphs.iter().map(|p| 1 + p.runs.iter().map(|r| {
            r.text.chars().count() + r.tables.iter().map(|t| t.rows.len()).sum::<usize>()
        }).sum::<usize>()).sum::<usize>();
        let limit = rows.saturating_mul(columns.iter().sum::<usize>() + 1);
        for _ in 0..=limit {
            flow.keep.counts.clear();
            flow.keep.tables.clear();
            flow.keep.spans.clear();
            let fits = self.lay_text_once(flow, paragraphs, st, None);
            match flow.keep.retry(paragraphs, &columns) {
                None => return fits,
                Some(Retry::Unsplit(pi)) => { flow.keep.unsplit.insert(pi); }
                Some(Retry::Move(key, target)) => {
                    if flow.keep.targets.get(&key).is_some_and(|old| *old >= target) { return fits; }
                    flow.keep.targets.insert(key, target);
                }
            }
            flow.restore(&saved);
            *st = initial.clone();
        }
        false
    }
}

impl Flow<'_> {
    pub(super) fn keep_target(&mut self, key: Key) -> bool {
        if let Some(&target) = self.keep.targets.get(&key) {
            while self.keep_area(self.fi, self.ci) < target {
                if !self.advance() { return false; }
            }
        }
        true
    }
}
