//! The Ink Manager (Resources/Graphic.xml `Ink`), as the Separations Preview and Swatches panel
//! menus open it:
//! - each ink is an element whose Self is `Ink/<name>` (process inks `Ink/$ID/Process Cyan`);
//!   its `InkType` is Normal, Transparent, Opaque or OpaqueIgnore, `ConvertToProcess` makes a
//!   spot ink separate as CMYK, `AliasInkName` (the target ink's plain name: oracle ink-alias)
//!   prints it on another ink's plate, and the spot swatch of the same name points at the
//!   target's swatch in `SpotInkAliasSpotColorReference` ("n" without an alias);
//! - the four process inks are fixed: they cannot be converted, aliased or given another type;
//! - "All Spots to Process" is every spot ink's `ConvertToProcess` (the dialog keeps no other
//!   state of its own);
//! - an alias may not lead back to the ink itself (the chain is followed).
//!
//! Measured in oracle `inks` (ink-convert, ink-alias, …).
use super::swatches::{of_kind, scan, self_of};
use super::*;

const GRAPHIC: &str = "Resources/Graphic.xml";
const TYPES: [&str; 4] = ["Normal", "Transparent", "Opaque", "OpaqueIgnore"];
const PROCESS: [&str; 4] = ["Process Cyan", "Process Magenta", "Process Yellow", "Process Black"];

fn clean(name: &str) -> &str {
    name.strip_prefix("$ID/").unwrap_or(name)
}

/// The ink element named `name` ("Process Cyan" or "$ID/Process Cyan" alike).
fn find(ix: &Index, t: &str, name: &str) -> Option<usize> {
    of_kind(ix, t, "Ink").into_iter().find(|&i| ix.value(t, i, "Name").is_some_and(|n| clean(&n) == clean(name)))
}

fn ink_name(ix: &Index, t: &str, i: usize) -> String {
    clean(&ix.value(t, i, "Name").unwrap_or_default()).to_owned()
}

/// The ink an alias reference names ("Ink/Gold", "Ink/$ID/Process Cyan").
fn alias_of(ix: &Index, t: &str, i: usize) -> Option<String> {
    let a = ix.value(t, i, "AliasInkName")?;
    let a = clean(a.strip_prefix("Ink/").unwrap_or(&a)).to_owned();
    (!a.is_empty() && a != "n" && a != "None").then_some(a)
}

/// What a spot swatch's `SpotInkAliasSpotColorReference` holds for an alias onto the ink `target`: its spot swatch.
fn alias_swatch(ix: &Index, t: &str, target: &str) -> String {
    let me = self_of("Color", target);
    if ix.by_self(t, &me).is_some() { me } else { "n".into() }
}

/// One ink's Ink Manager settings. `alias`: `None` leaves it, `Some("")` clears it.
pub(super) fn set_ink(w: &mut Work<'_>, name: &str, ink_type: Option<&str>, convert: Option<bool>, alias: Option<&str>) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let i = find(&ix, &t, name).ok_or_else(|| EditError::new("notFound", format!("ink {name}")))?;
    let me = ink_name(&ix, &t, i);
    let process = PROCESS.contains(&me.as_str());
    let mut edits = Vec::new();
    if let Some(ty) = ink_type {
        if !TYPES.contains(&ty) {
            return Err(EditError::new("invalid", format!("ink type {ty}")));
        }
        if process && ty != "Normal" {
            return Err(EditError::new("unsupported", format!("{me} is a process ink: its type cannot change")));
        }
        edits.push(attr_edit(&ix, &t, i, "InkType", ty));
    }
    if let Some(on) = convert {
        if process && on {
            return Err(EditError::new("unsupported", format!("{me} is a process ink: it cannot be converted")));
        }
        edits.push(attr_edit(&ix, &t, i, "ConvertToProcess", if on { "true" } else { "false" }));
    }
    let mut swatch_ref = None;
    match alias {
        None => {}
        Some("") => {
            edits.extend(attr_removal(&ix, &t, i, "AliasInkName"));
            swatch_ref = Some("n".to_owned());
        }
        Some(target) => {
            if process {
                return Err(EditError::new("unsupported", format!("{me} is a process ink: it cannot be aliased")));
            }
            let j = find(&ix, &t, target).ok_or_else(|| EditError::new("notFound", format!("ink {target}")))?;
            // follow the chain from the target: it must not come back to this ink
            let mut at = j;
            for _ in 0..of_kind(&ix, &t, "Ink").len() {
                if at == i {
                    break;
                }
                match alias_of(&ix, &t, at).and_then(|a| find(&ix, &t, &a)) {
                    Some(next) => at = next,
                    None => break,
                }
            }
            if at == i {
                return Err(EditError::new("invalid", format!("{target} already prints on {me}'s plate")));
            }
            let target_name = ink_name(&ix, &t, j);
            edits.push(attr_edit(&ix, &t, i, "AliasInkName", &target_name));
            swatch_ref = Some(alias_swatch(&ix, &t, &target_name));
        }
    }
    if let Some(value) = swatch_ref
        && let Some(sw) = ix.by_self(&t, &self_of("Color", &me))
    {
        edits.push(attr_edit(&ix, &t, sw, "SpotInkAliasSpotColorReference", &value));
    }
    if edits.is_empty() {
        return Ok(());
    }
    w.splice(GRAPHIC, edits)
}

/// "All Spots to Process": every spot ink's `ConvertToProcess`.
pub(super) fn all_spots(w: &mut Work<'_>, on: bool) -> Result<(), EditError> {
    let (t, ix) = scan(w, GRAPHIC)?;
    let want = if on { "true" } else { "false" };
    let edits: Vec<_> = of_kind(&ix, &t, "Ink")
        .into_iter()
        .filter(|&i| !PROCESS.contains(&ink_name(&ix, &t, i).as_str()) && ix.attr(&t, i, "ConvertToProcess") != Some(want))
        .map(|i| attr_edit(&ix, &t, i, "ConvertToProcess", want))
        .collect();
    if edits.is_empty() {
        return Ok(());
    }
    w.splice(GRAPHIC, edits)
}
