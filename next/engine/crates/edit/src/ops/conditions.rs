//! Condition definitions, their visibility and the indicator mode (Window > Type & Tables > Conditional Text).
//! Measured with InDesign 2025 (next/oracle/rules/condx-c1.ko.md, scenarios condx-*):
//! - a definition is a `Condition` element of designmap.xml (`Self="Condition/<name>"`, spaces kept) after the last
//!   `NamedGrid`; its colour is the first of the 27 interface colours (LightBlue ... LightGray) no condition uses, LightBlue
//!   when all are used; a second condition of the same name is refused (names differ by case);
//! - deleting a condition removes its token from every `AppliedConditions` (the attribute goes when the list is empty), so text
//!   whose remaining conditions are all hidden moves into `HiddenText`, and hidden text that is left without a hidden
//!   condition returns to the flow;
//! - hiding text is a `HiddenText` element at the start of the next `CharacterStyleRange` of the paragraph, holding a copy of the
//!   paragraph start tag around the hidden ranges (conditions kept); showing takes the ranges back out.
//! - Condition Options (`set_options`): `Name` (and with it `Self`, `Condition/<name>`), `IndicatorMethod` (UseUnderline/UseHighlight),
//!   `UnderlineIndicatorAppearance` (Wavy/Solid/Dashed) and `IndicatorColor` (a UI colour name, or a list of three doubles for an
//!   own colour). A renamed condition takes its new place in the name order, and every `AppliedConditions` token follows, hidden
//!   text included (condx-opt-*).
//!   Hidden text over paragraph breaks is `para.rs` (condx-hide-para-*, condx-show-para-*); text with a note, variable or object inside
//!   is refused before the first byte changes.
use super::{attr_edit, attr_removal};
use crate::{EditError, Work, xml::{Index, escape}};
use folio_model::conditions::Condition;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;

mod para;

const DM: &str = "designmap.xml";
/// The interface colours new conditions take (`UI_COLORS` up to LightGray; measured with 30 new conditions).
const PALETTE: usize = 27;

fn error(m: impl Into<String>) -> EditError { EditError::new("invalid", m) }
fn unsupported(m: impl Into<String>) -> EditError { EditError::new("unsupported", m) }

fn definitions(w: &Work<'_>) -> Result<Vec<Condition>, EditError> {
    folio_idml::conditions::read(&w.ed.parts).map_err(|e| EditError::new("xml", e))
}

fn find<'a>(defs: &'a [Condition], name: &str) -> Result<&'a Condition, EditError> {
    defs.iter().find(|c| c.id == name || c.name == name).ok_or_else(|| EditError::new("notFound", format!("condition {name}")))
}

fn decode(token: &str) -> String {
    let b = token.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && let Some(Ok(v)) = token.get(i + 1..i + 3).map(|h| u8::from_str_radix(h, 16)) {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| token.to_owned())
}

/// Collation of condition names: case-insensitive, lower case first among equals.
fn sort_key(name: &str) -> (String, String) {
    (name.to_lowercase(), name.chars().map(|c| if c.is_lowercase() { c.to_uppercase().next().unwrap_or(c) } else { c.to_lowercase().next().unwrap_or(c) }).collect())
}

fn hidden_ids(defs: &[Condition]) -> HashSet<String> {
    defs.iter().filter(|c| !c.visible).map(|c| c.id.clone()).collect()
}

/// The colour a new condition gets: the first interface colour none of `defs` uses.
pub(crate) fn next_color(defs: &[Condition]) -> &'static str {
    let used: HashSet<&str> = defs.iter().map(|c| c.indicator_color.as_str()).collect();
    folio_model::UI_COLORS[..PALETTE].iter().copied().find(|c| !used.contains(c)).unwrap_or(folio_model::UI_COLORS[0])
}

pub(crate) fn create(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    if name.trim().is_empty() { return Err(error("condition name required")); }
    if name.trim() != name || name.chars().any(char::is_control) || name.contains(['%', '/']) {
        return Err(error("condition name has characters that are not measured (leading or trailing space, control characters, % or /)"));
    }
    let defs = definitions(w)?;
    if defs.iter().any(|c| c.name == name) { return Err(error("A condition with that name exists already.")); }
    let id = format!("Condition/{name}");
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let top = |tag: &str| (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == tag && ix.nodes[i].parent.is_some_and(|p| ix.nodes[p].parent.is_none())).collect::<Vec<_>>();
    // InDesign keeps conditions sorted by name (condx-create-case: Print, web, Web), so the new one goes before the first greater name
    let existing = top("Condition");
    let at = match existing.iter().find(|&&i| sort_key(&ix.value(&t, i, "Name").unwrap_or_default()) > sort_key(name)) {
        Some(&i) => Some(ix.nodes[i].start),
        None => existing.last().or(top("NamedGrid").last()).map(|&i| ix.nodes[i].end).or_else(|| top("Preferences").first().map(|&i| ix.nodes[i].start)),
    }.ok_or_else(|| unsupported("designmap has no place for conditions"))?;
    // before an element the new one needs its own line start; after one, the line break comes first
    let before_element = existing.iter().any(|&i| ix.nodes[i].start == at);
    let body = format!(
        "<Condition Self=\"{id}\" Name=\"{name}\" IndicatorMethod=\"UseUnderline\" UnderlineIndicatorAppearance=\"Wavy\" Visible=\"true\">
		<Properties>
			<IndicatorColor type=\"enumeration\">{color}</IndicatorColor>
		</Properties>
	</Condition>",
        id = escape(&id), name = escape(name), color = next_color(&defs));
    let element = if before_element { format!("{body}
	") } else { format!("
	{body}") };
    w.splice(DM, vec![(at..at, element)])?;
    w.created.push(id);
    Ok(())
}

pub(crate) fn delete(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    let defs = definitions(w)?;
    let id = find(&defs, name)?.id.clone();
    let before = hidden_ids(&defs);
    let mut after = before.clone();
    after.remove(&id);
    // stories first: a refusal leaves the definition where it is (and the command rolls back anyway)
    reflow(w, Some(&id), &before, &after)?;
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let i = ix.by_self(&t, &escape(&id)).filter(|&i| ix.local(&t, i) == "Condition").ok_or_else(|| EditError::new("notFound", format!("condition {name}")))?;
    let n = &ix.nodes[i];
    let mut start = n.start;
    while start > 0 && matches!(t.as_bytes()[start - 1], b'\t' | b' ') { start -= 1; }
    if start > 0 && t.as_bytes()[start - 1] == b'\n' { start -= 1; }
    w.splice(DM, vec![(start..n.end, String::new())])
}

/// The condition without its indicator colour (the `IndicatorColor` property gone, as InDesign leaves a document's condition when Synchronize
/// Book changes its method or appearance and the source's colour is one of its own: oracle bookx-c3-cond/cond2).
pub(crate) fn clear_color(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    let defs = definitions(w)?;
    let id = find(&defs, name)?.id.clone();
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let i = ix.by_self(&t, &escape(&id)).filter(|&i| ix.local(&t, i) == "Condition").ok_or_else(|| EditError::new("notFound", format!("condition {name}")))?;
    let Some(properties) = ix.children(i).iter().copied().find(|&k| ix.local(&t, k) == "Properties") else { return Ok(()) };
    let Some(color) = ix.children(properties).iter().copied().find(|&k| ix.local(&t, k) == "IndicatorColor") else { return Ok(()) };
    // the only property goes with its wrapper; the line break before it goes too
    let target = if ix.children(properties).len() == 1 { properties } else { color };
    let mut start = ix.nodes[target].start;
    while start > 0 && matches!(t.as_bytes()[start - 1], b'\t' | b' ') { start -= 1; }
    if start > 0 && t.as_bytes()[start - 1] == b'\n' { start -= 1; }
    w.splice(DM, vec![(start..ix.nodes[target].end, String::new())])
}

pub(crate) fn set_visible(w: &mut Work<'_>, name: &str, visible: &bool) -> Result<(), EditError> {
    let defs = definitions(w)?;
    let c = find(&defs, name)?;
    if c.visible == *visible { return Ok(()); }
    let before = hidden_ids(&defs);
    let mut after = before.clone();
    if *visible { after.remove(&c.id); } else { after.insert(c.id.clone()); }
    reflow(w, None, &before, &after)?;
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let i = ix.by_self(&t, &escape(&c.id)).ok_or_else(|| EditError::new("notFound", format!("condition {name}")))?;
    w.splice(DM, vec![attr_edit(&ix, &t, i, "Visible", if *visible { "true" } else { "false" })])
}

/// Token of a condition in `AppliedConditions` (a space would split the list).
fn token(id: &str) -> String { id.replace(' ', "%20") }

/// What a Condition Options colour value means: a UI colour (`Red`, `LIGHT_BLUE`, `lightblue`) or `r,g,b` (0..=255) for an own colour.
fn color_xml(value: &str) -> Result<String, EditError> {
    let norm = |s: &str| s.chars().filter(|c| *c != '_' && *c != ' ').flat_map(char::to_lowercase).collect::<String>();
    if let Some(name) = folio_model::UI_COLORS.iter().find(|c| norm(c) == norm(value)) {
        return Ok(format!("<IndicatorColor type=\"enumeration\">{name}</IndicatorColor>"));
    }
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    let numbers: Option<Vec<f64>> = if parts.len() == 3 {
        parts.iter().map(|p| p.parse::<f64>().ok().filter(|v| v.is_finite() && (0.0..=255.0).contains(v))).collect()
    } else { None };
    let Some(n) = numbers else { return Err(error(format!("unknown indicator colour {value} (a UI colour name or r,g,b from 0 to 255)"))) };
    let items: String = n.iter().map(|v| format!("\n\t\t\t\t<ListItem type=\"double\">{v}</ListItem>")).collect();
    Ok(format!("<IndicatorColor type=\"list\">{items}\n\t\t\t</IndicatorColor>"))
}

/// Condition Options: `props` holds any of `name` (the new name), `method` (`USE_UNDERLINE`, `USE_HIGHLIGHT`), `appearance`
/// (`WAVY`, `SOLID`, `DASHED`) and `color` (a UI colour name or `r,g,b`). The new id of a renamed condition is reported in `created`.
pub(crate) fn set_options(w: &mut Work<'_>, name: &str, props: &BTreeMap<String, String>) -> Result<(), EditError> {
    if let Some(k) = props.keys().find(|k| !matches!(k.as_str(), "name" | "method" | "appearance" | "color")) {
        return Err(error(format!("unknown condition option {k} (name, method, appearance, color)")));
    }
    let pick = |key: &str, table: &[(&str, &'static str)]| -> Result<Option<&'static str>, EditError> {
        let Some(v) = props.get(key) else { return Ok(None) };
        let norm = |s: &str| s.chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect::<String>();
        table.iter().find(|(k, _)| norm(k) == norm(v)).map(|(_, idml)| Some(*idml)).ok_or_else(|| error(format!("unknown {key} {v}")))
    };
    let method = pick("method", &[("USE_UNDERLINE", "UseUnderline"), ("USE_HIGHLIGHT", "UseHighlight")])?;
    let appearance = pick("appearance", &[("WAVY", "Wavy"), ("SOLID", "Solid"), ("DASHED", "Dashed")])?;
    let color = props.get("color").map(|c| color_xml(c)).transpose()?;
    let defs = definitions(w)?;
    let c = find(&defs, name)?.clone();
    let renamed = props.get("name").filter(|n| **n != c.name);
    if let Some(new) = renamed {
        if new.trim().is_empty() { return Err(error("condition name required")); }
        if new.trim() != new || new.chars().any(char::is_control) || new.contains(['%', '/']) {
            return Err(error("condition name has characters that are not measured (leading or trailing space, control characters, % or /)"));
        }
        if defs.iter().any(|d| d.name == **new) { return Err(error("A condition with that name exists already.")); }
    }
    let new_id = renamed.map(|n| format!("Condition/{n}"));
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let top = |tag: &str| (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == tag && ix.nodes[i].parent.is_some_and(|p| ix.nodes[p].parent.is_none())).collect::<Vec<_>>();
    if new_id.is_some() && !top("ConditionSet").is_empty() {
        return Err(unsupported("renaming a condition of a document with condition sets is not measured"));
    }
    let i = ix.by_self(&t, &escape(&c.id)).filter(|&i| ix.local(&t, i) == "Condition").ok_or_else(|| EditError::new("notFound", format!("condition {name}")))?;
    // the new text of the definition
    let n = &ix.nodes[i];
    let mut el = t[n.start..n.end].to_owned();
    let ex = Index::scan(&el).map_err(|e| EditError::new("xml", e))?;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    if let Some(m) = method { edits.push(attr_edit(&ex, &el, 0, "IndicatorMethod", m)); }
    if let Some(a) = appearance { edits.push(attr_edit(&ex, &el, 0, "UnderlineIndicatorAppearance", a)); }
    if let (Some(id), Some(new)) = (&new_id, renamed) {
        edits.push(attr_edit(&ex, &el, 0, "Self", id));
        edits.push(attr_edit(&ex, &el, 0, "Name", new));
    }
    if let Some(xml) = &color {
        if ex.nodes[0].empty { return Err(unsupported("a condition without Properties cannot take a colour yet")); }
        let props_node = ex.children(0).iter().copied().find(|&k| ex.local(&el, k) == "Properties");
        let old = props_node.and_then(|p| ex.children(p).iter().copied().find(|&k| ex.local(&el, k) == "IndicatorColor"));
        edits.push(match (props_node, old) {
            (_, Some(k)) => (ex.nodes[k].start..ex.nodes[k].end, xml.clone()),
            (Some(p), None) if !ex.nodes[p].empty => (ex.nodes[p].close_start..ex.nodes[p].close_start, format!("\t{xml}\n\t\t")),
            _ => return Err(unsupported("a condition without IndicatorColor cannot take a colour yet")),
        });
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    for (r, s) in edits { el.replace_range(r, &s); }
    // where it goes: the name order decides (condx-opt-rename-sort)
    let mut parts = Vec::new();
    match (&new_id, renamed) {
        (Some(_), Some(new)) => {
            let key = sort_key(new);
            let existing = top("Condition");
            let others: Vec<usize> = existing.iter().copied().filter(|&k| k != i).collect();
            let slot = others.iter().position(|&k| sort_key(&ix.value(&t, k, "Name").unwrap_or_default()) > key).unwrap_or(others.len());
            if existing.iter().position(|&k| k == i) == Some(slot) {
                parts.push((n.start..n.end, el));
            } else {
                let mut start = n.start;
                while start > 0 && matches!(t.as_bytes()[start - 1], b'\t' | b' ') { start -= 1; }
                if start > 0 && t.as_bytes()[start - 1] == b'\n' { start -= 1; }
                parts.push((start..n.end, String::new()));
                parts.push(match others.get(slot) {
                    Some(&k) => (ix.nodes[k].start..ix.nodes[k].start, format!("{el}\n\t")),
                    None => {
                        let last = *others.last().ok_or_else(|| unsupported("designmap has no place for conditions"))?;
                        (ix.nodes[last].end..ix.nodes[last].end, format!("\n\t{el}"))
                    }
                });
            }
        }
        _ => parts.push((n.start..n.end, el)),
    }
    w.splice(DM, parts)?;
    if let Some(id) = new_id {
        rename_tokens(w, &c.id, &id)?;
        w.created.push(id);
    }
    Ok(())
}

/// Every story range that names condition `old` names `new` instead (hidden text too).
fn rename_tokens(w: &mut Work<'_>, old: &str, new: &str) -> Result<(), EditError> {
    let parts: Vec<String> = w.ed.parts.keys().filter(|p| p.starts_with("Stories/Story_")).cloned().collect();
    for part in parts {
        let t = w.text(&part)?.to_owned();
        if !t.contains("AppliedConditions") { continue; }
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", format!("{part}: {e}")))?;
        let mut edits = Vec::new();
        for i in 0..ix.nodes.len() {
            let Some(list) = ix.value(&t, i, "AppliedConditions") else { continue };
            let before: Vec<&str> = list.split_whitespace().collect();
            let after: Vec<String> = before.iter().map(|x| if decode(x) == old { token(new) } else { (*x).to_owned() }).collect();
            if after.iter().map(String::as_str).ne(before.iter().copied()) { edits.push(attr_edit(&ix, &t, i, "AppliedConditions", &after.join(" "))); }
        }
        if !edits.is_empty() { w.splice(&part, edits)?; }
    }
    Ok(())
}

pub(crate) fn set_indicators(w: &mut Work<'_>, mode: &str) -> Result<(), EditError> {
    let value = match mode {
        "HIDE_INDICATORS" | "HideIndicators" => "HideIndicators",
        "SHOW_INDICATORS" | "ShowIndicators" => "ShowIndicators",
        "SHOW_AND_PRINT_INDICATORS" | "ShowAndPrintIndicators" => "ShowAndPrintIndicators",
        _ => return Err(error(format!("unknown indicator mode {mode}"))),
    };
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "ConditionalTextPreference").ok_or_else(|| unsupported("designmap has no ConditionalTextPreference"))?;
    w.splice(DM, vec![attr_edit(&ix, &t, i, "ShowConditionIndicators", value)])
}

/// The indicator mode as the panel shows it (`HIDE_INDICATORS` ...), `SHOW_INDICATORS` when the document says nothing.
pub(crate) fn indicator_mode(designmap: &str) -> Result<String, EditError> {
    let ix = Index::scan(designmap).map_err(|e| EditError::new("xml", e))?;
    let mode = (0..ix.nodes.len()).find(|&i| ix.local(designmap, i) == "ConditionalTextPreference")
        .and_then(|i| ix.value(designmap, i, "ShowConditionIndicators")).unwrap_or_else(|| "ShowIndicators".into());
    Ok(match mode.as_str() {
        "HideIndicators" => "HIDE_INDICATORS", "ShowAndPrintIndicators" => "SHOW_AND_PRINT_INDICATORS", _ => "SHOW_INDICATORS",
    }.into())
}

impl crate::Editor {
    /// Condition definitions and the indicator mode of the edited document.
    pub fn condition_state(&self) -> Result<(Vec<Condition>, String), EditError> {
        let dm = self.part(DM).ok_or_else(|| error("missing designmap"))?;
        let defs = folio_idml::conditions::read(&self.parts).map_err(|e| EditError::new("xml", e))?;
        Ok((defs, indicator_mode(dm)?))
    }
}

/// Rewrites every story for a change of the definitions: `drop` leaves the condition out of all lists, and text moves into
/// or out of `HiddenText` as `hidden_before` becomes `hidden_after`.
fn reflow(w: &mut Work<'_>, drop: Option<&str>, hidden_before: &HashSet<String>, hidden_after: &HashSet<String>) -> Result<(), EditError> {
    let parts: Vec<String> = w.ed.parts.keys().filter(|p| p.starts_with("Stories/Story_")).cloned().collect();
    for part in parts {
        let t = w.text(&part)?.to_owned();
        if !t.contains("AppliedConditions") { continue; }
        let edits = story_edits(&t, drop, hidden_before, hidden_after).map_err(|e| EditError::new(e.code, format!("{part}: {}", e.message)))?;
        if edits.is_empty() { continue; }
        w.splice(&part, edits)?;
        if let Some(story) = part.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")) {
            w.ed.text_state.remove(story);
            w.recompose(story);
        }
    }
    Ok(())
}

struct Csr {
    tokens: Vec<String>,
    changed: bool,
    /// Text that becomes hidden and still sits in the flow.
    hide: bool,
    /// Text inside `HiddenText` that shows again.
    reveal: bool,
    /// The `HiddenText` this range is in.
    hidden_in: Option<usize>,
}

fn attributes(t: &str, ix: &Index, i: usize) -> Vec<(String, String)> {
    let mut v: Vec<_> = ix.attrs(t, i).into_iter().map(|a| (t[a.name.0..a.name.1].to_owned(), t[a.value.0..a.value.1].to_owned())).collect();
    v.sort();
    v
}

/// Ranges that only hold text can move; a paragraph break, note, variable or object inside cannot.
fn plain(ix: &Index, t: &str, i: usize) -> Result<(), EditError> {
    for &k in ix.children(i) {
        let tag = ix.local(t, k);
        if tag != "Content" && tag != "Properties" {
            return Err(unsupported(if tag == "Br" { "hiding or showing text across a paragraph break is not measured".into() } else { format!("hiding or showing text with {tag} inside is not measured") }));
        }
    }
    Ok(())
}

/// The range as text with its `AppliedConditions` as `c` has them (`blank`: none at all).
fn render(t: &str, ix: &Index, i: usize, c: &Csr, blank: bool) -> String {
    let n = &ix.nodes[i];
    let mut s = t[n.start..n.end].to_owned();
    if !c.changed && !blank { return s; }
    if let Some(a) = ix.attrs(t, i).into_iter().find(|a| &t[a.name.0..a.name.1] == "AppliedConditions") {
        let off = n.start;
        if blank || c.tokens.is_empty() {
            let mut st = a.name.0;
            while st > off && t.as_bytes()[st - 1].is_ascii_whitespace() { st -= 1; }
            s.replace_range(st - off..a.value.1 + 1 - off, "");
        } else {
            s.replace_range(a.value.0 - off..a.value.1 - off, &escape(&c.tokens.join(" ")));
        }
    }
    s
}

/// The start tag of a range and its `Properties`, without `AppliedConditions`: what the placeholder of hidden text looks like.
fn placeholder_head(t: &str, ix: &Index, i: usize, c: &Csr) -> String {
    let blank = render(t, ix, i, c, true);
    let props = ix.children(i).iter().find(|&&k| ix.local(t, k) == "Properties").map(|&k| &t[ix.nodes[k].start..ix.nodes[k].end]).unwrap_or_default();
    let tag_end = blank.find('>').map_or(0, |e| e + 1);
    format!("{}{props}", &blank[..tag_end])
}

fn story_edits(t: &str, drop: Option<&str>, before: &HashSet<String>, after: &HashSet<String>) -> Result<Vec<(Range<usize>, String)>, EditError> {
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let mut info: HashMap<usize, Csr> = HashMap::new();
    for i in 0..ix.nodes.len() {
        if ix.local(t, i) != "CharacterStyleRange" { continue; }
        let old: Vec<String> = ix.value(t, i, "AppliedConditions").map(|r| r.split_whitespace().map(str::to_owned).collect()).unwrap_or_default();
        let tokens: Vec<String> = old.iter().filter(|x| drop.is_none_or(|d| decode(x) != d)).cloned().collect();
        let all_in = |list: &[String], set: &HashSet<String>| !list.is_empty() && list.iter().all(|x| set.contains(&decode(x)));
        let (was, now) = (all_in(&old, before), all_in(&tokens, after));
        let hidden_in = ix.ancestors(i).find(|&a| ix.local(t, a) == "HiddenText");
        info.insert(i, Csr { changed: tokens.len() != old.len(), tokens, hide: now && !was && hidden_in.is_none(), reveal: !now && hidden_in.is_some(), hidden_in });
    }
    // HiddenText elements whose ranges all show again (a partly shown one is refused)
    let mut revealed_hosts: HashMap<usize, usize> = HashMap::new();
    for h in (0..ix.nodes.len()).filter(|&h| ix.local(t, h) == "HiddenText") {
        let members: Vec<usize> = info.iter().filter(|(_, c)| c.hidden_in == Some(h)).map(|(&i, _)| i).collect();
        let shown = members.iter().filter(|&&i| info[&i].reveal).count();
        if shown == 0 { continue; }
        if shown != members.len() { return Err(unsupported("only part of a hidden text becoming visible is not measured")); }
        let host = ix.nodes[h].parent.filter(|&p| ix.local(t, p) == "CharacterStyleRange").ok_or_else(|| unsupported("hidden text outside a character range is not measured"))?;
        if info[&host].changed { return Err(unsupported("hidden text inside a conditional range is not measured")); }
        if revealed_hosts.insert(host, h).is_some() { return Err(unsupported("several hidden texts in one range showing together is not measured")); }
    }
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    // ranges that only lose a token stay where they are
    for (&i, c) in &info {
        if !c.changed || c.hide || c.reveal { continue; }
        edits.push(if c.tokens.is_empty() {
            attr_removal(&ix, t, i, "AppliedConditions").ok_or_else(|| error("AppliedConditions vanished"))?
        } else {
            attr_edit(&ix, t, i, "AppliedConditions", &c.tokens.join(" "))
        });
    }
    // paragraphs that lose, gain or move text are rebuilt: InDesign writes neighbouring ranges of equal attributes as one
    // (condx-delete-both), and hidden text sits in a range of the attributes it had, minus the conditions (condx-hide-two-ranges)
    let touched = |k: usize| info.get(&k).is_some_and(|c| c.changed || c.hide || c.reveal) || revealed_hosts.contains_key(&k);
    // hidden runs that take a paragraph break with them rewrite several paragraphs at once
    let groups = para::find_groups(t, &ix, &info, &revealed_hosts)?;
    let in_group: HashMap<usize, usize> = groups.iter().enumerate().flat_map(|(g, x)| x.paras.iter().map(move |&p| (p, g))).collect();
    for p in (0..ix.nodes.len()).rev().filter(|&p| ix.local(t, p) == "ParagraphStyleRange") {
        if let Some(&g) = in_group.get(&p) {
            if groups[g].paras[0] == p {
                let (range, text) = para::hide(t, &ix, &info, &groups[g], &edits, &touched)?;
                edits.retain(|(r, _)| !(r.start >= range.start && r.end <= range.end));
                edits.push((range, text));
            }
            continue;
        }
        let kids = ix.children(p);
        if !kids.iter().any(|&k| touched(k)) { continue; }
        let mut pieces: Vec<Piece> = Vec::new();
        let mut shown: Option<para::Shown> = None;
        let mut k = 0;
        while k < kids.len() {
            let kid = kids[k];
            if info.get(&kid).is_some_and(|c| c.hide) {
                let first = k;
                while k < kids.len() && info.get(&kids[k]).is_some_and(|c| c.hide) { k += 1; }
                let group = &kids[first..k];
                for &g in group { plain(&ix, t, g)?; }
                let open = &t[ix.nodes[p].start..ix.nodes[p].open_end];
                let hidden: String = group.iter().map(|&g| render(t, &ix, g, &info[&g], false)).collect();
                let last = *group.last().unwrap();
                let text = format!("{}<HiddenText>{open}{hidden}</ParagraphStyleRange></HiddenText></CharacterStyleRange>", placeholder_head(t, &ix, last, &info[&last]));
                pieces.push(Piece::new(text, true));
                continue;
            }
            k += 1;
            if let Some(&h) = revealed_hosts.get(&kid) {
                if ix.children(h).len() > 1 {
                    if shown.is_some() { return Err(unsupported("several hidden texts over paragraph breaks in one paragraph are not measured")); }
                    let (host, hn) = (&ix.nodes[kid], &ix.nodes[h]);
                    let left = format!("{}</CharacterStyleRange>", apply(t, &(host.start..hn.start), &edits));
                    let right = format!("{}{}", &t[host.start..host.open_end], apply(t, &(hn.end..host.end), &edits));
                    shown = Some(para::Shown { hidden: h, at: pieces.len(), left, right });
                    continue;
                }
                let [para] = ix.children(h) else { return Err(unsupported("hidden text over several paragraphs is not measured")) };
                if ix.local(t, *para) != "ParagraphStyleRange" || attributes(t, &ix, p) != attributes(t, &ix, *para) {
                    return Err(unsupported("hidden text of another paragraph style is not measured"));
                }
                let (host, hn) = (&ix.nodes[kid], &ix.nodes[h]);
                let left = format!("{}</CharacterStyleRange>", apply(t, &(host.start..hn.start), &edits));
                let right = format!("{}{}", &t[host.start..host.open_end], apply(t, &(hn.end..host.end), &edits));
                pieces.push(Piece::new(left, true));
                for &r in ix.children(*para) {
                    if ix.local(t, r) != "CharacterStyleRange" { return Err(unsupported("hidden text with other content is not measured")); }
                    plain(&ix, t, r)?;
                    pieces.push(Piece::new(render(t, &ix, r, &info[&r], false), true));
                }
                pieces.push(Piece::new(right, true));
                continue;
            }
            let text = apply(t, &(ix.nodes[kid].start..ix.nodes[kid].end), &edits);
            pieces.push(Piece::new(text, touched(kid)));
        }
        if let Some(s) = shown {
            // a hidden text over paragraph breaks comes back as the paragraphs it was
            let whole = ix.nodes[p].start..ix.nodes[p].end;
            let paragraphs = para::show(t, &ix, &info, p, &s, pieces)?;
            let text = paragraphs.into_iter()
                .map(|(open, ps)| format!("{open}{}</ParagraphStyleRange>", merge(ps).into_iter().map(|m| m.text).collect::<Vec<_>>().join("\n")))
                .collect::<Vec<_>>().join("\n");
            edits.retain(|(r, _)| !(r.start >= whole.start && r.end <= whole.end));
            edits.push((whole, text));
            continue;
        }
        let inner = ix.nodes[p].open_end..ix.nodes[p].close_start;
        edits.retain(|(r, _)| !(r.start >= inner.start && r.end <= inner.end));
        edits.push((inner, merge(pieces).into_iter().map(|m| m.text).collect::<Vec<_>>().join("\n")));
    }
    Ok(edits)
}

/// The pieces of a paragraph without the empty ones, neighbours of equal attributes written as one.
fn merge(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut merged: Vec<Piece> = Vec::new();
    for piece in pieces.into_iter().filter(|p| !p.empty()) {
        match merged.last_mut() {
            Some(last) if last.joins(&piece) => last.absorb(piece),
            _ => merged.push(piece),
        }
    }
    merged
}

/// `text` of a source range with the edits inside it applied (insertions belong to the range they start in).
fn apply(t: &str, range: &Range<usize>, edits: &[(Range<usize>, String)]) -> String {
    let mut inside: Vec<&(Range<usize>, String)> = edits.iter()
        .filter(|(r, _)| if r.start == r.end { r.start >= range.start && r.start < range.end } else { r.start >= range.start && r.end <= range.end }).collect();
    inside.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut s = t[range.clone()].to_owned();
    for (r, new) in inside { s.replace_range(r.start - range.start..r.end - range.start, new); }
    s
}

/// One child of a paragraph as text; a character range that only holds text can join its neighbour.
struct Piece {
    text: String,
    /// Attributes and `Properties` of the range, where its content starts and where it ends, whether it holds a `HiddenText`.
    plain: Option<(Vec<(String, String)>, String, usize, usize, bool)>,
    touched: bool,
}

impl Piece {
    fn new(text: String, touched: bool) -> Piece {
        let plain = Index::scan(&text).ok().filter(|ix| ix.nodes.first().is_some_and(|n| ix.local(&text, 0) == "CharacterStyleRange" && n.end == text.len() && !n.empty)).and_then(|ix| {
            if ix.children(0).iter().any(|&c| !matches!(ix.local(&text, c), "Properties" | "Content" | "Br" | "HiddenText")) { return None; }
            let props = ix.children(0).iter().find(|&&c| ix.local(&text, c) == "Properties").copied();
            let from = props.map_or(ix.nodes[0].open_end, |c| ix.nodes[c].end);
            let hidden = ix.children(0).iter().any(|&c| ix.local(&text, c) == "HiddenText");
            let props = props.map(|c| text[ix.nodes[c].start..ix.nodes[c].end].to_owned()).unwrap_or_default();
            Some((attributes(&text, &ix, 0), props, from, ix.nodes[0].close_start, hidden))
        });
        Piece { text, plain, touched }
    }
    /// A range with nothing in it (only a start tag and `Properties`) is not written.
    fn empty(&self) -> bool {
        self.plain.as_ref().is_some_and(|(_, _, from, close, _)| self.text[*from..*close].trim().is_empty())
    }
    fn joins(&self, next: &Piece) -> bool {
        (self.touched || next.touched) && !self.text.rsplit("</HiddenText>").next().unwrap_or_default().contains("<Br") && matches!((&self.plain, &next.plain), (Some(a), Some(b)) if a.0 == b.0 && a.1 == b.1 && !(a.4 && b.4))
    }
    fn absorb(&mut self, next: Piece) {
        let close = self.plain.as_ref().unwrap().3;
        let (from, end) = { let n = next.plain.as_ref().unwrap(); (n.2, n.3) };
        // two Content elements in a row are one (condx-delete-both: "Al" and "pha " become "Alpha ")
        let (a, b) = (self.text[..close].trim_end(), next.text[from..end].trim_start());
        let body = match (a.strip_suffix("</Content>"), b.strip_prefix("<Content>")) {
            (Some(a), Some(b)) => format!("{a}{b}"),
            _ => format!("{a}{b}"),
        };
        self.text = format!("{body}{}", &self.text[close..]);
        self.touched |= next.touched;
        self.plain = Piece::new(self.text.clone(), self.touched).plain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Editor};
    use crate::commands::RegisteredCommand as R;
    use std::collections::BTreeMap;

    const DESIGNMAP: &str = "<Document>\n\t<NamedGrid Self=\"g\"/>\n\t<Condition Self=\"Condition/Web\" Name=\"Web\" IndicatorMethod=\"UseUnderline\" UnderlineIndicatorAppearance=\"Wavy\" Visible=\"true\">\n\t\t<Properties>\n\t\t\t<IndicatorColor type=\"enumeration\">LightBlue</IndicatorColor>\n\t\t</Properties>\n\t</Condition>\n\t<ConditionalTextPreference ShowConditionIndicators=\"ShowIndicators\" ActiveConditionSet=\"n\" />\n</Document>";
    const STORY: &str = "<Story Self=\"s\"><ParagraphStyleRange AppliedParagraphStyle=\"P\"><CharacterStyleRange AppliedConditions=\"Condition/Web\"><Content>ab</Content><Br /></CharacterStyleRange></ParagraphStyleRange><ParagraphStyleRange AppliedParagraphStyle=\"P\"><CharacterStyleRange><Content>cd</Content></CharacterStyleRange></ParagraphStyleRange></Story>";

    fn editor() -> Editor {
        Editor::new(BTreeMap::from([("designmap.xml".to_owned(), DESIGNMAP.to_owned()), ("Stories/Story_s.xml".to_owned(), STORY.to_owned())]))
    }

    fn run(e: &mut Editor, c: R) -> Result<crate::Outcome, EditError> { e.apply("test", &[Command::Registered(c)]) }

    #[test]
    fn colour_is_the_first_unused_and_falls_back_to_light_blue() {
        let with = |colors: &[&str]| colors.iter().map(|c| Condition { indicator_color: (*c).into(), ..Condition::default() }).collect::<Vec<_>>();
        assert_eq!(next_color(&[]), "LightBlue");
        assert_eq!(next_color(&with(&["Red", "LightBlue"])), "Green");
        assert_eq!(next_color(&with(&["Red"])), "LightBlue");
        let all: Vec<_> = folio_model::UI_COLORS[..PALETTE].to_vec();
        assert_eq!(next_color(&with(&all)), "LightBlue");
    }

    #[test]
    fn names_sort_like_indesign_and_new_ones_go_in_order() {
        assert!(sort_key("Print") < sort_key("web") && sort_key("web") < sort_key("Web"));
        let mut e = editor();
        let out = run(&mut e, R::CreateCondition { name: "Alpha".into() }).unwrap();
        assert_eq!(out.created, vec!["Condition/Alpha"]);
        let (defs, _) = e.condition_state().unwrap();
        assert_eq!(defs.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Alpha", "Web"]);
        assert_eq!(defs[0].indicator_color, "Red");
        run(&mut e, R::CreateCondition { name: "Zed".into() }).unwrap();
        assert_eq!(e.condition_state().unwrap().0.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Alpha", "Web", "Zed"]);
    }

    #[test]
    fn refuses_duplicates_and_unmeasured_names_without_changing_anything() {
        let mut e = editor();
        for name in ["Web", "", " x", "a%b", "a/b"] {
            assert!(run(&mut e, R::CreateCondition { name: name.into() }).is_err(), "{name:?}");
        }
        assert_eq!(e.parts()["designmap.xml"], DESIGNMAP);
    }

    fn squeeze(s: &str) -> String { s.chars().filter(|c| !c.is_whitespace()).collect() }

    #[test]
    fn hiding_a_paragraph_with_its_break_and_showing_it_again_gives_the_text_back() {
        let mut e = editor();
        run(&mut e, R::SetConditionVisible { name: "Web".into(), visible: false }).unwrap();
        let hidden = squeeze(&e.parts()["Stories/Story_s.xml"]);
        // the first paragraph goes into the HiddenText of the next one (condx-hide-para-whole-br): its own break, then an empty
        // paragraph of the default style that stands for the paragraph that followed
        assert!(hidden.contains("<ParagraphStyleRangeAppliedParagraphStyle=\"P\"><CharacterStyleRange><HiddenText><ParagraphStyleRangeAppliedParagraphStyle=\"P\"><CharacterStyleRangeAppliedConditions=\"Condition/Web\"><Content>ab</Content><Br/></CharacterStyleRange></ParagraphStyleRange><ParagraphStyleRangeAppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\">"), "{hidden}");
        assert!(hidden.ends_with("<Content>cd</Content></CharacterStyleRange></ParagraphStyleRange></Story>"), "{hidden}");
        run(&mut e, R::SetConditionVisible { name: "Web".into(), visible: true }).unwrap();
        // two neighbouring paragraphs of the same start tag are one ParagraphStyleRange in the file, the way InDesign writes
        // them (condx-show-para-br-bold); a character range that ends in a Br is not merged with the one after it
        let shown = squeeze(&e.parts()["Stories/Story_s.xml"]);
        assert_eq!(shown, squeeze("<Story Self=\"s\"><ParagraphStyleRange AppliedParagraphStyle=\"P\"><CharacterStyleRange AppliedConditions=\"Condition/Web\"><Content>ab</Content><Br /></CharacterStyleRange><CharacterStyleRange><Content>cd</Content></CharacterStyleRange></ParagraphStyleRange></Story>"), "{shown}");
        // nothing but that paragraph range boundary differs from the start: same text, same break, same conditions and styles
        let without_boundary = |s: &str| s.replace("</ParagraphStyleRange><ParagraphStyleRangeAppliedParagraphStyle=\"P\">", "");
        assert_eq!(without_boundary(&shown), without_boundary(&squeeze(STORY)));
        // one undo per command
        e.undo().unwrap();
        assert_eq!(squeeze(&e.parts()["Stories/Story_s.xml"]), hidden);
        e.undo().unwrap();
        assert_eq!(e.parts()["designmap.xml"], DESIGNMAP);
        assert_eq!(e.parts()["Stories/Story_s.xml"], STORY);
    }

    #[test]
    fn text_from_the_middle_of_a_paragraph_over_a_break_joins_the_paragraphs_and_comes_apart_again() {
        let story = "<Story Self=\"s\"><ParagraphStyleRange AppliedParagraphStyle=\"A\"><CharacterStyleRange><Content>one </Content></CharacterStyleRange><CharacterStyleRange AppliedConditions=\"Condition/Web\"><Content>two</Content><Br /></CharacterStyleRange></ParagraphStyleRange><ParagraphStyleRange AppliedParagraphStyle=\"B\"><CharacterStyleRange AppliedConditions=\"Condition/Web\"><Content>thr</Content></CharacterStyleRange><CharacterStyleRange><Content>ee</Content></CharacterStyleRange></ParagraphStyleRange></Story>";
        let mut e = Editor::new(BTreeMap::from([("designmap.xml".to_owned(), DESIGNMAP.to_owned()), ("Stories/Story_s.xml".to_owned(), story.to_owned())]));
        run(&mut e, R::SetConditionVisible { name: "Web".into(), visible: false }).unwrap();
        let hidden = squeeze(&e.parts()["Stories/Story_s.xml"]);
        // one paragraph of the first style holds what stays: "one " and "ee" (condx-hide-para-mid)
        assert_eq!(hidden.matches("<ParagraphStyleRangeAppliedParagraphStyle=\"A\">").count(), 2, "{hidden}");
        assert!(hidden.contains("<Content>one</Content><HiddenText>") && hidden.contains("</HiddenText><Content>ee</Content>"), "{hidden}");
        run(&mut e, R::SetConditionVisible { name: "Web".into(), visible: true }).unwrap();
        assert_eq!(squeeze(&e.parts()["Stories/Story_s.xml"]), squeeze(story));
    }

    #[test]
    fn deleting_a_condition_drops_its_tokens_and_the_attribute() {
        let mut e = editor();
        run(&mut e, R::DeleteCondition { name: "Web".into() }).unwrap();
        assert!(!e.parts()["designmap.xml"].contains("Condition/Web"));
        assert!(!e.parts()["Stories/Story_s.xml"].contains("AppliedConditions"));
        e.undo().unwrap();
        assert_eq!(e.parts()["designmap.xml"], DESIGNMAP);
        assert_eq!(e.parts()["Stories/Story_s.xml"], STORY);
    }

    fn options(name: &str, props: &[(&str, &str)]) -> R {
        R::SetConditionOptions { name: name.into(), props: props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect() }
    }

    #[test]
    fn options_refuse_duplicates_and_unknown_values_without_changing_anything() {
        let mut e = editor();
        run(&mut e, R::CreateCondition { name: "Alpha".into() }).unwrap();
        let before = e.parts()["designmap.xml"].clone();
        for props in [&[("name", "Web")][..], &[("name", " x")], &[("name", "")], &[("method", "bold")], &[("appearance", "dotted")], &[("color", "300,0,0")], &[("color", "nope")], &[("size", "3")]] {
            assert!(run(&mut e, options("Alpha", props)).is_err(), "{props:?}");
        }
        assert!(run(&mut e, options("Missing", &[("method", "USE_HIGHLIGHT")])).is_err());
        assert_eq!(e.parts()["designmap.xml"], before);
        // the same name is no change at all
        run(&mut e, options("Alpha", &[("name", "Alpha")])).unwrap();
        assert_eq!(e.parts()["designmap.xml"], before);
    }

    #[test]
    fn options_set_method_appearance_and_both_kinds_of_colour_and_undo() {
        let mut e = editor();
        let start = e.parts()["designmap.xml"].clone();
        run(&mut e, options("Web", &[("method", "USE_HIGHLIGHT"), ("appearance", "dashed"), ("color", "LIGHT_GRAY")])).unwrap();
        let (defs, _) = e.condition_state().unwrap();
        assert_eq!((defs[0].indicator_method.as_str(), defs[0].underline_appearance.as_str(), defs[0].indicator_color.as_str()), ("UseHighlight", "Dashed", "LightGray"));
        run(&mut e, options("Web", &[("color", "10, 200.5, 30")])).unwrap();
        assert_eq!(e.condition_state().unwrap().0[0].indicator_color, "10,200.5,30");
        run(&mut e, options("Web", &[("color", "Red")])).unwrap();
        assert_eq!(e.condition_state().unwrap().0[0].indicator_color, "Red");
        while e.undo().is_some() {}
        assert_eq!(e.parts()["designmap.xml"], start);
    }

    #[test]
    fn renaming_moves_the_definition_in_name_order_and_follows_every_reference() {
        let mut e = editor();
        run(&mut e, R::CreateCondition { name: "Alpha".into() }).unwrap();
        let start = (e.parts()["designmap.xml"].clone(), e.parts()["Stories/Story_s.xml"].clone());
        let out = run(&mut e, options("Web", &[("name", "My Zed")])).unwrap();
        assert_eq!(out.created, vec!["Condition/My Zed"]);
        let (defs, _) = e.condition_state().unwrap();
        assert_eq!(defs.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect::<Vec<_>>(), [("Condition/Alpha", "Alpha"), ("Condition/My Zed", "My Zed")]);
        assert!(e.parts()["Stories/Story_s.xml"].contains("AppliedConditions=\"Condition/My%20Zed\""));
        // and back under the first place by name
        run(&mut e, options("Condition/My Zed", &[("name", "0")])).unwrap();
        assert_eq!(e.condition_state().unwrap().0.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["0", "Alpha"]);
        e.undo().unwrap();
        e.undo().unwrap();
        assert_eq!((e.parts()["designmap.xml"].clone(), e.parts()["Stories/Story_s.xml"].clone()), start);
    }

    #[test]
    fn indicator_mode_is_stored_in_the_preference() {
        let mut e = editor();
        run(&mut e, R::SetConditionIndicators { mode: "SHOW_AND_PRINT_INDICATORS".into() }).unwrap();
        assert!(e.parts()["designmap.xml"].contains("ShowConditionIndicators=\"ShowAndPrintIndicators\""));
        assert_eq!(e.condition_state().unwrap().1, "SHOW_AND_PRINT_INDICATORS");
        assert!(run(&mut e, R::SetConditionIndicators { mode: "bogus".into() }).is_err());
    }
}
