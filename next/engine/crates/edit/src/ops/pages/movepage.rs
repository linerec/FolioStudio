//! Page tool: a page dragged (or typed X / Y in the control bar) inside its spread, with "Objects Move with Page" on or off.
//! Measured with real mouse and keyboard input in InDesign 2025 (`oracle/rules/pagetool-c1.ko.md`, goldens `oracle/goldens/pagetool-c1`).
//!
//! The page moves by `d` against everything else of its spread. The spread lays its page out again, so what is stored is the other
//! way round: the page stays where the spread puts it and everything that does not go with the page moves by `-d`.
//! With the option on, the page's own objects and guides go with it (they do not move at all against the page); a locked object
//! (`Locked`) and an object on the pasteboard stay behind. "Own" is the page the object overlaps most, as `Spread::read` finds it,
//! and it is decided before the page moves. Pages of one spread keep their places against each other in InDesign (a facing spread
//! moves only up and down); Folio's spread layout has no place for that, so there the page moves as `setPageGeometry` does it
//! (option off), and with the option on the command is refused.
use super::*;

pub(crate) fn move_page(w: &mut Work<'_>, page: &str, by: &[f64; 2], objects_move_with_page: &bool) -> Result<(), EditError> {
    if by.iter().any(|v| !v.is_finite() || v.abs() > 15552.0) {
        return Err(EditError::new("invalid", "a page moves by 15552 pt or less"));
    }
    let mut book = Book::read(w)?;
    let (s, p) = book.locate(page)?;
    let [dx, dy] = *by;
    let alone = book.spreads[s].pages.len() == 1 && book.pref("FacingPages") != "true" && book.pref("PageBinding") != "RightToLeft" && book.spreads[s].rotation() == 0;
    if !alone {
        // InDesign keeps the pages of a spread at places against each other (a facing spread moves only up and down) which Folio's spread
        // layout cannot hold: with the option off the page goes the way the control bar's X / Y always did (`setPageGeometry`, Page.reframe)
        if *objects_move_with_page {
            return Err(EditError::new("unsupported", "Objects Move with Page in a spread of several pages (or with facing pages, right-to-left binding) has not been implemented"));
        }
        let b = bounds(&book.spreads[s].pages[p].xml);
        let m = mat(&book.spreads[s].pages[p].xml);
        let rect = [b[0] + m.0[4] + dx, b[1] + m.0[5] + dy, b[2] + m.0[4] + dx, b[3] + m.0[5] + dy];
        drop(book);
        return set_page_geometry(w, std::slice::from_ref(&page.to_owned()), &rect);
    }
    if dx == 0.0 && dy == 0.0 {
        return Ok(());
    }
    let spread = &mut book.spreads[s];
    let mine = spread.pages[0].id.clone();
    let rect = {
        let b = bounds(&spread.pages[0].xml);
        let m = mat(&spread.pages[0].xml);
        [b[0] + m.0[4], b[1] + m.0[5], b[2] + m.0[4], b[3] + m.0[5]]
    };
    for item in &mut spread.items {
        // on the page: it overlaps it (`Spread::read`), or it is a line (no area) lying across or inside it
        let on_page = item.page.as_deref() == Some(mine.as_str()) || {
            let ix = scan(&item.xml)?;
            spread_bounds(&ix, &item.xml, 0).is_some_and(|b| {
                let (ox, oy) = (b[2].min(rect[2]) - b[0].max(rect[0]), b[3].min(rect[3]) - b[1].max(rect[1]));
                ox >= 0.0 && oy >= 0.0 && (ox > 0.0 || oy > 0.0) && (b[2] - b[0] <= 0.0 || b[3] - b[1] <= 0.0)
            })
        };
        let goes_along = *objects_move_with_page && on_page && get(&item.xml, "Locked") != "true";
        if !goes_along {
            shift(&mut item.xml, -dx, -dy);
        }
    }
    if !*objects_move_with_page {
        // the guides of a spread live inside its page, those that fit to the page and those of the spread alike: a guide is a place against
        // the page, so it moves like an object that stays
        let t = &mut spread.pages[0].xml;
        let ix = scan(t)?;
        let mut edits = Vec::new();
        for g in ix.named(t, 0, "Guide") {
            let Some(at) = ix.attr(t, g, "Location").and_then(|v| v.parse::<f64>().ok()) else { continue };
            let vertical = ix.attr(t, g, "Orientation") == Some("Vertical");
            edits.push(attr_edit(&ix, t, g, "Location", &geom::num(at - if vertical { dx } else { dy })));
        }
        edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        for (r, v) in edits {
            t.replace_range(r, &v);
        }
    }
    book.write(w)?;
    layoutadjust::lay_out_spreads(w)
}
