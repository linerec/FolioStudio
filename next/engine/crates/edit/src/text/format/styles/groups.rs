//! Duplicate Style, New Style Group and moving a style into a group, for paragraph and character styles (oracle text-format tstyle-*).
//! Measured with the DOM's `style.duplicate()`, `paragraphStyleGroups.add`, `style.move(AT_END, group)`:
//! - The copy is `<Name> copy`, right after the style, with the style's settings and base; the shortcut is cleared and it has an id of its
//!   own (`StyleUniqueId` is left out, as for a style made new). A paragraph style whose next style is itself has the copy as its own next
//!   style (tstyle-duplicate-paragraph).
//! - A group is `<ParagraphStyleGroup Self="ParagraphStyleGroup/$ID/Heads" Name="$ID/Heads" />` at the end of its parent (the kind's root
//!   group, or a group); a group in a group joins the names (`Heads%3aSub`, `$ID/Heads:Sub`) (tstyle-group-new).
//! - A style in a group is named with the groups before it (`Self` `ParagraphStyle/Heads%3aName`, `Name` `Heads:Name`); a style moved there
//!   stands after the styles of the group, and every reference to it follows (tstyle-group-move, tstyle-group-character, tstyle-group-out).
use super::identity::retarget;
use super::*;
use crate::ops::tablestylegroups::{append, remove};

pub(super) struct Kind { pub(super) tag: &'static str, pub(super) group: &'static str, pub(super) root: &'static str }
/// The groups of a path of names (`Heads:Sub`; none for the top level).
pub(super) fn path(group: Option<&str>) -> Vec<String> {
    group.filter(|g| !g.is_empty()).map(|g| g.split(':').map(str::to_owned).collect()).unwrap_or_default()
}
pub(super) fn kind_of(k: &str) -> Result<Kind, EditError> {
    match k {
        "paragraph" => Ok(Kind { tag: "ParagraphStyle", group: "ParagraphStyleGroup", root: "RootParagraphStyleGroup" }),
        "character" => Ok(Kind { tag: "CharacterStyle", group: "CharacterStyleGroup", root: "RootCharacterStyleGroup" }),
        _ => Err(EditError::new("invalid", "style kind must be paragraph or character")),
    }
}
pub(super) fn scan(w: &Work<'_>) -> Result<(String, Index), EditError> {
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    Ok((t, ix))
}
pub(super) fn valid_name(name: &str) -> Result<(), EditError> {
    if name.trim().is_empty() || name.chars().any(|c| c.is_control() || matches!(c, '/' | ':')) {
        return Err(EditError::new("invalid", "a name is empty or has a slash, a colon or a control character in it"));
    }
    Ok(())
}
/// The group names of a style id (`ParagraphStyle/Heads%3aName`: `Heads`) and its own name.
pub(super) fn split_id(tag: &str, id: &str) -> (Vec<String>, String) {
    let rest = id.strip_prefix(&format!("{tag}/")).unwrap_or(id);
    let mut parts: Vec<String> = rest.split("%3a").map(str::to_owned).collect();
    let leaf = parts.pop().unwrap_or_default();
    (parts, leaf)
}
/// The `Self` and `Name` of a style named `leaf` in `groups`.
pub(super) fn join_id(tag: &str, groups: &[String], leaf: &str) -> (String, String) {
    let all: Vec<&str> = groups.iter().map(String::as_str).chain(std::iter::once(leaf)).collect();
    (format!("{tag}/{}", all.join("%3a")), all.join(":"))
}
/// The group named by a path of names (`Heads:Sub`; none for the top level): its element, or the kind's root group.
pub(super) fn group_node(k: &Kind, t: &str, ix: &Index, group: &str) -> Result<usize, EditError> {
    if group.is_empty() {
        return (0..ix.nodes.len()).find(|&i| ix.local(t, i) == k.root).ok_or_else(|| EditError::new("notFound", k.root));
    }
    let id = format!("{}/$ID/{}", k.group, group.replace(':', "%3a"));
    ix.by_self(t, &id).filter(|&i| ix.local(t, i) == k.group).ok_or_else(|| EditError::new("notFound", format!("style group {group}")))
}
fn style_node(k: &Kind, t: &str, ix: &Index, id: &str) -> Result<usize, EditError> {
    // the DOM: a built-in style is not moved into a group ("Invalid parameter."); a root style is not duplicated either ("Invalid request on a
    // root style."), `[Basic Paragraph]` is (`duplicate`, oracle bs-duplicate)
    if protected(id) { return Err(EditError::new("invalid", if root(id) { "Invalid request on a root style." } else { "Invalid parameter." })); }
    ix.by_self(t, id).filter(|&i| ix.local(t, i) == k.tag).ok_or_else(|| EditError::new("notFound", id))
}
fn indent_of(t: &str, at: usize) -> &str {
    let line = t[..at].rfind('\n').map_or(0, |p| p + 1);
    let s = &t[line..at];
    if s.chars().all(char::is_whitespace) { s } else { "" }
}
/// The element `i` with its `Self` and `Name` changed (a copy: no shortcut, no `StyleUniqueId`, the next style that was the style itself).
fn rebuilt(t: &str, ix: &Index, i: usize, id: &str, name: &str, copy_of: Option<&str>) -> String {
    let n = &ix.nodes[i];
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for a in ix.attrs(t, i) {
        let key = &t[a.name.0..a.name.1];
        let value = unescape(&t[a.value.0..a.value.1]);
        match key {
            "Self" => edits.push((a.value.0..a.value.1, escape(id))),
            "Name" => edits.push((a.value.0..a.value.1, escape(name))),
            "KeyboardShortcut" if copy_of.is_some() => edits.push((a.value.0..a.value.1, "0 0".into())),
            "ExtendedKeyboardShortcut" if copy_of.is_some() => edits.push((a.value.0..a.value.1, "0 0 0".into())),
            "NextStyle" if copy_of == Some(value.as_str()) => edits.push((a.value.0..a.value.1, escape(id))),
            "StyleUniqueId" if copy_of.is_some() => {
                // the attribute goes with the space before it
                let start = t[..a.name.0].rfind(|c: char| !c.is_whitespace()).map_or(a.name.0, |p| p + 1);
                edits.push((start..a.value.1 + 1, String::new()));
            }
            _ => {}
        }
    }
    let mut out = t[n.start..n.end].to_owned();
    for (r, v) in edits.into_iter().rev() {
        out.replace_range(r.start - n.start..r.end - n.start, &v);
    }
    out
}

/// Duplicate Style: `name` (or `<Name> copy`) right after the style.
pub(crate) fn duplicate(w: &mut Work<'_>, kind: &str, style: &str, name: &str) -> Result<(), EditError> {
    let k = kind_of(kind)?;
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id(k.tag, style)?;
    let (t, ix) = scan(w)?;
    let basic = id == "ParagraphStyle/$ID/NormalParagraphStyle";
    let i = if basic { ix.by_self(&t, &id).ok_or_else(|| EditError::new("notFound", id.clone()))? } else { style_node(&k, &t, &ix, &id)? };
    let (groups, leaf) = split_id(k.tag, &id);
    // the copy of `[Basic Paragraph]` is `Basic Paragraph copy` (bs-duplicate)
    let leaf = if basic { "Basic Paragraph".to_owned() } else { leaf };
    let name = if name.is_empty() { format!("{leaf} copy") } else { name.to_owned() };
    valid_name(&name)?;
    let (new_id, full) = join_id(k.tag, &groups, &name);
    if ix.by_self(&t, &new_id).is_some() {
        return Err(EditError::new("invalid", "style name already exists"));
    }
    let element = rebuilt(&t, &ix, i, &new_id, &full, Some(&id));
    let at = ix.nodes[i].end;
    let indent = indent_of(&t, ix.nodes[i].start).to_owned();
    w.splice(PART, vec![(at..at, format!("\n{indent}{element}"))])
}

/// New Style Group (`parent`: the path of the group it goes in, empty for the top level).
pub(crate) fn create_group(w: &mut Work<'_>, kind: &str, name: &str, parent: &str) -> Result<(), EditError> {
    let k = kind_of(kind)?;
    valid_name(name)?;
    let (t, ix) = scan(w)?;
    let at = group_node(&k, &t, &ix, parent)?;
    let mut names = path(Some(parent));
    names.push(name.to_owned());
    let id = format!("{}/$ID/{}", k.group, names.join("%3a"));
    if ix.by_self(&t, &id).is_some() || ix.by_self(&t, &join_id(k.tag, &names[..names.len() - 1], name).0).is_some() {
        return Err(EditError::new("invalid", "a style group or style of that name exists"));
    }
    let tag = k.group;
    let edit = append(&t, &ix, at, &|_| format!("<{tag} Self=\"{}\" Name=\"$ID/{}\" />", escape(&id), escape(&names.join(":"))), None);
    w.splice(PART, vec![edit])
}

/// Move a style into a group (`group`: its path, empty for the top level), after the styles that are there.
pub(crate) fn move_style(w: &mut Work<'_>, kind: &str, style: &str, group: &str) -> Result<(), EditError> {
    let k = kind_of(kind)?;
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id(k.tag, style)?;
    let (t, ix) = scan(w)?;
    let i = style_node(&k, &t, &ix, &id)?;
    let target = group_node(&k, &t, &ix, group)?;
    let groups = path(Some(group));
    let (old, leaf) = split_id(k.tag, &id);
    if old == groups { return Ok(()); }
    let (new_id, full) = join_id(k.tag, &groups, &leaf);
    if ix.by_self(&t, &new_id).is_some() {
        return Err(EditError::new("invalid", "a style of that name is already there"));
    }
    let element = t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
    // a style moved into a group stands before the style that was last there, not after it (tsg-move-three: Heading, Other, Spare moved in
    // that order make Other, Spare, Heading); in an empty group it is the only child, and at the top level it goes to the end
    let before_last = |t: &str, ix: &Index, target: usize| -> Option<(std::ops::Range<usize>, String)> {
        if group.is_empty() { return None; }
        let last = *ix.children(target).iter().rev().find(|&&c| ix.local(t, c) == k.tag)?;
        let at = t[..ix.nodes[last].start].rfind(char::from(10)).map_or(ix.nodes[last].start, |p| p + 1);
        Some((at..at, format!("{}{element}{}", indent_of(t, ix.nodes[last].start), char::from(10))))
    };
    let put = before_last(&t, &ix, target).unwrap_or_else(|| append(&t, &ix, target, &|_| element.clone(), Some(k.tag)));
    let take = remove(&t, &ix, k.group, i);
    // the two edits touch different elements, except for a group that is emptied and filled: take first, then put into the new text
    if put.0.start >= take.0.end || put.0.end <= take.0.start {
        w.splice(PART, vec![take, put])?;
    } else {
        w.splice(PART, vec![take])?;
        let (t, ix) = scan(w)?;
        let target = group_node(&k, &t, &ix, group)?;
        w.splice(PART, vec![before_last(&t, &ix, target).unwrap_or_else(|| append(&t, &ix, target, &|_| element.clone(), Some(k.tag)))])?;
    }
    retarget(w, &id, &new_id, &full)?;
    refresh_stories(w)
}

/// Copy to Group: the style stays and a copy with the same name goes into `group` (oracle loadstyles ls-copy-*;
/// the DOM's `style.duplicate()` moved into the group and named as the style was).
pub(crate) fn copy_to_group(w: &mut Work<'_>, kind: &str, style: &str, group: &str) -> Result<(), EditError> {
    let k = kind_of(kind)?;
    let cat = Catalog::read(&w.ed.parts)?;
    let id = cat.id(k.tag, style)?;
    let (t, ix) = scan(w)?;
    let i = style_node(&k, &t, &ix, &id)?;
    let target = group_node(&k, &t, &ix, group)?;
    let (_, leaf) = split_id(k.tag, &id);
    let (new_id, full) = join_id(k.tag, &path(Some(group)), &leaf);
    if ix.by_self(&t, &new_id).is_some() {
        return Err(EditError::new("invalid", "a style of that name is already there"));
    }
    let element = rebuilt(&t, &ix, i, &new_id, &full, Some(&id));
    // the copy stands first in the group, before its styles (ls-copy-para, ls-copy-para-late); in an empty group it is the only child
    let Some(&first) = ix.children(target).first() else {
        return w.splice(PART, vec![append(&t, &ix, target, &|_| element.clone(), Some(k.tag))]);
    };
    let at = t[..ix.nodes[first].start].rfind(char::from(10)).map_or(ix.nodes[first].start, |p| p + 1);
    let indent = indent_of(&t, ix.nodes[first].start).to_owned();
    w.splice(PART, vec![(at..at, format!("{indent}{element}{}", char::from(10)))])
}

/// A style made new (`element`, in the kind `tag`) goes at the end of the styles of the group at `group` (a path of names).
pub(super) fn insert_in(w: &mut Work<'_>, tag: &str, group: &str, element: String) -> Result<(), EditError> {
    let k = kind_of(if tag == "ParagraphStyle" { "paragraph" } else { "character" })?;
    let (t, ix) = scan(w)?;
    let target = group_node(&k, &t, &ix, group)?;
    w.splice(PART, vec![append(&t, &ix, target, &|_| element.clone(), Some(k.tag))])
}
