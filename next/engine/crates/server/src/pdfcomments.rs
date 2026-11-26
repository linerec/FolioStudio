//! PDF comments. `pdfComments` reads the annotations of a project PDF (nothing is stored); `pdfCommentsDoc` lists the comments the
//! document holds; `importPdfComments` (an edit command) is prepared here: the host reads the PDF, ties each comment to the layout
//! and keeps the file as a project reference (principle 1), then the editor stores the comments (edit `ops/pdfcomments.rs`).
//! Rules: `next/oracle/rules/pdfcomments.ko.md`.
use crate::{Session, edit::EditState, place::Prepared};
use folio_edit::{Command, commands::RegisteredCommand};
use folio_model::layout::DocumentLayout;
use folio_model::pdfcomments::{PdfComment, PdfDestination, PdfReply};
use folio_model::{Document, Rect};
use folio_project::format::{self, RefKind, Reference};
use serde_json::{Value, json};

pub fn query(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let folio = session.and_then(|s| s.folio.as_ref()).ok_or("Open a project document to read PDF comments")?;
    let relative = req["path"].as_str().ok_or("Project-relative PDF path required")?;
    let path = folio_project::paths::checked_project_path(&folio.opened.project.root, relative)?;
    if !path.extension().is_some_and(|s| s.eq_ignore_ascii_case("pdf")) {
        return Err("A project PDF is required".into());
    }
    let catalogue = folio_links::pdfcomments::read(&path)?;
    Ok(json!({"type":"pdfComments", "path":relative, "catalogue":catalogue}))
}

/// The identity InDesign ties a PDF to its document by (measured): the document's XMP `xmpMM:OriginalDocumentID`, which every page of
/// the PDF it exports carries in `PieceInfo/InDesign/OriginalDocumentID`. What Folio's PDF export writes (`ExportOptions::document_id`):
/// that value, or for a document without one (made in Folio) its project id in the same `xmp.did:` form. The import compares with the
/// XMP value only (a document without one cannot tell another document's PDF).
pub fn document_id(cur: &Session) -> Option<String> {
    let xmp = cur.edit.as_ref().and_then(|e| e.lock().ok().and_then(|s| s.editor.part("META-INF/metadata.xml").map(str::to_owned)));
    identity(xmp.as_deref(), cur)
}

fn identity(xmp: Option<&str>, cur: &Session) -> Option<String> {
    xmp.and_then(xmp_original_id).or_else(|| cur.folio.as_ref().map(|f| format!("xmp.did:{}", f.opened.manifest.id)))
}

/// `xmpMM:OriginalDocumentID` of an XMP packet, written as an element or as an attribute.
fn xmp_original_id(xmp: &str) -> Option<String> {
    const KEY: &str = "xmpMM:OriginalDocumentID";
    let at = xmp.find(KEY)?;
    let rest = &xmp[at + KEY.len()..];
    let value = if let Some(body) = rest.strip_prefix('>') { body.split('<').next()? } else { rest.strip_prefix("=\"")?.split('"').next()? };
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn refusal(code: &str, error: impl Into<String>) -> Value { json!({"ok": false, "code": code, "error": error.into()}) }

/// Every reference the document has: the manifest's and the ones this session added.
fn references<'a>(cur: &'a Session, es: Option<&'a EditState>) -> Vec<&'a Reference> {
    let mut all: Vec<&Reference> = cur.folio.as_ref().map(|f| f.opened.manifest.references.iter().collect()).unwrap_or_default();
    if let Some(g) = es.and_then(|e| e.graphics.as_ref()) {
        let more: Vec<&Reference> = g.references.iter().filter(|r| !all.iter().any(|o| o.id == r.id)).collect();
        all.extend(more);
    }
    all
}

/// The name of the PDF a comment came from: the reference's file, or the last part of a path of another computer.
fn file_name(file: &str, refs: &[&Reference]) -> String {
    let file = file.strip_prefix("$ID/").unwrap_or(file);
    if let Some(id) = file.strip_prefix(format::PATH_REF_SCHEME) {
        let r = refs.iter().find(|r| r.id == id);
        return r.and_then(|r| r.path.as_ref().map(|p| p.file_name().to_owned()).or_else(|| r.hint.as_deref().map(last_part))).unwrap_or_default();
    }
    last_part(file)
}

fn last_part(path: &str) -> String { path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned() }

/// The page `index` and the rectangle it covers on its spread.
fn page_at(doc: &Document, index: usize) -> Option<(&folio_model::Page, &folio_model::Spread, Rect)> {
    doc.spreads.iter().find_map(|s| s.pages.iter().find(|p| p.index == index).map(|p| (p, s, p.transform.apply_rect(&p.bounds))))
}

/// The topmost page item of the spread whose box holds `point` (spread coordinates), with its transform to the spread.
fn item_at(items: &[folio_model::PageItem], parent: folio_model::Affine, point: folio_model::Point) -> Option<(String, folio_model::Affine)> {
    let mut found = None;
    for item in items.iter().filter(|i| i.visible) {
        let transform = parent.then_inner(&item.transform);
        if let folio_model::ItemKind::Group { children } = &item.kind {
            if let Some(hit) = item_at(children, transform, point) { found = Some(hit); }
            continue;
        }
        let Some(bounds) = folio_model::paths_bounds(&item.paths) else { continue };
        let b = transform.apply_rect(&bounds);
        if point.x >= b.left && point.x <= b.right && point.y >= b.top && point.y <= b.bottom { found = Some((item.id.clone(), transform)); }
    }
    found
}

fn page_ids(doc: &Document) -> Vec<String> {
    let mut pages: Vec<(usize, String)> = doc.spreads.iter().flat_map(|s| s.pages.iter().map(|p| (p.index, p.id.clone()))).collect();
    pages.sort_by_key(|p| p.0);
    pages.into_iter().map(|p| p.1).collect()
}

fn find_item_page(doc: &Document, id: &str) -> Option<usize> {
    fn has(items: &[folio_model::PageItem], id: &str) -> bool {
        items.iter().any(|i| i.id == id || matches!(&i.kind, folio_model::ItemKind::Group { children } if has(children, id)))
    }
    doc.spreads.iter().find(|s| has(&s.items, id)).and_then(|s| s.pages.first()).map(|p| p.index)
}

/// The page (0-based) a comment is on, for the panel's groups.
fn page_of(c: &PdfComment, doc: &Document, layout: &DocumentLayout, ids: &[String]) -> Option<usize> {
    let d = c.destination.as_ref()?;
    match d.kind.as_str() {
        "page" => ids.iter().position(|p| *p == d.target),
        "pageItem" => find_item_page(doc, &d.target),
        _ => crate::text_edit::story_page(doc, layout, &d.target, d.start),
    }
}

/// `pdfCommentsDoc`: the comments the document holds, each with the page it is on and the PDF it came from.
pub fn document(_req: &Value, session: Option<&Session>) -> Result<Value, String> {
    let cur = session.ok_or("no document open")?;
    let Some(edit) = cur.edit.as_ref() else { return Ok(json!({"type": "pdfCommentsDoc", "available": false, "comments": [], "pageCount": 0})) };
    let state = edit.lock().expect("shared state must remain unpoisoned after each operation");
    let comments = state.editor.pdf_comments().map_err(|e| e.message)?;
    let refs = references(cur, Some(&state));
    let ids = page_ids(&cur.doc);
    let rows: Vec<Value> = comments.iter().map(|c| {
        let mut row = serde_json::to_value(c).unwrap_or(Value::Null);
        let reference = c.file.strip_prefix("$ID/").unwrap_or(&c.file).strip_prefix(format::PATH_REF_SCHEME).and_then(|id| refs.iter().find(|r| r.id == id));
        row["page"] = json!(page_of(c, &cur.doc, &cur.layout, &ids));
        row["fileName"] = json!(file_name(&c.file, &refs));
        row["path"] = json!(reference.and_then(|r| r.path.as_ref()).map(|p| p.as_str().to_owned()));
        row
    }).collect();
    Ok(json!({"type": "pdfCommentsDoc", "available": true, "comments": rows, "pageCount": ids.len()}))
}

/// The transform of the page item `id` to its spread.
fn item_transform(items: &[folio_model::PageItem], parent: folio_model::Affine, id: &str) -> Option<folio_model::Affine> {
    for item in items {
        let transform = parent.then_inner(&item.transform);
        if item.id == id { return Some(transform); }
        if let folio_model::ItemKind::Group { children } = &item.kind
            && let Some(found) = item_transform(children, transform, id) { return Some(found); }
    }
    None
}

/// The kinds InDesign 2025 puts a badge on (measured 2026-11-12, rules/pdfcomments-c5.ko.md). The line, arrow, oval, rectangle, polygon,
/// connected-lines, pencil and cloud comments are not drawn on the layout at all (only the panel lists them).
fn has_badge(kind: &str) -> bool {
    matches!(kind, "StickyNoteType" | "TextBoxType" | "TextTypewriterType" | "TextCalloutType" | "StampType")
}

/// `pdfCommentMarks`: what InDesign 2025 draws on `page` for the comments tied to the layout (the PDF Comments panel's eye button shows or
/// hides it; measured 2026-11-11 and 2026-11-12, rules/pdfcomments.ko.md and pdfcomments-c5.ko.md). A comment on text is the boxes of its
/// characters (`boxes`: along the baseline `from` → `to`, `up` and `down` points; page points); a note, a text box, a typewriter text, a
/// stamp and a callout are a badge at the top left corner of the comment's box (`badge`, page points; a callout's at its target point, the
/// start of its leader line). The shapes, lines and the pencil are not drawn: no mark. Comments that lost their object (orphans,
/// accepted ones) are not drawn. With `selected` (the id of the comment picked in the panel) the answer has a `selection`: `ring` (the badge's corner) for a
/// comment that has a badge, `center` (the page's middle) for one that is not drawn.
pub fn marks(req: &Value, session: Option<&Session>) -> Result<Value, String> {
    let cur = session.ok_or("no document open")?;
    let page = req["page"].as_u64().ok_or("pdfCommentMarks needs page")? as usize;
    let selected = req["selected"].as_str();
    let Some(edit) = cur.edit.as_ref() else { return Ok(json!({"type": "pdfCommentMarks", "page": page, "marks": []})) };
    let state = edit.lock().expect("shared state must remain unpoisoned after each operation");
    let comments = state.editor.pdf_comments().map_err(|e| e.message)?;
    let ids = page_ids(&cur.doc);
    let mut out = Vec::new();
    let mut selection = Value::Null;
    for c in comments.iter().filter(|c| !c.orphan) {
        let Some(d) = c.destination.as_ref() else { continue };
        if page_of(c, &cur.doc, &cur.layout, &ids) != Some(page) { continue; }
        let Some((p, spread, on_spread)) = page_at(&cur.doc, page) else { continue };
        let mark = match d.kind.as_str() {
            "text" => json!({"id": c.id, "kind": c.kind, "boxes": crate::text_edit::range_boxes(&cur.doc, &cur.layout, page, &d.target, d.start, d.end)}),
            "page" | "pageItem" => {
                let badge = if d.kind == "page" {
                    [d.rect[0] - p.bounds.left, d.rect[1] - p.bounds.top]
                } else {
                    let Some(transform) = item_transform(&spread.items, folio_model::Affine::IDENTITY, &d.target) else { continue };
                    let corner = if c.kind == "TextCalloutType" { folio_model::Point::new(d.point[0], d.point[1]) } else { folio_model::Point::new(d.rect[0], d.rect[1]) };
                    let at = transform.apply(corner);
                    [at.x - on_spread.left, at.y - on_spread.top]
                };
                // the comment picked in the panel (measured 2026-11-12, rules/pdfcomments-c5.ko.md section 5): a frame around its badge, and for the
                // comments that have none a frame mark in the middle of its page
                if selected == Some(c.id.as_str()) {
                    selection = if has_badge(&c.kind) { json!({"id": c.id, "kind": c.kind, "ring": badge}) }
                    else { json!({"id": c.id, "kind": c.kind, "center": [(p.bounds.right - p.bounds.left) / 2.0, (p.bounds.bottom - p.bounds.top) / 2.0]}) };
                }
                if !has_badge(&c.kind) { continue; }
                json!({"id": c.id, "kind": c.kind, "badge": badge})
            }
            _ => continue,
        };
        out.push(mark);
    }
    Ok(json!({"type": "pdfCommentMarks", "page": page, "marks": out, "selection": selection}))
}

/// The PDF comment as the editor stores it: its type, and where it is on the layout. A text markup is tied to the text its quads
/// cover; a note and the shapes to the page (InDesign ties a text box or a shape to a page item: the object is not looked for, so the
/// page it is on and its box are what is kept, rules/pdfcomments.ko.md); a caret by itself is an orphan, as InDesign measured; what
/// cannot be placed is an orphan with an empty box.
pub fn to_comments(catalogue: &folio_links::pdfcomments::Catalogue, source: &str, doc: &Document, layout: &DocumentLayout) -> Vec<PdfComment> {
    let ids = page_ids(doc);
    catalogue.comments.iter().filter_map(|c| {
        let kind = folio_idml::pdfcomments::kind_of(&c.comment_type)?;
        let page = (c.page as usize).checked_sub(1);
        let page_box = page.and_then(|p| catalogue.boxes.get(p)).copied().unwrap_or([0.0, 0.0, 612.0, 792.0]);
        // PDF user space (origin bottom left) → the page's points (origin top left)
        let to_page = |x: f64, y: f64| (x - page_box[0], page_box[3] - y);
        let page_rect = |r: [f64; 4]| { let (l, t) = to_page(r[0], r[3]); let (rr, b) = to_page(r[2], r[1]); Rect { left: l, top: t, right: rr, bottom: b } };
        let page_id = page.and_then(|p| ids.get(p)).cloned();
        let box_of = c.rect.map(page_rect);
        // InDesign keeps the box in the page's inner coordinates (its GeometricBounds start at (left, top), measured on a page whose top is 146)
        let inner = page.and_then(|p| page_at(doc, p)).map(|(p, _, _)| (p.bounds.left, p.bounds.top)).unwrap_or((0.0, 0.0));
        let on_page = |rect: Option<Rect>| page_id.clone().map(|target| {
            let r = rect.map(|r| Rect { left: r.left + inner.0, top: r.top + inner.1, right: r.right + inner.0, bottom: r.bottom + inner.1 }).unwrap_or_default();
            PdfDestination { kind: "page".into(), target, rect: [r.left, r.top, r.right, r.bottom], ..Default::default() }
        });
        // a text box or shape belongs to the page item under it: the box in the item's own coordinates (measured: a text box over a
        // frame gave the frame and the box minus the frame's place)
        let on_item = |rect: Rect| -> Option<PdfDestination> {
            let (_, spread, at) = page_at(doc, page?)?;
            let spread_rect = Rect { left: rect.left + at.left, top: rect.top + at.top, right: rect.right + at.left, bottom: rect.bottom + at.top };
            let middle = folio_model::Point::new((spread_rect.left + spread_rect.right) / 2.0, (spread_rect.top + spread_rect.bottom) / 2.0);
            let (id, transform) = item_at(&spread.items, folio_model::Affine::IDENTITY, middle)?;
            let inverse = transform.invert()?;
            let r = inverse.apply_rect(&spread_rect);
            // an arrow's end and a callout's start are the destination's TargetPoint, in the item's own coordinates (measured 2026-11-12)
            let point = c.target.map(|[x, y]| { let (px, py) = to_page(x, y); inverse.apply(folio_model::Point::new(px + at.left, py + at.top)) }).map_or([0.0, 0.0], |p| [p.x, p.y]);
            Some(PdfDestination { kind: "pageItem".into(), target: id, point, rect: [r.left, r.top, r.right, r.bottom], ..Default::default() })
        };
        let strikes = matches!(c.comment_type.as_str(), "strike" | "insert" | "replace");
        let (destination, orphan) = match c.comment_type.as_str() {
            "highlight" | "underline" | "squiggly" | "strike" => {
                let quads: Vec<Rect> = c.quads.iter().map(|q| {
                    let xs = [q[0], q[2], q[4], q[6]];
                    let ys = [q[1], q[3], q[5], q[7]];
                    page_rect([xs.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max), ys.iter().cloned().fold(f64::MIN, f64::max)])
                }).collect();
                let found = page.filter(|_| !quads.is_empty()).and_then(|p| crate::text_edit::text_in_rect(doc, layout, p, &quads));
                match found {
                    Some((story, start, end)) => (Some(PdfDestination { kind: "text".into(), target: story, start, end, ..Default::default() }), false),
                    None => (on_page(None), true),
                }
            }
            "insert" => (on_page(None), true),
            // a note keeps to the page (measured); the other markup goes to the item under it when there is one
            "sticky" => (on_page(box_of), page_id.is_none()),
            _ => (box_of.and_then(on_item).or_else(|| on_page(box_of)), page_id.is_none()),
        };
        Some(PdfComment {
            id: String::new(), reviewer: c.author.clone(), content: if c.comment_type == "stamp" && !c.subject.is_empty() { format!("{} : {}", c.subject, c.content) } else { c.content.clone() },
            date: c.date.clone(), kind: kind.into(), file: source.into(), status: folio_idml::pdfcomments::OPEN.into(), orphan,
            applied: strikes.then_some(false), destination,
            replies: c.replies.iter().map(|r| PdfReply { id: String::new(), reviewer: r.author.clone(), content: r.content.clone(), date: r.date.clone() }).collect(),
        })
    }).collect()
}

/// Before an edit with `importPdfComments`: the file is read, kept as a reference of the document (the project file, root-relative,
/// with its SHA-256) and the comments are made ready for the editor. Errors carry InDesign's warnings as codes: `corrupt`,
/// `noComments`, `notInDesign`, `otherDocument`, `duplicate`.
pub(crate) fn prepare(commands: &mut [Command], cur: &Session, es: &EditState, prepared: &mut Prepared) -> Result<(), Value> {
    for command in commands {
        let Command::Registered(RegisteredCommand::ImportPdfComments { path, again, source, comments }) = command else { continue };
        if comments.is_some() && source.is_some() { continue; } // given (the checks, the oracle)
        let folio = cur.folio.as_ref().ok_or_else(|| refusal("notProject", "PDF 주석은 프로젝트 문서에서만 가져올 수 있습니다"))?;
        let root = &folio.opened.project.root;
        if !path.to_ascii_lowercase().ends_with(".pdf") { return Err(refusal("invalid", "A project PDF is required")); }
        let file = folio_project::paths::checked_project_path(root, path).map_err(|e| refusal("invalid", e))?;
        let metadata = file.metadata().map_err(|_| refusal("notFound", format!("{path}: no such file")))?;
        if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 { return Err(refusal("invalid", "PDF comments require a file no larger than 128 MiB")); }
        let bytes = std::fs::read(&file).map_err(|e| refusal("notFound", format!("{path}: {e}")))?;
        let catalogue = folio_links::pdfcomments::catalogue(&bytes).map_err(|e| refusal(if e.starts_with("Cannot parse") { "corrupt" } else { "invalid" }, e))?;
        if catalogue.comments.is_empty() { return Err(refusal("noComments", "PDF has no comments. Select another PDF to continue.")); }
        // InDesign's two checks of where the PDF came from (measured: each page's PieceInfo/InDesign/OriginalDocumentID against the document's)
        if !catalogue.origins.iter().any(|o| o.indesign) { return Err(refusal("notInDesign", "PDF is not created from InDesign. Select another PDF to continue.")); }
        // only a document that has an OriginalDocumentID of its own (every InDesign document does) can tell another document's PDF
        if let Some(own) = es.editor.part("META-INF/metadata.xml").and_then(xmp_original_id)
            && catalogue.origins.iter().any(|o| o.document.as_deref().is_some_and(|d| d != own))
        {
            return Err(refusal("otherDocument", "The PDF you are trying to import is created from another InDesign document. Select the correct PDF to continue."));
        }
        let rel = folio_project::paths::relative_to_root(root, &file).map_err(|e| refusal("invalid", e.to_string()))?;
        let refs = references(cur, Some(es));
        let existing = refs.iter().find(|r| r.kind == RefKind::File && r.path.as_ref() == Some(&rel) && r.sha256.as_deref() == Some(catalogue.sha256.as_str())).map(|r| r.id.clone())
            .or_else(|| prepared.refs.iter().find(|r| r.path.as_ref() == Some(&rel) && r.sha256.as_deref() == Some(catalogue.sha256.as_str())).map(|r| r.id.clone()));
        // InDesign compares the file names of what was imported before
        let name = rel.file_name().to_owned();
        let stored = es.editor.pdf_comments().map_err(|e| refusal(e.code, e.message))?;
        if !*again && stored.iter().any(|c| file_name(&c.file, &refs) == name) {
            return Err(refusal("duplicate", "PDF with the same file name is already imported. Re-import can lead to duplicate comments. Do you want to continue?"));
        }
        let id = match existing {
            Some(id) => id,
            None => {
                let r = Reference {
                    id: format!("r{}", folio_project::util::short_id()), kind: RefKind::File, path: Some(rel), sha256: Some(catalogue.sha256.clone()),
                    bytes: Some(metadata.len()), hint: None, original_name: None, status: Some("found".into()), font: None, extra: Default::default(),
                };
                let id = r.id.clone();
                prepared.refs.push(r);
                id
            }
        };
        let source_text = format!("$ID/{}{id}", format::PATH_REF_SCHEME);
        *comments = Some(to_comments(&catalogue, &source_text, &cur.doc, &cur.layout));
        *source = Some(source_text);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use folio_model::layout::{FrameText, LaidLine, TextCluster};
    use folio_model::{Affine, ItemKind, Page, PageItem, Spread, TextFrame};

    /// A two-page document like the one the sample PDF was made from (400 × 500 pt pages, a text frame at (36, 40) on each), whose
    /// layout has a cluster per character under each commented word: the words' places are the quads of the PDF.
    fn document(catalogue: &folio_links::pdfcomments::Catalogue) -> (Document, DocumentLayout) {
        let mut doc = Document::default();
        let mut layout = DocumentLayout::default();
        let words: HashMap<&str, (usize, usize)> = HashMap::from([("Highlight", (6, 11)), ("Underline", (20, 25)), ("Strike", (12, 19)), ("REPLACED", (52, 56))]);
        for page in 0..2usize {
            let story = format!("s{page}");
            let frame = format!("f{page}");
            let mut clusters = Vec::new();
            for c in catalogue.comments.iter().filter(|c| c.page as usize == page + 1 && !c.quads.is_empty()) {
                let (start, end) = *words.iter().find(|(k, _)| c.content.starts_with(**k)).unwrap().1;
                let q = c.quads[0];
                let (x0, x1) = (q[0].min(q[2]), q[0].max(q[2]));
                // a character's box is as high as its line's text: the quad of a word is taller than that and neighbouring lines' quads overlap
                let middle = 500.0 - (q[1] + q[5]) / 2.0 - 40.0;
                let (top, bottom) = (middle - 5.0, middle + 5.0);
                for i in start..end {
                    let step = (x1 - x0) / (end - start) as f64;
                    clusters.push(TextCluster { start: i, end: i + 1, x0: x0 - 36.0 + step * (i - start) as f64, x1: x0 - 36.0 + step * (i - start + 1) as f64, top, bottom, path_map: None });
                }
            }
            layout.frames.insert(frame.clone(), FrameText { frame: frame.clone(), lines: vec![LaidLine { start: 0, end: 60, text_clusters: clusters, ..Default::default() }] });
            doc.spreads.push(Spread {
                pages: vec![Page { id: format!("p{page}"), index: page, bounds: Rect { right: 400.0, bottom: 500.0, ..Default::default() }, ..Default::default() }],
                items: vec![PageItem { id: frame, kind: ItemKind::TextFrame(TextFrame { story, ..Default::default() }), transform: Affine::translate(36.0, 40.0), visible: true, ..Default::default() }],
                ..Default::default()
            });
        }
        (doc, layout)
    }

    /// The page and the frame of `persist.idml` (InDesign 2025's own IDML of the measured document): page 1 has `GeometricBounds`
    /// `146 0 646 400` and a transform of (-200, -396), the frame `ufa` is a 328 x 428 box at (106, 146) with its path from (-270, -360).
    fn measured_document() -> Document {
        use folio_model::{PathPoint, Point, SubPath};
        let corner = |x: f64, y: f64| PathPoint { anchor: Point::new(x, y), left: Point::new(x, y), right: Point::new(x, y) };
        let frame = PageItem {
            id: "ufa".into(), kind: ItemKind::TextFrame(TextFrame { story: "ufd".into(), ..Default::default() }), transform: Affine::translate(106.0, 146.0), visible: true,
            paths: vec![SubPath { points: vec![corner(-270.0, -360.0), corner(-270.0, 68.0), corner(58.0, 68.0), corner(58.0, -360.0)], open: false }],
            ..Default::default()
        };
        let mut doc = Document::default();
        doc.spreads.push(Spread {
            pages: vec![Page { id: "ud6".into(), index: 0, bounds: Rect { left: 0.0, top: 146.0, right: 400.0, bottom: 646.0 }, transform: Affine::translate(-200.0, -396.0), ..Default::default() }],
            items: vec![frame], ..Default::default()
        });
        doc
    }

    fn annotation_catalogue() -> folio_links::pdfcomments::Catalogue {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/rules/pdfcomments-annot.pdf");
        folio_links::pdfcomments::catalogue(&std::fs::read(file).unwrap()).unwrap()
    }

    #[test]
    #[cfg_attr(not(has_comment_annot), ignore = "annotation PDF fixture is absent")]
    fn boxes_are_kept_as_indesign_does_in_the_page_and_item_coordinates() {
        let catalogue = annotation_catalogue();
        let comments = to_comments(&catalogue, "x", &measured_document(), &DocumentLayout::default());
        // the note of page 1 (InDesign wrote Top 166 Bottom 182: the page's inner box starts at 146)
        let note = comments[0].destination.clone().unwrap();
        assert_eq!((note.kind.as_str(), note.target.as_str(), note.rect), ("page", "ud6", [300.0, 166.0, 316.0, 182.0]));
        // the text box lies over the frame: InDesign wrote the frame and the box in the frame's own coordinates (-270 -136 -6 -96)
        let text_box = comments[5].destination.clone().unwrap();
        assert_eq!(comments[5].kind, "TextBoxType");
        assert_eq!((text_box.kind.as_str(), text_box.target.as_str()), ("pageItem", "ufa"));
        for (got, want) in text_box.rect.iter().zip([-270.0, -136.0, -6.0, -96.0]) { assert!((got - want).abs() < 0.01, "{:?}", text_box.rect); }
        // a caret is an orphan with an empty box on the page (InDesign: Left 0 Top 0 Right 0 Bottom 0)
        assert!(comments[6].orphan);
        assert_eq!(comments[6].destination.as_ref().unwrap().rect, [0.0; 4]);
    }

    #[test]
    #[cfg_attr(not(has_comment_annot), ignore = "annotation PDF fixture is absent")]
    fn comments_are_tied_to_the_layout_as_indesign_does() {
        let catalogue = annotation_catalogue();
        let (doc, layout) = document(&catalogue);
        let comments = to_comments(&catalogue, "$ID/folio:path-ref/r1", &doc, &layout);
        assert_eq!(comments.len(), 14);
        let kinds: Vec<&str> = comments[..7].iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["StickyNoteType", "HighlightTextType", "UnderlineTextType", "StrikethroughTextType", "StrikethroughTextType", "TextBoxType", "InsertTextType"]);
        let d = |i: usize| comments[i].destination.clone().unwrap();
        // the note: a page destination with the note's own box in the page's points (measured on page 2: 300 20 316 36)
        assert_eq!((d(0).kind.as_str(), d(0).target.as_str(), d(0).rect), ("page", "p0", [300.0, 20.0, 316.0, 36.0]));
        assert_eq!(d(7).target, "p1");
        // text markups: the characters the quads cover (InDesign: 6..11, 20..25, 12..19, 52..56)
        let text = |i: usize| (d(i).kind, d(i).target, d(i).start, d(i).end);
        assert_eq!(text(1), ("text".into(), "s0".into(), 6, 11));
        assert_eq!(text(2), ("text".into(), "s0".into(), 20, 25));
        assert_eq!(text(3), ("text".into(), "s0".into(), 12, 19));
        assert_eq!(text(4), ("text".into(), "s0".into(), 52, 56));
        assert_eq!(text(8), ("text".into(), "s1".into(), 6, 11));
        assert_eq!((comments[3].applied, comments[1].applied, comments[6].applied), (Some(false), None, Some(false)));
        // a caret by itself is an orphan with an empty box (InDesign: 2 of the 14 comments); everything else is tied
        assert_eq!(comments.iter().filter(|c| c.orphan).count(), 2);
        assert!(comments[6].orphan && d(6).rect == [0.0; 4]);
        assert_eq!(comments[0].replies.len(), 1);
        assert!(comments.iter().all(|c| c.status == "OpenStatus" && c.file == "$ID/folio:path-ref/r1" && c.id.is_empty()));
        // text that is not under the quads leaves the comment an orphan on its page
        let (doc, empty) = (document(&catalogue).0, DocumentLayout::default());
        let comments = to_comments(&catalogue, "x", &doc, &empty);
        assert!(comments[1].orphan && comments[1].destination.as_ref().unwrap().kind == "page");
    }
}
