//! Items anchored in text that sit outside their line: above-line objects and custom
//! positions (IDML `AnchoredObjectSetting`, `folio_model::AnchorSettings`).
//!
//! Rules measured on InDesign 2025 (20.5.5) specimens with InDesign's own bounds of every
//! object (artifacts/next-20261018/content-drops/anchors):
//!
//! - An above-line object takes `space above + height + space after` more room before its
//!   line; its top is where the line's em box top would have been plus the space above, and
//!   the line's em box top sits the space after below it. A first line in a frame starts
//!   a leading below the frame top instead of its first-baseline offset. Horizontally it
//!   aligns to the column (paragraph indents do not apply), the offset moving it outward.
//! - A custom object puts its anchor point at the reference: horizontally the anchor
//!   marker, the column, the frame, the page margins or the page edge, aligned left,
//!   centre or right, the X offset moving it away from the aligned edge (right when
//!   centred); vertically a height of the anchor's line (baseline, cap height, x-height,
//!   top of leading, em box top, middle or bottom) or an area aligned top, centre or bottom,
//!   the Y offset moving it down. "Keep within column" keeps a line-relative object
//!   between the column's top and bottom. Relative to the spine, a right-hand page swaps
//!   left and right.
//! - The em box of a Roman font is centred on half its cap height (`FaceMetrics::embox_top`).

use super::{Column, FlowFrame, LineMetrics};
use folio_compose::Alignment;
use folio_model::layout::PlacedObject;
use folio_model::{AnchorAlign, AnchorPosition, AnchorReference, AnchorSettings, PageItem, Rect};

pub(super) fn bounds(item:&PageItem)->Option<Rect> {
    // A polygon stroked with mitre joins reaches as far as its mitre tips, not only half the weight (outlines-c3: the text
    // after an outlined, stroked range starts at the right edge of that box). Only for a path that is not turned or skewed.
    let t=&item.transform;
    if matches!(item.kind,folio_model::ItemKind::Polygon{..}) && item.stroke.weight>0.0 && !matches!(item.stroke.paint,folio_model::Paint::None)
        && item.stroke.alignment==folio_model::StrokeAlignment::Center && item.stroke.join==folio_model::LineJoin::Miter
        && t.b.abs()<1e-9 && t.c.abs()<1e-9 && !item.paths.is_empty() && item.paths.iter().all(|p|!p.open)
        && let Some(inner)=folio_model::strokebounds::stroke_bounds(&item.paths,item.stroke.weight,item.stroke.miter_limit.unwrap_or(4.0)) {
        return Some(t.apply_rect(&inner));
    }
    // An inline group takes half a point more on every side, in its line's height and its advance (InDesign 2025, library item placed in
    // text: bookx-library-text/bookx-lib-text-group, 1 pt higher and 1 pt wider than the group's visible bounds).
    let mut b=super::item_bounds(item)?; let d=if matches!(item.kind,folio_model::ItemKind::Group{..}) {0.5} else {super::stroke_outset(item)};
    b.left-=d; b.top-=d; b.right+=d; b.bottom+=d; Some(b)
}

/// Width an anchored item takes in its line: an inline item its width, an above-line or
/// custom-positioned item none.
pub(super) fn object_advance(item: &PageItem) -> f64 {
    match item.anchor.map(|a| a.position) {
        Some(AnchorPosition::AboveLine | AnchorPosition::Custom) => 0.0,
        _ => bounds(item).map_or(0.0, |b| b.width().max(0.0)),
    }
}

/// The anchor height of the run: the line's font metrics the references measure.
#[derive(Clone, Copy, Debug)]
pub(super) struct AnchorFont {
    pub size: f64,
    pub cap: f64,
    pub x_height: f64,
    /// Em box top above the baseline (points).
    pub embox_top: f64,
}

/// Room before a line for its above-line objects, and how high inline objects reach
/// above the baseline: added to the line's metrics.
pub(super) fn line_room(item: &PageItem, rise: f64, auto_leading: Option<(f64, f64)>, m: &mut LineMetrics) {
    let Some(b) = bounds(item) else { return };
    let a = item.anchor.unwrap_or_default();
    match a.position {
        AnchorPosition::Inline => {
            // The item's bottom is on the baseline; the Y offset moves it up.
            // The object's own baseline shift does not count (outlines-c3, InDesign 2025: raising the object character by
            // 5, 10 or 20 pt leaves the line's baseline where it was; sh*-l* cases of measure/sh.jsonl).
            let _ = rise;
            let above = a.y_offset + b.height();
            m.objects_above = m.objects_above.max(above);
            // Auto leading counts it as a character as high as it reaches: its height
            // and the run's auto-leading share of the point size (InDesign 2025: a 17 pt
            // high object in 10 pt text with 12 pt auto leading, 19 pt).
            if let Some((pct, size)) = auto_leading {
                m.leading = m.leading.max(above + (pct / 100.0 - 1.0) * size);
            }
        }
        AnchorPosition::AboveLine => m.above_line += a.space_above + b.height() + a.y_offset,
        AnchorPosition::Custom => {}
    }
}

/// Alignment of the paragraph, for `TextAlign`.
fn text_align(alignment: Alignment) -> AnchorAlign {
    match alignment {
        Alignment::Center | Alignment::CenterJustified => AnchorAlign::Center,
        Alignment::Right | Alignment::RightJustified => AnchorAlign::End,
        _ => AnchorAlign::Start,
    }
}

/// A horizontal or vertical alignment to `lo..hi`, offset outward (`End`: + offset,
/// `Start`: − offset for x, + offset for y).
fn aligned(align: AnchorAlign, lo: f64, hi: f64, offset: f64, x: bool) -> f64 {
    match align {
        AnchorAlign::End => hi + offset,
        AnchorAlign::Center => (lo + hi) / 2.0 + offset,
        _ if x => lo - offset,
        _ => lo + offset,
    }
}

fn mirror(a: AnchorAlign) -> AnchorAlign {
    match a {
        AnchorAlign::Start => AnchorAlign::End,
        AnchorAlign::End => AnchorAlign::Start,
        other => other,
    }
}

/// Places an above-line object: `cursor` is the top of the next object's room (start: the
/// line's em box top less its above-line room), advanced past this one.
pub(super) fn place_above_line(
    item: &PageItem,
    cursor: &mut f64,
    col: &Column,
    alignment: Alignment,
    verso: bool,
) -> Option<PlacedObject> {
    let b = bounds(item)?;
    let a = item.anchor.unwrap_or_default();
    *cursor += a.space_above;
    let top = *cursor;
    *cursor += b.height() + a.y_offset;
    let mut align = match a.horizontal_alignment {
        AnchorAlign::TextAlign => text_align(alignment),
        other => other,
    };
    if a.spine_relative && !verso {
        align = mirror(align);
    }
    let left = match align {
        AnchorAlign::End => col.right + a.x_offset - b.width(),
        AnchorAlign::Center => (col.left + col.right) / 2.0 + a.x_offset - b.width() / 2.0,
        _ => col.left - a.x_offset,
    };
    Some(PlacedObject { item: item.id.clone(), x: left - b.left, y: top - b.top, fixed_x: true, fixed_y: false })
}

/// Places a custom-positioned object whose marker is at `x` on a line at `baseline`.
#[allow(clippy::too_many_arguments)]
pub(super) fn place_custom(
    item: &PageItem,
    x: f64,
    baseline: f64,
    font: &AnchorFont,
    m: &LineMetrics,
    col: &Column,
    frame: &FlowFrame<'_>,
    alignment: Alignment,
) -> Option<PlacedObject> {
    let b = bounds(item)?;
    let a: AnchorSettings = item.anchor.unwrap_or_default();
    let swap = a.spine_relative && frame.facing && !frame.verso;
    let page = frame.page.map(|(r, _)| r);
    let margins = frame.page.map(|(r, [top, left, bottom, right])| {
        let (left, right) = if frame.facing && frame.verso {
            (right, left)
        } else {
            (left, right)
        };
        Rect {
            left: r.left + left,
            top: r.top + top,
            right: r.right - right,
            bottom: r.bottom - bottom,
        }
    });
    let area = |r: AnchorReference| -> Rect {
        match r {
            AnchorReference::ColumnEdge => Rect { left: col.left, top: col.top, right: col.right, bottom: col.bottom },
            AnchorReference::PageEdge => page.unwrap_or(frame.area),
            AnchorReference::PageMargins => margins.unwrap_or(frame.area),
            _ => frame.area,
        }
    };
    // Horizontal.
    let (lo, hi) = match a.horizontal_reference {
        AnchorReference::AnchorLocation => (x, x),
        r => {
            let r = area(r);
            (r.left, r.right)
        }
    };
    let mut halign = match a.horizontal_alignment {
        AnchorAlign::TextAlign => text_align(alignment),
        other => other,
    };
    let mut px = a.point.0;
    if swap {
        halign = mirror(halign);
        px = 1.0 - px;
    }
    let tx = aligned(halign, lo, hi, a.x_offset, true);
    // Vertical.
    let line = |y: f64| y + a.y_offset;
    let (ty, line_relative) = match a.vertical_reference {
        AnchorReference::LineBaseline => (line(baseline), true),
        AnchorReference::Capheight => (line(baseline - font.cap), true),
        AnchorReference::LineXheight => (line(baseline - font.x_height), true),
        AnchorReference::TopOfLeading => (line(baseline - m.leading), true),
        AnchorReference::EmboxTop => (line(baseline - font.embox_top), true),
        AnchorReference::EmboxMiddle => (line(baseline - font.embox_top + font.size / 2.0), true),
        AnchorReference::EmboxBottom => (line(baseline - font.embox_top + font.size), true),
        r => {
            let r = area(r);
            let valign = match a.vertical_alignment {
                AnchorAlign::TextAlign => AnchorAlign::Start,
                other => other,
            };
            (aligned(valign, r.top, r.bottom, a.y_offset, false), false)
        }
    };
    let left = tx - px * b.width();
    let mut top = ty - a.point.1 * b.height();
    if a.pin && line_relative {
        top = top.min(col.bottom - b.height()).max(col.top);
    }
    Some(PlacedObject {
        item: item.id.clone(),
        x: left - b.left,
        y: top - b.top,
        fixed_x: a.horizontal_reference != AnchorReference::AnchorLocation,
        fixed_y: !line_relative,
    })
}
