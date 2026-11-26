//! The Swatches panel menu (oracle basic-swatchmenu: swatch-sort-*, swatch-group-new):
//! - Sort ▸ All Swatches By Name / By Color Values reorders the root colour group's slots after
//!   the four fixed swatches ([None], [Registration], [Paper], [Black]); the slots keep their
//!   Self, their SwatchItemRef changes, and each moved swatch's SwatchColorGroupReference follows;
//! - by name is a natural, case-blind order (C=15 before C=100; a tint after its base, a
//!   gradient among the others); by colour values: CMYK swatches by C, M, Y, K, then RGB by
//!   R, G, B, then Lab, mixed inks, gradients; a tint sits by its base, below the base itself;
//!   equal values keep their order;
//! - a new colour group is a designmap `ColorGroup` (not the root) after the existing ones, its
//!   `ColorGroupSwatch` slots naming the swatches, which leave their old group: the later slots
//!   of that group move down one and its last slot goes (as in Delete Swatch);
//! - Merge Swatches puts the first swatch in place of the others (Delete Swatch's replacement);
//! - Add Unnamed Colors gives unnamed colours (`Name="$ID/"`) that objects use a swatch named
//!   with its value, as New Color Swatch's "Name with Color Value" does (not measured:
//!   scripting cannot make an unnamed colour; the rule follows the swatch rename).
use super::color_commands::{AddUnnamedColors, DefaultPaint, MergeSwatches, NewColorGroup, NewMixedInk, NewUnnamedColor, RenameColorGroup, SetDefaultFillStroke, SetPaintColor, SortSwatches, UngroupColorGroup};
use super::swatches::{add_slot, delete_swatch, insert, of_kind, rereference, scan, self_of, slots};
use super::*;
use crate::xml::unescape;
use std::cmp::Ordering;
use std::collections::BTreeMap;

const GRAPHIC: &str = "Resources/Graphic.xml";
const DM: &str = "designmap.xml";
const FIXED: [&str; 4] = ["Swatch/None", "Color/Registration", "Color/Paper", "Color/Black"];

// ---- sorting ---------------------------------------------------------------------------

struct Key {
    rank: u8,
    values: Vec<f64>,
    tint: f64,
    name: String,
}

fn key(ix: &Index, t: &str, id: &str, depth: u8) -> Key {
    let Some(i) = ix.by_self(t, id) else { return Key { rank: 99, values: vec![], tint: 100.0, name: String::new() } };
    let name = ix.value(t, i, "Name").unwrap_or_default();
    match ix.local(t, i) {
        "Color" => {
            let rank = match ix.attr(t, i, "Space") { Some("CMYK") => 0, Some("RGB") => 1, Some("LAB") => 2, _ => 3 };
            let values = ix.attr(t, i, "ColorValue").unwrap_or_default().split_whitespace().filter_map(|v| v.parse().ok()).collect();
            Key { rank, values, tint: 100.0, name }
        }
        "Tint" if depth == 0 => {
            let base = ix.value(t, i, "BaseColor").unwrap_or_default();
            let mut k = key(ix, t, &base, 1);
            k.tint = ix.attr(t, i, "TintValue").and_then(|v| v.parse().ok()).unwrap_or(100.0);
            k.name = name;
            k
        }
        "MixedInk" => Key { rank: 10, values: vec![], tint: 100.0, name },
        "Gradient" => Key { rank: 20, values: vec![], tint: 100.0, name },
        _ => Key { rank: 30, values: vec![], tint: 100.0, name },
    }
}

fn by_value(a: &Key, b: &Key) -> Ordering {
    a.rank.cmp(&b.rank)
        .then_with(|| a.values.iter().zip(&b.values).map(|(x, y)| x.partial_cmp(y).unwrap_or(Ordering::Equal)).find(|o| o.is_ne()).unwrap_or(Ordering::Equal))
        .then_with(|| a.tint.partial_cmp(&b.tint).unwrap_or(Ordering::Equal))
}

/// "C=15" before "C=100": digit runs compare as numbers, letters without case.
fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let run = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut s = String::new();
                    while let Some(c) = it.next_if(|c| c.is_ascii_digit()) { s.push(c); }
                    s
                };
                let (x, y) = (run(&mut a), run(&mut b));
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let o = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if o.is_ne() { return o; }
            }
            (Some(x), Some(y)) => {
                let (x, y) = (x.to_lowercase().next().unwrap_or(x), y.to_lowercase().next().unwrap_or(y));
                if x != y { return x.cmp(&y); }
                a.next();
                b.next();
            }
        }
    }
}

pub(super) fn sort(w: &mut Work<'_>, p: &SortSwatches) -> Result<(), EditError> {
    let name = match p.by.as_str() {
        "name" => true,
        "value" => false,
        other => return Err(EditError::new("invalid", format!("sort by {other}"))),
    };
    let (dm, dx) = scan(w, DM)?;
    let all = slots(&dx, &dm)?;
    let (g, gx) = scan(w, GRAPHIC)?;
    let lead = all.iter().take_while(|(_, _, r)| FIXED.contains(&r.as_str())).count();
    let pos: Vec<usize> = (lead..all.len()).filter(|&i| p.only.as_ref().is_none_or(|o| o.contains(&all[i].2))).collect();
    let mut rows: Vec<(String, Key)> = pos.iter().map(|&i| (all[i].2.clone(), key(&gx, &g, &all[i].2, 0))).collect();
    // equal keys keep their order (sort_by is stable)
    rows.sort_by(|a, b| if name { natural(&a.1.name, &b.1.name) } else { by_value(&a.1, &b.1) });
    let (mut slot_edits, mut swatch_edits) = (Vec::new(), Vec::new());
    for (&i, (new, _)) in pos.iter().zip(&rows) {
        if *new == all[i].2 { continue; }
        slot_edits.push(attr_edit(&dx, &dm, all[i].0, "SwatchItemRef", new));
        if let Some(s) = gx.by_self(&g, new) && gx.attr(&g, s, "SwatchColorGroupReference").is_some() {
            swatch_edits.push(attr_edit(&gx, &g, s, "SwatchColorGroupReference", &all[i].1));
        }
    }
    swatch_edits.sort_by_key(|e| e.0.start);
    w.splice(DM, slot_edits)?;
    w.splice(GRAPHIC, swatch_edits)
}

// ---- colour groups ---------------------------------------------------------------------

struct Group {
    open: String,
    id: String,
    root: bool,
    /// The slots' Self as they were (new slots take `stem` and their index).
    selfs: Vec<String>,
    refs: Vec<String>,
    stem: String,
}

fn stem_of(selfs: &[String], name: &str) -> String {
    match selfs.first() {
        Some(s) => s.trim_end_matches(|c: char| c.is_ascii_hexdigit()).to_owned(),
        None => {
            let h = name.bytes().fold(0xcbf29ce4u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x01000193));
            format!("u{h:x}ColorGroupSwatch")
        }
    }
}

fn read_groups(t: &str, ix: &Index) -> Result<(std::ops::Range<usize>, Vec<Group>), EditError> {
    let nodes: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "ColorGroup").collect();
    let (Some(&first), Some(&last)) = (nodes.first(), nodes.last()) else {
        return Err(EditError::new("unsupported", "designmap has no colour groups"));
    };
    let groups = nodes.iter().map(|&i| {
        let kids: Vec<usize> = ix.children(i).iter().copied().filter(|&c| ix.local(t, c) == "ColorGroupSwatch").collect();
        let selfs: Vec<String> = kids.iter().map(|&c| ix.attr(t, c, "Self").unwrap_or_default().to_owned()).collect();
        let name = ix.value(t, i, "Name").unwrap_or_default();
        Group {
            open: t[ix.nodes[i].start..ix.nodes[i].open_end].to_owned(),
            id: ix.value(t, i, "Self").unwrap_or_default(),
            root: ix.attr(t, i, "IsRootColorGroup") == Some("true"),
            stem: stem_of(&selfs, &name),
            refs: kids.iter().map(|&c| ix.value(t, c, "SwatchItemRef").unwrap_or_default()).collect(),
            selfs,
        }
    }).collect();
    Ok((ix.nodes[first].start..ix.nodes[last].end, groups))
}

fn slot_self(g: &Group, i: usize) -> String {
    g.selfs.get(i).cloned().unwrap_or_else(|| format!("{}{i:x}", g.stem))
}

/// The designmap's colour groups written anew, and each swatch pointing at its slot.
fn write_groups(w: &mut Work<'_>, range: std::ops::Range<usize>, groups: &[Group]) -> Result<(), EditError> {
    let (dm, _) = scan(w, DM)?;
    let ind = indent_before(&dm, range.start).to_owned();
    let mut xml = String::new();
    for (n, g) in groups.iter().enumerate() {
        if n > 0 { xml += &format!("\n{ind}"); }
        xml += &g.open;
        for (i, r) in g.refs.iter().enumerate() {
            xml += &format!("\n{ind}\t<ColorGroupSwatch Self=\"{}\" SwatchItemRef=\"{}\" />", escape(&slot_self(g, i)), escape(r));
        }
        xml += &format!("\n{ind}</ColorGroup>");
    }
    w.splice(DM, vec![(range, xml)])?;
    let (t, ix) = scan(w, GRAPHIC)?;
    let mut edits = Vec::new();
    for g in groups {
        for (i, r) in g.refs.iter().enumerate() {
            if let Some(s) = ix.by_self(&t, r) {
                let want = slot_self(g, i);
                if ix.value(&t, s, "SwatchColorGroupReference").as_deref() != Some(&want) && ix.attr(&t, s, "SwatchColorGroupReference").is_some() {
                    edits.push(attr_edit(&ix, &t, s, "SwatchColorGroupReference", &want));
                }
            }
        }
    }
    edits.sort_by_key(|e| e.0.start);
    w.splice(GRAPHIC, edits)
}

fn load(w: &Work<'_>) -> Result<(std::ops::Range<usize>, Vec<Group>), EditError> {
    let (dm, dx) = scan(w, DM)?;
    read_groups(&dm, &dx)
}

pub(super) fn new_group(w: &mut Work<'_>, p: &NewColorGroup) -> Result<(), EditError> {
    if p.name.is_empty() || p.swatches.is_empty() {
        return Err(EditError::new("invalid", "a colour group needs a name and swatches"));
    }
    let (range, mut groups) = load(w)?;
    let me = self_of("ColorGroup", &p.name);
    if groups.iter().any(|g| g.id == me) {
        return Err(EditError::new("invalid", format!("a colour group named {} already exists", p.name)));
    }
    for s in &p.swatches {
        if FIXED.contains(&s.as_str()) {
            return Err(EditError::new("locked", format!("{s} cannot be grouped")));
        }
        let holder = groups.iter_mut().find(|g| g.refs.contains(s)).ok_or_else(|| EditError::new("notFound", format!("swatch {s}")))?;
        let at = holder.refs.iter().position(|r| r == s).unwrap();
        // later slots move down one and the last slot goes
        holder.refs.remove(at);
        holder.selfs.truncate(holder.refs.len());
    }
    groups.retain(|g| g.root || !g.refs.is_empty());
    let open = format!("<ColorGroup Self=\"{}\" Name=\"{}\" IsRootColorGroup=\"false\">", escape(&me), escape(&p.name));
    groups.push(Group { open, id: me, root: false, selfs: vec![], refs: p.swatches.clone(), stem: stem_of(&[], &p.name) });
    write_groups(w, range, &groups)
}

pub(super) fn rename_group(w: &mut Work<'_>, p: &RenameColorGroup) -> Result<(), EditError> {
    let (range, mut groups) = load(w)?;
    let me = self_of("ColorGroup", &p.name);
    if p.name.is_empty() || groups.iter().any(|g| g.id == me && g.id != p.id) {
        return Err(EditError::new("invalid", format!("a colour group named {} already exists", p.name)));
    }
    let g = groups.iter_mut().find(|g| g.id == p.id && !g.root).ok_or_else(|| EditError::new("notFound", format!("colour group {}", p.id)))?;
    g.open = format!("<ColorGroup Self=\"{}\" Name=\"{}\" IsRootColorGroup=\"false\">", escape(&me), escape(&p.name));
    g.id = me;
    write_groups(w, range, &groups)
}

pub(super) fn ungroup(w: &mut Work<'_>, p: &UngroupColorGroup) -> Result<(), EditError> {
    let (range, mut groups) = load(w)?;
    let at = groups.iter().position(|g| g.id == p.id && !g.root).ok_or_else(|| EditError::new("notFound", format!("colour group {}", p.id)))?;
    let moved = groups.remove(at).refs;
    let root = groups.iter_mut().find(|g| g.root).ok_or_else(|| EditError::new("unsupported", "designmap has no root colour group"))?;
    root.refs.extend(moved);
    write_groups(w, range, &groups)
}

// ---- merging ---------------------------------------------------------------------------

pub(super) fn merge(w: &mut Work<'_>, p: &MergeSwatches) -> Result<(), EditError> {
    if !p.ids.contains(&p.keep) || p.ids.len() < 2 {
        return Err(EditError::new("invalid", "merge needs the swatch to keep and at least one more"));
    }
    for id in p.ids.iter().filter(|id| **id != p.keep) {
        delete_swatch(w, id, &p.keep)?;
    }
    Ok(())
}

// ---- unnamed colours -------------------------------------------------------------------

/// "C=10 M=20 Y=30 K=40", "R=255 G=0 B=0", "L=50 a=0 b=0": the name New Color Swatch gives with its value.
fn value_name(space: &str, v: &[f64]) -> Option<String> {
    let keys: &[&str] = match space { "CMYK" => &["C", "M", "Y", "K"], "RGB" => &["R", "G", "B"], "LAB" => &["L", "a", "b"], _ => return None };
    (v.len() == keys.len()).then(|| keys.iter().zip(v).map(|(k, x)| format!("{k}={}", geom::num((x * 100.0).round() / 100.0))).collect::<Vec<_>>().join(" "))
}

pub(super) fn add_unnamed(w: &mut Work<'_>, p: &AddUnnamedColors) -> Result<(), EditError> {
    let used = referenced(&w.ed.parts);
    let (t, ix) = scan(w, GRAPHIC)?;
    let unnamed: Vec<String> = of_kind(&ix, &t, "Color").into_iter()
        .filter(|&i| ix.attr(&t, i, "Name") == Some("$ID/"))
        .filter_map(|i| ix.value(&t, i, "Self"))
        .filter(|id| used.contains_key(id.as_str()) && p.only.as_ref().is_none_or(|o| o.contains(id))).collect();
    for old in unnamed {
        let (t, ix) = scan(w, GRAPHIC)?;
        let Some(i) = ix.by_self(&t, &old) else { continue };
        let values: Vec<f64> = ix.attr(&t, i, "ColorValue").unwrap_or_default().split_whitespace().filter_map(|v| v.parse().ok()).collect();
        let Some(name) = value_name(ix.attr(&t, i, "Space").unwrap_or_default(), &values) else { continue };
        let me = self_of("Color", &name);
        if ix.by_self(&t, &me).is_some() { continue; }
        let slot = add_slot(w, &me)?;
        let (t, ix) = scan(w, GRAPHIC)?;
        let i = ix.by_self(&t, &old).unwrap();
        let mut element = t[ix.nodes[i].start..ix.nodes[i].end]
            .replacen(&format!("Self=\"{}\"", escape(&old)), &format!("Self=\"{}\"", escape(&me)), 1)
            .replacen("Name=\"$ID/\"", &format!("Name=\"{}\"", escape(&name)), 1)
            .replacen("Visible=\"false\"", "Visible=\"true\"", 1);
        element = match element.find("SwatchColorGroupReference=\"") {
            Some(k) => {
                let v = k + "SwatchColorGroupReference=\"".len();
                let end = v + element[v..].find('"').unwrap_or(0);
                format!("{}{slot}{}", &element[..v], &element[end..])
            }
            None => element,
        };
        w.splice(GRAPHIC, vec![(element_span(&ix, &t, i), String::new())])?;
        insert(w, "Color", &me, &element, "Ink")?;
        rereference(w, &old, &me)?;
    }
    Ok(())
}

/// The unnamed colour of a value: the space, the clamped and rounded values, their text and the colour's id
/// (`Color/u<hash>`, one per value). The Color panel's sliders and the Color Picker give objects these colours.
fn unnamed_of(space: &str, value: &[f64]) -> Result<(&'static str, String, String), EditError> {
    let (space, ranges): (&'static str, &[(f64, f64)]) = match space {
        "CMYK" | "cmyk" => ("CMYK", &[(0.0, 100.0); 4]),
        "RGB" | "rgb" => ("RGB", &[(0.0, 255.0); 3]),
        "LAB" | "lab" => ("LAB", &[(0.0, 100.0), (-128.0, 127.0), (-128.0, 127.0)]),
        other => return Err(EditError::new("invalid", format!("colour space {other}"))),
    };
    if value.len() != ranges.len() || value.iter().any(|v| !v.is_finite()) {
        return Err(EditError::new("invalid", "colour values"));
    }
    let value: Vec<f64> = value.iter().zip(ranges).map(|(v, (lo, hi))| ((v.clamp(*lo, *hi)) * 1000.0).round() / 1000.0).collect();
    let text = value.iter().map(|x| geom::num(*x)).collect::<Vec<_>>().join(" ");
    let h = format!("{space} {text}").bytes().fold(0x811c9dc5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x01000193));
    Ok((space, text, format!("Color/u{h:x}")))
}

/// The id the unnamed colour of this value has (or would get), for a caller that applies it in the same edit.
pub fn unnamed_color_id(space: &str, value: &[f64]) -> Result<String, EditError> {
    unnamed_of(space, value).map(|(_, _, id)| id)
}

/// Makes the unnamed colour (`Name="$ID/"`, hidden, as InDesign keeps the colours its sliders make) once per value.
fn ensure_unnamed(w: &mut Work<'_>, space: &str, value: &[f64]) -> Result<String, EditError> {
    let (space, text, me) = unnamed_of(space, value)?;
    let (t, ix) = scan(w, GRAPHIC)?;
    if ix.by_self(&t, &me).is_none() {
        let element = format!(
            "<Color Self=\"{me}\" Model=\"Process\" Space=\"{space}\" ColorValue=\"{text}\" ColorOverride=\"Normal\" ConvertToHsb=\"false\" AlternateSpace=\"NoAlternateColor\" AlternateColorValue=\"\" Name=\"$ID/\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"false\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"n\" />");
        insert(w, "Color", &me, &element, "Ink")?;
    }
    Ok(me)
}

/// The Color panel's sliders: the colour is an unnamed one, made once per value, and the objects' fill (or stroke) takes it.
pub(super) fn set_color(w: &mut Work<'_>, p: &SetPaintColor) -> Result<(), EditError> {
    let me = ensure_unnamed(w, &p.space, &p.value)?;
    let stroke = p.stroke == Some(true);
    for id in expand(w, &p.ids) {
        set_paint(w, &id, (!stroke).then_some(me.as_str()), None, stroke.then_some(me.as_str()), None, None)?;
    }
    Ok(())
}

/// The Color Picker's OK on text (a caret, characters or the story of a text frame): the colour exists, the text edit then takes it by id.
pub(super) fn new_unnamed(w: &mut Work<'_>, p: &NewUnnamedColor) -> Result<(), EditError> {
    let me = ensure_unnamed(w, &p.space, &p.value)?;
    w.created.push(me);
    Ok(())
}

const PREFS: &str = "Resources/Preferences.xml";

/// The colour id a default paint names: a swatch of the document, or the unnamed colour of its values (made when it is new).
fn default_paint_id(w: &mut Work<'_>, d: &DefaultPaint) -> Result<String, EditError> {
    match (&d.swatch, &d.space, &d.value) {
        (Some(id), None, None) => {
            let (t, ix) = scan(w, GRAPHIC)?;
            if ix.by_self(&t, id).is_none() {
                return Err(EditError::new("notFound", format!("swatch {id}")));
            }
            Ok(id.clone())
        }
        (None, Some(space), Some(value)) => ensure_unnamed(w, space, value),
        _ => Err(EditError::new("invalid", "a default paint is a swatch, or a colour space with its values")),
    }
}

/// The Color Picker (or Default Fill and Stroke) with nothing selected (colorpicker-c2, measured in InDesign 2025): the objects drawn from now on take the
/// colour; the ones that exist do not change. The document keeps it as `PageItemDefault/@FillColor` / `@StrokeColor`, which `create` already reads.
pub(super) fn set_default_fill_stroke(w: &mut Work<'_>, p: &SetDefaultFillStroke) -> Result<(), EditError> {
    if p.fill.is_none() && p.stroke.is_none() {
        return Err(EditError::new("invalid", "set a default fill, a default stroke, or both"));
    }
    let fill = p.fill.as_ref().map(|d| default_paint_id(w, d)).transpose()?;
    let stroke = p.stroke.as_ref().map(|d| default_paint_id(w, d)).transpose()?;
    let prefs = w.text(PREFS)?.to_owned();
    let px = Index::scan(&prefs).map_err(|e| EditError::new("xml", e))?;
    let item = (0..px.nodes.len()).find(|&i| px.local(&prefs, i) == "PageItemDefault").ok_or_else(|| EditError::new("xml", "no PageItemDefault"))?;
    let mut edits = Vec::new();
    for (attr, id) in [("FillColor", fill), ("StrokeColor", stroke)] {
        if let Some(id) = id
            && px.value(&prefs, item, attr).as_deref() != Some(id.as_str())
        {
            edits.push(attr_edit(&px, &prefs, item, attr, &id));
        }
    }
    if edits.is_empty() { Ok(()) } else { w.splice(PREFS, edits) }
}

/// The document's default fill and stroke of new objects (`PageItemDefault`), with their tints.
#[derive(Clone, Debug, PartialEq)]
pub struct DefaultFillStroke {
    pub fill: String,
    pub fill_tint: f64,
    pub stroke: String,
    pub stroke_tint: f64,
}

impl crate::Editor {
    /// What the next object drawn gets for fill and stroke; `None` when the document has no `PageItemDefault`.
    pub fn default_fill_stroke(&self) -> Option<DefaultFillStroke> {
        let prefs = self.parts.get(PREFS)?;
        let px = Index::scan(prefs).ok()?;
        let item = (0..px.nodes.len()).find(|&i| px.local(prefs, i) == "PageItemDefault")?;
        let tint = |name: &str| px.value(prefs, item, name).and_then(|v| v.parse::<f64>().ok()).filter(|t| (0.0..=100.0).contains(t)).unwrap_or(100.0);
        Some(DefaultFillStroke {
            fill: px.value(prefs, item, "FillColor").unwrap_or_else(|| "Swatch/None".into()),
            fill_tint: tint("FillTint"),
            stroke: px.value(prefs, item, "StrokeColor").unwrap_or_else(|| "Swatch/None".into()),
            stroke_tint: tint("StrokeTint"),
        })
    }
}

// ---- mixed inks ------------------------------------------------------------------------

/// `Oracle Spot` as the lists of an IDML attribute write it (a space separates the entries).
fn listed(s: &str) -> String {
    s.replace(' ', "%20")
}

/// Swatches panel ▸ New Mixed Ink Swatch (oracle basic-swatchmenu swatch-mixed-*): a `MixedInk` after the
/// last Ink, named by its Self `MixedInk/<name>`, listing the inks (`InkList`, their `Ink` Self
/// with %20 for spaces), their percentages, the spot inks among them and those inks' colours,
/// with a new last slot in the panel. It needs one spot ink.
pub(super) fn new_mixed_ink(w: &mut Work<'_>, p: &NewMixedInk) -> Result<(), EditError> {
    if p.name.is_empty() || p.inks.is_empty() || p.inks.len() != p.percentages.len() || p.percentages.iter().any(|v| !v.is_finite() || !(0.0..=100.0).contains(v)) {
        return Err(EditError::new("invalid", "a mixed ink needs a name, inks and a percentage for each"));
    }
    let (t, ix) = scan(w, GRAPHIC)?;
    let me = self_of("MixedInk", &p.name);
    if ix.by_self(&t, &me).is_some() || ix.by_self(&t, &self_of("Color", &p.name)).is_some() {
        return Err(EditError::new("invalid", format!("a swatch named {} already exists", p.name)));
    }
    let (mut names, mut spots) = (Vec::new(), Vec::new());
    for id in &p.inks {
        let i = ix.by_self(&t, id).filter(|&i| ix.local(&t, i) == "Ink").ok_or_else(|| EditError::new("notFound", format!("ink {id}")))?;
        let name = ix.value(&t, i, "Name").unwrap_or_default();
        names.push(listed(&name));
        if !id.starts_with("Ink/$ID/Process") {
            spots.push(name);
        }
    }
    if spots.is_empty() {
        return Err(EditError::new("invalid", "a mixed ink needs a spot ink"));
    }
    let slot = add_slot(w, &me)?;
    let element = format!(
        "<MixedInk Self=\"{}\" Model=\"Mixedinkmodel\" Space=\"MixedInk\" InkList=\"{}\" InkPercentages=\"{}\" BaseColor=\"n\" InkNameList=\"{}\" MixedInkSpotColorNameList=\"{}\" MixedInkSpotColorList=\"{}\" Name=\"{}\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"true\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"{slot}\" />",
        escape(&me),
        escape(&p.inks.iter().map(|i| listed(i)).collect::<Vec<_>>().join(" ")),
        p.percentages.iter().map(|v| geom::num(*v)).collect::<Vec<_>>().join(" "),
        escape(&names.join(" ")),
        escape(&spots.iter().map(|n| listed(n)).collect::<Vec<_>>().join(" ")),
        escape(&spots.iter().map(|n| format!("Color/{}", listed(n))).collect::<Vec<_>>().join(" ")),
        escape(&p.name));
    if of_kind(&ix, &t, "PastedSmoothShade").is_empty() {
        insert(w, "MixedInk", &me, &element, "Swatch")
    } else {
        insert(w, "MixedInk", &me, &element, "PastedSmoothShade")
    }
}

// ---- usage -----------------------------------------------------------------------------

/// The page items that name each swatch (`Spreads/`), and how many other places do (styles,
/// stories, preferences); a swatch only a gradient or tint names counts as unused, as
/// InDesign's Select All Unused does (probe-unused). Graphic.xml and the designmap are the
/// swatches' own definitions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SwatchUse {
    pub items: Vec<String>,
    pub other: usize,
}

const ITEMS: [&str; 8] = ["Rectangle", "Oval", "Polygon", "GraphicLine", "TextFrame", "Group", "Button", "MultiStateObject"];

pub fn referenced(parts: &BTreeMap<String, String>) -> BTreeMap<String, SwatchUse> {
    let mut out: BTreeMap<String, SwatchUse> = BTreeMap::new();
    for (name, t) in parts {
        if !name.ends_with(".xml") || name == GRAPHIC || name == DM { continue; }
        let spread = name.starts_with("Spreads/") || name.starts_with("MasterSpreads/");
        let ix = if spread { Index::scan(t).ok() } else { None };
        for prefix in ["Color/", "Tint/", "Gradient/", "Swatch/", "MixedInk/"] {
            for (at, _) in t.match_indices(prefix) {
                if at == 0 || !matches!(t.as_bytes()[at - 1], b'"' | b'>') { continue; }
                let end = t[at..].find(['"', '<']).map_or(t.len(), |e| at + e);
                let id = unescape(&t[at..end]);
                let entry = out.entry(id).or_default();
                let owner = ix.as_ref().and_then(|ix| {
                    let n = ix.nodes.partition_point(|n| n.start <= at).checked_sub(1)?;
                    std::iter::once(n).chain(ix.ancestors(n)).find(|&a| ITEMS.contains(&ix.local(t, a)) && ix.attr(t, a, "Self").is_some())
                        .and_then(|a| ix.value(t, a, "Self"))
                });
                match owner {
                    Some(o) => if !entry.items.contains(&o) { entry.items.push(o) },
                    None => entry.other += 1,
                }
            }
        }
    }
    out
}

pub fn unused(parts: &BTreeMap<String, String>) -> Vec<String> {
    let used = referenced(parts);
    let Some(g) = parts.get(GRAPHIC) else { return vec![] };
    let Ok(ix) = Index::scan(g) else { return vec![] };
    (0..ix.nodes.len())
        .filter(|&i| matches!(ix.local(g, i), "Color" | "Tint" | "Gradient" | "MixedInk"))
        .filter(|&i| ix.attr(g, i, "Visible") != Some("false") && ix.attr(g, i, "Name") != Some("$ID/"))
        .filter_map(|i| ix.value(g, i, "Self"))
        .filter(|id| !FIXED.contains(&id.as_str()) && !used.contains_key(id))
        .collect()
}

/// A colour group of the Swatches panel (the root group holds the ungrouped swatches).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub root: bool,
    pub swatches: Vec<String>,
}

pub fn color_groups(parts: &BTreeMap<String, String>) -> Vec<GroupInfo> {
    let Some(t) = parts.get(DM) else { return vec![] };
    let Ok(ix) = Index::scan(t) else { return vec![] };
    let Ok((_, groups)) = read_groups(t, &ix) else { return vec![] };
    groups.into_iter().map(|g| GroupInfo {
        name: ix.by_self(t, &g.id).and_then(|i| ix.value(t, i, "Name")).unwrap_or_default(),
        id: g.id, root: g.root, swatches: g.refs,
    }).collect()
}

#[cfg(test)]
mod tests {
    use crate::{Command, Editor};
    use std::collections::BTreeMap;

    fn editor() -> Editor {
        let dm = "<Document>\n\t<ColorGroup Self=\"ColorGroup/[Root Color Group]\" Name=\"[Root Color Group]\" IsRootColorGroup=\"true\">\n\
\t\t<ColorGroupSwatch Self=\"u1ColorGroupSwatch0\" SwatchItemRef=\"Swatch/None\" />\n\
\t\t<ColorGroupSwatch Self=\"u1ColorGroupSwatch1\" SwatchItemRef=\"Color/Zeta\" />\n\
\t\t<ColorGroupSwatch Self=\"u1ColorGroupSwatch2\" SwatchItemRef=\"Color/C=15 M=0 Y=0 K=0\" />\n\
\t\t<ColorGroupSwatch Self=\"u1ColorGroupSwatch3\" SwatchItemRef=\"Color/C=100 M=0 Y=0 K=0\" />\n\
\t</ColorGroup>\n\t<ABullet Self=\"b\" />\n</Document>";
        let g = "<Graphic>\n\
\t<Color Self=\"Color/C=100 M=0 Y=0 K=0\" Space=\"CMYK\" ColorValue=\"100 0 0 0\" Name=\"C=100 M=0 Y=0 K=0\" SwatchColorGroupReference=\"u1ColorGroupSwatch3\" />\n\
\t<Color Self=\"Color/C=15 M=0 Y=0 K=0\" Space=\"CMYK\" ColorValue=\"15 0 0 0\" Name=\"C=15 M=0 Y=0 K=0\" SwatchColorGroupReference=\"u1ColorGroupSwatch2\" />\n\
\t<Color Self=\"Color/Zeta\" Space=\"CMYK\" ColorValue=\"0 0 0 10\" Name=\"Zeta\" SwatchColorGroupReference=\"u1ColorGroupSwatch1\" />\n\
\t<Swatch Self=\"Swatch/None\" Name=\"None\" SwatchColorGroupReference=\"u1ColorGroupSwatch0\" />\n</Graphic>";
        Editor::new(BTreeMap::from([("designmap.xml".into(), dm.into()), ("Resources/Graphic.xml".into(), g.into())]))
    }
    fn cmd(json: &str) -> Command { serde_json::from_str(json).unwrap() }
    fn refs(e: &Editor) -> Vec<String> {
        e.part("designmap.xml").unwrap().split("SwatchItemRef=\"").skip(1).map(|s| s.split('"').next().unwrap().to_owned()).collect()
    }

    #[test]
    fn sort_by_name_is_natural_and_by_value_is_c_m_y_k() {
        let mut e = editor();
        e.apply("sort", &[cmd(r#"{"op":"sortSwatches","by":"name"}"#)]).unwrap();
        assert_eq!(refs(&e), ["Swatch/None", "Color/C=15 M=0 Y=0 K=0", "Color/C=100 M=0 Y=0 K=0", "Color/Zeta"]);
        assert!(e.part("Resources/Graphic.xml").unwrap().contains("Name=\"Zeta\" SwatchColorGroupReference=\"u1ColorGroupSwatch3\""));
        e.apply("sort", &[cmd(r#"{"op":"sortSwatches","by":"value"}"#)]).unwrap();
        assert_eq!(refs(&e), ["Swatch/None", "Color/Zeta", "Color/C=15 M=0 Y=0 K=0", "Color/C=100 M=0 Y=0 K=0"]);
        e.undo().unwrap();
        e.undo().unwrap();
        assert_eq!(refs(&e), ["Swatch/None", "Color/Zeta", "Color/C=15 M=0 Y=0 K=0", "Color/C=100 M=0 Y=0 K=0"]);
    }

    #[test]
    fn unnamed_colours_that_objects_use_become_swatches_named_with_their_value() {
        let mut e = editor();
        let mut parts = e.parts().clone();
        parts.insert("Resources/Graphic.xml".into(), parts["Resources/Graphic.xml"].replace("</Graphic>",
            "	<Color Self=\"Color/u99\" Model=\"Process\" Space=\"CMYK\" ColorValue=\"10 20 30 40\" Name=\"$ID/\" Visible=\"false\" SwatchColorGroupReference=\"n\" />
	<Color Self=\"Color/u98\" Model=\"Process\" Space=\"CMYK\" ColorValue=\"1 2 3 4\" Name=\"$ID/\" Visible=\"false\" SwatchColorGroupReference=\"n\" />
</Graphic>"));
        parts.insert("Spreads/Spread_a.xml".into(), "<Spread><Rectangle Self=\"r1\" FillColor=\"Color/u99\" StrokeColor=\"Color/Zeta\"><Properties><Label><KeyValuePair Key=\"k\" Value=\"v\"/></Label></Properties></Rectangle><Oval Self=\"o1\" FillColor=\"Color/Zeta\" /></Spread>".into());
        let mut e2 = Editor::new(parts);
        std::mem::swap(&mut e, &mut e2);
        let used = super::referenced(e.parts());
        assert_eq!(used["Color/u99"].items, ["r1"]);
        assert_eq!(used["Color/Zeta"].items, ["r1", "o1"]);
        // swatches only a definition names are unused; hidden and unnamed ones are not offered
        assert_eq!(super::unused(e.parts()), ["Color/C=100 M=0 Y=0 K=0", "Color/C=15 M=0 Y=0 K=0"]);
        e.apply("unnamed", &[cmd(r#"{"op":"addUnnamedColors"}"#)]).unwrap();
        let g = e.part("Resources/Graphic.xml").unwrap();
        assert!(g.contains("Self=\"Color/C=10 M=20 Y=30 K=40\"") && g.contains("Name=\"C=10 M=20 Y=30 K=40\" Visible=\"true\" SwatchColorGroupReference=\"u1ColorGroupSwatch4\""), "{g}");
        assert!(g.contains("Color/u98"), "an unnamed colour nothing uses stays unnamed");
        assert!(e.part("Spreads/Spread_a.xml").unwrap().contains("FillColor=\"Color/C=10 M=20 Y=30 K=40\""));
        assert_eq!(refs(&e).last().unwrap(), "Color/C=10 M=20 Y=30 K=40");
        e.undo().unwrap();
        assert!(e.part("Resources/Graphic.xml").unwrap().contains("Self=\"Color/u99\""));
    }

    #[test]
    fn the_colour_panel_gives_objects_one_hidden_unnamed_colour_per_value() {
        let mut parts = editor().parts().clone();
        parts.insert("Spreads/Spread_a.xml".into(), "<Spread><Rectangle Self=\"r1\" /><Oval Self=\"o1\" /></Spread>".into());
        let mut e = Editor::new(parts);
        e.apply("c", &[cmd(r#"{"op":"setPaintColor","ids":["r1","o1"],"space":"RGB","value":[10.4996,20,300]}"#)]).unwrap();
        let s = e.part("Spreads/Spread_a.xml").unwrap().to_owned();
        let g = e.part("Resources/Graphic.xml").unwrap().to_owned();
        let id = g.split("Self=\"Color/u").nth(1).unwrap().split('"').next().unwrap().to_owned();
        assert_eq!(s.matches(&format!("FillColor=\"Color/u{id}\"")).count(), 2, "{s}");
        assert!(g.contains("Space=\"RGB\" ColorValue=\"10.5 20 255\"") && g.contains("Name=\"$ID/\"") && g.contains("Visible=\"false\""), "{g}");
        e.apply("c", &[cmd(r#"{"op":"setPaintColor","ids":["r1"],"stroke":true,"space":"rgb","value":[10.5,20,255]}"#)]).unwrap();
        assert_eq!(e.part("Resources/Graphic.xml").unwrap().matches("Name=\"$ID/\"").count(), 1, "the same value reuses the colour");
        assert!(e.part("Spreads/Spread_a.xml").unwrap().contains(&format!("StrokeColor=\"Color/u{id}\"")));
        assert!(e.apply("c", &[cmd(r#"{"op":"setPaintColor","ids":["r1"],"space":"CMYK","value":[1,2,3]}"#)]).is_err());
    }

    #[test]
    fn the_color_picker_makes_the_unnamed_colour_and_text_takes_it_by_the_reported_id() {
        let mut e = editor();
        let r = e.apply("c", &[cmd(r#"{"op":"newUnnamedColor","space":"CMYK","value":[10,20.0004,30,40]}"#)]).unwrap();
        let id = super::unnamed_color_id("CMYK", &[10.0, 20.0004, 30.0, 40.0]).unwrap();
        assert_eq!(r.created, [id.clone()]);
        let g = e.part("Resources/Graphic.xml").unwrap().to_owned();
        assert!(g.contains(&format!("Self=\"{id}\" Model=\"Process\" Space=\"CMYK\" ColorValue=\"10 20 30 40\"")) && g.contains("Name=\"$ID/\"") && g.contains("Visible=\"false\""), "{g}");
        // the same value makes no second colour, and the id is the one setPaintColor would use
        e.apply("c", &[cmd(r#"{"op":"newUnnamedColor","space":"cmyk","value":[10,20,30,40]}"#)]).unwrap();
        assert_eq!(e.part("Resources/Graphic.xml").unwrap().matches("Name=\"$ID/\"").count(), 1);
        assert!(e.apply("c", &[cmd(r#"{"op":"newUnnamedColor","space":"HSB","value":[1,2,3]}"#)]).is_err());
    }

    #[test]
    fn a_new_colour_group_takes_swatches_out_of_the_root_group_and_ungroup_puts_them_back() {
        let mut e = editor();
        e.apply("group", &[cmd(r#"{"op":"newColorGroup","name":"Cool","swatches":["Color/Zeta","Color/C=15 M=0 Y=0 K=0"]}"#)]).unwrap();
        let dm = e.part("designmap.xml").unwrap().to_owned();
        assert_eq!(refs(&e), ["Swatch/None", "Color/C=100 M=0 Y=0 K=0", "Color/Zeta", "Color/C=15 M=0 Y=0 K=0"], "{dm}");
        assert!(dm.contains("<ColorGroup Self=\"ColorGroup/Cool\" Name=\"Cool\" IsRootColorGroup=\"false\">"));
        assert!(dm.find("ColorGroup/Cool").unwrap() < dm.find("ABullet").unwrap());
        let g = e.part("Resources/Graphic.xml").unwrap();
        assert!(g.contains("Name=\"C=100 M=0 Y=0 K=0\" SwatchColorGroupReference=\"u1ColorGroupSwatch1\""), "{g}");
        e.apply("ungroup", &[cmd(r#"{"op":"ungroupColorGroup","id":"ColorGroup/Cool"}"#)]).unwrap();
        assert_eq!(refs(&e), ["Swatch/None", "Color/C=100 M=0 Y=0 K=0", "Color/Zeta", "Color/C=15 M=0 Y=0 K=0"]);
        assert!(!e.part("designmap.xml").unwrap().contains("ColorGroup/Cool"));
    }
}
