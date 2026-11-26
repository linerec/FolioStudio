//! Vertical text (`StoryPreference/@StoryOrientation="Vertical"`).
//!
//! A vertical frame is composed as a horizontal one turned a quarter counter-clockwise: the "virtual" frame is as wide
//! as the frame is tall, its columns run left to right where the frame's run top to bottom, and its lines stack
//! downwards where the frame's stack from the right edge to the left. The ordinary composer sets that frame; each line
//! is then turned back a quarter clockwise into the frame ([`Map`]). Glyphs that were set as horizontal text turn with
//! it (Roman letters, digits, spaces: InDesign 2025 turns half-width text 90° clockwise); upright glyphs (Hangul, Han,
//! kana, full-width forms) are counter-turned in place, take their font's vertical form (`vert`/`vrt2`) and advance
//! by their vertical metrics (`vmtx`).
//!
//! Measured with InDesign 2025 (artifacts/vtext-c1-20261023, Malgun Gothic 12 pt, Japanese composer, PDF):
//! - the first line's Roman baseline is the frame's right edge minus the ascent (as a horizontal frame's top plus the
//!   ascent), later lines step left by the leading; the frame's columns stack from its top;
//! - an upright glyph advances by its `vmtx` advance (Malgun Gothic Hangul 2176/2048 em, the vertical bracket forms
//!   0.5 em); its vertical origin (`VORG`, else the outline top plus the top side bearing) sits on the pen, and it is
//!   centred across the line on the em box centre, `embox_top - 0.5` em right of the Roman baseline;
//! - a tate-chu-yoko group takes one em of the line; its glyphs stay horizontal on a baseline `embox_top` em below the
//!   group's start, the group centred on the em box centre;
//! - `CharacterRotation` turns the glyph counter-clockwise around its em box centre, and the glyph's box turned takes its
//!   room along the line (45°: Hangul about 17.5 pt at 12 pt, where upright it takes 12.75).
use crate::shape::{ClusterKind, FaceTable, ShapedParagraph};
use folio_model::layout::{DecorationKind, LaidLine, TextShape};
use folio_model::{Affine, Paragraph, Point, Rect};
use rustybuzz::ttf_parser::{GlyphId, Tag};

/// How a shaped cluster sits in vertical text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    /// Set as horizontal text and turned with the line (90° clockwise on the page).
    Turned,
    /// Upright: `hadv` the horizontal advance (ems) it is centred with, `vorigin` the distance of its vertical origin
    /// above its baseline (ems), `vadv` its vertical advance and `adv` its advance along the line (points; more than `vadv`
    /// when a character rotation turns its box).
    Upright { hadv: f64, vorigin: f64, adv: f64, vadv: f64 },
    /// One cluster of a tate-chu-yoko group: `lead` how far the composer put it after the group's start (points),
    /// `offset` where it starts in the horizontal group and `width` the group's width (points).
    Tcy { lead: f64, offset: f64, width: f64, em: f64 },
}

/// Whether a character stands upright in vertical text (Unicode `Vertical_Orientation` U and Tu, with the CJK
/// punctuation that has vertical forms; the rest turns).
pub fn upright(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x2FDF | 0x2FF0..=0x2FFF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3100..=0x312F
        | 0x3130..=0x318F | 0x3190..=0x31FF | 0x3200..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE10..=0xFE1F | 0xFE30..=0xFE4F | 0xFF01..=0xFF60 | 0xFFE0..=0xFFE6
        | 0x1F200..=0x1F2FF | 0x20000..=0x3FFFF)
}

/// Half-width characters a run's "rotate single-byte characters" sets upright.
fn single_byte(c: char) -> bool {
    (c as u32) < 0x0100 && !c.is_whitespace() && !c.is_control()
}

impl FaceTable<'_> {
    /// The vertical form of a glyph (`vrt2`, else `vert`; single substitutions), or the glyph itself.
    pub fn vertical_form(&self, font: folio_model::layout::FontId, glyph: u16) -> u16 {
        let Some(face) = self.ttf_face(font) else { return glyph };
        let Some(gsub) = face.tables().gsub else { return glyph };
        for tag in [Tag::from_bytes(b"vrt2"), Tag::from_bytes(b"vert")] {
            let Some(feature) = gsub.features.find(tag) else { continue };
            for index in feature.lookup_indices {
                let Some(lookup) = gsub.lookups.get(index) else { continue };
                for k in 0..lookup.subtables.len() {
                    use rustybuzz::ttf_parser::gsub::{SingleSubstitution, SubstitutionSubtable};
                    let Some(SubstitutionSubtable::Single(single)) = lookup.subtables.get::<SubstitutionSubtable>(k) else { continue };
                    let Some(at) = single.coverage().get(GlyphId(glyph)) else { continue };
                    let out = match single {
                        SingleSubstitution::Format1 { delta, .. } => (i32::from(glyph) + i32::from(delta)) as u16,
                        SingleSubstitution::Format2 { substitutes, .. } => match substitutes.get(at) { Some(g) => g.0, None => continue },
                    };
                    return out;
                }
            }
            return glyph;
        }
        glyph
    }

    /// Horizontal advance, vertical advance and vertical origin of a glyph, in ems. A face without vertical metrics:
    /// one em, the origin at the em box top.
    pub fn vertical_metrics(&self, font: folio_model::layout::FontId, glyph: u16) -> (f64, f64, f64) {
        let m = self.metrics(font);
        let upem = m.units_per_em.max(1.0);
        let fallback = (1.0, 1.0, m.embox_top);
        let Some(face) = self.ttf_face(font) else { return fallback };
        let g = GlyphId(glyph);
        let hadv = face.glyph_hor_advance(g).map_or(1.0, |a| f64::from(a) / upem);
        let Some(vadv) = face.glyph_ver_advance(g) else { return (hadv, 1.0, m.embox_top) };
        let origin = face.glyph_y_origin(g).map(f64::from).or_else(|| {
            let tsb = face.glyph_ver_side_bearing(g)?;
            let top = face.glyph_bounding_box(g).map_or(0, |b| b.y_max);
            Some(f64::from(top) + f64::from(tsb))
        });
        (hadv, f64::from(vadv) / upem, origin.map_or(m.embox_top, |o| o / upem))
    }
}

/// Sets a vertical paragraph's clusters: upright ones take their vertical form and advance, tate-chu-yoko groups one
/// em. Fills `shaped.vertical` (one entry per cluster).
pub fn adjust(shaped: &mut ShapedParagraph, para: &Paragraph, table: &FaceTable<'_>) {
    let n = shaped.clusters.len();
    let mut kinds = vec![Orientation::Turned; n];
    let auto = auto_groups(shaped, para);
    let mut k = 0;
    while k < n {
        let cl = &shaped.clusters[k];
        let attrs = &para.runs[cl.run].attrs;
        let auto_end = auto.iter().find(|r| r.start == k).map(|r| r.end);
        if auto_end.is_some() || (attrs.vertical.tatechuyoko && matches!(cl.kind, ClusterKind::Glyphs | ClusterKind::Space | ClusterKind::FixedSpace)) {
            // the group: this run's clusters up to the next run or a break (auto tate-chu-yoko: the digits found)
            let run = cl.run;
            let end = auto_end.unwrap_or_else(|| (k..n).find(|&j| shaped.clusters[j].run != run || !matches!(shaped.clusters[j].kind, ClusterKind::Glyphs | ClusterKind::Space | ClusterKind::FixedSpace)).unwrap_or(n));
            let width: f64 = shaped.clusters[k..end].iter().map(|c| c.advance).sum::<f64>()
                + shaped.clusters[k..end.saturating_sub(1).max(k)].iter().map(|c| c.tracking).sum::<f64>();
            let em = shaped.clusters[k].size;
            let mut offset = 0.0;
            for (i, j) in (k..end).enumerate() {
                let c = &mut shaped.clusters[j];
                kinds[j] = Orientation::Tcy { lead: if i == 0 { 0.0 } else { em }, offset, width, em };
                offset += c.advance + if j + 1 < end { c.tracking } else { 0.0 };
                c.advance = if i == 0 { em } else { 0.0 };
                c.tracking = if j + 1 == end { c.tracking } else { 0.0 };
                c.kind = ClusterKind::Glyphs;
            }
            k = end;
            continue;
        }
        let up = cl.kind == ClusterKind::Glyphs && (upright(cl.ch) || (attrs.vertical.upright_single_byte && single_byte(cl.ch)));
        if up {
            let size = cl.size;
            let (font, range) = (cl.font, cl.glyphs.clone());
            let mut adv = 0.0;
            let mut first = None;
            for g in &mut shaped.glyphs[range] {
                g.id = table.vertical_form(g.font, g.id);
                let (hadv, vadv, vorigin) = table.vertical_metrics(g.font, g.id);
                first.get_or_insert((hadv, vorigin));
                adv += vadv * size;
            }
            let (hadv, vorigin) = first.unwrap_or_else(|| { let (h, _, o) = table.vertical_metrics(font, 0); (h, o) });
            let along = turned_extent(attrs.vertical.rotation, adv, hadv * size);
            let c = &mut shaped.clusters[k];
            c.advance = along;
            kinds[k] = Orientation::Upright { hadv, vorigin, adv: along, vadv: adv };
        } else if cl.kind == ClusterKind::Glyphs && attrs.vertical.rotation != 0.0 {
            let c = &mut shaped.clusters[k];
            c.advance = turned_extent(attrs.vertical.rotation, c.advance, c.size);
        }
        k += 1;
    }
    shaped.vertical = kinds;
}

/// Auto tate-chu-yoko (`AutoTcy`, `AutoTcyIncludeRoman` of the paragraph): the cluster ranges of runs of half-width
/// digits (and Roman letters when asked) no longer than the paragraph's digit count, outside explicit tate-chu-yoko.
/// InDesign 2025 (vtext-c2, oracle vtext2/vt2-autotcy, AutoTcy 2): "11" and "21" stand as groups, "2026" and "100" turn, a
/// lone "3" stands upright; with Roman letters and 3: "AB", "CDE", "345" stand as groups.
fn auto_groups(shaped: &ShapedParagraph, para: &Paragraph) -> Vec<std::ops::Range<usize>> {
    let digits = para.attrs.auto_tcy as usize;
    if digits == 0 {
        return Vec::new();
    }
    let roman = para.attrs.auto_tcy_roman;
    let counts = |k: usize| {
        let c = &shaped.clusters[k];
        c.kind == ClusterKind::Glyphs && !para.runs[c.run].attrs.vertical.tatechuyoko
            && (c.ch.is_ascii_digit() || (roman && c.ch.is_ascii_alphabetic()))
    };
    let n = shaped.clusters.len();
    let mut out = Vec::new();
    let mut k = 0;
    while k < n {
        if !counts(k) {
            k += 1;
            continue;
        }
        let end = (k..n).find(|&j| !counts(j)).unwrap_or(n);
        if end - k <= digits {
            out.push(k..end);
        }
        k = end;
    }
    out
}

/// How far a box `along` long and `across` wide reaches along the line when turned by `deg`.
fn turned_extent(deg: f64, along: f64, across: f64) -> f64 {
    if deg == 0.0 { return along; }
    let (s, c) = deg.to_radians().sin_cos();
    c.abs() * along + s.abs() * across
}

/// Where a glyph of a vertical line goes, still in the virtual (horizontal) frame: its origin and the linear part
/// the final quarter turn completes (`None`: turned with the line).
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub linear: Option<Affine>,
}

/// The inverse of the quarter turn's linear part: an upright glyph's virtual linear transform.
const UPRIGHT: Affine = Affine::new(0.0, -1.0, 1.0, 0.0, 0.0, 0.0);

/// Counter-clockwise turn by `deg` on the page (y down) around `c`, then `inner`.
fn turn(deg: f64, c: Point, x: f64, y: f64, inner: Option<Affine>) -> Placement {
    let (s, k) = deg.to_radians().sin_cos();
    let r = Affine::new(k, -s, s, k, 0.0, 0.0);
    let p = r.apply(Point::new(x - c.x, y - c.y));
    Placement { x: c.x + p.x, y: c.y + p.y, linear: Some(r.then_inner(&inner.unwrap_or(Affine::IDENTITY))) }
}

/// A glyph of cluster kind `o`: `pen` the composer's x for the cluster, `baseline` its baseline (rise applied),
/// `dx`/`dy` the glyph's offset from shaping (points, y up), `size` the point size, `embox_top` its font's (ems),
/// `rotation` the run's `CharacterRotation`, `tcy_offset` the run's tate-chu-yoko X/Y offsets.
#[allow(clippy::too_many_arguments)]
pub fn place(o: Orientation, pen: f64, baseline: f64, dx: f64, dy: f64, size: f64, embox_top: f64, rotation: f64, tcy_offset: (f64, f64), advance: f64) -> Placement {
    // the em box centre line across the vertical line, in virtual y
    let centre = baseline - (embox_top - 0.5) * size;
    let p = match o {
        Orientation::Turned => Placement { x: pen + dx, y: baseline - dy, linear: None },
        // (a turned box's room is wider than the glyph: the glyph sits in its middle)
        Orientation::Upright { hadv, vorigin, adv, vadv } => Placement { x: pen + (adv - vadv) / 2.0 + vorigin * size - dy, y: centre + hadv * size / 2.0 - dx, linear: Some(UPRIGHT) },
        Orientation::Tcy { lead, offset, width, em } => {
            let start = pen - lead;
            let (xo, yo) = tcy_offset;
            return Placement { x: start + embox_top * em - dy - yo, y: centre + width / 2.0 - offset - dx - xo, linear: Some(UPRIGHT) };
        }
    };
    if rotation == 0.0 {
        return p;
    }
    // An upright glyph turns around the middle of its room moved down the line by its vertical origin's height over the em
    // box top (vtext-c2, InDesign 2025 PDF of vtext/vt-rot: 90° and 45° glyphs sit (I - R)(0, 0.469) pt from a turn about the
    // room's middle, Malgun Gothic 12 pt: (1878 - 1798) / 2048 em).
    let (along, shift) = match o { Orientation::Upright { adv, vorigin, .. } => (adv, (vorigin - embox_top) * size), _ => (advance, 0.0) };
    turn(rotation, Point::new(pen + along / 2.0 + shift, centre), p.x, p.y, p.linear)
}

/// The quarter turn from the virtual frame into a vertical frame whose text area spans `left..right` across.
pub fn map(left: f64, right: f64) -> Affine {
    Affine::new(0.0, 1.0, -1.0, 0.0, left + right, 0.0)
}

/// The virtual frame's area for a vertical frame's `area`: as wide as the frame is tall.
pub fn virtual_rect(area: Rect) -> Rect {
    Rect { left: area.top, right: area.bottom, top: area.left, bottom: area.right }
}

/// Turns a composed line of the virtual frame into the vertical frame (`m` from [`map`]).
pub fn place_line(line: &mut LaidLine, m: &Affine) {
    let linear = Affine { tx: 0.0, ty: 0.0, ..*m };
    for g in &mut line.glyphs {
        let p = m.apply(Point::new(g.x, g.y));
        g.x = p.x;
        g.y = p.y;
        let l = linear.then_inner(&g.path_transform.unwrap_or(Affine::IDENTITY));
        let identity = (l.a - 1.0).abs() < 1e-12 && l.b.abs() < 1e-12 && l.c.abs() < 1e-12 && (l.d - 1.0).abs() < 1e-12;
        // An upright glyph keeps an explicit identity: every glyph of a vertical line is placed on its own (the PDF writes its exact
        // position, not one chained from the glyph before with InDesign's horizontal Td rounding, which drifts down a column).
        g.path_transform = Some(if identity { Affine::IDENTITY } else { l });
    }
    for c in &mut line.text_clusters {
        c.path_map = Some(match c.path_map { Some(inner) => m.then_inner(&inner), None => *m });
    }
    // the line's own shapes (paragraph rules, shading, cell fills) turn with it
    for s in &mut line.shapes {
        move_paths(s, m);
    }
    // Underlines and strike-throughs become rules down the line (on the left of the Roman baseline for an underline).
    for d in std::mem::take(&mut line.decorations) {
        if d.kind == DecorationKind::Highlight && d.screen_only {
            continue;
        }
        let mut shapes = if d.shapes.is_empty() {
            TextShape::rect(d.x0, d.y - d.thickness / 2.0, d.x1, d.y + d.thickness / 2.0, d.paint.clone(), d.overprint).into_iter().collect()
        } else {
            d.shapes.iter().map(|s| { let mut s = s.clone(); move_paths(&mut s, &Affine::translate(d.x0, d.y)); s }).collect::<Vec<_>>()
        };
        for s in &mut shapes { move_paths(s, m); }
        line.shapes.extend(shapes);
    }
    for o in &mut line.objects {
        let p = m.apply(Point::new(o.x, o.y));
        o.x = p.x;
        o.y = p.y;
    }
}

fn move_paths(s: &mut TextShape, m: &Affine) {
    for p in &mut s.paths {
        for pt in &mut p.points {
            pt.anchor = m.apply(pt.anchor);
            pt.left = m.apply(pt.left);
            pt.right = m.apply(pt.right);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upright_classes() {
        for c in ['가', '漢', 'あ', 'カ', '「', '。', 'Ａ', '１'] { assert!(upright(c), "{c}"); }
        for c in ['A', '1', ' ', '(', '-', 'é'] { assert!(!upright(c), "{c}"); }
    }

    #[test]
    fn quarter_turn_maps_lines_right_to_left() {
        // a frame 330..560 across: the virtual top (y = 330) is the right edge
        let m = map(330.0, 560.0);
        let p = m.apply(Point::new(60.0, 330.0 + 9.1113));
        assert!((p.x - (560.0 - 9.1113)).abs() < 1e-9 && (p.y - 60.0).abs() < 1e-9);
        let next = m.apply(Point::new(60.0, 330.0 + 29.1113));
        assert!(next.x < p.x, "the next line is left of the first");
    }

    #[test]
    fn upright_glyph_lands_on_the_em_box_centre() {
        // InDesign 2025, Malgun Gothic 12 pt: first line's Roman baseline 550.8887, the glyph's vertical origin on the
        // pen (y 60), centred on x 555.4238 (W2 position vector 500 917)
        let m = map(330.0, 560.0);
        let o = Orientation::Upright { hadv: 1.0, vorigin: 1878.0 / 2048.0, adv: 12.75, vadv: 12.75 };
        let p = place(o, 60.0, 330.0 + 9.1113, 0.0, 0.0, 12.0, 1798.0 / 2048.0, 0.0, (0.0, 0.0), 12.75);
        let f = m.apply(Point::new(p.x, p.y));
        assert!((f.x - (555.4238 - 6.0)).abs() < 0.001, "{f:?}");
        assert!((f.y - (60.0 + 1878.0 / 2048.0 * 12.0)).abs() < 0.001, "{f:?}");
        let l = Affine { tx: 0.0, ty: 0.0, ..m }.then_inner(&p.linear.unwrap());
        assert!((l.a - 1.0).abs() < 1e-12 && l.b.abs() < 1e-12 && l.c.abs() < 1e-12 && (l.d - 1.0).abs() < 1e-12, "upright: {l:?}");
    }
}
