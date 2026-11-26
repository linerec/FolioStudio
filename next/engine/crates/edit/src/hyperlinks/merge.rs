//! Data Merge ▸ Create Merged Document (oracle datamerge-c3, InDesign 2025 `mergeRecords` probes in
//! `artifacts/datamerge-c3-20261023/measurement/`). The template document becomes the merged one:
//! - one record per page: record 1 stays on the template's own spreads, each next record gets a
//!   copy of every template spread after the last one (pages ordered record by record);
//! - several records per page (one-page documents only; a longer one is refused with InDesign's
//!   message): the page items' bounding box is placed at the top left margin, then right (rows
//!   first) or down (columns first) with the gaps, a copy fits while it ends inside the margins
//!   (equal counts as fitting), the first one on a page is placed anyway; the pages take the
//!   merge margins; the template's own items are gone, later records are in front;
//! - a placeholder takes the record's value (an empty one is two U+FEFF), the hyperlink objects
//!   of the merge fields are gone, "remove blank lines" deletes the paragraph break before a
//!   paragraph that is only empty values (the one after it for the first paragraph).
use super::*;
use crate::ops::Command;
use std::collections::BTreeMap;

struct PageInfo {
    origin: [f64; 2],
    size: [f64; 2],
}

struct SpreadInfo {
    id: String,
    pages: Vec<PageInfo>,
    items: Vec<String>,
    page_ids: Vec<String>,
}

const ITEMS: &[&str] = &["Rectangle", "Oval", "Polygon", "GraphicLine", "TextFrame", "Group", "Button", "MultiStateObject", "FormField", "EPSText", "TextPath"];

fn numbers(s: &str) -> Vec<f64> {
    s.split_whitespace().filter_map(|v| v.parse().ok()).collect()
}

fn spreads(w: &mut Work<'_>) -> Result<Vec<SpreadInfo>, EditError> {
    let dm = w.text("designmap.xml")?.to_owned();
    let ix = Index::scan(&dm).map_err(|e| EditError::new("xml", e))?;
    let root = ix.roots().next().ok_or_else(|| invalid("missing document"))?;
    let mut out = Vec::new();
    for &c in ix.children(root) {
        if ix.local(&dm, c) != "Spread" { continue; }
        let Some(src) = ix.attr(&dm, c, "src") else { continue };
        let t = w.text(src)?;
        let sx = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        let top = sx.roots().next().ok_or_else(|| invalid("empty spread"))?;
        let Some(&spread) = sx.named(t, top, "Spread").first() else { continue };
        let mut info = SpreadInfo { id: sx.attr(t, spread, "Self").unwrap_or("").to_owned(), pages: Vec::new(), items: Vec::new(), page_ids: Vec::new() };
        for &k in sx.children(spread) {
            let name = sx.local(t, k);
            if name == "Page" {
                let b = numbers(sx.attr(t, k, "GeometricBounds").unwrap_or(""));
                let m = numbers(sx.attr(t, k, "ItemTransform").unwrap_or("1 0 0 1 0 0"));
                if b.len() == 4 && m.len() == 6 {
                    info.pages.push(PageInfo { origin: [m[4] + b[1], m[5] + b[0]], size: [b[3] - b[1], b[2] - b[0]] });
                    info.page_ids.push(sx.attr(t, k, "Self").unwrap_or("").to_owned());
                }
            } else if ITEMS.contains(&name)
                && let Some(id) = sx.attr(t, k, "Self") { info.items.push(id.to_owned()); }
        }
        out.push(info);
    }
    Ok(out)
}

/// `2-3`, `5`, `1-2, 4`: 1-based record numbers; empty = every record.
pub(crate) fn select(range: &str, count: usize) -> Result<Vec<usize>, EditError> {
    let bad = || EditError::new("invalid", "One or more of the records specified are not valid record numbers.");
    if range.trim().is_empty() { return Ok((0..count).collect()); }
    let mut out = Vec::new();
    for part in range.split(',') {
        let part = part.trim();
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse::<usize>().map_err(|_| bad())?, b.trim().parse::<usize>().map_err(|_| bad())?),
            None => { let n = part.parse::<usize>().map_err(|_| bad())?; (n, n) }
        };
        if a == 0 || a > b || b > count { return Err(bad()); }
        out.extend(a - 1..b);
    }
    Ok(out)
}

fn descendants(ix: &Index, i: usize) -> impl Iterator<Item = usize> + '_ {
    let (s, e) = (ix.nodes[i].start, ix.nodes[i].end);
    (i..ix.nodes.len()).take_while(move |&k| ix.nodes[k].start < e).filter(move |&k| ix.nodes[k].start >= s)
}

/// Stories shown by the text frames inside `owners` (spreads or page items).
fn stories_of(w: &mut Work<'_>, owners: &[String]) -> Result<Vec<String>, EditError> {
    let mut out: Vec<String> = Vec::new();
    for id in owners {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        for k in descendants(&ix, i) {
            if ix.local(t, k) == "TextFrame"
                && let Some(story) = ix.attr(t, k, "ParentStory")
                && !out.iter().any(|s| s == story)
            {
                out.push(story.to_owned());
            }
        }
    }
    Ok(out)
}

/// The text of the `Content` elements of one piece of story XML.
fn content_text(inner: &str) -> String {
    let mut out = String::new();
    let mut rest = inner;
    while let Some(a) = rest.find("<Content>") {
        let Some(b) = rest[a..].find("</Content>") else { break };
        out.push_str(&xml::unescape(&rest[a + 9..a + b]));
        rest = &rest[a + b + 10..];
    }
    out
}

/// Joins `Content` elements that follow one another inside a character range.
fn coalesce(w: &mut Work<'_>, story: &str) -> Result<(), EditError> {
    let (part, ix, _) = w.find(story)?;
    let t = w.text(&part)?;
    let mut edits = Vec::new();
    for k in 0..ix.nodes.len() {
        if ix.local(t, k) != "CharacterStyleRange" { continue; }
        let kids = ix.children(k);
        let mut j = 0;
        while j < kids.len() {
            if ix.local(t, kids[j]) != "Content" { j += 1; continue; }
            let mut last = j;
            while last + 1 < kids.len() && ix.local(t, kids[last + 1]) == "Content" && t[ix.nodes[kids[last]].end..ix.nodes[kids[last + 1]].start].trim().is_empty() { last += 1; }
            if last > j {
                let text: String = (j..=last).map(|n| &t[ix.nodes[kids[n]].open_end..ix.nodes[kids[n]].close_start]).collect();
                edits.push((ix.nodes[kids[j]].start..ix.nodes[kids[last]].end, format!("<Content>{text}</Content>")));
            }
            j = last + 1;
        }
    }
    if !edits.is_empty() { w.splice(&part, edits)?; }
    Ok(())
}

/// Puts a record's values into one story; returns the fields filled and the text each one got, in story order.
fn fill(w: &mut Work<'_>, story: &str, fields: &[String], record: &[String], remove_blank_lines: bool) -> Result<Vec<(String, String)>, EditError> {
    let (part, ix, _) = w.find(story)?;
    let t = w.text(&part)?;
    let mut edits = Vec::new();
    let mut filled = Vec::new();
    for k in 0..ix.nodes.len() {
        if ix.local(t, k) != "HyperlinkTextSource" { continue; }
        let n = &ix.nodes[k];
        let inner = &t[n.open_end..n.close_start];
        let text = content_text(inner);
        let Some(field) = text.strip_prefix("<<").and_then(|s| s.strip_suffix(">>")) else { continue };
        let Some(at) = fields.iter().position(|f| f == field) else { continue };
        let value = record.get(at).map(String::as_str).unwrap_or("");
        let value = if value.is_empty() { "\u{FEFF}\u{FEFF}" } else { value };
        filled.push((field.to_owned(), value.to_owned()));
        let placeholder = format!("<Content>{}</Content>", esc(&text));
        let new = if inner.contains(&placeholder) { inner.replacen(&placeholder, &format!("<Content>{}</Content>", esc(value)), 1) } else { format!("<Content>{}</Content>", esc(value)) };
        edits.push((n.start..n.end, new));
    }
    if edits.is_empty() { return Ok(filled); }
    w.splice(&part, edits)?;
    w.recompose(story);
    coalesce(w, story)?;
    if remove_blank_lines { blank_lines(w, story)?; }
    Ok(filled)
}

fn blank_lines(w: &mut Work<'_>, story: &str) -> Result<(), EditError> {
    let (part, _, _) = w.find(story)?;
    let chars: Vec<char> = crate::text::contents(w.text(&part)?, story)?.chars().collect();
    let mut breaks = std::collections::BTreeSet::new();
    let mut start = 0;
    let mut paragraphs = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '\r' { paragraphs.push((start, i)); start = i + 1; }
    }
    paragraphs.push((start, chars.len()));
    for (n, &(s, e)) in paragraphs.iter().enumerate() {
        if e > s && chars[s..e].iter().all(|&c| c == '\u{FEFF}') {
            if n > 0 { breaks.insert(s - 1); } else if paragraphs.len() > 1 { breaks.insert(e); }
        }
    }
    for at in breaks.into_iter().rev() { crate::text::delete(w, story, at, at + 1)?; }
    Ok(())
}

/// The hyperlink objects Data Merge keeps for its fields are not part of a merged document.
fn drop_merge_links(w: &mut Work<'_>) -> Result<(), EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let root = ix.roots().next().ok_or_else(|| invalid("missing document"))?;
    let mut keys = Vec::new();
    let mut edits = Vec::new();
    let span = |k: usize| {
        let mut s = ix.nodes[k].start;
        while s > 0 && t.as_bytes()[s - 1] != b'\n' && t.as_bytes()[s - 1].is_ascii_whitespace() { s -= 1; }
        if s > 0 && t.as_bytes()[s - 1] == b'\n' { s -= 1; }
        s..ix.nodes[k].end
    };
    for &k in ix.children(root) {
        if ix.local(t, k) == "HyperlinkURLDestination" && ix.attr(t, k, "Name").is_some_and(|n| n.starts_with("DBF_") || n.starts_with("IMG_") || n.starts_with("QRC_")) {
            if let Some(key) = ix.attr(t, k, "DestinationUniqueKey") { keys.push(key.to_owned()); }
            edits.push(span(k));
        }
    }
    let mut sources = Vec::new();
    for &k in ix.children(root) {
        if ix.local(t, k) == "Hyperlink" && ix.attr(t, k, "DestinationUniqueKey").is_some_and(|key| keys.iter().any(|x| x == key)) {
            if let Some(s) = ix.attr(t, k, "Source") { sources.push(s.to_owned()); }
            edits.push(span(k));
        } else if matches!(ix.local(t, k), "DataMergeImagePlaceholder" | "DataMergeQrcodePlaceholder") {
            edits.push(span(k));
        }
    }
    for &k in ix.children(root) {
        if ix.local(t, k) == "HyperlinkPageItemSource" && ix.attr(t, k, "Self").is_some_and(|s| sources.iter().any(|x| x == s)) { edits.push(span(k)); }
    }
    w.splice("designmap.xml", edits.into_iter().map(|r| (r, String::new())).collect())
}

fn set_margins(w: &mut Work<'_>, margins: [f64; 4]) -> Result<(), EditError> {
    for part in w.parts_named("Spreads/") {
        let t = w.text(&part)?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for k in 0..ix.nodes.len() {
            if ix.local(t, k) != "Page" { continue; }
            let Some(&m) = ix.named(t, k, "MarginPreference").first() else { continue };
            let width = numbers(ix.attr(t, k, "GeometricBounds").unwrap_or("")).chunks(4).next().map_or(0.0, |b| if b.len() == 4 { b[3] - b[1] } else { 0.0 });
            let columns = numbers(ix.attr(t, m, "ColumnCount").unwrap_or("1")).first().copied().unwrap_or(1.0).max(1.0);
            let gutter = numbers(ix.attr(t, m, "ColumnGutter").unwrap_or("12")).first().copied().unwrap_or(12.0);
            let usable = width - margins[1] - margins[3];
            // equal columns, as the page had them: the column positions follow the new margins
            let size = (usable - gutter * (columns - 1.0)) / columns;
            let positions: Vec<String> = (0..columns as usize).flat_map(|c| { let a = c as f64 * (size + gutter); [a, a + size] }).map(num).collect();
            for (name, value) in [("Top", margins[0]), ("Bottom", margins[2]), ("Left", margins[1]), ("Right", margins[3])] {
                edits.push(crate::ops::attr_edit(&ix, t, m, name, &num(value)));
            }
            edits.push(crate::ops::attr_edit(&ix, t, m, "ColumnsPositions", &positions.join(" ")));
        }
        if !edits.is_empty() { w.splice(&part, edits)?; }
    }
    Ok(())
}

fn num(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}


/// Of `ids`, the ones that are page items directly on a spread, and not in `exclude`.
fn spread_items(w: &mut Work<'_>, ids: Vec<String>, exclude: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        if exclude.contains(&id) { continue; }
        let Ok((part, ix, i)) = w.find(&id) else { continue };
        let Ok(t) = w.text(&part) else { continue };
        if ITEMS.contains(&ix.local(t, i)) && ix.nodes[i].parent.is_some_and(|q| ix.local(t, q) == "Spread") { out.push(id); }
    }
    out
}

/// Of `ids`, the pages, as the spreads that hold them.
fn spreads_of_pages(w: &mut Work<'_>, ids: Vec<String>) -> Result<Vec<String>, EditError> {
    let mut out = Vec::new();
    for id in ids {
        let Ok((part, ix, i)) = w.find(&id) else { continue };
        let t = w.text(&part)?;
        if ix.local(t, i) != "Page" { continue; }
        let parent = ix.nodes[i].parent.ok_or_else(|| invalid("page without spread"))?;
        out.push(ix.attr(t, parent, "Self").unwrap_or("").to_owned());
    }
    Ok(out)
}

/// `Self` ids of an object and everything inside it, in document order.
fn ids_inside(w: &mut Work<'_>, id: &str) -> Result<Vec<String>, EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    Ok(descendants(&ix, i).filter_map(|k| ix.attr(t, k, "Self").map(str::to_owned)).collect())
}

/// The fields the template's placeholders name (text `<<Field>>` and image frames), each once.
fn placeholder_fields(w: &Work<'_>) -> Result<Vec<String>, EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let root = ix.roots().next().ok_or_else(|| invalid("missing document"))?;
    let used: Vec<&str> = ix.children(root).iter().filter(|&&k| ix.local(t, k) == "Hyperlink").filter_map(|&k| ix.attr(t, k, "DestinationUniqueKey")).collect();
    let mut out: Vec<String> = Vec::new();
    for &k in ix.children(root) {
        if ix.local(t, k) != "HyperlinkURLDestination" || !ix.attr(t, k, "DestinationUniqueKey").is_some_and(|key| used.contains(&key)) { continue; }
        let url = xml::unescape(ix.attr(t, k, "DestinationURL").unwrap_or(""));
        if let Some(field) = url.strip_prefix("DBF_").or_else(|| url.strip_prefix("IMG_")).or_else(|| url.strip_prefix("QRC_")) && !out.iter().any(|f| f == field) { out.push(field.to_owned()); }
    }
    Ok(out)
}

/// The image and QR code fields' frames of the template: (frame, field, is a QR code field).
fn image_placeholders(w: &Work<'_>) -> Result<Vec<(String, String, bool)>, EditError> {
    let t = w.text("designmap.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    Ok((0..ix.nodes.len()).filter(|&k| matches!(ix.local(t, k), "DataMergeImagePlaceholder" | "DataMergeQrcodePlaceholder"))
        .filter_map(|k| Some((ix.attr(t, k, "PlaceholderPageItem")?.to_owned(), xml::unescape(ix.attr(t, k, "Field")?), ix.local(t, k) == "DataMergeQrcodePlaceholder"))).collect())
}

/// The copies of the image fields' frames: (field, frame in this record's copy), found by position
/// in the copied objects (the copy of an object has the same objects inside, in the same order).
fn resolve(w: &mut Work<'_>, placeholders: &[(String, String, bool)], pairs: &[(String, String)]) -> Result<Vec<(String, String, bool)>, EditError> {
    let mut out = Vec::new();
    for (frame, field, qr) in placeholders {
        for (original, copy) in pairs {
            let before = ids_inside(w, original)?;
            if let Some(k) = before.iter().position(|id| id == frame) {
                if let Some(target) = ids_inside(w, copy)?.get(k) { out.push((field.clone(), target.clone(), *qr)); }
                break;
            }
        }
    }
    Ok(out)
}

/// How a picture sits in its frame (Data Merge options ▸ Fitting): a fit as Object ▸ Fitting does it, or none:
/// the picture at full size, centred in the frame or (without "Center image") at its top left.
#[derive(Clone, Copy)]
enum Fitting {
    Fit(crate::ops::Fit),
    Keep { center: bool },
}

/// Places the record's pictures into its image frames and fits them (a picture that is not given
/// stays an empty frame, as a missing file does).
fn place_images(
    w: &mut Work<'_>, frames: &[(String, String, bool)], fields: &[String], record: &[String],
    fitting: Fitting, graphics: &BTreeMap<String, crate::ops::GraphicSource>, embed: Option<&BTreeMap<String, String>>,
) -> Result<(), EditError> {
    for (field, target, qr) in frames {
        let Some(column) = fields.iter().position(|f| f == field) else { continue };
        let Some(value) = record.get(column) else { continue };
        // a QR code field: the value becomes the code in the frame; no value leaves the frame empty
        if *qr {
            if !value.is_empty() { super::qr::place(w, target, value)?; }
            continue;
        }
        let Some(source) = graphics.get(value) else { continue };
        crate::ops::run(w, &Command::PlaceGraphic { source: source.clone(), id: Some(target.clone()), page: None, point: None, rect: None, layer: None })?;
        match fitting {
            Fitting::Fit(crate::ops::Fit::FrameToContent) => {
                // Data Merge grows or shrinks the frame from its top left (measured), Object ▸ Fitting from its middle
                let before = w.bounds_of(target)?;
                crate::ops::run(w, &Command::Fit { id: target.clone(), how: crate::ops::Fit::FrameToContent })?;
                let after = w.bounds_of(target)?;
                crate::ops::run(w, &Command::Move { ids: vec![target.clone()], by: [before[0] - after[0], before[1] - after[1]] })?;
            }
            Fitting::Fit(how) => crate::ops::run(w, &Command::Fit { id: target.clone(), how })?,
            Fitting::Keep { center: true } => {}
            Fitting::Keep { center: false } => {
                // placing centres the picture at full size; its top left goes to the frame's
                let frame = w.bounds_of(target)?;
                let [width, height] = if matches!(source.orientation, 6 | 8) { [source.size[1], source.size[0]] } else { source.size };
                let by = [(width - (frame[2] - frame[0])) / 2.0, (height - (frame[3] - frame[1])) / 2.0];
                crate::ops::run(w, &Command::MoveContent { id: target.clone(), by })?;
            }
        }
        // "Link Images" off: the picture is embedded (measured: `StoredState="Embedded"` and the file's bytes as `Contents`, the link's address stays)
        if let Some(data) = embed.and_then(|e| e.get(value)) { crate::ops::run(w, &Command::EmbedLink { id: target.clone(), data: data.clone() })?; }
    }
    Ok(())
}

/// Data Merge ▸ Preview (build-c): the document's own placeholders show one record's values in place: the text fields take the
/// values (as the merge puts them, "remove blank lines" too) and a QR code field's frame the code; nothing is copied, the data source
/// and the field links stay. The panel sends it inside a preview session (`previewApply`), so it is taken back exactly and never
/// enters the undo history; sent as an edit of its own it fills the placeholders for good. Picture fields need the host's files and stay as they are.
pub(crate) fn fill_record(w: &mut Work<'_>, fields: &[String], record: &[String], remove_blank_lines: &bool) -> Result<(), EditError> {
    if placeholder_fields(w)?.iter().any(|f| !fields.contains(f)) {
        return Err(invalid("There is at least one data placeholder that cannot be found in the data source. Make sure all the placeholders correspond to the fields in the data source."));
    }
    let owners: Vec<String> = spreads(w)?.into_iter().map(|s| s.id).collect();
    for story in stories_of(w, &owners)? { fill(w, &story, fields, record, *remove_blank_lines)?; }
    for (frame, field, qr) in image_placeholders(w)? {
        if !qr { continue; }
        let Some(value) = fields.iter().position(|f| *f == field).and_then(|at| record.get(at)) else { continue };
        if !value.is_empty() { super::qr::place(w, &frame, value)?; }
    }
    Ok(())
}

pub(crate) fn run(
    w: &mut Work<'_>, fields: &[String], records: &[Vec<String>], range: &str, multiple: &bool,
    arrange: &String, row_spacing: &f64, column_spacing: &f64, margins: &[f64; 4], remove_blank_lines: &bool,
    fitting: &str, center_image: &bool, graphics: &BTreeMap<String, crate::ops::GraphicSource>, embed: &Option<BTreeMap<String, String>>,
    link: &Option<String>,
) -> Result<(), EditError> {
    let fitting = match fitting {
        "proportional" | "" => Fitting::Fit(crate::ops::Fit::Proportionally),
        "fill" => Fitting::Fit(crate::ops::Fit::Fill),
        "contentToFrame" => Fitting::Fit(crate::ops::Fit::ContentToFrame),
        "frameToContent" => Fitting::Fit(crate::ops::Fit::FrameToContent),
        "none" => Fitting::Keep { center: *center_image },
        _ => return Err(invalid("fitting is proportional, fill, contentToFrame, frameToContent or none")),
    };
    // a placeholder of a field the selected data source does not have is an error (measured, `datamerge-c4-t`: the text stays `<<Field>>` in
    // the template and Create Merged Document says so, whether some or none of the placeholders still match)
    if placeholder_fields(w)?.iter().any(|f| !fields.contains(f)) {
        return Err(invalid("There is at least one data placeholder that cannot be found in the data source. Make sure all the placeholders correspond to the fields in the data source."));
    }
    let placeholders = image_placeholders(w)?;
    // (template container, record's copy of it) per record, for the pictures
    let mut frames: Vec<Vec<(String, String, bool)>> = Vec::new();
    let picked = select(range, records.len())?;
    if picked.is_empty() { return Err(invalid("no records to merge")); }
    if ![*row_spacing, *column_spacing, margins[0], margins[1], margins[2], margins[3]].iter().all(|v| v.is_finite()) || *row_spacing < 0.0 || *column_spacing < 0.0 {
        return Err(invalid("spacing and margins must be finite numbers, the spacings not negative"));
    }
    if !matches!(arrange.as_str(), "rows" | "columns") { return Err(invalid("arrange is rows or columns")); }
    let template = spreads(w)?;
    if template.is_empty() { return Err(invalid("the document has no pages")); }
    let page_count: usize = template.iter().map(|s| s.pages.len()).sum();
    let mut scopes: Vec<Vec<String>> = Vec::new();
    if *multiple {
        if page_count > 1 { return Err(invalid("Multiple records per page not allowed for a multi-page document.")); }
        let first = &template[0];
        if first.items.is_empty() { return Err(invalid("the page has no objects to merge")); }
        let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for id in &first.items {
            let b = w.bounds_of(id)?;
            bounds = [bounds[0].min(b[0]), bounds[1].min(b[1]), bounds[2].max(b[2]), bounds[3].max(b[3])];
        }
        let page = &first.pages[0];
        let (cw, ch) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
        let (left, top, right, bottom) = (margins[1], margins[0], page.size[0] - margins[3], page.size[1] - margins[2]);
        let eps = 1e-6;
        // (page, x, y) of every record's top left in page coordinates
        let mut spots: Vec<(usize, f64, f64)> = Vec::new();
        let (mut p, mut x, mut y) = (0usize, left, top);
        for n in 0..picked.len() {
            if n > 0 {
                let across = (x + cw + column_spacing, y);
                let down = (x, y + ch + row_spacing);
                let fits = |s: (f64, f64)| s.0 + cw <= right + eps && s.1 + ch <= bottom + eps;
                let (a, b) = if arrange == "rows" { (across, (left, down.1)) } else { (down, (across.0, top)) };
                if fits(a) { (x, y) = a; } else if fits(b) { (x, y) = b; } else { p += 1; (x, y) = (left, top); }
            }
            spots.push((p, x, y));
        }
        let pages = p + 1;
        let mut spread_ids = vec![first.id.clone()];
        if pages > 1 {
            let before = w.created.len();
            crate::ops::run(w, &Command::InsertPages { count: pages - 1, before: None, master: None })?;
            let news = w.created[before..].to_vec();
            spread_ids.extend(spreads_of_pages(w, news)?);
        }
        let (ox, oy) = (page.origin[0], page.origin[1]);
        for &(pg, x, y) in &spots {
            let before = w.created.len();
            crate::ops::run(w, &Command::Duplicate { ids: first.items.clone(), by: [0.0, 0.0] })?;
            let made = w.created[before..].to_vec();
            let copies = spread_items(w, made, &first.items);
            crate::ops::run(w, &Command::MoveToSpread { ids: copies.clone(), spread: spread_ids[pg].clone(), by: [x - (bounds[0] - ox), y - (bounds[1] - oy)] })?;
            if copies.len() != first.items.len() { return Err(invalid("the page objects could not be copied")); }
            let pairs: Vec<(String, String)> = first.items.iter().cloned().zip(copies.iter().cloned()).collect();
            frames.push(resolve(w, &placeholders, &pairs)?);
            scopes.push(copies);
        }
        crate::ops::run(w, &Command::Delete { ids: first.items.clone() })?;
        set_margins(w, *margins)?;
    } else {
        let mut copies: Vec<Vec<String>> = vec![template.iter().map(|s| s.id.clone()).collect()];
        // record 1 stays where it is; the others' objects are matched to the template's by position
        let identity: Vec<(String, String)> = template.iter().map(|s| (s.id.clone(), s.id.clone())).collect();
        frames.push(resolve(w, &placeholders, &identity)?);
        if picked.len() > 1 {
            if template.len() == 1 {
                // every copy is made from the template (a copy's own story is not composed yet); the copies are
                // alike, each lands right after the template, so the last made is the second record's
                let mut made = Vec::new();
                for _ in 1..picked.len() {
                    let before = w.created.len();
                    crate::ops::run(w, &Command::DuplicateSpread { spread: template[0].id.clone() })?;
                    let pages = w.created[before..].to_vec();
                    made.push(spreads_of_pages(w, pages)?.into_iter().next().ok_or_else(|| invalid("spread not duplicated"))?);
                }
                made.reverse();
                for id in made {
                    frames.push(resolve(w, &placeholders, &[(template[0].id.clone(), id.clone())])?);
                    copies.push(vec![id]);
                }
            } else {
                let before = w.created.len();
                let more = template.len() * (picked.len() - 1);
                crate::ops::run(w, &Command::InsertPages { count: more, before: None, master: None })?;
                let news = w.created[before..].to_vec();
                let mut news = spreads_of_pages(w, news)?.into_iter();
                for _ in 1..picked.len() {
                    let mut record = Vec::new();
                    let mut pairs = Vec::new();
                    for source in &template {
                        let target = news.next().ok_or_else(|| invalid("page missing"))?;
                        let before = w.created.len();
                        crate::ops::run(w, &Command::Duplicate { ids: source.items.clone(), by: [0.0, 0.0] })?;
                        let made = w.created[before..].to_vec();
                        let made = spread_items(w, made, &source.items);
                        if made.len() != source.items.len() { return Err(invalid("the page objects could not be copied")); }
                        pairs.extend(source.items.iter().cloned().zip(made.iter().cloned()));
                        crate::ops::run(w, &Command::MoveToSpread { ids: made, spread: target.clone(), by: [0.0, 0.0] })?;
                        record.push(target);
                    }
                    frames.push(resolve(w, &placeholders, &pairs)?);
                    copies.push(record);
                }
            }
        }
        scopes = copies;
    }
    // what each record's stories got (Update Content in Data Fields finds the texts again by them)
    let mut filled: Vec<super::merged::Filled> = Vec::new();
    // and which QR code each record's QR code frames got (rest-c1: Update Content in Data Fields makes them again)
    let mut codes: Vec<super::merged::CodeFilled> = Vec::new();
    for (n, &index) in picked.iter().enumerate() {
        for story in stories_of(w, &scopes[n])? {
            let got = fill(w, &story, fields, &records[index], *remove_blank_lines)?;
            if !got.is_empty() { filled.push(super::merged::Filled { story, record: index, fields: got }); }
        }
        place_images(w, &frames[n], fields, &records[index], fitting, graphics, embed.as_ref())?;
        for (field, frame, _) in frames[n].iter().filter(|f| f.2) {
            let value = fields.iter().position(|f| f == field).and_then(|c| records[index].get(c)).cloned().unwrap_or_default();
            codes.push(super::merged::CodeFilled { frame: frame.clone(), field: field.clone(), record: index, value });
        }
    }
    // the merged document has no data source (measured: no `DataMerge` element, the options stay)
    crate::hyperlinks::datasource::remove(w)?;
    drop_merge_links(w)?;
    // Folio-only: the merged document remembers its template, data source and field texts (`folio.datamerge` label, key `merged`)
    if let Some(link) = link { super::merged::remember(w, link, &filled, &codes)?; }
    Ok(())
}
