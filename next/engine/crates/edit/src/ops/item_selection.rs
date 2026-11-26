//! Identifying editable page items and placed graphics.

use super::*;

/// Page items: the elements that sit on a spread or in a group and carry a transform.
pub(super) const ITEMS: &[&str] = &["Rectangle", "Oval", "Polygon", "GraphicLine", "TextFrame", "Group", "Button", "MultiStateObject", "FormField", "EPSText", "TextPath"];
pub(super) const GRAPHICS: &[&str] = &["Image", "EPS", "PDF", "ImportedPage", "WMF", "PICT", "SVG"];

pub(super) fn is_item(ix: &Index, t: &str, i: usize) -> bool {
    ITEMS.contains(&ix.local(t, i)) && ix.attr(t, i, "Self").is_some()
}

pub(super) fn check_item(ix: &Index, t: &str, i: usize, id: &str) -> Result<(), EditError> {
    if !is_item(ix, t, i) {
        return Err(EditError::new("unsupported", format!("{id} ({}) is not a page item", ix.local(t, i))));
    }
    Ok(())
}
