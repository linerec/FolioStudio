//! Page content: master and spread items layer by layer, each frame as InDesign paints it
//! (fill, placed graphic clipped to the frame, pasted-in items, text and inline objects,
//! stroke), with overprint, live transparency groups and ink-native colour.

mod gradients;
mod graphic;
mod paint;
mod text;
mod transparency;

use crate::content::Stream;
use crate::flatten::{PageFlat, R};
use crate::registry::Registry;
use folio_model::geom::StrokeArt;
use folio_model::layout::{DocumentLayout, FrameText};
use folio_model::{Affine, Document, ItemKind, PageItem, Rect};
use folio_render::effects::Part;
use folio_render::sep::part;
use pdf_writer::types::RenderingIntent;
use std::collections::HashMap;

pub struct Drawer<'a, 'd> {
    pub doc: &'d Document,
    pub layout: &'d DocumentLayout,
    fonts: &'d folio_text::FontStore,
    pub reg: &'a mut Registry<'d>,
    /// Layer id → (stacking rank, written): the layers Export Layers takes, every layer with Acrobat layers.
    layers: HashMap<&'d str, (usize, bool)>,
    /// Acrobat layers: stacking rank → the layer's optional content group.
    oc_of_rank: HashMap<usize, usize>,
    inline: HashMap<&'d str, &'d PageItem>,
    /// Story text as layout indexes it (runs, then a return between paragraphs).
    story_chars: HashMap<&'d str, Vec<char>>,
    /// Master frames laid out again for the page whose master is being drawn.
    page_text: HashMap<&'d str, &'d FrameText>,
    /// Document page index being drawn (reports).
    pub page: usize,
    /// Master items hidden on the page whose master is being drawn.
    hidden: std::collections::HashSet<&'d str>,
    overprint_black: bool,
    solid_color_intent: RenderingIntent,
    /// Flattening (PDF/X-1a, PDF/X-3): transparent constructs are left out and recorded,
    /// units of paint order noted ([`crate::flatten`]); `None` writes everything live.
    pub flat: Option<PageFlat>,
}

/// One source of items for a page: a spread's items (or a master's, mapped onto a page).
pub struct Source<'d> {
    pub items: &'d [PageItem],
    pub transform: Affine,
    /// Master items: only those on this master page (spread x range), for this document page.
    pub master_page: Option<(Rect, usize)>,
    /// Master items the document page overrides or removed (not drawn).
    pub hidden: Vec<&'d str>,
}

fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(&b)),
        (a, None) => a,
        (None, b) => b,
    }
}

fn grow(r: Rect, m: f64) -> Rect {
    Rect { left: r.left - m, top: r.top - m, right: r.right + m, bottom: r.bottom + m }
}


impl<'a, 'd> Drawer<'a, 'd> {
    pub fn new(doc: &'d Document, layout: &'d DocumentLayout, fonts: &'d folio_text::FontStore, reg: &'a mut Registry<'d>) -> Self {
        let acrobat = reg.ocg_of_layer.is_some();
        let which = reg.export_layers;
        let layers = doc.layers.iter().map(|l| (l.id.as_str(), (l.order, acrobat || which.includes(l.visible, l.printable)))).collect();
        let oc_of_rank = reg.ocg_of_layer.as_ref().map(|m| doc.layers.iter().filter_map(|l| m.get(l.id.as_str()).map(|&o| (l.order, o))).collect()).unwrap_or_default();
        let inline = doc
            .stories
            .values()
            .flat_map(|s| s.all_runs())
            .flat_map(|r| &r.inline)
            .map(|i| (i.id.as_str(), i))
            .collect();
        Drawer {
            doc,
            layout,
            fonts,
            reg,
            layers,
            oc_of_rank,
            inline,
            story_chars: HashMap::new(),
            page: 0,
            hidden: Default::default(),
            page_text: HashMap::new(),
            overprint_black: doc.print.overprint_black,
            solid_color_intent: doc.print.solid_color_intent.as_deref().map(crate::image_color::pdf_intent).unwrap_or_default(),
            flat: None,
        }
    }

    fn rank(&self, item: &PageItem) -> Option<usize> {
        match item.layer.as_deref().and_then(|l| self.layers.get(l)) {
            Some(&(rank, printed)) => printed.then_some(rank),
            None => Some(0),
        }
    }

    fn printed(&self, item: &PageItem) -> bool {
        item.visible && !item.print.nonprinting
    }

    /// Draws the sources layer by layer: within a layer, sources in order (masters first).
    pub fn draw_sources(&mut self, s: &mut Stream, sources: &[Source<'d>]) {
        if self.solid_color_intent != RenderingIntent::default() {
            s.c.set_rendering_intent(self.solid_color_intent);
        }
        let mut ranks: Vec<usize> = Vec::new();
        for src in sources {
            for it in src.items {
                if let Some(r) = self.rank(it)
                    && !ranks.contains(&r)
                {
                    ranks.push(r);
                }
            }
        }
        ranks.sort_unstable();
        for rank in ranks {
            // Acrobat layers: each layer's content in its optional content (InDesign: `/OC /MCn BDC … EMC` per layer, bottom first)
            let oc = self.oc_of_rank.get(&rank).copied();
            if let Some(o) = oc {
                s.begin_layer(o);
            }
            for (si, src) in sources.iter().enumerate() {
                self.page_text.clear();
                self.hidden = src.hidden.iter().copied().collect();
                if let Some((_, page)) = src.master_page {
                    let layout = self.layout;
                    for pf in layout.page_frames.iter().filter(|p| p.page == page) {
                        self.page_text.insert(pf.frame.frame.as_str(), &pf.frame);
                    }
                }
                s.push();
                s.concat(&src.transform);
                for (ii, item) in src.items.iter().enumerate() {
                    if self.rank(item) != Some(rank) || !self.printed(item) {
                        continue;
                    }
                    if let Some(f) = self.flat.as_mut() {
                        f.cur = (rank as u32, si as u32, ii as u32);
                    }
                    if let Some((mp, _)) = &src.master_page {
                        // Master items belong to the master page their centre lies on.
                        let on_page = self.item_extent(item).is_none_or(|e| {
                            let cx = (e.left + e.right) / 2.0;
                            cx >= mp.left - 0.01 && cx <= mp.right + 0.01
                        });
                        if !on_page {
                            continue;
                        }
                    }
                    self.draw_item(s, item);
                }
                s.pop();
            }
            if oc.is_some() {
                s.end_layer();
            }
        }
        self.page_text.clear();
        self.hidden.clear();
    }

    fn draw_children(&mut self, s: &mut Stream, items: &'d [PageItem]) {
        let mut order: Vec<(usize, &'d PageItem)> =
            items.iter().filter(|i| self.printed(i)).filter_map(|i| self.rank(i).map(|r| (r, i))).collect();
        order.sort_by_key(|(r, _)| *r);
        for (_, item) in order {
            self.draw_item(s, item);
        }
    }

    /// Bounds of an item in its parent's coordinates, with room for its stroke and text.
    pub fn item_extent(&self, item: &PageItem) -> Option<Rect> {
        let inner = match &item.kind {
            ItemKind::Group { children } => children.iter().filter_map(|c| self.item_extent(c)).reduce(|a, b| a.union(&b))?,
            _ => {
                let mut r = folio_model::paths_bounds(&item.paths).map(|b| grow(b, item.stroke.weight.max(0.0) + 1.0));
                r = union(r, item.stroke.art.as_ref().and_then(StrokeArt::bounds).map(|b| grow(b, 1.0)));
                if item.text_frame().is_some() {
                    r = union(r, self.text_extent(&item.id));
                }
                for c in &item.clipped {
                    // pasted-in items are clipped to the frame; only frameless ones extend it
                    if item.paths.is_empty() {
                        r = union(r, self.item_extent(c));
                    }
                }
                r?
            }
        };
        Some(item.transform.apply_rect(&inner))
    }

    pub fn draw_item(&mut self, s: &mut Stream, item: &'d PageItem) {
        if self.hidden.contains(item.id.as_str()) {
            return;
        }
        self.reg.counts.items += 1;
        let ext = self.effect_extent(item);
        if let Some(e) = &ext
            && !s.visible(&grow(*e, 2.0))
        {
            self.reg.counts.items_culled += 1;
            return;
        }
        const LIVE: [&str; 9] = [
            "DropShadowSetting (object)",
            "OuterGlowSetting (object)",
            "FeatherSetting (object)",
            "DirectionalFeatherSetting (object)",
            "GradientFeatherSetting (object)",
            "InnerShadowSetting (object)",
            "InnerGlowSetting (object)",
            "BevelAndEmbossSetting (object)",
            "SatinSetting (object)",
        ];
        for fx in &item.print.effects {
            let partial = fx.ends_with(" opacity") && ["fill", "stroke", "content"].iter().any(|w| fx.starts_with(w))
                || fx.starts_with("blend mode ") && ["(fill)", "(stroke)", "(content)"].iter().any(|w| fx.ends_with(w));
            // the levels' effects are drawn live with their part
            let level = ["(fill)", "(stroke)", "(content)"].iter().find_map(|w| fx.strip_suffix(w)).is_some_and(|e| LIVE.iter().any(|l| l.starts_with(e)));
            if !LIVE.contains(&fx.as_str()) && !partial && !level {
                self.reg.warn.add("effect", format!("{fx} not exported (no live effect in the PDF)"), Some(self.page), Some(&item.id));
            }
        }
        if item.opacity <= 0.0 {
            return;
        }
        if let Some(f) = self.flat.as_mut() {
            f.depth += 1;
            if f.depth == 1 {
                f.extent = ext.map(|e| R::of(&s.ctm().apply_rect(&grow(e, 2.0))));
            }
        }
        if Self::needs_group(item) {
            if self.flat.is_some() {
                // composited as a whole: one unit of paint order, rasterised
                self.unit_begin(s, part::BEHIND);
                if let Some(e) = ext {
                    self.flat_site(s, &grow(e, 1.0), "opacity, blend mode or group");
                }
                self.unit_end(s);
            } else {
                self.draw_group(s, item, ext);
            }
        } else {
            self.draw_effects_and_body(s, item, &item.print.fx, Part::All);
        }
        if let Some(f) = self.flat.as_mut() {
            f.depth -= 1;
        }
    }

}
