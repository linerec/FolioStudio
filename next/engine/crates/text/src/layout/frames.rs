//! Find text frame chains and measure their columns.

use super::*;

fn master_prefix(spread: &Spread) -> Option<String> {
    let name = spread.name.as_deref()?;
    Some(name.split('-').next().unwrap_or(name).to_string())
}

/// Bounds of an item in its parent's coordinates (groups: union of their members).
pub(super) fn item_bounds(item: &PageItem) -> Option<Rect> {
    let inner = match &item.kind {
        ItemKind::Group { children } => children.iter().filter_map(item_bounds).reduce(|a, b| a.union(&b)),
        _ => paths_bounds(&item.paths),
    }?;
    Some(item.transform.apply_rect(&inner))
}

/// How far an item's visible stroke reaches outside its path.
pub(crate) fn stroke_outset(item: &PageItem) -> f64 {
    if matches!(item.stroke.paint, folio_model::Paint::None) || item.stroke.weight <= 0.0 {
        return 0.0;
    }
    match item.stroke.alignment {
        folio_model::StrokeAlignment::Center => item.stroke.weight / 2.0,
        folio_model::StrokeAlignment::Outside => item.stroke.weight,
        folio_model::StrokeAlignment::Inside => 0.0,
    }
}

/// Visible bounds of an item in its parent's coordinates: its paths and the part of its
/// stroke outside them. A bounding-box wrap starts there (#347 u4df96: a 0.244 pt centred
/// stroke moves the text 0.122 pt; InDesign 2025 experiment: a 1 pt stroke, 0.5 pt).
pub(super) fn visible_bounds(item: &PageItem) -> Option<Rect> {
    let inner = match &item.kind {
        ItemKind::Group { children } => children.iter().filter_map(visible_bounds).reduce(|a, b| a.union(&b)),
        _ => {
            let b = paths_bounds(&item.paths)?;
            let outset = stroke_outset(item);
            Some(Rect { left: b.left - outset, top: b.top - outset, right: b.right + outset, bottom: b.bottom + outset })
        }
    }?;
    Some(item.transform.apply_rect(&inner))
}

impl<'d> DocumentFrames<'d> {
    pub fn new(doc: &'d Document) -> Self {
        Self::build(doc, None)
    }

    /// The frames with `item` in place of the document's item of the same id (a trial size).
    pub fn with_item(doc: &'d Document, item: &'d PageItem) -> Self {
        Self::build(doc, Some(item))
    }

    fn build(doc: &'d Document, replace: Option<&'d PageItem>) -> Self {
        let mut out = DocumentFrames {
            replace,
            layer_order: doc.layers.iter().map(|l| (l.id.as_str(), l.order)).collect(),
            facing: doc.print.facing_pages,
            vertical: doc.stories.values().filter(|s| s.vertical).map(|s| s.id.as_str()).collect(),
            ..DocumentFrames::default()
        };
        for (si, spread) in doc.spreads.iter().chain(&doc.master_spreads).enumerate() {
            let is_master = si >= doc.spreads.len();
            let master = if spread.name.is_some() { master_prefix(spread) } else { None };
            // Pages in spread coordinates.
            let pages: Vec<(Rect, String, Affine, [f64; 4], String)> = spread
                .pages
                .iter()
                .map(|p| {
                    (
                        p.transform.apply_rect(&p.bounds),
                        master.clone().unwrap_or_else(|| p.name.clone()),
                        p.transform,
                        p.margins,
                        p.section_marker.clone(),
                    )
                })
                .collect();
            for item in &spread.items {
                out.walk(item, Affine::IDENTITY, Some(&pages), Some((si, is_master)));
            }
        }
        // Frames anchored in stories.
        for story in doc.stories.values() {
            for run in story.all_runs() {
                for item in &run.inline {
                    out.walk(item, Affine::IDENTITY, None, None);
                }
            }
        }
        out.assign_wraps(doc.text_preferences.z_order_text_wrap);
        // Chains: from each frame without a predecessor along `next`.
        let mut starts: Vec<&'d str> = out
            .frames
            .iter()
            .filter(|(_, f)| f.frame.previous.as_deref().is_none_or(|p| !out.frames.contains_key(p)))
            .map(|(id, _)| *id)
            .collect();
        starts.sort_unstable();
        let mut used: HashSet<&'d str> = HashSet::new();
        for start in starts {
            let story = out.frames[start].frame.story.as_str();
            if out.chains.contains_key(story) {
                continue;
            }
            let mut chain = Vec::new();
            let mut cur = Some(start);
            while let Some(id) = cur {
                if !used.insert(id) {
                    break;
                }
                chain.push(id);
                cur = out.frames[id].frame.next.as_deref().and_then(|n| out.frames.get_key_value(n).map(|(k, _)| *k));
            }
            out.chains.insert(story, chain);
        }
        // Stories whose frames form a cycle: start anywhere.
        let mut rest: Vec<&'d str> = out.frames.keys().copied().filter(|id| !used.contains(id)).collect();
        rest.sort_unstable();
        for id in rest {
            let story = out.frames[id].frame.story.as_str();
            out.chains.entry(story).or_default().push(id);
        }
        out
    }

    fn walk(
        &mut self,
        item: &'d PageItem,
        parent: Affine,
        pages: Option<&[(Rect, String, Affine, [f64; 4], String)]>,
        spread: Option<(usize, bool)>,
    ) {
        let item = match self.replace {
            Some(r) if r.id == item.id => r,
            _ => item,
        };
        let to_spread = parent.then_inner(&item.transform);
        self.counter += 1;
        let z = (item.layer.as_deref().and_then(|l| self.layer_order.get(l)).copied().unwrap_or(0), self.counter);
        if let Some((si, _)) = spread
            && let Some(obstacle) = Obstacle::from_item(item, si, z, parent, to_spread)
        {
            self.obstacles.push(obstacle);
        }
        if let Some(tf) = item.text_frame() {
            let (page_name, verso, page_transform, page_in_spread, page_rect, margins, section_marker) = match pages {
                Some(pages) if !pages.is_empty() => {
                    let b = paths_bounds(&item.paths).map(|r| to_spread.apply_rect(&r)).unwrap_or_default();
                    let (cx, cy) = ((b.left + b.right) / 2.0, (b.top + b.bottom) / 2.0);
                    let (pi, page) = pages
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            let d = |r: &Rect| {
                                let dx = (r.left - cx).max(cx - r.right).max(0.0);
                                let dy = (r.top - cy).max(cy - r.bottom).max(0.0);
                                dx * dx + dy * dy
                            };
                            d(&a.0).total_cmp(&d(&b.0))
                        })
                        .unwrap();
                    (Some(page.1.clone()), (page.0.left + page.0.right) / 2.0 < 0.0, Some(page.2), Some(pi), Some(page.0), page.3, Some(page.4.clone()))
                }
                _ => (None, false, None, None, None, [0.0; 4], None),
            };
            let turned = self.vertical.contains(tf.story.as_str()) && item.text_path.is_none();
            self.frames.insert(
                item.id.as_str(),
                FrameRef {
                    item,
                    frame: tf,
                    turned,
                    page_name,
                    section_marker,
                    verso,
                    to_spread,
                    page_transform,
                    page_rect,
                    margins,
                    wraps: Vec::new(),
                    shape: own_shape(item, tf, to_spread, turned),
                    spread: spread.map(|s| s.0),
                    page_in_spread,
                    z,
                },
            );
        }
        if let ItemKind::Group { children } = &item.kind {
            for c in children {
                self.walk(c, to_spread, pages, spread);
            }
        }
        for c in &item.clipped {
            self.walk(c, to_spread, pages, spread);
        }
    }

    /// The spread a frame is on (`spreads`, then `master_spreads`), `None` when anchored.
    pub fn spread_of(&self, id: &str) -> Option<usize> {
        self.frames.get(id)?.spread
    }

    pub fn frame(&self, id: &str) -> Option<&FrameRef<'d>> {
        self.frames.get(id)
    }

    /// Frame ids of a story in thread order (empty when the story has no frame).
    pub fn chain(&self, story: &str) -> &[&'d str] {
        self.chains.get(story).map_or(&[], Vec::as_slice)
    }

    /// Stories that have at least one frame.
    pub fn stories(&self) -> impl Iterator<Item = &&'d str> {
        self.chains.keys()
    }

    /// Every text frame id.
    pub fn frame_ids(&self) -> impl Iterator<Item = &&'d str> {
        self.frames.keys()
    }
}

/// How far a frame's stroke reaches into it: insets count from the stroke's inner edge
/// (measured on #347: a 3 pt centred stroke moves the text 1.5 pt in).
pub(crate) fn stroke_inset(item: &PageItem) -> f64 {
    if matches!(item.stroke.paint, folio_model::Paint::None) || item.stroke.weight <= 0.0 {
        return 0.0;
    }
    match item.stroke.alignment {
        folio_model::StrokeAlignment::Center => item.stroke.weight / 2.0,
        folio_model::StrokeAlignment::Inside => item.stroke.weight,
        folio_model::StrokeAlignment::Outside => 0.0,
    }
}

pub(crate) fn columns_of(item: &PageItem, f: &TextFrame) -> Vec<Column> {
    if let Some(path) = &item.text_path {
        let offset = match path.path_alignment {
            folio_model::textpath::PathAlignment::Top => -item.stroke.weight / 2.0,
            folio_model::textpath::PathAlignment::Center => 0.0,
            folio_model::textpath::PathAlignment::Bottom => item.stroke.weight / 2.0,
        };
        let baseline = path.baseline + offset;
        let right = path.start + crate::textpath::width(item, path);
        return vec![Column { left: path.start, right, top: baseline, bottom: baseline, previous: None }];
    }
    let Some(b) = paths_bounds(&item.paths) else { return Vec::new() };
    let stroke = stroke_inset(item);
    let [it, il, ib, ir] = f.insets.map(|v| v + stroke);
    let (left, right, top, bottom) = (b.left + il, b.right - ir, b.top + it, b.bottom - ib);
    let n = f.columns.max(1) as usize;
    let gutter = f.gutter.max(0.0);
    let width = f.column_width.unwrap_or_else(|| ((right - left) - gutter * (n as f64 - 1.0)) / n as f64);
    (0..n)
        .map(|k| {
            let l = left + k as f64 * (width + gutter);
            Column { left: l, right: l + width, top, bottom, previous: None }
        })
        .collect()
}

/// The columns of a vertical frame in its virtual horizontal frame (`crate::vertical`): the frame's top to bottom is
/// the virtual left to right, its right edge the virtual top. The frame's columns stack from its top.
pub(crate) fn vertical_columns(item: &PageItem, f: &TextFrame) -> Vec<Column> {
    let Some(b) = paths_bounds(&item.paths) else { return Vec::new() };
    let stroke = stroke_inset(item);
    let [it, il, ib, ir] = f.insets.map(|v| v + stroke);
    let v = crate::vertical::virtual_rect(b);
    // frame top inset → virtual left, bottom → right, right inset → virtual top, left → bottom
    let (left, right, top, bottom) = (v.left + it, v.right - ib, v.top + ir, v.bottom - il);
    let n = f.columns.max(1) as usize;
    let gutter = f.gutter.max(0.0);
    let width = f.column_width.unwrap_or_else(|| ((right - left) - gutter * (n as f64 - 1.0)) / n as f64);
    (0..n)
        .map(|k| {
            let l = left + k as f64 * (width + gutter);
            Column { left: l, right: l + width, top, bottom, previous: None }
        })
        .collect()
}
