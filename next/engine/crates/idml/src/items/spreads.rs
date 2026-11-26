//! Reads regular and master spreads with their pages, column grids and guides.

use super::{ItemReader, affine, bounds4, is_item};
use crate::props::{Props, clean_name, is_none_ref, nums};
use crate::warn::Warnings;
use crate::xml::Element;
use folio_model::{Page, Point, Rect, Spread};

impl<'a> ItemReader<'a, '_> {
    /// A spread or master spread (`root` is the part's root element) without its items, and
    /// the item elements to read into `Spread::items` (in z-order).
    pub fn spread(
        &self,
        root: &'a Element<'a>,
        master: bool,
        next_page: &mut usize,
        w: &mut Warnings,
    ) -> Option<(Spread, Vec<&'a Element<'a>>)> {
        let tag = if master { "MasterSpread" } else { "Spread" };
        let s = root.child(tag).unwrap_or(root);
        if s.name != tag {
            w.add(format!("part without <{tag}> skipped"));
            return None;
        }
        let mut spread = Spread {
            id: s.attr("Self").unwrap_or_default().to_owned(),
            name: if master { s.attr("Name").map(|n| clean_name(n).to_owned()) } else { None },
            transform: affine(s.attr("ItemTransform"), w),
            pages: Vec::new(),
            items: Vec::new(),
            guides: Vec::new(),
            hidden: !master && s.attr("SpreadHidden") == Some("true"),
            hide_master_items: !master && s.attr("ShowMasterItems") == Some("false"),
            name_prefix: if master { s.attr("NamePrefix").map(str::to_owned) } else { None },
            base_name: if master { s.attr("BaseName").map(|n| clean_name(n).to_owned()) } else { None },
        };
        let mut items = Vec::new();
        for c in s.elements() {
            match c.name {
                "Page" => {
                    // A page's guides, and the spread's (IDML writes those in the first page).
                    let own = spread.pages.len();
                    for g in c.children_named("Guide") {
                        let spread_guide = match g.attr("PageIndex").and_then(crate::props::num) {
                            Some(i) => i == 0.0,
                            None => g.attr("FitToPage") == Some("false"),
                        };
                        spread.guides.push(guide(g, (!spread_guide).then_some(own), w));
                    }
                    // document pages number from 0, master pages from `MASTER_PAGE_BASE` (the caller's counters)
                    let index = *next_page;
                    *next_page += 1;
                    let bounds = bounds4(c.attr("GeometricBounds")).unwrap_or_else(|| {
                        w.add("page without GeometricBounds");
                        Rect::default()
                    });
                    spread.pages.push(Page {
                        id: c.attr("Self").unwrap_or_default().to_owned(),
                        name: c.attr("Name").map(|n| clean_name(n).to_owned()).unwrap_or_default(),
                        bounds,
                        transform: affine(c.attr("ItemTransform"), w),
                        master: c.attr("AppliedMaster").filter(|m| !is_none_ref(m)).map(str::to_owned),
                        master_transform: affine(c.attr("MasterPageTransform"), w),
                        index,
                        section_marker: String::new(),
                        section_last: String::new(),
                        section_start: 0,
                        // pairs: master item, the page item overriding it ("n": removed)
                        master_overrides: c
                            .attr("OverrideList")
                            .map(|l| l.split_whitespace().step_by(2).filter(|m| !is_none_ref(m)).map(str::to_owned).collect())
                            .unwrap_or_default(),
                        margins: c
                            .child("MarginPreference")
                            .map(|m| ["Top", "Left", "Bottom", "Right"].map(|k| m.attr(k).and_then(crate::props::num).unwrap_or(0.0)))
                            .unwrap_or_default(),
                        columns: c.child("MarginPreference").map(columns).unwrap_or_default(),
                    });
                    if c.elements().any(|x| is_item(x.name)) {
                        w.add("page items inside <Page> ignored");
                    }
                }
                n if is_item(n) => items.push(c),
                // Older IDML writes the spread's guides in the spread itself.
                "Guide" => spread.guides.push(guide(c, None, w)),
                "FlattenerPreference" | "Properties" => {}
                other => w.add(format!("element {other} inside {tag} not read")),
            }
        }
        Some((spread, items))
    }
}

/// The column grid of a `MarginPreference`.
fn columns(m: &Element<'_>) -> folio_model::PageColumns {
    let d = folio_model::PageColumns::default();
    folio_model::PageColumns {
        count: m.f64("ColumnCount").map(|v| v.round().clamp(1.0, 1000.0) as u32).unwrap_or(d.count),
        gutter: m.f64("ColumnGutter").unwrap_or(d.gutter),
        positions: m.get("ColumnsPositions").and_then(|v| v.f64s()).filter(|v| !v.is_empty()),
        direction: m.str("ColumnDirection").map(str::to_owned).unwrap_or(d.direction),
    }
}

/// A `Guide` element; its spread position is set by [`place_guides`].
fn guide(g: &Element<'_>, page: Option<usize>, w: &mut Warnings) -> folio_model::Guide {
    let d = folio_model::Guide::default();
    let location = g.f64("Location").unwrap_or_else(|| {
        w.add("guide without a readable Location (0 used)");
        0.0
    });
    folio_model::Guide {
        id: g.attr("Self").unwrap_or_default().to_owned(),
        horizontal: g.attr("Orientation") != Some("Vertical"),
        location,
        position: location,
        page,
        fit_to_page: g.attr("FitToPage") != Some("false"),
        locked: g.attr("Locked") == Some("true"),
        layer: g.attr("ItemLayer").filter(|l| !is_none_ref(l)).map(str::to_owned),
        color: g.get("GuideColor").and_then(ui_color).unwrap_or(d.color),
        view_threshold: g.f64("ViewThreshold"),
        guide_type: g.attr("GuideType").map(str::to_owned),
    }
}

/// An interface colour property (`LayerColor`, `GuideColor`): an enumeration name or a list
/// of three numbers.
pub fn ui_color(v: crate::props::Val<'_>) -> Option<folio_model::UiColor> {
    match v {
        crate::props::Val::Str(s) => {
            let s = s.trim();
            match nums(s).filter(|n| n.len() == 3) {
                Some(n) => Some(folio_model::UiColor::Rgb([n[0], n[1], n[2]])),
                None => (!s.is_empty()).then(|| folio_model::UiColor::Named(s.to_owned())),
            }
        }
        crate::props::Val::Elem(_) => v.f64s().filter(|n| n.len() == 3).map(|n| folio_model::UiColor::Rgb([n[0], n[1], n[2]])),
    }
}

/// Sets each guide's spread position from its `Location`, which IDML measures from the ruler
/// origin: the top-left corner of the spread's pages (`SpreadOrigin`), the top-left corner of
/// the guide's own page (`PageOrigin`; spread guides as with `SpreadOrigin`), or for a
/// vertical guide on a spread of two pages the left edge of the right page (`SpineOrigin`).
/// Evidence: the INDD reader's guide notes (third_party/indd-utils docs/format/objects.md,
/// "Guides"), which it reproduces for 8,580 of 9,127 IDML guides exactly and the rest to the
/// last digits.
pub fn place_guides(spread: &mut Spread, ruler_origin: &str) {
    let corners: Vec<Point> = spread
        .pages
        .iter()
        .map(|p| {
            let r = p.transform.apply_rect(&p.bounds);
            Point::new(r.left, r.top)
        })
        .collect();
    let spread_origin = corners.iter().copied().reduce(|a, b| Point::new(a.x.min(b.x), a.y.min(b.y))).unwrap_or_default();
    for g in &mut spread.guides {
        let mut origin = spread_origin;
        match ruler_origin {
            "PageOrigin" => {
                if let Some(c) = g.page.and_then(|i| corners.get(i)) {
                    origin = *c;
                }
            }
            "SpineOrigin" if !g.horizontal && corners.len() == 2 => origin.x = corners[0].x.max(corners[1].x),
            _ => {}
        }
        g.position = g.location + if g.horizontal { origin.y } else { origin.x };
    }
}
