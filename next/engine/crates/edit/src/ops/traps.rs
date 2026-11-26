//! Trap presets (Window ▸ Output ▸ Trap Presets), as InDesign keeps them (next/oracle/rules/trapx.ko.md):
//! - a definition is a `TrapPreset` element at the end of Resources/Styles.xml whose Self is `TrapPreset/<name>`;
//! - a document page names its preset in `Page/@AppliedTrapPreset`;
//! - deleting a preset needs a replacement, and the pages that used it take the replacement.
//!   Saving a preset is not trapping: Folio does not draw traps in separations yet (trapx.ko.md, item 3).
use super::*;
use super::swatches::{scan, self_of};
use folio_model::traps::{Preset, DEFAULT_PRESET, NO_TRAP_PRESET};

const STYLES: &str = "Resources/Styles.xml";
const KIND: &str = "TrapPreset";

/// (attribute, low, high): what InDesign accepts, in points and percents (measured, trapx.ko.md): a value outside is refused, not clamped.
/// BlackDensity must be above 0.
const NUMBERS: [(&str, f64, f64); 7] = [
    ("DefaultTrapWidth", 0.0, 8.0), ("BlackWidth", 0.0, 8.0), ("StepThreshold", 1.0, 100.0), ("BlackColorThreshold", 0.0, 100.0),
    ("BlackDensity", f64::MIN_POSITIVE, 10.0), ("SlidingTrapThreshold", 0.0, 100.0), ("ColorReduction", 0.0, 100.0),
];
/// MITER_END_JOIN, ROUND_END_JOIN, BEVEL_END_JOIN / MITER_TRAP_ENDS, OVERLAP_TRAP_ENDS / CENTER_EDGES, CHOKE, IMAGE_NEUTRAL_DENSITY, IMAGES_OVER_SPREAD.
const JOINS: [&str; 3] = ["MiterEndJoin", "RoundEndJoin", "BevelEndJoin"];
const ENDS: [&str; 2] = ["MiterTrapEnds", "OverlapTrapEnds"];
const PLACEMENTS: [&str; 4] = ["CenterEdges", "Choke", "ImageNeutralDensity", "ImagesOverSpread"];
const BOOLEANS: [&str; 4] = ["ObjectsToImages", "ImagesToImages", "InternalImages", "OneBitImages"];

fn presets(ix: &Index, t: &str) -> Vec<usize> {
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == KIND && ix.attr(t, i, "Self").is_some()).collect()
}

/// The preset's attributes as the engine writes them, merged over `base` (the element, or InDesign's defaults).
fn merged(base: &Preset, p: &Preset) -> Result<Vec<(&'static str, String)>, EditError> {
    let pick = |new: Option<f64>, old: Option<f64>| new.or(old);
    let numbers = [
        pick(p.default_width, base.default_width), pick(p.black_width, base.black_width), pick(p.step_threshold, base.step_threshold),
        pick(p.black_color_threshold, base.black_color_threshold), pick(p.black_density, base.black_density),
        pick(p.sliding_threshold, base.sliding_threshold), pick(p.color_reduction, base.color_reduction),
    ];
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for ((key, lo, hi), value) in NUMBERS.iter().zip(numbers) {
        let v = value.ok_or_else(|| EditError::new("invalid", format!("{key} is missing")))?;
        if !v.is_finite() || v < *lo || v > *hi {
            return Err(EditError::new("invalid", format!("{key} must be between {lo} and {hi}")));
        }
        out.push((key, geom::num(v)));
    }
    // The IDML words of the scripting enumerations (EndJoin, TrapEndTypes, TrapImagePlacementTypes), read from the IDML InDesign
    // wrote (oracle trapx-enum-words). A value already in the element is kept as it is; a new one has to be one of these.
    let text = |new: &str, old: &str, allowed: &[&str], key: &'static str| -> Result<(&'static str, String), EditError> {
        if !new.is_empty() && !allowed.contains(&new) { return Err(EditError::new("invalid", format!("{key} {new} is not an InDesign value"))); }
        Ok((key, if new.is_empty() { old } else { new }.to_owned()))
    };
    out.push(text(&p.join, &base.join, &JOINS, "TrapJoin")?);
    out.push(text(&p.end, &base.end, &ENDS, "TrapEnd")?);
    out.push(text(&p.image_placement, &base.image_placement, &PLACEMENTS, "ImagePlacement")?);
    let flags = [p.objects_to_images.or(base.objects_to_images), p.images_to_images.or(base.images_to_images),
        p.internal_images.or(base.internal_images), p.one_bit_images.or(base.one_bit_images)];
    for (key, flag) in BOOLEANS.iter().zip(flags) {
        out.push((key, flag.ok_or_else(|| EditError::new("invalid", format!("{key} is missing")))?.to_string()));
    }
    Ok(out)
}

/// The preset element read back into the model's shape (the numbers and words, not the IDML text).
fn read(ix: &Index, t: &str, i: usize) -> Preset {
    let text = |k: &str| ix.attr(t, i, k).unwrap_or_default().to_owned();
    let number = |k: &str| ix.attr(t, i, k).and_then(|v| v.parse::<f64>().ok());
    let flag = |k: &str| ix.attr(t, i, k).and_then(|v| v.parse::<bool>().ok());
    Preset {
        id: text("Self"), name: text("Name"), default_width: number("DefaultTrapWidth"), black_width: number("BlackWidth"),
        join: text("TrapJoin"), end: text("TrapEnd"), objects_to_images: flag("ObjectsToImages"), images_to_images: flag("ImagesToImages"),
        internal_images: flag("InternalImages"), one_bit_images: flag("OneBitImages"), image_placement: text("ImagePlacement"),
        step_threshold: number("StepThreshold"), black_color_threshold: number("BlackColorThreshold"), black_density: number("BlackDensity"),
        sliding_threshold: number("SlidingTrapThreshold"), color_reduction: number("ColorReduction"),
    }
}

fn element(id: &str, name: &str, values: &[(&str, String)]) -> String {
    let mut e = format!("<{KIND} Self=\"{}\" Name=\"{}\"", escape(id), escape(name));
    // IDML's attribute order (panels-traps.ko.md): widths, join, end, image flags, placement, thresholds
    let order = ["DefaultTrapWidth", "BlackWidth", "TrapJoin", "TrapEnd", "ObjectsToImages", "ImagesToImages", "InternalImages", "OneBitImages",
        "ImagePlacement", "StepThreshold", "BlackColorThreshold", "BlackDensity", "SlidingTrapThreshold", "ColorReduction"];
    for key in order {
        if let Some((_, v)) = values.iter().find(|(k, _)| *k == key) { e.push_str(&format!(" {key}=\"{}\"", escape(v))); }
    }
    e.push_str(" />");
    e
}

/// A name another preset does not have (InDesign compares whole names).
fn name_free(ix: &Index, t: &str, name: &str, except: Option<usize>) -> bool {
    !presets(ix, t).into_iter().any(|i| Some(i) != except && ix.attr(t, i, "Name") == Some(name))
}

fn insert(w: &mut Work<'_>, e: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, STYLES)?;
    let at = presets(&ix, &t).last().map(|&i| ix.nodes[i].end).ok_or_else(|| EditError::new("unsupported", "Styles.xml has no TrapPreset"))?;
    w.splice(STYLES, vec![(at..at, format!("\n\t{e}"))])
}

/// Every Page (document and parent pages) naming `old` names `new` instead.
fn rename_in_pages(w: &mut Work<'_>, old: &str, new: &str) -> Result<(), EditError> {
    let needle = format!("AppliedTrapPreset=\"{}\"", escape(old));
    let parts: Vec<String> = w.ed.parts.keys().filter(|p| p.ends_with(".xml") && (p.starts_with("Spreads/") || p.starts_with("MasterSpreads/"))).cloned().collect();
    for part in parts {
        let (t, ix) = scan(w, &part)?;
        if !t.contains(&needle) { continue; }
        let edits = (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "Page" && ix.attr(&t, i, "AppliedTrapPreset") == Some(old))
            .map(|i| attr_edit(&ix, &t, i, "AppliedTrapPreset", new)).collect();
        w.splice(&part, edits)?;
    }
    Ok(())
}

/// Window ▸ Output ▸ Trap Presets ▸ New / Preset Options: `id` empty creates, otherwise edits. A name already
/// used by another preset is refused (InDesign: `add` fails).
pub(crate) fn save(w: &mut Work<'_>, p: &Preset) -> Result<(), EditError> {
    let (t, ix) = scan(w, STYLES)?;
    let existing = if p.id.is_empty() { None } else {
        Some(ix.by_self(&t, &p.id).filter(|&i| ix.local(&t, i) == KIND).ok_or_else(|| EditError::new("notFound", format!("trap preset {}", p.id)))?)
    };
    let base = match existing { Some(i) => read(&ix, &t, i), None => Preset::with_defaults("") };
    let name = if p.name.is_empty() { base.name.clone() } else { p.name.clone() };
    if name.trim().is_empty() { return Err(EditError::new("invalid", "a trap preset needs a name")); }
    if !name_free(&ix, &t, &name, existing) { return Err(EditError::new("invalid", format!("a trap preset named {name} already exists"))); }
    let values = merged(&base, p)?;
    match existing {
        None => {
            let id = self_of(KIND, &name);
            insert(w, &element(&id, &name, &values))?;
            w.created.push(id);
        }
        Some(i) => {
            let renamed = name != base.name;
            if renamed && p.built_in() { return Err(EditError::new("locked", "the built-in presets keep their names")); }
            let id = if renamed { self_of(KIND, &name) } else { base.id.clone() };
            let mut edits = vec![attr_edit(&ix, &t, i, "Name", &name), attr_edit(&ix, &t, i, "Self", &id)];
            edits.extend(values.iter().map(|(k, v)| attr_edit(&ix, &t, i, k, v)));
            w.splice(STYLES, edits)?;
            if renamed { rename_in_pages(w, &base.id, &id)?; }
            w.created.push(id);
        }
    }
    Ok(())
}

/// Trap Presets panel menu ▸ Duplicate Preset: a copy with `name` (empty: InDesign's own copy name).
pub(crate) fn duplicate(w: &mut Work<'_>, id: &str, name: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, STYLES)?;
    let i = ix.by_self(&t, id).filter(|&i| ix.local(&t, i) == KIND).ok_or_else(|| EditError::new("notFound", format!("trap preset {id}")))?;
    let base = read(&ix, &t, i);
    let name = if name.is_empty() { copy_name(&ix, &t, &base.name) } else { name.to_owned() };
    if name.trim().is_empty() || !name_free(&ix, &t, &name, None) { return Err(EditError::new("invalid", format!("a trap preset named {name} already exists"))); }
    let values = merged(&base, &Preset::default())?;
    let new = self_of(KIND, &name);
    insert(w, &element(&new, &name, &values))?;
    w.created.push(new);
    Ok(())
}

/// InDesign names a copy `<name> Copy <n>` with the smallest free n; a copy of a copy counts from the original's name
/// (trapx.ko.md). The built-ins are named after their scripting names, which is what the DOM shows.
fn copy_name(ix: &Index, t: &str, base: &str) -> String {
    // measured (trapx-builtin-probe): a copy of the built-in default is "[Default] Copy 1", of the no-trap preset "[No trap preset] Copy 1"
    let base = match base { "$ID/k[No Trap Preset]" => "[No trap preset]", "$ID/kDefaultTrapStyleName" => "[Default]", other => other.strip_prefix("$ID/").unwrap_or(other) };
    let stem = match base.rsplit_once(" Copy ") { Some((s, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => s, _ => base };
    (1..).map(|n| format!("{stem} Copy {n}")).find(|name| name_free(ix, t, name, None)).unwrap()
}

/// Trap Presets panel menu ▸ Delete Preset: `replacement` takes the pages that used `id`.
pub(crate) fn delete(w: &mut Work<'_>, id: &str, replacement: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, STYLES)?;
    let i = ix.by_self(&t, id).filter(|&i| ix.local(&t, i) == KIND).ok_or_else(|| EditError::new("notFound", format!("trap preset {id}")))?;
    if replacement.is_empty() { return Err(EditError::new("invalid", "deleting a trap preset needs a replacement")); }
    if replacement == id { return Err(EditError::new("invalid", "the replacement must be another preset")); }
    ix.by_self(&t, replacement).filter(|&r| ix.local(&t, r) == KIND).ok_or_else(|| EditError::new("notFound", format!("trap preset {replacement}")))?;
    if id == DEFAULT_PRESET || id == NO_TRAP_PRESET { return Err(EditError::new("locked", "the built-in presets cannot be deleted")); }
    rename_in_pages(w, id, replacement)?;
    let (t, ix) = scan(w, STYLES)?;
    let i = ix.by_self(&t, id).unwrap_or(i);
    w.splice(STYLES, vec![(element_span(&ix, &t, i), String::new())])
}

/// Trap Presets panel menu ▸ Assign Trap Preset: the pages `pages` use `preset` (document pages and parent pages: InDesign
/// takes both, trapx.ko.md). Nothing changes when one page is unknown.
pub(crate) fn assign(w: &mut Work<'_>, pages: &[String], preset: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w, STYLES)?;
    ix.by_self(&t, preset).filter(|&i| ix.local(&t, i) == KIND).ok_or_else(|| EditError::new("notFound", format!("trap preset {preset}")))?;
    if pages.is_empty() { return Err(EditError::new("invalid", "no pages to assign")); }
    for page in pages {
        let (part, ix, i) = w.find(page)?;
        let t = w.text(&part)?.to_owned();
        if ix.local(&t, i) != "Page" || !(part.starts_with("Spreads/") || part.starts_with("MasterSpreads/")) { return Err(EditError::new("invalid", format!("{page} is not a page"))); }
        let edit = attr_edit(&ix, &t, i, "AppliedTrapPreset", preset);
        w.splice(&part, vec![edit])?;
    }
    Ok(())
}

/// Trap Presets panel ▸ Load Trap Presets (build-c): the trap presets of another document, given as its `Resources/Styles.xml`; a preset with
/// the same name takes the loaded one's settings, the others are added. The same rules as Book ▸ Synchronize for trap presets (ops/book_resources.rs).
pub(crate) fn load(w: &mut Work<'_>, parts: &std::collections::BTreeMap<String, String>) -> Result<(), EditError> {
    if !parts.contains_key("Resources/Styles.xml") { return Err(EditError::new("invalid", "the other document's Resources/Styles.xml is needed")); }
    super::book_resources::sync(w, &["trapStyle".to_owned()], parts)
}
