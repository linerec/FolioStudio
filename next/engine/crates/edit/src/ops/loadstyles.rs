//! Load Paragraph Styles… / Load Character Styles… / Load All Text Styles… / Load Object Styles…: the styles of another document
//! (its `Resources/Styles.xml` and `Resources/Graphic.xml` parts, read by the server; no path of it is kept) come into this one. Measured
//! with `document.importStyles` (oracle `loadstyles`, fixtures loadstyles and loadstyles-other, `ls-*`):
//! - A style the document lacks is added at the end of the styles of its group (before the groups there); a group it lacks is made,
//!   at the end of its parent, with a group of the same names (`Other` > `In Other`) (ls-para-overwrite).
//! - A style whose name (with its groups) the document has is a clash. Use Incoming Style Definition (`overwrite`) replaces the definition
//!   in its place, built-in styles too (`[Basic Paragraph]`: its settings go on into `TextDefault` of Preferences.xml). Auto-Rename
//!   (`rename`) adds the incoming one as `<name> copy` at the end of its group; `[Basic Paragraph]` is `Basic Paragraph copy`. Skip
//!   (`skip`: the DOM's DO_NOT_LOAD_THE_STYLE) leaves the document's style (ls-para-overwrite, -rename, -skip).
//! - A built-in style (`$ID/…`) the other document defines the same way is no clash and nothing happens.
//! - What a loaded style names follows it: a swatch it uses comes along (the paste rules of `xdoc`), a style it is based on, its next
//!   style, a drop cap's character style: the loaded one, the renamed copy when it came as a copy, the document's own when it was left
//!   (the drop cap style `Cap` of ls-para-* comes with the paragraph style that names it). A copy's next style that was the style
//!   itself is the copy itself, and its shortcut is cleared (ls-para-rename).
//! - A style named in `only` (the dialog's checked rows; none named: all) is loaded; the others are not, but are brought in when a loaded
//!   style names them and the document lacks them.
use std::collections::{BTreeMap, BTreeSet};

use super::clipboard::Source;
use super::xdoc::Importer;
use super::*;
use crate::xml::{Index, unescape};

const STYLES: &str = "Resources/Styles.xml";
const KINDS: [(&str, &str, &str); 5] = [
    ("ParagraphStyle", "ParagraphStyleGroup", "RootParagraphStyleGroup"),
    ("CharacterStyle", "CharacterStyleGroup", "RootCharacterStyleGroup"),
    ("ObjectStyle", "ObjectStyleGroup", "RootObjectStyleGroup"),
    ("CellStyle", "CellStyleGroup", "RootCellStyleGroup"),
    ("TableStyle", "TableStyleGroup", "RootTableStyleGroup"),
];
/// Attributes that are the style's identity or its place, not its definition.
const NOT_DEFINING: [&str; 5] = ["Self", "Name", "StyleUniqueId", "KeyboardShortcut", "ExtendedKeyboardShortcut"];

/// The kinds a `kinds` list names, in the order they are loaded (`text` is paragraph and character styles).
fn selected(kinds: &[String]) -> Result<Vec<usize>, EditError> {
    let mut out = Vec::new();
    for k in kinds {
        let ixs: &[usize] = match k.as_str() {
            "paragraph" => &[0],
            "character" => &[1],
            "object" => &[2],
            "cell" => &[3],
            "table" => &[4],
            "text" => &[0, 1],
            _ => return Err(EditError::new("invalid", "style kind must be paragraph, character, object, cell, table or text")),
        };
        for &i in ixs {
            if !out.contains(&i) {
                out.push(i);
            }
        }
    }
    Ok(out)
}

/// A style of a document.
struct Row {
    kind: usize,
    node: usize,
    id: String,
    /// `Name` as written (`Heads:Sub`, `$ID/NormalParagraphStyle`).
    name: String,
}

fn rows(t: &str, ix: &Index, kinds: &[usize]) -> Vec<Row> {
    let mut out = Vec::new();
    for &kind in kinds {
        for node in 0..ix.nodes.len() {
            if ix.local(t, node) != KINDS[kind].0 {
                continue;
            }
            let (Some(id), Some(name)) = (ix.value(t, node, "Self"), ix.value(t, node, "Name")) else { continue };
            out.push(Row { kind, node, id, name });
        }
    }
    out
}

/// The groups before the name in a style's `Self` (`ParagraphStyle/Heads%3aSub`: `Heads`) and its own name (`Sub`).
fn split(tag: &str, id: &str) -> (Vec<String>, String) {
    let mut parts: Vec<String> = id.strip_prefix(&format!("{tag}/")).unwrap_or(id).split("%3a").map(str::to_owned).collect();
    let leaf = parts.pop().unwrap_or_default();
    (parts, leaf)
}

/// What a built-in style is called in the panels (`$ID/NormalParagraphStyle`: `Basic Paragraph`; `$ID/[Normal Text Frame]`: `Normal Text Frame`).
fn shown(leaf: &str) -> String {
    let leaf = leaf.strip_prefix("$ID/").unwrap_or(leaf);
    match leaf {
        "NormalParagraphStyle" => "Basic Paragraph".to_owned(),
        _ => {
            let bare = leaf.trim_start_matches('[').trim_end_matches(']');
            // [Normal Graphics Frame], [Normal Text Frame], [Normal Grid] are the Basic ones in the panel and in a copy's name (ls-object-rename)
            bare.strip_prefix("Normal ").map_or_else(|| bare.to_owned(), |rest| format!("Basic {rest}"))
        }
    }
}

/// The placeholders InDesign never loads: they are the same in every document.
fn system(id: &str) -> bool {
    id.ends_with("/$ID/[No paragraph style]") || id.ends_with("/$ID/[No character style]") || id == "ObjectStyle/$ID/[None]"
        || id == "CellStyle/$ID/[None]" || id == "TableStyle/$ID/[No table style]"
}

/// The definition of an element: its attributes but the identity ones, and what is inside it.
fn definition(t: &str, ix: &Index, i: usize) -> (Vec<(String, String)>, String) {
    let mut attrs: Vec<(String, String)> = ix.attrs(t, i).iter()
        .map(|a| (t[a.name.0..a.name.1].to_owned(), t[a.value.0..a.value.1].to_owned()))
        .filter(|(k, _)| !NOT_DEFINING.contains(&k.as_str()))
        .collect();
    attrs.sort();
    let n = &ix.nodes[i];
    let inner = if n.empty { String::new() } else { t[n.open_end..n.close_start].split_whitespace().collect::<Vec<_>>().join(" ") };
    (attrs, inner)
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Add,
    Overwrite,
    Keep,
}

struct Item {
    row: usize,
    act: Act,
    to_id: String,
    to_name: String,
}

pub(crate) fn load(w: &mut Work<'_>, kinds: &[String], parts: &BTreeMap<String, String>, clash: &str, only: &[String]) -> Result<(), EditError> {
    let kinds = selected(kinds)?;
    if !matches!(clash, "overwrite" | "rename" | "skip") {
        return Err(EditError::new("invalid", "clash must be overwrite, rename or skip"));
    }
    let src_text = parts.get(STYLES).ok_or_else(|| EditError::new("invalid", "the document has no styles"))?.clone();
    let sx = Index::scan(&src_text).map_err(|e| EditError::new("xml", e))?;
    let src_rows = rows(&src_text, &sx, &kinds);
    let t = w.text(STYLES)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let mut taken: BTreeSet<String> = rows(&t, &ix, &[0, 1, 2, 3, 4]).into_iter().map(|r| r.id).collect();
    let have: BTreeMap<String, usize> = rows(&t, &ix, &[0, 1, 2, 3, 4]).into_iter().map(|r| (r.id, r.node)).collect();
    let mut items: Vec<Item> = Vec::new();
    for (k, row) in src_rows.iter().enumerate() {
        if !only.is_empty() && !only.contains(&row.id) {
            continue;
        }
        let tag = KINDS[row.kind].0;
        let item = match have.get(&row.id) {
            None => {
                taken.insert(row.id.clone());
                Item { row: k, act: Act::Add, to_id: row.id.clone(), to_name: row.name.clone() }
            }
            // a style that is only a placeholder ([None], [No paragraph style]) is never loaded; one defined the same way is not written again
            Some(_) if system(&row.id) => continue,
            Some(&mine) if clash == "overwrite" && definition(&t, &ix, mine) == definition(&src_text, &sx, row.node) => {
                Item { row: k, act: Act::Keep, to_id: row.id.clone(), to_name: row.name.clone() }
            }
            Some(_) if clash == "overwrite" => Item { row: k, act: Act::Overwrite, to_id: row.id.clone(), to_name: row.name.clone() },
            Some(_) if clash == "skip" => Item { row: k, act: Act::Keep, to_id: row.id.clone(), to_name: row.name.clone() },
            Some(_) => {
                let (groups, leaf) = split(tag, &row.id);
                let leaf = shown(&leaf);
                let mut n = 1;
                let (to_id, to_name) = loop {
                    let copy = if n == 1 { format!("{leaf} copy") } else { format!("{leaf} copy {n}") };
                    let all: Vec<&str> = groups.iter().map(String::as_str).chain(std::iter::once(copy.as_str())).collect();
                    let id = format!("{tag}/{}", all.join("%3a"));
                    if !taken.contains(&id) {
                        break (id, all.join(":"));
                    }
                    n += 1;
                };
                taken.insert(to_id.clone());
                Item { row: k, act: Act::Add, to_id, to_name }
            }
        };
        items.push(item);
    }
    let source = Source::of(&crate::Editor::new(parts.clone()));
    let mut importer = Importer::new(&source);
    for item in &items {
        importer.preset(&src_rows[item.row].id, &item.to_id);
    }
    let mut overwritten = false;
    for item in &items {
        if item.act == Act::Keep {
            continue;
        }
        let row = &src_rows[item.row];
        let mut element = importer.rewrite(w, &src_text[sx.nodes[row.node].start..sx.nodes[row.node].end])?;
        element = set_attr(&element, "Name", &item.to_name)?;
        element = without_attr(&element, "StyleUniqueId");
        let (tag, group_tag, root_tag) = KINDS[row.kind];
        let t = w.text(STYLES)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        match item.act {
            Act::Overwrite => {
                let Some(mine) = ix.by_self(&t, &item.to_id) else { continue };
                if let Some(unique) = ix.value(&t, mine, "StyleUniqueId") {
                    element = set_attr(&element, "StyleUniqueId", &unique)?;
                }
                let n = &ix.nodes[mine];
                w.splice(STYLES, vec![(n.start..n.end, element.clone())])?;
                overwritten = true;
                if item.to_id == "ParagraphStyle/$ID/NormalParagraphStyle" {
                    text_default(w, &element)?;
                }
            }
            _ => {
                if item.to_id != row.id {
                    // a copy: no shortcut
                    if attr_value(&element, "KeyboardShortcut").is_some() {
                        element = set_attr(&element, "KeyboardShortcut", "0 0")?;
                    }
                    if attr_value(&element, "ExtendedKeyboardShortcut").is_some() {
                        element = set_attr(&element, "ExtendedKeyboardShortcut", "0 0 0")?;
                    }
                }
                let (groups, _) = split(tag, &item.to_id);
                let target = ensure_group(w, group_tag, root_tag, &groups)?;
                let t = w.text(STYLES)?.to_owned();
                let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
                let at = group_node(group_tag, root_tag, &t, &ix, &target)?;
                let edit = crate::ops::tablestylegroups::append(&t, &ix, at, &|_| element.clone(), Some(tag));
                w.splice(STYLES, vec![edit])?;
            }
        }
    }
    if overwritten {
        crate::text::format::styles::refresh_stories(w)?;
    }
    Ok(())
}

fn attr_value(element: &str, name: &str) -> Option<String> {
    let ix = Index::scan(element).ok()?;
    ix.attr(element, 0, name).map(unescape)
}

fn set_attr(element: &str, name: &str, value: &str) -> Result<String, EditError> {
    let ix = Index::scan(element).map_err(|e| EditError::new("xml", e))?;
    let (range, text) = crate::ops::attr_edit(&ix, element, 0, name, value);
    let mut out = element.to_owned();
    out.replace_range(range, &text);
    Ok(out)
}

/// The element without the attribute (and the space before it).
fn without_attr(element: &str, name: &str) -> String {
    let Ok(ix) = Index::scan(element) else { return element.to_owned() };
    for a in ix.attrs(element, 0) {
        if &element[a.name.0..a.name.1] == name {
            let start = element[..a.name.0].rfind(|c: char| !c.is_whitespace()).map_or(a.name.0, |p| p + 1);
            let mut out = element.to_owned();
            out.replace_range(start..a.value.1 + 1, "");
            return out;
        }
    }
    element.to_owned()
}

/// The group of the names `groups` (made, with the groups before it, when the document lacks it): the names back.
fn ensure_group(w: &mut Work<'_>, group_tag: &str, root_tag: &str, groups: &[String]) -> Result<Vec<String>, EditError> {
    for depth in 1..=groups.len() {
        let path = &groups[..depth];
        let t = w.text(STYLES)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let id = format!("{group_tag}/$ID/{}", path.join("%3a"));
        if ix.by_self(&t, &id).is_some() {
            continue;
        }
        let parent = group_node(group_tag, root_tag, &t, &ix, &path[..depth - 1])?;
        let name = path.join(":");
        let element = format!("<{group_tag} Self=\"{}\" Name=\"$ID/{}\" />", escape(&id), escape(&name));
        // the object styles of the top level have their groups first, after the groups there (ls-object-overwrite)
        let edit = if group_tag == "ObjectStyleGroup" && depth == 1 {
            let indent = {
                let first = ix.children(parent).first().map_or(ix.nodes[parent].close_start, |&c| ix.nodes[c].start);
                let line = t[..first].rfind(char::from(10)).map_or(0, |p| p + 1);
                t[line..first].to_owned()
            };
            match ix.children(parent).iter().rev().find(|&&c| ix.local(&t, c) == group_tag) {
                Some(&last) => (ix.nodes[last].end..ix.nodes[last].end, format!("
{indent}{element}")),
                None => match ix.children(parent).first() {
                    Some(&first) => (ix.nodes[first].start..ix.nodes[first].start, format!("{element}
{indent}")),
                    None => crate::ops::tablestylegroups::append(&t, &ix, parent, &|_| element.clone(), None),
                },
            }
        } else {
            crate::ops::tablestylegroups::append(&t, &ix, parent, &|_| element.clone(), None)
        };
        w.splice(STYLES, vec![edit])?;
    }
    Ok(groups.to_vec())
}

fn group_node(group_tag: &str, root_tag: &str, t: &str, ix: &Index, path: &[String]) -> Result<usize, EditError> {
    if path.is_empty() {
        return (0..ix.nodes.len()).find(|&i| ix.local(t, i) == root_tag).ok_or_else(|| EditError::new("notFound", root_tag));
    }
    ix.by_self(t, &format!("{group_tag}/$ID/{}", path.join("%3a"))).ok_or_else(|| EditError::new("notFound", format!("style group {}", path.join(":"))))
}

/// `[Basic Paragraph]` set differently: the settings of the document's text default follow (ls-para-overwrite: `PointSize` in `TextDefault`).
fn text_default(w: &mut Work<'_>, element: &str) -> Result<(), EditError> {
    const PREFERENCES: &str = "Resources/Preferences.xml";
    let Ok(t) = w.text(PREFERENCES).map(str::to_owned) else { return Ok(()) };
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "TextDefault") else { return Ok(()) };
    let own: BTreeSet<String> = ix.attrs(&t, i).iter().map(|a| t[a.name.0..a.name.1].to_owned()).collect();
    let src = Index::scan(element).map_err(|e| EditError::new("xml", e))?;
    let mut edits = Vec::new();
    for a in src.attrs(element, 0) {
        let key = &element[a.name.0..a.name.1];
        if matches!(key, "Self" | "Name" | "NextStyle" | "Imported" | "StyleUniqueId" | "KeyboardShortcut" | "ExtendedKeyboardShortcut") || !own.contains(key) {
            continue;
        }
        edits.push(crate::ops::attr_edit(&ix, &t, i, key, &unescape(&element[a.value.0..a.value.1])));
    }
    if edits.is_empty() {
        return Ok(());
    }
    w.splice(PREFERENCES, edits)
}

/// A style of another document's file, for the Load Styles window.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStyle {
    /// `paragraph`, `character` or `object`.
    pub kind: String,
    /// Its `Self` in the file (what `loadStyles` `only` names).
    pub id: String,
    /// The name shown (a built-in style by its panel name).
    pub name: String,
    /// The groups it is in.
    pub group: Vec<String>,
    /// The document has a style of that name.
    pub conflict: bool,
    /// The document's own definition is the same: loading it changes nothing.
    pub same: bool,
}

/// The styles of the other document's `Styles.xml` (the kinds `kinds` names) against this document's: which ones are a clash.
pub fn describe(kinds: &[String], other: &BTreeMap<String, String>, this: &BTreeMap<String, String>) -> Result<Vec<FileStyle>, EditError> {
    let kinds = selected(kinds)?;
    let src_text = other.get(STYLES).ok_or_else(|| EditError::new("invalid", "the document has no styles"))?;
    let sx = Index::scan(src_text).map_err(|e| EditError::new("xml", e))?;
    let t = this.get(STYLES).ok_or_else(|| EditError::new("invalid", "this document has no styles"))?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let have: BTreeMap<String, usize> = rows(t, &ix, &[0, 1, 2, 3, 4]).into_iter().map(|r| (r.id, r.node)).collect();
    let mut out = Vec::new();
    for row in rows(src_text, &sx, &kinds) {
        let (groups, leaf) = split(KINDS[row.kind].0, &row.id);
        let mine = have.get(&row.id).copied();
        let same = mine.is_some_and(|m| definition(t, &ix, m) == definition(src_text, &sx, row.node));
        if system(&row.id) {
            continue;
        }
        out.push(FileStyle { kind: ["paragraph", "character", "object", "cell", "table"][row.kind].to_owned(), id: row.id, name: shown(&unescape_percent(&leaf)), group: groups.iter().map(|g| unescape_percent(g)).collect(), conflict: mine.is_some(), same });
    }
    Ok(out)
}

fn unescape_percent(s: &str) -> String {
    s.replace("%3a", ":").replace("%25", "%")
}
