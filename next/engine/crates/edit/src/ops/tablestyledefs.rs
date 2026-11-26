//! New, edit (with rename) and delete of a cell style or a table style (oracle `tablestyles` cs-create*, cs-edit*, cs-delete*,
//! ts-create, ts-edit, ts-delete). Measured with the DOM's `cellStyles.add`, `tableStyles.add`, the properties of a style and
//! `remove(replacement)`:
//! - A style is a `CellStyle` / `TableStyle` element in the group of its kind in `Resources/Styles.xml`, new ones at the end of it:
//!   `Self` `CellStyle/Name`, `ExtendedKeyboardShortcut="0 0 0"`, the settings it gives (nothing else: a setting the style does not
//!   give is inherited), `KeyboardShortcut="0 0"` and `Name`; a cell style always names its paragraph style (`[No paragraph style]`
//!   when it gives none). The attributes come in InDesign's order (the order of the DOM's properties). `<Properties><BasedOn>` names
//!   the style it is based on, `$ID/[None]` / `$ID/[No table style]` (a string) when none.
//! - Setting a region's cell style in a table style also writes `<Region>RegionSameAsBodyRegion="false"` (ts-create).
//! - Renaming changes `Self` and `Name`, and the references to it: the cells' `AppliedCellStyle`, the tables' `AppliedTableStyle`,
//!   `BasedOn` of other styles and the region cell styles of table styles (cs-edit-rename).
//! - Deleting a cell style gives its cells the replacement as if it were applied to them (cs-delete, cs-delete-none: the priority is
//!   worked out again and the replacement's paragraph style put on their paragraphs); the same for the tables of a table style.
//!   Styles based on it become based on the replacement. Deleting a style that is in use with the DOM's `remove()` crashed InDesign for
//!   text variables and was not tried for these (the replacement path is the one the DOM takes for a style that has a replacement).
use super::*;
use std::collections::BTreeMap;

const PART: &str = "Resources/Styles.xml";
const NO_PARAGRAPH: &str = "ParagraphStyle/$ID/[No paragraph style]";
pub(crate) mod selection;

/// A cell style or a table style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableStyleKind {
    Cell,
    Table,
}

impl TableStyleKind {
    pub(super) fn tag(self) -> &'static str {
        if self == TableStyleKind::Cell { "CellStyle" } else { "TableStyle" }
    }
    pub(super) fn group(self) -> &'static str {
        if self == TableStyleKind::Cell { "RootCellStyleGroup" } else { "RootTableStyleGroup" }
    }
    pub(super) fn none(self) -> &'static str {
        if self == TableStyleKind::Cell { "CellStyle/$ID/[None]" } else { "TableStyle/$ID/[No table style]" }
    }
    pub(super) fn group_tag(self) -> &'static str {
        if self == TableStyleKind::Cell { "CellStyleGroup" } else { "TableStyleGroup" }
    }
    pub(super) fn root_name(self) -> &'static str {
        if self == TableStyleKind::Cell { "$ID/[None]" } else { "$ID/[No table style]" }
    }
}

/// InDesign's order for the attributes of a cell style (the order of the DOM's properties).
pub(super) fn cell_order() -> Vec<String> {
    let mut o: Vec<String> = ["GraphicLeftInset", "GraphicTopInset", "GraphicRightInset", "GraphicBottomInset", "ClipContentToGraphicCell", "TextTopInset", "TextLeftInset", "TextBottomInset", "TextRightInset",
        "ClipContentToTextCell", "AppliedParagraphStyle", "GradientFillLength", "GradientFillAngle", "GradientFillStart", "TopInset", "LeftInset", "BottomInset", "RightInset", "FillColor", "FillTint",
        "OverprintFill", "TopLeftDiagonalLine", "TopRightDiagonalLine", "DiagonalLineInFront", "DiagonalLineStrokeWeight", "DiagonalLineStrokeType", "DiagonalLineStrokeColor", "DiagonalLineStrokeTint",
        "DiagonalLineStrokeOverprint", "DiagonalLineStrokeGapColor", "DiagonalLineStrokeGapTint", "DiagonalLineStrokeGapOverprint", "ClipContentToCell", "FirstBaselineOffset", "VerticalJustification",
        "ParagraphSpacingLimit", "MinimumFirstBaselineOffset", "RotationAngle"].iter().map(|s| (*s).to_owned()).collect();
    for side in ["Left", "Top", "Right", "Bottom"] {
        for p in ["Weight", "Type", "Color", "Tint", "Overprint", "GapColor", "GapTint", "GapOverprint"] {
            o.push(format!("{side}EdgeStroke{p}"));
        }
    }
    o
}

/// The attributes of the document's root table style, in its order (it holds every setting a table style can give).
fn table_order(t: &str, ix: &Index) -> Vec<String> {
    let Some(root) = ix.by_self(t, TableStyleKind::Table.none()) else { return Vec::new() };
    ix.attrs(t, root).iter().map(|a| t[a.name.0..a.name.1].to_owned()).filter(|n| n != "Self" && n != "Name").collect()
}

fn settings_ok(kind: TableStyleKind, key: &str, order: &[String]) -> bool {
    key == "BasedOn" || order.iter().any(|o| o == key) || (kind == TableStyleKind::Table && key.ends_with("RegionSameAsBodyRegion"))
}

fn check_value(key: &str, value: &str) -> Result<(), EditError> {
    let number = key.ends_with("Weight") || key.ends_with("Tint") || key.ends_with("Inset") || key.ends_with("Count") || key.ends_with("Offset") || key.starts_with("Space") || key.ends_with("Angle");
    if number && key != "FirstBaselineOffset" && !value.is_empty() && value.parse::<f64>().map_or(true, |v| !v.is_finite()) {
        return Err(EditError::new("invalid", format!("{key}: a number")));
    }
    Ok(())
}

pub(super) fn element_attrs(t: &str, ix: &Index, i: usize) -> Vec<(String, String)> {
    ix.attrs(t, i).iter().map(|a| (t[a.name.0..a.name.1].to_owned(), crate::xml::unescape(&t[a.value.0..a.value.1]))).collect()
}

/// The element's text: attributes in `order`, the other children kept, `BasedOn` as given.
pub(super) fn render(kind: TableStyleKind, indent: &str, attrs: &[(String, String)], based: &str, extra: &str, order: &[String]) -> String {
    let rank = |name: &str| -> (u8, usize) {
        match name {
            "Self" => (0, 0),
            "ExtendedKeyboardShortcut" => (1, 0),
            "Name" if kind == TableStyleKind::Table => (2, 0),
            "KeyboardShortcut" => (9, 0),
            "Name" => (10, 0),
            n => (5, order.iter().position(|o| o == n).unwrap_or(order.len())),
        }
    };
    let mut sorted: Vec<&(String, String)> = attrs.iter().collect();
    sorted.sort_by_key(|(k, _)| rank(k));
    // a table style names its region styles after the settings: the flags first
    let tag = kind.tag();
    let mut s = format!("<{tag}");
    for (k, v) in sorted {
        s.push_str(&format!(" {k}=\"{}\"", escape(v)));
    }
    s.push('>');
    let based = if based.is_empty() || based == kind.none() {
        format!("<BasedOn type=\"string\">{}</BasedOn>", kind.root_name())
    } else {
        format!("<BasedOn type=\"object\">{}</BasedOn>", escape(based))
    };
    s.push_str(&format!("\n{indent}\t<Properties>\n{indent}\t\t{based}\n{indent}\t</Properties>{extra}\n{indent}</{tag}>"));
    s
}

pub(super) fn based_on(t: &str, ix: &Index, i: usize, kind: TableStyleKind) -> String {
    ix.path(t, i, &["Properties", "BasedOn"]).map(|b| {
        let n = &ix.nodes[b];
        let name = crate::xml::unescape(t[n.open_end..n.close_start].trim());
        if name.starts_with(&format!("{}/", kind.tag())) { name } else { format!("{}/{name}", kind.tag()) }
    }).unwrap_or_else(|| kind.none().to_owned())
}

pub(super) fn scan(w: &Work<'_>) -> Result<(String, Index), EditError> {
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    Ok((t, ix))
}

pub(super) fn order_of(kind: TableStyleKind, t: &str, ix: &Index) -> Vec<String> {
    if kind == TableStyleKind::Cell { cell_order() } else { table_order(t, ix) }
}

fn apply_props(kind: TableStyleKind, attrs: &mut Vec<(String, String)>, props: &BTreeMap<String, String>, based: &mut Option<String>, order: &[String]) -> Result<(), EditError> {
    for (key, value) in props {
        if !settings_ok(kind, key, order) {
            return Err(EditError::new("invalid", format!("{key} is not a setting of a {}", kind.tag())));
        }
        check_value(key, value)?;
        if key == "BasedOn" {
            *based = Some(value.clone());
            continue;
        }
        // a region's cell style makes the region its own
        if kind == TableStyleKind::Table && key.ends_with("RegionCellStyle") && !key.starts_with("Body") {
            let flag = key.replace("CellStyle", "SameAsBodyRegion");
            set(attrs, &flag, if value.is_empty() || value == TableStyleKind::Cell.none() { "true" } else { "false" }.to_owned());
        }
        if value.is_empty() {
            attrs.retain(|(k, _)| k != key);
        } else {
            set(attrs, key, value.clone());
        }
        // the text insets go with the insets (a cell style gives both)
        if kind == TableStyleKind::Cell && key.ends_with("Inset") && !key.starts_with("Text") && !key.starts_with("Graphic") && !value.is_empty() {
            set(attrs, &format!("Text{key}"), value.clone());
        }
    }
    Ok(())
}

pub(super) fn set(attrs: &mut Vec<(String, String)>, key: &str, value: String) {
    match attrs.iter_mut().find(|(k, _)| k == key) {
        Some(a) => a.1 = value,
        None => attrs.push((key.to_owned(), value)),
    }
}

pub(super) fn valid_name(name: &str) -> Result<(), EditError> {
    if name.trim().is_empty() || name.chars().any(|c| c.is_control() || c == '/' || c == ':') {
        return Err(EditError::new("invalid", "a style name is not empty and has no slash or colon"));
    }
    Ok(())
}

/// `CellStyle/Grp%3aSub%3aName` is the style `Name` in the group `Grp`, in its group `Sub`: the groups and the style's own name.
pub(super) fn split_id(kind: TableStyleKind, id: &str) -> (Vec<String>, String) {
    let rest = id.strip_prefix(&format!("{}/", kind.tag())).unwrap_or(id);
    let mut parts: Vec<String> = rest.split("%3a").map(str::to_owned).collect();
    let leaf = parts.pop().unwrap_or_default();
    (parts, leaf)
}

/// The `Self` and the `Name` of the style `leaf` in `groups` (the group names go before it, joined by `%3a` / `:`).
pub(super) fn join_id(kind: TableStyleKind, groups: &[String], leaf: &str) -> (String, String) {
    let mut id = format!("{}/", kind.tag());
    let mut name = String::new();
    for g in groups {
        id.push_str(&format!("{g}%3a"));
        name.push_str(&format!("{g}:"));
    }
    id.push_str(leaf);
    name.push_str(leaf);
    (id, name)
}

/// Cell Styles / Table Styles ▸ New Style.
pub(super) fn create(w: &mut Work<'_>, kind: TableStyleKind, name: &str, props: &BTreeMap<String, String>, group: Option<&str>) -> Result<(), EditError> {
    valid_name(name)?;
    let (t, ix) = scan(w)?;
    let target = super::tablestylegroups::group_node(&t, &ix, kind, group)?;
    let groups = group.map(|g| super::tablestylegroups::group_path(kind, g)).unwrap_or_default();
    let (id, full_name) = join_id(kind, &groups, name);
    if ix.by_self(&t, &id).is_some() {
        return Err(EditError::new("invalid", format!("a {} named {name} exists", kind.tag())));
    }
    let order = order_of(kind, &t, &ix);
    let mut attrs: Vec<(String, String)> = vec![("Self".into(), id.clone()), ("ExtendedKeyboardShortcut".into(), "0 0 0".into()), ("KeyboardShortcut".into(), "0 0".into()), ("Name".into(), full_name)];
    if kind == TableStyleKind::Cell {
        attrs.push(("AppliedParagraphStyle".into(), NO_PARAGRAPH.into()));
    }
    let mut based = None;
    apply_props(kind, &mut attrs, props, &mut based, &order)?;
    if let Some(b) = &based
        && b != kind.none() && ix.by_self(&t, b).is_none() {
        return Err(EditError::new("notFound", format!("style {b}")));
    }
    // new styles go at the end of their group (the group's closing tag stands on its own line: the new element goes above it)
    let edit = super::tablestylegroups::append(&t, &ix, target, &|indent| render(kind, indent, &attrs, based.as_deref().unwrap_or(""), "", &order), Some(kind.tag()));
    w.splice(PART, vec![edit])
}

/// What a `Self` reference to the style `from` becomes, in every part: `to`.
pub(super) fn replace_references(w: &mut Work<'_>, kind: TableStyleKind, from: &str, to: &str, fallback: Option<&str>) -> Result<(), EditError> {
    let parts: Vec<String> = w.ed.parts.iter().filter(|(n, t)| n.ends_with(".xml") && t.contains(from)).map(|(n, _)| n.clone()).collect();
    for part in parts {
        let t = w.text(&part)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for i in 0..ix.nodes.len() {
            for a in ix.attrs(&t, i) {
                let key = &t[a.name.0..a.name.1];
                // the cells' and tables' own references were replaced as an application (delete) or follow the rename
                let applied = fallback.is_none() && ((key == "AppliedCellStyle" && kind == TableStyleKind::Cell) || (key == "AppliedTableStyle" && kind == TableStyleKind::Table));
                let region = key.ends_with("RegionCellStyle") && kind == TableStyleKind::Cell;
                if (applied || region) && crate::xml::unescape(&t[a.value.0..a.value.1]) == from {
                    edits.push((a.value.0..a.value.1, escape(to)));
                }
            }
            if ix.local(&t, i) == "BasedOn" && ix.attr(&t, i, "type") == Some("object") {
                let n = &ix.nodes[i];
                if !n.empty && crate::xml::unescape(t[n.open_end..n.close_start].trim()) == from
                    && let Some(owner) = ix.ancestors(i).find(|&p| ix.local(&t, p) == kind.tag()) {
                    // a replacement based on the deleted style takes the deleted style's own base
                    match fallback.filter(|_| ix.attr(&t, owner, "Self") == Some(escape(to).as_str())) {
                        Some(base) if base == kind.none() => edits.push((n.start..n.end, format!("<BasedOn type=\"string\">{}</BasedOn>", kind.root_name()))),
                        Some(base) => edits.push((n.open_end..n.close_start, escape(base))),
                        None if to == kind.none() => edits.push((n.start..n.end, format!("<BasedOn type=\"string\">{}</BasedOn>", kind.root_name()))),
                        None => edits.push((n.open_end..n.close_start, escape(to))),
                    }
                }
            }
        }
        if !edits.is_empty() {
            w.splice(&part, edits)?;
        }
    }
    Ok(())
}

/// Cell Styles / Table Styles ▸ Style Options: the style `style` with `props` over what it gives, renamed to `rename`.
pub(super) fn edit(w: &mut Work<'_>, kind: TableStyleKind, style: &str, rename: Option<&str>, props: &BTreeMap<String, String>) -> Result<(), EditError> {
    if style.contains("/$ID/") {
        return Err(EditError::new("unsupported", "a built-in style is not edited"));
    }
    let (t, ix) = scan(w)?;
    let i = ix.by_self(&t, style).filter(|&i| ix.local(&t, i) == kind.tag()).ok_or_else(|| EditError::new("notFound", format!("style {style}")))?;
    let order = order_of(kind, &t, &ix);
    let mut attrs = element_attrs(&t, &ix, i);
    let mut based = Some(based_on(&t, &ix, i, kind));
    apply_props(kind, &mut attrs, props, &mut based, &order)?;
    if let Some(b) = based.as_deref() {
        if b == style {
            return Err(EditError::new("invalid", "a style cannot be based on itself"));
        }
        if b != kind.none() && ix.by_self(&t, b).is_none() {
            return Err(EditError::new("notFound", format!("style {b}")));
        }
        // no loop through the styles it is based on
        let mut cur = b.to_owned();
        for _ in 0..32 {
            let Some(c) = ix.by_self(&t, &cur) else { break };
            let next = based_on(&t, &ix, c, kind);
            if next == style {
                return Err(EditError::new("invalid", "cyclic style inheritance"));
            }
            if next == kind.none() {
                break;
            }
            cur = next;
        }
    }
    let mut new_id = style.to_owned();
    if let Some(n) = rename.filter(|n| !n.is_empty()) {
        valid_name(n)?;
        let (groups, _) = split_id(kind, style);
        new_id = join_id(kind, &groups, n).0;
        if new_id != style && ix.by_self(&t, &new_id).is_some() {
            return Err(EditError::new("invalid", format!("a {} named {n} exists", kind.tag())));
        }
        set(&mut attrs, "Self", new_id.clone());
        set(&mut attrs, "Name", join_id(kind, &split_id(kind, style).0, n).1);
    }
    let extra: String = ix.children(i).iter().filter(|&&c| ix.local(&t, c) != "Properties").map(|&c| format!("\n{}\t{}", indent_before(&t, ix.nodes[i].start), &t[ix.nodes[c].start..ix.nodes[c].end])).collect();
    let indent = indent_before(&t, ix.nodes[i].start).to_owned();
    let mut element = render(kind, &indent, &attrs, based.as_deref().unwrap_or(""), &extra, &order);
    if let (Some(properties), Some(old_base)) = (ix.path(&t, i, &["Properties"]), ix.path(&t, i, &["Properties", "BasedOn"])) {
        let updated = Index::scan(&element).map_err(|e| EditError::new("xml", e))?;
        let new_properties = updated.path(&element, 0, &["Properties"]).unwrap();
        let new_base = updated.path(&element, 0, &["Properties", "BasedOn"]).unwrap();
        let p = &ix.nodes[properties];
        let b = &ix.nodes[old_base];
        let n = &updated.nodes[new_base];
        let mut preserved = t[p.start..p.end].to_owned();
        preserved.replace_range(b.start-p.start..b.end-p.start, &element[n.start..n.end]);
        let p = &updated.nodes[new_properties];
        element.replace_range(p.start..p.end, &preserved);
    }
    w.splice(PART, vec![(ix.nodes[i].start..ix.nodes[i].end, element)])?;
    if new_id != style {
        replace_references(w, kind, style, &new_id, None)?;
    }
    let recompose: Vec<String> = w.ed.parts.keys().filter(|n| n.starts_with("Stories/")).filter_map(|n| n.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).map(str::to_owned)).collect();
    for story in recompose {
        w.recompose(&story);
    }
    Ok(())
}

/// The tables (their `Self`) with the cells that have the cell style `style` ([column, row] each), or the tables with the table style.
pub(super) fn users(w: &Work<'_>, kind: TableStyleKind, style: &str) -> Vec<(String, Vec<[usize; 2]>)> {
    let mut out = Vec::new();
    for (part, t) in w.ed.parts.iter().filter(|(n, t)| n.starts_with("Stories/") && t.contains(style)) {
        let _ = part;
        let Ok(ix) = Index::scan(t) else { continue };
        for tb in (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Table") {
            let Some(id) = ix.attr(t, tb, "Self") else { continue };
            if kind == TableStyleKind::Table {
                if ix.value(t, tb, "AppliedTableStyle").as_deref() == Some(style) {
                    out.push((id.to_owned(), Vec::new()));
                }
                continue;
            }
            let cells: Vec<[usize; 2]> = ix.children(tb).iter().copied().filter(|&c| ix.local(t, c) == "Cell" && ix.value(t, c, "AppliedCellStyle").as_deref() == Some(style))
                .filter_map(|c| {
                    let name = ix.attr(t, c, "Name")?;
                    let (a, b) = name.split_once(':')?;
                    Some([a.parse().ok()?, b.parse().ok()?])
                }).collect();
            if !cells.is_empty() {
                out.push((id.to_owned(), cells));
            }
        }
    }
    out
}

/// Cell Styles / Table Styles ▸ Delete Style: what has the style gets `replacement` as if it were applied.
pub(super) fn delete(w: &mut Work<'_>, kind: TableStyleKind, style: &str, replacement: &str) -> Result<(), EditError> {
    if style.contains("/$ID/") {
        return Err(EditError::new("invalid", "a built-in style is not deleted"));
    }
    let (t, ix) = scan(w)?;
    let i = ix.by_self(&t, style).filter(|&i| ix.local(&t, i) == kind.tag()).ok_or_else(|| EditError::new("notFound", format!("style {style}")))?;
    if replacement == style {
        return Err(EditError::new("invalid", "a style is not replaced by itself"));
    }
    if replacement != kind.none() && ix.by_self(&t, replacement).is_none() {
        return Err(EditError::new("notFound", format!("style {replacement}")));
    }
    for (table, cells) in users(w, kind, style) {
        if kind == TableStyleKind::Cell {
            tables::styles::apply_cell_style(w, &table, &cells, replacement)?;
        } else {
            tables::styles::apply_table_style(w, &table, replacement)?;
        }
    }
    // styles based on it (a replacement based on it takes its base), region styles of table styles
    let fallback = based_on(&t, &ix, i, kind);
    replace_references(w, kind, style, replacement, Some(&fallback))?;
    let (t, ix) = scan(w)?;
    let i = ix.by_self(&t, style).ok_or_else(|| EditError::new("notFound", format!("style {style}")))?;
    let line = t[..ix.nodes[i].start].rfind('\n').unwrap_or(ix.nodes[i].start);
    w.splice(PART, vec![(line..ix.nodes[i].end, String::new())])
}
