//! Runtime text geometry. Coordinates are page points, indexes Unicode scalars, including
//! CR, tables and anchors (oracle text-special-index/text-special-delete).
//! Source ranges come from shaping clusters, never from glyph counts (ligatures / jamo).
use folio_model::layout::{DocumentLayout, LaidLine, TextCluster};
use folio_model::{Affine, Document, ItemKind, PageItem, Point, Rect};
use crate::wire;
use serde_json::{Value, json};
pub mod overrides;

struct Frame<'a> {
    id: &'a str,
    story: &'a str,
    page: usize,
    transform: Affine,
    left: f64,
    lines: &'a [LaidLine],
}

/// The text frames a person can edit (visible and unlocked, on visible unlocked layers).
fn frames<'a>(doc: &'a Document, layout: &'a DocumentLayout) -> Vec<Frame<'a>> {
    shown_frames(doc, layout, true)
}

/// The text frames on screen; `editable`: only those a person can edit.
fn shown_frames<'a>(doc: &'a Document, layout: &'a DocumentLayout, editable: bool) -> Vec<Frame<'a>> {
    fn walk<'a>(
        items: &'a [PageItem],
        parent: Affine,
        pages: &[(usize, Rect)],
        doc: &Document,
        layout: &'a DocumentLayout,
        editable: bool,
        out: &mut Vec<Frame<'a>>,
    ) {
        for item in items {
            if !item.visible
                || (editable && item.locked)
                || doc
                    .layers
                    .iter()
                    .any(|l| Some(&l.id) == item.layer.as_ref() && (!l.visible || (editable && l.locked)))
            {
                continue;
            }
            let transform = parent.then_inner(&item.transform);
            match &item.kind {
                ItemKind::Group { children } => walk(children, transform, pages, doc, layout, editable, out),
                _ => {
                    let Some(tf) = item.text_frame() else { continue; };
                    let Some(laid) = layout.frames.get(&item.id) else {
                        continue;
                    };
                    let center = folio_model::paths_bounds(&item.paths)
                        .map(|r| Point::new((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0))
                        .unwrap_or_default();
                    let center = transform.apply(center);
                    let Some((page, r)) = pages
                        .iter()
                        .min_by(|a, b| distance(center, &a.1).total_cmp(&distance(center, &b.1)))
                    else {
                        continue;
                    };
                    out.push(Frame {
                        id: &item.id,
                        story: &tf.story,
                        page: *page,
                        transform: Affine::translate(-r.left, -r.top).then_inner(&transform),
                        left: folio_model::paths_bounds(&item.paths).map_or(0.0, |r|r.left),
                        lines: &laid.lines,
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    // the parents too: a parent's text is edited like a page's (its pages count on from MASTER_PAGE_BASE)
    for spread in doc.spreads.iter().chain(&doc.master_spreads) {
        let pages: Vec<_> = spread
            .pages
            .iter()
            .map(|p| (p.index, p.transform.apply_rect(&p.bounds)))
            .collect();
        walk(
            &spread.items,
            Affine::IDENTITY,
            &pages,
            doc,
            layout,
            editable,
            &mut out,
        );
    }
    out
}

/// A line of one text: a story's own line, or a line of a table cell's text (which counts its
/// characters from 0 and has clusters of its own).
struct Owned<'a> {
    f: &'a Frame<'a>,
    owner: &'a str,
    clusters: &'a [TextCluster],
    start: usize,
    end: usize,
}

/// Every line with the text it belongs to (a cell's lines belong to the cell).
fn all_lines<'a>(fs: &'a [Frame<'a>]) -> impl Iterator<Item = Owned<'a>> {
    fs.iter().flat_map(|f| f.lines.iter().map(move |l| match &l.cell {
        Some(c) => Owned { f, owner: c.id.as_str(), clusters: &c.clusters, start: c.start, end: c.end },
        None => Owned { f, owner: f.story, clusters: &l.text_clusters, start: l.start, end: l.end },
    }))
}

/// The lines of the text `owner` (a story or a cell), in frame and line order.
fn owned<'a>(fs: &'a [Frame<'a>], owner: &str) -> Vec<Owned<'a>> {
    all_lines(fs).filter(|o| o.owner == owner).collect()
}

fn distance(p: Point, r: &Rect) -> f64 {
    (p.x - p.x.clamp(r.left, r.right)).powi(2) + (p.y - p.y.clamp(r.top, r.bottom)).powi(2)
}
fn box_of(c: &TextCluster) -> Rect {
    Rect {
        left: c.x0.min(c.x1),
        right: c.x0.max(c.x1),
        top: c.top,
        bottom: c.bottom,
    }
}
fn x_at(c: &TextCluster, at: usize) -> f64 {
    c.x0 + (c.x1 - c.x0)
        * (at.saturating_sub(c.start) as f64 / c.end.saturating_sub(c.start).max(1) as f64)
            .clamp(0.0, 1.0)
}
fn rect_json(f: &Frame, r: Rect) -> Value {
    rect_json_mapped(f, r, None)
}
/// `map`: text on a path, where the straight line's box `r` goes (`TextCluster::path_map`). The quad follows the path;
/// `rect` is its bounding box.
fn rect_json_mapped(f: &Frame, r: Rect, map: Option<Affine>) -> Value {
    let points = [
        Point::new(r.left, r.top),
        Point::new(r.right, r.top),
        Point::new(r.right, r.bottom),
        Point::new(r.left, r.bottom),
    ];
    let to_page = |p: Point| f.transform.apply(map.map_or(p, |m| m.apply(p)));
    let quad: Vec<_> = points
        .into_iter()
        .map(|p| {
            let p = to_page(p);
            [p.x, p.y]
        })
        .collect();
    let b = match map {
        None => f.transform.apply_rect(&r),
        Some(_) => {
            let (xs, ys): (Vec<f64>, Vec<f64>) = quad.iter().map(|q| (q[0], q[1])).unzip();
            Rect { left: xs.iter().cloned().fold(f64::INFINITY, f64::min), right: xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                   top: ys.iter().cloned().fold(f64::INFINITY, f64::min), bottom: ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max) }
        }
    };
    json!({"page":f.page,"frame":f.id,"rect":[b.left,b.top,b.right,b.bottom],"quad":quad})
}

fn caret(fs: &[Frame], story: &str, at: usize, upstream: bool, text: &[char]) -> Value {
    let mut candidates = Vec::new();
    for line in owned(fs, story) {
        for c in line.clusters {
            if c.start <= at && at <= c.end {
                let score = if upstream {
                    usize::from(c.end == at)
                } else {
                    usize::from(c.start == at)
                };
                candidates.push((score, line.f, (line.start, line.end), c));
            }
        }
    }
    let Some((_, f, (line_start, line_end), c)) = candidates.into_iter().max_by_key(|x| x.0) else {
        return Value::Null;
    };
    let x = x_at(c, at);
    let mut v = rect_json_mapped(
        f,
        Rect {
            left: x,
            right: x,
            top: c.top,
            bottom: c.bottom,
        },
        c.path_map,
    );
    // End goes before an explicit CR/forced break, while soft-wrap End uses upstream.
    let end = if line_end > line_start
        && text
            .get(line_end - 1)
            .is_some_and(|c| matches!(c, '\r' | '\n' | '\u{2028}'))
    {
        line_end - 1
    } else {
        line_end
    };
    v["lineStart"] = json!(line_start);
    v["lineEnd"] = json!(end);
    v["lineSelectionEnd"] = json!(line_end);
    v
}

/// Type ▸ Show Hidden Characters: the marks InDesign draws on `page` for spaces, tabs,
/// paragraph ends, forced line breaks and the end of each story, in page coordinates. Each mark
/// is the character's advance on the baseline (`from` → `to`) and the point above `from` at the
/// top of the line's text (`up`), so the mark can be drawn along a turned frame; `frame` is the
/// text frame (its layer gives InDesign's colour). A story's text comes from `text`.
///
/// With `notes`, only the anchors of the editorial notes (`kind` "note", a zero-width U+FEFF in
/// the story): `up` is then the point a type size above the baseline, the size InDesign scales
/// its note marker by (an hourglass 0.5 wide, from 0.7 above to 0.1 below the baseline, centred
/// on the anchor; measured in InDesign 2025, artifacts/notes-view-20261025).
fn marks(doc: &Document, layout: &DocumentLayout, page: usize, notes: bool, text: &dyn Fn(&str) -> Option<Vec<char>>) -> Value {
    let fs = shown_frames(doc, layout, false);
    let mut stories: std::collections::HashMap<&str, Option<Vec<char>>> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for f in fs.iter().filter(|f| f.page == page) {
        let Some(chars) = stories.entry(f.story).or_insert_with(|| text(f.story)).as_ref() else {
            continue;
        };
        let at = |x: f64, y: f64| {
            let p = f.transform.apply(Point::new(x, y));
            [p.x, p.y]
        };
        for line in f.lines.iter().filter(|l| l.cell.is_none()) {
            let mut last: Option<&TextCluster> = None;
            for c in &line.text_clusters {
                for k in c.start..c.end.max(c.start + 1) {
                    if notes {
                        if chars.get(k) == Some(&'\u{feff}') {
                            let x = x_at(c, k);
                            // the type size of the glyph the anchor follows (else the first one)
                            let size = line.glyphs.iter().rev().find(|g| g.x <= x + 0.01).or(line.glyphs.first())
                                .map_or((line.baseline - c.top) / 0.75, |g| g.size);
                            out.push(json!({"kind": "note", "frame": f.id, "from": at(x, line.baseline), "to": at(x, line.baseline), "up": at(x, line.baseline - size)}));
                        }
                        continue;
                    }
                    let kind = match chars.get(k) {
                        Some(' ') => "space",
                        Some('\u{a0}' | '\u{202f}') => "nonbreakingSpace",
                        Some('\u{2000}'..='\u{200a}' | '\u{3000}' | '\u{205f}') => "otherSpace",
                        Some('\t') => "tab",
                        Some('\r') => "paragraph",
                        Some('\n' | '\u{2028}') => "lineBreak",
                        _ => continue,
                    };
                    let (x0, x1) = (x_at(c, k), x_at(c, k + 1));
                    out.push(json!({"kind": kind, "frame": f.id, "from": at(x0, line.baseline), "to": at(x1, line.baseline), "up": at(x0, c.top)}));
                }
                last = Some(c);
            }
            // the end of the story, after its last character
            if !notes && line.end >= chars.len() {
                let (x, top) = last.map_or((line.left, line.baseline - 8.0), |c| (c.x0.max(c.x1), c.top));
                out.push(json!({"kind": "end", "frame": f.id, "from": at(x, line.baseline), "to": at(x, line.baseline), "up": at(x, top)}));
            }
        }
    }
    json!({"type": "textMarks", "page": page, "marks": out})
}

/// PDF Comments (the eye button's drawing): the boxes the characters `start..end` of `story` take on `page`, one per line. Each box is
/// the stretch of the line the characters cover along the baseline (`from` → `to`), the point above `from` at the top of the line's
/// clusters (`up`) and the point below it at their bottom (`down`), all in page points, so a turned frame turns the box.
pub(crate) fn range_boxes(doc: &Document, layout: &DocumentLayout, page: usize, story: &str, start: usize, end: usize) -> Vec<[[f64; 2]; 4]> {
    let mut out = Vec::new();
    for f in shown_frames(doc, layout, false).iter().filter(|f| f.page == page && f.story == story) {
        let at = |x: f64, y: f64| {
            let p = f.transform.apply(Point::new(x, y));
            [p.x, p.y]
        };
        for line in f.lines.iter().filter(|l| l.cell.is_none()) {
            let (mut left, mut right, mut top, mut bottom) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
            for c in line.text_clusters.iter().filter(|c| c.end > c.start && c.end > start && c.start < end) {
                let (a, b) = (x_at(c, c.start.max(start)), x_at(c, c.end.min(end)));
                left = left.min(a.min(b));
                right = right.max(a.max(b));
                top = top.min(c.top);
                bottom = bottom.max(c.bottom);
            }
            if right > left {
                out.push([at(left, line.baseline), at(right, line.baseline), at(left, top), at(left, bottom)]);
            }
        }
    }
    out
}

/// The characters of one story that a box on a page covers: `(story, start, end)`, end exclusive. A character is covered when the
/// middle of its box is inside `rect` (page points, the page's top left corner is the origin); the story with most characters wins.
/// PDF Comments: a text markup's quads give the commented text (the story and the characters a PDFCommentTextDestination names).
pub(crate) fn text_in_rect(doc: &Document, layout: &DocumentLayout, page: usize, rects: &[Rect]) -> Option<(String, usize, usize)> {
    let fs = frames(doc, layout);
    let mut found: std::collections::BTreeMap<&str, (usize, usize, usize)> = std::collections::BTreeMap::new();
    for line in all_lines(&fs).filter(|o| o.f.page == page && o.owner == o.f.story) {
        for c in line.clusters.iter().filter(|c| c.end > c.start) {
            let b = box_of(c);
            let middle = line.f.transform.apply(Point::new((b.left + b.right) / 2.0, (b.top + b.bottom) / 2.0));
            if rects.iter().any(|r| middle.x >= r.left && middle.x <= r.right && middle.y >= r.top && middle.y <= r.bottom) {
                let e = found.entry(line.owner).or_insert((usize::MAX, 0, 0));
                e.0 = e.0.min(c.start);
                e.1 = e.1.max(c.end);
                e.2 += c.end - c.start;
            }
        }
    }
    found.into_iter().max_by_key(|(_, v)| v.2).map(|(story, (start, end, _))| (story.to_owned(), start, end))
}

/// The page a character of a story is on (the page of the frame whose lines hold it).
pub(crate) fn story_page(doc: &Document, layout: &DocumentLayout, story: &str, at: usize) -> Option<usize> {
    let fs = frames(doc, layout);
    all_lines(&fs).filter(|o| o.owner == story && o.owner == o.f.story).find(|o| o.start <= at && at < o.end.max(o.start + 1)).map(|o| o.f.page)
}

/// Where `start..end` of a story sits on screen: one entry per line it touches, with the page,
/// the frame, and the baseline from the first to the last of those characters (page points).
/// Screen-only marks that follow words (the dynamic-spelling underline) are drawn from this.
pub(crate) fn baseline_segments(doc: &Document, layout: &DocumentLayout, story: &str, start: usize, end: usize) -> Vec<Value> {
    let mut out = Vec::new();
    for f in shown_frames(doc, layout, false).iter().filter(|f| f.story == story) {
        for line in f.lines.iter().filter(|l| l.cell.is_none()) {
            let (mut x0, mut x1) = (f64::INFINITY, f64::NEG_INFINITY);
            for c in line.text_clusters.iter().filter(|c| c.path_map.is_none() && c.start < end && c.end > start) {
                let (a, b) = (x_at(c, start.max(c.start)), x_at(c, end.min(c.end)));
                x0 = x0.min(a.min(b));
                x1 = x1.max(a.max(b));
            }
            if x0 < x1 {
                let (p, q) = (f.transform.apply(Point::new(x0, line.baseline)), f.transform.apply(Point::new(x1, line.baseline)));
                out.push(json!({"page": f.page, "frame": f.id, "from": [p.x, p.y], "to": [q.x, q.y]}));
            }
        }
    }
    out
}

/// The table `id` of a story (not looking into cells).
fn table_of<'d>(doc: &'d Document, story: &str, id: &str) -> Option<&'d folio_model::Table> {
    doc.stories.get(story)?.paragraphs.iter().flat_map(|p| &p.runs).flat_map(|r| &r.tables).find(|t| t.id == id)
}

/// The cell of `t` covering [column, row].
fn cell_at(t: &folio_model::Table, column: usize, row: usize) -> Option<&folio_model::TableCell> {
    t.cells.iter().find(|c| (c.column..c.column + c.column_span.max(1)).contains(&column) && (c.row..c.row + c.row_span.max(1)).contains(&row))
}

/// The rectangle of cells from `a` to `b` ([column, row]) grown until no merged cell reaches
/// out of it (as InDesign selects across merged cells): first and last [column, row].
fn cell_range(t: &folio_model::Table, a: [usize; 2], b: [usize; 2]) -> ([usize; 2], [usize; 2]) {
    let (mut c0, mut c1, mut r0, mut r1) = (a[0].min(b[0]), a[0].max(b[0]), a[1].min(b[1]), a[1].max(b[1]));
    loop {
        let mut grew = false;
        for c in &t.cells {
            let (cs, ce, rs, re) = (c.column, c.column + c.column_span.max(1) - 1, c.row, c.row + c.row_span.max(1) - 1);
            if ce < c0 || cs > c1 || re < r0 || rs > r1 {
                continue;
            }
            if cs < c0 { c0 = cs; grew = true; }
            if ce > c1 { c1 = ce; grew = true; }
            if rs < r0 { r0 = rs; grew = true; }
            if re > r1 { r1 = re; grew = true; }
        }
        if !grew {
            return ([c0, r0], [c1, r1]);
        }
    }
}

/// Table cells: `tableHit` (the cell under a page point), `tableCells` (a range of cells and
/// where it shows, for the selection), `tableLines` (where the column and row lines are, for dragging them).
fn table_command(req: &Value, doc: &Document, layout: &DocumentLayout) -> Result<Value, String> {
    let fs = frames(doc, layout);
    if req["cmd"] == "tableHit" {
        let page = wire::required_index(req, "page", "page required")?;
        let x = wire::required_number(req, "x", "x required")?;
        let y = wire::required_number(req, "y", "y required")?;
        let p = Point::new(x, y);
        for f in fs.iter().filter(|f| f.page == page) {
            let Some(inv) = f.transform.invert() else { continue };
            let q = inv.apply(p);
            for line in f.lines {
                let Some(g) = &line.table else { continue };
                let (Some(&x0), Some(&x1)) = (g.columns.first(), g.columns.last()) else { continue };
                if q.x < x0 || q.x > x1 {
                    continue;
                }
                let Some(&(row, _, _)) = g.rows.iter().find(|r| r.1 <= q.y && q.y <= r.2) else { continue };
                let column = g.columns.windows(2).position(|w| q.x <= w[1]).unwrap_or(0);
                let Some(t) = table_of(doc, f.story, &g.table) else { continue };
                let Some(c) = cell_at(t, column, row) else { continue };
                return Ok(json!({"type": "tableHit", "hit": {
                    "story": f.story, "frame": f.id, "page": f.page, "table": g.table,
                    "cell": [c.column, c.row], "id": c.id, "span": [c.column_span.max(1), c.row_span.max(1)],
                    "rows": t.rows.len(), "columns": t.columns.len(), "header": t.header_rows, "footer": t.footer_rows,
                }}));
            }
        }
        return Ok(json!({"type": "tableHit", "hit": null}));
    }
    if req["cmd"] == "tableLines" {
        // Read-only: where the table's column and row lines are on the pages (already laid out; nothing is computed here), for dragging them.
        // The table is `table`, or the one on `page` whose grid (grown by `tol` points) holds the point (`x`, `y`).
        // `line` numbers are `dragTableLine`'s: line k lies between column (row) k-1 and k; the outer right (bottom) edge is the last one.
        let near = |f: &Frame, g: &folio_model::layout::TableGrid| -> bool {
            let (Some(p), Some(x), Some(y)) = (req["page"].as_u64(), req["x"].as_f64(), req["y"].as_f64()) else { return false };
            let tol = req["tol"].as_f64().unwrap_or(0.0);
            let Some(inv) = f.transform.invert() else { return false };
            let q = inv.apply(Point::new(x, y));
            let (Some(&x0), Some(&x1)) = (g.columns.first(), g.columns.last()) else { return false };
            let (Some(first), Some(last)) = (g.rows.first(), g.rows.last()) else { return false };
            f.page == p as usize && q.x >= x0 - tol && q.x <= x1 + tol && q.y >= first.1 - tol && q.y <= last.2 + tol
        };
        let id = match req["table"].as_str() {
            Some(id) => id.to_string(),
            None => match fs.iter().find_map(|f| f.lines.iter().filter_map(|l| l.table.as_ref()).find(|g| near(f, g))) {
                Some(g) => g.table.clone(),
                None => return Ok(json!({"type": "tableLines", "table": null, "pieces": []})),
            },
        };
        let mut pieces = Vec::new();
        for f in &fs {
            let Some(g) = f.lines.iter().filter_map(|l| l.table.as_ref()).find(|g| g.table == id) else { continue };
            let (Some(&x0), Some(_)) = (g.columns.first(), g.columns.last()) else { continue };
            let (Some(first), Some(last)) = (g.rows.first(), g.rows.last()) else { continue };
            let at = |x: f64, y: f64| { let p = f.transform.apply(Point::new(x, y)); json!([p.x, p.y]) };
            // `size`: how big the column (row) before the line shows (a row grown by its text is higher than its stored height); `min` (rows): what its text needs at least
            let columns: Vec<_> = (1..g.columns.len()).map(|k| json!({"line": k, "from": at(g.columns[k], first.1), "to": at(g.columns[k], last.2), "size": g.columns[k] - g.columns[k - 1]})).collect();
            let rows: Vec<_> = g.rows.iter().enumerate().map(|(k, r)| json!({"line": r.0 + 1, "from": at(x0, r.2), "to": at(*g.columns.last().unwrap_or(&x0), r.2), "size": r.2 - r.1, "min": g.content_min.get(k).copied().unwrap_or(0.0)})).collect();
            pieces.push(json!({"frame": f.id, "page": f.page, "columns": columns, "rows": rows}));
        }
        let t = fs.iter().find_map(|f| table_of(doc, f.story, &id));
        return Ok(json!({"type": "tableLines", "table": id, "pieces": pieces,
            "columns": t.map_or(0, |t| t.columns.len()), "rows": t.map_or(0, |t| t.rows.len()), "header": t.map_or(0, |t| t.header_rows)}));
    }
    // tableCells
    let story = wire::required_text(req, "story", "story required")?;
    let id = wire::required_text(req, "table", "table required")?;
    let pair = |v: &Value| -> Option<[usize; 2]> { Some([v[0].as_u64()? as usize, v[1].as_u64()? as usize]) };
    let (a, b) = (pair(&req["from"]).ok_or("from required")?, pair(&req["to"]).ok_or("to required")?);
    let t = table_of(doc, story, id).ok_or("no such table")?;
    let (from, to) = cell_range(t, a, b);
    let mut rects = Vec::new();
    for f in fs.iter().filter(|f| f.story == story) {
        for line in f.lines {
            let Some(g) = line.table.as_ref().filter(|g| g.table == id) else { continue };
            let rows: Vec<_> = g.rows.iter().filter(|r| (from[1]..=to[1]).contains(&r.0)).collect();
            let (Some(first), Some(last)) = (rows.first(), rows.last()) else { continue };
            let (Some(&left), Some(&right)) = (g.columns.get(from[0]), g.columns.get(to[0] + 1)) else { continue };
            rects.push(rect_json(f, Rect { left, top: first.1, right, bottom: last.2 }));
        }
    }
    Ok(json!({"type": "tableCells", "from": from, "to": to, "rects": rects}))
}

fn geometry(
    req: &Value,
    doc: &Document,
    layout: &DocumentLayout,
    text: Option<&str>,
) -> Result<Value, String> {
    let fs = frames(doc, layout);
    let cmd = req["cmd"].as_str().unwrap_or("");
    if cmd == "textHit" {
        let page = wire::required_index(req, "page", "textHit needs page")?;
        let x = wire::required_finite_number(req, "x", "textHit needs finite x")?;
        let y = wire::required_finite_number(req, "y", "textHit needs finite y")?;
        // A normal click can enter a story or its footnotes. Table cells use tableHit first.
        let only = req["story"].as_str();
        let mut best: Option<(f64, &Frame, &str, &TextCluster, Point)> = None;
        for line in all_lines(&fs)
            .filter(|o| o.f.page == page && req["frame"].as_str().is_none_or(|id| id == o.f.id) && only.map_or(o.owner == o.f.story || folio_idml::footnotes::address(o.owner).is_some(), |s| s == o.owner))
        {
            let f = line.f;
            let Some(inv) = f.transform.invert() else {
                continue;
            };
            let p = inv.apply(Point::new(x, y));
            for c in line.clusters {
                // Text on a path: look at the click from the cluster's own (straight) frame.
                let p = c.path_map.and_then(|m| m.invert()).map_or(p, |inv| inv.apply(p));
                // Clicking far to the right of a short line still belongs to that line,
                // not to a longer line below it. Choose the row before its horizontal stop.
                let b = box_of(c);
                let dy = (p.y - p.y.clamp(b.top, b.bottom)).abs();
                let dx = (p.x - p.x.clamp(b.left, b.right)).abs();
                let d = dy * 1_000_000.0 + dx;
                if best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((d, f, line.owner, c, p));
                }
            }
        }
        let Some((_, f, owner, c, p)) = best else {
            return Ok(json!({"type":cmd,"hit":null}));
        };
        let t = if (c.x1 - c.x0).abs() < 1e-8 {
            0.0
        } else {
            ((p.x - c.x0) / (c.x1 - c.x0)).clamp(0.0, 1.0)
        };
        let at = c.start + ((c.end - c.start) as f64 * t).round() as usize;
        return Ok(json!({"type":cmd,"hit":{"story":owner,"at":at,"frame":f.id,"page":f.page}}));
    }
    let story = wire::required_text(req, "story", "story required")?;
    let chars: Vec<_> = text.ok_or("story text unavailable")?.chars().collect();
    let at = req["at"]
        .as_u64()
        .or_else(|| req["end"].as_u64())
        .ok_or("at/end required")? as usize;
    let start = req["start"].as_u64().unwrap_or(at as u64) as usize;
    if at > chars.len() || start > chars.len() {
        return Err("character index outside story".into());
    }
    if cmd == "textNavigate" {
        let current=caret(&fs,story,at,req["affinity"]=="upstream",&chars);
        let mut lines:Vec<_>=owned(&fs,story).into_iter().filter(|l|!l.clusters.is_empty()).collect();
        lines.sort_by_key(|l|(l.start,l.end));
        let Some(index)=lines.iter().position(|l|current["frame"]==l.f.id && current["lineStart"].as_u64()==Some(l.start as u64)) else {
            return Ok(json!({"type":cmd,"at":at,"x":req["x"],"affinity":"downstream"}));
        };
        let from=&lines[index];
        let c=from.clusters.iter().find(|c|c.start<=at && at<=c.end).expect("caret identifies a line containing the requested character");
        let desired=req["x"].as_f64().filter(|x|x.is_finite()).unwrap_or_else(||x_at(c,at)-from.f.left);
        let next=match req["direction"].as_str() {
            Some("up")=>index.saturating_sub(1),Some("down")=>(index+1).min(lines.len()-1),_=>return Err("direction must be up or down".into())
        };
        let line=&lines[next];
        let end=if line.end>line.start && chars.get(line.end-1).is_some_and(|c|matches!(c,'\r'|'\n'|'\u{2028}')) {line.end-1} else {line.end};
        let stop=line.clusters.iter().flat_map(|c|[c.start,c.end].map(|i|(i,(x_at(c,i)-line.f.left-desired).abs())))
            .filter(|(i,_)|*i<=end).min_by(|a,b|a.1.total_cmp(&b.1)).map(|(i,_)|i).unwrap_or(line.start);
        return Ok(json!({"type":cmd,"at":stop,"x":desired,"affinity":if stop==line.end {"upstream"} else {"downstream"}}));
    }
    let mut rects = Vec::new();
    if cmd == "textSelection" {
        let (lo, hi) = (start.min(at), start.max(at));
        for line in owned(&fs, story) {
            let f = line.f;
            // Text on a path: one quad per cluster, following the path (no merging into a straight band).
            for c in line.clusters.iter().filter(|c| c.path_map.is_some() && c.start < hi && c.end > lo) {
                let (a, b) = (x_at(c, lo.max(c.start)), x_at(c, hi.min(c.end)));
                rects.push(rect_json_mapped(f, Rect { left: a.min(b), right: a.max(b).max(a.min(b) + 1.5), top: c.top, bottom: c.bottom }, c.path_map));
            }
            let mut spans: Vec<Rect> = line
                .clusters
                .iter()
                .filter(|c| c.path_map.is_none() && c.start < hi && c.end > lo)
                .map(|c| {
                    let (a, b) = (x_at(c, lo.max(c.start)), x_at(c, hi.min(c.end)));
                    Rect {
                        left: a.min(b),
                        right: a.max(b).max(a.min(b) + 1.5),
                        top: c.top,
                        bottom: c.bottom,
                    }
                })
                .collect();
            spans.sort_by(|a, b| a.left.total_cmp(&b.left));
            let mut merged: Vec<Rect> = Vec::new();
            for r in spans {
                if let Some(last) = merged.last_mut()
                    && r.left <= last.right + 0.01
                {
                    *last = last.union(&r);
                } else {
                    merged.push(r);
                }
            }
            rects.extend(merged.into_iter().map(|r| rect_json(f, r)));
        }
    }
    Ok(
        json!({"type":cmd,"story":story,"at":at,"start":start,"rects":rects,
        "caret":caret(&fs,story,at,req["affinity"]=="upstream",&chars),
        // vertical text: the arrow keys turn with it (up/down move along the line, left/right to the next/previous line)
        "vertical":doc.stories.get(story).is_some_and(|s|s.vertical)}),
    )
}

pub fn command(req: &Value, session: &crate::Session) -> Result<Value, String> {
    if req["cmd"] == "styleInfo" {
        let shared=session.edit.as_ref().ok_or("style editing requires an editable document")?;
        let state=shared.lock().map_err(|_|"edit state unavailable")?;
        let kind = wire::required_text(req, "kind", "kind required")?;
        let style = wire::required_text(req, "style", "style required")?;
        return state.editor.style_info(kind, style)
            .map(|style|json!({"type":"styleInfo","style":style})).map_err(|e|e.to_string());
    }
    // Style Options "Shortcut" field and the key handler: what a key combination means as a style shortcut (valid or not, its label, the styles that already use it)
    if req["cmd"] == "styleShortcut" {
        let shared=session.edit.as_ref().ok_or("style editing requires an editable document")?;
        let state=shared.lock().map_err(|_|"edit state unavailable")?;
        return state.editor.style_shortcut_info(req["shortcut"].as_str().unwrap_or("")).map(|info|json!({"type":"styleShortcut","info":info})).map_err(|e|e.to_string());
    }
    if req["cmd"] == "textFind" {
        let q=serde_json::from_value(req["query"].clone()).map_err(|e|e.to_string())?;
        let shared=session.edit.as_ref().ok_or("text editing requires an editable document")?;
        let mut state=shared.lock().map_err(|_|"edit state unavailable")?;
        return state.editor.find_text(&q).map(|matches|json!({"type":"textFind","matches":matches})).map_err(|e|e.to_string());
    }
    if req["cmd"] == "textMarks" {
        let page = wire::required_index(req, "page", "textMarks needs page")?;
        let text = |story: &str| -> Option<Vec<char>> {
            let state = session.edit.as_ref()?.lock().ok()?;
            state.editor.parts().iter().filter(|(p, _)| p.starts_with("Stories/"))
                .find_map(|(_, s)| folio_edit::text::contents(s, story).ok()).map(|t| t.chars().collect())
        };
        return Ok(marks(&session.doc, &session.layout, page, req["only"] == "notes", &text));
    }
    if req["cmd"] == "textLayout" {
        let out: Vec<_>=frames(&session.doc,&session.layout).iter().filter(|f|req["story"].as_str().is_none_or(|s|s==f.story))
            .map(|f|json!({"frame":f.id,"story":f.story,"page":f.page,"lines":f.lines.iter().map(|l|json!({
                "start":l.start,"end":l.end,"baseline":f.transform.apply(Point::new(l.left,l.baseline)).y,
                "left":f.transform.apply(Point::new(l.left,l.baseline)).x,
                "right":f.transform.apply(Point::new(l.right,l.baseline)).x,
                "cell":l.cell.as_ref().map(|c|c.id.as_str())
            })).collect::<Vec<_>>()})).collect();
        return Ok(json!({"type":"textLayout","frames":out}));
    }
    if req["cmd"] == "textHit" {
        return geometry(req, &session.doc, &session.layout, None);
    }
    if req["cmd"] == "tableHit" || req["cmd"] == "tableCells" || req["cmd"] == "tableLines" {
        return table_command(req, &session.doc, &session.layout);
    }
    if req["cmd"] == "tableInfo" {
        let shared = session.edit.as_ref().ok_or("table editing requires an editable document")?;
        let mut state = shared.lock().map_err(|_| "edit state unavailable")?;
        let id = wire::required_text(req, "table", "table required")?;
        let mut info = state.editor.table(id).map_err(|e| e.to_string())?;
        let model = session.doc.stories.values().find_map(|story| table_of(&session.doc, &story.id, id));
        if let (Some(table), Some(cells)) = (model, info["cells"].as_array_mut()) {
            for cell in cells {
                if let Some(effective) = table.cells.iter().find(|c| cell["id"].as_str() == Some(c.id.as_str())) {
                    cell["rotation"] = json!(effective.rotation);
                    cell["writingDirection"] = json!(!effective.perpendicular);
                }
            }
        }
        return Ok(json!({"type": "tableInfo", "table": info}));
    }
    let story = wire::required_text(req, "story", "story required")?;
    let shared = session
        .edit
        .as_ref()
        .ok_or("text editing requires an editable document")?;
    let mut state = shared.lock().map_err(|_| "edit state unavailable")?;
    if req["cmd"] == "textCursor" {
        state.editor.reset_insertion_style(story);
        return Ok(json!({"type":"textCursor"}));
    }
    if req["cmd"] == "textFormat" || req["cmd"] == "textCopy" {
        let start = wire::required_index(req, "start", "start required")?;
        let end = wire::required_index(req, "end", "end required")?;
        if req["cmd"] == "textCopy" {
            return state.editor.copy_text(story,start,end).map(|f|json!({"type":"textCopy","fragment":f})).map_err(|e|e.to_string());
        }
        let format=state.editor.text_format(story,start,end).map_err(|e|e.to_string())?;
        let mut fonts=std::collections::BTreeMap::<&str,std::collections::BTreeSet<&str>>::new();
        for f in session.fonts.faces() { fonts.entry(&f.family).or_default().insert(&f.style); }
        let features = match (format["appliedFont"].as_str(), format["fontStyle"].as_str()) {
            (Some(family), Some(style)) => folio_text::opentype::supported(&session.fonts, family, style, &session.doc.font_table),
            _ => Vec::new(),
        };
        return Ok(json!({"type":"textFormat","format":format,"fonts":fonts,"features":features}));
    }
    let text = state
        .editor
        .parts()
        .iter()
        .filter(|(p, _)| p.starts_with("Stories/"))
        .find_map(|(_, s)| folio_edit::text::contents(s, story).ok())
        .ok_or("story absent or contains unsupported text elements")?;
    if req["cmd"] == "textStory" {
        let mut context = state.editor.parts().values().find_map(|s| folio_edit::text::note_context(s, story, req["at"].as_u64().unwrap_or(0) as usize).ok());
        if let Some(context) = context.as_mut() {
            let target = context["endnoteRange"].as_str().and_then(|range|state.editor.parts().values().find_map(|s|folio_edit::text::endnote_target(s,range).ok()));
            context["endnoteTarget"] = target.unwrap_or(serde_json::Value::Null);
        }
        return Ok(
            json!({"type":"textStory","story":story,"text":text,"length":text.chars().count(),"rev":state.editor.rev(),
            "overset":session.layout.overset.iter().any(|id|id==story),"cell":!session.doc.stories.contains_key(story),"noteContext":context}),
        );
    }
    geometry(req, &session.doc, &session.layout, Some(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_model::layout::FrameText;

    #[test]
    fn normal_click_can_enter_footnote_but_story_restricted_drag_stays_in_body() {
        let (doc, mut layout) = sample();
        layout.frames.get_mut("f").unwrap().lines.push(LaidLine {
            cell: Some(folio_model::layout::CellText {
                id: "s/footnote/0".into(), start: 0, end: 1,
                clusters: vec![TextCluster {start:0,end:1,x0:0.0,x1:10.0,top:400.0,bottom:412.0,path_map:None}],
            }),
            ..Default::default()
        });
        let mut req = json!({"cmd":"textHit","page":0,"x":35.0,"y":445.0});
        let hit = geometry(&req,&doc,&layout,None).unwrap();
        assert_eq!(hit["hit"]["story"],"s/footnote/0");
        req["story"] = json!("s");
        assert_eq!(geometry(&req,&doc,&layout,None).unwrap()["hit"]["story"],"s");
    }
    #[test]
    fn baseline_segments_follow_the_word_along_its_line() {
        let (doc, layout) = sample();
        // characters 1..3 are the ligature cluster, x 10..20 in the frame at (30, 40)
        let segs = baseline_segments(&doc, &layout, "s", 1, 3);
        assert_eq!(segs.len(), 1);
        assert_eq!((segs[0]["from"].clone(), segs[0]["to"].clone(), segs[0]["page"].clone()), (json!([40.0, 40.0]), json!([50.0, 40.0]), json!(0)));
        assert!(baseline_segments(&doc, &layout, "other", 0, 3).is_empty());
    }
    fn sample() -> (Document, DocumentLayout) {
        let mut doc = Document::default();
        let item = PageItem {
            id: "f".into(),
            kind: ItemKind::TextFrame(folio_model::TextFrame {
                story: "s".into(),
                ..Default::default()
            }),
            transform: Affine::translate(30.0, 40.0),
            visible: true,
            ..Default::default()
        };
        doc.spreads.push(folio_model::Spread {
            pages: vec![folio_model::Page {
                index: 0,
                bounds: Rect {
                    right: 600.0,
                    bottom: 800.0,
                    ..Default::default()
                },
                ..Default::default()
            }],
            items: vec![item],
            ..Default::default()
        });
        let clusters = vec![
            TextCluster {
                start: 0,
                end: 1,
                x0: 0.0,
                x1: 10.0,
                top: 0.0,
                bottom: 12.0, path_map: None,
            },
            TextCluster {
                start: 1,
                end: 3,
                x0: 10.0,
                x1: 20.0,
                top: 0.0,
                bottom: 12.0, path_map: None,
            }, // ligature, two scalars
            TextCluster {
                start: 3,
                end: 4,
                x0: 20.0,
                x1: 20.0,
                top: 0.0,
                bottom: 12.0, path_map: None,
            }, // CR
        ];
        let mut layout = DocumentLayout::default();
        layout.frames.insert(
            "f".into(),
            FrameText {
                frame: "f".into(),
                lines: vec![
                    LaidLine {
                        start: 0,
                        end: 4,
                        text_clusters: clusters,
                        ..Default::default()
                    },
                    LaidLine {
                        start: 4,
                        end: 4,
                        text_clusters: vec![TextCluster {
                            start: 4,
                            end: 4,
                            x0: 0.0,
                            x1: 0.0,
                            top: 16.0,
                            bottom: 28.0, path_map: None,
                        }],
                        ..Default::default()
                    },
                ],
            },
        );
        (doc, layout)
    }
    #[test]
    fn range_boxes_follow_the_characters_of_a_comment() {
        let (doc, layout) = sample();
        // the first cluster covers x 30..40, the ligature 40..50 (frame at (30, 40)); one line, a box per line
        let one = range_boxes(&doc, &layout, 0, "s", 0, 1);
        assert_eq!(one.len(), 1);
        assert!((one[0][0][0] - 30.0).abs() < 0.01 && (one[0][1][0] - 40.0).abs() < 0.01, "{one:?}");
        assert!(one[0][2][1] <= one[0][0][1] && one[0][3][1] >= one[0][0][1], "up above the baseline, down below it: {one:?}");
        let both = range_boxes(&doc, &layout, 0, "s", 0, 3);
        assert!((both[0][1][0] - 50.0).abs() < 0.01, "{both:?}");
        assert!(range_boxes(&doc, &layout, 1, "s", 0, 3).is_empty() && range_boxes(&doc, &layout, 0, "other", 0, 3).is_empty());
    }
    #[test]
    fn text_in_rect_reads_the_characters_a_box_covers() {
        let (doc, layout) = sample();
        // the frame is at (30, 40): the first cluster covers x 30..40, the ligature 40..50, both y 40..52
        let covered = |left: f64, right: f64| text_in_rect(&doc, &layout, 0, &[Rect { left, top: 41.0, right, bottom: 51.0 }]);
        assert_eq!(covered(29.0, 41.0), Some(("s".into(), 0, 1)));
        assert_eq!(covered(29.0, 46.0), Some(("s".into(), 0, 3)));
        assert_eq!(covered(41.0, 49.0), Some(("s".into(), 1, 3)));
        assert_eq!(covered(31.0, 39.0), Some(("s".into(), 0, 1)));
        assert_eq!(covered(100.0, 120.0), None);
        assert_eq!(text_in_rect(&doc, &layout, 1, &[Rect { left: 0.0, top: 0.0, right: 600.0, bottom: 800.0 }]), None);
        assert_eq!(story_page(&doc, &layout, "s", 2), Some(0));
        assert_eq!(story_page(&doc, &layout, "other", 2), None);
    }
    #[test]
    fn hit_uses_source_clusters_and_page_transform() {
        let (doc, layout) = sample();
        let v = geometry(
            &json!({"cmd":"textHit","page":0,"frame":"f","x":45.0,"y":46.0}),
            &doc,
            &layout,
            None,
        )
        .unwrap();
        assert_eq!(v["hit"]["at"], 2);
        assert_eq!(v["hit"]["story"], "s");
        let v = geometry(
            &json!({"cmd":"textHit","page":0,"frame":"f","x":550.0,"y":46.0}),
            &doc,
            &layout,
            None,
        )
        .unwrap();
        assert_eq!(
            v["hit"]["at"], 3,
            "blank area to the right stays on the clicked line"
        );
    }
    #[test]
    fn horizontal_path_uses_the_same_text_geometry() {
        let (mut doc, layout) = sample();
        let item = &mut doc.spreads[0].items[0];
        let frame = item.text_frame().unwrap().clone();
        item.kind = ItemKind::GraphicLine;
        item.text_path = Some(folio_model::textpath::HorizontalTextPath {
            frame, start: 0.0, end: 200.0, baseline: 12.0,
            path_alignment: folio_model::textpath::PathAlignment::Center,
            curve: None, flipped: false, effect: folio_model::textpath::PathEffect::Rainbow,
            text_alignment: folio_model::textpath::PathTextAlignment::Baseline, spacing: 0.0,
        });
        let v = geometry(&json!({"cmd":"textHit","page":0,"frame":"f","x":45.0,"y":46.0}), &doc, &layout, None).unwrap();
        assert_eq!(v["hit"]["story"], "s");
        assert_eq!(v["hit"]["at"], 2);
        let v = geometry(&json!({"cmd":"textCaret","story":"s","at":1}), &doc, &layout, Some("Afi\r")).unwrap();
        assert_eq!(v["caret"]["frame"], "f");
    }
    #[test]
    fn caret_empty_line_and_selection_return_pages() {
        let (doc, layout) = sample();
        let v = geometry(
            &json!({"cmd":"textSelection","story":"s","start":0,"end":4}),
            &doc,
            &layout,
            Some("Afi\r"),
        )
        .unwrap();
        assert_eq!(v["caret"]["rect"], json!([30.0, 56.0, 30.0, 68.0]));
        assert_eq!(v["rects"][0]["rect"], json!([30.0, 40.0, 51.5, 52.0]));
        assert_eq!(v["caret"]["lineStart"], 4);
        assert!(
            geometry(
                &json!({"cmd":"textCaret","story":"s","at":5}),
                &doc,
                &layout,
                Some("Afi\r")
            )
            .is_err()
        );
    }
    #[test]
    fn overset_has_no_fabricated_caret_and_locked_frame_no_hit() {
        let (mut doc, layout) = sample();
        let v = geometry(
            &json!({"cmd":"textCaret","story":"s","at":7}),
            &doc,
            &layout,
            Some("Afi\rXYZ"),
        )
        .unwrap();
        assert!(v["caret"].is_null());
        doc.spreads[0].items[0].locked = true;
        let v = geometry(
            &json!({"cmd":"textHit","page":0,"x":45,"y":46}),
            &doc,
            &layout,
            None,
        )
        .unwrap();
        assert!(v["hit"].is_null());
    }
    #[test]
    fn vertical_navigation_keeps_column_and_handles_empty_lines() {
        let (doc,layout)=sample();
        let down=geometry(&json!({"cmd":"textNavigate","story":"s","at":3,"direction":"down"}),&doc,&layout,Some("Afi\r")).unwrap();
        assert_eq!(down["at"],4);
        assert_eq!(down["x"],20.0);
        let up=geometry(&json!({"cmd":"textNavigate","story":"s","at":4,"direction":"up","x":20}),&doc,&layout,Some("Afi\r")).unwrap();
        assert_eq!(up["at"],3);
        let home=geometry(&json!({"cmd":"textNavigate","story":"s","at":0,"direction":"up"}),&doc,&layout,Some("Afi\r")).unwrap();
        assert_eq!(home["at"],0);
        assert!(geometry(&json!({"cmd":"textNavigate","story":"s","at":9,"direction":"down"}),&doc,&layout,Some("Afi\r")).is_err());
    }
    #[test]
    fn vertical_navigation_crosses_threaded_frames_in_source_order() {
        let (mut doc,mut layout)=sample();
        let mut second=doc.spreads[0].items[0].clone();
        second.id="next".into();second.transform=Affine::translate(200.0,300.0);
        doc.spreads[0].items.push(second);
        let mut line=layout.frames["f"].lines[0].clone();
        line.start=4;line.end=7;
        line.text_clusters=vec![
            TextCluster{start:4,end:5,x0:0.0,x1:10.0,top:0.0,bottom:12.0,path_map:None},
            TextCluster{start:5,end:6,x0:10.0,x1:20.0,top:0.0,bottom:12.0,path_map:None},
            TextCluster{start:6,end:7,x0:20.0,x1:30.0,top:0.0,bottom:12.0,path_map:None},
        ];
        layout.frames.get_mut("f").unwrap().lines.truncate(1);
        layout.frames.insert("next".into(),FrameText{lines:vec![line],..Default::default()});
        let down=geometry(&json!({"cmd":"textNavigate","story":"s","at":1,"direction":"down"}),&doc,&layout,Some("Afi\rXYZ")).unwrap();
        assert_eq!(down["at"],5);assert_eq!(down["x"],10.0);
        let up=geometry(&json!({"cmd":"textNavigate","story":"s","at":5,"direction":"up","x":10}),&doc,&layout,Some("Afi\rXYZ")).unwrap();
        assert_eq!(up["at"],1);
    }
    #[test]
    fn real_text_fixture_keeps_unicode_specials_and_frame_thread() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../oracle/fixtures/text.idml");
        let doc = folio_idml::read_package(&path, None).unwrap();
        let fonts = folio_text::FontStore::discover(&[]);
        let layout = folio_text::layout_document(&doc, &fonts);
        let fs = frames(&doc, &layout);
        let korean = doc
            .stories
            .values()
            .find(|s| {
                s.paragraphs
                    .iter()
                    .flat_map(|p| &p.runs)
                    .any(|r| r.text.contains("한글"))
            })
            .unwrap();
        let empty_text: Vec<char> = "한글 조판입니다.\r둘째 문단입니다.".chars().collect();
        for at in [0, 1, 2, 10, 19] {
            let c = caret(&fs, &korean.id, at, false, &empty_text);
            assert!(!c.is_null(), "Korean scalar {at}");
        }
        let threaded: Vec<_> = fs.iter().filter(|f| f.story == "u11d").collect();
        assert_eq!(threaded.len(), 2);
        let end = threaded
            .iter()
            .flat_map(|f| f.lines.iter().flat_map(|l| &l.text_clusters))
            .map(|c| c.end)
            .max()
            .unwrap();
        assert_eq!(end, 99);
        let special = fs.iter().find(|f| f.story == "u15f").unwrap();
        let all: Vec<_> = special
            .lines
            .iter()
            .flat_map(|l| &l.text_clusters)
            .collect();
        assert!(
            all.iter().any(|c| c.start == 1 && c.end == 2),
            "emoji is one scalar"
        );
        assert!(
            all.iter().any(|c| c.start == 9 && c.end == 10),
            "table is one scalar"
        );
        assert!(
            all.iter().all(|c| c.end <= 13),
            "cell text must not leak into story offsets"
        );
    }
}
