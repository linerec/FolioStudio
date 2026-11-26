//! Duplicate Style Group for the five style kinds (paragraph, character, object, table, cell); oracle `stylegroup-dup-*`.
//! Measured with the DOM's `group.duplicate()` (InDesign 2025, `artifacts/menurest-c1-20261023/measured/probe-groups2.json`):
//! - The copy is named `<Name> copy`; when that name is taken among the groups of the parent, `<Name> copy 2`, `copy 3`, …
//! - It holds a copy of everything in the group (styles and sub-groups): `Self` and `Name` carry the new group name (`B copy:B1`,
//!   `B copy:BS:BS1`), and every reference to a style of the group from inside the copy (`BasedOn`, `NextStyle`, …) names the copy of that
//!   style; references to styles outside the group stay. The copied styles keep every other attribute as it was (the shortcut and the
//!   `StyleUniqueId` too, unlike a style duplicated alone).
//! - The copy stands right after the group it was made from, except at the top level of the paragraph and table style panels, where it
//!   stands right before it (nested groups: always after).
use super::*;

const PART: &str = "Resources/Styles.xml";

/// The tags of a kind: (group, style, root group).
fn tags(kind: &str) -> Result<(&'static str, &'static str, &'static str), EditError> {
    Ok(match kind {
        "paragraph" => ("ParagraphStyleGroup", "ParagraphStyle", "RootParagraphStyleGroup"),
        "character" => ("CharacterStyleGroup", "CharacterStyle", "RootCharacterStyleGroup"),
        "object" => ("ObjectStyleGroup", "ObjectStyle", "RootObjectStyleGroup"),
        "table" => ("TableStyleGroup", "TableStyle", "RootTableStyleGroup"),
        "cell" => ("CellStyleGroup", "CellStyle", "RootCellStyleGroup"),
        _ => return Err(EditError::new("invalid", "style kind must be paragraph, character, object, table or cell")),
    })
}

/// A group given by its `Self` (`ParagraphStyleGroup/$ID/B%3aBS`) or by its path of names (`B:BS`): its `Self`.
fn group_id(group_tag: &str, group: &str) -> String {
    if group.starts_with(&format!("{group_tag}/")) { group.to_owned() } else { format!("{group_tag}/$ID/{}", group.replace(':', "%3a")) }
}

/// Replaces `from` by `to` in `text` wherever `from` is followed by one of `after` (the end of the text counts as none of them).
fn replace_before(text: &str, from: &str, to: &str, after: &[&str]) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    let mut rest = text;
    while let Some(at) = rest.find(from) {
        let end = at + from.len();
        out.push_str(&rest[..at]);
        if after.iter().any(|a| rest[end..].starts_with(a)) { out.push_str(to) } else { out.push_str(from) }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Duplicate Style Group: `group` is the group's path of names or its `Self`; the new group's `Self` goes to `created`.
pub(crate) fn duplicate(w: &mut Work<'_>, kind: &str, group: &String) -> Result<(), EditError> {
    let (group_tag, style_tag, root_tag) = tags(kind)?;
    let id = group_id(group_tag, group);
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let g = ix.by_self(&t, &id).filter(|&i| ix.local(&t, i) == group_tag).ok_or_else(|| EditError::new("notFound", format!("style group {group}")))?;
    let old = id.strip_prefix(&format!("{group_tag}/$ID/")).unwrap_or(&id).to_owned();
    let (parent_enc, leaf) = match old.rfind("%3a") { Some(p) => (old[..p + 3].to_owned(), old[p + 3..].to_owned()), None => (String::new(), old.clone()) };
    // `<Name> copy`, then `<Name> copy 2`, …: the first name no group has
    let mut new_leaf = format!("{leaf} copy");
    let mut n = 2;
    while ix.by_self(&t, &format!("{group_tag}/$ID/{parent_enc}{new_leaf}")).is_some() {
        new_leaf = format!("{leaf} copy {n}");
        n += 1;
    }
    let new = format!("{parent_enc}{new_leaf}");
    let (old_name, new_name) = (old.replace("%3a", ":"), new.replace("%3a", ":"));
    let mut text = t[ix.nodes[g].start..ix.nodes[g].end].to_owned();
    text = replace_before(&text, &format!("{group_tag}/$ID/{old}"), &format!("{group_tag}/$ID/{new}"), &["\"", "%3a", "<"]);
    text = replace_before(&text, &format!("{style_tag}/{old}%3a"), &format!("{style_tag}/{new}%3a"), &[""]);
    text = replace_before(&text, &format!("Name=\"$ID/{old_name}"), &format!("Name=\"$ID/{new_name}"), &["\"", ":"]);
    text = replace_before(&text, &format!("Name=\"{old_name}:"), &format!("Name=\"{new_name}:"), &[""]);
    let at_root = ix.nodes[g].parent.is_some_and(|p| ix.local(&t, p) == root_tag);
    let before = at_root && matches!(kind, "paragraph" | "table");
    let indent = indent_before(&t, ix.nodes[g].start).to_owned();
    let edit = if before {
        let line = t[..ix.nodes[g].start].rfind('\n').map_or(ix.nodes[g].start, |p| p + 1);
        (line..line, format!("{indent}{text}\n"))
    } else {
        let at = ix.nodes[g].end;
        (at..at, format!("\n{indent}{text}"))
    };
    w.splice(PART, vec![edit])?;
    w.created.push(format!("{group_tag}/$ID/{new}"));
    Ok(())
}
