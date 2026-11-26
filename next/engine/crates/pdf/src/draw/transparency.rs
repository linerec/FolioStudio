//! Writes transparency groups and effects and records sites for flattening.

use super::{Drawer, Source, grow};
use crate::content::Stream;
use crate::flatten::R;
use crate::registry::{FormDef, GroupDef, GsDef};
use folio_model::layout::FrameText;
use folio_model::{Affine, BlendMode, Graphic, ItemKind, ObjectEffects, PageItem, Rect};
use folio_render::effects::Part;
use folio_render::sep::part;

impl<'a, 'd> Drawer<'a, 'd> {

    pub(super) fn unit_begin(&mut self, s: &mut Stream, part: u8) {
        if let Some(f) = self.flat.as_mut() {
            f.begin(s, part);
        }
    }

    pub(super) fn unit_end(&mut self, s: &mut Stream) {
        if let Some(f) = self.flat.as_mut() {
            f.end(s);
        }
    }

    /// In flattening, records a transparent construct over `local` (current user space) and
    /// returns true: the caller leaves it out.
    pub(super) fn flat_site(&mut self, s: &Stream, local: &Rect, kind: &'static str) -> bool {
        match self.flat.as_mut() {
            Some(f) => {
                f.site(s, local, kind);
                true
            }
            None => false,
        }
    }

    /// Like [`Self::flat_site`] with a rectangle already in page space.
    pub(super) fn flat_site_page(&mut self, r: Option<R>, kind: &'static str) -> bool {
        match self.flat.as_mut() {
            Some(f) => {
                if let Some(r) = r {
                    let key = f.open_key().unwrap_or((f.cur.0, f.cur.1, f.cur.2, 0));
                    f.sites.push(crate::flatten::Site { key, r });
                    *f.kinds.entry(kind).or_default() += 1;
                }
                true
            }
            None => false,
        }
    }

    /// Whether the sources may hold transparency (a conservative look before drawing a page
    /// in flattening mode; pages without it are drawn exactly as live output draws them).
    pub fn may_have_transparency(&self, sources: &[Source<'d>]) -> bool {
        fn graphic_transparent(g: &Graphic) -> bool {
            matches!(g.element.as_str(), "PDF" | "EPS" | "ImportedPage")
                || g.preview.as_deref().is_some_and(|p| !p.to_ascii_lowercase().ends_with(".jpg") && folio_render::header_has_alpha(p).unwrap_or(false))
        }
        fn item_transparent(d: &Drawer<'_, '_>, item: &PageItem, depth: usize) -> bool {
            if depth > 64 {
                return true;
            }
            let fx = &item.print.fx;
            if Drawer::needs_group(item)
                || fx.drop_shadow.is_some()
                || fx.outer_glow.is_some()
                || fx.feather.is_some()
                || fx.directional_feather.is_some()
                || fx.gradient_feather.is_some()
                || fx.inner_shadow.is_some()
                || fx.inner_glow.is_some()
                || fx.bevel.is_some()
                || fx.satin.is_some()
                || item.print.fill_blend.is_some()
                || item.print.stroke_blend.is_some()
                || item.print.content_blend.is_some()
                || item.print.level_fx.fill.any()
                || item.print.level_fx.stroke.any()
                || item.print.level_fx.content.any()
            {
                return true;
            }
            // These arms consume their variants even when false; guards would fall through to text-on-path below.
            #[allow(clippy::collapsible_match)]
            match &item.kind {
                ItemKind::Group { children } => {
                    if children.iter().any(|c| item_transparent(d, c, depth + 1)) {
                        return true;
                    }
                }
                ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => {
                    // an SVG read by resvg may draw part of itself as pixels with transparency (folio_render svg/rich.rs)
                    let rich_svg = || g.element == "SVG" && d.reg.graphics.svg_source(g).and_then(|p| folio_render::svg::read(&p).ok()).is_some_and(|s| s.rich.is_some());
                    if graphic_transparent(g) || g.embedded_data.is_some() || rich_svg() {
                        return true;
                    }
                }
                _ if item.text_frame().is_some() => {
                    if let Some(t) = d.frame_text(&item.id) {
                        for o in t.lines.iter().flat_map(|l| &l.objects) {
                            if let Some(i) = d.inline.get(o.item.as_str())
                                && item_transparent(d, i, depth + 1)
                            {
                                return true;
                            }
                        }
                    }
                }
                _ => {}
            }
            item.clipped.iter().any(|c| item_transparent(d, c, depth + 1))
        }
        // links found with alpha (PNG / TIFF / PSD transparency)
        let alpha_links = |item: &PageItem| -> bool {
            let mut any = false;
            let mut stack = vec![item];
            while let Some(i) = stack.pop() {
                if let ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } = &i.kind {
                    any |= self.reg.graphics.link_has_alpha(&g.id);
                }
                if let ItemKind::Group { children } = &i.kind {
                    stack.extend(children.iter());
                }
                stack.extend(i.clipped.iter());
            }
            any
        };
        sources.iter().any(|src| src.items.iter().any(|i| item_transparent(self, i, 0) || alpha_links(i)))
    }

    /// Bounds of the item with its shadow, glow and outer bevel (parent coordinates).
    pub(super) fn effect_extent(&self, item: &PageItem) -> Option<Rect> {
        let ext = self.item_extent(item)?;
        let mut r = ext;
        let l = &item.print.level_fx;
        for fx in [&item.print.fx, &l.fill, &l.content, &l.stroke] {
            for (kind, e) in crate::effects::outer(fx) {
                r = r.union(&crate::effects::shadow_rect(&ext, e, kind));
            }
        }
        Some(r.union(&grow(ext, crate::effects::item_reach(item))))
    }

    pub(super) fn needs_group(item: &PageItem) -> bool {
        item.opacity < 1.0
            || item.blend_mode != BlendMode::Normal
            || (matches!(item.kind, ItemKind::Group { .. }) && (item.print.isolate_blending || item.print.knockout_group))
    }

    /// Shadows and glows behind the item (or one part of it, InDesign's Effects levels), then
    /// it with its inner effects (feathered when it is).
    pub(super) fn draw_effects_and_body(&mut self, s: &mut Stream, item: &'d PageItem, fx: &'d ObjectEffects, which: Part) {
        let behind = fx.drop_shadow.is_some() || fx.outer_glow.is_some();
        if behind {
            self.unit_begin(s, order(which, part::BEHIND));
        }
        for (kind, e) in crate::effects::outer(fx) {
            self.draw_shadow(s, item, e, kind, which);
        }
        if behind {
            self.unit_end(s);
        }
        if fx.feather.is_some() || fx.directional_feather.is_some() || fx.gradient_feather.is_some() {
            self.unit_begin(s, order(which, part::FILL));
            self.draw_feathered(s, item, fx, which);
            self.unit_end(s);
        } else {
            self.draw_body_fx(s, item, fx, which);
        }
    }

    /// The item (or one part of it), then its inner effects (inner shadow, inner glow, satin,
    /// bevel and emboss): each effect colour painted through its mask with the effect's blend
    /// mode and opacity, as InDesign exports them live.
    fn draw_body_fx(&mut self, s: &mut Stream, item: &'d PageItem, fx: &'d ObjectEffects, which: Part) {
        match which {
            Part::All => self.draw_body(s, item),
            p => self.draw_body_part(s, item, p),
        }
        if fx.inner_shadow.is_none() && fx.inner_glow.is_none() && fx.satin.is_none() && fx.bevel.is_none() {
            return;
        }
        let Some(ext) = self.item_extent(item) else { return };
        if self.flat.is_some() {
            self.unit_begin(s, order(which, part::INNER));
            self.flat_site(s, &grow(ext, crate::effects::reach(fx) + 2.0), "inner effect");
            self.unit_end(s);
            return;
        }
        let ppu = crate::effects::inner_ppu(crate::effects::pixels_per_unit(&s.ctm(), &grow(ext, crate::effects::reach(fx) + 30.0)), fx);
        let layers = {
            let (layout, page_text) = (self.layout, &self.page_text);
            let text_of = |id: &str| -> Option<&'d FrameText> { page_text.get(id).copied().or_else(|| layout.frames.get(id)) };
            let shapes = crate::effects::Shapes { text: &text_of, glyphs: self.fonts, inline: &self.inline };
            crate::effects::inner_layers(item, &shapes, &ext, ppu, fx, which)
        };
        for l in layers {
            if !l.mask.any() || l.opacity <= 0.0 {
                continue;
            }
            let Some(col) = self.reg.res.resolve(l.color) else { continue };
            let rect = l.mask.rect;
            let form = self.mask_form(l.mask);
            let gs = self.reg.gs(GsDef::Mask { form, alpha: (l.opacity.clamp(0.0, 1.0) * 1000.0).round() as u32, mode: l.blend });
            s.push();
            s.ext_gstate(gs);
            // InDesign overprints a 100 % black effect colour like other black ([Black] overprint)
            s.set_overprint(self.reg, Some(self.overprint_black && col.black100), None);
            s.set_fill(&col.col);
            s.rect(&rect);
            s.fill(false);
            s.pop();
            s.transparency = true;
            self.reg.counts.effects += 1;
        }
    }

    fn mask_form(&mut self, raster: crate::effects::Alpha) -> usize {
        let rect = raster.rect;
        let i = self.reg.rasters.len();
        self.reg.rasters.push(raster);
        let mut m = Stream::new_form(None);
        m.push();
        m.concat(&Affine::new(rect.width(), 0.0, 0.0, -rect.height(), rect.left, rect.bottom));
        m.xobject_raster(i);
        m.pop();
        let (content, used, _) = m.finish_form();
        self.reg.add_form(FormDef::Mask { content, used, bbox: rect })
    }

    /// A drop shadow or outer glow: the effect colour painted through the blurred silhouette.
    fn draw_shadow(&mut self, s: &mut Stream, item: &'d PageItem, e: &folio_model::ShadowEffect, kind: crate::effects::Outer, which: Part) {
        let Some(ext) = self.item_extent(item) else { return };
        let Some(col) = self.reg.res.resolve(&e.color) else { return };
        let rect = crate::effects::shadow_rect(&ext, e, kind);
        if self.flat_site(s, &rect, "drop shadow or outer glow") {
            return;
        }
        let ppu = crate::effects::shadow_ppu(crate::effects::pixels_per_unit(&s.ctm(), &rect), e, kind);
        let a = {
            let (layout, page_text) = (self.layout, &self.page_text);
            let text_of = |id: &str| -> Option<&'d FrameText> { page_text.get(id).copied().or_else(|| layout.frames.get(id)) };
            let shapes = crate::effects::Shapes { text: &text_of, glyphs: self.fonts, inline: &self.inline };
            crate::effects::shadow(item, &shapes, &rect, ppu, e, kind, which)
        };
        let Some(a) = a else { return };
        let form = self.mask_form(a);
        let gs = self.reg.gs(GsDef::Mask { form, alpha: (e.opacity * 1000.0).round() as u32, mode: e.blend });
        s.push();
        s.ext_gstate(gs);
        s.set_overprint(self.reg, Some(false), None);
        s.set_fill(&col.col);
        s.rect(&rect);
        s.fill(false);
        s.pop();
        s.transparency = true;
        self.reg.counts.effects += 1;
    }

    /// Basic, directional and gradient feather: the item drawn in a group under a soft mask.
    fn draw_feathered(&mut self, s: &mut Stream, item: &'d PageItem, fx: &'d ObjectEffects, which: Part) {
        let Some(ext) = self.item_extent(item) else { return };
        let rect = grow(ext, 2.0);
        if self.flat_site(s, &rect, "feather") {
            return;
        }
        let ppu = crate::effects::pixels_per_unit(&s.ctm(), &rect);
        let a = {
            let (layout, page_text) = (self.layout, &self.page_text);
            let text_of = |id: &str| -> Option<&'d FrameText> { page_text.get(id).copied().or_else(|| layout.frames.get(id)) };
            let shapes = crate::effects::Shapes { text: &text_of, glyphs: self.fonts, inline: &self.inline };
            crate::effects::feather_mask(item, &shapes, &ext, ppu, fx, which)
        };
        let Some(a) = a else {
            self.draw_body_fx(s, item, fx, which);
            return;
        };
        // the item itself in a group, painted through the mask
        let inv = s.ctm().invert();
        let cull = match (s.cull, inv) {
            (Some(c), Some(inv)) => Some(inv.apply_rect(&c)),
            _ => None,
        };
        let mut f = Stream::new_form(cull);
        self.draw_body_fx(&mut f, item, fx, which);
        let (content, used, _) = f.finish_form();
        let body = self.reg.add_form(FormDef::Group { content, used, bbox: rect, group: GroupDef { isolated: false, knockout: false } });
        let mask = self.mask_form(a);
        let gs = self.reg.gs(GsDef::Mask { form: mask, alpha: 1000, mode: BlendMode::Normal });
        s.push();
        s.ext_gstate(gs);
        s.xobject_form(body);
        s.pop();
        s.transparency = true;
        self.reg.counts.effects += 1;
    }

    /// The item drawn opaque into a transparency group XObject, composited once with its
    /// opacity and blend mode (InDesign applies them to the object as a whole).
    pub(super) fn draw_group(&mut self, s: &mut Stream, item: &'d PageItem, ext: Option<Rect>) {
        let Some(ext) = ext else { return };
        let bbox = grow(ext, 1.0);
        let inv = s.ctm().invert();
        let cull = match (s.cull, inv) {
            (Some(c), Some(inv)) => Some(inv.apply_rect(&c)),
            _ => None,
        };
        let mut f = Stream::new_form(cull);
        self.draw_effects_and_body(&mut f, item, &item.print.fx, Part::All);
        let (content, used, _) = f.finish_form();
        let form = self.reg.add_form(FormDef::Group {
            content,
            used,
            bbox,
            group: GroupDef { isolated: item.print.isolate_blending, knockout: item.print.knockout_group },
        });
        self.reg.counts.transparency_groups += 1;
        s.push();
        if item.opacity < 1.0 || item.blend_mode != BlendMode::Normal {
            let gs = self.reg.gs(GsDef::Blend { alpha: (item.opacity.clamp(0.0, 1.0) * 1000.0).round() as u32, mode: item.blend_mode });
            s.ext_gstate(gs);
        }
        s.xobject_form(form);
        s.pop();
        s.transparency = true;
    }
}

/// The paint-order unit of an effect stage: the item's own stage, or that of the part a
/// level's effects belong to.
fn order(which: Part, stage: u8) -> u8 {
    match which {
        Part::All => stage,
        Part::Fill => part::FILL,
        Part::Content => part::CONTENT,
        Part::Stroke => part::STROKE,
    }
}
