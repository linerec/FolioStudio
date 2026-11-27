//! Links, effective resolution, colour spaces and fonts in placed graphics.

use super::{Ctx, round1};
use crate::geom::{Placed, scales};
use crate::messages::{msg, term};
use crate::rules::{
    FONT_NOT_EMBEDDED,
    LINK_MISSING,
    LINK_MODIFIED,
    LINK_AMBIGUOUS,
    LINK_EMBEDDED,
    LINK_NAME_ONLY,
    LINK_RELOCATED,
    LINK_UNREADABLE,
    LINK_EPS_PREVIEW,
    IMAGE_RESOLUTION,
    COLOR_SPACE,
};
use crate::Severity;
use folio_links::{ColorSpace as LinkSpace, How, LinkEntry, LinkStatus};
use folio_model::{Graphic, ItemKind};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

impl Ctx<'_> {
    pub(super) fn graphic_of<'a>(p: &'a Placed<'_>) -> Option<&'a Graphic> {
        match &p.item.kind {
            ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => Some(g),
            ItemKind::Other { .. } => p.item.print.poster.as_deref(),
            _ => None,
        }
    }

    pub(super) fn check_links_and_images(&mut self) {
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
    fn graphic_page(&self, p: &Placed<'_>, gr: &Graphic) -> Option<usize> {
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
            // Effective resolution is compared in whole ppi: 299.9994 meets a 300 ppi minimum.
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

    pub(super) fn check_placed_fonts(&mut self) {
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
}
