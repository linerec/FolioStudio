//! Style groups, moving and duplicating of cell styles and table styles (oracle `tablestyles` cs-duplicate, ts-duplicate,
//! cs-group-*). Measured with the DOM's `cellStyleGroups.add`, `style.move(AT_END, group)`, `group.name`, `group.remove()` and
//! `style.duplicate()`:
//! - A group is a `CellStyleGroup` / `TableStyleGroup` element at the end of its parent (the kind's root group, or a group), empty
//!   ones as `<CellStyleGroup Self="CellStyleGroup/$ID/Grp" Name="$ID/Grp" />`. A group in a group joins the names: `Self`
//!   `CellStyleGroup/$ID/Grp%3aSub`, `Name` `$ID/Grp:Sub` (cs-group-new, cs-group-nested).
//! - A style in a group is named with the groups before it: `Self` `CellStyle/Grp%3aSub%3aName`, `Name` `Grp:Sub:Name`. Moving a style
//!   into a group takes the element out of where it was and puts it at the end of the group; every reference follows (the cells'
//!   `AppliedCellStyle`, `BasedOn` of other styles, the region cell styles of table styles) (cs-group-move, cs-group-nested, ts-group-move).
//! - Renaming a group renames the groups and the styles in it, and the references to them (cs-group-rename).
//! - Duplicate Style makes `<Name> copy` right after the style, with the same settings and base (cs-duplicate, ts-duplicate).
//! - Removing a group that holds cell styles takes the styles away with no replacement (`group.remove()`, cs-group-delete): the cells
//!   that had one get `[None]` and keep what it gave as their own settings (fill, edge strokes with the cells across a shared edge,
//!   vertical justification, in InDesign's order), the styles based on it take those settings in and its base. Table styles in a group
//!   are not measured: only an empty table style group is deleted.
use super::tablestyledefs::*;
use super::*;

const PART: &str = "Resources/Styles.xml";

/// The element of a group (`None`: the kind's root group, which holds the styles and groups with no group).
pub(super) fn group_node(t: &str, ix: &Index, kind: TableStyleKind, group: Option<&str>) -> Result<usize, EditError> {
    match group {
        None => (0..ix.nodes.len()).find(|&i| ix.local(t, i) == kind.group()).ok_or_else(|| EditError::new("notFound", kind.group())),
        Some(id) => ix.by_self(t, id).filter(|&i| ix.local(t, i) == kind.group_tag()).ok_or_else(|| EditError::new("notFound", format!("style group {id}"))),
    }
}

/// The group names of a group's `Self` (`CellStyleGroup/$ID/Grp%3aSub`: `Grp`, `Sub`).
pub(super) fn group_path(kind: TableStyleKind, id: &str) -> Vec<String> {
    let rest = id.strip_prefix(&format!("{}/", kind.group_tag())).unwrap_or(id);
    rest.strip_prefix("$ID/").unwrap_or(rest).split("%3a").map(str::to_owned).collect()
}

fn group_attrs(kind: TableStyleKind, path: &[String]) -> (String, String) {
    (format!("{}/$ID/{}", kind.group_tag(), path.join("%3a")), format!("$ID/{}", path.join(":")))
}

/// The edit that adds `child` (given the indent of the group's children) as the last child of the group element `g`.
pub(crate) fn append(t: &str, ix: &Index, g: usize, child: &dyn Fn(&str) -> String, style: Option<&str>) -> (std::ops::Range<usize>, String) {
    let n = &ix.nodes[g];
    let own = indent_before(t, n.start).to_owned();
    let inner = format!("{own}\t");
    // a style goes after the styles of the group, before its groups (cs-group-out: a style moved out stands before the group that is there)
    if let Some(last) = style.and_then(|tag| ix.children(g).iter().rev().find(|&&c| ix.local(t, c) == tag)) {
        let at = ix.nodes[*last].end;
        return (at..at, format!("\n{inner}{}", child(&inner)));
    }
    // a group with groups but no styles: the style stands before its groups (InDesign writes the styles of a group first, os-group-mixed,
    // sgd-para-nested)
    if let Some(&first) = style.and_then(|tag| ix.children(g).iter().find(|&&c| ix.local(t, c) != tag)) {
        let at = t[..ix.nodes[first].start].rfind('\n').map_or(ix.nodes[first].start, |p| p + 1);
        return (at..at, format!("{inner}{}\n", child(&inner)));
    }
    if n.empty {
        let open = t[n.start..n.open_end].trim_end_matches("/>").trim_end();
        (n.start..n.end, format!("{open}>\n{inner}{}\n{own}</{}>", child(&inner), t[n.name.clone()].to_owned()))
    } else {
        let at = n.close_start;
        let before = t[..at].rfind('\n').map_or(at, |p| p + 1);
        // the closing tag is on a line of its own, unless something Folio wrote before (a style made new) stands in front of it
        if !t[before..at].trim().is_empty() {
            return (at..at, format!("\n{inner}{}\n{own}", child(&inner)));
        }
        (before..before, format!("{inner}{}\n", child(&inner)))
    }
}

/// The edit that takes the element `i` away (its line with it); a group left with nothing in it becomes an empty element again.
pub(crate) fn remove(t: &str, ix: &Index, group_tag: &str, i: usize) -> (std::ops::Range<usize>, String) {
    let n = &ix.nodes[i];
    if let Some(p) = n.parent.filter(|&p| ix.local(t, p) == group_tag && ix.children(p).len() == 1) {
        let g = &ix.nodes[p];
        return (g.start..g.end, format!("{} />", t[g.start..g.open_end - 1].trim_end()));
    }
    let line = t[..n.start].rfind('\n').unwrap_or(n.start);
    (line..n.end, String::new())
}

/// New Style Group (`parent`: the group it goes in, none for the top level).
pub(super) fn create_group(w: &mut Work<'_>, kind: TableStyleKind, name: &str, parent: Option<&str>) -> Result<(), EditError> {
    valid_name(name)?;
    let (t, ix) = scan(w)?;
    let at = group_node(&t, &ix, kind, parent)?;
    let mut path = parent.map(|p| group_path(kind, p)).unwrap_or_default();
    path.push(name.to_owned());
    let (id, full) = group_attrs(kind, &path);
    if ix.by_self(&t, &id).is_some() {
        return Err(EditError::new("invalid", format!("a style group named {name} exists")));
    }
    let tag = kind.group_tag();
    let edit = append(&t, &ix, at, &|_| format!("<{tag} Self=\"{}\" Name=\"{}\" />", escape(&id), escape(&full)), None);
    w.splice(PART, vec![edit])
}

/// Rename Style Group: the groups and styles in it and the references to them follow.
pub(super) fn rename_group(w: &mut Work<'_>, kind: TableStyleKind, group: &str, name: &str) -> Result<(), EditError> {
    valid_name(name)?;
    let (t, ix) = scan(w)?;
    let g = group_node(&t, &ix, kind, Some(group))?;
    let old = group_path(kind, group);
    let depth = old.len() - 1;
    let mut path = old.clone();
    path[depth] = name.to_owned();
    if group_attrs(kind, &path).0 != group && ix.by_self(&t, &group_attrs(kind, &path).0).is_some() {
        return Err(EditError::new("invalid", format!("a style group named {name} exists")));
    }
    let mut edits = Vec::new();
    let mut moved = Vec::new();
    for i in std::iter::once(g).chain((0..ix.nodes.len()).filter(|&i| ix.ancestors(i).any(|a| a == g))) {
        let local = ix.local(&t, i);
        let Some(id) = ix.attr(&t, i, "Self") else { continue };
        if local == kind.group_tag() {
            let mut p = group_path(kind, id);
            p[depth] = name.to_owned();
            let (id, full) = group_attrs(kind, &p);
            edits.push(attr_edit(&ix, &t, i, "Self", &id));
            edits.push(attr_edit(&ix, &t, i, "Name", &full));
        } else if local == kind.tag() {
            let (mut groups, leaf) = split_id(kind, id);
            groups[depth] = name.to_owned();
            let (new_id, full) = join_id(kind, &groups, &leaf);
            edits.push(attr_edit(&ix, &t, i, "Self", &new_id));
            edits.push(attr_edit(&ix, &t, i, "Name", &full));
            moved.push((id.to_owned(), new_id));
        }
    }
    w.splice(PART, edits)?;
    for (from, to) in moved {
        replace_references(w, kind, &from, &to, None)?;
    }
    Ok(())
}

/// Delete Style Group. A group with cell styles in it takes them away with no replacement (cs-group-delete): the cells that had them keep
/// their settings, the styles based on them take the settings and their base. (Table styles in a group are not measured: not deleted.)
pub(super) fn delete_group(w: &mut Work<'_>, kind: TableStyleKind, group: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let g = group_node(&t, &ix, kind, Some(group))?;
    let styles: Vec<String> = (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == kind.tag() && ix.ancestors(i).any(|a| a == g))
        .filter_map(|i| ix.attr(&t, i, "Self").map(str::to_owned)).collect();
    if !styles.is_empty() {
        if kind != TableStyleKind::Cell {
            return Err(EditError::new("unsupported", "a table style group with styles in it is not deleted (move the styles out first)"));
        }
        for style in &styles {
            release_cell_style(w, style)?;
        }
    }
    let (t, ix) = scan(w)?;
    let g = group_node(&t, &ix, kind, Some(group))?;
    w.splice(PART, vec![remove(&t, &ix, kind.group_tag(), g)])
}

/// A cell style that is going away: its cells keep what it gave, the styles based on it take it in, region styles name `[None]`.
fn release_cell_style(w: &mut Work<'_>, style: &str) -> Result<(), EditError> {
    let kind = TableStyleKind::Cell;
    for (table, cells) in users(w, kind, style) {
        tables::styles::keep_cell_style(w, &table, &cells, style)?;
    }
    let (t, ix) = scan(w)?;
    let i = style_node(&t, &ix, kind, style)?;
    let base = based_on(&t, &ix, i, kind);
    let given = tables::styles::given(w, style, "CellStyle/");
    let dependents: Vec<String> = (0..ix.nodes.len()).filter(|&j| j != i && ix.local(&t, j) == kind.tag() && based_on(&t, &ix, j, kind) == style)
        .filter_map(|j| ix.attr(&t, j, "Self").map(str::to_owned)).collect();
    let order = cell_order();
    for d in dependents {
        let (t, ix) = scan(w)?;
        let Some(j) = ix.by_self(&t, &d) else { continue };
        let own = element_attrs(&t, &ix, j);
        let mut props: std::collections::BTreeMap<String, String> = given.iter().filter(|(k, _)| order.contains(k) && !own.iter().any(|(n, _)| n == *k)).map(|(k, v)| (k.clone(), v.clone())).collect();
        props.insert("BasedOn".into(), base.clone());
        edit(w, kind, &d, None, &props)?;
    }
    replace_references(w, kind, style, kind.none(), None)
}

/// The element of style `i` as it is written in `groups` under `leaf` (its new `Self` and a function that writes it at an indent).
fn rebuilt(kind: TableStyleKind, t: &str, ix: &Index, i: usize, groups: &[String], leaf: &str, copy: bool) -> (String, Box<dyn Fn(&str) -> String>) {
    let (id, full) = join_id(kind, groups, leaf);
    let mut attrs = element_attrs(t, ix, i);
    set(&mut attrs, "Self", id.clone());
    set(&mut attrs, "Name", full);
    if copy {
        set(&mut attrs, "ExtendedKeyboardShortcut", "0 0 0".to_owned());
        set(&mut attrs, "KeyboardShortcut", "0 0".to_owned());
    }
    let order = order_of(kind, t, ix);
    let based = based_on(t, ix, i, kind);
    let extra_nodes: Vec<String> = ix.children(i).iter().filter(|&&c| ix.local(t, c) != "Properties").map(|&c| t[ix.nodes[c].start..ix.nodes[c].end].to_owned()).collect();
    (id, Box::new(move |indent: &str| {
        let extra: String = extra_nodes.iter().map(|e| format!("\n{indent}\t{e}")).collect();
        render(kind, indent, &attrs, &based, &extra, &order)
    }))
}

fn style_node(t: &str, ix: &Index, kind: TableStyleKind, style: &str) -> Result<usize, EditError> {
    if style.contains("/$ID/") {
        return Err(EditError::new("unsupported", "a built-in style is not moved or duplicated"));
    }
    ix.by_self(t, style).filter(|&i| ix.local(t, i) == kind.tag()).ok_or_else(|| EditError::new("notFound", format!("style {style}")))
}

/// Move a style into a group (`None`: out of its group, to the top level), at the end of it.
pub(super) fn move_style(w: &mut Work<'_>, kind: TableStyleKind, style: &str, group: Option<&str>) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let i = style_node(&t, &ix, kind, style)?;
    let target = group_node(&t, &ix, kind, group)?;
    let groups = group.map(|g| group_path(kind, g)).unwrap_or_default();
    let (old_groups, leaf) = split_id(kind, style);
    if old_groups == groups {
        return Ok(());
    }
    let (new_id, element) = rebuilt(kind, &t, &ix, i, &groups, &leaf, false);
    if ix.by_self(&t, &new_id).is_some() {
        return Err(EditError::new("invalid", format!("a {} named {leaf} exists there", kind.tag())));
    }
    let (take, put) = (remove(&t, &ix, kind.group_tag(), i), append(&t, &ix, target, &*element, Some(kind.tag())));
    // the two edits touch different elements, except for a group that is emptied and filled: take first, then put into the new text
    if put.0.start >= take.0.end || put.0.end <= take.0.start {
        w.splice(PART, vec![take, put])?;
    } else {
        w.splice(PART, vec![take])?;
        let (t, ix) = scan(w)?;
        let target = group_node(&t, &ix, kind, group)?;
        w.splice(PART, vec![append(&t, &ix, target, &*element, Some(kind.tag()))])?;
    }
    replace_references(w, kind, style, &new_id, None)?;
    recompose_all(w);
    Ok(())
}

/// Duplicate Style: `name` (or `<Name> copy`) next to the style.
pub(super) fn duplicate(w: &mut Work<'_>, kind: TableStyleKind, style: &str, name: Option<&str>) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let i = style_node(&t, &ix, kind, style)?;
    let (groups, leaf) = split_id(kind, style);
    let name = name.map(str::to_owned).unwrap_or_else(|| format!("{leaf} copy"));
    valid_name(&name)?;
    let (new_id, element) = rebuilt(kind, &t, &ix, i, &groups, &name, true);
    if ix.by_self(&t, &new_id).is_some() {
        return Err(EditError::new("invalid", format!("a {} named {name} exists", kind.tag())));
    }
    // the copy stands right after the style (cs-duplicate)
    let indent = indent_before(&t, ix.nodes[i].start).to_owned();
    let at = ix.nodes[i].end;
    w.splice(PART, vec![(at..at, format!("
{indent}{}", element(&indent)))])
}

fn recompose_all(w: &mut Work<'_>) {
    let stories: Vec<String> = w.ed.parts.keys().filter_map(|n| n.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).map(str::to_owned)).collect();
    for story in stories {
        w.recompose(&story);
    }
}
