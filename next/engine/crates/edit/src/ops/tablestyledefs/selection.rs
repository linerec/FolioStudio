use super::*;

pub(crate) fn create_from_selection(w: &mut Work<'_>, kind: &str, table: &str, cell: &[usize], name: &str, group: &str) -> Result<(), EditError> {
    let kind = match kind {
        "cell" => TableStyleKind::Cell,
        "table" => TableStyleKind::Table,
        _ => return Err(EditError::new("invalid", "style kind must be cell or table")),
    };
    if cell.len() != 2 { return Err(EditError::new("invalid", "cell needs column and row")); }
    let (props, _) = selected_properties(w, kind, table, cell)?;
    create(w, kind, name, &props, (!group.is_empty()).then_some(group))?;
    if kind == TableStyleKind::Cell && group.is_empty() {
        let (t, ix) = scan(w)?;
        let id = join_id(kind, &[], name).0;
        let base = props.get("BasedOn").map(String::as_str).unwrap_or(kind.none());
        if let (Some(new), Some(base)) = (ix.by_self(&t, &id), ix.by_self(&t, base))
            && ix.nodes[new].parent == ix.nodes[base].parent {
            let n = &ix.nodes[new];
            let at = ix.nodes[base].end;
            let node = t[n.start..n.end].to_owned();
            w.splice(PART, vec![(n.start..n.end, String::new()), (at..at, format!("\n\t\t{node}"))])?;
        }
    }
    let groups = if group.is_empty() { Vec::new() } else { super::super::tablestylegroups::group_path(kind, group) };
    w.created.push(join_id(kind, &groups, name).0);
    Ok(())
}

fn selected_properties(w: &Work<'_>, kind: TableStyleKind, table: &str, cell: &[usize]) -> Result<(BTreeMap<String, String>, String), EditError> {
    let (styles, sx) = scan(w)?;
    let order = order_of(kind, &styles, &sx);
    let mut props = BTreeMap::new();
    for (part, text) in &w.ed.parts {
        if !part.starts_with("Stories/") { continue; }
        let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
        let Some(tb) = ix.by_self(text, table).filter(|&i| ix.local(text, i) == "Table") else { continue };
        let node = if kind == TableStyleKind::Cell {
            let pos = format!("{}:{}", cell[0], cell[1]);
            ix.children(tb).iter().copied().find(|&i| ix.local(text, i) == "Cell" && ix.attr(text, i, "Name") == Some(&pos))
                .ok_or_else(|| EditError::new("notFound", "selected cell"))?
        } else { tb };
        for key in &order {
            if kind == TableStyleKind::Table && (key.starts_with("Text") || key == "ClipContentToTextCell") { continue; }
            if let Some(value) = ix.value(text, node, key) { props.insert((*key).to_owned(), value); }
        }
        if kind == TableStyleKind::Cell
            && let Some(base) = ix.value(text, node, "AppliedCellStyle") { props.insert("BasedOn".into(), base); }
        let applied = ix.value(text, node, if kind == TableStyleKind::Cell { "AppliedCellStyle" } else { "AppliedTableStyle" }).unwrap_or_else(|| kind.none().to_owned());
        return Ok((props, applied));
    }
    Err(EditError::new("notFound", "selected table"))
}

pub(crate) fn redefine_table(w: &mut Work<'_>, table: &str) -> Result<(), EditError> {
    let (props, style) = selected_properties(w, TableStyleKind::Table, table, &[])?;
    edit(w, TableStyleKind::Table, &style, None, &props)?;
    super::super::tables::styles::clear_table_overrides(w, table)
}
