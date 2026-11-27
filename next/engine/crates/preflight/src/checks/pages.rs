//! Blank pages, page sizes and the document output profile.

use super::{Ctx, round1};
use crate::geom::intersection;
use crate::messages::msg;
use crate::rules::{BLANK_PAGE, NO_CMYK_PROFILE, PAGE_SIZES};
use crate::Severity;
use folio_model::{ItemKind, Rect};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl Ctx<'_> {
    /// Pages with nothing of their own that prints (master items do not count, as in
    /// InDesign's rule).
    pub(super) fn check_blank_pages(&mut self) {
        // a page is used when a printing item of a document spread overlaps it
        let mut used = BTreeSet::new();
        for p in &self.g.items {
            if !p.prints() || self.g.master(p) || matches!(p.item.kind, ItemKind::Group { .. }) {
                continue;
            }
            let Some(r) = p.visual.or(p.bounds) else {
                if let Some(pg) = p.page {
                    used.insert(pg);
                }
                continue;
            };
            // touching a page edge (e.g. starting exactly at the spine) counts, as in InDesign
            let r = Rect { left: r.left - 0.5, top: r.top - 0.5, right: r.right + 0.5, bottom: r.bottom + 0.5 };
            for b in &self.g.spreads[p.spread].pages {
                if intersection(&r, &b.rect).is_some() {
                    used.insert(b.index);
                }
            }
        }
        let pages: Vec<(usize, String)> = self.doc.spreads.iter().flat_map(|s| &s.pages).map(|p| (p.index, p.name.clone())).collect();
        for (index, name) in pages {
            if !used.contains(&index) {
                self.push(BLANK_PAGE, Severity::Info, Some(index), None, msg("preflight.page.blank", format!("빈 쪽: {name}"), json!({"name":name})), json!({"page": name}));
            }
        }
    }

    pub(super) fn check_document(&mut self) {
        if self.o.cmyk_output && self.doc.print.cmyk_profile.is_none() {
            self.push(NO_CMYK_PROFILE, Severity::Warning, None, None, msg("preflight.profile.missing", "문서에 CMYK 출력 프로파일이 지정되지 않음".into(), json!({})), json!({"rgbProfile": self.doc.print.rgb_profile}));
        }
        let mut sizes: BTreeMap<(i64, i64), Vec<String>> = BTreeMap::new();
        for s in &self.doc.spreads {
            for p in &s.pages {
                let r = p.transform.apply_rect(&p.bounds);
                sizes.entry(((r.width() * 100.0).round() as i64, (r.height() * 100.0).round() as i64)).or_default().push(p.name.clone());
            }
        }
        if sizes.len() > 1 {
            let list: Vec<Value> = sizes
                .iter()
                .map(|((w, h), pages)| json!({"width": *w as f64 / 100.0, "height": *h as f64 / 100.0, "widthMm": round1(*w as f64 / 100.0 * 25.4 / 72.0), "heightMm": round1(*h as f64 / 100.0 * 25.4 / 72.0), "pages": pages}))
                .collect();
            let desc: Vec<String> = sizes.keys().map(|(w, h)| format!("{:.0}×{:.0} mm", *w as f64 / 100.0 * 25.4 / 72.0, *h as f64 / 100.0 * 25.4 / 72.0)).collect();
            self.push(PAGE_SIZES, Severity::Warning, None, None, msg("preflight.page.sizes", format!("쪽 크기가 {}가지: {}", sizes.len(), desc.join(", ")), json!({"count":sizes.len(),"sizes":desc})), json!({"sizes": list}));
        }
    }
}
