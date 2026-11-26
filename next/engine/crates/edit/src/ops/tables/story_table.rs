//! Insert, convert and remove tables in their containing stories.

use super::*;

/// Table ▸ Insert Table at story position `at` (the story's end without one): `rows` body rows
/// and `header`/`footer` rows of `columns` columns sharing the frame's column width.
pub(in crate::ops) fn insert_table(w: &mut Work<'_>, story: &str, at: Option<usize>, rows: u32, columns: u32, header: u32, footer: u32) -> Result<(), EditError> {
    let story = &crate::text::story_of(w, story)?;
    // InDesign 2025's `Table.add` takes bodyRowCount 1-10000, columnCount 1-200, headerRowCount 0-25 and footerRowCount 0-25 (each its own
    // range, not their sum); measured in `next/oracle/measured/tablelimits-2025.json`, tested in `tests/tablelimit.rs`.
    if !(1..=MAX_BODY_ROWS).contains(&rows) || !(1..=MAX_COLUMNS).contains(&columns) || header > MAX_HEADER_FOOTER_ROWS || footer > MAX_HEADER_FOOTER_ROWS {
        return Err(EditError::new("invalid", format!("a table: 1 to {MAX_BODY_ROWS} body rows, 1 to {MAX_COLUMNS} columns, 0 to {MAX_HEADER_FOOTER_ROWS} header rows and 0 to {MAX_HEADER_FOOTER_ROWS} footer rows")));
    }
    let width = story_column_width(w, story)?;
    let at = match at {
        Some(a) => a,
        None => crate::text::contents_len(w, story)?,
    };
    let uid = w.new_id();
    let id = format!("{story}i{}", &uid[1..]);
    let (part, ix, i) = w.find(story)?;
    let t = w.text(&part)?.to_owned();
    let ps = paragraph_style_at(&ix, &t, i);
    let ind = format!("{}\t\t\t", indent_before(&t, ix.nodes[i].start));
    let insets = [("TextTopInset", "4"), ("TextLeftInset", "4"), ("TextBottomInset", "4"), ("TextRightInset", "4"), ("ClipContentToTextCell", "false")];
    let line = |extra: (&str, String)| Line { attrs: insets.iter().map(|(k, v)| (k.to_string(), v.to_string())).chain([(extra.0.to_owned(), extra.1)]).collect() };
    let n_rows = (rows + header + footer) as usize;
    let mut x = TableX {
        id: id.clone(),
        attrs: vec![("HeaderRowCount".into(), header.to_string()), ("FooterRowCount".into(), footer.to_string())].into_iter()
            .chain(insets.iter().map(|(k, v)| (k.to_string(), v.to_string())))
            .chain([("BodyRowCount".into(), rows.to_string()), ("ColumnCount".into(), columns.to_string()),
                ("AppliedTableStyle".into(), "TableStyle/$ID/[No table style]".into()), ("TableDirection".into(), "LeftToRightDirection".into())])
            .collect(),
        other: Vec::new(),
        rows: (0..n_rows).map(|_| line(("SingleRowHeight", "3".into()))).collect(),
        columns: (0..columns).map(|_| line(("SingleColumnWidth", geom::num(width / columns as f64)))).collect(),
        cells: Vec::new(),
        part: part.clone(),
        range: 0..0,
        indent: ind.clone(),
    };
    let cell_attrs: Vec<(String, String)> = [("CellType", "TextTypeCell")].iter().chain(insets.iter()).chain([("AppliedCellStyle", "CellStyle/$ID/[None]"), ("AppliedCellStylePriority", "0")].iter())
        .map(|(k, v)| (k.to_string(), v.to_string())).collect();
    // ids: the body rows first, then the header and the footer rows (as InDesign numbers them)
    let order: Vec<usize> = (header as usize..n_rows - footer as usize).chain(0..header as usize).chain(n_rows - footer as usize..n_rows).collect();
    let mut k = 0u64;
    for r in order {
        for c in 0..columns as usize {
            x.cells.push(CellX { id: format!("{id}i{k:x}"), column: c, row: r, columns: 1, rows: 1, attrs: cell_attrs.clone(), inner: empty_cell_text(&ps, &ind) });
            k += 1;
        }
    }
    let raw = render(&mut x);
    crate::text::replace_with_node(w, story, at, at, '\u{16}', &raw)
}

/// The paragraph style at the end of a story (its last paragraph range).
fn paragraph_style_at(ix: &Index, t: &str, story: usize) -> String {
    ix.named(t, story, "ParagraphStyleRange").last().and_then(|&p| ix.attr(t, p, "AppliedParagraphStyle")).unwrap_or("ParagraphStyle/$ID/NormalParagraphStyle").to_owned()
}

/// The column width of the story's first frame (`TextColumnFixedWidth`).
fn story_column_width(w: &mut Work<'_>, story: &str) -> Result<f64, EditError> {
    let pat = format!(" ParentStory=\"{story}\"");
    for name in w.parts_named("Spreads/").into_iter().chain(w.parts_named("MasterSpreads/")) {
        let t = w.text(&name)?;
        if !t.contains(&pat) {
            continue;
        }
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "TextFrame" && ix.attr(t, i, "ParentStory") == Some(story) && ix.attr(t, i, "PreviousTextFrame").is_none_or(|p| p == "n")
                && let Some(&p) = ix.named(t, i, "TextFramePreference").first()
                && let Some(v) = ix.attr(t, p, "TextColumnFixedWidth").and_then(|v| v.parse::<f64>().ok())
            {
                return Ok(v);
            }
        }
    }
    Err(EditError::new("unsupported", format!("{story} has no frame to size a table by")))
}

/// Table ▸ Convert Text to Table: story characters [start, end) (to the end without one), its
/// paragraphs the rows and its tabs the columns, the columns sharing the frame's column width.
pub(in crate::ops) fn convert_to_table(w: &mut Work<'_>, story: &str, start: usize, end: Option<usize>) -> Result<(), EditError> {
    let story = &crate::text::story_of(w, story)?;
    let end = match end {
        Some(e) => e,
        None => crate::text::contents_len(w, story)?,
    };
    let cells = crate::text::cells_of(w, story, start, end)?;
    let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
    if cells.is_empty() || columns == 0 {
        return Err(EditError::new("invalid", "no text to make a table of"));
    }
    let width = story_column_width(w, story)?;
    let uid = w.new_id();
    let id = format!("{story}i{}", &uid[1..]);
    let (part, ix, i) = w.find(story)?;
    let t = w.text(&part)?.to_owned();
    let ps = paragraph_style_at(&ix, &t, i);
    let ind = format!("{}\t\t\t", indent_before(&t, ix.nodes[i].start));
    let insets = [("TextTopInset", "4"), ("TextLeftInset", "4"), ("TextBottomInset", "4"), ("TextRightInset", "4"), ("ClipContentToTextCell", "false")];
    let line = |extra: (&str, String)| Line { attrs: insets.iter().map(|(k, v)| (k.to_string(), v.to_string())).chain([(extra.0.to_owned(), extra.1)]).collect() };
    let cell_attrs: Vec<(String, String)> = [("CellType", "TextTypeCell")].iter().chain(insets.iter()).chain([("AppliedCellStyle", "CellStyle/$ID/[None]"), ("AppliedCellStylePriority", "0")].iter())
        .map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let mut x = TableX {
        id: id.clone(),
        attrs: vec![("HeaderRowCount".into(), "0".into()), ("FooterRowCount".into(), "0".into())].into_iter()
            .chain(insets.iter().map(|(k, v)| (k.to_string(), v.to_string())))
            .chain([("BodyRowCount".into(), cells.len().to_string()), ("ColumnCount".into(), columns.to_string()),
                ("AppliedTableStyle".into(), "TableStyle/$ID/[No table style]".into()), ("TableDirection".into(), "LeftToRightDirection".into())])
            .collect(),
        other: Vec::new(),
        rows: (0..cells.len()).map(|_| line(("SingleRowHeight", "3".into()))).collect(),
        columns: (0..columns).map(|_| line(("SingleColumnWidth", geom::num(width / columns as f64)))).collect(),
        cells: Vec::new(),
        part,
        range: 0..0,
        indent: ind.clone(),
    };
    let mut k = 0u64;
    for (r, row) in cells.iter().enumerate() {
        for c in 0..columns {
            let inner = match row.get(c) {
                Some(xml) => format!("\n{ind}\t\t{xml}\n{ind}\t"),
                None => empty_cell_text(&ps, &ind),
            };
            x.cells.push(CellX { id: format!("{id}i{k:x}"), column: c, row: r, columns: 1, rows: 1, attrs: cell_attrs.clone(), inner });
            k += 1;
        }
    }
    let raw = render(&mut x);
    crate::text::replace_with_node(w, story, start, end, '\u{16}', &raw)
}

/// Table ▸ Convert Table to Text: the table becomes its cells' text, cells separated by tabs
/// and rows by paragraph ends, each cell's text keeping its formatting (tbl-to-text).
pub(in crate::ops) fn to_text(w: &mut Work<'_>, table: &str) -> Result<(), EditError> {
    let x = TableX::read(w, table)?;
    let story = x.story(w).ok_or_else(|| EditError::new("xml", "a table outside a story"))?;
    let at = crate::text::node_position(w, &story, table)?;
    let mut rows: Vec<Vec<String>> = vec![Vec::new(); x.rows.len()];
    let mut cells: Vec<&CellX> = x.cells.iter().collect();
    cells.sort_by_key(|c| (c.row, c.column));
    for c in cells {
        rows[c.row].push(c.id.clone());
    }
    crate::text::unnest(w, &story, at, &rows)
}

/// Table ▸ Delete ▸ Table.
pub(in crate::ops) fn delete_table(w: &mut Work<'_>, table: &str) -> Result<(), EditError> {
    let x = TableX::read(w, table)?;
    let story = x.story(w).ok_or_else(|| EditError::new("xml", "a table outside a story"))?;
    let at = crate::text::node_position(w, &story, table)?;
    crate::text::delete(w, &story, at, at + 1)
}
