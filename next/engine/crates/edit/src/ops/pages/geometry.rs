use super::*;

pub(crate) fn set_page_geometry(
    w: &mut Work<'_>,
    pages: &[String],
    rect: &[f64; 4],
) -> Result<(), EditError> {
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    if rect.iter().any(|v| !v.is_finite())
        || !(1.0..=15552.0).contains(&width)
        || !(1.0..=15552.0).contains(&height)
    {
        return Err(EditError::new(
            "invalid",
            "page dimensions must be 1–15552 pt",
        ));
    }
    let mut book = Book::read(w)?;
    let ix = scan(&book.prefs)?;
    if (0..ix.nodes.len()).any(|i| {
        ix.attr(&book.prefs, i, "EnableLayoutAdjustment") == Some("true")
            || ix.attr(&book.prefs, i, "EnableAdjustLayout") == Some("true")
    }) {
        return Err(EditError::new(
            "unsupported",
            "page geometry with layout adjustment enabled has not been measured",
        ));
    }
    if book.pref("PageBinding") == "RightToLeft" {
        return Err(EditError::new(
            "unsupported",
            "page geometry with right-to-left binding has not been measured",
        ));
    }
    let min_height = book.pref("PageHeight").parse().unwrap_or(0.);
    book.gap = 36. + 2. * book.pasteboard_space();
    let mut seen = BTreeSet::new();
    for id in pages.iter().filter(|id| seen.insert(*id)) {
        let (s, p) = book.locate(id)?;
        let page = &mut book.spreads[s].pages[p];
        let m = mat(&page.xml);
        if m.0[..4] != [1.0, 0.0, 0.0, 1.0] {
            return Err(EditError::new(
                "unsupported",
                "transformed page geometry has not been measured",
            ));
        }
        let b = bounds(&page.xml);
        if [b[0] + m.0[4], b[1] + m.0[5], b[2] + m.0[4], b[3] + m.0[5]] == *rect {
            continue;
        }
        set(
            &mut page.xml,
            "GeometricBounds",
            &[
                rect[1] - m.0[5],
                rect[0] - m.0[4],
                rect[3] - m.0[5],
                rect[2] - m.0[4],
            ]
            .map(geom::num)
            .join(" "),
        );
        update_columns(&mut page.xml, width)?;
        set(&mut page.xml, "UseMasterGrid", "false");
        let spread = &mut book.spreads[s];
        let mut refreshed = Spread::read(spread.part.clone(), &spread.output()?)?;
        for (item, old) in refreshed.items.iter_mut().zip(&spread.items) {
            if refreshed.pages.len() == 1 && item.page.is_none() {
                item.page = old.page.clone();
            }
        }
        *spread = refreshed;
        book.position_with_options(min_height, false);
    }
    book.write(w)
}

pub(super) fn update_columns(page: &mut String, width: f64) -> Result<(), EditError> {
    let ix = scan(page)?;
    if let Some(&i) = ix.named(page, 0, "MarginPreference").first() {
        let get = |k: &str, default: f64| {
            ix.attr(page, i, k)
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        let positions = guides::column_positions(
            width,
            get("Left", 0.0),
            get("Right", 0.0),
            get("ColumnCount", 1.0) as u32,
            get("ColumnGutter", 12.0),
        );
        let (range, value) = attr_edit(&ix, page, i, "ColumnsPositions", &positions);
        page.replace_range(range, &value);
    }
    Ok(())
}
