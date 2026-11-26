//! Rename Style Group and Delete Style Group for paragraph and character styles (oracle text-format tsg-*, styles-c2).
//! Measured with the DOM's `group.name = …` and `group.remove()` (rules in next/oracle/rules/styles-c2.ko.md):
//! - Renaming changes the `Self` and `Name` of the group, of the groups in it and of the styles in them (`Heads%3aName` → `Titles%3aName`);
//!   the styles keep their place and every reference follows (text, `BasedOn`, `NextStyle`, nested/GREP/drop cap styles). The same name
//!   as before is no change; a name of a group (or a style) that is in the same place is refused; brackets are refused.
//! - Deleting takes the group and everything in it away, with no replacement:
//!   - the text that had one of the styles breaks the link to it (`[No paragraph style]` with what the style gave as its own settings;
//!     the same as Break Link to Style, tsg-delete-applied, tsg-delete-based);
//!   - a style outside that was based on one of them is based on `[No ... style]` and takes in the settings of the whole chain above the
//!     deleted style — the settings of the farthest style winning, then its own (tsg-delete-chain, tsg-delete-chain-conflict: Heading's
//!     18 pt is lost to Body's 14 pt);
//!   - a next style (and a nested, GREP or drop cap style) that was one of them becomes `[No ... style]` (tsg-delete-next, tsg-delete-next-in).
use super::groups::{group_node, join_id, kind_of, path, scan, split_id, valid_name, Kind};
use super::identity::retarget;
use super::*;
use crate::ops::tablestylegroups::remove;
use std::collections::BTreeSet;

fn no_brackets(name: &str) -> Result<(), EditError> {
    if name.contains('[') || name.contains(']') {
        return Err(EditError::new("invalid", "a style group name cannot contain brackets"));
    }
    Ok(())
}

/// The `Self` of every style in the group node `g` and in the groups below it, in document order.
fn styles_in(k: &Kind, t: &str, ix: &Index, g: usize) -> Vec<String> {
    (0..ix.nodes.len())
        .filter(|&i| ix.local(t, i) == k.tag && ix.ancestors(i).any(|a| a == g))
        .filter_map(|i| ix.attr(t, i, "Self").map(str::to_owned))
        .collect()
}

fn attr_edit(ix: &Index, t: &str, i: usize, name: &str, value: &str) -> Option<(std::ops::Range<usize>, String)> {
    ix.attrs(t, i).into_iter().find(|a| &t[a.name.0..a.name.1] == name).map(|a| (a.value.0..a.value.1, escape(value)))
}

/// Rename Style Group: `group` is its path of names (`Heads:Sub`), `name` the new name of the last one.
pub(crate) fn rename(w: &mut Work<'_>, kind: &str, group: &str, name: &str) -> Result<(), EditError> {
    let k = kind_of(kind)?;
    valid_name(name)?;
    no_brackets(name)?;
    if group.is_empty() {
        return Err(EditError::new("invalid", "a style group is named by its path"));
    }
    let (t, ix) = scan(w)?;
    let g = group_node(&k, &t, &ix, group)?;
    let old = path(Some(group));
    let depth = old.len() - 1;
    if old[depth] == name {
        return Ok(());
    }
    let mut renamed = old.clone();
    renamed[depth] = name.to_owned();
    let new_group = |names: &[String]| (format!("{}/$ID/{}", k.group, names.join("%3a")), format!("$ID/{}", names.join(":")));
    // a group or a style of that name in the same place (the DOM refuses both: "A style group with this name already exists.")
    if ix.by_self(&t, &new_group(&renamed).0).is_some() || ix.by_self(&t, &join_id(k.tag, &renamed[..depth], name).0).is_some() {
        return Err(EditError::new("invalid", "a style group or style of that name is already there"));
    }
    let mut edits = Vec::new();
    let mut moved = Vec::new();
    for i in std::iter::once(g).chain((0..ix.nodes.len()).filter(|&i| ix.ancestors(i).any(|a| a == g))) {
        let local = ix.local(&t, i);
        let Some(id) = ix.attr(&t, i, "Self").map(str::to_owned) else { continue };
        if local == k.group {
            let mut names: Vec<String> = id.strip_prefix(&format!("{}/$ID/", k.group)).unwrap_or(&id).split("%3a").map(str::to_owned).collect();
            names[depth] = name.to_owned();
            let (new_id, full) = new_group(&names);
            edits.extend(attr_edit(&ix, &t, i, "Self", &new_id));
            edits.extend(attr_edit(&ix, &t, i, "Name", &full));
        } else if local == k.tag {
            // the style's own `Self` and `Name` change in `retarget`, with the references
            let (mut groups, leaf) = split_id(k.tag, &id);
            groups[depth] = name.to_owned();
            let (new_id, full) = join_id(k.tag, &groups, &leaf);
            moved.push((id, new_id, full));
        }
    }
    w.splice(PART, edits)?;
    for (from, to, full) in moved {
        retarget(w, &from, &to, &full)?;
    }
    refresh_stories(w)
}

/// What the chain above a deleted style gives: the settings of every style on it, the farthest winning (the roots `[No ... style]` give
/// only the defaults and are not part of it).
fn chain_given(cat: &Catalog, tag: &str, start: &str) -> Attrs {
    let mut given = Attrs::new();
    let mut seen: Vec<String> = Vec::new();
    let mut at = start.to_owned();
    while !at.is_empty() && !at.ends_with(&format!("/$ID/[No {} style]", if tag == "ParagraphStyle" { "paragraph" } else { "character" })) && !seen.contains(&at) {
        seen.push(at.clone());
        let Some(style) = cat.styles.get(&at) else { break };
        for (key, value) in style.iter().filter(|(key, _)| !META.contains(&key.as_str())) {
            given.insert(key.clone(), value.clone());
        }
        at = based(style, tag);
    }
    given
}

/// Delete Style Group (`group`: its path of names).
pub(crate) fn delete(w: &mut Work<'_>, kind: &str, group: &str) -> Result<(), EditError> {
    let k = kind_of(kind)?;
    if group.is_empty() {
        return Err(EditError::new("invalid", "a style group is named by its path"));
    }
    let (t, ix) = scan(w)?;
    let g = group_node(&k, &t, &ix, group)?;
    let doomed: BTreeSet<String> = styles_in(&k, &t, &ix, g).into_iter().collect();
    if !doomed.is_empty() {
        let paragraph = k.tag == "ParagraphStyle";
        let none_id = none(k.tag);
        // 1. the text breaks the link (before the styles go: it needs what they give)
        crate::text::format::break_link::break_all(w, paragraph, &doomed)?;
        // 2. a style based on one of them is based on nothing and takes in what the chain gave
        let cat = Catalog::read(&w.ed.parts)?;
        let dependents: Vec<(String, Attrs)> = cat.styles.iter()
            .filter(|(id, a)| id.starts_with(&format!("{}/", k.tag)) && !doomed.contains(*id) && doomed.contains(&based(a, k.tag)))
            .map(|(id, a)| (id.clone(), a.clone()))
            .collect();
        for (id, mut a) in dependents {
            for (key, value) in chain_given(&cat, k.tag, &based(&a, k.tag)) {
                a.entry(key).or_insert(value);
            }
            a.insert("BasedOn".into(), reference(&none_id, k.tag));
            put(w, &id, k.tag, &a)?;
        }
    }
    // 3. the group goes with everything in it
    let (t, ix) = scan(w)?;
    let g = group_node(&k, &t, &ix, group)?;
    w.splice(PART, vec![remove(&t, &ix, k.group, g)])?;
    // 4. what still names one of the styles (next styles, nested, GREP and drop cap styles) names `[No ... style]`
    if !doomed.is_empty() {
        let none_id = none(k.tag);
        let parts: Vec<(String, String)> = w.ed.parts.iter().filter(|(p, _)| p.ends_with(".xml")).map(|(p, t)| (p.clone(), t.clone())).collect();
        for (part, t) in parts {
            let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
            let mut edits = Vec::new();
            for (i, n) in ix.nodes.iter().enumerate() {
                for a in ix.attrs(&t, i) {
                    let key = &t[a.name.0..a.name.1];
                    if folio_idml::style_identity::is_reference_attribute(key) && doomed.contains(&unescape(&t[a.value.0..a.value.1])) {
                        edits.push((a.value.0..a.value.1, escape(&none_id)));
                    }
                }
                if ix.attr(&t, i, "type") == Some("object") && !n.empty && ix.children(i).is_empty() && doomed.contains(&unescape(t[n.open_end..n.close_start].trim())) {
                    edits.push((n.open_end..n.close_start, escape(&none_id)));
                }
            }
            w.splice(&part, edits)?;
        }
    }
    refresh_stories(w)
}
