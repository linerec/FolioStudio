//! Type ▸ Create Outlines: a glyph as the paths InDesign builds from it.
//!
//! Measured on InDesign 2025 (next/oracle/rules/outlines.toml, measurement in
//! `artifacts/outlines-c1-20261023`): for TrueType (`glyf`) faces the polygon follows the font's own contour
//! points, not the shaper's path:
//!
//! - every contour is walked **backwards** (last font point first), starting at the first on-curve point of that
//!   backwards sequence; two off-curve points in a row imply an on-curve point at their middle, as in the font;
//! - a quadratic segment becomes a cubic one whose handles are `anchor + 2/3 (control - anchor)`, truncated
//!   toward zero to 1/64 of a font unit (FreeType's 26.6 fixed point at 1 unit = 1 pixel);
//! - when the contour closes with a curve into an **implied** start point the handle is merged into the first
//!   anchor; when it closes with a curve into an **explicit** start point the end anchor stays as a duplicate of
//!   the start; a closing line adds nothing.
//!
//! Hinted fonts (Calibri, Georgia) move some points by a few font units under InDesign (the TrueType program
//! runs at 1 unit = 1 pixel); this code does not run the program. Faces without `glyf` (OpenType CFF), variable faces and
//! composite glyphs that join components by matching points take the parser's outline instead (`parsed_paths`: an approximation).
use ttf_parser::{Face, Tag};

/// (left handle, anchor, right handle) in font units, y up.
pub type Anchor = [[f64; 2]; 3];
/// One point of a TrueType contour: x, y, on the curve.
type Pt = (f64, f64, bool);

fn q64(v: f64) -> f64 {
    (v * 64.0).trunc() / 64.0
}

/// The polygon path InDesign makes of one contour (points in font order).
pub fn contour_to_path(font_order: &[Pt]) -> Vec<Anchor> {
    let n = font_order.len();
    if n < 3 {
        return Vec::new();
    }
    let r: Vec<Pt> = font_order.iter().rev().copied().collect();
    // (x, y, on curve, implied) around the closed contour
    let mut ex: Vec<(f64, f64, bool, bool)> = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (a, b) = (r[i], r[(i + 1) % n]);
        ex.push((a.0, a.1, a.2, false));
        if !a.2 && !b.2 {
            ex.push(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0, true, true));
        }
    }
    let Some(start) = ex.iter().position(|p| p.2) else { return Vec::new() };
    ex.rotate_left(start);
    let start_implied = ex[0].3;
    let closing_curve = !ex[ex.len() - 1].2;
    let first = ex[0];
    ex.push(first);
    let at = |p: (f64, f64, bool, bool)| [p.0, p.1];
    let mut out: Vec<Anchor> = vec![[at(ex[0]); 3]];
    let mut j = 1;
    while j < ex.len() {
        if ex[j].2 {
            out.push([at(ex[j]); 3]);
            j += 1;
        } else {
            let (c, e) = (ex[j], ex[j + 1]);
            let a = out[out.len() - 1][1];
            let last = out.len() - 1;
            out[last][2] = [q64(a[0] + (c.0 - a[0]) * 2.0 / 3.0), q64(a[1] + (c.1 - a[1]) * 2.0 / 3.0)];
            out.push([[q64(e.0 + (c.0 - e.0) * 2.0 / 3.0), q64(e.1 + (c.1 - e.1) * 2.0 / 3.0)], at(e), at(e)]);
            j += 2;
        }
    }
    if closing_curve && start_implied {
        let end = out.pop().map(|a| a[0]);
        if let Some(l) = end {
            out[0][0] = l;
        }
    } else if !closing_curve {
        out.pop();
    }
    out
}

fn u16be(d: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(p)?, *d.get(p + 1)?]))
}
fn i16be(d: &[u8], p: usize) -> Option<i16> {
    u16be(d, p).map(|v| v as i16)
}
fn u32be(d: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*d.get(p)?, *d.get(p + 1)?, *d.get(p + 2)?, *d.get(p + 3)?]))
}

/// The contours of a `glyf` glyph (points in font units, font order).
fn glyph_points(face: &Face<'_>, gid: u16, depth: u8) -> Option<Vec<Vec<Pt>>> {
    if depth > 8 {
        return None;
    }
    let raw = face.raw_face();
    let glyf = raw.table(Tag::from_bytes(b"glyf"))?;
    let loca = raw.table(Tag::from_bytes(b"loca"))?;
    let long = i16be(raw.table(Tag::from_bytes(b"head"))?, 50)? != 0;
    let at = |g: usize| -> Option<usize> { if long { Some(u32be(loca, 4 * g)? as usize) } else { Some(u16be(loca, 2 * g)? as usize * 2) } };
    let (p0, p1) = (at(gid as usize)?, at(gid as usize + 1)?);
    if p1 <= p0 {
        return Some(Vec::new());
    }
    let d = glyf.get(p0..p1.min(glyf.len()))?;
    let n = i16be(d, 0)?;
    if n >= 0 {
        let n = n as usize;
        let ends: Vec<usize> = (0..n).map(|i| u16be(d, 10 + i * 2).map(usize::from)).collect::<Option<_>>()?;
        let count = ends.last().map_or(0, |e| e + 1);
        let mut p = 10 + n * 2;
        p += 2 + u16be(d, p)? as usize;
        let mut flags = Vec::with_capacity(count);
        while flags.len() < count {
            let f = *d.get(p)?;
            p += 1;
            flags.push(f);
            if f & 8 != 0 {
                let rep = *d.get(p)?;
                p += 1;
                flags.extend(std::iter::repeat_n(f, rep as usize));
            }
        }
        flags.truncate(count);
        let mut axes: [Vec<f64>; 2] = [Vec::with_capacity(count), Vec::with_capacity(count)];
        for (k, (short, same)) in [(2u8, 16u8), (4u8, 32u8)].into_iter().enumerate() {
            let mut v = 0i32;
            for &f in &flags {
                if f & short != 0 {
                    let b = *d.get(p)? as i32;
                    p += 1;
                    v += if f & same != 0 { b } else { -b };
                } else if f & same == 0 {
                    v += i16be(d, p)? as i32;
                    p += 2;
                }
                axes[k].push(v as f64);
            }
        }
        let mut out = Vec::with_capacity(n);
        let mut a = 0;
        for e in ends {
            out.push((a..=e).map(|i| (axes[0][i], axes[1][i], flags[i] & 1 != 0)).collect());
            a = e + 1;
        }
        return Some(out);
    }
    // composite: components placed by offsets (point matching is refused)
    let mut out = Vec::new();
    let mut p = 10;
    loop {
        let flags = u16be(d, p)?;
        let sub = u16be(d, p + 2)?;
        p += 4;
        let (dx, dy) = if flags & 1 != 0 {
            let v = (i16be(d, p)? as f64, i16be(d, p + 2)? as f64);
            p += 4;
            v
        } else {
            let v = (*d.get(p)? as i8 as f64, *d.get(p + 1)? as i8 as f64);
            p += 2;
            v
        };
        if flags & 2 == 0 {
            return None;
        }
        let f2 = |p: usize| i16be(d, p).map(|v| v as f64 / 16384.0);
        let (xs, s01, s10, ys) = if flags & 8 != 0 {
            let s = f2(p)?;
            p += 2;
            (s, 0.0, 0.0, s)
        } else if flags & 0x40 != 0 {
            let v = (f2(p)?, 0.0, 0.0, f2(p + 2)?);
            p += 4;
            v
        } else if flags & 0x80 != 0 {
            let v = (f2(p)?, f2(p + 2)?, f2(p + 4)?, f2(p + 6)?);
            p += 8;
            v
        } else {
            (1.0, 0.0, 0.0, 1.0)
        };
        for c in glyph_points(face, sub, depth + 1)? {
            out.push(c.into_iter().map(|(x, y, on)| (xs * x + s10 * y + dx, s01 * x + ys * y + dy, on)).collect());
        }
        if flags & 0x20 == 0 {
            break;
        }
    }
    Some(out)
}

/// The polygon paths (one per contour, in font order) of `glyph`, in font units, y up.
///
/// A static TrueType (`glyf`) face is read by the measured rule above. Every other face (OpenType CFF / CFF2, a variable face, a composite that
/// joins components by matching points) gets the outline as the font parser reads it, at the coordinates the layout set (the default instance
/// when none are set): cubic segments as they are, quadratic ones raised to cubic. That is an approximation (InDesign's start point and
/// direction for such faces were not measured: `outlines.toml` open questions); the shape is the same.
pub fn glyph_paths(face: &Face<'_>, glyph: u16) -> Option<Vec<Vec<Anchor>>> {
    let raw = face.raw_face();
    if raw.table(Tag::from_bytes(b"fvar")).is_none()
        && let Some(contours) = glyph_points(face, glyph, 0)
    {
        return Some(contours.iter().map(|c| contour_to_path(c)).filter(|p| !p.is_empty()).collect());
    }
    parsed_paths(face, glyph)
}

/// Collects a parsed outline as anchors with handles.
#[derive(Default)]
struct CubicPaths {
    done: Vec<Vec<Anchor>>,
    open: Vec<Anchor>,
}

impl CubicPaths {
    fn finish(&mut self) {
        let mut c = std::mem::take(&mut self.open);
        // a contour that comes back to its start with a curve shares that anchor: the last anchor's incoming handle moves to the first
        if c.len() > 1 && c[c.len() - 1][1] == c[0][1] {
            let last = c.pop().map(|a| a[0]);
            if let Some(l) = last {
                c[0][0] = l;
            }
        }
        if c.len() >= 2 {
            self.done.push(c);
        }
    }
    fn tip(&self) -> [f64; 2] {
        self.open.last().map_or([0.0, 0.0], |a| a[1])
    }
}

impl ttf_parser::OutlineBuilder for CubicPaths {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        let p = [f64::from(x), f64::from(y)];
        self.open.push([p; 3]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = [f64::from(x), f64::from(y)];
        self.open.push([p; 3]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (a, q, e) = (self.tip(), [f64::from(x1), f64::from(y1)], [f64::from(x), f64::from(y)]);
        let c1 = [a[0] + (q[0] - a[0]) * 2.0 / 3.0, a[1] + (q[1] - a[1]) * 2.0 / 3.0];
        let c2 = [e[0] + (q[0] - e[0]) * 2.0 / 3.0, e[1] + (q[1] - e[1]) * 2.0 / 3.0];
        if let Some(l) = self.open.last_mut() {
            l[2] = c1;
        }
        self.open.push([c2, e, e]);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let e = [f64::from(x), f64::from(y)];
        if let Some(l) = self.open.last_mut() {
            l[2] = [f64::from(x1), f64::from(y1)];
        }
        self.open.push([[f64::from(x2), f64::from(y2)], e, e]);
    }
    fn close(&mut self) {
        self.finish();
    }
}

/// The outline of `glyph` as the font parser reads it (CFF, CFF2, glyf with variations), as cubic anchors in font units, y up.
fn parsed_paths(face: &Face<'_>, glyph: u16) -> Option<Vec<Vec<Anchor>>> {
    let mut b = CubicPaths::default();
    // a glyph with no outline (a space) is an empty list; a face without outlines at all (a bitmap font) has none to give
    match face.outline_glyph(ttf_parser::GlyphId(glyph), &mut b) {
        Some(_) => {
            b.finish();
            Some(b.done)
        }
        None if face.tables().glyf.is_some() || face.tables().cff.is_some() || face.tables().cff2.is_some() => Some(Vec::new()),
        None => None,
    }
}


// ---------------------------------------------------------------------------------------------
// Whole frames: what the editor's `createOutlines` command is handed (see `folio-edit` ops/outlines.rs).

use crate::fonts::FontStore;
use folio_model::layout::DocumentLayout;
use folio_model::{Document, ItemKind, PageItem, Paint};

/// One polygon: a fill (and stroke) and its subpaths, each point `[left x, y, anchor x, y, right x, y]`
/// in the frame's inner coordinates.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Poly {
    pub fill: String,
    pub tint: f64,
    pub stroke: String,
    pub weight: f64,
    pub paths: Vec<Vec<[f64; 6]>>,
}

/// The polygons InDesign makes of one text frame: one per line and per run of one fill.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FrameShapes {
    pub frame: String,
    pub polys: Vec<Poly>,
}

fn find<'a>(items: &'a [PageItem], id: &str) -> Option<&'a PageItem> {
    items.iter().find_map(|it| {
        if it.id == id {
            return Some(it);
        }
        match &it.kind {
            ItemKind::Group { children } => find(children, id),
            _ => None,
        }
    })
}

fn swatch(p: &Paint) -> Result<(String, f64), String> {
    match p {
        Paint::None => Ok((String::new(), 100.0)),
        Paint::Swatch { id, tint } => Ok((id.clone(), *tint)),
        Paint::Gradient { .. } => Err("text with a gradient is not supported by Create Outlines".into()),
    }
}

/// The frames Create Outlines turns into polygons when `ids` are selected: a threaded frame takes the whole story with it
/// (InDesign converts every frame of the thread; outlines-c2 `frame-thread`).
pub fn frames_to_convert(doc: &Document, ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let story = doc.spreads.iter().find_map(|sp| find(&sp.items, id)).and_then(|it| match &it.kind {
            ItemKind::TextFrame(f) => Some(f.story.clone()),
            _ => None,
        });
        let mut frames = Vec::new();
        if let Some(story) = &story {
            story_frames_all(doc, story, &mut frames);
        }
        let all: Vec<String> = if frames.is_empty() { vec![id.clone()] } else { frames.iter().map(|f| f.id.clone()).collect() };
        for f in all {
            if !out.contains(&f) {
                out.push(f);
            }
        }
    }
    out
}

type OutlinePolygon = ((Paint, Paint, u64), Vec<Vec<[f64; 6]>>);

pub fn frame_shapes(doc: &Document, layout: &DocumentLayout, fonts: &FontStore, id: &str) -> Result<FrameShapes, String> {
    let item = doc.spreads.iter().find_map(|sp| find(&sp.items, id)).ok_or_else(|| format!("{id}: no such text frame"))?;
    let ItemKind::TextFrame(_frame) = &item.kind else { return Err(format!("{id} is not a text frame")) };
    let text = layout.frames.get(id).ok_or_else(|| format!("{id}: the frame has no composed text"))?;
    // one polygon per line and per run of one fill and stroke (a frame's underline, strike-through and rules vanish)
    let mut polys: Vec<OutlinePolygon> = Vec::new();
    let mut order: Vec<usize> = Vec::new();
    for line in &text.lines {
        if line.rotation != 0.0 || !line.objects.is_empty() || line.table.is_some() || line.cell.is_some() {
            return Err("text with anchored objects, tables or turned lines is not supported by Create Outlines".into());
        }
        let mut current: Option<usize> = None;
        for g in &line.glyphs {
            let (upem, paths) = fonts.native_outline(g.font, g.glyph).ok_or_else(|| "this font has no outlines that can be read (a bitmap or colour-only face); it cannot be turned into outlines".to_string())?;
            if paths.is_empty() {
                continue;
            }
            // the same fill and stroke go on in one polygon, a line at a time
            let key = if g.stroke == Paint::None || g.stroke_weight <= 0.0 { (g.fill.clone(), Paint::None, 0) } else { (g.fill.clone(), g.stroke.clone(), g.stroke_weight.to_bits()) };
            let at = match current {
                Some(k) if polys[k].0 == key => k,
                _ => {
                    polys.push((key, Vec::new()));
                    order.push(polys.len() - 1);
                    current = Some(polys.len() - 1);
                    polys.len() - 1
                }
            };
            let shear = -(g.skew.to_radians()).tan();
            let map = |p: [f64; 2]| -> [f64; 2] {
                let ex = p[0] / upem * g.size * g.hscale;
                let ey = -p[1] / upem * g.size * g.vscale;
                [g.x + ex + shear * ey, g.y + ey]
            };
            for path in paths {
                let pts: Vec<[f64; 6]> = path.iter().map(|a| {
                    let (l, c, r) = (map(a[0]), map(a[1]), map(a[2]));
                    [l[0], l[1], c[0], c[1], r[0], r[1]]
                }).collect();
                polys[at].1.push(pts);
            }
        }
    }
    let polys = order.into_iter().map(|k| {
        let (fill, tint) = swatch(&polys[k].0.0)?;
        let (stroke, _) = swatch(&polys[k].0.1)?;
        Ok(Poly { fill, tint, stroke, weight: f64::from_bits(polys[k].0.2), paths: polys[k].1.clone() })
    }).collect::<Result<Vec<_>, String>>()?;
    if polys.is_empty() {
        return Err("the frame has no text to turn into outlines".into());
    }
    Ok(FrameShapes { frame: id.to_owned(), polys })
}

// ---------------------------------------------------------------------------------------------
// A text range (rules: next/oracle/rules/outlines-c2.ko.md): InDesign keeps the frame and the story and puts one
// inline polygon in the story for every line and every run of one paint of the selected characters.

/// One inline polygon: the story characters [start, end) it replaces, its paint and glyph paths in the frame's
/// inner coordinates, the line's baseline and the leading its object character gets.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RunShape {
    pub frame: String,
    pub start: usize,
    pub end: usize,
    pub poly: Poly,
    pub baseline: f64,
    pub leading: f64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RangeShapes {
    pub story: String,
    pub runs: Vec<RunShape>,
}

fn story_frames<'a>(items: &'a [PageItem], story: &str, out: &mut Vec<&'a PageItem>) {
    for it in items {
        match &it.kind {
            ItemKind::TextFrame(f) if f.story == story => out.push(it),
            ItemKind::Group { children } => story_frames(children, story, out),
            _ => {}
        }
    }
}

fn leading_of(a: &folio_model::CharacterAttrs) -> f64 {
    match a.leading {
        folio_model::Leading::Fixed(v) => v,
        folio_model::Leading::Auto => a.auto_leading / 100.0 * a.point_size,
    }
}

/// The glyph's contours, scaled and sheared into the frame's inner coordinates.
fn place_paths(g: &folio_model::layout::PlacedGlyph, upem: f64, paths: &[Vec<Anchor>]) -> Vec<Vec<[f64; 6]>> {
    let shear = -(g.skew.to_radians()).tan();
    let map = |p: [f64; 2]| -> [f64; 2] {
        let ex = p[0] / upem * g.size * g.hscale;
        let ey = -p[1] / upem * g.size * g.vscale;
        [g.x + ex + shear * ey, g.y + ey]
    };
    paths.iter().map(|path| path.iter().map(|a| {
        let (l, c, r) = (map(a[0]), map(a[1]), map(a[2]));
        [l[0], l[1], c[0], c[1], r[0], r[1]]
    }).collect()).collect()
}

fn story_frames_all<'a>(doc: &'a Document, story: &str, out: &mut Vec<&'a PageItem>) {
    for sp in &doc.spreads {
        story_frames(&sp.items, story, out);
    }
    // thread order is by the chain's first frame; the overset check only needs the largest line end
    out.sort_by_key(|it| match &it.kind {
        ItemKind::TextFrame(f) => (f.previous.is_some(), it.id.clone()),
        _ => (true, it.id.clone()),
    });
}

/// The inline polygons InDesign makes of the characters [start, end) of a story (or of the story of a text frame):
/// per line, one per run of one paint; the whitespace at the ends of a run stays text.
pub fn range_shapes(doc: &Document, layout: &DocumentLayout, fonts: &FontStore, id: &str, start: usize, end: usize) -> Result<RangeShapes, String> {
    let story_id = if doc.stories.contains_key(id) {
        id.to_owned()
    } else {
        let item = doc.spreads.iter().find_map(|sp| find(&sp.items, id)).ok_or_else(|| format!("{id}: no such story or text frame"))?;
        let ItemKind::TextFrame(f) = &item.kind else { return Err(format!("{id} is not a text frame")) };
        f.story.clone()
    };
    let story = doc.stories.get(&story_id).ok_or_else(|| format!("{story_id}: story not found"))?;
    if start >= end {
        return Err("select the text to turn into outlines".into());
    }
    // the story's characters with their attributes, as InDesign counts them
    let mut chars: Vec<(char, &folio_model::CharacterAttrs, bool)> = Vec::new();
    for p in &story.paragraphs {
        for run in &p.runs {
            let special = run.variable.is_some() || run.note.is_some() || !run.tables.is_empty();
            chars.extend(run.text.chars().map(|c| (c, &run.attrs, special || c == '\u{16}')));
        }
        if let Some(last) = p.runs.last() {
            chars.push(('\r', &last.attrs, false));
        }
    }
    if end > chars.len() {
        return Err("the selection is outside the story".into());
    }
    if chars[start..end].iter().any(|c| c.2) {
        return Err("a range with tables, notes or variables is not supported by Create Outlines".into());
    }
    let mut frames = Vec::new();
    story_frames_all(doc, &story_id, &mut frames);
    struct Open {
        key: (Paint, Paint, u64),
        fill: Paint,
        first: usize,
        last: usize,
        paths: Vec<Vec<[f64; 6]>>,
    }
    let mut runs: Vec<RunShape> = Vec::new();
    let mut shown = 0usize;
    for frame in frames {
        let Some(text) = layout.frames.get(&frame.id) else { continue };
        for line in &text.lines {
            shown = shown.max(line.end);
            if line.end <= start || line.start >= end {
                continue;
            }
            // an underline, a strike-through and a paragraph rule stay: the object character keeps its character format
            // and the paragraph its rule (outlines-c3, goldens outlines-c3-under / -strike / -rule)
            if line.rotation != 0.0 || line.table.is_some() || line.cell.is_some() {
                return Err("text with tables or turned lines is not supported by Create Outlines".into());
            }
            if line.text_clusters.is_empty() {
                return Err("this line has no character map; it is not supported by Create Outlines".into());
            }
            // the cluster a glyph belongs to, by its pen position
            let cluster_of = |x: f64| {
                line.text_clusters.iter().find(|c| x >= c.x0.min(c.x1) - 0.001 && x < c.x0.max(c.x1) - 0.001)
                    .or_else(|| line.text_clusters.iter().find(|c| (x - c.x0).abs() < 0.001))
            };
            let mut open: Option<Open> = None;
            let mut done: Vec<Open> = Vec::new();
            for g in &line.glyphs {
                let Some(c) = cluster_of(g.x) else { continue };
                if c.end <= start || c.start >= end {
                    continue;
                }
                if c.start < start || c.end > end {
                    return Err("the selection splits a ligature; it cannot be turned into outlines".into());
                }
                let (upem, paths) = fonts.native_outline(g.font, g.glyph).ok_or_else(|| "this font has no outlines that can be read (a bitmap or colour-only face); it cannot be turned into outlines".to_string())?;
                if paths.is_empty() {
                    continue;
                }
                // a different stroke starts a new polygon, as a different fill does (outlines-c3 measured)
                let key = if g.stroke == Paint::None || g.stroke_weight <= 0.0 { (g.fill.clone(), Paint::None, 0u64) } else { (g.fill.clone(), g.stroke.clone(), g.stroke_weight.to_bits()) };
                let object_between = open.as_ref().is_some_and(|o| o.last < c.start && chars[o.last..c.start].iter().any(|k| k.0 == '\u{fffc}'));
                if open.as_ref().is_some_and(|o| o.key != key || object_between) {
                    done.extend(open.take());
                }
                let o = open.get_or_insert_with(|| Open { key: key.clone(), fill: g.fill.clone(), first: c.start, last: c.end, paths: Vec::new() });
                o.last = c.end;
                o.paths.extend(place_paths(g, upem, &paths));
            }
            done.extend(open.take());
            for o in done {
                let (fill, tint) = swatch(&o.fill)?;
                let (stroke, _) = swatch(&o.key.1)?;
                let weight = f64::from_bits(o.key.2);
                let leading = chars[o.first..o.last].iter().map(|c| leading_of(c.1)).fold(0.0, f64::max);
                runs.push(RunShape { frame: frame.id.clone(), start: o.first, end: o.last, poly: Poly { fill, tint, stroke, weight, paths: o.paths }, baseline: line.baseline, leading });
            }
        }
    }
    if shown < end && chars[shown..end].iter().any(|c| !c.0.is_whitespace()) {
        return Err("Cannot create outlines from overset text.".into());
    }
    if runs.is_empty() {
        return Err("the selection has no characters that can be turned into outlines".into());
    }
    runs.sort_by_key(|r| r.start);
    Ok(RangeShapes { story: story_id, runs })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Arial 'A' outer contour and the 'a' bowl's closing curve, as InDesign 2025 built them at 48 pt
    // (artifacts/outlines-c1-20261023/wide/arial.json).
    #[test]
    fn line_contour_is_walked_backwards() {
        let a = [(-3., 0., true), (560., 1466., true), (769., 1466., true), (1369., 0., true), (1148., 0., true), (977., 444., true), (364., 444., true), (203., 0., true)];
        let p = contour_to_path(&a);
        let anchors: Vec<[f64; 2]> = p.iter().map(|x| x[1]).collect();
        assert_eq!(anchors.first(), Some(&[203., 0.]));
        assert_eq!(anchors.last(), Some(&[-3., 0.]));
        assert_eq!(p.len(), 8);
    }

    #[test]
    fn closing_curve_into_an_implied_point_merges() {
        // Arial 'O' inner: first point on, last two off -> starts at the middle of the last two, no duplicate
        let o = [(299., 711., true), (299., 446., false), (584., 141., false), (799., 141., true), (1018., 141., false), (1301., 449., false), (1301., 732., true),
            (1301., 911., false), (1180., 1178., false), (947., 1325., false), (802., 1325., true), (596., 1325., false), (299., 1042., false)];
        let p = contour_to_path(&o);
        assert_eq!(p.len(), 9);
        assert_eq!(p[0][1], [447.5, 1183.5]);
        assert_eq!(p[0][2], [546.5, 1277.828125]);
        assert_eq!(p[0][0], [348.5, 1089.15625]);
    }

    #[test]
    fn a_parsed_outline_becomes_cubic_anchors() {
        use ttf_parser::OutlineBuilder;
        let mut b = CubicPaths::default();
        // a square with one rounded corner: line, line, cubic back to the start, then a quadratic triangle
        b.move_to(0.0, 0.0);
        b.line_to(100.0, 0.0);
        b.line_to(100.0, 100.0);
        b.curve_to(60.0, 140.0, 20.0, 100.0, 0.0, 0.0);
        b.close();
        b.move_to(10.0, 10.0);
        b.quad_to(40.0, 40.0, 70.0, 10.0);
        b.line_to(10.0, 10.0);
        b.close();
        b.finish();
        assert_eq!(b.done.len(), 2);
        let sq = &b.done[0];
        // the curve ends on the start: its incoming handle moves to the first anchor and the duplicate goes
        assert_eq!(sq.len(), 3);
        assert_eq!(sq[0][0], [20.0, 100.0]);
        assert_eq!(sq[2][2], [60.0, 140.0]);
        let tri = &b.done[1];
        assert_eq!(tri.len(), 2);
        // quadratic control (40,40) raised to cubic: handles two thirds of the way to the control
        assert!((tri[0][2][0] - 30.0).abs() < 1e-9 && (tri[0][2][1] - 30.0).abs() < 1e-9);
        assert!((tri[1][0][0] - 50.0).abs() < 1e-9 && (tri[1][0][1] - 30.0).abs() < 1e-9);
    }

    /// An OpenType CFF face (no `glyf`): the outline comes from the parser as cubic anchors. Skipped where no such font is installed.
    #[test]
    fn a_cff_face_gives_cubic_outlines() {
        let dirs = [std::env::var_os("LOCALAPPDATA").map(|d| std::path::Path::new(&d).join("Microsoft/Windows/Fonts")), Some("/usr/share/fonts".into()), Some("/Library/Fonts".into())];
        let Some(path) = dirs.into_iter().flatten().filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "otf")) else { return };
        let data = std::fs::read(path).unwrap();
        let Ok(face) = Face::parse(&data, 0) else { return };
        if face.raw_face().table(Tag::from_bytes(b"glyf")).is_some() || face.raw_face().table(Tag::from_bytes(b"CFF ")).is_none() {
            return;
        }
        let gid = face.glyph_index('O').or_else(|| face.glyph_index('가')).expect("a glyph");
        let paths = glyph_paths(&face, gid.0).expect("outline");
        assert!(!paths.is_empty() && paths.iter().all(|p| p.len() >= 3));
        // a curved letter has real handles
        assert!(paths.iter().flatten().any(|a| a[0] != a[1] || a[2] != a[1]));
        // no contour repeats its start point at the end
        assert!(paths.iter().all(|p| p[0][1] != p[p.len() - 1][1]));
        // a space has no contours but is not an error
        if let Some(sp) = face.glyph_index(' ') {
            assert_eq!(glyph_paths(&face, sp.0), Some(Vec::new()));
        }
    }

    #[test]
    fn closing_curve_into_an_explicit_point_keeps_a_duplicate() {
        // Arial 'a' outer ends with (864,0) on, (836,56) off: the walk starts at (864,0) and ends on it again
        let a = [(828., 131., true), (728., 46., false), (543., -24., false), (437., -24., true), (262., -24., false), (74., 147., false), (74., 280., true),
            (1052., 0., true), (864., 0., true), (836., 56., false)];
        let p = contour_to_path(&a);
        assert_eq!(p.first().map(|x| x[1]), Some([864., 0.]));
        assert_eq!(p.last().map(|x| x[1]), Some([864., 0.]));
        assert_eq!(p.last().map(|x| x[0]), Some([845.328125, 37.328125]));
    }
}
