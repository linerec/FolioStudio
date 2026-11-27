//! Frame paints, hairlines and paint checks shared with text.

use super::{Ctx, round1};
use crate::colors::{Inks, paint_name};
use crate::geom::scales;
use crate::messages::{msg, part};
use crate::rules::{
    COLOR_SPACE,
    INK_COVERAGE,
    REGISTRATION_COLOR,
    WHITE_OVERPRINT,
    OVERPRINT_COLOR,
    HAIRLINE,
    UNSUPPORTED_EFFECT,
};
use crate::Severity;
use folio_model::{ItemKind, Paint};
use serde_json::{Value, json};

impl Ctx<'_> {
    pub(super) fn check_item_colors(&mut self) {
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if !self.checked(p) || matches!(p.item.kind, ItemKind::Group { .. }) {
                continue;
            }
            let page = self.page_of(p);
            let id = p.item.id.clone();
            let master = self.master_data(p);
            let t = p.to_spread;
            let item = p.item;
            let has_stroke = item.stroke.paint != Paint::None && item.stroke.weight > 0.0;
            let parts = [("fill", &item.fill, item.print.overprint_fill, true), ("stroke", &item.stroke.paint, item.print.overprint_stroke, has_stroke)];
            for (what, paint, overprint, used) in parts {
                if !used || *paint == Paint::None {
                    continue;
                }
                let inks = self.inks.paint(paint);
                let pname = paint_name(self.doc, paint);
                let label = if what == "fill" { "칠" } else { "선" };
                self.paint_checks(&inks, &pname, label, overprint, page, &id, &master, what);
            }
            if has_stroke {
                let (sx, sy) = scales(&t);
                let w = item.stroke.weight * (sx * sy).sqrt();
                if w < self.o.hairline_pt {
                    self.push(
                        HAIRLINE,
                        Severity::Warning,
                        page,
                        Some(&id),
                        msg("preflight.hairline", format!("너무 가는 선: {w:.3} pt (기준 {} pt)", self.o.hairline_pt), json!({"weight":format!("{w:.3}"),"threshold":self.o.hairline_pt})),
                        json!({"weight": round3(w), "storedWeight": item.stroke.weight, "threshold": self.o.hairline_pt, "master": master}),
                    );
                }
            }
            if !item.print.effects.is_empty() || matches!(&item.kind, ItemKind::Other { element } if !matches!(element.as_str(), "Movie" | "Sound")) {
                let what = match &item.kind {
                    ItemKind::Other { element } => vec![format!("page item {element}")],
                    _ => item.print.effects.clone(),
                };
                self.push(
                    UNSUPPORTED_EFFECT,
                    Severity::Warning,
                    page,
                    Some(&id),
                    msg("preflight.effect.unsupported", format!("출력에서 재현하지 못하는 효과: {}", what.join(", ")), json!({"effects":what})),
                    json!({"effects": what, "master": master}),
                );
            }
        }
    }

    // Paint facts and finding location stay explicit at each call site.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_checks(&mut self, inks: &Inks, pname: &str, label: &str, overprint: bool, page: Option<usize>, id: &str, master: &Value, what: &str) {
        let subject = format!("{what}:{pname}");
        if self.o.cmyk_output
            && let Some(space) = inks.device_space()
        {
            let s = format!("{space:?}");
            self.push_grouped(COLOR_SPACE, Severity::Warning, page, &subject, id, || msg("preflight.color.paint", format!("{s} 색 사용({label}): {pname}"), json!({"space":s,"part":part(what),"paint":pname})), || json!({"paint": pname, "space": s, "part": what, "master": master}));
        }
        if matches!(inks, Inks::Registration(_)) {
            self.push_grouped(REGISTRATION_COLOR, Severity::Error, page, &subject, id, || msg("preflight.color.registration", format!("맞춰찍기(Registration) 색을 내용에 사용({label})"), json!({"part":part(what)})), || json!({"paint": pname, "part": what, "master": master}));
        }
        if overprint {
            if inks.is_white() {
                self.push_grouped(WHITE_OVERPRINT, Severity::Error, page, &subject, id, || msg("preflight.color.whiteOverprint", format!("흰색 중복인쇄({label}): {pname} — 인쇄되지 않음"), json!({"part":part(what),"paint":pname})), || json!({"paint": pname, "part": what, "master": master}));
            } else if !inks.k_only() {
                self.push_grouped(OVERPRINT_COLOR, Severity::Warning, page, &subject, id, || msg("preflight.color.overprint", format!("검정 외 색의 중복인쇄({label}): {pname}"), json!({"part":part(what),"paint":pname})), || json!({"paint": pname, "part": what, "master": master}));
            }
        }
        // registration is reported by its own rule; its 400 % is not an ink-limit question
        if let Some(tac) = inks.coverage()
            && !matches!(inks, Inks::Registration(_))
            && tac > self.o.max_ink + 0.5
        {
            let max = self.o.max_ink;
            self.push_grouped(INK_COVERAGE, Severity::Error, page, &subject, id, || msg("preflight.ink.coverage", format!("잉크 총량 {tac:.0}% (기준 {max:.0}%): {pname}"), json!({"coverage":format!("{tac:.0}"),"max":format!("{max:.0}"),"paint":pname})), || json!({"paint": pname, "coverage": round1(tac), "max": max, "part": what, "master": master}));
        }
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
