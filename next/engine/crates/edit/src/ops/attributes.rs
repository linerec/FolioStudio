//! Window ▸ Output ▸ Attributes (overprint fill / stroke / gap, nonprinting) and Window ▸ Utilities ▸ Script Label (the object's
//! label). One command, `setObjectAttributes`, takes IDML-like property names: `overprintFill`, `overprintStroke`,
//! `overprintGap`, `nonprinting` (booleans) and `label` (text).
//!
//! Measured with InDesign 2025 (artifacts/attrpanels-20261023/STATUS.ko.md, oracle attrpanels):
//! * An attribute is written where it differs from what the object style gives (`OverprintFill="true"`) and taken away where it
//!   does not (`false` is the default, so turning one off removes it).
//! * A script cannot set what does not apply: `overprintFill` needs a fill (not None, and not a graphic line), `overprintStroke`
//!   a stroke, `overprintGap` a stroke type other than Solid with a gap colour. The panel greys those boxes out.
//! * On a group the setting goes on the group and on everything inside it (nested groups too), except where it does not apply to a
//!   member (a member without a stroke does not get `OverprintStroke`). A group refuses a setting that applies to none of its
//!   members (`overprintStroke` of a group of filled shapes without strokes: oracle ap-group).
//! * The label is the script label `Label` (`Properties/Label/KeyValuePair Key="Label"`, a return as `&#xd;`); an empty label takes
//!   the pair away.
use super::objectstyle::from_style;
use super::{attr_edit, attr_removal, check_item, is_item, set_label};
use crate::xml::Index;
use crate::{EditError, Work};
use std::collections::BTreeMap;

const LABEL_KEY: &str = "Label";
const NONE: &str = "Swatch/None";
const SOLID: &str = "StrokeStyle/$ID/Solid";

/// (property, IDML attribute, the object style category it belongs to)
const FLAGS: [(&str, &str, &str); 4] = [
    ("overprintFill", "OverprintFill", "EnableFill"),
    ("overprintStroke", "OverprintStroke", "EnableStroke"),
    ("overprintGap", "OverprintGap", "EnableStroke"),
    ("nonprinting", "Nonprinting", ""),
];

/// What the object gives `name`: its own attribute, else its object style's, else `default`.
fn resolved(w: &Work<'_>, ix: &Index, t: &str, i: usize, name: &str, enable: &str, default: &str) -> String {
    ix.attr(t, i, name).map(str::to_owned).or_else(|| from_style(w, ix, t, i, &[], name, enable)).unwrap_or_else(|| default.to_owned())
}

/// Whether the setting applies to the item as it is now (InDesign refuses a script otherwise).
fn applies(w: &Work<'_>, ix: &Index, t: &str, i: usize, attribute: &str) -> bool {
    let local = ix.local(t, i);
    let stroked = || resolved(w, ix, t, i, "StrokeColor", "EnableStroke", NONE) != NONE;
    match attribute {
        "OverprintFill" => local != "GraphicLine" && resolved(w, ix, t, i, "FillColor", "EnableFill", NONE) != NONE,
        "OverprintStroke" => stroked(),
        "OverprintGap" => stroked() && resolved(w, ix, t, i, "StrokeType", "EnableStroke", SOLID) != SOLID && resolved(w, ix, t, i, "GapColor", "EnableStroke", NONE) != NONE,
        _ => true,
    }
}

/// Whether the setting applies to the item, or for a group to something inside it (a group refuses a stroke setting when no member has a stroke).
fn applies_to_any(w: &Work<'_>, ix: &Index, t: &str, i: usize, attribute: &str) -> bool {
    if ix.local(t, i) != "Group" {
        return applies(w, ix, t, i, attribute);
    }
    ix.children(i).iter().any(|&c| is_item(ix, t, c) && applies_to_any(w, ix, t, c, attribute))
}

/// `i` and what is inside it, in document order.
fn members(ix: &Index, t: &str, i: usize, out: &mut Vec<usize>) {
    out.push(i);
    if ix.local(t, i) == "Group" {
        for &c in ix.children(i) {
            if is_item(ix, t, c) {
                members(ix, t, c, out);
            }
        }
    }
}

/// The objects `ids` take the settings in `props`.
pub(crate) fn set(w: &mut Work<'_>, ids: &[String], props: &BTreeMap<String, serde_json::Value>) -> Result<(), EditError> {
    let mut flags: Vec<(&str, bool)> = Vec::new();
    let mut label: Option<&str> = None;
    for (key, value) in props {
        if key == "label" {
            label = Some(value.as_str().ok_or_else(|| EditError::new("invalid", "label: text"))?);
        } else if let Some((_, attribute, _)) = FLAGS.iter().find(|(name, ..)| name == key) {
            flags.push((attribute, value.as_bool().ok_or_else(|| EditError::new("invalid", format!("{key}: true or false")))?));
        } else {
            return Err(EditError::new("invalid", format!("{key} is not an attribute (overprintFill, overprintStroke, overprintGap, nonprinting, label)")));
        }
    }
    for id in ids {
        if !flags.is_empty() {
            let (part, ix, i) = w.find(id)?;
            let t = w.text(&part)?.to_owned();
            check_item(&ix, &t, i, id)?;
            let mut nodes = Vec::new();
            members(&ix, &t, i, &mut nodes);
            let mut edits = Vec::new();
            for &n in &nodes {
                for &(attribute, on) in &flags {
                    if !applies_to_any(w, &ix, &t, n, attribute) {
                        // a script is refused; a selection of several objects and the inside of a group skip it
                        if n == i && ids.len() == 1 {
                            return Err(EditError::new("notApplicable", format!("{id}: {attribute} does not apply (no fill, stroke or gap colour to overprint)")));
                        }
                        continue;
                    }
                    let enable = FLAGS.iter().find(|(_, a, _)| *a == attribute).map_or("", |f| f.2);
                    let base = from_style(w, &ix, &t, n, &[], attribute, enable).unwrap_or_else(|| "false".into());
                    if (base == "true") == on {
                        edits.extend(attr_removal(&ix, &t, n, attribute));
                    } else {
                        edits.push(attr_edit(&ix, &t, n, attribute, if on { "true" } else { "false" }));
                    }
                }
            }
            w.splice(&part, edits)?;
        }
        if let Some(text) = label {
            let (part, ix, i) = w.find(id)?;
            let t = w.text(&part)?;
            check_item(&ix, t, i, id)?;
            if text.is_empty() {
                remove_label(w, id)?;
            } else {
                set_label(w, id, LABEL_KEY, text)?;
            }
        }
    }
    Ok(())
}

/// Takes the pair `Label` away (the whole `Label` element when it was the only one).
fn remove_label(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let Some(props) = ix.named(&t, i, "Properties").first().copied() else { return Ok(()) };
    let Some(label) = ix.named(&t, props, "Label").first().copied() else { return Ok(()) };
    let pairs = ix.named(&t, label, "KeyValuePair");
    let Some(pair) = pairs.iter().copied().find(|&k| ix.value(&t, k, "Key").as_deref() == Some(LABEL_KEY)) else { return Ok(()) };
    // the line the element is on goes with it
    let (start, end) = if pairs.len() == 1 { (ix.nodes[label].start, ix.nodes[label].end) } else { (ix.nodes[pair].start, ix.nodes[pair].end) };
    let mut from = start;
    while from > 0 && matches!(t.as_bytes()[from - 1], b' ' | b'\t') {
        from -= 1;
    }
    if from > 0 && t.as_bytes()[from - 1] == b'\n' {
        from -= 1;
        if from > 0 && t.as_bytes()[from - 1] == b'\r' {
            from -= 1;
        }
    }
    w.splice(&part, vec![(from..end, String::new())])
}
