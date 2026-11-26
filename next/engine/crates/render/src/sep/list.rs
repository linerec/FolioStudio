//! The plates a document prints on: every paint on every page resolved through the Ink
//! Manager ([`crate::inks`]), process inks first, then the spot inks that keep their plate,
//! in Ink Manager order.

use super::{DocInks, InkKind, PlateInk};
use crate::inks::{Col, CsDef, PROCESS_NAMES, Resolver};
use crate::item_extent;
use folio_color::{BlendSpace, Colors};
use folio_model::layout::{DocumentLayout, FrameText};
use folio_model::{ColorModel, ColorSpace, Document, ItemKind, PageItem, Paint, Rect};
use std::collections::{BTreeSet, HashMap};

/// What one page's walk found.
#[derive(Default)]
struct Found {
    process: bool,
    spots: BTreeSet<String>,
}

struct Walk<'d> {
    doc: &'d Document,
    layout: &'d DocumentLayout,
    res: Resolver<'d>,
    layers: HashMap<&'d str, bool>,
    inline: HashMap<&'d str, &'d PageItem>,
    page_text: HashMap<&'d str, &'d FrameText>,
    seen: HashMap<(String, u32), (bool, Vec<String>)>,
}

impl<'d> Walk<'d> {
    fn printed(&self, item: &PageItem) -> bool {
        item.visible && !item.print.nonprinting && item.layer.as_deref().and_then(|l| self.layers.get(l)).is_none_or(|p| *p)
    }

    /// Records the inks of a flat paint (a gradient: of its stops).
    fn paint(&mut self, paint: &Paint, found: &mut Found) {
        match paint {
            Paint::None => {}
            Paint::Gradient { id, .. } => {
                let stops: Vec<String> = self.doc.gradients.get(id).map(|g| g.stops.iter().map(|s| s.color.clone()).collect()).unwrap_or_default();
                for s in stops {
                    self.paint(&Paint::Swatch { id: s, tint: 100.0 }, found);
                }
            }
            Paint::Swatch { id, tint } => {
                let key = (id.clone(), (tint * 1000.0).round() as u32);
                let (process, spots) = match self.seen.get(&key) {
                    Some(hit) => hit.clone(),
                    None => {
                        let hit = match self.res.resolve(paint) {
                            Some(r) => col_inks(&r.col, &self.res),
                            None => (false, Vec::new()),
                        };
                        self.seen.insert(key, hit.clone());
                        hit
                    }
                };
                found.process |= process;
                found.spots.extend(spots);
            }
        }
    }

    fn item(&mut self, item: &'d PageItem, found: &mut Found) {
        if !self.printed(item) {
            return;
        }
        if item.paths.iter().any(|p| !p.points.is_empty()) {
            self.paint(&item.fill, found);
            if item.stroke.weight > 0.0 {
                self.paint(&item.stroke.paint, found);
                if item.stroke.art.is_some() {
                    self.paint(&item.stroke.gap, found);
                }
            }
        }
        let fx = &item.print.fx;
        for e in [&fx.drop_shadow, &fx.outer_glow, &fx.inner_shadow].into_iter().flatten() {
            self.paint(&e.color, found);
        }
        if let Some(g) = &fx.inner_glow {
            self.paint(&g.color, found);
        }
        if let Some(s) = &fx.satin {
            self.paint(&s.color, found);
        }
        if let Some(b) = &fx.bevel {
            self.paint(&b.highlight, found);
            self.paint(&b.shadow, found);
        }
        match &item.kind {
            ItemKind::Group { children } => {
                for c in children {
                    self.item(c, found);
                }
            }
            ItemKind::Rectangle { content: Some(_) } | ItemKind::Oval { content: Some(_) } | ItemKind::Polygon { content: Some(_) } => {
                // placed images print on process plates (a placed PDF's own spot inks are
                // not seen: only its proxy is)
                found.process = true;
            }
            _ => {}
        }
        if let Some(tf) = item.text_frame() {
            self.text(&item.id, &tf.story, found);
        }
        for c in &item.clipped {
            self.item(c, found);
        }
    }

    fn text(&mut self, frame: &str, _story: &str, found: &mut Found) {
        let Some(text) = self.page_text.get(frame).copied().or_else(|| self.layout.frames.get(frame)) else { return };
        let doc = self.doc;
        let mut paints: Vec<&'d Paint> = Vec::new();
        let mut objects: Vec<&'d PageItem> = Vec::new();
        for line in &text.lines {
            paints.extend(line.shapes.iter().map(|s| &s.paint));
            paints.extend(line.decorations.iter().flat_map(|d| d.shapes.iter().map(|s| &s.paint)));
            for g in &line.glyphs {
                if !paints.iter().any(|p| **p == g.fill) {
                    paints.push(&g.fill);
                }
            }
            paints.extend(line.glyphs.iter().filter(|g| g.stroke != Paint::None).map(|g| &g.stroke));
            objects.extend(line.objects.iter().filter_map(|o| self.inline.get(o.item.as_str()).copied()));
        }
        for p in paints {
            // text paints flat: a gradient prints as its first stop
            match p {
                Paint::Gradient { id, .. } => {
                    if let Some(first) = doc.gradients.get(id).and_then(|g| g.stops.first()) {
                        self.paint(&Paint::Swatch { id: first.color.clone(), tint: 100.0 }, found);
                    }
                }
                p => self.paint(p, found),
            }
        }
        for o in objects {
            self.item(o, found);
        }
    }
}

/// Whether a resolved colour uses process plates, and the spot plates it uses.
fn col_inks(col: &Col, res: &Resolver) -> (bool, Vec<String>) {
    match col {
        Col::Cmyk(c) => (c.iter().any(|v| *v > 0.0), Vec::new()),
        Col::Gray(g) => (*g < 1.0, Vec::new()),
        Col::Space(i, vals) => match res.spaces.defs.get(*i) {
            // Registration prints on every plate; it adds none
            Some(CsDef::Separation { ink, .. }) if ink == "All" => (true, Vec::new()),
            Some(CsDef::Separation { ink, .. }) => (false, vec![ink.clone()]),
            Some(CsDef::DeviceN { inks, .. }) => {
                let process = inks.iter().zip(vals).any(|(n, v)| PROCESS_NAMES.contains(&n.as_str()) && *v > 0.0);
                (process, inks.iter().filter(|n| !PROCESS_NAMES.contains(&n.as_str())).cloned().collect())
            }
            _ => (true, Vec::new()),
        },
    }
}

/// The plates of `doc` and the ones each page uses.
pub fn document_inks(doc: &Document, layout: &DocumentLayout, colors: &Colors) -> DocInks {
    let mut walk = Walk {
        doc,
        layout,
        res: Resolver::new(doc),
        layers: doc.layers.iter().map(|l| (l.id.as_str(), l.visible && l.printable)).collect(),
        inline: doc.stories.values().flat_map(|s| s.all_runs()).flat_map(|r| &r.inline).map(|i| (i.id.as_str(), i)).collect(),
        page_text: HashMap::new(),
        seen: HashMap::new(),
    };
    let mut pages: Vec<(usize, Found)> = Vec::new();
    for spread in &doc.spreads {
        for page in &spread.pages {
            let mut found = Found::default();
            let rect = page.transform.apply_rect(&page.bounds);
            // the page's master (items on the master page it uses, not overridden)
            walk.page_text = layout.page_frames.iter().filter(|p| p.page == page.index).map(|p| (p.frame.frame.as_str(), &p.frame)).collect();
            for source in folio_model::parents::sources(doc, spread, page) {
                for item in &source.spread.items {
                    if source.hidden.contains(&item.id.as_str()) { continue; }
                    let on_page = item_extent(item).is_none_or(|e| {
                        let cx = (e.left + e.right) / 2.0;
                        cx >= source.range.left - 0.01 && cx <= source.range.right + 0.01
                    });
                    if on_page { walk.item(item, &mut found); }
                }
            }
            walk.page_text.clear();
            for item in &spread.items {
                if item_extent(item).is_some_and(|e| overlaps(&e, &rect)) {
                    walk.item(item, &mut found);
                }
            }
            pages.push((page.index, found));
        }
    }
    pages.sort_by_key(|(i, _)| *i);

    // spot plates in Ink Manager order, then any other (an ink a swatch names but the Ink
    // Manager does not list)
    let used: BTreeSet<String> = pages.iter().flat_map(|(_, f)| f.spots.iter().cloned()).collect();
    let mut spots: Vec<String> = doc.print.inks.iter().filter(|i| !i.process && used.contains(&i.name)).map(|i| i.name.clone()).collect();
    for s in &used {
        if !spots.contains(s) {
            spots.push(s.clone());
        }
    }
    let mut inks: Vec<PlateInk> = PROCESS_NAMES
        .iter()
        .enumerate()
        .map(|(k, name)| {
            let mut cmyk = [0.0f32; 4];
            cmyk[k] = 1.0;
            let ink_type = doc.print.inks.iter().find(|i| i.process && i.name.ends_with(name)).map(|i| i.ink_type.clone()).unwrap_or_default();
            let pct = |i: usize| if i == k { 100.0 } else { 0.0 };
            PlateInk { name: name.to_string(), kind: InkKind::Process, ink_type, cmyk, rgb: colors.cmyk_to_srgb(pct(0), pct(1), pct(2), pct(3)), aliases: Vec::new() }
        })
        .collect();
    for name in &spots {
        let swatch = doc.swatches.values().find(|s| s.model == ColorModel::Spot && s.name == *name && s.base.is_none());
        let (cmyk, rgb) = match swatch {
            Some(sw) => (spot_cmyk(sw, colors), colors.swatch_rgb(sw, 100.0, BlendSpace::Rgb).unwrap_or(sw.display_rgb)),
            None => ([0.0, 0.0, 0.0, 1.0], [0, 0, 0]),
        };
        let ink_type = doc.print.inks.iter().find(|i| i.name == *name).map(|i| i.ink_type.clone()).unwrap_or_default();
        inks.push(PlateInk { name: name.clone(), kind: InkKind::Spot, ink_type, cmyk, rgb, aliases: Vec::new() });
    }
    let index: HashMap<String, usize> = inks.iter().enumerate().map(|(i, k)| (k.name.clone(), i)).collect();
    // what the Ink Manager did to spot swatches
    let mut converted = Vec::new();
    for (name, u) in &walk.res.ink_use {
        if u.output.contains("converted to process") {
            converted.push(name.clone());
        } else if let Some(target) = u.output.strip_prefix("alias of ") {
            let target = match target {
                "Process Cyan" => "Cyan",
                "Process Magenta" => "Magenta",
                "Process Yellow" => "Yellow",
                "Process Black" => "Black",
                t => t,
            };
            if let Some(&p) = index.get(target) {
                inks[p].aliases.push(name.clone());
            }
        }
    }
    let pages = pages
        .into_iter()
        .map(|(_, f)| {
            let mut v: Vec<usize> = if f.process { vec![0, 1, 2, 3] } else { Vec::new() };
            v.extend(f.spots.iter().filter_map(|s| index.get(s).copied()));
            v
        })
        .collect();
    DocInks { inks, pages, converted, index }
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/// CMYK (0–1) of a spot ink's full coverage for simulating it: its CMYK alternate, or its Lab
/// or RGB alternate through the document profiles.
fn spot_cmyk(sw: &folio_model::Swatch, colors: &Colors) -> [f32; 4] {
    let v = |i: usize| sw.values.get(i).copied().unwrap_or(0.0);
    let ink = |c: [u8; 4]| c.map(|x| x as f32 / 255.0);
    match sw.space {
        ColorSpace::Cmyk if sw.values.len() >= 4 => [0, 1, 2, 3].map(|i| (v(i) / 100.0).clamp(0.0, 1.0) as f32),
        ColorSpace::Rgb if sw.values.len() >= 3 => ink(colors.rgb_to_cmyk8([0, 1, 2].map(|i| v(i).round().clamp(0.0, 255.0) as u8))),
        ColorSpace::Lab if sw.values.len() >= 3 => ink(colors.srgb_to_cmyk8(folio_color::lab_to_srgb([v(0), v(1), v(2)]))),
        ColorSpace::Gray if !sw.values.is_empty() => [0.0, 0.0, 0.0, (v(0) / 100.0).clamp(0.0, 1.0) as f32],
        _ => ink(colors.srgb_to_cmyk8(sw.display_rgb)),
    }
}
