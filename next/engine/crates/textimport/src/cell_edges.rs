//! The cell borders of a Word table (`w:tcBorders`) as InDesign 2025 makes them on import: which cell edges get a stroke and the
//! `…EdgeStrokePriority` numbers. The rules are measured, not designed (artifacts/docxtext-c3-20261023/measurement/e*, f*, h*, i*, j*, k*, m*;
//! the Python model with the same rules is tools/edge-sim.py; the summary is next/oracle/rules/docxtext-c3.ko.md).
//!
//! Edges are shared by two cells: `H(r, c)` is the edge above row `r` in column `c` (`r == rows` is the table bottom), `V(r, c)` the edge
//! left of column `c` in row `r`. A stroke written on a cell is written on the neighbour's opposite side too (the later cell wins).
use std::collections::HashMap;

pub const LEFT: usize = 0;
pub const TOP: usize = 1;
pub const RIGHT: usize = 2;
pub const BOTTOM: usize = 3;
const NORMAL: [usize; 4] = [LEFT, TOP, RIGHT, BOTTOM];
/// The order InDesign uses for the ops that reach the last cell of the table (it is the last thing it does).
const LAST_ROW: [usize; 4] = [TOP, BOTTOM, LEFT, RIGHT];

/// One border line as InDesign names it: the weight in points, the swatch (none: InDesign's own black), the stroke style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub weight: String,
    pub color: Option<String>,
    pub kind: String,
}

/// The four sides of a cell (left, top, right, bottom); `None` is a side the file does not say.
pub type Sides = [Option<Stroke>; 4];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub priority: u32,
    pub stroke: Stroke,
}

/// `(horizontal, i, j)`: `true` is `H(i, j)`, `false` is `V(i, j)`.
pub type EdgeKey = (bool, usize, usize);

fn key(r: usize, c: usize, side: usize) -> EdgeKey {
    match side {
        LEFT => (false, r, c),
        TOP => (true, r, c),
        RIGHT => (false, r, c + 1),
        _ => (true, r + 1, c),
    }
}

struct Sim<'a> {
    rows: usize,
    cols: usize,
    cells: &'a HashMap<(usize, usize), Sides>,
    edges: HashMap<EdgeKey, Edge>,
    n: u32,
    prev: usize,
    reached: bool,
}

impl Sim<'_> {
    fn has(&self, r: usize, c: usize) -> bool {
        self.cells.contains_key(&(r, c))
    }

    fn side(&self, r: usize, c: usize, side: usize) -> Option<&Stroke> {
        self.cells.get(&(r, c)).and_then(|s| s[side].as_ref())
    }

    /// The same stroke on this side of every cell from `c` to the last column of the row.
    fn reaches_last(&self, r: usize, c: usize, side: usize, st: &Stroke) -> bool {
        (c..self.cols).all(|c2| self.side(r, c2, side) == Some(st))
    }

    fn full(&self, r: usize) -> bool {
        (0..self.cols).all(|c| self.has(r, c) && self.cells[&(r, c)] == self.cells[&(r, 0)])
    }

    /// Every bordered cell of row `r2` (a run from the first column) is the same as the cell above it in row `r`.
    fn similar(&self, r: usize, r2: usize) -> bool {
        let cs: Vec<usize> = (0..self.cols).filter(|&c| self.has(r2, c)).collect();
        !cs.is_empty() && cs.iter().enumerate().all(|(i, &c)| i == c && self.cells.get(&(r, c)) == self.cells.get(&(r2, c)))
    }

    fn put(&mut self, k: EdgeKey, priority: u32, stroke: &Stroke) {
        self.edges.insert(k, Edge { priority, stroke: stroke.clone() });
    }

    /// The numbers InDesign spends before it makes an op in row `r` (`f`: the op's run reaches the last column).
    fn op_consume(&mut self, r: usize, f: bool) {
        if self.rows > 1 {
            let (pr, preached) = (self.prev, self.reached);
            if r > pr {
                let free = preached && r == pr + 1 && (!f || r == self.rows - 1);
                self.n += u32::from(!free);
            } else if f && !preached && r + 1 < self.rows {
                self.n += 1;
            }
        }
        self.prev = r;
        self.reached = f;
    }

    /// A bottom stroke of a cell that is not in the last column is copied down its column (not into the last row) over rows that have no
    /// borders of their own, unless the last cell of its row has borders.
    fn propagate(&mut self, r: usize, c: usize, priority: u32, st: &Stroke) {
        if c + 1 == self.cols || self.has(r, self.cols - 1) {
            return;
        }
        let mut q = r + 1;
        while q + 2 <= self.rows && !(0..self.cols).any(|c2| self.has(q, c2)) {
            self.put((true, q + 1, c), priority, st);
            q += 1;
        }
    }

    fn row_mode(&mut self, r: usize) {
        let cs: Vec<usize> = (0..self.cols).filter(|&c| self.has(r, c)).collect();
        let mut ops: [u32; 4] = [0; 4];
        let mut prevc: Option<usize> = None;
        for c in cs {
            let sides = self.cells[&(r, c)].clone();
            let present: Vec<usize> = NORMAL.iter().copied().filter(|&s| sides[s].is_some()).collect();
            let order: Vec<usize> = if r + 1 == self.rows {
                let reach = |s: usize| self.reaches_last(r, c, s, sides[s].as_ref().expect("the side was selected by is_some without changing the cells"));
                let mut o: Vec<usize> = NORMAL.iter().copied().filter(|&s| present.contains(&s) && !reach(s)).collect();
                o.extend(LAST_ROW.iter().copied().filter(|&s| present.contains(&s) && reach(s)));
                o
            } else {
                present
            };
            for s in order {
                let st = sides[s].clone().expect("the side was selected by is_some without changing the cells");
                let merged = c > 0 && prevc == Some(c - 1) && {
                    let before = &self.cells[&(r, c - 1)];
                    if s == LEFT { before[RIGHT].as_ref() == Some(&st) } else { before[s].as_ref() == Some(&st) }
                };
                let priority = if merged {
                    ops[if s == LEFT { RIGHT } else { s }]
                } else {
                    let f = self.reaches_last(r, c, s, &st);
                    self.op_consume(r, f);
                    let p = self.n;
                    self.n += 1;
                    p
                };
                ops[s] = priority;
                self.put(key(r, c, s), priority, &st);
                if s == BOTTOM && !self.reaches_last(r, c, BOTTOM, &st) {
                    self.propagate(r, c, priority, &st);
                }
            }
            prevc = Some(c);
        }
    }

    /// Rows whose cells all have the same borders are done a side at a time: all but the last of a run of similar rows are one range, the last
    /// row of the run is another.
    fn block_mode(&mut self, blk: &[usize]) {
        let includes_last = *blk.last().expect("block_mode receives a nonempty run and splits it into nonempty groups") + 1 == self.rows && self.has(self.rows - 1, self.cols - 1);
        let groups: Vec<&[usize]> = if blk.len() > 1 { vec![&blk[..blk.len() - 1], &blk[blk.len() - 1..]] } else { vec![blk] };
        let order = if includes_last { LAST_ROW } else { NORMAL };
        let mut started = false;
        for s in order {
            for g in &groups {
                let cs: Vec<(usize, usize)> = g.iter().flat_map(|&r| (0..self.cols).map(move |c| (r, c))).filter(|&(r, c)| self.side(r, c, s).is_some()).collect();
                if cs.is_empty() {
                    continue;
                }
                if !started {
                    self.op_consume(g[0], true);
                    started = true;
                }
                let priority = self.n;
                self.n += 1;
                let last_row = *g.last().expect("block_mode receives a nonempty run and splits it into nonempty groups");
                self.prev = last_row;
                self.reached = cs.iter().any(|&(r, c)| r == last_row && c + 1 == self.cols);
                for (r, c) in cs {
                    let st = self.side(r, c, s).expect("the side was selected by is_some without changing the cells").clone();
                    self.put(key(r, c, s), priority, &st);
                    if s == BOTTOM && !self.reaches_last(r, c, BOTTOM, &st) {
                        self.propagate(r, c, priority, &st);
                    }
                }
            }
        }
    }
}

/// The edges InDesign gives a `rows` x `cols` table whose cells (no merged cells) have the borders `cells`.
pub fn simulate(rows: usize, cols: usize, cells: &HashMap<(usize, usize), Sides>) -> HashMap<EdgeKey, Edge> {
    let mut sim = Sim { rows, cols, cells, edges: HashMap::new(), n: 2, prev: 0, reached: false };
    let mut r = 0;
    while r < rows {
        if sim.full(r) {
            let mut blk = vec![r];
            while *blk.last().expect("the row block starts with r and only grows") + 1 < rows && sim.similar(*blk.last().expect("the row block starts with r and only grows"), *blk.last().expect("the row block starts with r and only grows") + 1) {
                blk.push(*blk.last().expect("the row block starts with r and only grows") + 1);
            }
            r = *blk.last().expect("the row block starts with r and only grows") + 1;
            sim.block_mode(&blk);
        } else {
            if (0..cols).any(|c| sim.has(r, c)) {
                sim.row_mode(r);
            }
            r += 1;
        }
    }
    sim.edges
}

/// Writes the cell edge attributes of `table` for the borders its cells say (`borders` goes with `table.cells`): a merged cell is taken as
/// the grid positions it covers (its left side is the left side of its first column, its right of its last, its top of its first row, its
/// bottom of its last, and no side lies inside it) and the edges are those of the plain table of those positions; a merged cell then
/// reads the edge at its first covered position along each side. The rules are measured for plain tables (a Word file: docxtext-c3, an
/// RTF file: screens-import `screens-rtf-border-a .. c`); InDesign's numbering of the edges of a merged cell was not measured.
/// A stroke's colour `hex:X` is a colour of the file: `name(X)` gives its swatch (called in the order InDesign made the strokes).
pub(crate) fn apply(table: &mut crate::Table, borders: &[Option<Sides>], columns: usize, mut name: impl FnMut(&str) -> Option<String>) {
    let mut cells: HashMap<(usize, usize), Sides> = HashMap::new();
    for (cell, sides) in table.cells.iter().zip(borders) {
        let Some(sides) = sides.as_ref().filter(|s| s.iter().any(Option::is_some)) else { continue };
        for row in cell.row..cell.row + cell.row_span {
            for column in cell.column..cell.column + cell.column_span {
                let mut part: Sides = [None, None, None, None];
                if column == cell.column {
                    part[LEFT] = sides[LEFT].clone();
                }
                if row == cell.row {
                    part[TOP] = sides[TOP].clone();
                }
                if column + 1 == cell.column + cell.column_span {
                    part[RIGHT] = sides[RIGHT].clone();
                }
                if row + 1 == cell.row + cell.row_span {
                    part[BOTTOM] = sides[BOTTOM].clone();
                }
                if part.iter().any(Option::is_some) {
                    cells.insert((row, column), part);
                }
            }
        }
    }
    let edges = simulate(table.rows, columns, &cells);
    // the swatches of the strokes that are left, in the order InDesign made the strokes
    let mut by_order: Vec<_> = edges.iter().collect();
    by_order.sort_by_key(|(k, e)| (e.priority, **k));
    let mut names: HashMap<String, String> = HashMap::new();
    for (_, edge) in by_order {
        if let Some(hex) = edge.stroke.color.as_deref().and_then(|c| c.strip_prefix("hex:"))
            && !names.contains_key(hex)
            && let Some(swatch) = name(hex)
        {
            names.insert(hex.to_owned(), swatch);
        }
    }
    for cell in &mut table.cells {
        for (side, label) in [(LEFT, "Left"), (TOP, "Top"), (RIGHT, "Right"), (BOTTOM, "Bottom")] {
            let key = match side {
                LEFT => (false, cell.row, cell.column),
                TOP => (true, cell.row, cell.column),
                RIGHT => (false, cell.row, cell.column + cell.column_span),
                _ => (true, cell.row + cell.row_span, cell.column),
            };
            let Some(edge) = edges.get(&key) else { continue };
            let mut put = |k: &str, v: &str| cell.attrs.push((format!("{label}EdgeStroke{k}"), v.to_owned()));
            put("Weight", &edge.stroke.weight);
            if let Some(color) = &edge.stroke.color {
                put("Color", color.strip_prefix("hex:").and_then(|h| names.get(h)).map_or(color.as_str(), String::as_str));
            }
            put("Type", &edge.stroke.kind);
            put("Tint", "100");
            put("Overprint", "false");
            put("Priority", &edge.priority.to_string());
            put("GapTint", "100");
            put("GapOverprint", "false");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Stroke {
        Stroke { weight: "1".into(), color: Some("Color/Word_R255_G0_B0".into()), kind: "StrokeStyle/$ID/Solid".into() }
    }

    fn one(sides: &[usize]) -> Sides {
        let mut s: Sides = [None, None, None, None];
        for &i in sides {
            s[i] = Some(red());
        }
        s
    }

    fn priorities(rows: usize, cols: usize, cells: &[((usize, usize), Sides)]) -> Vec<(EdgeKey, u32)> {
        let map: HashMap<_, _> = cells.iter().cloned().collect();
        let mut v: Vec<_> = simulate(rows, cols, &map).into_iter().map(|(k, e)| (k, e.priority)).collect();
        v.sort();
        v
    }

    #[test]
    fn one_bottom_stroke_is_copied_down_its_column() {
        // e04: the bottom of the first cell of a 3x3 table also gets the bottom of the second row, priority 2
        assert_eq!(priorities(3, 3, &[((0, 0), one(&[BOTTOM]))]), vec![((true, 1, 0), 2), ((true, 2, 0), 2)]);
        // the last column does not copy, and its priority is 3 (e16)
        assert_eq!(priorities(3, 3, &[((0, 2), one(&[BOTTOM]))]), vec![((true, 1, 2), 3)]);
    }

    #[test]
    fn the_sides_of_one_cell_take_the_next_numbers_in_order() {
        // i-series: a cell in the middle: left, top, right, bottom = 3, 4, 5, 6; the last cell: top, bottom, left, right
        let all = [LEFT, TOP, RIGHT, BOTTOM];
        let mid = priorities(3, 3, &[((1, 1), one(&all))]);
        assert_eq!(mid, vec![((false, 1, 1), 3), ((false, 1, 2), 5), ((true, 1, 1), 4), ((true, 2, 1), 6)]);
        let last = priorities(3, 3, &[((2, 2), one(&all))]);
        assert_eq!(last, vec![((false, 2, 2), 5), ((false, 2, 3), 6), ((true, 2, 2), 3), ((true, 3, 2), 4)]);
    }

    #[test]
    fn a_table_of_cells_with_all_four_sides_is_done_a_side_at_a_time() {
        // e55 (3x3): top 3, inner lines 5, bottom 6, left 7 (rows 1-2) and 8 (last row), right and inner verticals 9 and 10
        let all = [LEFT, TOP, RIGHT, BOTTOM];
        let cells: Vec<_> = (0..3).flat_map(|r| (0..3).map(move |c| ((r, c), one(&all)))).collect();
        let p: HashMap<_, _> = priorities(3, 3, &cells).into_iter().collect();
        assert_eq!((p[&(true, 0, 0)], p[&(true, 1, 1)], p[&(true, 2, 2)], p[&(true, 3, 0)]), (3, 5, 5, 6));
        assert_eq!((p[&(false, 0, 0)], p[&(false, 1, 0)], p[&(false, 2, 0)]), (7, 7, 8));
        assert_eq!((p[&(false, 0, 1)], p[&(false, 1, 3)], p[&(false, 2, 2)]), (9, 9, 10));
    }
}
