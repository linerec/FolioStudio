//! Read and write tables, rows, columns and cells in story XML.

use super::*;

/// One `Row` or `Column`: its attributes besides Self and Name.
#[derive(Clone, Debug)]
pub(super) struct Line {
    pub(super) attrs: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub(super) struct CellX {
    pub(super) id: String,
    pub(super) column: usize,
    pub(super) row: usize,
    pub(super) columns: usize,
    pub(super) rows: usize,
    /// Attributes besides Self, Name, RowSpan and ColumnSpan.
    pub(super) attrs: Vec<(String, String)>,
    /// The XML between the cell's tags.
    pub(super) inner: String,
}

/// A table taken apart.
pub(super) struct TableX {
    pub(super) id: String,
    pub(super) attrs: Vec<(String, String)>,
    /// Other children (kept, first).
    pub(super) other: Vec<String>,
    pub(super) rows: Vec<Line>,
    pub(super) columns: Vec<Line>,
    pub(super) cells: Vec<CellX>,
    /// Where the table element is (its part and byte range) and its indentation.
    pub(super) part: String,
    pub(super) range: std::ops::Range<usize>,
    pub(super) indent: String,
}

fn attrs_of(ix: &Index, t: &str, i: usize, skip: &[&str]) -> Vec<(String, String)> {
    ix.attrs(t, i).iter().map(|a| (t[a.name.0..a.name.1].to_owned(), crate::xml::unescape(&t[a.value.0..a.value.1])))
        .filter(|(k, _)| !skip.contains(&k.as_str())).collect()
}

pub(super) fn get<'a>(attrs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

pub(super) fn put(attrs: &mut Vec<(String, String)>, k: &str, v: String) {
    match attrs.iter_mut().find(|(n, _)| n == k) {
        Some(a) => a.1 = v,
        None => attrs.push((k.to_owned(), v)),
    }
}

/// Puts `k` just before `before` when it is new (InDesign's place for it), else in place.
pub(super) fn put_before(attrs: &mut Vec<(String, String)>, k: &str, v: String, before: &str) {
    if let Some(a) = attrs.iter_mut().find(|(n, _)| n == k) {
        a.1 = v;
        return;
    }
    let at = attrs.iter().position(|(n, _)| n == before).unwrap_or(attrs.len());
    attrs.insert(at, (k.to_owned(), v));
}

impl TableX {
    pub(super) fn read(w: &mut Work<'_>, id: &str) -> Result<TableX, EditError> {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        if ix.local(t, i) != "Table" {
            return Err(EditError::new("invalid", format!("{id} is not a table")));
        }
        let n = &ix.nodes[i];
        let mut x = TableX {
            id: id.to_owned(),
            attrs: attrs_of(&ix, t, i, &["Self"]),
            other: Vec::new(),
            rows: Vec::new(),
            columns: Vec::new(),
            cells: Vec::new(),
            part: part.clone(),
            range: n.start..n.end,
            indent: indent_before(t, n.start).to_owned(),
        };
        for &c in ix.children(i) {
            let cn = &ix.nodes[c];
            match ix.local(t, c) {
                "Row" => x.rows.push(Line { attrs: attrs_of(&ix, t, c, &["Self", "Name"]) }),
                "Column" => x.columns.push(Line { attrs: attrs_of(&ix, t, c, &["Self", "Name"]) }),
                "Cell" => {
                    let name = ix.attr(t, c, "Name").unwrap_or("0:0");
                    let (col, row) = name.split_once(':').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
                        .ok_or_else(|| EditError::new("xml", format!("cell name {name}")))?;
                    let span = |k: &str| ix.attr(t, c, k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
                    x.cells.push(CellX {
                        id: ix.attr(t, c, "Self").unwrap_or_default().to_owned(),
                        column: col,
                        row,
                        columns: span("ColumnSpan"),
                        rows: span("RowSpan"),
                        attrs: attrs_of(&ix, t, c, &["Self", "Name", "RowSpan", "ColumnSpan"]),
                        inner: if cn.empty { String::new() } else { t[cn.open_end..cn.close_start].to_owned() },
                    });
                }
                _ => x.other.push(t[cn.start..cn.end].to_owned()),
            }
        }
        Ok(x)
    }

    pub(super) fn write(&mut self, w: &mut Work<'_>) -> Result<(), EditError> {
        let ind = &self.indent;
        let a = |attrs: &[(String, String)]| attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", escape(v))).collect::<String>();
        let mut s = format!("<Table Self=\"{}\"{}>", self.id, a(&self.attrs));
        for o in &self.other {
            s.push_str(&format!("\n{ind}\t{o}"));
        }
        for (k, r) in self.rows.iter().enumerate() {
            s.push_str(&format!("\n{ind}\t<Row Self=\"{}Row{k}\" Name=\"{k}\"{} />", self.id, a(&r.attrs)));
        }
        for (k, c) in self.columns.iter().enumerate() {
            s.push_str(&format!("\n{ind}\t<Column Self=\"{}Column{k}\" Name=\"{k}\"{} />", self.id, a(&c.attrs)));
        }
        self.cells.sort_by_key(|c| (c.row, c.column));
        for c in &self.cells {
            s.push_str(&format!("\n{ind}\t<Cell Self=\"{}\" Name=\"{}:{}\" RowSpan=\"{}\" ColumnSpan=\"{}\"{}>{}</Cell>",
                c.id, c.column, c.row, c.rows, c.columns, a(&c.attrs), c.inner));
        }
        s.push_str(&format!("\n{ind}</Table>"));
        // the counts
        let header = get(&self.attrs, "HeaderRowCount").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let footer = get(&self.attrs, "FooterRowCount").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let body = self.rows.len().saturating_sub(header + footer);
        let s = s.replacen(&format!(" BodyRowCount=\"{}\"", get(&self.attrs, "BodyRowCount").unwrap_or("")), &format!(" BodyRowCount=\"{body}\""), 1)
            .replacen(&format!(" ColumnCount=\"{}\"", get(&self.attrs, "ColumnCount").unwrap_or("")), &format!(" ColumnCount=\"{}\"", self.columns.len()), 1);
        let part = self.part.clone();
        w.splice(&part, vec![(self.range.clone(), s)])?;
        if let Some(story) = self.story(w) {
            w.recompose(&story);
        }
        Ok(())
    }

    pub(super) fn story(&self, w: &mut Work<'_>) -> Option<String> {
        self.part.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).map(str::to_owned).or_else(|| {
            let t = w.text(&self.part).ok()?;
            let ix = Index::scan(t).ok()?;
            let i = ix.by_self(t, &self.id)?;
            ix.ancestors(i).find(|&a| ix.local(t, a) == "Story").and_then(|a| ix.attr(t, a, "Self")).map(str::to_owned)
        })
    }

    /// The next cell id: the table's Self + "i" + one more than the highest number used.
    pub(super) fn new_cell_id(&self, used: &mut u64) -> String {
        let prefix = format!("{}i", self.id);
        let max = self.cells.iter().filter_map(|c| c.id.strip_prefix(&prefix).and_then(|h| u64::from_str_radix(h, 16).ok())).max();
        let next = (*used).max(max.map_or(0, |m| m + 1));
        *used = next + 1;
        format!("{prefix}{next:x}")
    }

    /// The cell covering (column, row).
    pub(super) fn at(&self, column: usize, row: usize) -> Option<usize> {
        self.cells.iter().position(|c| (c.column..c.column + c.columns).contains(&column) && (c.row..c.row + c.rows).contains(&row))
    }

    pub(super) fn header(&self) -> usize {
        get(&self.attrs, "HeaderRowCount").and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// Rows [at, end) leave the table: their cells go, cells spanning into them lose those
    /// rows (one starting there starts at `at`), the header and footer counts follow.
    pub(super) fn remove_rows(&mut self, at: usize, end: usize) {
        let (h, f, n) = (self.header(), self.footer(), self.rows.len());
        let count = end - at;
        self.cells.retain(|c| {
            let cut = (c.row + c.rows).min(end).saturating_sub(c.row.max(at));
            cut < c.rows
        });
        for c in self.cells.iter_mut() {
            let (s, e) = (c.row, c.row + c.rows);
            c.rows -= e.min(end).saturating_sub(s.max(at));
            if s >= end {
                c.row -= count;
            } else if s >= at {
                c.row = at;
            }
        }
        self.rows.drain(at..end);
        let cut = |a: usize, b: usize| b.min(end).saturating_sub(a.max(at));
        let (hc, fc) = (cut(0, h), cut(n - f, n));
        if hc > 0 {
            put(&mut self.attrs, "HeaderRowCount", (h - hc).to_string());
        }
        if fc > 0 {
            put(&mut self.attrs, "FooterRowCount", (f - fc).to_string());
        }
    }
}

/// An empty cell like `like`: its attributes, an empty paragraph in its paragraph style.
pub(super) fn empty_like(like: &CellX, id: String, column: usize, row: usize, indent: &str) -> CellX {
    let style = |tag: &str, attr: &str, default: &str| -> String {
        like.inner.find(&format!("<{tag} ")).and_then(|i| {
            let rest = &like.inner[i..];
            let key = format!("{attr}=\"");
            let j = rest.find(&key)? + key.len();
            rest[j..].find('"').map(|e| rest[j..j + e].to_owned())
        }).unwrap_or_else(|| default.to_owned())
    };
    let ps = style("ParagraphStyleRange", "AppliedParagraphStyle", "ParagraphStyle/$ID/NormalParagraphStyle");
    CellX {
        id,
        column,
        row,
        columns: 1,
        rows: 1,
        attrs: like.attrs.clone(),
        inner: empty_cell_text(&ps, indent),
    }
}

pub(super) fn empty_cell_text(paragraph_style: &str, indent: &str) -> String {
    format!("\n{indent}\t\t<ParagraphStyleRange AppliedParagraphStyle=\"{}\">\n{indent}\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\" />\n{indent}\t\t</ParagraphStyleRange>\n{indent}\t",
        escape(paragraph_style))
}

/// Whether a cell holds text.
pub(super) fn has_text(c: &CellX) -> bool {
    c.inner.contains("<Content>") || c.inner.contains("<Table ") || c.inner.contains("<Rectangle ")
}

// ---------------------------------------------------------------------------------------
// commands

/// The table element's text for a table not yet in a story.
pub(super) fn render(x: &mut TableX) -> String {
    let ind = x.indent.clone();
    let a = |attrs: &[(String, String)]| attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", escape(v))).collect::<String>();
    let mut s = format!("<Table Self=\"{}\"{}>", x.id, a(&x.attrs));
    for (k, r) in x.rows.iter().enumerate() {
        s.push_str(&format!("\n{ind}\t<Row Self=\"{}Row{k}\" Name=\"{k}\"{} />", x.id, a(&r.attrs)));
    }
    for (k, c) in x.columns.iter().enumerate() {
        s.push_str(&format!("\n{ind}\t<Column Self=\"{}Column{k}\" Name=\"{k}\"{} />", x.id, a(&c.attrs)));
    }
    x.cells.sort_by_key(|c| (c.row, c.column));
    for c in &x.cells {
        s.push_str(&format!("\n{ind}\t<Cell Self=\"{}\" Name=\"{}:{}\" RowSpan=\"{}\" ColumnSpan=\"{}\"{}>{}</Cell>", c.id, c.column, c.row, c.rows, c.columns, a(&c.attrs), c.inner));
    }
    s.push_str(&format!("\n{ind}</Table>"));
    s
}

/// The Table menu's view of a table: header and footer rows, each row's height and growth,
/// each column's width and each cell's place, span and options as the IDML stores them
/// (absent: the default).
pub(crate) fn table_info(w: &mut Work<'_>, table: &str) -> Result<serde_json::Value, EditError> {
    let x = TableX::read(w, table)?;
    let num = |attrs: &[(String, String)], k: &str| get(attrs, k).and_then(|v| v.parse::<f64>().ok());
    let rows: Vec<serde_json::Value> = x.rows.iter().map(|r| serde_json::json!({
        "height": num(&r.attrs, "SingleRowHeight"),
        "minHeight": num(&r.attrs, "MinimumHeight"),
        "autoGrow": get(&r.attrs, "AutoGrow") != Some("false"),
        // Cell Options ▸ Rows and Columns ▸ Keep Options (tools-c2): Keep with Next Row, Start Row (IDML's StartRow, default Anywhere)
        "keepWithNext": get(&r.attrs, "KeepWithNextRow") == Some("true"),
        "start": get(&r.attrs, "StartRow").unwrap_or("Anywhere"),
    })).collect();
    let columns: Vec<Option<f64>> = x.columns.iter().map(|c| num(&c.attrs, "SingleColumnWidth")).collect();
    let cells: Vec<serde_json::Value> = x.cells.iter().map(|c| {
        let a = &c.attrs;
        let strokes: serde_json::Map<String, serde_json::Value> = ["Top", "Left", "Bottom", "Right"].iter().map(|s| ((*s).to_owned(), serde_json::json!({
            "weight": num(a, &format!("{s}EdgeStrokeWeight")),
            "color": get(a, &format!("{s}EdgeStrokeColor")),
        }))).collect();
        serde_json::json!({
            "id": c.id, "cell": [c.column, c.row], "span": [c.columns, c.rows],
            "style": get(a, "AppliedCellStyle"),
            "fill": get(a, "FillColor"), "fillTint": num(a, "FillTint"),
            "insets": (["Top", "Left", "Bottom", "Right"].map(|s| num(a, &format!("Text{s}Inset")))),
            "vertical": get(a, "VerticalJustification").unwrap_or("TopAlign"),
            "strokes": strokes,
            "graphic": get(a, "CellType") == Some("GraphicTypeCell"), "empty": !has_text(c),
            "clip": get(a, "ClipContentToCell") == Some("true"),
            "diagonal": DiagonalLines::info(a),
            "rotation": num(a, "RotationAngle").unwrap_or(0.0),
            "writingDirection": get(a, "WritingDirection") != Some("false"),
        })
    }).collect();
    // the alternating patterns (Table Options ▸ Row Strokes, Column Strokes, Fills), as the table has them now
    let options = super::tableopts::effective(w.ed.parts(), table);
    Ok(serde_json::json!({ "id": x.id, "header": x.header(), "footer": x.footer(), "style": get(&x.attrs, "AppliedTableStyle"), "rows": rows, "columns": columns, "cells": cells, "options": options }))
}
