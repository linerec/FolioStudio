//! Edit ▸ Convert to Profile (InDesign 2026 measured, 2025 not checked: `artifacts/profileconv-c1-20261023/STATUS.ko.md`):
//! the document's colours are converted from its profiles to the destination ones and the document is assigned the new profiles.
//!
//! - Every swatch colour in RGB or CMYK (process and spot, also the hidden unnamed ones) is converted, each space from the document's
//!   profile of that space to the destination one (`folio_color::ColorConverter`: the dialog's engine Adobe (ACE), intent and black
//!   point compensation). Lab swatches, the locked swatches (`ColorEditable="false"`: Black, Registration; Paper, None) are not;
//!   tints, gradients and every object keep pointing at their swatch, placed graphics keep their own profile and pixels.
//! - A swatch whose name is InDesign's automatic one for its values (`C=10 M=20 Y=30 K=40`, `R=10 G=20 B=30`) is renamed to the
//!   automatic name of the new values; a name already taken gets ` 2`, ` 3`... (measured: `C=94 M=0 Y=2 K=0 2`).
//! - `Document@RGBProfile` / `@CMYKProfile` take the destination profile names; policies and rendering intents are left alone.
//! - One undo step. A space whose destination is empty or the document's own profile is left as it is.

use super::swatches::{self, of_kind, scan};
use super::{attr_edit, geom};
use crate::{EditError, Work};
use folio_color::icc::{Intent, Space};
use folio_color::{ColorConverter, ConvertSettings, ProfileIndex, auto_name_cmyk, auto_name_rgb, read_profile};
use std::collections::BTreeSet;
use std::sync::OnceLock;

const GRAPHIC: &str = "Resources/Graphic.xml";
const DESIGNMAP: &str = "designmap.xml";
/// The profiles of a document that names none (InDesign's North American defaults).
const DEFAULT_RGB: &str = "sRGB IEC61966-2.1";
const DEFAULT_CMYK: &str = "U.S. Web Coated (SWOP) v2";

/// The installed profile with this description (the OS profile folders; nothing is bundled).
fn profile_bytes(name: &str) -> Result<Vec<u8>, EditError> {
    static INDEX: OnceLock<ProfileIndex> = OnceLock::new();
    let index = INDEX.get_or_init(ProfileIndex::scan_installed);
    let path = index
        .find(name)
        .or_else(|| ProfileIndex::scan(index.roots()).find(name))
        .ok_or_else(|| EditError::new("notFound", format!("ICC profile \"{name}\" is not installed")))?;
    read_profile(&path).map_err(|e| EditError::new("invalid", format!("ICC profile \"{name}\": {e}")))
}

fn intent_of(name: &str) -> Result<Intent, EditError> {
    match name {
        "UseColorSettings" | "RelativeColorimetric" | "" => Ok(Intent::Relative),
        "Perceptual" => Ok(Intent::Perceptual),
        "Saturation" => Ok(Intent::Saturation),
        "AbsoluteColorimetric" => Ok(Intent::Absolute),
        other => Err(EditError::new("invalid", format!("invalid rendering intent: {other}"))),
    }
}

/// The document's RGB and CMYK profile names (from designmap.xml's text).
fn profiles_of(designmap: &str) -> Result<(String, String), EditError> {
    let ix = crate::xml::Index::scan(designmap).map_err(|e| EditError::new("xml", e.to_string()))?;
    let i = document_node(designmap, &ix)?;
    let name = |key: &str, default: &str| ix.value(designmap, i, key).filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_owned());
    Ok((name("RGBProfile", DEFAULT_RGB), name("CMYKProfile", DEFAULT_CMYK)))
}

fn document_profiles(w: &Work<'_>) -> Result<(String, String), EditError> {
    profiles_of(w.text(DESIGNMAP)?)
}

fn document_node(t: &str, ix: &crate::xml::Index) -> Result<usize, EditError> {
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Document").ok_or_else(|| EditError::new("xml", "missing Document"))
}

fn numbers(s: Option<&str>) -> Vec<f64> {
    s.unwrap_or_default().split_whitespace().filter_map(|x| x.parse().ok()).collect()
}

/// `convertToProfile`: see the module comment. `rgb_profile` / `cmyk_profile` are ICC descriptions ("" keeps the document's).
pub(crate) fn convert(w: &mut Work<'_>, rgb_profile: &str, cmyk_profile: &str, intent: &str, black_point_compensation: &bool) -> Result<(), EditError> {
    let settings = ConvertSettings { intent: intent_of(intent)?, black_point_compensation: *black_point_compensation };
    let (src_rgb, src_cmyk) = document_profiles(w)?;
    let dst_rgb = if rgb_profile.is_empty() { src_rgb.clone() } else { rgb_profile.to_owned() };
    let dst_cmyk = if cmyk_profile.is_empty() { src_cmyk.clone() } else { cmyk_profile.to_owned() };
    let build = |from: &str, to: &str, space: Space| -> Result<Option<ColorConverter>, EditError> {
        if from == to {
            return Ok(None);
        }
        let conv = ColorConverter::new(&profile_bytes(from)?, &profile_bytes(to)?, settings)
            .map_err(|e| EditError::new("invalid", format!("cannot convert \"{from}\" to \"{to}\": {e}")))?;
        debug_assert_eq!(conv.space(), space);
        Ok(Some(conv))
    };
    let rgb = build(&src_rgb, &dst_rgb, Space::Rgb)?;
    let cmyk = build(&src_cmyk, &dst_cmyk, Space::Cmyk)?;

    // pass 1: the values of every convertible colour (one splice), and which of them are renamed
    let (t, ix) = scan(w, GRAPHIC)?;
    let colors = of_kind(&ix, &t, "Color");
    let mut taken: BTreeSet<String> = ["Color", "Tint", "Gradient", "MixedInk", "MixedInkGroup"]
        .iter()
        .flat_map(|k| of_kind(&ix, &t, k))
        .filter_map(|i| ix.value(&t, i, "Name"))
        .collect();
    let mut edits = Vec::new();
    let mut renames: Vec<(String, String, String)> = Vec::new(); // (Self, old name, automatic name of the new values)
    for i in colors {
        if ix.attr(&t, i, "ColorEditable") == Some("false") || ix.attr(&t, i, "Model") == Some("Registration") {
            continue;
        }
        let (conv, auto): (&ColorConverter, fn(&[f64]) -> Option<String>) = match (ix.attr(&t, i, "Space"), &rgb, &cmyk) {
            (Some("RGB"), Some(c), _) => (c, auto_name_rgb),
            (Some("CMYK"), _, Some(c)) => (c, auto_name_cmyk),
            _ => continue,
        };
        let old = numbers(ix.attr(&t, i, "ColorValue"));
        let Some(new) = conv.convert(&old) else { continue };
        if new == old {
            continue;
        }
        edits.push(attr_edit(&ix, &t, i, "ColorValue", &new.iter().map(|v| geom::num(*v)).collect::<Vec<_>>().join(" ")));
        let name = ix.value(&t, i, "Name").unwrap_or_default();
        if auto(&old).is_some_and(|a| a == name) && let (Some(me), Some(next)) = (ix.value(&t, i, "Self"), auto(&new)) {
            renames.push((me, name, next));
        }
    }
    w.splice(GRAPHIC, edits)?;

    // pass 2: the renames (each moves the element to its sorted place and re-references it)
    for (me, old_name, wanted) in renames {
        let mut name = wanted.clone();
        let mut n = 2;
        while name != old_name && taken.contains(&name) {
            name = format!("{wanted} {n}");
            n += 1;
        }
        if name == old_name {
            continue;
        }
        taken.remove(&old_name);
        taken.insert(name.clone());
        swatches::set_swatch(w, &me, None, None, None, Some(&name))?;
    }

    // the document's profiles
    let (t, ix) = scan(w, DESIGNMAP)?;
    let i = document_node(&t, &ix)?;
    let mut edits = Vec::new();
    for (key, was, now) in [("RGBProfile", &src_rgb, &dst_rgb), ("CMYKProfile", &src_cmyk, &dst_cmyk)] {
        if was != now {
            edits.push(attr_edit(&ix, &t, i, key, now));
        }
    }
    w.splice(DESIGNMAP, edits)
}

/// A swatch across a rename: its slot in the swatch panel (the hidden unnamed colours have none: their `Self`).
fn identity(ix: &crate::xml::Index, t: &str, i: usize) -> String {
    match ix.attr(t, i, "SwatchColorGroupReference") {
        Some(slot) if slot != "n" => slot.to_owned(),
        _ => ix.attr(t, i, "Self").unwrap_or_default().to_owned(),
    }
}

type Row = std::collections::BTreeMap<&'static str, String>;

impl crate::Editor {
    /// What `convertToProfile` would do to the document now, without doing it (a trial on a copy): the profiles before and after, and every
    /// swatch whose values change (`id` before the conversion, name, new name when renamed, space, values before and after).
    pub fn convert_to_profile_plan(&self, rgb_profile: &str, cmyk_profile: &str, intent: &str, black_point_compensation: bool) -> Result<serde_json::Value, EditError> {
        use serde_json::json;
        let designmap = self.part(DESIGNMAP).ok_or_else(|| EditError::new("notFound", DESIGNMAP))?;
        let (src_rgb, src_cmyk) = profiles_of(designmap)?;
        let mut trial = crate::Editor::new(self.parts().clone());
        let command = crate::commands::RegisteredCommand::ConvertToProfile {
            rgb_profile: rgb_profile.into(), cmyk_profile: cmyk_profile.into(), intent: intent.into(), black_point_compensation,
        };
        trial.apply("Convert to Profile", &[command.into()])?;
        let (dst_rgb, dst_cmyk) = profiles_of(trial.part(DESIGNMAP).ok_or_else(|| EditError::new("notFound", DESIGNMAP))?)?;
        let list = |parts: &std::collections::BTreeMap<String, String>| -> Result<Vec<(String, Row)>, EditError> {
            let t = parts.get(GRAPHIC).ok_or_else(|| EditError::new("notFound", GRAPHIC))?;
            let ix = crate::xml::Index::scan(t).map_err(|e| EditError::new("xml", e.to_string()))?;
            Ok(of_kind(&ix, t, "Color").into_iter().map(|i| {
                let get = |k: &str| ix.value(t, i, k).unwrap_or_default();
                (identity(&ix, t, i), [("id", get("Self")), ("name", get("Name")), ("space", get("Space")), ("values", get("ColorValue"))].into_iter().collect())
            }).collect())
        };
        let after: std::collections::BTreeMap<String, Row> = list(trial.parts())?.into_iter().collect();
        let (mut rgb, mut cmyk, mut renamed) = (0, 0, 0);
        let mut changed = Vec::new();
        for (key, was) in list(self.parts())? {
            let Some(now) = after.get(&key) else { continue };
            if was["values"] == now["values"] {
                continue;
            }
            if was["space"] == "RGB" { rgb += 1 } else { cmyk += 1 }
            renamed += (was["name"] != now["name"]) as usize;
            let nums = |m: &Row| numbers(Some(m["values"].as_str()));
            changed.push(json!({"id": was["id"], "name": was["name"], "newName": now["name"], "space": was["space"], "from": nums(&was), "to": nums(now)}));
        }
        Ok(json!({
            "source": {"rgb": src_rgb, "cmyk": src_cmyk}, "destination": {"rgb": dst_rgb, "cmyk": dst_cmyk},
            "count": {"rgb": rgb, "cmyk": cmyk, "renamed": renamed}, "changed": changed,
        }))
    }
}
