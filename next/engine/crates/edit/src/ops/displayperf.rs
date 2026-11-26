//! Display Performance: Object ▸ Display Performance (Fast / Typical / High Quality / Use View Setting) and
//! View ▸ Display Performance ▸ Clear Object-Level Display Settings.
//!
//! Measured with InDesign 2025 (artifacts/displayperf-20261023/STATUS.ko.md, oracle displayperf):
//! * The object commands write `LocalDisplaySetting` (`Optimized` is Fast Display, `Default` is Use View Setting) on
//!   the selected frame, shape, line, text frame or group, on the graphic placed in it and on every item inside a
//!   group (groups inside groups too). A selected graphic (the content grabber) takes it alone.
//! * Clear Object-Level Display Settings sets every item and graphic of every spread back to `Default`; items of
//!   parent pages keep theirs. It does not need a selection.
//! * The view's own setting (a window state) and Allow Object-Level Display Settings (an application preference)
//!   are not in the document, so no command here.
use super::{GRAPHICS, ITEMS, attr_edit};
use crate::xml::Index;
use crate::{EditError, Work};
use folio_model::LocalDisplay;

const ATTR: &str = "LocalDisplaySetting";

fn is_graphic(ix: &Index, t: &str, i: usize) -> bool {
    GRAPHICS.contains(&ix.local(t, i)) && ix.attr(t, i, "Self").is_some()
}

fn is_item(ix: &Index, t: &str, i: usize) -> bool {
    ITEMS.contains(&ix.local(t, i)) && ix.attr(t, i, "Self").is_some()
}

fn parse(setting: &str) -> Result<LocalDisplay, EditError> {
    LocalDisplay::from_idml(setting).ok_or_else(|| EditError::new("invalid", format!("{setting} is not a display setting (Default, HighQuality, Typical, Optimized)")))
}

/// `i` and everything inside it that carries a display setting: items and graphics.
fn collect(ix: &Index, t: &str, i: usize, out: &mut Vec<usize>) {
    if is_item(ix, t, i) || is_graphic(ix, t, i) {
        out.push(i);
    }
    for &c in ix.children(i) {
        collect(ix, t, c, out);
    }
}

/// Object ▸ Display Performance for the objects `ids`; `content`: the graphic inside a frame only (the content grabber).
pub(crate) fn set(w: &mut Work<'_>, ids: &[String], setting: &str, content: &bool) -> Result<(), EditError> {
    let value = parse(setting)?.idml();
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        let mut targets = Vec::new();
        if is_graphic(&ix, t, i) {
            targets.push(i);
        } else if !is_item(&ix, t, i) {
            return Err(EditError::new("unsupported", format!("{id} ({}) is not a page item", ix.local(t, i))));
        } else if *content {
            targets.extend(ix.children(i).iter().copied().filter(|&c| is_graphic(&ix, t, c)));
        } else {
            collect(&ix, t, i, &mut targets);
        }
        let edits = targets.into_iter().map(|n| attr_edit(&ix, t, n, ATTR, value)).collect();
        w.splice(&part, edits)?;
    }
    Ok(())
}

/// View ▸ Display Performance ▸ Clear Object-Level Display Settings.
pub(crate) fn clear(w: &mut Work<'_>) -> Result<(), EditError> {
    for part in w.parts_named("Spreads/") {
        let t = w.text(&part)?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        let edits = (0..ix.nodes.len())
            .filter(|&n| (is_item(&ix, t, n) || is_graphic(&ix, t, n)) && ix.attr(t, n, ATTR).is_some_and(|v| v != "Default"))
            .map(|n| attr_edit(&ix, t, n, ATTR, "Default"))
            .collect();
        w.splice(&part, edits)?;
    }
    Ok(())
}
