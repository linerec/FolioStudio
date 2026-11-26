//! The checks. Each `check_*` adds findings for one group of rules.

use crate::messages::{Message, msg, term, part, edge_terms, transparency, font_face};
use crate::colors::{InkTable, Inks, paint_name};
use crate::geom::{Doc, Placed, intersection, scales};
use crate::rules::*;
use crate::{Finding, Options, Severity};
use folio_links::{ColorSpace as LinkSpace, How, LinkEntry, LinkReport, LinkStatus};
use folio_model::layout::DocumentLayout;
use folio_model::{BlendMode, ColorModel, Document, Graphic, ItemKind, Paint, Rect};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Ctx<'d> {
    pub doc: &'d Document,
    pub layout: &'d DocumentLayout,
    pub links: &'d LinkReport,
    pub o: &'d Options,
    pub g: &'d Doc<'d>,
    pub inks: InkTable<'d>,
    pub findings: Vec<Finding>,
    pub limitations: Vec<String>,
    /// Graphic id → effective resolution (see [`crate::ImageFact`]).
    pub images: BTreeMap<String, crate::ImageFact>,
    /// (rule, page, subject) → finding index, for aggregated findings.
    groups: HashMap<(String, Option<usize>, String), usize>,
    /// Story id → text frame item indices in thread order.
    story_frames: HashMap<&'d str, Vec<usize>>,
    /// Story id → laid character ranges (start, end, frame item index), sorted.
    laid: HashMap<&'d str, Vec<(usize, usize, usize)>>,
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Where a run of text is: frame item indices whose lines show part of `[start, end)`.
fn frames_showing(laid: &[(usize, usize, usize)], start: usize, end: usize) -> Vec<usize> {
    let mut out: Vec<usize> = laid.iter().filter(|(s, e, _)| *s < end && start < *e).map(|x| x.2).collect();
    out.dedup();
    out
}

fn visible_text(s: &str) -> bool {
    s.chars().any(|c| !c.is_whitespace() && c != '\u{FFFC}' && !c.is_control())
}

impl<'d> Ctx<'d> {
    pub fn new(g: &'d Doc<'d>, doc: &'d Document, layout: &'d DocumentLayout, links: &'d LinkReport, o: &'d Options) -> Self {
        let mut story_frames: HashMap<&'d str, Vec<usize>> = HashMap::new();
        for (i, p) in g.items.iter().enumerate() {
            if let ItemKind::TextFrame(tf) = &p.item.kind {
                story_frames.entry(tf.story.as_str()).or_default().push(i);
            }
        }
        // thread order: head (no previous in the set), then `next`
        for frames in story_frames.values_mut() {
            if frames.len() < 2 {
                continue;
            }
            let ids: HashMap<&str, usize> = frames.iter().map(|&i| (g.items[i].item.id.as_str(), i)).collect();
            let tf = |i: usize| match &g.items[i].item.kind {
                ItemKind::TextFrame(t) => Some(t),
                _ => None,
            };
            let head = frames.iter().copied().find(|&i| tf(i).and_then(|t| t.previous.as_deref()).is_none_or(|p| !ids.contains_key(p)));
            let mut ordered = Vec::with_capacity(frames.len());
            let mut cur = head;
            while let Some(c) = cur {
                if ordered.contains(&c) {
                    break;
                }
                ordered.push(c);
                cur = tf(c).and_then(|t| t.next.as_deref()).and_then(|n| ids.get(n).copied());
            }
            for &f in frames.iter() {
                if !ordered.contains(&f) {
                    ordered.push(f);
                }
            }
            *frames = ordered;
        }
        let mut laid: HashMap<&'d str, Vec<(usize, usize, usize)>> = HashMap::new();
        for (fid, ft) in &layout.frames {
            let Some(&fi) = g.by_id.get(fid.as_str()) else { continue };
            let ItemKind::TextFrame(tf) = &g.items[fi].item.kind else { continue };
            let v = laid.entry(tf.story.as_str()).or_default();
            for l in &ft.lines {
                v.push((l.start, l.end.max(l.start + 1), fi));
            }
        }
        for v in laid.values_mut() {
            v.sort_unstable();
        }
        Ctx {
            doc,
            layout,
            links,
            o,
            inks: InkTable::new(doc),
            g,
            findings: Vec::new(),
            limitations: Vec::new(),
            images: BTreeMap::new(),
            groups: HashMap::new(),
            story_frames,
            laid,
        }
    }

    pub fn run(&mut self) {
        self.check_fonts();
        self.check_missing_glyphs();
        self.check_links_and_images();
        self.check_item_colors();
        self.check_text();
        self.check_spots();
        self.check_overset();
        self.check_bleed_and_safety();
        self.check_visibility();
        self.check_empty_frames();
        self.check_transparency();
        self.check_placed_fonts();
        self.check_blank_pages();
        self.check_document();
        if self.o.image_ink {
            crate::image_ink::check(self);
        }
    }

    // -----------------------------------------------------------------------------------
    // Output helpers

    fn page_name(&self, page: Option<usize>) -> Option<String> {
        page.and_then(|p| self.g.page_box(p)).map(|b| b.name.clone())
    }

    pub fn push(&mut self, rule: &str, severity: Severity, page: Option<usize>, item: Option<&str>, message: Message, data: Value) {
        // items on masters: the master page's name (e.g. "A"), with no document page index
        let page_name = self.page_name(page).or_else(|| {
            let p = &self.g.items[*self.g.by_id.get(item?)?];
            let s = &self.g.spreads[p.spread];
            if !s.master {
                return None;
            }
            s.pages.iter().find(|b| Some(b.index) == p.page).map(|b| b.name.clone())
        });
        self.findings.push(Finding { rule: rule.into(), severity, page, page_name, item: item.map(str::to_owned), message: message.text, message_id: message.id, message_args: message.args, data });
    }

    /// One finding per (rule, page, subject); later items are added to `data.items`.
    // Each argument supplies a separate finding field or its lazy payload.
    #[allow(clippy::too_many_arguments)]
    pub fn push_grouped(&mut self, rule: &str, severity: Severity, page: Option<usize>, subject: &str, item: &str, message: impl FnOnce() -> Message, data: impl FnOnce() -> Value) {
        let key = (rule.to_owned(), page, subject.to_owned());
        if let Some(&i) = self.groups.get(&key) {
            let f = &mut self.findings[i];
            if let Some(items) = f.data.get_mut("items").and_then(Value::as_array_mut)
                && !items.iter().any(|v| v.as_str() == Some(item))
            {
                items.push(json!(item));
            }
            if let Some(c) = f.data.get_mut("count") {
                *c = json!(c.as_u64().unwrap_or(1) + 1);
            }
            if f.severity > severity {
                f.severity = severity;
            }
            return;
        }
        let mut d = data();
        if let Value::Object(m) = &mut d {
            m.insert("items".into(), json!([item]));
            m.insert("count".into(), json!(1));
        }
        self.groups.insert(key, self.findings.len());
        self.push(rule, severity, page, Some(item), message(), d);
    }

    /// Page to report an item on (document page; masters and pasteboard → None).
    fn page_of(&self, p: &Placed<'_>) -> Option<usize> {
        self.g.doc_page(p)
    }

    /// Items the print checks look at: printing, on a page (or anywhere when the pasteboard
    /// is included).
    fn checked(&self, p: &Placed<'_>) -> bool {
        crate::scope::checked(p,self.g,self.doc,self.o)
    }

    fn master_data(&self, p: &Placed<'_>) -> Value {
        match self.g.master_name(p) {
            Some(m) => json!(m),
            None => Value::Null,
        }
    }

    // -----------------------------------------------------------------------------------
    // Fonts

    fn check_fonts(&mut self) {
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
    fn check_missing_glyphs(&mut self) {
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

    // -----------------------------------------------------------------------------------
    // Links, resolution, image colour spaces

    fn graphic_of<'a>(p: &'a Placed<'_>) -> Option<&'a Graphic> {
        match &p.item.kind {
            ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => Some(g),
            ItemKind::Other { .. } => p.item.print.poster.as_deref(),
            _ => None,
        }
    }

    fn check_links_and_images(&mut self) {
        let by_owner: HashMap<&str, &LinkEntry> = self.links.links.iter().map(|e| (e.owner.as_str(), e)).collect();
        let mut seen_media = BTreeSet::new();
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if !self.checked(p) {
                continue;
            }
            let page = self.page_of(p);
            let master = self.master_data(p);
            let frame_id = p.item.id.clone();
            // movies / sounds: the media file itself
            if let Some(ml) = &p.item.print.media_link
                && let Some(e) = by_owner.get(p.item.id.as_str())
                && seen_media.insert(p.item.id.clone())
            {
                let e: &LinkEntry = e;
                self.link_findings(e, page, &frame_id, &master, &ml.uri.clone());
            }
            let Some(gr) = Self::graphic_of(p) else { continue };
            let page = self.graphic_page(p, gr);
            let entry = by_owner.get(gr.id.as_str()).copied();
            if let Some(e) = entry {
                let uri = e.recorded.uri.clone();
                self.link_findings(e, page, &frame_id, &master, &uri);
            }
            let to_spread = self.g.items[i].to_spread;
            self.image_checks(gr, entry, page, &frame_id, &master, &to_spread);
        }
    }

    /// Page of a placed graphic: the page under the graphic's centre (as InDesign's
    /// preflight reports links and images), else the frame's page.
    pub fn graphic_page(&self, p: &Placed<'_>, gr: &Graphic) -> Option<usize> {
        if self.g.master(p) {
            return None;
        }
        let b = p.to_spread.then_inner(&gr.transform).apply_rect(&gr.bounds);
        let (cx, cy) = ((b.left + b.right) / 2.0, (b.top + b.bottom) / 2.0);
        self.g.spreads[p.spread]
            .pages
            .iter()
            .find(|pg| cx >= pg.rect.left && cx < pg.rect.right && cy >= pg.rect.top - 1000.0 && cy < pg.rect.bottom + 1000.0)
            .map(|pg| pg.index)
            .filter(|_| p.page.is_some())
            .or(p.page)
    }

    fn link_findings(&mut self, e: &LinkEntry, page: Option<usize>, frame: &str, master: &Value, uri: &str) {
        let name = folio_links::parse_uri(uri).and_then(|l| l.file_name().map(str::to_owned)).unwrap_or_else(|| uri.to_owned());
        let base = json!({"graphic": e.owner, "frame": frame, "uri": uri, "file": name, "master": master, "element": e.element});
        let with = |extra: Value| {
            let mut d = base.clone();
            if let (Value::Object(m), Value::Object(x)) = (&mut d, extra) {
                m.extend(x);
            }
            d
        };
        if e.embedded {
            self.push(LINK_EMBEDDED, Severity::Info, page, Some(frame), msg("preflight.link.embedded", format!("내장된 파일: {name}"), json!({"name":name})), with(json!({})));
            return;
        }
        match &e.status {
            Some(LinkStatus::Missing) | None => {
                // Without the file only the low-resolution preview stored in the document can
                // be printed; for placed PDF/AI/EPS that also turns vector art into pixels.
                let what = if matches!(e.element.as_str(), "PDF" | "EPS" | "ImportedPage") {
                    "문서 안의 저해상도 미리보기 그림으로 인쇄됨(글자·선이 흐려짐)"
                } else {
                    "문서 안의 저해상도 미리보기로 인쇄됨"
                };
                self.push(LINK_MISSING, Severity::Error, page, Some(frame), msg("preflight.link.missing", format!("연결 파일 없음: {name} — {what}"), json!({"name":name,"what":if matches!(e.element.as_str(), "PDF" | "EPS" | "ImportedPage") { term("preflight.preview.vector") } else { term("preflight.preview.image") }})), with(json!({})));
            }
            Some(LinkStatus::Ambiguous(c)) => {
                self.push(
                    LINK_AMBIGUOUS,
                    Severity::Error,
                    page,
                    Some(frame),
                    msg("preflight.link.ambiguous", format!("같은 이름의 파일이 {}개 있어 고를 수 없음: {name}", c.len()), json!({"count":c.len(),"name":name})),
                    with(json!({"candidates": c, "identityMatches": e.identity_matches})),
                );
            }
            Some(LinkStatus::Found { path, how }) => {
                // a Photoshop EPS (its picture in `%ImageData`) prints from that picture at its full resolution (build-a2)
                let photoshop = matches!(&e.info, Some(Ok(i)) if i.width > 0 && i.height > 0);
                if e.element == "EPS" && !photoshop {
                    self.push(
                        LINK_EPS_PREVIEW,
                        Severity::Warning,
                        page,
                        Some(frame),
                        msg("preflight.link.epsPreview", format!("EPS 는 파일에 든 미리 보기 그림으로 화면과 PDF 에 나옵니다: {name} — 인쇄용은 PDF 로 바꿔 놓거나 IDML 로 넘기세요"), json!({"name":name})),
                        with(json!({"path": path})),
                    );
                }
                match how {
                    How::Search { score: 0 } => self.push(
                        LINK_NAME_ONLY,
                        Severity::Warning,
                        page,
                        Some(frame),
                        msg("preflight.link.nameOnly", format!("파일 이름만 같은 파일로 연결함(폴더가 다름): {name}"), json!({"name":name})),
                        with(json!({"path": path})),
                    ),
                    How::Original => {}
                    other => self.push(
                        LINK_RELOCATED,
                        Severity::Info,
                        page,
                        Some(frame),
                        msg("preflight.link.relocated", format!("다른 위치에서 찾음: {name}"), json!({"name":name})),
                        with(json!({"path": path, "how": other})),
                    ),
                }
                if let Some(m) = &e.modified {
                    self.push(
                        LINK_MODIFIED,
                        Severity::Error,
                        page,
                        Some(frame),
                        msg("preflight.link.modified", format!("연결 파일이 배치 후 바뀜: {name}"), json!({"name":name})),
                        with(json!({"path": path, "modified": m})),
                    );
                }
                if let Some(Err(err)) = &e.info
                    && !matches!(e.element.as_str(), "Movie" | "Sound")
                {
                    self.push(
                        LINK_UNREADABLE,
                        Severity::Warning,
                        page,
                        Some(frame),
                        msg("preflight.link.unreadable", format!("연결 파일을 읽을 수 없음: {name}"), json!({"name":name})),
                        with(json!({"path": path, "error": err.to_string()})),
                    );
                }
            }
        }
    }

    /// Effective resolution and colour space of a placed graphic.
    fn image_checks(&mut self, gr: &Graphic, entry: Option<&LinkEntry>, page: Option<usize>, frame: &str, master: &Value, to_spread: &folio_model::Affine) {
        let info = entry.and_then(|e| e.info.as_ref()).and_then(|r| r.as_ref().ok());
        let full = to_spread.then_inner(&gr.transform);
        let (sx, sy) = scales(&full);
        let name = entry
            .and_then(|e| folio_links::parse_uri(&e.recorded.uri))
            .and_then(|l| l.file_name().map(str::to_owned))
            .unwrap_or_else(|| gr.id.clone());
        let raster_from_idml = !matches!(gr.element.as_str(), "PDF" | "EPS" | "ImportedPage" | "WMF" | "PICT" | "SVG");
        // Pixel size: the file's header, else IDML's actual ppi.
        let bw = gr.bounds.width().abs();
        let bh = gr.bounds.height().abs();
        let (pixels, bitmap, source) = match info {
            Some(i) if !i.vector => {
                let (mut w, mut h) = (i.width as f64, i.height as f64);
                // EXIF rotation or a rotated placement: match the aspect of the bounds
                if bw > 0.0 && bh > 0.0 && ((w / h) - (bw / bh)).abs() > ((h / w) - (bw / bh)).abs() {
                    std::mem::swap(&mut w, &mut h);
                }
                (Some((w, h)), i.bits == 1 && i.color_space == LinkSpace::Gray, "file")
            }
            Some(_) => (None, false, "file"),
            None if raster_from_idml => {
                let px = gr.actual_ppi.filter(|a| a.0 > 0.0 && a.1 > 0.0).map(|a| (bw * a.0 / 72.0, bh * a.1 / 72.0));
                let bitmap = gr.image_space.as_deref() == Some("Black and White");
                (px, bitmap, "idml")
            }
            None => (None, false, "idml"),
        };
        if let Some((w, h)) = pixels
            && bw > 0.0
            && bh > 0.0
            && sx > 0.0
            && sy > 0.0
        {
            let ppi_x = w / (bw * sx / 72.0);
            let ppi_y = h / (bh * sy / 72.0);
            // InDesign shows and records whole numbers (DOM `effectivePpi`, IDML `EffectivePpi`, measured 2026-11-19 in
            // artifacts/pdfxcheck-c2-20261023): 299.9994 ppi is "300", so it passes a 300 ppi minimum.
            let ppi = ppi_x.min(ppi_y).round();
            self.images.insert(gr.id.clone(), crate::ImageFact { frame: frame.to_owned(), page, ppi: [round1(ppi_x), round1(ppi_y)] });
            let (warn, err) = if bitmap { (self.o.bitmap_ppi_warn, self.o.bitmap_ppi_error) } else { (self.o.image_ppi_warn, self.o.image_ppi_error) };
            if ppi < warn {
                let sev = if ppi < err { Severity::Error } else { Severity::Warning };
                let kind = if bitmap { "1비트 이미지" } else { "이미지" };
                self.push(
                    IMAGE_RESOLUTION,
                    sev,
                    page,
                    Some(frame),
                    msg("preflight.image.resolution", format!("{kind} 해상도 낮음: {name} {ppi:.0} ppi (기준 {warn:.0})"), json!({"kind":if bitmap { term("preflight.bitmap") } else { term("preflight.image") },"name":name,"ppi":format!("{ppi:.0}"),"warn":format!("{warn:.0}")})),
                    json!({"graphic": gr.id, "file": name, "effectivePpi": [round1(ppi_x), round1(ppi_y)], "pixels": [w.round(), h.round()],
                           "threshold": warn, "errorBelow": err, "bitmap": bitmap, "source": source,
                           "recordedEffectivePpi": gr.effective_ppi, "master": master}),
                );
            }
        }
        if !self.o.cmyk_output {
            return;
        }
        // Colour space: file header first, IDML's record otherwise.
        let mut spaces: Vec<&str> = Vec::new();
        let mut inside = false;
        match info {
            Some(i) if i.vector => {
                if let Some(vc) = &i.vector_colors {
                    for s in vc.spaces.iter().chain(&vc.image_spaces) {
                        match s {
                            LinkSpace::Rgb => spaces.push("RGB"),
                            LinkSpace::Lab => spaces.push("Lab"),
                            _ => {}
                        }
                    }
                    inside = true;
                }
            }
            Some(i) => match i.color_space {
                LinkSpace::Rgb => spaces.push("RGB"),
                LinkSpace::Lab => spaces.push("Lab"),
                LinkSpace::Indexed => spaces.push("Indexed"),
                _ => {}
            },
            None => match gr.image_space.as_deref() {
                Some("RGB") => spaces.push("RGB"),
                Some("LAB") | Some("Lab") => spaces.push("Lab"),
                Some("Indexed") => spaces.push("Indexed"),
                _ => {}
            },
        }
        spaces.sort();
        spaces.dedup();
        if !spaces.is_empty() {
            let list = spaces.join("/");
            let msg = if inside { msg("preflight.color.placed", format!("배치 파일 안에 {list} 색: {name}"), json!({"list":list,"name":name})) } else { msg("preflight.color.image", format!("{list} 이미지: {name}"), json!({"list":list,"name":name})) };
            self.push(
                COLOR_SPACE,
                Severity::Warning,
                page,
                Some(frame),
                msg,
                json!({"graphic": gr.id, "file": name, "spaces": spaces, "placedFile": inside, "source": if info.is_some() {"file"} else {"idml"}, "master": master}),
            );
        }
    }

    // -----------------------------------------------------------------------------------
    // Vector colours of items: RGB/Lab, registration, white overprint, overprint, TAC,
    // hairlines

    fn check_item_colors(&mut self) {
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
    fn paint_checks(&mut self, inks: &Inks, pname: &str, label: &str, overprint: bool, page: Option<usize>, id: &str, master: &Value, what: &str) {
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

    // -----------------------------------------------------------------------------------
    // Text: colours, small multi-ink text, black knockout, text strokes

    fn check_text(&mut self) {
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

    // -----------------------------------------------------------------------------------
    // Spot colours and the Ink Manager

    fn check_spots(&mut self) {
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

    // -----------------------------------------------------------------------------------
    // Overset text

    fn check_overset(&mut self) {
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

    // -----------------------------------------------------------------------------------
    // Bleed and safety margin

    fn bleed(&self) -> [f64; 4] {
        match self.o.bleed_pt {
            Some(b) => [b; 4],
            None => {
                let b = self.doc.print.bleed;
                if b.iter().all(|v| *v <= 0.0) { [3.0 * 72.0 / 25.4; 4] } else { b }
            }
        }
    }

    fn check_bleed_and_safety(&mut self) {
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

    // -----------------------------------------------------------------------------------
    // Hidden / non-printing content

    fn check_visibility(&mut self) {
        // layers
        let mut per_layer: BTreeMap<&str, usize> = BTreeMap::new();
        for p in &self.g.items {
            if let Some(l) = p.layer
                && (!l.visible || !l.printable)
                && p.parent.is_none()
                && !p.anchored
            {
                *per_layer.entry(l.id.as_str()).or_default() += 1;
            }
        }
        for (lid, n) in per_layer {
            let Some(l) = self.doc.layers.iter().find(|l| l.id == lid) else { continue };
            let why = if !l.visible { "숨긴 레이어" } else { "인쇄하지 않는 레이어" };
            let name = l.name.clone();
            self.push(HIDDEN_CONTENT, Severity::Info, None, Some(lid), msg("preflight.hidden.layer", format!("{why} '{name}'에 개체 {n}개"), json!({"why":if !l.visible { term("preflight.hiddenLayer") } else { term("preflight.nonprintingLayer") },"name":name,"count":n})), json!({"layer": lid, "name": name, "visible": l.visible, "printable": l.printable, "items": n}));
        }
        // items (not already explained by their layer or a hidden parent)
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            let parent_hidden = p.parent.is_some_and(|q| !self.g.items[q].visible || self.g.items[q].nonprinting);
            if parent_hidden || !p.layer_prints {
                continue;
            }
            let hidden = !p.item.visible;
            let nonprinting = p.item.print.nonprinting;
            if !hidden && !nonprinting {
                continue;
            }
            if !has_content(p, self.doc) {
                continue;
            }
            let page = self.page_of(p);
            let id = p.item.id.clone();
            let master = self.master_data(p);
            let why = if hidden { "숨긴 개체" } else { "인쇄하지 않는 개체" };
            self.push(HIDDEN_CONTENT, Severity::Info, page, Some(&id), msg("preflight.hidden.item", format!("{why}: 출력되지 않음"), json!({"why":if hidden { term("preflight.hiddenObject") } else { term("preflight.nonprintingObject") }})), json!({"hidden": hidden, "nonprinting": nonprinting, "master": master}));
        }
    }

    fn check_empty_frames(&mut self) {
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if !self.checked(p) {
                continue;
            }
            let item = p.item;
            let no_paint = item.fill == Paint::None && (item.stroke.paint == Paint::None || item.stroke.weight <= 0.0);
            let empty = match &item.kind {
                ItemKind::Rectangle { content: None } | ItemKind::Oval { content: None } | ItemKind::Polygon { content: None } => item.clipped.is_empty() && no_paint,
                ItemKind::TextFrame(tf) => {
                    no_paint && self.doc.stories.get(&tf.story).is_none_or(|s| !s.paragraphs.iter().flat_map(|p| &p.runs).any(|r| visible_text(&r.text) || !r.inline.is_empty()))
                }
                _ => false,
            };
            if empty {
                let page = self.page_of(p);
                let id = item.id.clone();
                let master = self.master_data(p);
                let what = if matches!(item.kind, ItemKind::TextFrame(_)) { "빈 텍스트 프레임" } else { "빈 프레임" };
                self.push(EMPTY_FRAME, Severity::Info, page, Some(&id), msg("preflight.frame.empty", what.into(), json!({"what":if matches!(item.kind, ItemKind::TextFrame(_)) { term("preflight.emptyTextFrame") } else { term("preflight.emptyFrame") }})), json!({"text": matches!(item.kind, ItemKind::TextFrame(_)), "master": master}));
            }
        }
    }

    // -----------------------------------------------------------------------------------
    // Transparency over spot colours

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

    fn check_transparency(&mut self) {
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

    // -----------------------------------------------------------------------------------
    // Fonts inside placed PDF / EPS files

    fn check_placed_fonts(&mut self) {
        let by_owner: HashMap<&str, &LinkEntry> = self.links.links.iter().map(|e| (e.owner.as_str(), e)).collect();
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if !self.checked(p) {
                continue;
            }
            let Some(gr) = Self::graphic_of(p) else { continue };
            let Some(e) = by_owner.get(gr.id.as_str()) else { continue };
            let Some(Ok(info)) = &e.info else { continue };
            let missing: Vec<&str> = info.fonts.iter().filter(|f| !f.embedded).map(|f| f.name.as_str()).collect();
            if missing.is_empty() {
                continue;
            }
            let file = e.path().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let page = self.graphic_page(p, gr);
            let master = self.master_data(p);
            let id = p.item.id.clone();
            self.push(
                FONT_NOT_EMBEDDED,
                Severity::Error,
                page,
                Some(&id),
                msg("preflight.font.notEmbedded", format!("배치 파일에 글꼴이 들어 있지 않음: {} ({file})", missing.join(", ")), json!({"fonts":missing,"file":file})),
                json!({"graphic": gr.id, "file": file, "fonts": missing, "master": master}),
            );
        }
    }

    /// Pages with nothing of their own that prints (master items do not count, as in
    /// InDesign's rule).
    fn check_blank_pages(&mut self) {
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

    // -----------------------------------------------------------------------------------
    // Document

    fn check_document(&mut self) {
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

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn edge_ko(e: &str) -> &'static str {
    match e {
        "top" => "위",
        "bottom" => "아래",
        "left" => "왼쪽",
        _ => "오른쪽",
    }
}

fn story_chars(s: &folio_model::Story) -> usize {
    let text: usize = s.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.text.chars().count()).sum();
    text + s.paragraphs.len().saturating_sub(1)
}

/// An item that would draw something: paint, a graphic, pasted content, text, children.
fn has_content(p: &Placed<'_>, doc: &Document) -> bool {
    let i = p.item;
    if i.fill != Paint::None || (i.stroke.paint != Paint::None && i.stroke.weight > 0.0) || !i.clipped.is_empty() {
        return true;
    }
    match &i.kind {
        ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => content.is_some(),
        ItemKind::TextFrame(tf) => doc.stories.get(&tf.story).is_some_and(|s| s.paragraphs.iter().flat_map(|p| &p.runs).any(|r| visible_text(&r.text))),
        ItemKind::Group { children } => !children.is_empty(),
        ItemKind::Other { .. } => true,
        ItemKind::GraphicLine => false,
    }
}

/// Spot names that are probably process colours by mistake.
pub fn looks_process(name: &str) -> bool {
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
