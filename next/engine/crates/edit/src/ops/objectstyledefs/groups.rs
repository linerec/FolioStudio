//! Style groups of the Object Styles panel: New Style Group, moving a style into a group, Copy to Group (oracle `objectstyles` os-group-*).
//! Measured with the DOM's `objectStyleGroups.add`, `style.move(AT_END, group)` and `style.duplicate()`:
//! - A group is an `ObjectStyleGroup` element, empty ones `<ObjectStyleGroup Self="ObjectStyleGroup/$ID/Grp" Name="$ID/Grp" />`. A group in a
//!   group joins the names: `Self` `ObjectStyleGroup/$ID/Grp%3aSub`, `Name` `$ID/Grp:Sub` (os-group-new, os-group-nested).
//! - At the top level the groups stand first in `RootObjectStyleGroup`, before every style, a new one after the groups there (os-group-new,
//!   os-group-two). In a group the styles stand first, then its groups (os-group-mixed).
//! - A style in a group is named with the groups before it: `Self` `ObjectStyle/Grp%3aSub%3aName`, `Name` `Grp:Sub:Name`. Moving a style
//!   takes the element out of where it was and puts it at the end of the styles of the group; every reference follows (`AppliedObjectStyle`
//!   of the page items, `BasedOn` of other styles) (os-group-move, os-group-mixed). Out of a group it goes to the end of the root group
//!   (os-group-move-out).
//! - Copy to Group keeps the style where it is and puts a copy with the same name into the group (os-group-copy).
use super::*;
use crate::ops::tablestylegroups::append as append_child;

const GROUP: &str = "ObjectStyleGroup";
const ROOT: &str = "RootObjectStyleGroup";

type Edit = (std::ops::Range<usize>, String);

fn valid(name: &str) -> Result<(), EditError> {
    if name.trim().is_empty() || name.chars().any(|c| c.is_control() || matches!(c, '/' | ':' | '%')) || name.starts_with('$') {
        return Err(EditError::new("invalid", "a style group name is not empty and has no slash, colon or percent sign"));
    }
    Ok(())
}

/// The group names of a group's `Self` (`ObjectStyleGroup/$ID/Grp%3aSub`: `Grp`, `Sub`).
pub(super) fn path_of(id: &str) -> Vec<String> {
    let rest = id.strip_prefix("ObjectStyleGroup/").unwrap_or(id);
    rest.strip_prefix("$ID/").unwrap_or(rest).split("%3a").map(str::to_owned).collect()
}

fn group_attrs(path: &[String]) -> (String, String) {
    (format!("ObjectStyleGroup/$ID/{}", path.join("%3a")), format!("$ID/{}", path.join(":")))
}

/// `ObjectStyle/Grp%3aSub%3aName` is the style `Name` in the group `Grp`, in its group `Sub`.
pub(super) fn split_style(id: &str) -> (Vec<String>, String) {
    let mut parts: Vec<String> = id.strip_prefix("ObjectStyle/").unwrap_or(id).split("%3a").map(str::to_owned).collect();
    let leaf = parts.pop().unwrap_or_default();
    (parts, leaf)
}

pub(super) fn style_attrs(groups: &[String], leaf: &str) -> (String, String) {
    let mut parts = groups.to_vec();
    parts.push(leaf.to_owned());
    (format!("ObjectStyle/{}", parts.join("%3a")), parts.join(":"))
}

/// A group given by its `Self` or by its path in the DOM's way (`Grp:Sub`): its `Self`.
fn group_id(group: &str) -> String {
    if group.starts_with("ObjectStyleGroup/") { group.to_owned() } else { group_attrs(&group.split(':').map(str::to_owned).collect::<Vec<_>>()).0 }
}

/// The element of a group (`None` or an empty id: the root group, which holds the styles and groups with no group).
fn group_node(t: &str, ix: &Index, group: Option<&str>) -> Result<usize, EditError> {
    match group.filter(|g| !g.is_empty()) {
        None => (0..ix.nodes.len()).find(|&i| ix.local(t, i) == ROOT).ok_or_else(|| EditError::new("xml", "no root object style group")),
        Some(id) => ix.by_self(t, &group_id(id)).filter(|&i| ix.local(t, i) == GROUP).ok_or_else(|| EditError::new("notFound", format!("style group {id}"))),
    }
}

fn movable(t: &str, ix: &Index, style: &str) -> Result<usize, EditError> {
    if style.contains("/$ID/") {
        return Err(EditError::new("unsupported", "a built-in object style is not moved or copied"));
    }
    find(t, ix, style)
}

/// The lines of an element again, as if it stood at the indent `to` (it stood at `from`).
fn reindented(text: &str, from: &str, to: &str) -> String {
    let mut lines = text.split('\n');
    let mut out = lines.next().unwrap_or("").to_owned();
    for line in lines {
        out.push('\n');
        out.push_str(&match line.strip_prefix(from) { Some(rest) => format!("{to}{rest}"), None => line.to_owned() });
    }
    out
}

/// The edit that puts a style element into the group `g`: after the styles of the group, else before its groups (the root group: at the end).
fn insert_style(t: &str, ix: &Index, g: usize, element: &str, from: &str) -> Edit {
    let own = indent_before(t, ix.nodes[g].start).to_owned();
    let inner = format!("{own}\t");
    let text = reindented(element, from, &inner);
    let kids = ix.children(g);
    if let Some(&last) = kids.iter().rev().find(|&&c| ix.local(t, c) == "ObjectStyle") {
        let at = ix.nodes[last].end;
        return (at..at, format!("\n{inner}{text}"));
    }
    if ix.local(t, g) == GROUP && let Some(&first) = kids.iter().find(|&&c| ix.local(t, c) == GROUP) {
        let at = t[..ix.nodes[first].start].rfind('\n').map_or(ix.nodes[first].start, |p| p + 1);
        return (at..at, format!("{inner}{text}\n"));
    }
    append_child(t, ix, g, &|_| text.clone(), None)
}

/// The edit that takes the element `i` away (its line with it); a group left with nothing in it becomes an empty element again.
fn remove(t: &str, ix: &Index, i: usize) -> Edit {
    let n = &ix.nodes[i];
    if let Some(p) = n.parent.filter(|&p| ix.local(t, p) == GROUP && ix.children(p).len() == 1) {
        let g = &ix.nodes[p];
        return (g.start..g.end, format!("{} />", t[g.start..g.open_end - 1].trim_end()));
    }
    let line = t[..n.start].rfind('\n').unwrap_or(n.start);
    (line..n.end, String::new())
}

/// New Style Group (`parent`: the group it goes in, none for the top level).
pub(super) fn create_group(w: &mut Work<'_>, name: &String, parent: &Option<String>) -> Result<(), EditError> {
    valid(name)?;
    let (t, ix) = scan(w)?;
    let at = group_node(&t, &ix, parent.as_deref())?;
    let mut path = parent.as_deref().filter(|p| !p.is_empty()).map(|p| path_of(&group_id(p))).unwrap_or_default();
    path.push(name.clone());
    let (id, full) = group_attrs(&path);
    if ix.by_self(&t, &id).is_some() {
        return Err(EditError::new("invalid", format!("a style group named {name} exists")));
    }
    let element = format!("<{GROUP} Self=\"{}\" Name=\"{}\" />", escape(&id), escape(&full));
    let edit = if ix.local(&t, at) == ROOT {
        let own = indent_before(&t, ix.nodes[at].start).to_owned();
        let after = ix.children(at).iter().take_while(|&&c| ix.local(&t, c) == GROUP).last().map_or(ix.nodes[at].open_end, |&c| ix.nodes[c].end);
        (after..after, format!("\n{own}\t{element}"))
    } else {
        append_child(&t, &ix, at, &|_| element.clone(), None)
    };
    w.splice(PART, vec![edit])
}

/// The style `i` as it is written in `groups` under its own name: its new `Self` and its element text.
fn rewritten(t: &str, ix: &Index, i: usize, groups: &[String], leaf: &str) -> Result<(String, String), EditError> {
    let (id, full) = style_attrs(groups, leaf);
    let mut text = t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
    set_attr(&mut text, &[], "Self", &id)?;
    set_attr(&mut text, &[], "Name", &full)?;
    Ok((id, text))
}

/// Move a style into a group (`None` or empty: out of its group, to the top level), at the end of it.
pub(super) fn move_style(w: &mut Work<'_>, style: &str, group: &Option<String>) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let i = movable(&t, &ix, style)?;
    let target = group_node(&t, &ix, group.as_deref())?;
    let groups = group.as_deref().filter(|g| !g.is_empty()).map(|g| path_of(&group_id(g))).unwrap_or_default();
    let (old_groups, leaf) = split_style(style);
    if old_groups == groups {
        return Ok(());
    }
    let (new_id, element) = rewritten(&t, &ix, i, &groups, &leaf)?;
    if ix.by_self(&t, &new_id).is_some() {
        return Err(EditError::new("invalid", format!("an object style named {leaf} exists there")));
    }
    let from = indent_before(&t, ix.nodes[i].start).to_owned();
    let (take, put) = (remove(&t, &ix, i), insert_style(&t, &ix, target, &element, &from));
    // the two edits touch different elements, except for a group that is emptied and filled: take first, then put into the new text
    if put.0.start >= take.0.end || put.0.end <= take.0.start {
        w.splice(PART, vec![take, put])?;
    } else {
        w.splice(PART, vec![take])?;
        let (t, ix) = scan(w)?;
        let target = group_node(&t, &ix, group.as_deref())?;
        w.splice(PART, vec![insert_style(&t, &ix, target, &element, &from)])?;
    }
    references(w, style, &new_id)?;
    recompose(w);
    Ok(())
}

/// Copy to Group: the style stays, a copy with its name goes to the end of the styles of the group.
pub(super) fn copy_to_group(w: &mut Work<'_>, style: &str, group: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let i = movable(&t, &ix, style)?;
    let target = group_node(&t, &ix, Some(group))?;
    let (_, leaf) = split_style(style);
    let (new_id, element) = rewritten(&t, &ix, i, &path_of(&group_id(group)), &leaf)?;
    if ix.by_self(&t, &new_id).is_some() {
        return Err(EditError::new("invalid", format!("an object style named {leaf} exists there")));
    }
    let from = indent_before(&t, ix.nodes[i].start).to_owned();
    w.splice(PART, vec![insert_style(&t, &ix, target, &element, &from)])
}

/// Rename Style Group (Style Group Options…): the groups and styles in it take the new name in their `Self` and `Name`, and every reference
/// follows (`AppliedObjectStyle` of the page items, `BasedOn` of other styles); oracle `osg-rename-*`.
pub(super) fn rename_group(w: &mut Work<'_>, group: &str, name: &String) -> Result<(), EditError> {
    valid(name)?;
    let (t, ix) = scan(w)?;
    let g = group_node(&t, &ix, Some(group))?;
    let old = path_of(&group_id(group));
    let depth = old.len() - 1;
    let mut path = old.clone();
    path[depth] = name.clone();
    let new_group = group_attrs(&path).0;
    if new_group != group_id(group) && ix.by_self(&t, &new_group).is_some() {
        return Err(EditError::new("invalid", format!("a style group named {name} exists")));
    }
    let mut edits: Vec<Edit> = Vec::new();
    let mut moved: Vec<(String, String)> = Vec::new();
    for i in std::iter::once(g).chain((0..ix.nodes.len()).filter(|&i| ix.ancestors(i).any(|a| a == g))) {
        let local = ix.local(&t, i);
        let Some(id) = ix.attr(&t, i, "Self") else { continue };
        if local == GROUP {
            let mut p = path_of(id);
            p[depth] = name.clone();
            let (id, full) = group_attrs(&p);
            edits.push(attr_edit(&ix, &t, i, "Self", &id));
            edits.push(attr_edit(&ix, &t, i, "Name", &full));
        } else if local == "ObjectStyle" {
            let (mut groups, leaf) = split_style(id);
            groups[depth] = name.clone();
            let (new_id, full) = style_attrs(&groups, &leaf);
            edits.push(attr_edit(&ix, &t, i, "Self", &new_id));
            edits.push(attr_edit(&ix, &t, i, "Name", &full));
            moved.push((id.to_owned(), new_id));
        }
    }
    w.splice(PART, edits)?;
    for (from, to) in moved {
        references(w, &from, &to)?;
    }
    recompose(w);
    Ok(())
}

/// Delete Group…: the group goes with everything in it. Its styles (and those of the groups in it) are deleted with `[None]` as the
/// replacement: the items that had one get `[None]`, the styles based on one are based on what it was based on (oracle `osg-delete-*`,
/// the DOM's `group.remove()`).
pub(super) fn delete_group(w: &mut Work<'_>, group: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let g = group_node(&t, &ix, Some(group))?;
    let styles: Vec<String> = (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "ObjectStyle" && ix.ancestors(i).any(|a| a == g))
        .filter_map(|i| ix.attr(&t, i, "Self").map(str::to_owned)).collect();
    for style in &styles {
        super::deletion::delete(w, style, &NONE.to_owned())?;
    }
    let (t, ix) = scan(w)?;
    let g = group_node(&t, &ix, Some(group))?;
    w.splice(PART, vec![remove(&t, &ix, g)])
}

/// Sort by Name: the styles of the root group and of every group stand by name (os-group-sort); the built-in styles (`$ID/…`) stay first,
/// in the order they are in. The order of names ignores case (not measured with names that differ in case only). Only a run of styles that
/// stand together in their group is sorted (the files InDesign writes keep the styles together).
pub(super) fn sort_styles(w: &mut Work<'_>) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let mut edits: Vec<Edit> = Vec::new();
    for g in (0..ix.nodes.len()).filter(|&i| matches!(ix.local(&t, i), ROOT | GROUP)) {
        let kids = ix.children(g);
        let styles: Vec<usize> = kids.iter().copied().filter(|&c| ix.local(&t, c) == "ObjectStyle" && !ix.attr(&t, c, "Self").unwrap_or("").contains("/$ID/")).collect();
        let (Some(&first), Some(&last)) = (styles.first(), styles.last()) else { continue };
        // the styles stand together: no other child between the first and the last
        let (a, b) = (kids.iter().position(|&c| c == first), kids.iter().position(|&c| c == last));
        if a.zip(b).is_none_or(|(a, b)| b - a + 1 != styles.len()) {
            continue;
        }
        let mut sorted = styles.clone();
        sorted.sort_by_key(|&c| crate::xml::unescape(ix.attr(&t, c, "Name").unwrap_or("")).to_lowercase());
        if sorted == styles {
            continue;
        }
        // the elements in the new order, between the gaps the old order had
        let mut text = String::new();
        for (k, &c) in sorted.iter().enumerate() {
            if k > 0 {
                text.push_str(&t[ix.nodes[styles[k - 1]].end..ix.nodes[styles[k]].start]);
            }
            text.push_str(&t[ix.nodes[c].start..ix.nodes[c].end]);
        }
        edits.push((ix.nodes[first].start..ix.nodes[last].end, text));
    }
    if edits.is_empty() { Ok(()) } else { w.splice(PART, edits) }
}
