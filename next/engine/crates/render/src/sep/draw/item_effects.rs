//! Renders item effects onto ink planes and reuses cached effect rasters.

use super::Ctx;
use super::super::{FxEntry, FxKey, InnerFx, part};
use super::super::canvas::{Canvas, Cov, alpha_cov, pixel_box};
use crate::{effects, item_extent};
use folio_model::layout::FrameText;
use folio_model::{Affine, BlendMode, PageItem, Rect, ShadowEffect};

impl<'a> Ctx<'a> {
    /// Shadows and glows behind the item (or one part of it, InDesign's Effects levels), then
    /// it with its inner effects (feathered when it is).
    // Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn effects_and_body(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>, fx: &'a folio_model::ObjectEffects, which: effects::Part) {
        if self.allow(order(which, part::BEHIND)) {
            for (kind, e) in effects::outer(fx) {
                self.shadow(canvas, item, e, kind, parent, clip, which);
            }
        }
        if fx.feather.is_some() || fx.directional_feather.is_some() || fx.gradient_feather.is_some() {
            if self.allow(order(which, part::FILL)) {
                // the feathered body is one unit of paint order
                self.depth += 1;
                self.feathered(canvas, item, parent, clip, fx, which);
                self.depth -= 1;
            }
        } else {
            self.body_fx(canvas, item, parent, clip, fx, which);
        }
    }

    /// One part of an item without its effects.
    fn part_body(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>, which: effects::Part) {
        match which {
            effects::Part::All => self.body(canvas, item, parent, clip),
            p => self.body_part(canvas, item, parent, clip, p),
        }
    }

    /// Pixels per user unit of an effect raster drawn under `parent` over `rect`: the canvas
    /// resolution, or the fixed print resolution when one is set.
    fn fx_ppu(&self, parent: &Affine, rect: &Rect) -> f64 {
        effects::resolution(parent, rect, self.fx_per_point.unwrap_or(1.0))
    }

    /// The cache key of an effect raster (`kind`) of a part of `item` at `ppu`.
    fn fx_key(&self, item: &PageItem, kind: u8, which: effects::Part, ppu: f64) -> FxKey {
        let level = match which { effects::Part::All => 0, effects::Part::Fill => 1, effects::Part::Content => 2, effects::Part::Stroke => 3 };
        (item as *const PageItem as usize, kind + 8 * level, (ppu * 1e6).round() as i64, self.page)
    }

    fn cached_alpha(&self, key: FxKey, make: impl FnOnce() -> Option<effects::Alpha>) -> Option<std::sync::Arc<effects::Alpha>> {
        let Some(cache) = self.fx else { return make().map(std::sync::Arc::new) };
        if let Some(FxEntry::Alpha(a)) = cache.get(&key) {
            return a;
        }
        let a = make().map(std::sync::Arc::new);
        cache.put(key, FxEntry::Alpha(a.clone()));
        a
    }

    /// Runs `f` with the shapes the effect rasteriser needs.
    fn with_shapes<R>(&self, f: impl FnOnce(&effects::Shapes<'_, 'a>) -> R) -> R {
        let (layout, page_text) = (self.layout, &self.page_text);
        let text_of = |id: &str| -> Option<&'a FrameText> { page_text.get(id).copied().or_else(|| layout.frames.get(id)) };
        let shapes = effects::Shapes { text: &text_of, glyphs: self.glyphs, inline: &self.inline };
        f(&shapes)
    }

    /// A drop shadow or outer glow: its colour through the blurred silhouette (knockout, as
    /// the PDF writes it).
    // Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
    #[allow(clippy::too_many_arguments)]
    fn shadow(&mut self, canvas: &mut Canvas, item: &'a PageItem, e: &ShadowEffect, kind: effects::Outer, parent: &Affine, clip: Option<&Cov>, which: effects::Part) {
        let Some(ext) = item_extent(item) else { return };
        let rect = effects::shadow_rect(&ext, e, kind);
        let ppu = effects::shadow_ppu(self.fx_ppu(parent, &rect), e, kind);
        let key = self.fx_key(item, if kind == effects::Outer::DropShadow { 0 } else { 1 }, which, ppu);
        let Some(a) = self.cached_alpha(key, || self.with_shapes(|sh| effects::shadow(item, sh, &rect, ppu, e, kind, which))) else { return };
        let Some(col) = self.col(&e.color) else { return };
        let Some(cov) = alpha_cov(canvas.w, canvas.h, &a, parent, ppu, clip) else { return };
        canvas.soft = true;
        self.stats.soft_paints += 1;
        self.paint_col(canvas, &cov, &col, false, (e.opacity, e.blend));
        canvas.soft = false;
    }

    /// Basic, directional and gradient feather: the item in a layer under the feather mask.
    // Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
    #[allow(clippy::too_many_arguments)]
    fn feathered(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>, fx: &'a folio_model::ObjectEffects, which: effects::Part) {
        let Some(ext) = item_extent(item) else {
            self.body_fx(canvas, item, parent, clip, fx, which);
            return;
        };
        let ppu = self.fx_ppu(parent, &effects::grow(ext, 2.0));
        let key = self.fx_key(item, 2, which, ppu);
        let Some(feather) = self.cached_alpha(key, || self.with_shapes(|sh| effects::feather_mask(item, sh, &ext, ppu, fx, which))) else {
            self.body_fx(canvas, item, parent, clip, fx, which);
            return;
        };
        let r = parent.apply_rect(&feather.rect);
        let Some((x0, y0, x1, y1)) = pixel_box(&r, canvas.w, canvas.h, 1.0, clip) else { return };
        let (lw, lh) = (x1 - x0, y1 - y0);
        let mut layer = canvas.sublayer(lw, lh);
        let sub = clip.map(|c| c.rebase(x0, y0, lw, lh));
        let lt = Affine::translate(-(x0 as f64), -(y0 as f64)).then_inner(parent);
        self.stats.layers += 1;
        self.depth += 1;
        self.body_fx(&mut layer, item, &lt, sub.as_ref(), fx, which);
        self.depth -= 1;
        let Some(mask) = alpha_cov(lw, lh, &feather, &lt, ppu, None) else { return };
        self.stats.soft_paints += 1;
        canvas.composite(&layer, x0, y0, BlendMode::Normal, 1.0, Some(&mask));
    }

    /// The item (or one part of it), then its inner shadow, inner glow, satin, bevel and emboss.
    // Keep the drawing boundary explicit: geometry, paint, clipping and destination travel together.
    #[allow(clippy::too_many_arguments)]
    fn body_fx(&mut self, canvas: &mut Canvas, item: &'a PageItem, parent: &Affine, clip: Option<&Cov>, fx: &'a folio_model::ObjectEffects, which: effects::Part) {
        self.part_body(canvas, item, parent, clip, which);
        if fx.inner_shadow.is_none() && fx.inner_glow.is_none() && fx.satin.is_none() && fx.bevel.is_none() {
            return;
        }
        if !self.allow(order(which, part::INNER)) {
            return;
        }
        let Some(ext) = item_extent(item) else { return };
        let ppu = effects::inner_ppu(self.fx_ppu(parent, &effects::grow(ext, effects::reach(fx) + 30.0)), fx);
        let key = self.fx_key(item, 3, which, ppu);
        let layers = match self.fx.and_then(|c| c.get(&key)) {
            Some(FxEntry::Inner(l)) => l,
            _ => {
                let l: Vec<InnerFx> = self
                    .with_shapes(|sh| effects::inner_layers(item, sh, &ext, ppu, fx, which))
                    .into_iter()
                    .map(|l| (l.mask, l.color.clone(), l.blend, l.opacity))
                    .collect();
                let l = std::sync::Arc::new(l);
                if let Some(c) = self.fx {
                    c.put(key, FxEntry::Inner(l.clone()));
                }
                l
            }
        };
        canvas.soft = true;
        self.stats.soft_paints += 1;
        for (mask, color, blend, opacity) in layers.iter() {
            if !mask.any() || *opacity <= 0.0 {
                continue;
            }
            let Some(col) = self.col(color) else { continue };
            let Some(cov) = alpha_cov(canvas.w, canvas.h, mask, parent, ppu, clip) else { continue };
            // a 100 % black effect colour overprints like other [Black]
            let op = self.overprint_black && col.black100;
            self.paint_col(canvas, &cov, &col, op, (*opacity, *blend));
        }
        canvas.soft = false;
    }
}

/// The paint-order unit of an effect stage: the item's own stage, or that of the part a
/// level's effects belong to.
fn order(which: effects::Part, stage: u8) -> u8 {
    match which {
        effects::Part::All => stage,
        effects::Part::Fill => part::FILL,
        effects::Part::Content => part::CONTENT,
        effects::Part::Stroke => part::STROKE,
    }
}
