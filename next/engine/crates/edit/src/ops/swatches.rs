//! The Swatches panel, as InDesign does it (next/oracle/scenarios/basic-swatches.json):
//! - a swatch is an element of Resources/Graphic.xml whose Self is `Color/<name>` (`%` written
//!   `%25`); Graphic.xml keeps each kind's elements sorted by Self, so a new or renamed swatch
//!   goes to its sorted place (swatch-new-*, swatch-rename);
//! - the Swatches panel order is designmap's root ColorGroup: positional ColorGroupSwatch slots
//!   (`u12ColorGroupSwatch0`, `…1`, … in hex) whose SwatchItemRef names the swatch, and each
//!   swatch points back to its slot (SwatchColorGroupReference). A new swatch takes a new last
//!   slot; deleting one shifts the later swatches down a slot and drops the last slot
//!   (swatch-delete-replace);
//! - renaming changes Self, so every reference to it changes; deleting puts the replacement
//!   in its place everywhere (swatch-rename, swatch-delete-replace);
//! - a spot colour has an Ink of the same name (Angle 45, Frequency 70, the next TrapOrder).
//!   Its NeutralDensity is InDesign's own calculation, not matched yet (known-gaps.json): Folio
//!   writes the process inks' densities weighted by the colour's values.
use super::*;

const GRAPHIC: &str = "Resources/Graphic.xml";
const DM: &str = "designmap.xml";

pub(super) fn scan(w: &Work<'_>, part: &str) -> Result<(String, Index), EditError> {
    let t = w.text(part)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    Ok((t, ix))
}

/// `Color/<name>` with InDesign's escaping of the name.
pub(super) fn self_of(kind: &str, name: &str) -> String {
    format!("{kind}/{}", name.replace('%', "%25"))
}

fn values(v: &[f64]) -> String {
    v.iter().map(|x| geom::num(*x)).collect::<Vec<_>>().join(" ")
}

/// InDesign's hidden unnamed process black (`Name="$ID/"`, not in the Swatches panel, CMYK
/// 0 0 0 100), which every InDesign document carries: the colour a stroke gets when a weight
/// is given to an object without one (oracle: stroke-weight-unstroked).
pub(super) fn hidden_black(w: &Work<'_>) -> Result<Option<String>, EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let is = |i: usize, k: &str, v: &str| ix.attr(&t, i, k) == Some(v);
    Ok(of_kind(&ix, &t, "Color").into_iter()
        .find(|&i| is(i, "Name", "$ID/") && is(i, "Visible", "false") && is(i, "Model", "Process") && is(i, "Space", "CMYK")
            && ix.attr(&t, i, "ColorValue").is_some_and(|v| v.split_whitespace().map(|n| n.parse::<f64>().ok()).eq([Some(0.0), Some(0.0), Some(0.0), Some(100.0)])))
        .and_then(|i| ix.attr(&t, i, "Self").map(str::to_owned)))
}

/// The elements of one kind in Graphic.xml (Color, Tint, Ink…), in order.
pub(super) fn of_kind(ix: &Index, t: &str, kind: &str) -> Vec<usize> {
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == kind && ix.attr(t, i, "Self").is_some()).collect()
}

/// Where a new element of `kind` with this Self goes: its sorted place among its kind, or
/// (first of its kind) before `fallback_before`.
fn sorted_slot(ix: &Index, t: &str, kind: &str, me: &str, fallback_before: &str) -> Result<usize, EditError> {
    let same = of_kind(ix, t, kind);
    if let Some(&next) = same.iter().find(|&&i| ix.attr(t, i, "Self").unwrap_or_default() > me) {
        return Ok(element_span(ix, t, next).start);
    }
    if let Some(&last) = same.last() {
        return Ok(ix.nodes[last].end);
    }
    let before = of_kind(ix, t, fallback_before).first().copied()
        .ok_or_else(|| EditError::new("unsupported", format!("Graphic.xml has no {fallback_before}")))?;
    Ok(element_span(ix, t, before).start)
}

fn indent_of_kind(ix: &Index, t: &str) -> String {
    of_kind(ix, t, "Color").first().map(|&i| indent_before(t, ix.nodes[i].start).to_owned()).unwrap_or_else(|| "\t".into())
}

/// The root ColorGroup's slots: (node, slot Self, SwatchItemRef).
pub(super) fn slots(ix: &Index, t: &str) -> Result<Vec<(usize, String, String)>, EditError> {
    let root = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "ColorGroup" && ix.attr(t, i, "IsRootColorGroup") == Some("true"))
        .ok_or_else(|| EditError::new("unsupported", "designmap has no root colour group"))?;
    Ok(ix.children(root).iter().filter(|&&c| ix.local(t, c) == "ColorGroupSwatch")
        .map(|&c| (c, ix.attr(t, c, "Self").unwrap_or_default().to_owned(), ix.value(t, c, "SwatchItemRef").unwrap_or_default()))
        .collect())
}

/// Appends a slot for `swatch`; returns its Self.
pub(super) fn add_slot(w: &mut Work<'_>, swatch: &str) -> Result<String, EditError> {
    let (t, ix) = scan(w, DM)?;
    let all = slots(&ix, &t)?;
    let (last, last_id, _) = all.last().cloned().ok_or_else(|| EditError::new("unsupported", "empty root colour group"))?;
    let stem = last_id.trim_end_matches(|c: char| c.is_ascii_hexdigit());
    let next = all.iter().filter_map(|(_, id, _)| u64::from_str_radix(&id[stem.len().min(id.len())..], 16).ok()).max().map_or(0, |n| n + 1);
    let id = format!("{stem}{next:x}");
    let ind = indent_before(&t, ix.nodes[last].start).to_owned();
    let at = ix.nodes[last].end;
    w.splice(DM, vec![(at..at, format!("\n{ind}<ColorGroupSwatch Self=\"{id}\" SwatchItemRef=\"{}\" />", escape(swatch)))])?;
    Ok(id)
}

fn exists(w: &Work<'_>, me: &str) -> Result<bool, EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    Ok(ix.by_self(&t, me).is_some())
}

pub(super) fn insert(w: &mut Work<'_>, kind: &str, me: &str, element: &str, fallback_before: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let at = sorted_slot(&ix, &t, kind, me, fallback_before)?;
    let ind = indent_of_kind(&ix, &t);
    w.splice(GRAPHIC, vec![(at..at, format!("\n{ind}{element}"))])
}

/// Where the next Ink goes and its TrapOrder: after the last Ink, the next order. A document with no Ink at all (one File > New made before
/// it had the four process inks, mcp-c6) gets them first, after its last colour where InDesign has them (`folio_idml::inks`), so a spot
/// ink is the fifth as in InDesign. Returns (the place, the indentation, the order).
pub(super) fn next_ink_slot(w: &mut Work<'_>) -> Result<(usize, String, u32), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    if of_kind(&ix, &t, "Ink").is_empty() {
        let last = *of_kind(&ix, &t, "Color").last().ok_or_else(|| EditError::new("unsupported", "Graphic.xml has no colour to put the process inks after"))?;
        let (at, ind) = (ix.nodes[last].end, indent_before(&t, ix.nodes[last].start).to_owned());
        let inks: String = folio_idml::inks::PROCESS_INKS.iter().map(|ink| format!("\n{ind}{ink}")).collect();
        w.splice(GRAPHIC, vec![(at..at, inks)])?;
    }
    let (t, ix) = scan(w, GRAPHIC)?;
    let inks = of_kind(&ix, &t, "Ink");
    let last = *inks.last().ok_or_else(|| EditError::new("xml", "the process inks did not go in"))?;
    let order = inks.iter().filter_map(|&i| ix.attr(&t, i, "TrapOrder").and_then(|v| v.parse::<u32>().ok())).max().unwrap_or(0) + 1;
    Ok((ix.nodes[last].end, indent_before(&t, ix.nodes[last].start).to_owned(), order))
}

/// A spot colour's Ink (the next TrapOrder).
fn add_ink(w: &mut Work<'_>, name: &str, value: &[f64], space: &str) -> Result<(), EditError> {
    let (at, ind, order) = next_ink_slot(w)?;
    // not InDesign's calculation (known gap): the process inks' 0.61/0.76/0.16/1.7 weighted
    let nd = if space == "CMYK" && value.len() == 4 {
        [0.61, 0.76, 0.16, 1.7].iter().zip(value).map(|(d, v)| d * v / 100.0).sum::<f64>()
    } else { 0.5 };
    let me = self_of("Ink", name);
    w.splice(GRAPHIC, vec![(at..at, format!(
        "\n{ind}<Ink Self=\"{}\" Name=\"{}\" Angle=\"45\" ConvertToProcess=\"false\" Frequency=\"70\" NeutralDensity=\"{}\" PrintInk=\"true\" TrapOrder=\"{order}\" InkType=\"Normal\" />",
        escape(&me), escape(name), geom::num(nd)))])
}

fn space_name(space: &str) -> Result<&'static str, EditError> {
    match space {
        "cmyk" | "CMYK" => Ok("CMYK"),
        "rgb" | "RGB" => Ok("RGB"),
        "lab" | "LAB" => Ok("LAB"),
        other => Err(EditError::new("invalid", format!("colour space {other}"))),
    }
}

pub(super) fn new_swatch(w: &mut Work<'_>, name: &str, spot: bool, space: &str, value: &[f64]) -> Result<(), EditError> {
    new_swatch_in(w, name, spot, space, value, true)
}

/// A swatch that is in the colour group (`grouped`) or in none (`SwatchColorGroupReference="n"`).
pub(super) fn new_swatch_in(w: &mut Work<'_>, name: &str, spot: bool, space: &str, value: &[f64], grouped: bool) -> Result<(), EditError> {
    // an HSB swatch (the Color Picker's "Add HSB Swatch", measured in InDesign 2025) is an RGB colour with ConvertToHsb="true": `value` is its RGB
    let hsb = !spot && space.eq_ignore_ascii_case("hsb");
    let space = if hsb { "RGB" } else { space_name(space)? };
    let me = self_of("Color", name);
    if name.is_empty() || exists(w, &me)? {
        return Err(EditError::new("invalid", format!("a swatch named {name} already exists")));
    }
    let slot = if grouped { add_slot(w, &me)? } else { "n".to_owned() };
    let alias = if spot { " SpotInkAliasSpotColorReference=\"n\"" } else { "" };
    let element = format!(
        "<Color Self=\"{}\" Model=\"{}\" Space=\"{space}\" ColorValue=\"{}\" ColorOverride=\"Normal\" ConvertToHsb=\"{hsb}\"{alias} AlternateSpace=\"NoAlternateColor\" AlternateColorValue=\"\" Name=\"{}\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"{grouped}\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"{slot}\" />",
        escape(&me), if spot { "Spot" } else { "Process" }, values(value), escape(name));
    insert(w, "Color", &me, &element, "Ink")?;
    if spot {
        add_ink(w, name, value, space)?;
    }
    Ok(())
}

/// Every reference to `old` (an attribute value or a property's text) becomes `new`, in all
/// parts; the root colour group's slots are left to the caller.
pub(super) fn rereference(w: &mut Work<'_>, old: &str, new: &str) -> Result<(), EditError> {
    let (eo, en) = (escape(old), escape(new));
    let parts: Vec<String> = w.ed.parts.keys().filter(|p| p.ends_with(".xml")).cloned().collect();
    for part in parts {
        let t = w.text(&part)?.to_owned();
        let mut edits = Vec::new();
        for (open, close) in [("=\"", "\""), (">", "<")] {
            let needle = format!("{open}{eo}{close}");
            let mut from = 0;
            while let Some(k) = t[from..].find(&needle) {
                let at = from + k + open.len();
                let before = &t[..at];
                // not the element's own Self; designmap's slots shift instead (delete_swatch)
                let skip = before.ends_with("Self=\"") || (part == DM && before.ends_with("SwatchItemRef=\""));
                if !skip {
                    edits.push((at..at + eo.len(), en.clone()));
                }
                from = at + eo.len();
            }
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}

pub(super) fn set_swatch(w: &mut Work<'_>, id: &str, value: Option<&[f64]>, space: Option<&str>, spot: Option<bool>, rename: Option<&str>) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("notFound", format!("swatch {id}")))?;
    if ix.local(&t, i) != "Color" {
        return Err(EditError::new("unsupported", format!("{id} is not a colour swatch")));
    }
    if ix.attr(&t, i, "ColorEditable") == Some("false") {
        return Err(EditError::new("locked", format!("{id} cannot be edited")));
    }
    let mut set = Vec::new();
    if let Some(s) = space {
        set.push(("Space", space_name(s)?.to_owned()));
    }
    if let Some(v) = value {
        set.push(("ColorValue", values(v)));
    }
    let was_spot = ix.attr(&t, i, "Model") == Some("Spot");
    if let Some(sp) = spot {
        set.push(("Model", if sp { "Spot" } else { "Process" }.into()));
        if sp && !was_spot {
            set.push(("SpotInkAliasSpotColorReference", "n".into()));
        }
    }
    let name = ix.value(&t, i, "Name").unwrap_or_default();
    if !set.is_empty() {
        set_attrs(w, id, &set)?;
    }
    if spot == Some(true) && !was_spot {
        let (t, ix) = scan(w, GRAPHIC)?;
        let i = ix.by_self(&t, id).unwrap();
        let v: Vec<f64> = ix.attr(&t, i, "ColorValue").unwrap_or_default().split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let sp = ix.attr(&t, i, "Space").unwrap_or("CMYK").to_owned();
        add_ink(w, &name, &v, &sp)?;
    }
    if let Some(new_name) = rename && new_name != name {
        let me = self_of("Color", new_name);
        if new_name.is_empty() || exists(w, &me)? {
            return Err(EditError::new("invalid", format!("a swatch named {new_name} already exists")));
        }
        // the element moves to its sorted place under its new Self
        let (t, ix) = scan(w, GRAPHIC)?;
        let i = ix.by_self(&t, id).unwrap();
        let span = element_span(&ix, &t, i);
        let old = t[ix.nodes[i].start..ix.nodes[i].end]
            .replacen(&format!("Self=\"{}\"", escape(id)), &format!("Self=\"{}\"", escape(&me)), 1)
            .replacen(&format!("Name=\"{}\"", escape(&name)), &format!("Name=\"{}\"", escape(new_name)), 1);
        w.splice(GRAPHIC, vec![(span, String::new())])?;
        insert(w, "Color", &me, &old, "Ink")?;
        rereference(w, id, &me)?;
        // its slot names it by Self too
        let (t, ix) = scan(w, DM)?;
        if let Some((c, ..)) = slots(&ix, &t)?.into_iter().find(|(_, _, r)| r == id) {
            let e = attr_edit(&ix, &t, c, "SwatchItemRef", &me);
            w.splice(DM, vec![e])?;
        }
    }
    Ok(())
}

/// Mixed ink groups (InDesign 2025, artifacts/swatchx-c2-20261023): deleting a group deletes its swatches
/// too; deleting a group's last swatch deletes the group.
pub(super) fn delete_swatch(w: &mut Work<'_>, id: &str, replace: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("notFound", format!("swatch {id}")))?;
    let members_of = |ix: &Index, t: &str, group: &str| -> Vec<String> {
        of_kind(ix, t, "MixedInk").into_iter().filter(|&k| ix.attr(t, k, "BaseColor") == Some(group))
            .filter_map(|k| ix.attr(t, k, "Self").map(str::to_owned)).collect()
    };
    match ix.local(&t, i) {
        "MixedInkGroup" => {
            for member in members_of(&ix, &t, id) { delete_one(w, &member, replace)?; }
            delete_one(w, id, replace)
        }
        "MixedInk" => {
            let group = ix.attr(&t, i, "BaseColor").filter(|b| b.starts_with("MixedInkGroup/")).map(str::to_owned);
            delete_one(w, id, replace)?;
            if let Some(group) = group {
                let (t, ix) = scan(w, GRAPHIC)?;
                if ix.by_self(&t, &group).is_some() && members_of(&ix, &t, &group).is_empty() { delete_one(w, &group, replace)?; }
            }
            Ok(())
        }
        _ => delete_one(w, id, replace),
    }
}

fn delete_one(w: &mut Work<'_>, id: &str, replace: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("notFound", format!("swatch {id}")))?;
    if ix.attr(&t, i, "ColorRemovable") == Some("false") {
        return Err(EditError::new("locked", format!("{id} cannot be deleted")));
    }
    if ix.by_self(&t, replace).is_none() || replace == id {
        return Err(EditError::new("invalid", format!("replacement {replace}")));
    }
    if of_kind(&ix, &t, "Tint").iter().any(|&k| ix.attr(&t, k, "BaseColor") == Some(id)) {
        return Err(EditError::new("unsupported", "the swatch has tints based on it"));
    }
    rereference(w, id, replace)?;
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, id).unwrap();
    w.splice(GRAPHIC, vec![(element_span(&ix, &t, i), String::new())])?;
    // slots: the later swatches each move down one; the last slot goes
    let (t, ix) = scan(w, DM)?;
    let all = slots(&ix, &t)?;
    let Some(p) = all.iter().position(|(_, _, r)| r == id) else { return Ok(()) };
    let mut edits = Vec::new();
    let mut moved = Vec::new();
    for k in p..all.len() - 1 {
        edits.push(attr_edit(&ix, &t, all[k].0, "SwatchItemRef", &all[k + 1].2));
        moved.push((all[k + 1].2.clone(), all[k].1.clone()));
    }
    edits.push((element_span(&ix, &t, all[all.len() - 1].0), String::new()));
    w.splice(DM, edits)?;
    let (t, ix) = scan(w, GRAPHIC)?;
    let edits = moved.iter().filter_map(|(swatch, slot)| ix.by_self(&t, swatch).map(|k| attr_edit(&ix, &t, k, "SwatchColorGroupReference", slot))).collect();
    w.splice(GRAPHIC, edits)
}

pub(super) fn new_tint(w: &mut Work<'_>, base: &str, tint: f64) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let b = ix.by_self(&t, base).ok_or_else(|| EditError::new("notFound", format!("swatch {base}")))?;
    let base_name = ix.value(&t, b, "Name").unwrap_or_default();
    let name = format!("{base_name} {}%", geom::num(tint));
    let me = self_of("Tint", &name);
    if exists(w, &me)? {
        return Err(EditError::new("invalid", format!("{name} already exists")));
    }
    let slot = add_slot(w, &me)?;
    let element = format!(
        "<Tint Self=\"{}\" TintValue=\"{}\" BaseColor=\"{}\" Name=\"{}\" ColorOverride=\"Normal\" ConvertToHsb=\"false\" AlternateSpace=\"NoAlternateColor\" AlternateColorValue=\"\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"true\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"{slot}\" />",
        escape(&me), geom::num(tint), escape(base), escape(&name));
    insert(w, "Tint", &me, &element, "Swatch")?;
    Ok(())
}

pub(super) fn duplicate_swatch(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, id).ok_or_else(|| EditError::new("notFound", format!("swatch {id}")))?;
    if ix.local(&t, i) != "Color" {
        return Err(EditError::new("unsupported", format!("{id} is not a colour swatch")));
    }
    let name = ix.value(&t, i, "Name").unwrap_or_default();
    let copy = (1..).map(|n| if n == 1 { format!("{name} copy") } else { format!("{name} copy {n}") })
        .find(|n| ix.by_self(&t, &self_of("Color", n)).is_none()).unwrap();
    let me = self_of("Color", &copy);
    let slot = add_slot(w, &me)?;
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, id).unwrap();
    let element = t[ix.nodes[i].start..ix.nodes[i].end]
        .replacen(&format!("Self=\"{}\"", escape(id)), &format!("Self=\"{}\"", escape(&me)), 1)
        .replacen(&format!("Name=\"{}\"", escape(&name)), &format!("Name=\"{}\"", escape(&copy)), 1);
    let element = match element.find("SwatchColorGroupReference=\"") {
        Some(k) => {
            let v = k + "SwatchColorGroupReference=\"".len();
            let end = v + element[v..].find('"').unwrap_or(0);
            format!("{}{slot}{}", &element[..v], &element[end..])
        }
        None => element,
    };
    insert(w, "Color", &me, &element, "Ink")?;
    Ok(())
}
