//! Tables of an RTF file (`\trowd`, `\cellx`, `\cell`, `\row`) the way InDesign 2025 brings them in (oracle `docxtext-c5-rtf-w*`; measurements
//! `artifacts/docxtext-c5-20261023/measurement/w*.idml`, rule `next/oracle/rules/docxtext-c5.ko.md`). What was measured:
//! - The grid is the union of the cell edges of all rows (`\cellx`, counted from `\trleft`): a cell spans the columns between its edges. Rows of
//!   different grids make cells that span columns. `\trgaph` and `\trqc` change nothing: the cells have no inset (the table itself keeps 4 pt).
//! - `\trhdr` marks a header row; the table has as many header rows (at the top) as there are such rows. `\trrh` (at least) is the row's minimum height.
//! - A merged cell is a vertical one (`\clvmgf` begins, `\clvmrg` continues: a row span). A horizontal merge (`\clmgf`, `\clmrg`) merges nothing.
//! - Fill: `\clcbpat` is the fill colour at 100 %; with `\clshdng` and `\clcfpat` it is the foreground colour at that shading's percent (the
//!   background colour is not used). The cells under a shaded cell get its tint (the same as a Word table's).
//! - `\clvertalc`, `\clvertalb` set the vertical alignment of the cell's text.
//! - A table is a paragraph of its own: the character takes the formatting of the character before it; the return after it, the formatting at
//!   the end of the last row.
//! - A cell with one empty paragraph has no settings at all.
//! - Cell borders (`\clbrdrt`, `\clbrdrl`, `\clbrdrb`, `\clbrdrr` with `\brdrs`, `\brdrdb` … `\brdrw` in twips, `\brdrcf` a colour of the table) are
//!   written on the cells the way a Word file's cell borders are (`cell_edges`: the edge priorities and the strokes that go down a column;
//!   measured `screens-rtf-border-a .. c`). The borders of the table itself (`\trbrdrt`, `l`, `b`, `r`, `h`, `v`) go to the sides a cell
//!   does not say: the outer ones on the table's edge, `h` and `v` between rows and columns (build-a; not compared with InDesign).
use crate::messages::Message;
use super::CharState;
use crate::cell_edges::{BOTTOM, LEFT, RIGHT, Sides, Stroke, TOP};
use crate::{Cell, Paragraph, Table};

#[derive(Clone, Copy, PartialEq, Default)]
pub(super) enum Merge {
    #[default]
    None,
    First,
    Continue,
}

#[derive(Clone, Default)]
pub(super) struct CellDef {
    pub right: i32,
    pub merge: Merge,
    pub back: Option<usize>,
    pub fore: Option<usize>,
    pub shading: Option<i32>,
    pub valign: &'static str,
    /// The borders of the cell (`\clbrdr…`), and the side whose keywords are being read.
    pub borders: Sides,
    pub side: Option<usize>,
    /// The sides the cell says (a side it does not say takes the table's border).
    pub said: [bool; 4],
}

/// The table's own borders (`\trbrdrl`, `t`, `r`, `b`, then `h` between rows and `v` between columns).
pub(super) type TableSides = [Option<Stroke>; 6];
const INSIDE_H: usize = 4;
const INSIDE_V: usize = 5;

#[derive(Clone, Default)]
pub(super) struct RowDef {
    pub left: i32,
    pub cells: Vec<CellDef>,
    pub header: bool,
    pub height: Option<i32>,
    /// The cell definition being read (its keywords come before its `\cellx`).
    pub current: CellDef,
    /// The row's table borders and the one whose keywords are being read.
    pub table: TableSides,
    pub table_side: Option<usize>,
}

/// The rows read so far, and the row and cell being read.
#[derive(Default)]
pub(super) struct Grid {
    pub rows: Vec<(RowDef, Vec<Vec<Paragraph>>)>,
    pub def: Option<RowDef>,
    /// The paragraphs of the cell being read, and the cells of the row so far.
    pub cell: Vec<Paragraph>,
    pub cells: Vec<Vec<Paragraph>>,
    /// The formatting of the end of the last row: the return after the table.
    pub end: Option<CharState>,
    /// What the table says that is not carried over.
    pub notes: Vec<Message>,
    /// The keywords being read belong to the border of the table itself (`\trbrdr…`), not to a cell.
    pub table_border: bool,
}

const SOLID: &str = "StrokeStyle/$ID/Solid";

/// Twips as points, the way a stroke weight is written (`20` is `1`, `10` is `0.5`).
fn points(twips: i32) -> String {
    let v = f64::from(twips.max(0)) / 20.0;
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

impl Grid {
    pub fn row_start(&mut self) {
        self.def = Some(RowDef::default());
        self.cell.clear();
        self.cells.clear();
    }

    pub fn in_row(&self) -> bool {
        self.def.is_some()
    }

    pub fn row_end(&mut self) {
        let texts = std::mem::take(&mut self.cells);
        self.cell.clear();
        if let Some(def) = self.def.take()
            && !def.cells.is_empty() {
            self.rows.push((def, texts));
        }
    }

    /// A keyword of a row or cell definition; says whether it was one.
    pub fn word(&mut self, name: &str, p: Option<i32>) -> bool {
        let Some(def) = self.def.as_mut() else { return false };
        match name {
            "trleft" => def.left = p.unwrap_or(0),
            "trhdr" => def.header = p != Some(0),
            "trrh" => def.height = p.filter(|h| *h > 0),
            "clvmgf" => def.current.merge = Merge::First,
            "clvmrg" => def.current.merge = Merge::Continue,
            "clcbpat" => def.current.back = p.map(|v| v.max(0) as usize),
            "clcfpat" => def.current.fore = p.map(|v| v.max(0) as usize),
            "clshdng" => def.current.shading = p,
            "clvertalt" => def.current.valign = "",
            "clvertalc" => def.current.valign = "CenterAlign",
            "clvertalb" => def.current.valign = "BottomAlign",
            "clbrdrt" | "clbrdrl" | "clbrdrb" | "clbrdrr" => {
                let side = match name {
                    "clbrdrt" => TOP,
                    "clbrdrl" => LEFT,
                    "clbrdrb" => BOTTOM,
                    _ => RIGHT,
                };
                def.current.side = Some(side);
                def.current.said[side] = true;
                def.table_side = None;
                self.table_border = false;
                // a side without keywords is a solid line of no weight in black (as a Word cell border with no size)
                def.current.borders[side] = Some(Stroke { weight: "0".into(), color: Some("Color/Black".into()), kind: SOLID.into() });
            }
            "trbrdrt" | "trbrdrb" | "trbrdrl" | "trbrdrr" | "trbrdrh" | "trbrdrv" => {
                // the borders of the table itself: the keywords that follow are not a cell's; a cell that does not say a side takes them
                def.current.side = None;
                self.table_border = true;
                let side = match name {
                    "trbrdrl" => LEFT,
                    "trbrdrt" => TOP,
                    "trbrdrr" => RIGHT,
                    "trbrdrb" => BOTTOM,
                    "trbrdrh" => INSIDE_H,
                    _ => INSIDE_V,
                };
                def.table_side = Some(side);
                def.table[side] = Some(Stroke { weight: "0".into(), color: Some("Color/Black".into()), kind: SOLID.into() });
            }
            "brdrnone" => {
                if let Some(side) = def.current.side {
                    def.current.borders[side] = None;
                } else if let Some(side) = def.table_side.filter(|_| self.table_border) {
                    def.table[side] = None;
                }
            }
            "brdrw" | "brdrcf" | "brdrs" | "brdrth" | "brdrdb" | "brdrtriple" | "brdrdot" | "brdrdash" | "brdrdashsm" | "brdrdashd" | "brdrdashdd" | "brdrwavy" | "brdrthtnsg" => {
                let stroke = match (def.current.side, def.table_side.filter(|_| self.table_border)) {
                    (Some(s), _) => def.current.borders[s].as_mut(),
                    (None, Some(s)) => def.table[s].as_mut(),
                    _ => None,
                };
                if let Some(stroke) = stroke {
                    match name {
                        "brdrw" => stroke.weight = points(p.unwrap_or(0)),
                        "brdrcf" => stroke.color = Some(match p.unwrap_or(0) {
                            0 => "Color/Black".to_owned(),
                            index => format!("hex:{index}"),
                        }),
                        "brdrs" | "brdrth" => stroke.kind = SOLID.into(),
                        "brdrdb" => stroke.kind = "StrokeStyle/$ID/ThinThin".into(),
                        "brdrtriple" => stroke.kind = "StrokeStyle/$ID/Triple_Stroke".into(),
                        "brdrdot" => stroke.kind = "StrokeStyle/$ID/Canned Dotted".into(),
                        "brdrwavy" => stroke.kind = "StrokeStyle/$ID/Wavy".into(),
                        "brdrthtnsg" => stroke.kind = "StrokeStyle/$ID/ThinThick".into(),
                        _ => stroke.kind = "StrokeStyle/$ID/Canned Dashed 3x2".into(),
                    }
                }
            }
            "cellx" => {
                self.table_border = false;
                def.table_side = None;
                def.current.right = p.unwrap_or(0);
                let done = std::mem::take(&mut def.current);
                def.cells.push(done);
            }
            // horizontal merges merge nothing (measured); the rest changes nothing
            "clmgf" | "clmrg" | "trgaph" | "trqc" | "trql" | "trqr" | "trautofit" | "trkeep" | "trkeepfollow" | "trwWidth" | "trftsWidth" | "trwWidthB" | "trftsWidthB" | "trwWidthA"
            | "trftsWidthA" | "trpaddl" | "trpaddr" | "trpaddt" | "trpaddb" | "trpaddfl" | "trpaddfr" | "trpaddft" | "trpaddfb" | "trspdl" | "trspdr" | "trspdt" | "trspdb" | "trspdfl"
            | "trspdfr" | "trspdft" | "trspdfb" | "clwWidth" | "clftsWidth" | "clpadl" | "clpadr" | "clpadt" | "clpadb" | "clpadfl" | "clpadfr" | "clpadft" | "clpadfb" | "clNoWrap"
            | "irow" | "irowband" | "lastrow" | "tblind" | "tblindtype" | "tbllkborder" | "tbllkshading" | "tbllkfont" | "tbllkcolor" | "tbllkbestfit" | "tbllkhdrrows"
            | "tbllklastrow" | "tbllkhdrcols" | "tbllklastcol" | "tbllknorowband" | "tbllknocolband" => {}
            _ => return false,
        }
        true
    }
}

/// Builds the table from its rows. `fill` turns a colour table index into a swatch name (`None`: auto, no colour).
pub(super) fn build(rows: Vec<(RowDef, Vec<Vec<Paragraph>>)>, mut fill: impl FnMut(usize) -> Option<String>, empty_cell: impl Fn() -> Paragraph) -> Table {
    let mut edges: Vec<i32> = rows.iter().flat_map(|(d, _)| std::iter::once(d.left).chain(d.cells.iter().map(|c| c.right))).collect();
    edges.sort_unstable();
    edges.dedup();
    let columns: Vec<f64> = edges.windows(2).map(|w| f64::from(w[1] - w[0]) / 20.0).collect();
    let mut table = Table { columns, rows: rows.len(), zero_insets: true, ..Table::default() };
    table.header_rows = rows.iter().filter(|(d, _)| d.header).count().min(rows.len());
    table.row_heights = rows.iter().map(|(d, _)| d.height.map_or(0.0, |h| f64::from(h) / 20.0)).collect();
    if table.row_heights.iter().all(|h| *h == 0.0) {
        table.row_heights.clear();
    }
    table.attrs = [
        ("LeftBorderStrokeWeight", "0"),
        ("LeftBorderStrokeType", "n"),
        ("RightBorderStrokeWeight", "0"),
        ("RightBorderStrokeType", "n"),
        ("TopBorderStrokeWeight", "0"),
        ("TopBorderStrokeType", "n"),
        ("BottomBorderStrokeWeight", "0"),
        ("BottomBorderStrokeType", "n"),
        ("StartRowStrokeColor", "Color/Black"),
        ("StartRowStrokeCount", "1"),
        ("EndRowStrokeCount", "1"),
        ("StartRowStrokeType", "n"),
        ("EndRowStrokeType", "n"),
        ("StartColumnStrokeColor", "Color/Black"),
        ("StartColumnStrokeCount", "1"),
        ("EndColumnStrokeCount", "1"),
        ("StartColumnStrokeType", "n"),
        ("EndColumnLineStyle", "n"),
        ("StartRowStrokeWeight", "0"),
        ("EndRowStrokeWeight", "0"),
        ("StartColumnStrokeWeight", "0"),
        ("EndColumnStrokeWeight", "0"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    let at = |x: i32| edges.binary_search(&x).unwrap_or(0);
    // the cell above in each column that a vertical merge may continue
    let mut open: Vec<Option<usize>> = vec![None; table.columns.len()];
    let mut fills: Vec<Option<(String, u32)>> = Vec::new();
    let mut borders: Vec<Option<Sides>> = Vec::new();
    let mut own: Vec<(TableSides, [bool; 4])> = Vec::new();
    for (r, (def, texts)) in rows.into_iter().enumerate() {
        let mut left = def.left;
        let mut texts = texts.into_iter();
        for cd in &def.cells {
            let text = texts.next().unwrap_or_default();
            let column = at(left);
            let span = at(cd.right).saturating_sub(column).max(1);
            left = cd.right;
            if column + span > open.len() {
                open.resize(column + span, None);
            }
            if cd.merge == Merge::Continue
                && let Some(above) = open[column]
            {
                table.cells[above].row_span += 1;
                continue;
            }
            let mut paragraphs = text;
            // one empty paragraph is a cell with no settings
            if paragraphs.len() == 1 && paragraphs[0].runs.is_empty() {
                paragraphs = vec![empty_cell()];
            }
            let mut attrs: Vec<(String, String)> = [("LeftInset", "0"), ("TopInset", "0"), ("RightInset", "0"), ("BottomInset", "0")].iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
            attrs.push(("VerticalJustification".into(), if cd.valign.is_empty() { "TopAlign" } else { cd.valign }.to_owned()));
            table.cells.push(Cell { row: r, column, row_span: 1, column_span: span, paragraphs, attrs });
            let index = table.cells.len() - 1;
            for slot in &mut open[column..column + span] {
                *slot = (cd.merge == Merge::First).then_some(index);
            }
            // the fill: the foreground colour at the shading's percent, else the background colour at 100 %
            let f = match (cd.shading.filter(|s| *s > 0), cd.fore, cd.back) {
                (Some(s), Some(c), _) => fill(c).map(|n| (n, (s / 100).clamp(0, 100) as u32)),
                (_, _, Some(c)) => fill(c).map(|n| (n, 100)),
                _ => None,
            };
            fills.push(f);
            borders.push(Some(cd.borders.clone()));
            own.push((def.table.clone(), cd.said));
        }
    }
    let columns = table.columns.len();
    // a side the cell does not say takes the table's border: the outer one on the table's edge, the one between rows or columns inside
    for ((cell, sides), (tb, said)) in table.cells.iter().zip(&mut borders).zip(&own) {
        let Some(sides) = sides.as_mut() else { continue };
        for side in [LEFT, TOP, RIGHT, BOTTOM] {
            let from = match side {
                TOP if cell.row == 0 => TOP,
                BOTTOM if cell.row + cell.row_span >= table.rows => BOTTOM,
                LEFT if cell.column == 0 => LEFT,
                RIGHT if cell.column + cell.column_span >= columns => RIGHT,
                TOP | BOTTOM => INSIDE_H,
                _ => INSIDE_V,
            };
            if !said[side]
                && let Some(stroke) = &tb[from]
            {
                sides[side] = Some(stroke.clone());
            }
        }
    }
    crate::apply_fills(&mut table, &fills, columns);
    // the borders: a stroke's colour `hex:N` is the colour N of the colour table
    crate::cell_edges::apply(&mut table, &borders, columns, |index| index.parse::<usize>().ok().and_then(&mut fill));
    table
}
