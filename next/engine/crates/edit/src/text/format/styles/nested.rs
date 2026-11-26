//! Atomic replacement of a paragraph style's nested styles list, preserving imported record data.
use super::*;
use folio_model::nested::NestedStyle;
use std::collections::BTreeSet;

const TEMPLATE: &str = "<ListItem type=\"record\"><AppliedCharacterStyle type=\"object\"></AppliedCharacterStyle><Delimiter type=\"enumeration\"></Delimiter><Repetition type=\"long\"></Repetition><Inclusive type=\"boolean\"></Inclusive></ListItem>";

/// `raw` (one ListItem record) with the given values written in; every other byte stays. `custom`: the record's
/// `Delimiter` is the user's own character set (`type="string"`), else a named one (`type="enumeration"`).
fn write_record(raw: &str, values: &[(&str, String)], custom: Option<bool>) -> Result<String, EditError> {
    let ix = Index::scan(raw).map_err(|e| EditError::new("xml", e))?;
    let mut result = raw.to_owned();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for (name, value) in values {
        let i = ix.children(0).iter().copied().find(|&i| ix.local(raw, i) == *name)
            .ok_or_else(|| EditError::new("unsupported", "incomplete nested style record"))?;
        let n = &ix.nodes[i];
        if *name == "Delimiter" {
            let want = if custom == Some(true) { "string" } else { "enumeration" };
            if let Some(t) = ix.attrs(raw, i).into_iter().find(|a| &raw[a.name.0..a.name.1] == "type")
                && &raw[t.value.0..t.value.1] != want { edits.push((t.value.0..t.value.1, want.to_owned())); }
        }
        edits.push(if n.empty {
            (n.open_end - 2..n.end, format!(">{}</{name}>", escape(value)))
        } else { (n.open_end..n.close_start, escape(value)) });
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    for (r, v) in edits { result.replace_range(r, &v); }
    Ok(result)
}

/// One item to write: the record it came from, if any (its other bytes stay), and the new values.
struct Entry { source: Option<usize>, values: Vec<(&'static str, String)>, custom: Option<bool> }

/// Replaces the list `list` (`AllNestedStyles`, `AllLineStyles`) of a paragraph style with `entries`, and its
/// `Empty...` flag `flag`: one undo step; an imported record keeps whatever else it holds.
fn replace_list(w: &mut Work<'_>, cat: &Catalog, id: &str, list: &str, flag: &str, template: &str, entries: &[Entry]) -> Result<(), EditError> {
    let mut attrs = cat.styles[id].clone();
    let was_empty = val(&attrs, flag) == "true";
    let default = format!("<{list} type=\"list\"/>");
    let raw = attrs.get(list).and_then(|v| v.xml.as_deref()).unwrap_or(&default);
    let ix = Index::scan(raw).map_err(|e| EditError::new("xml", e))?;
    let records: Vec<_> = ix.children(0).iter().copied().filter(|&i| ix.local(raw, i) == "ListItem").map(|i| &raw[ix.nodes[i].start..ix.nodes[i].end]).collect();
    let mut used = BTreeSet::new();
    let mut content = String::new();
    for entry in entries {
        let base = if let Some(source) = entry.source {
            if !used.insert(source) { return Err(EditError::new("invalid", "duplicate nested style source index")); }
            *records.get(source).ok_or_else(|| EditError::new("invalid", "nested style source index out of bounds"))?
        } else { template };
        content.push_str(&write_record(base, &entry.values, entry.custom)?);
    }
    let root = &ix.nodes[0];
    let mut text = raw.to_owned();
    if root.empty {
        text.replace_range(root.open_end - 2..root.end, &format!(">{content}</{list}>"));
    } else {
        text.insert_str(root.close_start, &content);
        for &i in ix.children(0).iter().rev() {
            if ix.local(raw, i) == "ListItem" { text.replace_range(ix.nodes[i].start..ix.nodes[i].end, ""); }
        }
    }
    let remove = entries.is_empty() && ix.children(0).iter().all(|&i| ix.local(raw, i) == "ListItem") && ix.attrs(raw, 0).len() <= 1;
    attrs.insert(flag.into(), attr(if entries.is_empty() { "true" } else { "false" }));
    if remove { attrs.remove(list); }
    else { attrs.insert(list.into(), Property { value: String::new(), xml: Some(text) }); }
    put(w, id, "ParagraphStyle", &attrs)?;
    inherit_flag(w, cat, id, list, flag, was_empty, entries.is_empty())?;
    refresh_stories(w)
}

pub(crate) fn set(w: &mut Work<'_>, style: &str, items: &[NestedStyle]) -> Result<(), EditError> {
    if items.len() > 64 { return Err(EditError::new("invalid", "at most 64 nested styles")); }
    folio_idml::nested::check(items).map_err(|e| EditError::new("invalid", e))?;
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id("ParagraphStyle", style)?;
    if root(&id) { return Err(EditError::new("invalid", "Invalid request on a root style.")); }
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        // "[No character style]" is a style of its own in IDML: `$ID/[No character style]`
        let character = if item.style == folio_idml::nested::NO_STYLE || item.style.is_empty() { folio_idml::nested::NO_STYLE.to_owned() } else { cat.id("CharacterStyle", &item.style)? };
        entries.push(Entry { source: item.source, custom: Some(item.custom), values: vec![("AppliedCharacterStyle", character), ("Delimiter", item.delimiter.clone()),
            ("Repetition", item.repetition.to_string()), ("Inclusive", item.inclusive.to_string())] });
    }
    replace_list(w, &cat, &id, "AllNestedStyles", "EmptyNestedStyles", TEMPLATE, &entries)
}

const LINE_TEMPLATE: &str = "<ListItem type=\"record\"><AppliedCharacterStyle type=\"object\"></AppliedCharacterStyle><LineCount type=\"long\"></LineCount><RepeatLast type=\"long\"></RepeatLast></ListItem>";

/// Atomic replacement of a paragraph style's nested line styles (`AllLineStyles`): the character style of the paragraph's
/// first lines and so on, `[Repeat]` of the items before it (measured: rules/dropcap.ko.md).
pub(crate) fn set_lines(w: &mut Work<'_>, style: &str, items: &[folio_model::nested::NestedLineStyle]) -> Result<(), EditError> {
    if items.len() > 64 { return Err(EditError::new("invalid", "at most 64 nested line styles")); }
    folio_idml::nested::check_lines(items).map_err(|e| EditError::new("invalid", e))?;
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id("ParagraphStyle", style)?;
    if root(&id) { return Err(EditError::new("invalid", "Invalid request on a root style.")); }
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let character = if item.repeat || item.style == folio_idml::nested::NO_STYLE || item.style.is_empty() { folio_idml::nested::NO_STYLE.to_owned() } else { cat.id("CharacterStyle", &item.style)? };
        // IDML: a repeat has the item count as both LineCount and RepeatLast, every other item -1 for RepeatLast
        let repeat_last = if item.repeat { item.lines.to_string() } else { "-1".to_owned() };
        entries.push(Entry { source: item.source, custom: None, values: vec![("AppliedCharacterStyle", character), ("LineCount", item.lines.to_string()), ("RepeatLast", repeat_last)] });
    }
    replace_list(w, &cat, &id, "AllLineStyles", "EmptyLineStyles", LINE_TEMPLATE, &entries)
}

/// Sets (`character_style` is a style) or clears (empty) the drop cap's character style of a paragraph style: the first
/// item of its own nested styles, delimiter `Dropcap` (measured: rules/dropcap.ko.md). The other items stay as they are.
pub(crate) fn set_drop_cap_style(w: &mut Work<'_>, style: &str, character_style: &str) -> Result<(), EditError> {
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id("ParagraphStyle", style)?;
    let xml = w.ed.parts.get(PART).map(String::as_str).unwrap_or_default();
    let mut items = folio_idml::nested::rules(xml, &id).map_err(|e| EditError::new("xml", e))?;
    let old = items.iter().position(|i| !i.custom && i.delimiter == folio_idml::nested::DROP_CAP);
    let entry = old.map(|k| items.remove(k));
    if !character_style.is_empty() {
        let mut item = entry.unwrap_or(NestedStyle { style: String::new(), delimiter: folio_idml::nested::DROP_CAP.into(), custom: false, repetition: 1, inclusive: true, source: None });
        item.style = character_style.to_owned();
        items.insert(0, item);
    } else if entry.is_none() {
        return Ok(());
    }
    set(w, style, &items)
}

/// `EmptyNestedStyles` says whether a style's list, inherited or its own, is empty. A style that has no list of
/// its own and was inheriting the old state follows when the style it is based on changes (InDesign does the
/// same: measured with a child of a style that gained nested styles). A child that was cleared on purpose
/// while its parent had items keeps its own flag.
fn inherit_flag(w: &mut Work<'_>, cat: &Catalog, parent: &str, list: &str, flag: &str, was_empty: bool, now_empty: bool) -> Result<(), EditError> {
    if was_empty == now_empty { return Ok(()); }
    let old = if was_empty { "true" } else { "false" };
    for (id, a) in cat.styles.iter().filter(|(id, _)| id.starts_with("ParagraphStyle/")) {
        if based(a, "ParagraphStyle") != parent || a.contains_key(list) || val(a, flag) != old { continue; }
        let mut a = a.clone();
        a.insert(flag.into(), attr(if now_empty { "true" } else { "false" }));
        put(w, id, "ParagraphStyle", &a)?;
        inherit_flag(w, cat, id, list, flag, was_empty, now_empty)?;
    }
    Ok(())
}
