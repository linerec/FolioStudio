//! Document fonts and missing glyphs.

use super::{Ctx, frames_showing, visible_text};
use crate::messages::{font_face, msg, term};
use crate::rules::{FONT_MISSING, FONT_SUBSTITUTED, MISSING_GLYPH};
use crate::Severity;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};

impl Ctx<'_> {
    pub(super) fn check_fonts(&mut self) {
        // Where each font pair shows: first page, frames on pages, characters.
        struct Use {
            page: Option<usize>,
            story: String,
            chars: usize,
            frames: BTreeSet<usize>,
        }
        let mut uses: HashMap<(String, String), Use> = HashMap::new();
        for (sid, story) in &self.doc.stories {
            let laid = self.laid.get(sid.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            let mut off = 0;
            for p in &story.paragraphs {
                for r in &p.runs {
                    let n = r.text.chars().count();
                    if visible_text(&r.text) {
                        let key = (r.attrs.font_family.clone(), r.attrs.font_style.clone());
                        let shown: Vec<usize> = frames_showing(laid, off, off + n).into_iter().filter(|&fi| self.checked(&self.g.items[fi])).collect();
                        let e = uses.entry(key).or_insert(Use { page: None, story: sid.clone(), chars: 0, frames: BTreeSet::new() });
                        if !shown.is_empty() {
                            e.chars += n;
                            if e.page.is_none() {
                                e.page = self.page_of(&self.g.items[shown[0]]);
                                e.story = sid.clone();
                            }
                            e.frames.extend(shown);
                        }
                    }
                    off += n;
                }
                off += 1;
            }
        }
        let facts = self.o.fonts.clone();
        for (family, style) in self.layout.missing_fonts.clone() {
            let u = uses.get(&(family.clone(), style.clone()));
            let (page, story, chars) = u.map(|u| (u.page, u.story.clone(), u.chars)).unwrap_or((None, String::new(), 0));
            let frames: Vec<String> = u.map(|u| u.frames.iter().map(|&f| self.g.items[f].item.id.clone()).collect()).unwrap_or_default();
            let on_pages = !frames.is_empty();
            let saved = self.doc.font_table.iter().find(|f| f.family == family && f.style == style).map(|f| f.status.clone());
            let sub = facts.as_ref().and_then(|f| f.substituted.iter().find(|s| s.0 == family && s.1 == style)).map(|s| s.2.clone());
            let fallback = facts.as_ref().and_then(|f| f.missing.iter().find(|s| s.0 == family && s.1 == style)).map(|s| s.2.clone());
            let data = json!({"family": family, "style": style, "story": story, "characters": chars, "statusWhenSaved": saved,
                              "shownWith": sub.clone().or(fallback.clone()), "frames": frames, "count": frames.len().max(1), "onPages": on_pages});
            let item = frames.first().cloned().unwrap_or(story);
            let off_page = if on_pages { "" } else { " — 쪽에 보이는 글자에는 쓰이지 않음" };
            if let Some(face) = sub {
                let sev = if on_pages { Severity::Warning } else { Severity::Info };
                self.push(FONT_SUBSTITUTED, sev, page, Some(&item), msg("preflight.font.substituted", format!("글꼴 스타일 대체: {family} {style} → {face}{off_page}"), json!({"family":family,"style":style,"face":face,"offPage":if on_pages { json!("") } else { term("preflight.offPage") }})), data);
            } else {
                let sev = if on_pages { Severity::Error } else { Severity::Info };
                let shown = fallback.map(|f| format!(" ({f}로 표시)")).unwrap_or_default();
                self.push(FONT_MISSING, sev, page, Some(&item), msg("preflight.font.missing", format!("글꼴 없음: {family} {style}{shown}{off_page}"), json!({"family":family,"style":style,"shown":data["shownWith"].as_str().map(|f| json!({"messageId":"preflight.font.shown","messageArgs":{"face":font_face(f)}})),"offPage":if on_pages { json!("") } else { term("preflight.offPage") }})), data);
            }
        }
    }

    /// Characters set as .notdef boxes (InDesign Preflight "Missing glyphs"): one finding
    /// per story, font and frame showing them. Error on printing page items; text shown only
    /// on the pasteboard or masters, or only in overset, is information.
    pub(super) fn check_missing_glyphs(&mut self) {
        let layout = self.layout;
        for m in &layout.missing_glyphs {
            let laid = self.laid.get(m.story.as_str()).cloned().unwrap_or_default();
            // Characters per frame item (nested text: its anchor's frame).
            let mut per_frame: BTreeMap<usize, usize> = BTreeMap::new();
            for &(s, e) in &m.ranges {
                for &(ls, le, fi) in &laid {
                    let n = e.min(le).saturating_sub(s.max(ls));
                    if n > 0 {
                        *per_frame.entry(fi).or_default() += n;
                    }
                }
            }
            let shown: String = {
                let mut c: String = m.chars.chars().take(12).collect();
                if m.chars.chars().count() > 12 {
                    c.push('…');
                }
                c
            };
            let (family, style) = (&m.family, &m.style);
            let message = |n: usize| {
                if m.installed {
                    msg("preflight.glyph.installed", format!("글리프 없음: {family} {style}에 없는 글자 {n}자({shown}) — 빈 네모로 인쇄됨"), json!({"family":family,"style":style,"n":n,"shown":shown}))
                } else {
                    msg("preflight.glyph.missing", format!("글리프 없음: 설치된 글꼴 어디에도 없는 글자 {n}자({shown}) — {family} {style} 대체 글꼴의 빈 네모로 인쇄됨"), json!({"family":family,"style":style,"n":n,"shown":shown}))
                }
            };
            let data = |frame: Option<&str>, n: usize| {
                json!({"story": m.story, "family": family, "style": style, "installed": m.installed, "chars": m.chars,
                       "characters": n, "total": m.count, "frame": frame})
            };
            if per_frame.is_empty() {
                self.push(MISSING_GLYPH, Severity::Info, None, Some(&m.story), message(m.count), data(None, m.count));
                continue;
            }
            for (fi, n) in per_frame {
                let p = &self.g.items[fi];
                let sev = if self.checked(p) { Severity::Error } else { Severity::Info };
                let page = self.page_of(p);
                let id = p.item.id.clone();
                self.push(MISSING_GLYPH, sev, page, Some(&id), message(n), data(Some(&id), n));
            }
        }
    }
}
