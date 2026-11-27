//! Transparency over spot colours.

use super::{Ctx, visible_text};
use crate::geom::{Placed, intersection};
use crate::messages::{msg, term, transparency};
use crate::rules::{TRANSPARENCY_SPOT, TRANSPARENCY_USED};
use crate::Severity;
use folio_links::LinkEntry;
use folio_model::{BlendMode, ItemKind};
use serde_json::json;
use std::collections::HashMap;

impl Ctx<'_> {
    fn item_spots(&self, p: &Placed<'_>) -> Vec<String> {
        let mut v = Vec::new();
        self.inks.paint(&p.item.fill).spots(&mut v);
        if p.item.stroke.weight > 0.0 {
            self.inks.paint(&p.item.stroke.paint).spots(&mut v);
        }
        if let ItemKind::TextFrame(tf) = &p.item.kind
            && let Some(s) = self.doc.stories.get(&tf.story)
        {
            for r in s.paragraphs.iter().flat_map(|p| &p.runs) {
                if visible_text(&r.text) {
                    self.inks.paint(&r.attrs.fill).spots(&mut v);
                }
            }
        }
        v.sort();
        v.dedup();
        v
    }

    pub(super) fn check_transparency(&mut self) {
        let transparent: Vec<usize> = (0..self.g.items.len())
            .filter(|&i| {
                let p = &self.g.items[i];
                self.checked(p) && (p.item.opacity < 0.999 || p.item.blend_mode != BlendMode::Normal || !p.item.print.effects.is_empty())
            })
            .collect();
        for &t in &transparent {
            let tp = &self.g.items[t];
            let page = self.page_of(tp);
            let master = self.master_data(tp);
            let id = tp.item.id.clone();
            let mut what = Vec::new();
            if tp.item.opacity < 0.999 {
                what.push(format!("불투명도 {:.0}%", tp.item.opacity * 100.0));
            }
            if tp.item.blend_mode != BlendMode::Normal {
                what.push(format!("혼합 모드 {:?}", tp.item.blend_mode));
            }
            what.extend(tp.item.print.effects.iter().cloned());
            self.push(TRANSPARENCY_USED, Severity::Info, page, Some(&id), msg("preflight.transparency", format!("투명 효과: {}", what.join(", ")), json!({"effects":transparency(tp.item.opacity, &format!("{:?}",tp.item.blend_mode), &tp.item.print.effects)})), json!({"opacity": tp.item.opacity, "blendMode": format!("{:?}", tp.item.blend_mode), "effects": tp.item.print.effects, "master": master}));
        }
        // placed files with transparency of their own (alpha, PDF transparency groups / soft
        // masks), known when the file was found
        let by_owner: HashMap<&str, &LinkEntry> = self.links.links.iter().map(|e| (e.owner.as_str(), e)).collect();
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if !self.checked(p) {
                continue;
            }
            let Some(gr) = Self::graphic_of(p) else { continue };
            let Some(Some(Ok(info))) = by_owner.get(gr.id.as_str()).map(|e| e.info.as_ref()) else { continue };
            let pdf_transparency = info.vector_colors.as_ref().is_some_and(|v| v.transparency);
            if !(info.alpha || pdf_transparency) {
                continue;
            }
            let page = self.page_of(p);
            let master = self.master_data(p);
            let id = p.item.id.clone();
            let what = if pdf_transparency { "투명 효과가 있는 PDF" } else { "투명 영역(알파)이 있는 이미지" };
            self.push(TRANSPARENCY_USED, Severity::Info, page, Some(&id), msg("preflight.transparency.placed", format!("배치 파일의 투명: {what}"), json!({"what":if pdf_transparency { term("preflight.transparentPdf") } else { term("preflight.alphaImage") }})), json!({"graphic": gr.id, "alpha": info.alpha, "pdfTransparency": pdf_transparency, "master": master}));
        }
        if transparent.is_empty() {
            return;
        }
        // spot-using printing items per spread with bounds
        let mut spot_items: Vec<(usize, Vec<String>)> = Vec::new();
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if self.checked(p) {
                let s = self.item_spots(p);
                if !s.is_empty() {
                    spot_items.push((i, s));
                }
            }
        }
        for t in transparent {
            let tp = &self.g.items[t];
            let Some(tb) = tp.visual else { continue };
            let mut spots: Vec<String> = self.item_spots(tp);
            let mut below = Vec::new();
            for (si, names) in &spot_items {
                let s = &self.g.items[*si];
                if *si != t && s.spread == tp.spread && s.z < tp.z && s.visual.is_some_and(|b| intersection(&b, &tb).is_some()) {
                    spots.extend(names.iter().cloned());
                    below.push(s.item.id.clone());
                }
            }
            spots.sort();
            spots.dedup();
            if spots.is_empty() {
                continue;
            }
            let all_converted = spots.iter().all(|n| self.inks.ink(n).is_some_and(|i| i.convert_to_process));
            let sev = if all_converted { Severity::Info } else { Severity::Warning };
            let page = self.page_of(tp);
            let master = self.master_data(tp);
            let id = tp.item.id.clone();
            let how = if tp.item.opacity < 0.999 { format!("불투명도 {:.0}%", tp.item.opacity * 100.0) } else if tp.item.blend_mode != BlendMode::Normal { format!("혼합 모드 {:?}", tp.item.blend_mode) } else { "효과".into() };
            self.push(
                TRANSPARENCY_SPOT,
                sev,
                page,
                Some(&id),
                msg("preflight.transparency.spot", format!("별색과 겹친 투명 효과({how}): {}", spots.join(", ")), json!({"how":transparency(tp.item.opacity, &format!("{:?}",tp.item.blend_mode), &[]).into_iter().next().unwrap_or_else(|| term("preflight.effect")),"spots":spots})),
                json!({"spots": spots, "below": below, "opacity": tp.item.opacity, "blendMode": format!("{:?}", tp.item.blend_mode), "effects": tp.item.print.effects, "allConvertedToProcess": all_converted, "master": master}),
            );
        }
    }
}
