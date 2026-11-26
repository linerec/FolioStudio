//! Converts Word tables, cell geometry and borders as the body is read.

use super::Walk;
use super::super::archive::MAX_PART;
use super::super::styles::{line_kind, num, number, rgb_name};
use crate::cell_edges::{self, Sides, Stroke};
use crate::messages::msg;
use crate::xmltree::El;
use crate::Cell;
use crate::Table;

impl Walk<'_> {
    /// A table: the grid, the merged cells, the header rows and the text of each cell. It goes in front of the paragraph that follows.
    pub(super) fn table(&mut self, t: &El) -> Result<(), String> {
        let mut grid: Vec<f64> = t.child("tblGrid").map(|g| g.elements().filter(|c| c.name == "gridCol").filter_map(|c| number(c, "w")).map(|v| v / 20.0).collect()).unwrap_or_default();
        let border_attrs = t.child("tblPr").and_then(|x| x.child("tblBorders")).map(|b| self.table_borders(b)).unwrap_or_default();
        let outer = std::mem::take(&mut self.pending);
        let mut table = Table { attrs: border_attrs, ..Table::default() };
        // the fill of each cell that starts a grid position: (colour id, tint)
        let mut fills: Vec<Option<(String, u32)>> = Vec::new();
        // the borders (`w:tcBorders`) of each cell that starts a grid position
        let mut borders: Vec<Option<Sides>> = Vec::new();
        // the cell above in each grid column that a vertical merge may continue
        let mut open: Vec<Option<usize>> = Vec::new();
        for (r, tr) in t.elements().filter(|e| e.name == "tr").enumerate() {
            let header = tr.child("trPr").is_some_and(|x| x.child("tblHeader").is_some());
            if header && table.header_rows == r {
                table.header_rows += 1;
            }
            let mut column = 0usize;
            for (cell, tc) in tr.elements().filter(|e| e.name == "tc").enumerate() {
                let pr = tc.child("tcPr");
                let location = || format!("word/document.xml/tbl/tr[{}]/tc[{}]/gridSpan", r + 1, cell + 1);
                let span = match pr.and_then(|x| x.child("gridSpan")).and_then(|x| x.attr("val")) {
                    Some(value) => value.parse::<usize>().ok().filter(|&v| v > 0)
                        .ok_or_else(|| format!("invalidGridSpan at {}: {value}", location()))?,
                    None => 1,
                };
                // Bound the expanded grid by the same memory budget as an XML part.
                let limit = MAX_PART as usize / std::mem::size_of::<Option<usize>>();
                let end = column.checked_add(span).filter(|&v| v <= limit / (r + 1))
                    .ok_or_else(|| format!("tableGridLimit at {}: {span}", location()))?;
                let fill = pr.and_then(|x| x.child("shd")).and_then(|s| self.cell_fill(s));
                let cell_borders = pr.and_then(|x| x.child("tcBorders")).map(|b| self.cell_borders(b));
                if open.len() < end {
                    open.try_reserve(end - open.len()).map_err(|e| format!("tableGridAllocation at {}: {e}", location()))?;
                    open.resize(end, None);
                }
                let merge = pr.and_then(|x| x.child("vMerge"));
                let continues = merge.is_some_and(|m| m.attr("val").is_none_or(|v| v == "continue"));
                if continues && let Some(above) = open[column] {
                    table.cells[above].row_span += 1;
                } else {
                    let saved = std::mem::take(&mut self.paragraphs);
                    for c in tc.elements() {
                        match c.name.as_str() {
                            "p" => self.paragraph(c),
                            "tbl" => self.notes.add(msg("textimport.nestedTable", "표 안의 표는 아직 가져오지 않습니다", serde_json::json!({}))),
                            _ => {}
                        }
                    }
                    let mut paragraphs = std::mem::replace(&mut self.paragraphs, saved);
                    if paragraphs.iter().all(|p| p.runs.is_empty()) {
                        for p in &mut paragraphs {
                            p.style = None;
                        }
                    }
                    table.cells.push(Cell { row: r, column, row_span: 1, column_span: span, paragraphs, attrs: Vec::new() });
                    fills.push(fill);
                    borders.push(cell_borders);
                    let index = table.cells.len() - 1;
                    for slot in &mut open[column..column + span] {
                        *slot = merge.map(|_| index);
                    }
                }
                column += span;
            }
            table.rows = r + 1;
            if grid.len() < column {
                grid.resize(column, 0.0);
            }
        }
        for w in &mut grid {
            if *w <= 0.0 {
                *w = 100.0;
            }
        }
        crate::apply_fills(&mut table, &fills, grid.len());
        self.cell_edges(&mut table, &borders, grid.len());
        table.columns = grid;
        self.pending = outer;
        if table.rows == 0 || table.columns.is_empty() {
            return Ok(());
        }
        self.pending.push(table);
        Ok(())
    }

    /// The borders one cell says (`w:tcBorders`): the four sides as InDesign's strokes (measured: docxtext3-e44 .. e50, m01 .. m20). `start` and `end`
    /// are not read (InDesign ignores them). A line that is not known comes in as a solid line.
    fn cell_borders(&mut self, b: &El) -> Sides {
        let mut sides: Sides = [None, None, None, None];
        for side in b.elements() {
            let index = match side.name.as_str() {
                "left" => cell_edges::LEFT,
                "top" => cell_edges::TOP,
                "right" => cell_edges::RIGHT,
                "bottom" => cell_edges::BOTTOM,
                _ => continue,
            };
            let val = side.attr("val").unwrap_or("single");
            let nil = val == "nil";
            let none = nil || val == "none";
            let kind = match line_kind(val) {
                Some(kind) => kind.unwrap_or("StrokeStyle/$ID/Solid"),
                None => {
                    self.notes.add(msg("textimport.cellBorder", format!("표 칸 테두리 모양 {val} 은(는) 아직 InDesign과 같게 가져오지 않습니다 (실선으로 들어갑니다)"), serde_json::json!({"val": val})));
                    "StrokeStyle/$ID/Solid"
                }
            };
            // measured: a missing size is 0 pt (a solid line of no weight); `nil` has no colour either
            let weight = if none { 0.0 } else { number(side, "sz").unwrap_or(0.0) / 8.0 };
            let color = match side.attr("color") {
                _ if nil => None,
                Some(c) if c.eq_ignore_ascii_case("auto") => Some("Color/Black".to_owned()),
                // the swatch is made when the edge survives (a stroke another cell draws over is never used: measured, docxtext3-e41)
                Some(c) => rgb_name(&c.to_ascii_uppercase()).map(|_| format!("hex:{}", c.to_ascii_uppercase())),
                None => None,
            };
            sides[index] = Some(Stroke { weight: num(weight), color, kind: kind.to_owned() });
        }
        sides
    }

    /// The cell edges of a table whose cells say borders. The rules are measured for a table without merged cells; a merged cell
    /// (screens-import) is taken as the grid positions it covers: its left side is the left side of its first column, its right side of its
    /// last, its top of its first row, its bottom of its last, and no side lies inside it. The edges are then those of the plain table
    /// of those positions. InDesign's own numbering of the edges of a merged cell was not measured: the strokes are right, the
    /// priority numbers that decide which of two touching strokes wins may differ (said in the notes).
    fn cell_edges(&mut self, table: &mut Table, borders: &[Option<Sides>], columns: usize) {
        if borders.iter().all(|b| b.as_ref().is_none_or(|s| s.iter().all(Option::is_none))) {
            return;
        }
        let plain = table.cells.len() == table.rows * columns && table.cells.iter().all(|c| c.row_span == 1 && c.column_span == 1);
        if !plain {
            self.notes.add(msg("textimport.mergedBorders", "병합된 칸이 있는 표의 칸 테두리는 근사로 가져옵니다 (겹치는 테두리의 우선순위 번호가 InDesign과 다를 수 있습니다)", serde_json::json!({})));
        }
        cell_edges::apply(table, borders, columns, |hex| self.colors.name(hex));
    }

    /// A cell's fill (`w:shd`): the colour is the shading's fill (`clear`) or its colour (`solid`, `pctN`, with `N` as the tint); none for automatic.
    fn cell_fill(&mut self, shd: &El) -> Option<(String, u32)> {
        let (val, color, fill) = (shd.attr("val").unwrap_or("clear"), shd.attr("color").unwrap_or("auto"), shd.attr("fill").unwrap_or("auto"));
        let (source, tint) = match val {
            "clear" => (fill, 100),
            "solid" => (color, 100),
            "nil" | "none" => return None,
            other => match other.strip_prefix("pct").and_then(|n| n.parse::<u32>().ok()) {
                Some(n) => (color, n),
                None => {
                    self.notes.add(msg("textimport.cellPattern", format!("표 칸 배경의 무늬 {other} 은(는) 아직 InDesign과 같게 가져오지 않습니다 (바탕색으로 들어갑니다)"), serde_json::json!({"other": other})));
                    (fill, 100)
                }
            },
        };
        // an automatic colour is a fill of its own: no fill (measured: `clear` with `fill` auto makes `Swatch/None`, tint 100)
        Some((self.colors.name(source).unwrap_or_else(|| "Swatch/None".into()), tint))
    }

    /// The table-level border attributes of `w:tblBorders` (measured: docxtext2-b3 .. b8, docxtext-c1 table-border). Sides that are not
    /// said keep InDesign's own (black, 1 pt). The outer sides are `<Side>BorderStroke...`, `insideH` the row strokes and `insideV` the column strokes.
    fn table_borders(&mut self, b: &El) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut put = |k: String, v: String| out.push((k, v));
        for side in b.elements() {
            let name = match side.name.as_str() {
                "top" => "Top",
                "left" | "start" => "Left",
                "bottom" => "Bottom",
                "right" | "end" => "Right",
                "insideH" => "Row",
                "insideV" => "Column",
                _ => continue,
            };
            let val = side.attr("val").unwrap_or("single");
            let none = matches!(val, "none" | "nil");
            // the line kind of the table's borders (measured: docxtext3-s01 .. s13); `None`: the solid line, which has no type attribute
            let kind = match line_kind(val) {
                Some(kind) => kind,
                None => {
                    self.notes.add(msg("textimport.tableBorder", format!("표 테두리 모양 {val} 은(는) 아직 InDesign과 같게 가져오지 않습니다 (실선으로 들어갑니다)"), serde_json::json!({"val": val})));
                    None
                }
            };
            let weight = if none { 0.0 } else { number(side, "sz").unwrap_or(4.0) / 8.0 };
            let color = side.attr("color").filter(|c| !c.eq_ignore_ascii_case("auto")).and_then(|c| self.colors.name(&c.to_ascii_uppercase()));
            let w = num(weight);
            match name {
                "Row" | "Column" => {
                    // a line between rows or columns: a start and an end stroke, one line each; the start colour is named even for black
                    put(format!("Start{name}StrokeColor"), color.clone().unwrap_or_else(|| "Color/Black".into()));
                    if let Some(c) = &color {
                        put(format!("End{name}StrokeColor"), c.clone());
                    }
                    put(format!("Start{name}StrokeCount"), "1".into());
                    put(format!("End{name}StrokeCount"), "1".into());
                    if let Some(kind) = kind {
                        put(format!("Start{name}StrokeType"), kind.into());
                        // InDesign names the end type of the column strokes differently (measured)
                        put(if name == "Row" { "EndRowStrokeType".into() } else { "EndColumnLineStyle".into() }, kind.into());
                    }
                    if weight != 1.0 {
                        put(format!("Start{name}StrokeWeight"), w.clone());
                    }
                    put(format!("End{name}StrokeWeight"), w);
                }
                _ => {
                    if weight != 1.0 {
                        put(format!("{name}BorderStrokeWeight"), w);
                    }
                    if let Some(c) = color {
                        put(format!("{name}BorderStrokeColor"), c);
                    }
                    if let Some(kind) = kind {
                        put(format!("{name}BorderStrokeType"), kind.into());
                    }
                }
            }
        }
        out
    }
}
