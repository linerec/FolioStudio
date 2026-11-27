//! Text paint, small text, strokes and overset stories.

use super::{Ctx, frames_showing, round1, visible_text};
use crate::colors::{Inks, paint_name};
use crate::messages::{msg, term};
use crate::rules::{SMALL_TEXT_INKS, BLACK_TEXT_KNOCKOUT, HAIRLINE, OVERSET_TEXT};
use crate::Severity;
use folio_model::Paint;
use serde_json::json;

impl<'d> Ctx<'d> {
    pub(super) fn check_text(&mut self) {
        let overprint_black = self.doc.print.overprint_black;
        let stories: Vec<(&'d str, &'d folio_model::Story)> = self.doc.stories.iter().map(|(k, v)| (k.as_str(), v)).collect();
        for (sid, story) in stories {
            let laid: Vec<(usize, usize, usize)> = self.laid.get(sid).cloned().unwrap_or_default();
            if laid.is_empty() {
                continue;
            }
            let mut off = 0;
            for para in &story.paragraphs {
                for run in &para.runs {
                    let n = run.text.chars().count();
                    let (start, end) = (off, off + n);
                    off += n;
                    if !visible_text(&run.text) {
                        continue;
                    }
                    let frames = frames_showing(&laid, start, end);
                    let Some(&fi) = frames.iter().find(|&&f| self.checked(&self.g.items[f])) else { continue };
                    let fp = &self.g.items[fi];
                    let page = self.page_of(fp);
                    let master = self.master_data(fp);
                    let frame_id = fp.item.id.clone();
                    let a = &run.attrs;
                    let size = a.point_size * a.vertical_scale.max(a.horizontal_scale) / 100.0;
                    if a.fill != Paint::None {
                        let inks = self.inks.paint(&a.fill);
                        let pname = paint_name(self.doc, &a.fill);
                        self.paint_checks(&inks, &pname, "글자", a.overprint_fill, page, &frame_id, &master, "text");
                        let is_black_swatch = matches!(&a.fill, Paint::Swatch { id, tint } if id == "Color/Black" && *tint >= 99.95)
                            && self.doc.swatches.get("Color/Black").is_some_and(|s| s.tint.is_none());
                        let solid_k = matches!(&inks, Inks::Process(v) if v[0] <= 0.0 && v[1] <= 0.0 && v[2] <= 0.0 && v[3] >= 99.95);
                        let overprints = a.overprint_fill || (is_black_swatch && overprint_black);
                        // small text that is hard to register: several inks, or one ink (a
                        // tint, a colour) that knocks out of what is below (InDesign's "type
                        // too small" with trap-safe checking); solid black is the next rule's
                        let multi = inks.ink_count() >= 2;
                        let knockout = !multi && !overprints && !solid_k && inks.ink_count() == 1;
                        if size < self.o.small_text_pt && (multi || knockout) {
                            let rich = matches!(&inks, Inks::Process(v) if v[3] >= 60.0) && multi;
                            let kind = if rich { "리치 블랙" } else if multi { "여러 잉크" } else { "녹아웃되는 단색" };
                            let sz = round1(size);
                            let lim = self.o.small_text_pt;
                            let reason = if multi { "inks" } else { "knockout" };
                            self.push_grouped(
                                SMALL_TEXT_INKS,
                                Severity::Warning,
                                page,
                                &format!("{sid}:{pname}"),
                                &frame_id,
                                || msg("preflight.text.small", format!("작은 글자({sz} pt)에 {kind}: {pname}"), json!({"size":sz,"kind":if rich { term("preflight.richBlack") } else if multi { term("preflight.multipleInks") } else { term("preflight.knockoutInk") },"paint":pname})),
                                || json!({"story": sid, "paint": pname, "pointSize": sz, "threshold": lim, "inks": inks.ink_count(), "richBlack": rich, "reason": reason, "master": master}),
                            );
                        }
                        // black text that knocks out
                        if solid_k && !a.overprint_fill && !(is_black_swatch && overprint_black) {
                            let sev = if size < self.o.small_text_pt { Severity::Warning } else { Severity::Info };
                            self.push_grouped(
                                BLACK_TEXT_KNOCKOUT,
                                sev,
                                page,
                                &format!("{sid}:{pname}"),
                                &frame_id,
                                || msg("preflight.text.blackKnockout", format!("검정 글자가 중복인쇄되지 않음: {pname}"), json!({"paint":pname})),
                                || json!({"story": sid, "paint": pname, "pointSize": round1(size), "blackSwatch": is_black_swatch, "overprintBlackPreference": overprint_black, "master": master}),
                            );
                        }
                    }
                    if a.stroke != Paint::None && a.stroke_weight > 0.0 {
                        let inks = self.inks.paint(&a.stroke);
                        let pname = paint_name(self.doc, &a.stroke);
                        self.paint_checks(&inks, &pname, "글자 선", a.overprint_stroke, page, &frame_id, &master, "textStroke");
                        if a.stroke_weight < self.o.hairline_pt {
                            let w = a.stroke_weight;
                            let lim = self.o.hairline_pt;
                            self.push_grouped(
                                HAIRLINE,
                                Severity::Warning,
                                page,
                                &format!("text:{sid}"),
                                &frame_id,
                                || msg("preflight.text.hairline", format!("글자 외곽선이 너무 가늘음: {w:.3} pt"), json!({"weight":format!("{w:.3}")})),
                                || json!({"story": sid, "weight": w, "threshold": lim, "master": master}),
                            );
                        }
                    }
                }
                off += 1;
            }
        }
    }

    pub(super) fn check_overset(&mut self) {
        let overset = self.layout.overset.clone();
        for sid in overset {
            let frames = self.story_frames.get(sid.as_str()).cloned().unwrap_or_default();
            // last frame of the thread decides where it is reported
            let Some(&last) = frames.last() else { continue };
            let p = &self.g.items[last];
            if !self.checked(p) {
                continue;
            }
            let page = self.page_of(p);
            let master = self.master_data(p);
            let total = self.doc.stories.get(&sid).map(story_chars).unwrap_or(0);
            let shown = self.laid.get(sid.as_str()).and_then(|v| v.iter().map(|x| x.1).max()).unwrap_or(0);
            let hidden = total.saturating_sub(shown);
            let id = p.item.id.clone();
            let on_page = p.page.is_some();
            self.push(
                OVERSET_TEXT,
                Severity::Error,
                page,
                Some(&id),
                msg("preflight.text.overset", format!("넘친 텍스트: 약 {hidden}자"), json!({"count":hidden})),
                json!({"story": sid, "frame": id, "characters": total, "shown": shown.min(total), "overset": hidden, "onPage": on_page, "master": master}),
            );
        }
    }
}

fn story_chars(s: &folio_model::Story) -> usize {
    let text: usize = s.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.text.chars().count()).sum();
    text + s.paragraphs.len().saturating_sub(1)
}
