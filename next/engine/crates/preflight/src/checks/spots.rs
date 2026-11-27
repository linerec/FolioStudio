//! Spot colours and ink manager settings.

use super::{Ctx, visible_text};
use crate::messages::msg;
use crate::rules::{SPOT_COLORS, SPOT_UNUSED, SPOT_LOOKS_PROCESS, INK_ALIAS, SPOT_TO_PROCESS};
use crate::Severity;
use folio_model::{ColorModel, Paint};
use serde_json::{Value, json};
use std::collections::BTreeMap;

impl Ctx<'_> {
    pub(super) fn check_spots(&mut self) {
        // Uses of every swatch on printing items (items, text, gradients, tints).
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        let note = |p: &Paint, used: &mut BTreeMap<String, usize>| {
            let mut names = Vec::new();
            self.inks.paint(p).spots(&mut names);
            for n in names {
                *used.entry(n).or_default() += 1;
            }
        };
        for p in &self.g.items {
            if !p.prints() {
                continue;
            }
            note(&p.item.fill, &mut used);
            if p.item.stroke.weight > 0.0 {
                note(&p.item.stroke.paint, &mut used);
            }
        }
        for (sid, story) in &self.doc.stories {
            if !self.laid.contains_key(sid.as_str()) {
                continue;
            }
            for r in story.paragraphs.iter().flat_map(|p| &p.runs) {
                if visible_text(&r.text) {
                    note(&r.attrs.fill, &mut used);
                    if r.attrs.stroke_weight > 0.0 {
                        note(&r.attrs.stroke, &mut used);
                    }
                }
            }
        }
        // Spots in placed files.
        let mut placed: BTreeMap<String, usize> = BTreeMap::new();
        for e in &self.links.links {
            if let Some(Ok(i)) = &e.info
                && let Some(vc) = &i.vector_colors
            {
                for s in &vc.spots {
                    *placed.entry(s.clone()).or_default() += 1;
                }
            }
        }
        let spot_swatches: Vec<(&str, &str, bool)> = self
            .doc
            .swatches
            .values()
            .filter(|s| s.model == ColorModel::Spot && s.base.is_none())
            .map(|s| (s.id.as_str(), s.name.as_str(), self.inks.ink(&s.name).is_some_and(|i| i.convert_to_process)))
            .collect();
        let list: Vec<Value> = spot_swatches
            .iter()
            .map(|(id, n, conv)| json!({"swatch": id, "name": n, "uses": used.get(*n).copied().unwrap_or(0), "convertToProcess": conv}))
            .chain(placed.iter().filter(|(n, _)| !spot_swatches.iter().any(|s| s.1 == n.as_str())).map(|(n, c)| json!({"name": n, "placedFiles": c})))
            .collect();
        if !list.is_empty() {
            let separating: Vec<&str> = spot_swatches.iter().filter(|s| !s.2 && used.get(s.1).copied().unwrap_or(0) > 0).map(|s| s.1).collect();
            let msg = if separating.is_empty() {
                msg("preflight.spots.process", format!("별색 {}개 (모두 원색으로 분판되거나 쓰이지 않음)", list.len()), json!({"count":list.len()}))
            } else {
                msg("preflight.spots.separate", format!("별색 {}개, 별도 판으로 출력: {}", list.len(), separating.join(", ")), json!({"count":list.len(),"separating":separating}))
            };
            self.push(SPOT_COLORS, Severity::Info, None, None, msg, json!({"spots": list, "separating": separating}));
        }
        for (id, name, _) in &spot_swatches {
            if used.get(*name).copied().unwrap_or(0) == 0 {
                self.push(SPOT_UNUSED, Severity::Info, None, Some(id), msg("preflight.spot.unused", format!("쓰지 않는 별색: {name}"), json!({"name":name})), json!({"swatch": id, "name": name}));
            }
            if looks_process(name) {
                self.push(
                    SPOT_LOOKS_PROCESS,
                    Severity::Warning,
                    None,
                    Some(id),
                    msg("preflight.spot.processLike", format!("원색처럼 보이는 별색: {name}"), json!({"name":name})),
                    json!({"swatch": id, "name": name, "uses": used.get(*name).copied().unwrap_or(0)}),
                );
            }
        }
        let inks = self.doc.print.inks.clone();
        let mut converted = Vec::new();
        for ink in &inks {
            if ink.process {
                continue;
            }
            if let Some(a) = &ink.alias {
                self.push(INK_ALIAS, Severity::Warning, None, None, msg("preflight.ink.alias", format!("잉크 관리자 별칭: {} → {a}", ink.name), json!({"name":ink.name,"alias":a})), json!({"ink": ink.name, "alias": a}));
            }
            if ink.convert_to_process {
                converted.push(ink.name.clone());
            }
        }
        if !converted.is_empty() {
            self.push(
                SPOT_TO_PROCESS,
                Severity::Info,
                None,
                None,
                msg("preflight.spots.converted", format!("잉크 관리자가 별색 {}개를 원색으로 변환", converted.len()), json!({"count":converted.len()})),
                json!({"inks": converted}),
            );
        }
    }
}

/// Spot names that are probably process colours by mistake.
fn looks_process(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    let n = n.trim_start_matches("process ").trim();
    const EXACT: &[&str] = &["cyan", "magenta", "yellow", "black", "c", "m", "y", "k", "rich black", "4c", "4 color", "cmyk", "먹", "검정", "블랙", "빨강", "노랑", "파랑"];
    if EXACT.contains(&n) {
        return true;
    }
    // InDesign's automatic process names "C=0 M=100 Y=0 K=0", "K100", "먹100"
    let compact: String = n.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.starts_with("c=") && compact.contains("m=") && compact.contains("y=") && compact.contains("k=") {
        return true;
    }
    for prefix in ["k", "먹", "black", "cyan", "magenta", "yellow"] {
        if let Some(rest) = compact.strip_prefix(prefix)
            && !rest.is_empty()
            && rest.trim_end_matches('%').chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}
