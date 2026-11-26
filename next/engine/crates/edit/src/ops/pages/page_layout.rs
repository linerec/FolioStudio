//! Position spreads and pages and apply parent layout properties.

use super::*;

impl Book {
    pub(super) fn apply_master(&mut self, id: &str, master: Option<&str>) -> Result<(), EditError> {
        if let Some(m) = master
            && !self.masters.iter().any(|s| s.id() == m)
        {
            return Err(EditError::new("notFound", "parent spread"));
        }
        let (s, p) = self.locate(id)?;
        set(
            &mut self.spreads[s].pages[p].xml,
            "AppliedMaster",
            master.unwrap_or("n"),
        );
        // what the page overrode of the old parent is no longer overridden
        set(&mut self.spreads[s].pages[p].xml, "OverrideList", "");
        self.master_properties(s, p, true);
        Ok(())
    }
    fn master_properties(&mut self, s: usize, p: usize, copy_margins: bool) {
        let page = &mut self.spreads[s].pages[p];
        let b = bounds(&page.xml);
        let pm = mat(&page.xml);
        let left = pm.0[4] + b[0] < -0.001;
        let master = get(&page.xml, "AppliedMaster");
        let mp = self
            .masters
            .iter()
            .find(|s| s.id() == master)
            .and_then(|s| s.pages.get(if left { 0 } else { s.pages.len() - 1 }));
        let mut transform = Mat([1., 0., 0., 1., 0., 0.]);
        if let Some(mp) = mp {
            let mb = bounds(&mp.xml);
            transform.0[4] = b[0] - mb[0];
            transform.0[5] = b[1] - mb[1];
            set(
                &mut page.xml,
                "UseMasterGrid",
                if copy_margins && b == mb {
                    "true"
                } else {
                    "false"
                },
            );
            if copy_margins {
                copy_layout(&mut page.xml, &mp.xml);
            }
        } else {
            // no parent: nothing to follow (oracle: parent-apply-none-after-override)
            set(&mut page.xml, "UseMasterGrid", "false");
        }
        set(&mut page.xml, "MasterPageTransform", &transform.text());
    }
    pub(super) fn shuffle_and_position(&mut self, w: &mut Work<'_>) -> Result<(), EditError> {
        self.shuffle(w)?;
        self.position();
        Ok(())
    }

    /// Lays the spreads one under the other (oracle: pages-hide-spread, spreadrotate). A spread that is turned a quarter
    /// takes the room of its turned shape, and the pasteboard room beside it that was left and right is now above and below:
    /// the horizontal pasteboard margin (-1: one page width, measured on a 300 and a 500 pt wide page).
    pub(super) fn position(&mut self) {
        self.position_with_options(0., true);
    }
    pub(super) fn position_with_options(&mut self, min_height: f64, update_masters: bool) {
        let facing = self.pref("FacingPages") == "true";
        let numbers = self.numbers();
        let space = (self.gap - 36.) / 2.;
        let side = self.pasteboard_side();
        let mut y: f64;
        // the lower edge of the spread above and the room it asks for
        let mut above: Option<(f64, f64)> = None;
        for s in 0..self.spreads.len() {
            let binding = if !facing {
                self.spreads[s].pages[0].binding
            } else if self.spreads[s].allowed() && self.pref("AllowPageShuffle") != "false" {
                if numbers[&self.spreads[s].pages[0].id].is_multiple_of(2) {
                    1
                } else {
                    0
                }
            } else {
                get(&self.spreads[s].xml, "BindingLocation")
                    .parse()
                    .unwrap_or(0)
            };
            set(
                &mut self.spreads[s].xml,
                "BindingLocation",
                &binding.to_string(),
            );
            let max_height = self.spreads[s]
                .pages
                .iter()
                .map(|p| {
                    let r = bounds(&p.xml);
                    r[3] - r[1]
                })
                .fold(min_height, f64::max);
            let widths: Vec<_> = self.spreads[s]
                .pages
                .iter()
                .map(|p| {
                    let b = bounds(&p.xml);
                    b[2] - b[0]
                })
                .collect();
            let left = if facing {
                -widths.iter().take(binding).sum::<f64>()
            } else {
                -widths.iter().sum::<f64>() / 2.
            };
            let right = left + widths.iter().sum::<f64>();
            let turn = self.spreads[s].rotation();
            // the spread's extent up and down from its transform's `ty`, and the pasteboard room it asks for there
            let (low, high, room) = match turn {
                90 => (left, right, side.unwrap_or_else(|| widths.iter().copied().fold(0., f64::max))),
                270 => (-right, -left, side.unwrap_or_else(|| widths.iter().copied().fold(0., f64::max))),
                _ => (-max_height / 2., max_height / 2., space),
            };
            // a hidden spread takes no room: it lies where it would, and the next one lies under the last spread that is
            // shown (oracle: pages-hide-spread, rot-hide)
            y = match above {
                Some((bottom, above_room)) => bottom + 36. + above_room + room - low,
                None => 0.,
            };
            let quarter = match turn {
                90 => [0., 1., -1., 0.],
                180 => [-1., 0., 0., -1.],
                270 => [0., -1., 1., 0.],
                _ => [1., 0., 0., 1.],
            };
            set(
                &mut self.spreads[s].xml,
                "ItemTransform",
                &Mat([quarter[0], quarter[1], quarter[2], quarter[3], 0., y]).text(),
            );
            if !self.spreads[s].hidden() {
                above = Some((y + high, room));
            }
            let mut x = left;
            #[allow(clippy::needless_range_loop, reason = "the spread's page count determines how many pages to shift")]
            for p in 0..self.spreads[s].pages.len() {
                let page = &mut self.spreads[s].pages[p];
                let b = bounds(&page.xml);
                let old = mat(&page.xml);
                let h = b[3] - b[1];
                let dx = x - b[0] - old.0[4];
                let dy = -h / 2. - b[1] - old.0[5];
                let id = page.id.clone();
                shift(&mut page.xml, dx, dy);
                for item in &mut self.spreads[s].items {
                    if item.page.as_deref() == Some(&id) {
                        shift(&mut item.xml, dx, dy)
                    }
                }
                if update_masters && (dx.abs() > 1e-8 || dy.abs() > 1e-8) {
                    self.master_properties(s, p, false)
                }
                x += widths[p];
            }
        }
    }
    /// The horizontal pasteboard margin when the document sets one (`PasteboardMargins` -1 is automatic).
    fn pasteboard_side(&self) -> Option<f64> {
        let ix = Index::scan(&self.prefs).unwrap();
        (0..ix.nodes.len())
            .find(|&i| ix.local(&self.prefs, i) == "PasteboardPreference")
            .and_then(|i| ix.value(&self.prefs, i, "PasteboardMargins"))
            .and_then(|v| v.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()))
            .filter(|&h| h >= 0.)
    }
}

/// A page takes its parent page's margins and layout grid (`MarginPreference`,
/// `GridDataInformation`), as InDesign does when a parent is applied or a parent is based on one.
pub(in crate::ops) fn copy_layout(page: &mut String, parent: &str) {
    let Ok(ix) = Index::scan(parent) else { return };
    for tag in ["MarginPreference", "GridDataInformation"] {
        let Some(&mi) = ix.named(parent, 0, tag).first() else { continue };
        // each replacement scans again: the first changes the second's offsets
        let Ok(q) = Index::scan(page) else { return };
        let Some(&qi) = q.named(page, 0, tag).first() else { continue };
        page.replace_range(q.nodes[qi].start..q.nodes[qi].end, &element(parent, &ix, mi));
    }
}

/// Preferences ▸ Guides & Pasteboard ▸ Pasteboard Options (oracle: pages-pasteboard-prefs): the pasteboard margins
/// (horizontal, vertical) set `PasteboardMargins` and `MinimumSpaceAboveAndBelow` (the vertical one) and turn Match
/// Preview Background to Theme Color off; matching sets both margins to 0. The spreads lie 36 pt plus twice the
/// vertical margin apart, so they move.
pub(in crate::ops) fn set_pasteboard(w: &mut Work<'_>, margins: Option<(f64, f64)>, matching: Option<bool>) -> Result<(), EditError> {
    let mut b = Book::read(w)?;
    let mut edits = Vec::new();
    match (margins, matching) {
        (Some((h, v)), None) => edits.extend([("PasteboardMargins", format!("{} {}", geom::num(h), geom::num(v))), ("MinimumSpaceAboveAndBelow", geom::num(v)), ("MatchPreviewBackgroundToThemeColor", "false".into())]),
        (None, Some(true)) => edits.extend([("PasteboardMargins", "0 0".into()), ("MinimumSpaceAboveAndBelow", "0".into()), ("MatchPreviewBackgroundToThemeColor", "true".into())]),
        (None, Some(false)) => edits.push(("MatchPreviewBackgroundToThemeColor", "false".into())),
        _ => return Err(EditError::new("invalid", "pasteboard: margins or matching")),
    }
    for (k, v) in edits {
        let ix = Index::scan(&b.prefs).map_err(|e| EditError::new("xml", e))?;
        let i = (0..ix.nodes.len()).find(|&i| ix.local(&b.prefs, i) == "PasteboardPreference").ok_or_else(|| EditError::new("xml", "missing PasteboardPreference"))?;
        let (r, v) = attr_edit(&ix, &b.prefs, i, k, &v);
        b.prefs.replace_range(r, &v);
    }
    let space: f64 = b.pasteboard_space();
    b.gap = 36. + 2. * space;
    b.position();
    b.write(w)
}

/// View ▸ Rotate Spread and Pages panel ▸ Page Attributes ▸ Rotate Spread View (oracle: spreadrotate): the actions "90° CW",
/// "90° CCW" and "180°" turn the spread further from the way it lies (two 90° CW make 180°), "Clear Rotation" turns it back.
/// The turn is saved in the spread's `ItemTransform`; the spreads below make room for the turned one ([`Book::position`]).
/// The page items keep their own transforms, and nothing printed or exported to PDF turns.
pub(crate) fn rotate_view(w: &mut Work<'_>, spread: &String, how: &String) -> Result<(), EditError> {
    let mut b = Book::read(w)?;
    if b.pref("PageBinding") == "RightToLeft" {
        return Err(EditError::new("unsupported", "page-panel editing of right-to-left binding has not been measured yet"));
    }
    let by = match how.as_str() {
        "cw" => 90,
        "ccw" => 270,
        "180" => 180,
        "clear" => 0,
        _ => return Err(EditError::new("invalid", "rotate spread view: cw, ccw, 180 or clear")),
    };
    let s = b.spreads.iter_mut().find(|s| s.id() == *spread).ok_or_else(|| EditError::new("notFound", "spread"))?;
    let turn = if how == "clear" { 0 } else { (s.rotation() + by) % 360 };
    if turn == s.rotation() {
        return Ok(());
    }
    let quarter = match turn {
        90 => [0., 1., -1., 0.],
        180 => [-1., 0., 0., -1.],
        270 => [0., -1., 1., 0.],
        _ => [1., 0., 0., 1.],
    };
    let ty = mat(&s.xml).0[5];
    set(&mut s.xml, "ItemTransform", &Mat([quarter[0], quarter[1], quarter[2], quarter[3], 0., ty]).text());
    b.position();
    b.write(w)
}
