//! Bleed and safety margins at page edges.

use super::Ctx;
use crate::colors::Inks;
use crate::geom::intersection;
use crate::messages::{edge_terms, msg, term};
use crate::rules::{BLEED_MISSING, SAFETY_MARGIN};
use crate::Severity;
use folio_model::{ItemKind, Paint, Rect};
use serde_json::{Value, json};

impl Ctx<'_> {
    fn bleed(&self) -> [f64; 4] {
        match self.o.bleed_pt {
            Some(b) => [b; 4],
            None => {
                let b = self.doc.print.bleed;
                if b.iter().all(|v| *v <= 0.0) { [3.0 * 72.0 / 25.4; 4] } else { b }
            }
        }
    }

    pub(super) fn check_bleed_and_safety(&mut self) {
        const TOUCH: f64 = 1.0; // within 1 pt of the trim counts as touching
        const REACH: f64 = 0.5; // reaching the bleed within 0.5 pt is enough
        let bleed = self.bleed();
        let safety = self.o.safety_margin_pt;
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if !p.prints() || p.clipped || p.anchored || matches!(p.item.kind, ItemKind::Group { .. }) {
                continue;
            }
            if p.page.is_none() {
                continue;
            }
            let pages = &self.g.spreads[p.spread].pages;
            let Some(frame_r) = p.visual else { continue };
            // What prints: a fill or stroke in ink spans the frame; a placed graphic only its
            // own area inside the frame (a trim-size ad in a bleed-size frame still leaves
            // paper at the edge); pasted-in items the frame.
            let inked = |paint: &Paint| !matches!(self.inks.paint(paint), Inks::None | Inks::Paper) && !self.inks.paint(paint).is_white();
            let mut art: Option<Rect> = None;
            let mut add = |x: Rect| art = Some(art.map_or(x, |a: Rect| a.union(&x)));
            if inked(&p.item.fill) || (p.item.stroke.weight > 0.0 && inked(&p.item.stroke.paint)) || !p.item.clipped.is_empty() {
                add(frame_r);
            }
            if let Some(gr) = Self::graphic_of(p) {
                let gb = p.to_spread.then_inner(&gr.transform).apply_rect(&gr.bounds);
                if let Some(x) = intersection(&gb, &frame_r) {
                    add(x);
                }
            }
            let paints = art.is_some();
            let r = art.unwrap_or(frame_r);
            // every page the artwork is on is checked against its own trim (spread-wide
            // items bleed off the outer edge of each page)
            let boxes: Vec<&crate::geom::PageBox> = pages.iter().filter(|b| intersection(&r, &b.rect).is_some()).collect();
            for pb in boxes {
            let trim = pb.rect;
            // bleed per edge: top, bottom, left, right (inside/outside by page side)
            let (left_b, right_b) = if self.doc.print.facing_pages && pb.left_hand { (bleed[3], bleed[2]) } else { (bleed[2], bleed[3]) };
            let edges = [
                ("top", r.top, trim.top, bleed[0], false, r.left < trim.right && r.right > trim.left),
                ("bottom", r.bottom, trim.bottom, bleed[1], true, r.left < trim.right && r.right > trim.left),
                ("left", r.left, trim.left, left_b, false, r.top < trim.bottom && r.bottom > trim.top && !pb.spine_left),
                ("right", r.right, trim.right, right_b, true, r.top < trim.bottom && r.bottom > trim.top && !pb.spine_right),
            ];
            let page = if self.g.master(p) { None } else { Some(pb.index) };
            let master = self.master_data(p);
            let id = p.item.id.clone();
            let is_text = matches!(p.item.kind, ItemKind::TextFrame(_));
            if paints {
                let mut short = Vec::new();
                for (edge, v, t, b, far, overlaps) in edges {
                    if !overlaps || b <= 0.0 {
                        continue;
                    }
                    // distance outward past the trim (positive = into the bleed)
                    let out = if far { v - t } else { t - v };
                    if out > -TOUCH && out < b - REACH {
                        short.push(json!({"edge": edge, "beyondTrim": round2(out), "required": round2(b)}));
                    }
                }
                if !short.is_empty() {
                    let names: Vec<&str> = short.iter().filter_map(|s| s["edge"].as_str()).map(edge_ko).collect();
                    self.push(
                        BLEED_MISSING,
                        Severity::Error,
                        page,
                        Some(&id),
                        msg("preflight.bleed", format!("재단선에 닿았지만 도련까지 나가지 않음({})", names.join(", ")), json!({"edges":edge_terms(&short)})),
                        json!({"edges": short, "master": master}),
                    );
                }
            }
            // safety: content inside the page near the trim (not bleeding past it)
            if safety > 0.0 {
                let content = if is_text { self.text_extent(i) } else if paints { Some(r) } else { None };
                if let Some(c) = content {
                    // distance from each trim edge inward (negative: past the trim)
                    let gaps = [
                        ("top", c.top - trim.top, false),
                        ("bottom", trim.bottom - c.bottom, false),
                        ("left", c.left - trim.left, pb.spine_left),
                        ("right", trim.right - c.right, pb.spine_right),
                    ];
                    // per edge: an edge past the trim is bleed artwork (the bleed check owns it);
                    // text past the trim is cut off, so it counts
                    {
                        let near: Vec<Value> = gaps
                            .iter()
                            .filter(|(_, gap, spine)| !spine && *gap < safety - 0.01 && (is_text || *gap >= -0.01))
                            .map(|(e, gap, _)| json!({"edge": e, "distance": round2(*gap)}))
                            .collect();
                        if !near.is_empty() && intersection(&c, &trim).is_some() {
                            let names: Vec<&str> = near.iter().filter_map(|s| s["edge"].as_str()).map(edge_ko).collect();
                            let (sev, what) = if is_text { (Severity::Warning, "글자") } else { (Severity::Info, "개체") };
                            self.push(
                                SAFETY_MARGIN,
                                sev,
                                page,
                                Some(&id),
                                msg("preflight.safety", format!("{what}가 재단선에 너무 가까움({}, 기준 {:.1} mm)", names.join(", "), safety * 25.4 / 72.0), json!({"what":if is_text { term("preflight.text") } else { term("preflight.object") },"edges":edge_terms(&near),"margin":format!("{:.1}",safety * 25.4 / 72.0)})),
                                json!({"edges": near, "margin": round2(safety), "text": is_text, "master": master}),
                            );
                        }
                    }
                }
            }
            }
        }
    }

    /// Spread-space extent of the text a frame shows (glyph boxes approximated from point
    /// size: ascent 0.8 em, descent 0.2 em, last glyph 0.6 em wide).
    fn text_extent(&self, fi: usize) -> Option<Rect> {
        let p = &self.g.items[fi];
        let mut out: Option<Rect> = None;
        let mut add = |ft: &folio_model::layout::FrameText| {
            for l in &ft.lines {
                for gl in &l.glyphs {
                    let r = Rect { left: gl.x, top: gl.y - gl.size * 0.8, right: gl.x + gl.size * 0.6 * gl.hscale, bottom: gl.y + gl.size * 0.2 };
                    let r = p.to_spread.apply_rect(&r);
                    out = Some(out.map_or(r, |o| o.union(&r)));
                }
            }
        };
        if let Some(ft) = self.layout.frames.get(&p.item.id) {
            add(ft);
        }
        out
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn edge_ko(e: &str) -> &'static str {
    match e {
        "top" => "위",
        "bottom" => "아래",
        "left" => "왼쪽",
        _ => "오른쪽",
    }
}
