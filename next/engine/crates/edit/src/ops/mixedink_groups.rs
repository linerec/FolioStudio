use super::*;
use super::color_commands::{NewMixedInk, NewMixedInkGroup, RenameMixedInkGroup};
use super::swatches::{add_slot, insert, of_kind, rereference, scan, self_of, slots};

const GRAPHIC: &str = "Resources/Graphic.xml";

/// InDesign 2025 (measured, artifacts/swatchx-c2-20261023): one ink gives `initial + increment * k` for
/// k = 0..=repeat, rounded to 7 decimals; the first value of 100 or more is stored as 100 and ends the
/// ink's steps (increment 0 repeats the same value, it is not dropped). A group has 2..=31 inks, at most 27
/// of them spot inks. An empty name becomes "Group N". The last ink varies fastest.
pub const MAX_INKS: usize = 31;
pub const MAX_SPOT_INKS: usize = 27;

fn steps(start: f64, repeat: usize, increment: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for k in 0..=repeat {
        let v = ((start + increment * k as f64) * 1e7).round() / 1e7;
        if v >= 100.0 {
            out.push(100.0);
            break;
        }
        out.push(v);
    }
    out
}

fn expand(p: &NewMixedInkGroup, name: &str) -> Result<Vec<NewMixedInk>, EditError> {
    let n = p.inks.len();
    let spots = p.inks.iter().filter(|i| !i.starts_with("Ink/$ID/Process ")).count();
    if n < 2 || spots == 0 || p.initial.len() != n || p.repeat.len() != n || p.increment.len() != n
        || p.inks.iter().collect::<std::collections::BTreeSet<_>>().len() != n {
        return Err(EditError::new("invalid", "a group needs two or more distinct inks, one of them a spot ink, and one set of values per ink"));
    }
    if n > MAX_INKS || spots > MAX_SPOT_INKS {
        return Err(EditError::new("limit", "a mixed ink may have at most 27 spot inks and 31 inks in all"));
    }
    let mut rows = vec![Vec::new()];
    for i in 0..n {
        let (start, repeat, increment) = (p.initial[i], p.repeat[i], p.increment[i]);
        if !start.is_finite() || !repeat.is_finite() || !increment.is_finite()
            || !(0.0..=100.0).contains(&start) || !(0.0..=100.0).contains(&increment)
            || repeat < 0.0 || repeat.fract() != 0.0 || repeat > 1000.0 {
            return Err(EditError::new("invalid", "invalid mixed ink steps"));
        }
        let steps = steps(start, repeat as usize, increment);
        if rows.len().saturating_mul(steps.len()) > 1000 {
            return Err(EditError::new("limit", "a group may contain at most 1000 swatches"));
        }
        let mut next = Vec::new();
        for row in rows {
            for value in &steps {
                let mut values = row.clone();
                values.push(*value);
                next.push(values);
            }
        }
        rows = next;
    }
    Ok(rows.into_iter().enumerate().map(|(i, percentages)| NewMixedInk {
        name: format!("{name} Swatch {}", i + 1), inks: p.inks.clone(), percentages,
    }).collect())
}

/// The swatches a group would make (names as if no swatch had the name yet).
pub fn preview(p: &NewMixedInkGroup) -> Result<Vec<NewMixedInk>, EditError> {
    expand(p, if p.name.trim().is_empty() { "Group 1" } else { &p.name })
}

const SWATCH_KINDS: [&str; 5] = ["Color", "Tint", "Gradient", "MixedInk", "Swatch"];

fn taken(ix: &Index, text: &str, name: &str) -> bool {
    SWATCH_KINDS.iter().any(|kind| ix.by_self(text, &escape(&self_of(kind, name))).is_some())
}

pub(super) fn create(w: &mut Work<'_>, p: &NewMixedInkGroup) -> Result<(), EditError> {
    let (text, ix) = scan(w, GRAPHIC)?;
    // InDesign: a group may share its name with a colour or an ink, only a second group of that name would
    // share the IDML Self (Folio keeps group names unique); an empty name is "Group N" for the first free N.
    let name = if p.name.trim().is_empty() {
        (1..).map(|n| format!("Group {n}")).find(|n| ix.by_self(&text, &self_of("MixedInkGroup", n)).is_none()).unwrap()
    } else {
        p.name.clone()
    };
    let mut rows = expand(p, &name)?;
    let group = self_of("MixedInkGroup", &name);
    if ix.by_self(&text, &group).is_some() {
        return Err(EditError::new("invalid", "a mixed ink group with this name already exists"));
    }
    // a generated name another swatch already has gets " copy" (measured: "G Swatch 1 copy")
    for row in &mut rows {
        while taken(&ix, &text, &row.name) { row.name.push_str(" copy"); }
    }
    let slot = add_slot(w, &group)?;
    for row in &rows { super::swatchmenu::new_mixed_ink(w, row)?; }
    let (text, ix) = scan(w, GRAPHIC)?;
    let first = ix.by_self(&text, &self_of("MixedInk", &rows[0].name)).unwrap();
    let fields = ["Model", "InkList", "InkNameList", "MixedInkSpotColorNameList", "MixedInkSpotColorList", "ColorEditable", "ColorRemovable", "Visible", "SwatchCreatorID"];
    let attributes = fields.iter().map(|key| format!(" {key}=\"{}\"", escape(&ix.value(&text, first, key).unwrap_or_default()))).collect::<String>();
    let element = format!("<MixedInkGroup Self=\"{}\"{attributes} Name=\"{}\" SwatchColorGroupReference=\"{slot}\" />", escape(&group), escape(&name));
    let edits = rows.iter().map(|row| {
        let id = self_of("MixedInk", &row.name);
        let node = ix.by_self(&text, &id).unwrap();
        w.created.push(id);
        attr_edit(&ix, &text, node, "BaseColor", &group)
    }).collect();
    w.splice(GRAPHIC, edits)?;
    insert(w, "MixedInkGroup", &group, &element, "MixedInk")?;
    w.created.insert(0, group);
    Ok(())
}

const DESIGNMAP: &str = "designmap.xml";

/// One swatch element gets a new name: its Self and Name change, it moves to its sorted place in
/// Graphic.xml, every reference follows and its panel slot names the new Self (as swatch-rename does
/// for a colour). Returns the new id.
fn rename_one(w: &mut Work<'_>, kind: &str, old_id: &str, name: &str) -> Result<String, EditError> {
    let new_id = self_of(kind, name);
    set_attrs(w, &escape(old_id), &[("Self", new_id.clone()), ("Name", name.to_owned())])?;
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, &escape(&new_id)).ok_or_else(|| EditError::new("notFound", format!("swatch {new_id}")))?;
    let element = t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
    let fallback = if kind == "MixedInkGroup" { "MixedInk" } else if of_kind(&ix, &t, "PastedSmoothShade").is_empty() { "Swatch" } else { "PastedSmoothShade" };
    w.splice(GRAPHIC, vec![(element_span(&ix, &t, i), String::new())])?;
    insert(w, kind, &new_id, &element, fallback)?;
    rereference(w, old_id, &new_id)?;
    let (t, ix) = scan(w, DESIGNMAP)?;
    if let Some((slot, ..)) = slots(&ix, &t)?.into_iter().find(|(_, _, r)| r == old_id) {
        let edit = attr_edit(&ix, &t, slot, "SwatchItemRef", &new_id);
        w.splice(DESIGNMAP, vec![edit])?;
    }
    Ok(new_id)
}

/// Renames a mixed ink group (InDesign 2025, artifacts/swatchx-c3-20261023/measurement/mixedink-rename.json):
/// - the group's name and IDML Self change, and so do its swatches': in each member's name the first
///   occurrence of the old group name (case-sensitive, anywhere in the name) becomes the new name, so
///   "G Swatch 1" and "G Swatch 1 copy" follow, "Other" stays; references (fills, strokes, styles) follow;
/// - a renamed swatch whose new name another swatch already has gets " copy" (as when a group is made);
/// - an empty name, or the old name in other letter case, is ignored without an error;
/// - InDesign lets two groups share a name and then gives odd results (overlapping swatches); Folio
///   refuses a name another group has (their IDML Self would collide).
///   The new group id is reported as created.
pub(super) fn rename(w: &mut Work<'_>, p: &RenameMixedInkGroup) -> Result<(), EditError> {
    let id = if p.id.starts_with("MixedInkGroup/") { p.id.clone() } else { self_of("MixedInkGroup", &p.id) };
    let (text, ix) = scan(w, GRAPHIC)?;
    let group = ix.by_self(&text, &escape(&id)).filter(|&i| ix.local(&text, i) == "MixedInkGroup")
        .ok_or_else(|| EditError::new("notFound", format!("mixed ink group {}", p.id)))?;
    let old = ix.value(&text, group, "Name").unwrap_or_default();
    if p.name.is_empty() || p.name.to_lowercase() == old.to_lowercase() {
        return Ok(());
    }
    let new_id = self_of("MixedInkGroup", &p.name);
    if ix.by_self(&text, &escape(&new_id)).is_some() {
        return Err(EditError::new("invalid", "a mixed ink group with this name already exists"));
    }
    let members: Vec<String> = of_kind(&ix, &text, "MixedInk").into_iter()
        .filter(|&k| ix.value(&text, k, "BaseColor").as_deref() == Some(id.as_str()))
        .filter_map(|k| ix.value(&text, k, "Self")).collect();
    rename_one(w, "MixedInkGroup", &id, &p.name)?;
    for member in members {
        let (text, ix) = scan(w, GRAPHIC)?;
        let Some(k) = ix.by_self(&text, &escape(&member)) else { continue };
        let name = ix.value(&text, k, "Name").unwrap_or_default();
        let mut target = name.replacen(&old, &p.name, 1);
        if target == name { continue; }
        while taken(&ix, &text, &target) { target.push_str(" copy"); }
        rename_one(w, "MixedInk", &member, &target)?;
    }
    w.created.push(new_id);
    Ok(())
}
