//! Fill, stroke, corner geometry and text wrap on page items.

use super::{ItemReader, point};
use crate::props::is_none_ref;
use crate::strokes::StrokeStyles;
use crate::styles::{Cat, ObjectProps};
use crate::warn::Warnings;
use folio_model::geom::{self, Arrowhead, CornerKind, CornerOption, LineEnds, StrokePattern, StrokeSpec};
use folio_model::{GradientGeometry, PageItem, Paint, Stroke, StrokeAlignment, TextWrap, TextWrapMode, TextWrapSide};

impl<'a> ItemReader<'a, '_> {
    pub(super) fn paint(&self, o: &ObjectProps<'a, '_>, item: &mut PageItem, w: &mut Warnings) {
        item.fill = self.colors.paint(o.str(&[], "FillColor", Cat::Fill), o.f64(&[], "FillTint", Cat::Fill), w);
        let paint =
            self.colors.paint(o.str(&[], "StrokeColor", Cat::Stroke), o.f64(&[], "StrokeTint", Cat::Stroke), w);
        let weight = o.f64(&[], "StrokeWeight", Cat::Stroke).unwrap_or(1.0).max(0.0);
        let alignment = match o.str(&[], "StrokeAlignment", Cat::StrokeCorner) {
            Some("InsideAlignment") => StrokeAlignment::Inside,
            Some("OutsideAlignment") => StrokeAlignment::Outside,
            _ => StrokeAlignment::Center,
        };
        if matches!(item.fill, Paint::Gradient { .. }) {
            item.fill_gradient = Some(gradient_geometry(o, "Fill", Cat::Fill));
        }
        if matches!(paint, Paint::Gradient { .. }) {
            item.stroke_gradient = Some(gradient_geometry(o, "Stroke", Cat::Stroke));
        }
        let join = match o.str(&[], "EndJoin", Cat::StrokeCorner) {
            Some("RoundEndJoin") => folio_model::LineJoin::Round,
            Some("BevelEndJoin") => folio_model::LineJoin::Bevel,
            _ => folio_model::LineJoin::Miter,
        };
        let cap = match o.str(&[], "EndCap", Cat::StrokeCorner) {
            Some("RoundEndCap") => folio_model::LineCap::Round,
            Some("ProjectingEndCap") => folio_model::LineCap::Projecting,
            _ => folio_model::LineCap::Butt,
        };
        let miter_limit = o.f64(&[], "MiterLimit", Cat::StrokeCorner).filter(|m| *m >= 1.0);
        // Corner options change the item's shape: fill, clip, wrap and stroke follow it.
        let corners = corner_options(o);
        if corners.iter().any(CornerOption::active) {
            let in_path = path_order(corners);
            for sp in &mut item.paths {
                *sp = geom::apply_corners(sp, &in_path);
            }
        }
        let style = o.str(&[], "StrokeType", Cat::Stroke).filter(|t| !is_none_ref(t));
        let gap = match o.str(&[], "GapColor", Cat::Stroke) {
            Some(c) if !is_none_ref(c) => self.colors.paint(Some(c), o.f64(&[], "GapTint", Cat::Stroke).filter(|t| *t >= 0.0), w),
            _ => Paint::None,
        };
        let mut end = |k: &str| match o.str(&[], k, Cat::StrokeCorner) {
            Some(v) => Arrowhead::from_idml(v).unwrap_or_else(|| {
                w.add(format!("arrowhead {v} not known (left out)"));
                Arrowhead::None
            }),
            None => Arrowhead::None,
        };
        let ends = LineEnds {
            start: end("LeftLineEnd"),
            end: end("RightLineEnd"),
            start_scale: o.f64(&[], "LeftArrowHeadScale", Cat::StrokeCorner).filter(|v| *v > 0.0).unwrap_or(100.0),
            end_scale: o.f64(&[], "RightArrowHeadScale", Cat::StrokeCorner).filter(|v| *v > 0.0).unwrap_or(100.0),
            outside: o.str(&[], "ArrowHeadAlignment", Cat::StrokeCorner) == Some("OutsidePath"),
        };
        let mut stroke = Stroke {
            paint,
            weight,
            alignment,
            join,
            cap,
            miter_limit,
            style: style.filter(|t| StrokeStyles::name(t) != "Solid").map(str::to_owned),
            ends,
            gap,
            art: None,
            corners,
        };
        if weight > 0.0 && (stroke.paint != Paint::None || stroke.gap != Paint::None) {
            let dash_and_gap = o.get(&[], "StrokeDashAndGap", Cat::Stroke).and_then(|v| v.f64s());
            let adjust = crate::strokes::corner_adjust(o.str(&[], "StrokeCornerAdjustment", Cat::Stroke));
            let pattern = match style {
                None => StrokePattern::Solid,
                Some(t) => self.strokes.pattern(t, weight, dash_and_gap.as_deref(), adjust).unwrap_or_else(|| {
                    w.add(format!("stroke style {} not defined (drawn solid)", StrokeStyles::name(t)));
                    StrokePattern::Solid
                }),
            };
            let spec = StrokeSpec {
                weight,
                cap,
                join,
                miter_limit: miter_limit.unwrap_or(4.0),
                alignment,
                pattern,
                ends,
                gap: stroke.gap != Paint::None,
                whole_waves: false,
            };
            stroke.art = geom::stroke_art(&item.paths, &spec);
        }
        item.print.overprint_gap = o.bool(&[], "OverprintGap", Cat::Stroke).unwrap_or(false);
        item.stroke = stroke;
    }

    pub(super) fn wrap(&self, o: &ObjectProps<'a, '_>, item: &mut PageItem) {
        let p = ["TextWrapPreference"];
        let mode = match o.str(&p, "TextWrapMode", Cat::Wrap) {
            Some("BoundingBoxTextWrap") => TextWrapMode::BoundingBox,
            Some("Contour") => TextWrapMode::Contour,
            Some("JumpObjectTextWrap") => TextWrapMode::JumpObject,
            Some("NextColumnTextWrap") => TextWrapMode::NextColumn,
            _ => return,
        };
        let off = ["TextWrapPreference", "Properties", "TextWrapOffset"];
        let offsets = ["Top", "Left", "Bottom", "Right"].map(|k| o.f64(&off, k, Cat::Wrap).unwrap_or(0.0));
        let side = match o.str(&p, "TextWrapSide", Cat::Wrap) {
            Some("LeftSide") => TextWrapSide::LeftSide,
            Some("RightSide") => TextWrapSide::RightSide,
            Some("SideTowardsSpine") => TextWrapSide::SideTowardsSpine,
            Some("SideAwayFromSpine") => TextWrapSide::SideAwayFromSpine,
            Some("LargestArea") => TextWrapSide::LargestArea,
            _ => TextWrapSide::BothSides,
        };
        item.wraps = true;
        item.text_wrap = Some(TextWrap {
            mode,
            offsets,
            side,
            inverse: o.bool(&p, "Inverse", Cat::Wrap).unwrap_or(false),
            master_only: o.bool(&p, "ApplyToMasterPageOnly", Cat::Wrap).unwrap_or(false),
        });
    }
}

fn gradient_geometry(o: &ObjectProps<'_, '_>, prefix: &str, cat: Cat) -> GradientGeometry {
    let key = |k: &str| format!("Gradient{prefix}{k}");
    GradientGeometry {
        start: o.str(&[], &key("Start"), cat).and_then(|s| point(Some(s))).unwrap_or_default(),
        length: o.f64(&[], &key("Length"), cat).unwrap_or(0.0),
        angle: o.f64(&[], &key("Angle"), cat).unwrap_or(0.0),
        hilite_length: o.f64(&[], &key("HiliteLength"), cat).unwrap_or(0.0),
        hilite_angle: o.f64(&[], &key("HiliteAngle"), cat).unwrap_or(0.0),
    }
}

/// Corner options as InDesign names them: top left, top right, bottom left, bottom right.
/// InDesign 2025 writes `<Corner>CornerOption` only for a corner with a shape and
/// `<Corner>CornerRadius` only for a size other than 12, so with any of those an absent one is
/// None / 12; the item's `CornerOption` / `CornerRadius` then only repeat the top left's
/// (next/oracle/scenarios/basic-corners.json: corner-one, corner-br-only, corner-one-off).
/// Without any (IDML before per-corner options) they apply to all four corners.
fn corner_options(o: &ObjectProps<'_, '_>) -> [CornerOption; 4] {
    let kind = |v: Option<&str>| match v {
        Some("RoundedCorner") => Some(CornerKind::Rounded),
        Some("InverseRoundedCorner") => Some(CornerKind::InverseRounded),
        Some("InsetCorner") => Some(CornerKind::Inset),
        Some("BevelCorner") => Some(CornerKind::Bevel),
        Some("FancyCorner") => Some(CornerKind::Fancy),
        Some("None") => Some(CornerKind::None),
        _ => None,
    };
    const NAMES: [&str; 4] = ["TopLeft", "TopRight", "BottomLeft", "BottomRight"];
    let per = |c: &str, k: &str| o.str(&[], &format!("{c}Corner{k}"), Cat::StrokeCorner);
    let per_corner = NAMES.iter().any(|c| per(c, "Option").is_some() || per(c, "Radius").is_some());
    let (all_kind, all_radius) = if per_corner {
        (CornerKind::None, 12.0)
    } else {
        (kind(o.str(&[], "CornerOption", Cat::StrokeCorner)).unwrap_or(CornerKind::None), o.f64(&[], "CornerRadius", Cat::StrokeCorner).unwrap_or(12.0))
    };
    NAMES.map(|c| CornerOption {
        kind: kind(per(c, "Option")).unwrap_or(all_kind),
        radius: o.f64(&[], &format!("{c}CornerRadius"), Cat::StrokeCorner).unwrap_or(all_radius).max(0.0),
    })
}

/// [`corner_options`] in a rectangle's IDML point order: top left, bottom left, bottom right,
/// top right.
fn path_order([tl, tr, bl, br]: [CornerOption; 4]) -> [CornerOption; 4] {
    [tl, bl, br, tr]
}
