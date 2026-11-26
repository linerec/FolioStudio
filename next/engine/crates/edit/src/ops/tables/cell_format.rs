//! Apply formatting to cells and their shared edges.

use super::*;

/// A change to cells (Cell Options); absent fields stay.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellChange {
    /// Fill swatch (`Color/…`, `Swatch/None`) and tint (percent).
    #[serde(default)] pub fill: Option<String>,
    #[serde(default)] pub fill_tint: Option<f64>,
    /// Text insets top, left, bottom, right (points).
    #[serde(default)] pub insets: Option<[f64; 4]>,
    /// `TopAlign`, `CenterAlign`, `BottomAlign`, `JustifyAlign`.
    #[serde(default)] pub vertical: Option<String>,
    /// Edge strokes by side ("Top", "Left", "Bottom", "Right"): weight and colour.
    #[serde(default)] pub strokes: BTreeMap<String, EdgeStroke>,
    /// Cell Options ▸ Diagonal Lines (tables/cells.rs).
    #[serde(default)] pub diagonal: Option<DiagonalLines>,
    /// Cell Options ▸ Graphic: clip a graphic cell's content to the cell (`ClipContentToCell`).
    #[serde(default)] pub clip: Option<bool>,
    /// Cell text rotation in clockwise degrees: 0, 90, 180 or 270.
    #[serde(default)] pub rotation: Option<u16>,
    /// WritingDirection: true runs parallel to the story, false perpendicular.
    #[serde(default)] pub writing_direction: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeStroke {
    #[serde(default)] pub weight: Option<f64>,
    #[serde(default)] pub color: Option<String>,
}

/// Cell Options for cells `cells` ([column, row] each).
pub(in crate::ops) fn set_cells(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]], ch: &CellChange) -> Result<(), EditError> {
    if ch.rotation.is_some_and(|r| ![0, 90, 180, 270].contains(&r)) {
        return Err(EditError::new("invalid", "cell rotation: 0, 90, 180 or 270 degrees"));
    }
    if ch.fill_tint.is_some_and(|t| !(0.0..=100.0).contains(&t)) || ch.insets.is_some_and(|i| i.iter().any(|v| !v.is_finite() || *v < 0.0)) {
        return Err(EditError::new("invalid", "cell: tint 0 to 100 %, insets 0 or more"));
    }
    if ch.vertical.as_deref().is_some_and(|v| !["TopAlign", "CenterAlign", "BottomAlign", "JustifyAlign"].contains(&v)) {
        return Err(EditError::new("invalid", "unknown vertical justification"));
    }
    if ch.strokes.keys().any(|s| !["Top", "Left", "Bottom", "Right"].contains(&s.as_str())) {
        return Err(EditError::new("invalid", "edges: Top, Left, Bottom, Right"));
    }
    if let Some(d) = &ch.diagonal {
        d.check()?;
    }
    let mut x = TableX::read(w, table)?;
    for &[c, r] in cells {
        let k = x.at(c, r).ok_or_else(|| EditError::new("invalid", format!("no cell {c}:{r}")))?;
        let a = &mut x.cells[k].attrs;
        let before = "ClipContentToCell";
        if let Some(d) = &ch.diagonal { d.apply(a); }
        if let Some(v) = ch.clip { put(a, before, v.to_string()); }
        if let Some(v) = ch.rotation { put(a, "RotationAngle", v.to_string()); }
        if let Some(v) = ch.writing_direction { put(a, "WritingDirection", v.to_string()); }
        if let Some(f) = &ch.fill { put_before(a, "FillColor", f.clone(), before); }
        if let Some(t) = ch.fill_tint { put_before(a, "FillTint", geom::num(t), before); }
        if let Some(ins) = ch.insets {
            for (k2, v) in ["Top", "Left", "Bottom", "Right"].iter().zip(ins) {
                put(a, &format!("Text{k2}Inset"), geom::num(v));
                put_before(a, &format!("{k2}Inset"), geom::num(v), before);
            }
        }
        if let Some(v) = &ch.vertical { put_before(a, "VerticalJustification", v.clone(), before); }
        // An edge is shared: the cells across it in the same part of the table (header, body,
        // footer) get it as their opposite edge (tbl-cell-stroke-body); across the header's or
        // footer's boundary each side keeps its own (tbl-cell-stroke, tbl-cell-stroke-header).
        for (side, s) in &ch.strokes {
            let mut owners = vec![(k, side.as_str())];
            owners.extend(x.across(k, side).into_iter().map(|n| (n, opposite(side))));
            for (n, side) in owners {
                let a = &mut x.cells[n].attrs;
                if let Some(wt) = s.weight { put_before(a, &format!("{side}EdgeStrokeWeight"), geom::num(wt), before); }
                if let Some(col) = &s.color { put_before(a, &format!("{side}EdgeStrokeColor"), col.clone(), before); }
                // a stroke set on the cell outranks the defaults (InDesign writes priority 2)
                put(a, &format!("{side}EdgeStrokePriority"), "2".into());
            }
        }
    }
    x.write(w)
}

pub(super) fn opposite(side: &str) -> &'static str {
    match side {
        "Top" => "Bottom",
        "Bottom" => "Top",
        "Left" => "Right",
        _ => "Left",
    }
}

impl TableX {
    /// The cells across one edge of cell `k` sharing it: in the same part of the table.
    pub(super) fn across(&self, k: usize, side: &str) -> Vec<usize> {
        let c = &self.cells[k];
        let (c0, c1, r0, r1) = (c.column, c.column + c.columns, c.row, c.row + c.rows);
        let overlap = |a0: usize, a1: usize, b0: usize, b1: usize| a0 < b1 && b0 < a1;
        (0..self.cells.len()).filter(|&n| {
            let o = &self.cells[n];
            let (o0, o1, p0, p1) = (o.column, o.column + o.columns, o.row, o.row + o.rows);
            match side {
                "Top" => p1 == r0 && overlap(o0, o1, c0, c1) && self.part_of(p1 - 1) == self.part_of(r0),
                "Bottom" => p0 == r1 && overlap(o0, o1, c0, c1) && self.part_of(p0) == self.part_of(r1 - 1),
                "Left" => o1 == c0 && overlap(p0, p1, r0, r1),
                _ => o0 == c1 && overlap(p0, p1, r0, r1),
            }
        }).collect()
    }
}
