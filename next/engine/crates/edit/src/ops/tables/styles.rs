//! Cell Styles and Table Styles panels: applying a style and Clear Overrides (oracle `tablestyles`, measured with the DOM's
//! `cell.appliedCellStyle`, `table.appliedTableStyle`, `clearCellStyleOverrides` and `clearTableStyleOverrides`).
//! - A cell style is applied by naming it in the cell's `AppliedCellStyle`. `AppliedCellStylePriority` is 0 for `[None]`, else one
//!   more than the highest priority of the cells that share an edge with the cell (cs-apply-range: a block of six cells gets 1 2 3 /
//!   2 3 4). The cell keeps what it sets itself (cs-apply-local).
//! - For every edge the style (with the styles it is based on) gives a stroke, the cell and the cells across that edge in the same
//!   part of the table (`across`) get `…EdgeStrokePriority`: 1 when the style gives all four edges, else 0, but not less than the
//!   highest priority the cell has already (a stroke set on the cell itself has 2: cs-clear-stroke) (cs-edge-*, cs-apply-box).
//!   Applying `[None]` later leaves these attributes (cs-apply-none).
//! - A style with a paragraph style puts it on every paragraph of the cell (cs-apply-range). `[None]` takes a paragraph style the old
//!   style gave back to the basic paragraph style (cs-apply-blue-none); another style that gives none leaves it (cs-apply-blue-orange).
//! - A table style is its name in the table's `AppliedTableStyle`; the table keeps what it sets itself (ts-apply-local). Its region
//!   cell styles (header, footer, first and last column, body) work like a cell style of the cells that name none of their own, as far
//!   as the paragraph style goes: the header cells of ts-apply take the header region's paragraph style, a table style without one
//!   gives them back the basic one (ts-apply-zebra-frame).
//! - Clear Cell Style Overrides removes the cell's own settings of what the cell style gives (with the styles it is based on; for
//!   `[None]`: of everything a style can give, the four insets and `ClipContentToCell` included: cs-clear-many, cs-clear-inset,
//!   cs-clear-stroke); Clear Table Style Overrides removes every setting a table style could give, whatever the applied style gives
//!   (ts-clear-border) and leaves the cells alone (ts-clear-cells).
use super::*;
use std::collections::BTreeSet;

const CELL_NONE: &str = "CellStyle/$ID/[None]";
const TABLE_NONE: &str = "TableStyle/$ID/[No table style]";
const NO_PARAGRAPH: &str = "ParagraphStyle/$ID/[No paragraph style]";
const BASIC_PARAGRAPH: &str = "ParagraphStyle/$ID/NormalParagraphStyle";
const SIDES: [&str; 4] = ["Top", "Left", "Bottom", "Right"];

/// The document's style definitions (`Resources/Styles.xml`).
struct Sheet {
    t: String,
    ix: Index,
}

impl Sheet {
    fn read(w: &Work<'_>) -> Sheet {
        let t = w.text("Resources/Styles.xml").map(str::to_owned).unwrap_or_default();
        let ix = Index::scan(&t).unwrap_or_default();
        Sheet { t, ix }
    }

    /// The elements of `id` and the styles it is based on, nearest first. The built-in roots (`[None]`, `[No table style]`) are left
    /// out: they hold InDesign's defaults, which no style "gives".
    fn chain(&self, id: &str, prefix: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut id = id.to_owned();
        for _ in 0..32 {
            if id.ends_with("/$ID/[None]") || id.ends_with("/$ID/[No table style]") {
                break;
            }
            let Some(s) = self.ix.by_self(&self.t, &id) else { break };
            out.push(s);
            let Some(b) = self.ix.path(&self.t, s, &["Properties", "BasedOn"]) else { break };
            let n = &self.ix.nodes[b];
            let name = crate::xml::unescape(self.t[n.open_end..n.close_start].trim());
            if name.is_empty() {
                break;
            }
            id = if name.starts_with(prefix) { name } else { format!("{prefix}{name}") };
        }
        out
    }

    /// What the style chain gives `attr`, nearest first (the value `skip` counts as not given).
    fn value(&self, id: &str, prefix: &str, attr: &str, skip: &str) -> Option<String> {
        self.chain(id, prefix).into_iter().filter_map(|s| self.ix.value(&self.t, s, attr)).find(|v| v != skip && !v.is_empty() && v != "n")
    }

    /// Every attribute any style of the chain sets.
    fn names(&self, id: &str, prefix: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for s in self.chain(id, prefix) {
            for a in self.ix.attrs(&self.t, s) {
                out.insert(self.t[a.name.0..a.name.1].to_owned());
            }
        }
        out
    }

    /// The paragraph style a cell style gives its cells, if it gives one.
    fn paragraph(&self, cell_style: &str) -> Option<String> {
        self.value(cell_style, "CellStyle/", "AppliedParagraphStyle", NO_PARAGRAPH)
    }

    /// The cell style a table style names for a region (`Header`, `Footer`, `LeftColumn`, `RightColumn`, `Body`), if it names one.
    fn region(&self, table_style: &str, region: &str) -> Option<String> {
        let same = |r: &str| self.value(table_style, "TableStyle/", &format!("{r}RegionSameAsBodyRegion"), "").is_none_or(|v| v == "true");
        let region = if region != "Body" && same(region) { "Body" } else { region };
        self.value(table_style, "TableStyle/", &format!("{region}RegionCellStyle"), CELL_NONE)
    }
}

/// The settings a cell style can give a cell: fill, vertical justification and the other cell options, the stroke of each edge, the four
/// insets (not the text insets, which every cell carries) and `ClipContentToCell`.
fn cell_setting(name: &str) -> bool {
    const PLAIN: [&str; 9] = ["FillColor", "FillTint", "OverprintFill", "VerticalJustification", "FirstBaselineOffset", "ParagraphSpacingLimit", "MinimumFirstBaselineOffset", "RotationAngle", "ClipContentToCell"];
    PLAIN.contains(&name)
        || name.starts_with("GradientFill")
        || name.starts_with("Diagonal")
        || name.starts_with("TopLeftDiagonal")
        || name.starts_with("TopRightDiagonal")
        || SIDES.iter().any(|s| name == format!("{s}Inset") || (name.starts_with(&format!("{s}EdgeStroke")) && !name.ends_with("Priority")))
}

/// The paragraphs of a cell's content in another paragraph style: all of them, or those in `only_from`.
fn restyle(inner: &str, to: &str, only_from: Option<&str>) -> String {
    const OPEN: &str = "<X>";
    let wrap = format!("{OPEN}{inner}</X>");
    let Ok(ix) = Index::scan(&wrap) else { return inner.to_owned() };
    let mut edits = Vec::new();
    for i in 0..ix.nodes.len() {
        if ix.local(&wrap, i) != "ParagraphStyleRange" {
            continue;
        }
        let Some(current) = ix.value(&wrap, i, "AppliedParagraphStyle") else { continue };
        if current != to && only_from.is_none_or(|f| current == f) {
            edits.push(attr_edit(&ix, &wrap, i, "AppliedParagraphStyle", to));
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut out = wrap;
    for (range, text) in edits {
        out.replace_range(range, &text);
    }
    out[OPEN.len()..out.len() - "</X>".len()].to_owned()
}

fn priority(x: &TableX, k: usize) -> u32 {
    get(&x.cells[k].attrs, "AppliedCellStylePriority").and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Cell Styles panel: `style` (`CellStyle/Name`, `CellStyle/$ID/[None]`) on the cells ([column, row]), in that order.
pub(in crate::ops) fn apply_cell_style(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]], style: &str) -> Result<(), EditError> {
    let sheet = Sheet::read(w);
    let none = style == CELL_NONE;
    if !none && sheet.ix.by_self(&sheet.t, style).is_none() {
        return Err(EditError::new("notFound", format!("cell style {style}")));
    }
    let given = sheet.names(style, "CellStyle/");
    let edges: Vec<bool> = SIDES.iter().map(|s| given.iter().any(|n| n.starts_with(&format!("{s}EdgeStroke")))).collect();
    let strokes: u32 = if edges.iter().all(|e| *e) { 1 } else { 0 };
    let new_paragraph = sheet.paragraph(style);
    let mut x = TableX::read(w, table)?;
    for &[c, r] in cells {
        let k = x.at(c, r).ok_or_else(|| EditError::new("invalid", format!("no cell {c}:{r}")))?;
        let old = get(&x.cells[k].attrs, "AppliedCellStyle").unwrap_or(CELL_NONE).to_owned();
        let old_paragraph = sheet.paragraph(&old);
        let next = if none {
            0
        } else {
            1 + SIDES.iter().flat_map(|s| x.across(k, s)).map(|n| priority(&x, n)).max().unwrap_or(0)
        };
        put(&mut x.cells[k].attrs, "AppliedCellStyle", style.to_owned());
        put(&mut x.cells[k].attrs, "AppliedCellStylePriority", next.to_string());
        if !none {
            let held = SIDES.iter().filter_map(|s| get(&x.cells[k].attrs, &format!("{s}EdgeStrokePriority")).and_then(|v| v.parse::<u32>().ok())).max().unwrap_or(0);
            let value = strokes.max(held).to_string();
            for (side, given) in SIDES.iter().zip(&edges) {
                if !given {
                    continue;
                }
                put(&mut x.cells[k].attrs, &format!("{side}EdgeStrokePriority"), value.clone());
                for n in x.across(k, side) {
                    put(&mut x.cells[n].attrs, &format!("{}EdgeStrokePriority", super::opposite(side)), value.clone());
                }
            }
        }
        match (&new_paragraph, &old_paragraph) {
            (Some(p), _) => x.cells[k].inner = restyle(&x.cells[k].inner, p, None),
            (None, Some(p)) if none => x.cells[k].inner = restyle(&x.cells[k].inner, BASIC_PARAGRAPH, Some(p)),
            _ => {}
        }
    }
    x.write(w)
}

/// Clear Cell Style Overrides on the cells ([column, row]).
pub(in crate::ops) fn clear_cell_overrides(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]]) -> Result<(), EditError> {
    let sheet = Sheet::read(w);
    let mut x = TableX::read(w, table)?;
    for &[c, r] in cells {
        let k = x.at(c, r).ok_or_else(|| EditError::new("invalid", format!("no cell {c}:{r}")))?;
        let applied = get(&x.cells[k].attrs, "AppliedCellStyle").unwrap_or(CELL_NONE).to_owned();
        let given = (applied != CELL_NONE).then(|| sheet.names(&applied, "CellStyle/"));
        let cleared = |name: &str| cell_setting(name) && given.as_ref().is_none_or(|g| g.contains(name));
        // an edge is shared (set_cells): the cells across an edge whose stroke goes lose their side of it
        let mut style_edges = Vec::new();
        for side in SIDES {
            let edge = format!("{side}EdgeStroke");
            if !x.cells[k].attrs.iter().any(|(name, _)| name.starts_with(&edge) && cleared(name)) {
                continue;
            }
            // the stroke the style gives takes the edge with its own priority, 1 (cs-redefine: a stroke set on the cell had 2)
            if given.as_ref().is_some_and(|g| g.iter().any(|n| n.starts_with(&edge) && !n.ends_with("Priority"))) {
                style_edges.push(side);
            }
            let opposite = format!("{}EdgeStroke", super::opposite(side));
            for n in x.across(k, side) {
                x.cells[n].attrs.retain(|(name, _)| !name.starts_with(&opposite) || name.ends_with("Priority") && given.is_some());
            }
        }
        x.cells[k].attrs.retain(|(name, _)| !cleared(name));
        for side in style_edges {
            put(&mut x.cells[k].attrs, &format!("{side}EdgeStrokePriority"), "1".into());
            for n in x.across(k, side) {
                put(&mut x.cells[n].attrs, &format!("{}EdgeStrokePriority", super::opposite(side)), "1".into());
            }
        }
        // `[None]` gives every edge: the priorities of the cell's edges and of the cells across them go with the strokes (cs-clear-many)
        if given.is_none() {
            for side in SIDES {
                x.cells[k].attrs.retain(|(name, _)| *name != format!("{side}EdgeStrokePriority"));
                for n in x.across(k, side) {
                    x.cells[n].attrs.retain(|(name, _)| *name != format!("{}EdgeStrokePriority", super::opposite(side)));
                }
            }
        }
    }
    x.write(w)
}

/// The region a cell is in for its table style: the header and footer rows first, then the first and last column, else the body.
fn region_of(x: &TableX, k: usize) -> &'static str {
    let c = &x.cells[k];
    let columns = x.columns.len();
    match x.part_of(c.row) {
        0 => "Header",
        2 => "Footer",
        _ if c.column == 0 => "LeftColumn",
        _ if c.column + c.columns >= columns => "RightColumn",
        _ => "Body",
    }
}

/// Table Styles panel: `style` (`TableStyle/Name`, `TableStyle/$ID/[No table style]`) on the table.
pub(in crate::ops) fn apply_table_style(w: &mut Work<'_>, table: &str, style: &str) -> Result<(), EditError> {
    let sheet = Sheet::read(w);
    if style != TABLE_NONE && sheet.ix.by_self(&sheet.t, style).is_none() {
        return Err(EditError::new("notFound", format!("table style {style}")));
    }
    let mut x = TableX::read(w, table)?;
    let old = get(&x.attrs, "AppliedTableStyle").unwrap_or(TABLE_NONE).to_owned();
    put(&mut x.attrs, "AppliedTableStyle", style.to_owned());
    // the cells that name no style of their own follow the regions of the table style
    for k in 0..x.cells.len() {
        if get(&x.cells[k].attrs, "AppliedCellStyle").is_some_and(|s| s != CELL_NONE) {
            continue;
        }
        let region = region_of(&x, k);
        let paragraph = |table_style: &str| sheet.region(table_style, region).and_then(|s| sheet.paragraph(&s));
        let (before, after) = (paragraph(&old), paragraph(style));
        match (&after, &before) {
            (Some(p), _) => x.cells[k].inner = restyle(&x.cells[k].inner, p, None),
            (None, Some(p)) => x.cells[k].inner = restyle(&x.cells[k].inner, BASIC_PARAGRAPH, Some(p)),
            _ => {}
        }
    }
    x.write(w)
}

/// Clear Table Style Overrides.
pub(in crate::ops) fn clear_table_overrides(w: &mut Work<'_>, table: &str) -> Result<(), EditError> {
    let settings: BTreeSet<&str> = super::super::tableopts::style_attributes().collect();
    let mut x = TableX::read(w, table)?;
    x.attrs.retain(|(name, _)| !settings.contains(name.as_str()));
    x.write(w)
}

/// The attributes a style (with the styles it is based on) gives, name by name: the nearest style's value wins.
pub(in crate::ops) fn given(w: &Work<'_>, id: &str, prefix: &str) -> std::collections::BTreeMap<String, String> {
    let sheet = Sheet::read(w);
    sheet.names(id, prefix).into_iter()
        .filter(|n| !matches!(n.as_str(), "Self" | "Name" | "KeyboardShortcut" | "ExtendedKeyboardShortcut"))
        .filter_map(|n| sheet.value(id, prefix, &n, "").map(|v| (n, v))).collect()
}

/// A cell style taken away with no replacement (the DOM's `remove()`; oracle cs-group-delete): the cells that had it keep what it gave as
/// their own settings (the cells across a shared edge get their side of an edge stroke), then get `[None]` as `apply_cell_style` does.
pub(in crate::ops) fn keep_cell_style(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]], style: &str) -> Result<(), EditError> {
    let sheet = Sheet::read(w);
    let mut settings: Vec<(String, String)> = sheet.names(style, "CellStyle/").into_iter()
        .filter(|n| cell_setting(n) && !n.ends_with("Priority") && n != "ClipContentToCell")
        .filter_map(|n| sheet.value(style, "CellStyle/", &n, "").map(|v| (n, v))).collect();
    // InDesign writes the fill, then the edges, then the rest
    let rank = |name: &str| -> (u8, usize, String) {
        if name.starts_with("Fill") || name.starts_with("Gradient") || name == "OverprintFill" {
            return (0, 0, name.to_owned());
        }
        match SIDES.iter().position(|s| name.starts_with(&format!("{s}EdgeStroke"))) {
            Some(i) => (1, i, name.to_owned()),
            None => (2, 0, name.to_owned()),
        }
    };
    settings.sort_by_key(|(n, _)| rank(n));
    let before = "ClipContentToCell";
    let mut x = TableX::read(w, table)?;
    for &[c, r] in cells {
        let k = x.at(c, r).ok_or_else(|| EditError::new("invalid", format!("no cell {c}:{r}")))?;
        for (name, value) in &settings {
            if get(&x.cells[k].attrs, name).is_none() {
                put_before(&mut x.cells[k].attrs, name, value.clone(), before);
            }
            // the text inset goes with the inset
            if name.ends_with("Inset") && !name.starts_with("Text") {
                put(&mut x.cells[k].attrs, &format!("Text{name}"), value.clone());
            }
            for side in SIDES {
                if let Some(rest) = name.strip_prefix(&format!("{side}EdgeStroke")) {
                    for n in x.across(k, side) {
                        let opposite = format!("{}EdgeStroke{rest}", super::opposite(side));
                        if get(&x.cells[n].attrs, &opposite).is_none() {
                            put_before(&mut x.cells[n].attrs, &opposite, value.clone(), before);
                        }
                    }
                }
            }
        }
    }
    x.write(w)?;
    apply_cell_style(w, table, cells, CELL_NONE)
}

/// Cell Styles panel > Break Link to Style (oracle tablestyles cs-break-link): the cells keep what their style gave as their own settings
/// (the cells across an edge get their side of an edge stroke) and get `[None]`, the way a style taken away with no replacement does.
pub(crate) fn break_cell_link(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]]) -> Result<(), EditError> {
    let x = TableX::read(w, table)?;
    // the cells of each style, in the order given
    let mut by_style: Vec<(String, Vec<[usize; 2]>)> = Vec::new();
    for &[c, r] in cells {
        let k = x.at(c, r).ok_or_else(|| EditError::new("invalid", format!("no cell {c}:{r}")))?;
        let style = get(&x.cells[k].attrs, "AppliedCellStyle").unwrap_or(CELL_NONE).to_owned();
        if style == CELL_NONE {
            continue;
        }
        match by_style.iter_mut().find(|(s, _)| *s == style) {
            Some((_, list)) => list.push([c, r]),
            None => by_style.push((style, vec![[c, r]])),
        }
    }
    for (style, list) in by_style {
        keep_cell_style(w, table, &list, &style)?;
    }
    Ok(())
}

/// Cell Styles panel > Clear Attributes Not Defined by Style (oracle tablestyles cs-clear-not-defined): InDesign takes away every setting
/// a cell style can give that the cell holds itself (fill, strokes, insets, `ClipContentToCell`; also what the applied style defines: the
/// fill set over Oracle Cell Orange went too), so the cell shows its style alone. The edge priorities stay; the cells across an edge
/// whose stroke goes lose their side of it.
pub(crate) fn clear_cell_attributes(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]]) -> Result<(), EditError> {
    let sheet = Sheet::read(w);
    let mut x = TableX::read(w, table)?;
    for &[c, r] in cells {
        let k = x.at(c, r).ok_or_else(|| EditError::new("invalid", format!("no cell {c}:{r}")))?;
        for side in SIDES {
            let edge = format!("{side}EdgeStroke");
            if !x.cells[k].attrs.iter().any(|(name, _)| name.starts_with(&edge) && !name.ends_with("Priority")) {
                continue;
            }
            let opposite = format!("{}EdgeStroke", super::opposite(side));
            for n in x.across(k, side) {
                x.cells[n].attrs.retain(|(name, _)| !name.starts_with(&opposite) || name.ends_with("Priority"));
            }
        }
        // the text inset follows the inset that goes: back to what the cell style gives, else the table's (cs-clear-not-defined: 9 -> 4)
        let applied = get(&x.cells[k].attrs, "AppliedCellStyle").unwrap_or(CELL_NONE).to_owned();
        for side in SIDES {
            let inset = format!("{side}Inset");
            if get(&x.cells[k].attrs, &inset).is_none() {
                continue;
            }
            let text = format!("Text{inset}");
            let back = sheet.value(&applied, "CellStyle/", &inset, "").or_else(|| get(&x.attrs, &text).map(str::to_owned)).unwrap_or_else(|| "4".into());
            put(&mut x.cells[k].attrs, &text, back);
        }
        x.cells[k].attrs.retain(|(name, _)| !cell_setting(name));
    }
    x.write(w)
}

/// The table attributes InDesign writes with a table style's alternating row fills when the link is broken (ts-break-link): the
/// column fill group and the skip counts, with the values of `[No table style]`.
const ROW_FILL_COMPANIONS: [&str; 12] = ["StartColumnFillColor", "EndColumnFillColor", "StartColumnFillCount", "EndColumnFillCount", "ColumnFillsPriority",
    "SkipLastAlternatingFillRows", "SkipFirstAlternatingFillColumns", "SkipLastAlternatingFillColumns", "StartColumnFillTint", "EndColumnFillTint",
    "StartColumnFillOverprint", "EndColumnFillOverprint"];

/// Table Styles panel > Break Link to Style (oracle tablestyles ts-break-link): the table keeps what its style gives as its own settings
/// (what the table sets itself wins) and gets `[No table style]`; the cells that name no cell style of their own keep what the cell style
/// of their region gives (the header cells of Oracle Zebra get the fill of Oracle Cell Blue); their paragraphs stay as they are.
pub(crate) fn break_table_link(w: &mut Work<'_>, table: &str) -> Result<(), EditError> {
    let sheet = Sheet::read(w);
    let mut x = TableX::read(w, table)?;
    let style = get(&x.attrs, "AppliedTableStyle").unwrap_or(TABLE_NONE).to_owned();
    if style == TABLE_NONE {
        return Ok(());
    }
    let settings: BTreeSet<&str> = super::super::tableopts::style_attributes().collect();
    let mut given: Vec<(String, String)> = sheet.names(&style, "TableStyle/").into_iter()
        .filter(|n| settings.contains(n.as_str()))
        .filter_map(|n| sheet.value(&style, "TableStyle/", &n, "").map(|v| (n, v))).collect();
    if given.iter().any(|(n, _)| n.starts_with("StartRowFill") || n.starts_with("EndRowFill")) {
        let root = sheet.ix.by_self(&sheet.t, TABLE_NONE);
        for name in ROW_FILL_COMPANIONS {
            if !given.iter().any(|(n, _)| n == name)
                && let Some(v) = root.and_then(|i| sheet.ix.value(&sheet.t, i, name))
            {
                given.push((name.to_owned(), v.to_owned()));
            }
        }
    }
    // the region cell styles: what they give the cells without a style of their own
    let mut regions: Vec<(usize, String)> = Vec::new();
    for k in 0..x.cells.len() {
        if get(&x.cells[k].attrs, "AppliedCellStyle").is_some_and(|s| s != CELL_NONE) {
            continue;
        }
        if let Some(cell_style) = sheet.region(&style, region_of(&x, k)) {
            regions.push((k, cell_style));
        }
    }
    for (name, value) in given {
        if get(&x.attrs, &name).is_none() {
            put(&mut x.attrs, &name, value);
        }
    }
    put(&mut x.attrs, "AppliedTableStyle", TABLE_NONE.to_owned());
    let before = "ClipContentToCell";
    for (k, cell_style) in regions {
        let mut cell_given: Vec<(String, String)> = sheet.names(&cell_style, "CellStyle/").into_iter()
            .filter(|n| cell_setting(n) && !n.ends_with("Priority") && n != "ClipContentToCell")
            .filter_map(|n| sheet.value(&cell_style, "CellStyle/", &n, "").map(|v| (n, v))).collect();
        cell_given.sort_by_key(|(n, _)| (!(n.starts_with("Fill") || n.starts_with("Gradient") || n == "OverprintFill"), n.clone()));
        for (name, value) in cell_given {
            if get(&x.cells[k].attrs, &name).is_none() {
                put_before(&mut x.cells[k].attrs, &name, value, before);
            }
        }
    }
    x.write(w)
}
