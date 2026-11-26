//! Tables (IDML `Table` inside a story) into `folio_model::Table`, with table and cell
//! styles resolved.
//!
//! Table attributes: the `Table` element, then its table style's `BasedOn` chain down to
//! "[No table style]". Cell attributes: the `Cell` element, then its applied cell style
//! chain, then the table style's region cell style (header, footer, left / right column,
//! body), then table-wide defaults (text insets). Edges: a cell's own edge stroke, else the
//! table border on outer edges, else InDesign's default 1 pt solid [Black]. A cell without
//! its own fill takes the table's alternating row or column fill (body rows only).

use crate::props::{PropMap, Props, Val, is_none_ref};
use crate::styles::stroke_type_name;
use crate::text::StoryReader;
use crate::warn::Warnings;
use crate::xml::Element;
use folio_model::{CellStroke, FirstBaseline, Paint, Table, TableCell, TableRow, VerticalJustification};

/// Values from an element, then style layers from the most specific (last) to the root.
struct Lookup<'a, 'l> {
    el: Option<&'a Element<'a>>,
    layers: Vec<&'l PropMap<'a>>,
}

impl<'a> Lookup<'a, '_> {
    fn get(&self, k: &str) -> Option<Val<'a>> {
        if let Some(e) = self.el
            && let Some(v) = Props::get(&e, k)
        {
            return Some(v);
        }
        self.layers.iter().rev().find_map(|l| Props::get(l, k))
    }
    fn str(&self, k: &str) -> Option<&'a str> {
        self.get(k)?.as_str()
    }
    fn f64(&self, k: &str) -> Option<f64> {
        self.get(k)?.f64()
    }
    fn bool(&self, k: &str) -> Option<bool> {
        self.get(k)?.bool()
    }
}

const SIDES: [&str; 4] = ["Top", "Left", "Bottom", "Right"];

fn count(v: Option<f64>) -> usize {
    v.map_or(0, |v| v.round().clamp(0.0, 10_000.0) as usize)
}

/// A stroke from `{prefix}Weight/Color/Tint/Type/Overprint/Priority`; `None` when the
/// lookup sets none of them.
fn stroke(l: &Lookup<'_, '_>, prefix: &str, colors: &crate::color::Colors, w: &mut Warnings) -> Option<CellStroke> {
    let k = |s: &str| format!("{prefix}{s}");
    let weight = l.f64(&k("Weight"));
    let color = l.str(&k("Color"));
    let tint = l.f64(&k("Tint"));
    let kind = l.str(&k("Type")).or_else(|| l.str(&k("LineStyle")));
    if weight.is_none() && color.is_none() && tint.is_none() && kind.is_none() {
        return None;
    }
    let mut s = CellStroke {
        weight: weight.unwrap_or(1.0).max(0.0),
        paint: colors.paint(Some(color.unwrap_or("Color/Black")), tint, w),
        stroke_type: kind.map(stroke_type_name).unwrap_or_else(|| "Solid".into()),
        overprint: l.bool(&k("Overprint")).unwrap_or(false),
        priority: l.f64(&k("Priority")).map_or(0, |v| v as i32),
        gap: Paint::None,
        gap_overprint: false,
        explicit: true,
    };
    // the layout paints InDesign's own stroke types as patterns (text/src/layout/strokes.rs); a document's other styles are drawn solid
    if s.weight > 0.0 && s.paint != Paint::None && folio_model::geom::StrokePattern::builtin(&s.stroke_type, s.weight, None, folio_model::geom::CornerAdjust::DashesAndGaps).is_none() {
        w.add(format!("table stroke type {} drawn solid", s.stroke_type));
    }
    if s.weight <= 0.0 {
        s.paint = Paint::None;
    }
    gap(&mut s, l, prefix, colors, w);
    Some(s)
}

/// The gaps of a patterned stroke are painted: [Paper] where no gap colour is set (oracle tables-alternating: alt-gap-*).
fn gap(s: &mut CellStroke, l: &Lookup<'_, '_>, prefix: &str, colors: &crate::color::Colors, w: &mut Warnings) {
    let patterned = folio_model::geom::StrokePattern::builtin(&s.stroke_type, s.weight, None, folio_model::geom::CornerAdjust::DashesAndGaps).is_some_and(|p| p != folio_model::geom::StrokePattern::Solid);
    if patterned && s.paint != Paint::None {
        s.gap = colors.paint(Some(l.str(&format!("{prefix}GapColor")).unwrap_or("Color/Paper")), l.f64(&format!("{prefix}GapTint")), w);
        s.gap_overprint = l.bool(&format!("{prefix}GapOverprint")).unwrap_or(false);
    }
}

fn first_baseline(v: Option<&str>, w: &mut Warnings) -> FirstBaseline {
    match v {
        Some("AscentOffset") | None => FirstBaseline::Ascent,
        Some("CapHeight") => FirstBaseline::CapHeight,
        Some("XHeight") => FirstBaseline::XHeight,
        Some("LeadingOffset") => FirstBaseline::Leading,
        Some("EmboxHeight") => FirstBaseline::EmBoxHeight,
        Some("FixedHeight") => FirstBaseline::Fixed,
        Some(other) => {
            w.add(format!("cell first baseline offset {other} read as Ascent"));
            FirstBaseline::Ascent
        }
    }
}

/// `Name="column:row"`.
fn cell_pos(e: &Element<'_>) -> Option<(usize, usize)> {
    let (c, r) = e.attr("Name")?.split_once(':')?;
    Some((c.trim().parse().ok()?, r.trim().parse().ok()?))
}

pub fn read_table<'a>(sr: &StoryReader<'a, '_>, e: &'a Element<'a>, w: &mut Warnings, depth: usize) -> Table {
    let sheet = sr.sheet;
    let colors = sr.colors;
    let (layers, style) = sheet.table_layers(e.attr("AppliedTableStyle"), w);
    let t = Lookup { el: Some(e), layers };

    let mut rows: Vec<(usize, TableRow)> = Vec::new();
    for r in e.children_named("Row") {
        let idx = r.attr("Name").and_then(|n| n.trim().parse().ok()).unwrap_or(rows.len());
        let num = |k: &str| Props::f64(&r, k);
        let mut row = TableRow::default();
        if let Some(v) = num("SingleRowHeight") {
            row.height = v;
        }
        if let Some(v) = num("MinimumHeight") {
            row.min_height = v;
        }
        if let Some(v) = num("MaximumHeight") {
            row.max_height = v;
        }
        if let Some(v) = Props::bool(&r, "AutoGrow") {
            row.auto_grow = v;
        }
        if let Some(v) = Props::bool(&r, "KeepWithNextRow") {
            row.keep_with_next = v;
        }
        if let Some(v) = Props::str(&r, "StartRow") {
            row.start = v.to_owned();
        }
        rows.push((idx, row));
    }
    rows.sort_by_key(|r| r.0);
    let mut columns: Vec<(usize, f64)> = e
        .children_named("Column")
        .enumerate()
        .map(|(i, c)| {
            (c.attr("Name").and_then(|n| n.trim().parse().ok()).unwrap_or(i), Props::f64(&c, "SingleColumnWidth").unwrap_or(36.0))
        })
        .collect();
    columns.sort_by_key(|c| c.0);
    let header_rows = count(t.f64("HeaderRowCount"));
    let footer_rows = count(t.f64("FooterRowCount"));
    let mut table = Table {
        id: e.attr("Self").unwrap_or_default().to_owned(),
        style,
        rows: rows.into_iter().map(|r| r.1).collect(),
        columns: columns.into_iter().map(|c| c.1.max(0.0)).collect(),
        header_rows,
        footer_rows,
        cells: Vec::new(),
        space_before: t.f64("SpaceBefore").unwrap_or(4.0),
        space_after: t.f64("SpaceAfter").unwrap_or(-4.0),
        border_weights: SIDES.map(|s| t.f64(&format!("{s}BorderStrokeWeight")).unwrap_or(1.0).max(0.0)),
        skip_first_header: t.bool("SkipFirstHeader").unwrap_or(false),
        skip_last_footer: t.bool("SkipLastFooter").unwrap_or(false),
    };
    for k in ["BreakHeaders", "BreakFooters"] {
        if let Some(v) = t.str(k)
            && v != "InAllTextColumns"
        {
            w.add(format!("table {k} {v} read as in all text columns"));
        }
    }
    if t.str("TableDirection") == Some("RightToLeftDirection") {
        w.add("right-to-left table laid out left to right");
    }
    let (nrows, ncols) = (table.rows.len(), table.columns.len());
    if nrows == 0 || ncols == 0 {
        w.add("table without rows or columns skipped");
        return table;
    }

    // Region cell styles of the table style.
    let region = |k: &str| t.str(k).filter(|v| !is_none_ref(v));
    let same = |k: &str| t.bool(k).unwrap_or(true);
    let body_style = region("BodyRegionCellStyle");
    let header_style = if same("HeaderRegionSameAsBodyRegion") { body_style } else { region("HeaderRegionCellStyle") };
    let footer_style = if same("FooterRegionSameAsBodyRegion") { body_style } else { region("FooterRegionCellStyle") };
    let left_style = if same("LeftColumnRegionSameAsBodyRegion") { body_style } else { region("LeftColumnRegionCellStyle") };
    let right_style = if same("RightColumnRegionSameAsBodyRegion") { body_style } else { region("RightColumnRegionCellStyle") };

    let default_insets = SIDES.map(|s| t.f64(&format!("Text{s}Inset")).unwrap_or(4.0));
    let black = || colors.paint(Some("Color/Black"), None, &mut Warnings::default());
    let mut own_fill = Vec::new();
    // Bottom and right edges a cell sets itself (alternating strokes do not replace them).
    let mut own_edges: Vec<[bool; 2]> = Vec::new();
    let mut cells: Vec<TableCell> = Vec::new();
    for c in e.children_named("Cell") {
        let Some((col, row)) = cell_pos(c) else {
            w.add("table cell without a usable Name skipped");
            continue;
        };
        if row >= nrows || col >= ncols {
            w.add("table cell outside the table's rows / columns skipped");
            continue;
        }
        let row_span = count(Props::f64(&c, "RowSpan")).max(1).min(nrows - row);
        let column_span = count(Props::f64(&c, "ColumnSpan")).max(1).min(ncols - col);
        let region_style = if row < header_rows {
            header_style
        } else if row + footer_rows >= nrows && footer_rows > 0 {
            footer_style
        } else if col == 0 {
            left_style
        } else if col + column_span == ncols {
            right_style
        } else {
            body_style
        };
        let mut layers = sheet.cell_layers(region_style, w).0;
        let applied = c.attr("AppliedCellStyle").filter(|a| !is_none_ref(a));
        if applied.is_some() {
            layers.extend(sheet.cell_layers(applied, w).0);
        }
        let l = Lookup { el: Some(c), layers };
        let inset = |i: usize, side: &str| l.f64(&format!("Text{side}Inset")).or_else(|| l.f64(&format!("{side}Inset"))).unwrap_or(default_insets[i]);
        let fill_color = l.str("FillColor");
        own_fill.push((fill_color, l.f64("FillTint")));
        let fill = colors.paint(fill_color, l.f64("FillTint"), w);
        let outer = [row == 0, col == 0, row + row_span == nrows, col + column_span == ncols];
        own_edges.push([stroke(&l, "BottomEdgeStroke", colors, &mut Warnings::default()).is_some(), stroke(&l, "RightEdgeStroke", colors, &mut Warnings::default()).is_some()]);
        let edges = std::array::from_fn(|i| {
            let side = SIDES[i];
            stroke(&l, &format!("{side}EdgeStroke"), colors, w)
                .or_else(|| if outer[i] { stroke(&t, &format!("{side}BorderStroke"), colors, w) } else { None })
                .unwrap_or_else(|| CellStroke { paint: black(), ..CellStroke::default() })
        });
        let rotation = l.f64("RotationAngle").unwrap_or(0.0).rem_euclid(360.0);
        if ![0.0, 90.0, 180.0, 270.0].iter().any(|r| (rotation - r).abs() < 1.0) {
            w.add(format!("table cell rotation {rotation}° not modelled (set upright)"));
        }
        let diagonal = stroke(&l, "DiagonalLineStroke", colors, w).unwrap_or_else(|| CellStroke { paint: black(), ..CellStroke::default() });
        let cell = TableCell {
            id: c.attr("Self").unwrap_or_default().to_owned(),
            row,
            column: col,
            row_span,
            column_span,
            paragraphs: sr.nested(c, w, depth + 1),
            insets: [inset(0, "Top"), inset(1, "Left"), inset(2, "Bottom"), inset(3, "Right")],
            fill,
            fill_overprint: l.bool("FillOverprint").unwrap_or(false),
            edges,
            vertical_justification: match l.str("VerticalJustification") {
                Some("CenterAlign") => VerticalJustification::Center,
                Some("BottomAlign") => VerticalJustification::Bottom,
                Some("JustifyAlign") => {
                    w.add("cell vertical justification Justify read as Top");
                    VerticalJustification::Top
                }
                _ => VerticalJustification::Top,
            },
            first_baseline: first_baseline(l.str("FirstBaselineOffset"), w),
            min_first_baseline_offset: l.f64("MinimumFirstBaselineOffset").unwrap_or(0.0),
            rotation,
            perpendicular: !l.bool("WritingDirection").unwrap_or(true),
            clip: l.bool("ClipContentToCell").or_else(|| l.bool("ClipContentToTextCell")).unwrap_or(false),
            diagonal_down: l.bool("TopLeftDiagonalLine").unwrap_or(false),
            diagonal_up: l.bool("TopRightDiagonalLine").unwrap_or(false),
            diagonal,
            diagonal_in_front: l.bool("DiagonalLineInFront").unwrap_or(false),
        };
        cells.push(cell);
    }
    if Props::str(&e, "CellType").is_some_and(|v| v == "GraphicTypeCell") {
        w.add("graphic table cells not modelled");
    }
    alternating_fills(&t, &table, &mut cells, &own_fill, colors, w);
    alternating_strokes(&t, &table, &mut cells, &own_edges, colors, w);
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by_key(|&i| (cells[i].row, cells[i].column));
    table.cells = order.into_iter().map(|i| std::mem::take(&mut cells[i])).collect();
    table
}

/// Alternating row strokes on the bottom edges of body rows (not the table's last row, whose
/// bottom is the border) and column strokes on the right edges of columns (not the last),
/// for cells that do not set those edges (InDesign 2025, scratch experiment exp4).
fn alternating_strokes(
    t: &Lookup<'_, '_>,
    table: &Table,
    cells: &mut [TableCell],
    own_edges: &[[bool; 2]],
    colors: &crate::color::Colors,
    w: &mut Warnings,
) {
    let n = |k: &str| count(t.f64(k));
    let (rs, re) = (n("StartRowStrokeCount"), n("EndRowStrokeCount"));
    let (cs, ce) = (n("StartColumnStrokeCount"), n("EndColumnStrokeCount"));
    if rs + re == 0 && cs + ce == 0 {
        return;
    }
    let style = |pre: &str, w: &mut Warnings| {
        let mut s = stroke(t, &format!("{pre}Stroke"), colors, w).unwrap_or_default();
        if let Some(kind) = t.str(&format!("{pre}LineStyle")) {
            s.stroke_type = stroke_type_name(kind);
            gap(&mut s, t, &format!("{pre}Stroke"), colors, w);
        }
        s.priority = 0;
        s
    };
    let rows = (style("StartRow", w), style("EndRow", w));
    let cols = (style("StartColumn", w), style("EndColumn", w));
    let nrows = table.rows.len();
    let body_rows = nrows.saturating_sub(table.header_rows + table.footer_rows);
    let ncols = table.columns.len();
    // the strokes are on the edges between rows (columns): the skipped ones are counted among those, the table's last row (column)
    // having none (oracle xp-rows-skip22: 8 body rows, skip 2 first and 2 last leaves the edges below rows 3, 4 and 5)
    let pick = |k: usize, edges: usize, skip_first: usize, skip_last: usize, s: usize, e: usize| -> Option<bool> {
        if k < skip_first || k + skip_last >= edges || s + e == 0 {
            return None;
        }
        Some((k - skip_first) % (s + e) < s)
    };
    for (c, own) in cells.iter_mut().zip(own_edges) {
        let last_row = c.row + c.row_span.max(1) - 1;
        if !own[0] && last_row >= table.header_rows && last_row + 1 < table.header_rows + body_rows {
            let k = last_row - table.header_rows;
            if let Some(start) = pick(k, body_rows - 1, n("SkipFirstAlternatingStrokeRows"), n("SkipLastAlternatingStrokeRows"), rs, re) {
                c.edges[2] = if start { rows.0.clone() } else { rows.1.clone() };
            }
        }
        let last_col = c.column + c.column_span.max(1) - 1;
        if !own[1] && last_col + 1 < ncols
            && let Some(start) = pick(last_col, ncols - 1, n("SkipFirstAlternatingStrokeColumns"), n("SkipLastAlternatingStrokeColumns"), cs, ce) {
            c.edges[3] = if start { cols.0.clone() } else { cols.1.clone() };
        }
    }
}

/// Alternating fill properties supply the values the cell does not set. One pattern applies, not both:
/// the columns' when `ColumnFillsPriority` is on (every row, the header and footer too), else the rows' (body rows only); the other
/// pattern's counts are kept but not used (`alt-fill-both`, `alt-fill-both-columns-first`, `xp-colfill-footer`).
fn alternating_fills(
    t: &Lookup<'_, '_>,
    table: &Table,
    cells: &mut [TableCell],
    own_fill: &[(Option<&str>, Option<f64>)],
    colors: &crate::color::Colors,
    w: &mut Warnings,
) {
    let n = |k: &str| count(t.f64(k));
    let (rs, re) = (n("StartRowFillCount"), n("EndRowFillCount"));
    let (cs, ce) = (n("StartColumnFillCount"), n("EndColumnFillCount"));
    if rs + re == 0 && cs + ce == 0 {
        return;
    }
    let body_rows = table.rows.len().saturating_sub(table.header_rows + table.footer_rows);
    let ncols = table.columns.len();
    let pick = |k: usize, total: usize, skip_first: usize, skip_last: usize, s: usize, e: usize| -> Option<bool> {
        if k < skip_first || k + skip_last >= total || s + e == 0 {
            return None;
        }
        Some((k - skip_first) % (s + e) < s)
    };
    let columns = t.bool("ColumnFillsPriority").unwrap_or(false);
    for (c, own) in cells.iter_mut().zip(own_fill) {
        let chosen = if columns {
            pick(c.column, ncols, n("SkipFirstAlternatingFillColumns"), n("SkipLastAlternatingFillColumns"), cs, ce).map(|start| if start { "StartColumn" } else { "EndColumn" })
        } else if c.row >= table.header_rows && c.row < table.header_rows + body_rows {
            pick(c.row - table.header_rows, body_rows, n("SkipFirstAlternatingFillRows"), n("SkipLastAlternatingFillRows"), rs, re).map(|start| if start { "StartRow" } else { "EndRow" })
        } else {
            None
        };
        if let Some(pre) = chosen {
            c.fill = colors.paint(own.0.or_else(|| t.str(&format!("{pre}FillColor"))), own.1.or_else(|| t.f64(&format!("{pre}FillTint"))), w);
            if own.0.is_none() && own.1.is_none() {
                c.fill_overprint = t.bool(&format!("{pre}FillOverprint")).unwrap_or(false);
            }
        }
    }
}
