//! The Page tool's size presets (control bar "Page Size" list; DOM `documentPreferences.pageSize`).
//!
//! Measured with InDesign 2025 (`next/oracle/indesign/pagectl-c1-probe.jsx` group a and c, `next/oracle/rules/pagectl-c1.ko.md`):
//! these are the names the DOM accepts for `pageSize`; any other size reports `Custom`. A size is a preset when both sides equal the
//! table (the DOM does not round: 595.2756 x 841.8898 is already `Custom`) in either orientation. A preset keeps the orientation of
//! the page it is applied to (landscape stays landscape; a square page counts as portrait).
use super::*;

/// Name, width and height in points, portrait (width <= height), as InDesign stores them in IDML (nine decimals).
pub const PAGE_SIZE_PRESETS: [(&str, f64, f64); 11] = [
    ("Letter", 612.0, 792.0),
    ("Legal", 612.0, 1008.0),
    ("Tabloid", 792.0, 1224.0),
    ("Letter - Half", 396.0, 612.0),
    ("Legal - Half", 504.0, 612.0),
    ("Compact Disc", 340.157480315, 340.157480315),
    ("A3", 841.889763778, 1190.5511811),
    ("A4", 595.275590551, 841.889763778),
    ("A5", 419.527559055, 595.275590551),
    ("B5", 498.897637795, 708.661417323),
    ("iPad", 768.0, 1024.0),
];

/// The preset a page of this size is, in either orientation (`None`: Custom).
pub fn page_size_preset_of(width: f64, height: f64) -> Option<&'static str> {
    let (lo, hi) = (width.min(height), width.max(height));
    PAGE_SIZE_PRESETS
        .iter()
        .find(|(_, w, h)| (w - lo).abs() < 1e-6 && (h - hi).abs() < 1e-6)
        .map(|(name, _, _)| *name)
}

/// The size of a preset for a page of the given orientation: `(width, height)`.
pub fn page_size_of_preset(name: &str, landscape: bool) -> Option<(f64, f64)> {
    let &(_, w, h) = PAGE_SIZE_PRESETS.iter().find(|(n, _, _)| *n == name)?;
    Some(if landscape { (h, w) } else { (w, h) })
}

/// Gives pages a preset size (`preset`, in the orientation each page has) or explicit `width` / `height`, growing about `anchor`
/// (`[0, 0]` the top left, `[0.5, 0.5]` the centre; the reference point of the control bar). Every page is moved the way
/// `setPageGeometry` does (InDesign `Page.reframe`), one after the other because a changed page moves the pages after it.
pub(crate) fn set_page_size(
    w: &mut Work<'_>,
    pages: &[String],
    preset: &Option<String>,
    width: &Option<f64>,
    height: &Option<f64>,
    anchor: &Option<[f64; 2]>,
) -> Result<(), EditError> {
    let anchor = anchor.unwrap_or([0.0, 0.0]);
    if anchor.iter().any(|a| !a.is_finite() || !(0.0..=1.0).contains(a)) {
        return Err(EditError::new("invalid", "the anchor is two numbers from 0 to 1"));
    }
    if let Some(name) = preset
        && page_size_of_preset(name, false).is_none() {
        return Err(EditError::new("invalid", format!("unknown page size preset: {name}")));
    }
    if preset.is_none() && width.is_none() && height.is_none() {
        return Err(EditError::new("invalid", "a preset, a width or a height is needed"));
    }
    let mut seen = BTreeSet::new();
    for id in pages.iter().filter(|id| seen.insert(*id)) {
        // the page as it is now: an earlier page of this call may have moved it
        let r = {
            let book = Book::read(w)?;
            let (s, p) = book.locate(id)?;
            let page = &book.spreads[s].pages[p];
            let (m, b) = (mat(&page.xml), bounds(&page.xml));
            [b[0] + m.0[4], b[1] + m.0[5], b[2] + m.0[4], b[3] + m.0[5]]
        };
        let (cw, ch) = (r[2] - r[0], r[3] - r[1]);
        let (mut nw, mut nh) = (width.unwrap_or(cw), height.unwrap_or(ch));
        if let Some(name) = preset {
            (nw, nh) = page_size_of_preset(name, cw > ch).expect("checked above");
        }
        let (x, y) = (r[0] + (cw - nw) * anchor[0], r[1] + (ch - nh) * anchor[1]);
        set_page_geometry(w, std::slice::from_ref(id), &[x, y, x + nw, y + nh])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Measured with InDesign 2025 (pagectl-c1a / pagectl-c1c): `pageSize` names with their `pageWidth` x `pageHeight`.
    #[test]
    fn presets_are_the_measured_sizes() {
        assert_eq!(page_size_of_preset("A4", false), Some((595.275590551, 841.889763778)));
        assert_eq!(page_size_of_preset("A4", true), Some((841.889763778, 595.275590551)));
        assert_eq!(page_size_of_preset("Tabloid", true), Some((1224.0, 792.0)));
        assert_eq!(page_size_of_preset("Letter - Half", true), Some((612.0, 396.0)));
        assert_eq!(page_size_of_preset("Compact Disc", true), Some((340.157480315, 340.157480315)));
        assert_eq!(page_size_of_preset("1024 x 768", false), None);
    }

    /// The DOM names exact sizes only, in both orientations (pagectl-c1c: exact landscape A4 / Letter report their names).
    #[test]
    fn names_follow_exact_sizes() {
        assert_eq!(page_size_preset_of(612.0, 792.0), Some("Letter"));
        assert_eq!(page_size_preset_of(792.0, 612.0), Some("Letter"));
        assert_eq!(page_size_preset_of(841.889763778, 595.275590551), Some("A4"));
        assert_eq!(page_size_preset_of(340.157480315, 340.157480315), Some("Compact Disc"));
        // measured: these report Custom
        assert_eq!(page_size_preset_of(595.276, 841.89), None);
        assert_eq!(page_size_preset_of(595.2756, 841.8898), None);
        assert_eq!(page_size_preset_of(612.0, 792.001), None);
        assert_eq!(page_size_preset_of(300.0, 400.0), None);
    }
}
