//! Flattener Preview (InDesign: Window > Output > Flattener Preview): the page shown ghosted and
//! the objects a Highlight mode selects painted in red over it. Rules measured in InDesign 2025
//! (`next/oracle/rules/windowmenu-c1.ko.md`, `windowmenu-c2.ko.md`).
//!
//! 1. [`survey`] lists the page's top-level items in paint order with the facts the modes need
//!    ([`Facts`]): where they are, whether they are transparent (opacity, blend mode, effects),
//!    whether they hold text, a stroke, a gradient or a placed graphic.
//! 2. [`select`] turns a mode and a flattener preset into a [`Plan`]: which parts of which items
//!    are painted (layers are united; the maps of one layer are intersected).
//! 3. [`coverage`] draws a plan as coverage on the page's pixels and [`compose`] paints the
//!    ghosted page (the composited ink as gray) with the coverage mixed in as red.

use super::{DocInks, InkImages, InkPlanes, draw, part};
use crate::{Scene, effects, item_extent, locate_page};
use folio_model::{Affine, BlendMode, ItemKind, PageItem, Paint, Rect};
use std::collections::HashMap;

/// A top-level item's place in the page's paint order: (layer rank, source, item in the source).
pub type Key = (u32, u32, u32);

/// Every part of an item ([`part`] bits).
pub const ALL: u8 = 0b1_1111;

const fn bit(p: u8) -> u8 {
    1 << p
}

/// What the modes need to know about one top-level item.
#[derive(Clone, Debug)]
pub struct Facts {
    pub key: Key,
    pub id: String,
    /// The item's extent with its stroke, spread coordinates.
    pub ext: Rect,
    /// The extent with the reach of its shadows and glows.
    pub reach: Rect,
    /// Opacity, blend mode or group isolation: the item is composited as a whole.
    pub group: bool,
    /// Parts that carry effects (shadow, glow, feather, inner effects): a [`part`] bit each.
    pub fx: u8,
    /// Parts with their own opacity or blend mode (fill, stroke, content).
    pub blend: u8,
    pub text: bool,
    pub stroke: bool,
    pub gradient: bool,
    pub image: bool,
    /// The item paints something (a fill, a stroke, text, a graphic): empty frames are never affected.
    pub paints: bool,
}

impl Facts {
    pub fn transparent(&self) -> bool {
        self.group || self.fx != 0 || self.blend != 0
    }

    /// The parts that are the transparency: everything for a composited item, else the parts with effects.
    pub fn trans_parts(&self) -> u8 {
        if self.group { ALL } else { self.fx | self.blend }
    }
}

fn is_transparent(item: &PageItem) -> bool {
    item.opacity < 1.0 || item.blend_mode != BlendMode::Normal || (matches!(item.kind, ItemKind::Group { .. }) && (item.print.isolate_blending || item.print.knockout_group))
}

fn level_effects(item: &PageItem) -> (u8, u8) {
    let l = &item.print.level_fx;
    let (mut fx, mut blend) = (0u8, 0u8);
    let all = &item.print.fx;
    if effects::outer(all).next().is_some() {
        fx |= bit(part::BEHIND);
    }
    if all.feather.is_some() || all.directional_feather.is_some() || all.gradient_feather.is_some() {
        fx |= bit(part::FILL);
    }
    if all.inner_shadow.is_some() || all.inner_glow.is_some() || all.bevel.is_some() || all.satin.is_some() {
        fx |= bit(part::INNER);
    }
    for (e, p) in [(&l.fill, part::FILL), (&l.content, part::CONTENT), (&l.stroke, part::STROKE)] {
        if e.any() {
            fx |= bit(p);
        }
    }
    for (b, p) in [(item.print.fill_blend, part::FILL), (item.print.stroke_blend, part::STROKE), (item.print.content_blend, part::CONTENT)] {
        if b.is_some_and(|(a, m)| a < 1.0 || m != BlendMode::Normal) {
            blend |= bit(p);
        }
    }
    (fx, blend)
}

fn has_paint(p: &Paint) -> bool {
    match p {
        Paint::None => false,
        Paint::Swatch { id, .. } => !id.ends_with("/None"),
        _ => true,
    }
}

/// The facts of a top-level item at `key`, `parent` mapping its parent's coordinates to spread coordinates; `texted` tells
/// whether a text frame holds text.
pub(crate) fn facts_of(item: &PageItem, key: Key, parent: &Affine, texted: &dyn Fn(&PageItem) -> bool) -> Facts {
    // what the object covers: its shape and stroke (a text frame: the frame), groups: their members
    let zero = Rect { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };
    let own = match &item.kind {
        ItemKind::Group { .. } => item_extent(item).unwrap_or(zero),
        _ => folio_model::paths_bounds(&item.paths).map_or(zero, |b| {
            let m = item.stroke.weight / 2.0;
            Rect { left: b.left - m, top: b.top - m, right: b.right + m, bottom: b.bottom + m }
        }),
    };
    let mut reach = own;
    let l = &item.print.level_fx;
    for fx in [&item.print.fx, &l.fill, &l.content, &l.stroke] {
        for (kind, s) in effects::outer(fx) {
            reach = reach.union(&effects::shadow_rect(&own, s, kind));
        }
    }
    let (fx, blend) = level_effects(item);
    let mut f = Facts {
        key,
        id: item.id.clone(),
        ext: parent.apply_rect(&own),
        reach: parent.apply_rect(&reach),
        group: is_transparent(item),
        fx,
        blend,
        text: item.text_frame().is_some() && texted(item),
        stroke: item.stroke.weight > 0.0 && has_paint(&item.stroke.paint),
        gradient: matches!(item.fill, Paint::Gradient { .. }),
        image: matches!(item.kind, ItemKind::Rectangle { content: Some(_) } | ItemKind::Oval { content: Some(_) } | ItemKind::Polygon { content: Some(_) }),
        paints: false,
    };
    f.paints = has_paint(&item.fill) || f.stroke || f.text || f.image;
    if let ItemKind::Group { children } = &item.kind {
        // a group stands for its members: any transparency inside makes the whole group transparent
        for c in children {
            let cf = facts_of(c, key, &Affine::IDENTITY, texted);
            f.paints |= cf.paints;
            f.group |= cf.transparent();
            f.text |= cf.text;
            f.stroke |= cf.stroke;
            f.gradient |= cf.gradient;
            f.image |= cf.image;
        }
    }
    f
}

/// The page's top-level items in paint order.
pub fn survey(scene: &Scene, images: &InkImages, inks: &DocInks, index: usize) -> Option<Vec<Facts>> {
    draw::survey_area(scene, images, inks, index)
}

/// What Highlight shows (the panel's list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    None,
    ComplexRegions,
    Transparent,
    Affected,
    AffectedGraphics,
    OutlinedStrokes,
    OutlinedText,
    RasterFill,
    Rasterized,
}

impl Highlight {
    pub fn parse(s: &str) -> Option<Highlight> {
        Some(match s {
            "none" => Highlight::None,
            "complexRegions" => Highlight::ComplexRegions,
            "transparent" => Highlight::Transparent,
            "affected" => Highlight::Affected,
            "affectedGraphics" => Highlight::AffectedGraphics,
            "outlinedStrokes" => Highlight::OutlinedStrokes,
            "outlinedText" => Highlight::OutlinedText,
            "rasterFill" => Highlight::RasterFill,
            "rasterized" => Highlight::Rasterized,
            _ => return None,
        })
    }
}

/// The flattener settings the preview follows (a transparency flattener preset's values).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    /// Raster/vector balance, percent (does not change what the modes select on the measured page).
    pub balance: f64,
    pub strokes: bool,
    pub text: bool,
    pub clip: bool,
}

impl Preset {
    /// `[High Resolution]`.
    pub const HIGH: Preset = Preset { balance: 100.0, strokes: false, text: false, clip: true };
}

/// One piece of what is painted: the coverage of the `all` maps multiplied, made hard (any coverage counts as full) when
/// `hard` (a blurred shadow is highlighted as the whole raster it needs), minus what the `minus` maps cover (the object's own body).
#[derive(Clone, Debug, Default)]
pub struct Layer {
    pub all: Vec<HashMap<Key, u8>>,
    pub minus: Vec<HashMap<Key, u8>>,
    pub hard: bool,
}

/// What is painted: the layers' coverages are united.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub layers: Vec<Layer>,
}

fn add(map: &mut HashMap<Key, u8>, key: Key, parts: u8) {
    *map.entry(key).or_default() |= parts;
}

/// The items and parts the mode paints.
pub fn select(facts: &[Facts], h: Highlight, p: &Preset) -> Plan {
    let mut plan = Plan::default();
    if matches!(h, Highlight::None | Highlight::ComplexRegions | Highlight::RasterFill) {
        return plan;
    }
    let trans: Vec<usize> = facts.iter().enumerate().filter(|(_, f)| f.transparent()).map(|(i, _)| i).collect();
    // an object under a transparent object that overlaps it is affected as a whole (measured with every built-in
    // preset: gradients and placed graphics under transparency too)
    let affected: Vec<bool> = facts
        .iter()
        .enumerate()
        .map(|(i, f)| f.transparent() || (f.paints && trans.iter().any(|&t| t > i && facts[t].reach.intersects(&f.ext))))
        .collect();
    let overlaps_trans = |f: &Facts| trans.iter().any(|&t| facts[t].reach.intersects(&f.ext));
    let mut one = HashMap::new();
    match h {
        Highlight::Transparent => {
            for &t in &trans {
                add_transparent(&mut plan, &mut one, &facts[t]);
            }
        }
        Highlight::Affected => {
            for (i, f) in facts.iter().enumerate() {
                if f.transparent() {
                    add_transparent(&mut plan, &mut one, f);
                } else if affected[i] {
                    add(&mut one, f.key, ALL);
                }
            }
        }
        Highlight::AffectedGraphics => {
            for (i, f) in facts.iter().enumerate() {
                if affected[i] && f.image {
                    if f.transparent() {
                        add_transparent(&mut plan, &mut one, f);
                    } else {
                        add(&mut one, f.key, ALL);
                    }
                }
            }
        }
        Highlight::OutlinedStrokes => {
            // the strokes of affected objects; with "convert all strokes to outlines" every stroke the transparency reaches
            for (i, f) in facts.iter().enumerate() {
                if f.stroke && !f.transparent() && (affected[i] || (p.strokes && overlaps_trans(f))) {
                    add(&mut one, f.key, bit(part::STROKE));
                }
            }
        }
        Highlight::OutlinedText => {
            if p.text {
                for (i, f) in facts.iter().enumerate() {
                    if f.text && !f.transparent() && (affected[i] || overlaps_trans(f)) {
                        add(&mut one, f.key, bit(part::CONTENT));
                    }
                }
            }
        }
        Highlight::Rasterized => {
            // shadows, glows, feathers and inner effects are always raster
            for f in facts.iter().filter(|f| f.fx != 0) {
                add_effects(&mut plan, &mut one, f);
            }
            // a placed graphic under transparency is raster where the two overlap
            for &t in &trans {
                for f in facts.iter().take(t).filter(|f| f.image && !f.transparent()) {
                    if facts[t].reach.intersects(&f.ext) {
                        let mut a = HashMap::new();
                        add(&mut a, facts[t].key, facts[t].trans_parts());
                        let mut b = HashMap::new();
                        add(&mut b, f.key, ALL);
                        plan.layers.push(Layer { all: vec![a, b], ..Layer::default() });
                    }
                }
            }
        }
        Highlight::None | Highlight::ComplexRegions | Highlight::RasterFill => {}
    }
    if !one.is_empty() {
        plan.layers.insert(0, Layer { all: vec![one], ..Layer::default() });
    }
    plan
}

/// The part of a transparent item that is the transparency: the whole of a composited item, else its effects and the parts
/// with their own opacity or blend mode.
fn add_transparent(plan: &mut Plan, one: &mut HashMap<Key, u8>, f: &Facts) {
    if f.group {
        add(one, f.key, ALL);
        return;
    }
    add_effects(plan, one, f);
    if f.blend != 0 {
        add(one, f.key, f.blend);
    }
}

/// The effects of an item: shadows and glows behind it are painted where they show (a hard-edged region without the item's
/// own body), the other effects as the parts they act on.
fn add_effects(plan: &mut Plan, one: &mut HashMap<Key, u8>, f: &Facts) {
    let behind = f.fx & bit(part::BEHIND);
    if behind != 0 {
        let mut all = HashMap::new();
        add(&mut all, f.key, behind);
        let mut minus = HashMap::new();
        add(&mut minus, f.key, bit(part::FILL) | bit(part::CONTENT) | bit(part::STROKE));
        plan.layers.push(Layer { all: vec![all], minus: if f.group { Vec::new() } else { vec![minus] }, hard: true });
    }
    if f.fx & !behind != 0 {
        add(one, f.key, f.fx & !behind);
    }
}

/// The plan as coverage over the area of page `index` drawn at `scale`: `width × height`, 0–255.
// Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
#[allow(clippy::too_many_arguments)]
pub fn coverage(scene: &Scene, images: &InkImages, inks: &DocInks, index: usize, scale: f64, area: &Rect, width: u32, height: u32, plan: &Plan) -> Option<Vec<u8>> {
    let (_, _, bbox) = locate_page(scene.doc, index)?;
    let base = Affine::scale(scale, scale).then_inner(&Affine::translate(-(bbox.left + area.left), -(bbox.top + area.top)));
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; w * h];
    let cover = |maps: &[HashMap<Key, u8>]| -> Option<Option<Vec<u8>>> {
        let mut cov: Option<Vec<u8>> = None;
        for sel in maps {
            let c = draw::coverage_area(scene, images, inks, index, &base, w, h, sel)?;
            cov = Some(match cov {
                None => c,
                Some(mut a) => {
                    for (x, y) in a.iter_mut().zip(&c) {
                        *x = ((*x as u32 * *y as u32 + 127) / 255) as u8;
                    }
                    a
                }
            });
        }
        Some(cov)
    };
    for layer in &plan.layers {
        let Some(mut c) = cover(&layer.all)? else { continue };
        if layer.hard {
            for v in c.iter_mut() {
                *v = if *v > 0 { 255 } else { 0 };
            }
        }
        if let Some(m) = cover(&layer.minus)? {
            for (x, y) in c.iter_mut().zip(&m) {
                *x = ((*x as u32 * (255 - *y as u32) + 127) / 255) as u8;
            }
        }
        for (o, v) in out.iter_mut().zip(&c) {
            *o = (*o).max(*v);
        }
    }
    Some(out)
}

/// The ghosted page: the composited ink as gray, `g = 255 (1 - 0.5 min(1, 0.30 C + 0.59 M + 0.11 Y + K))`, and where
/// `mask` is set the mix of that gray and pure red, `(0.5 g + 127.5, 0.5 g, 0.5 g)`. RGBA.
pub fn compose(planes: &InkPlanes, inks: &DocInks, mask: Option<&[u8]>) -> Vec<u8> {
    let (w, h) = (planes.width as usize, planes.height as usize);
    let mut out = vec![255u8; w * h * 4];
    // weight of each plate in the gray (spot inks through their alternate CMYK)
    let weights: Vec<f64> = inks.inks.iter().map(|k| 0.30 * k.cmyk[0] as f64 + 0.59 * k.cmyk[1] as f64 + 0.11 * k.cmyk[2] as f64 + k.cmyk[3] as f64).collect();
    for i in 0..w * h {
        let mut cov = 0.0;
        for (p, wt) in weights.iter().enumerate().take(planes.count) {
            let v = planes.data[p * w * h + i];
            if v != 0 {
                cov += v as f64 / 255.0 * wt;
            }
        }
        let g = 255.0 * (1.0 - 0.5 * cov.min(1.0));
        let m = mask.map_or(0.0, |m| m[i] as f64 / 255.0);
        let (r, gb) = (g + m * (0.5 * g + 127.5 - g), g + m * (0.5 * g - g));
        out[i * 4] = r.round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 1] = gb.round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 2] = gb.round().clamp(0.0, 255.0) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(key: u32, ext: [f64; 4], group: bool) -> Facts {
        let r = Rect { left: ext[0], top: ext[1], right: ext[2], bottom: ext[3] };
        Facts { key: (0, 0, key), id: format!("i{key}"), ext: r, reach: r, group, fx: 0, blend: 0, text: false, stroke: false, gradient: false, image: false, paints: true }
    }

    #[test]
    fn affected_is_below_and_overlapping() {
        let f = vec![facts(0, [0.0, 0.0, 10.0, 10.0], false), facts(1, [20.0, 0.0, 30.0, 10.0], false), facts(2, [5.0, 0.0, 25.0, 10.0], true), facts(3, [0.0, 0.0, 10.0, 10.0], false)];
        let plan = select(&f, Highlight::Affected, &Preset::HIGH);
        let keys: Vec<u32> = { let mut v: Vec<u32> = plan.layers[0].all[0].keys().map(|k| k.2).collect(); v.sort(); v };
        // 0 and 1 lie under the transparent item 2 and overlap it; 3 is above it
        assert_eq!(keys, vec![0, 1, 2]);
    }

    #[test]
    fn graphics_under_transparency_are_affected_and_raster_where_they_overlap() {
        let mut img = facts(0, [0.0, 0.0, 10.0, 10.0], false);
        img.image = true;
        let f = vec![img, facts(1, [5.0, 5.0, 15.0, 15.0], true)];
        for preset in [Preset::HIGH, Preset { balance: 75.0, strokes: true, text: false, clip: false }] {
            assert_eq!(select(&f, Highlight::AffectedGraphics, &preset).layers[0].all[0].len(), 1);
            let r = select(&f, Highlight::Rasterized, &preset);
            assert_eq!(r.layers.len(), 1);
            assert_eq!(r.layers[0].all.len(), 2);
        }
    }
}
