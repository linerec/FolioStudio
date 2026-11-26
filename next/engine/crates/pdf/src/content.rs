//! Content streams: a thin layer over `pdf_writer::Content` that tracks the graphics state
//! (CTM, colours, overprint) so nothing is set twice, and records which document-wide
//! resources the stream uses.

use crate::colors::Col;
use folio_model::{Affine, Point, Rect, SubPath};
use pdf_writer::{Content, Name, Str};
use std::collections::BTreeSet;

/// Document-wide resources a stream refers to, by index (resource names are derived from them).
#[derive(Clone, Debug, Default)]
pub struct ResUse {
    pub color_spaces: BTreeSet<usize>,
    pub ext_gstates: BTreeSet<usize>,
    pub fonts: BTreeSet<u32>,
    pub forms: BTreeSet<usize>,
    pub images: BTreeSet<usize>,
    pub shadings: BTreeSet<usize>,
    pub patterns: BTreeSet<usize>,
    /// Effect rasters (soft mask images).
    pub rasters: BTreeSet<usize>,
    /// Acrobat layers (optional content groups) the content marks, in the order first used: the n-th is named `MCn`, as InDesign
    /// names them on each page.
    pub properties: Vec<usize>,
}


pub fn cs_name(i: usize) -> String {
    format!("CS{i}")
}
pub fn gs_name(i: usize) -> String {
    format!("GS{i}")
}
pub fn font_name(i: u32) -> String {
    format!("F{i}")
}
pub fn form_name(i: usize) -> String {
    format!("X{i}")
}
pub fn image_name(i: usize) -> String {
    format!("Im{i}")
}
pub fn raster_name(i: usize) -> String {
    format!("Mk{i}")
}
pub fn shading_name(i: usize) -> String {
    format!("Sh{i}")
}
pub fn pattern_name(i: usize) -> String {
    format!("P{i}")
}

#[derive(Clone, Debug, PartialEq)]
enum Paint {
    Col(Col),
    Pattern(usize),
}

#[derive(Clone, Debug)]
struct State {
    ctm: Affine,
    fill: Option<Paint>,
    stroke: Option<Paint>,
    /// Overprint (fill, stroke) in effect; `None` when inherited and unknown (forms).
    op: Option<(bool, bool)>,
    line_width: Option<f32>,
    join: Option<u8>,
    cap: Option<u8>,
    miter: Option<f32>,
    /// Dash in effect (`None` when inherited and unknown); an empty array is solid.
    dash: Option<(Vec<f32>, f32)>,
    /// Join and cap are as tracked (`None` = the PDF default); false after
    /// [`Stream::forget_state`], when they are unknown.
    style_known: bool,
}

pub struct Stream {
    pub c: Content,
    pub used: ResUse,
    stack: Vec<State>,
    st: State,
    /// Whether anything in the stream needs transparency (groups, soft masks, alpha images).
    pub transparency: bool,
    /// Area of interest in this stream's root coordinates; items outside it are skipped.
    pub cull: Option<Rect>,
    chain: Option<TextChain>,
    /// Counts what was painted (not text): a text object ends there in InDesign's files, so the chain does too.
    painted: u64,
}

/// Where InDesign's relative text moves have got to. InDesign writes one text object (`BT`) that runs on across
/// frames until something else is painted, and moves along it with `Td`: a move in units of the text matrix,
/// rounded to .001, each one from the previous run's exact position, so the rounding errors add up line by line.
/// A different matrix (size, scale, skew) starts with an absolute `Tm` instead. Positions are in the stream's root space.
#[derive(Clone, Copy)]
pub struct TextChain {
    /// Size, horizontal scale, vertical scale, skew: what a `Td` does not change.
    pub matrix: [f64; 4],
    /// Where the previous run really starts, and where the rounded moves put it.
    pub exact: (f64, f64),
    pub printed: (f64, f64),
    painted: u64,
}

/// The ExtGState an overprint combination needs: interned by the caller.
pub trait GsSource {
    fn overprint_gs(&mut self, fill: bool, stroke: bool) -> usize;
}

fn f(v: f64) -> f32 {
    v as f32
}

impl Stream {
    pub fn new(cull: Option<Rect>) -> Self {
        Stream {
            c: Content::new(),
            used: ResUse::default(),
            stack: Vec::new(),
            st: State {
                ctm: Affine::IDENTITY,
                fill: None,
                stroke: None,
                op: Some((false, false)),
                line_width: None,
                join: None,
                cap: None,
                miter: None,
                dash: Some((Vec::new(), 0.0)),
                style_known: true,
            },
            transparency: false,
            cull,
            chain: None,
            painted: 0,
        }
    }

    /// The chain of relative text moves, unless something was painted since it was last set.
    pub fn text_chain(&self) -> Option<TextChain> {
        self.chain.filter(|c| c.painted == self.painted)
    }

    pub fn set_text_chain(&mut self, matrix: [f64; 4], exact: (f64, f64), printed: (f64, f64)) {
        self.chain = Some(TextChain { matrix, exact, printed, painted: self.painted });
    }

    pub fn end_text_chain(&mut self) {
        self.chain = None;
    }

    /// A form XObject's stream: the graphics state is inherited, so nothing is assumed.
    pub fn new_form(cull: Option<Rect>) -> Self {
        let mut s = Stream::new(cull);
        s.st.op = None;
        s.st.dash = None;
        s
    }

    pub fn ctm(&self) -> Affine {
        self.st.ctm
    }

    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.c.len()
    }

    pub fn is_empty(&self) -> bool {
        self.c.len() == 0
    }

    /// Forgets the tracked colours, overprint and line style (not the CTM), so what follows
    /// sets everything it uses: the flattener may wrap the paint before or after this point
    /// in `q … Q`.
    pub fn forget_state(&mut self) {
        let ctm = self.st.ctm;
        self.st = State { ctm, fill: None, stroke: None, op: None, line_width: None, join: None, cap: None, miter: None, dash: None, style_known: false };
    }

    pub fn push(&mut self) {
        self.c.save_state();
        self.stack.push(self.st.clone());
    }

    pub fn pop(&mut self) {
        self.c.restore_state();
        if let Some(s) = self.stack.pop() {
            self.st = s;
        }
    }

    pub fn concat(&mut self, t: &Affine) {
        if *t == Affine::IDENTITY {
            return;
        }
        self.c.transform([f(t.a), f(t.b), f(t.c), f(t.d), f(t.tx), f(t.ty)]);
        self.st.ctm = self.st.ctm.then_inner(t);
    }

    /// Sets the area (stream space) nothing outside of which is written; returns the one it replaces.
    pub fn set_cull(&mut self, cull: Option<Rect>) -> Option<Rect> {
        std::mem::replace(&mut self.cull, cull)
    }

    /// Whether `r` (current user space) can be visible.
    pub fn visible(&self, r: &Rect) -> bool {
        match &self.cull {
            None => true,
            Some(c) => {
                let d = self.st.ctm.apply_rect(r);
                d.left <= c.right && c.left <= d.right && d.top <= c.bottom && c.top <= d.bottom
            }
        }
    }

    pub fn set_overprint(&mut self, gs: &mut dyn GsSource, fill: Option<bool>, stroke: Option<bool>) {
        let cur = self.st.op;
        let want = (fill.or(cur.map(|c| c.0)).unwrap_or(false), stroke.or(cur.map(|c| c.1)).unwrap_or(false));
        if cur == Some(want) {
            return;
        }
        let i = gs.overprint_gs(want.0, want.1);
        self.ext_gstate(i);
        self.st.op = Some(want);
    }

    pub fn ext_gstate(&mut self, i: usize) {
        self.used.ext_gstates.insert(i);
        self.c.set_parameters(Name(gs_name(i).as_bytes()));
    }

    pub fn set_fill(&mut self, col: &Col) {
        let p = Paint::Col(col.clone());
        if self.st.fill.as_ref() == Some(&p) {
            return;
        }
        match col {
            Col::Cmyk([c, m, y, k]) => {
                self.c.set_fill_cmyk(*c, *m, *y, *k);
            }
            Col::Gray(g) => {
                self.c.set_fill_gray(*g);
            }
            Col::Space(cs, v) => {
                let fill_space = match &self.st.fill {
                    Some(Paint::Col(Col::Space(prev, _))) => *prev == *cs,
                    _ => false,
                };
                if !fill_space {
                    let name = cs_name(*cs);
                    self.c.set_fill_color_space(pdf_writer::types::ColorSpaceOperand::Named(Name(name.as_bytes())));
                    self.used.color_spaces.insert(*cs);
                }
                self.c.set_fill_color(v.iter().copied());
            }
        }
        self.st.fill = Some(p);
    }

    pub fn set_stroke(&mut self, col: &Col) {
        let p = Paint::Col(col.clone());
        if self.st.stroke.as_ref() == Some(&p) {
            return;
        }
        match col {
            Col::Cmyk([c, m, y, k]) => {
                self.c.set_stroke_cmyk(*c, *m, *y, *k);
            }
            Col::Gray(g) => {
                self.c.set_stroke_gray(*g);
            }
            Col::Space(cs, v) => {
                let same = matches!(&self.st.stroke, Some(Paint::Col(Col::Space(prev, _))) if *prev == *cs);
                if !same {
                    let name = cs_name(*cs);
                    self.c.set_stroke_color_space(pdf_writer::types::ColorSpaceOperand::Named(Name(name.as_bytes())));
                    self.used.color_spaces.insert(*cs);
                }
                self.c.set_stroke_color(v.iter().copied());
            }
        }
        self.st.stroke = Some(p);
    }

    pub fn set_fill_pattern(&mut self, pattern: usize) {
        let p = Paint::Pattern(pattern);
        if self.st.fill.as_ref() == Some(&p) {
            return;
        }
        self.c.set_fill_color_space(pdf_writer::types::ColorSpaceOperand::Pattern);
        let name = pattern_name(pattern);
        self.c.set_fill_pattern(None, Name(name.as_bytes()));
        self.used.patterns.insert(pattern);
        self.st.fill = Some(p);
    }

    /// `d`: the dash pattern (lengths and phase) of later strokes; `None` = solid.
    pub fn set_dash(&mut self, dash: Option<&folio_model::geom::Dash>) {
        let want = match dash {
            Some(d) if d.array.iter().any(|v| *v > 0.0) => (d.array.iter().map(|v| f(*v)).collect(), f(d.phase)),
            _ => (Vec::new(), 0.0),
        };
        if self.st.dash.as_ref() == Some(&want) {
            return;
        }
        self.c.set_dash_pattern(want.0.iter().copied(), want.1);
        self.st.dash = Some(want);
    }

    pub fn set_stroke_pattern(&mut self, pattern: usize) {
        let p = Paint::Pattern(pattern);
        if self.st.stroke.as_ref() == Some(&p) {
            return;
        }
        self.c.set_stroke_color_space(pdf_writer::types::ColorSpaceOperand::Pattern);
        let name = pattern_name(pattern);
        self.c.set_stroke_pattern(None, Name(name.as_bytes()));
        self.used.patterns.insert(pattern);
        self.st.stroke = Some(p);
    }

    pub fn line_style(&mut self, width: f64, join: folio_model::LineJoin, cap: folio_model::LineCap, miter: Option<f64>) {
        use pdf_writer::types::{LineCapStyle, LineJoinStyle};
        let w = f(width);
        if self.st.line_width != Some(w) {
            self.c.set_line_width(w);
            self.st.line_width = Some(w);
        }
        let j = join as u8;
        if !self.st.style_known || self.st.join.unwrap_or(0) != j {
            self.c.set_line_join(match join {
                folio_model::LineJoin::Miter => LineJoinStyle::MiterJoin,
                folio_model::LineJoin::Round => LineJoinStyle::RoundJoin,
                folio_model::LineJoin::Bevel => LineJoinStyle::BevelJoin,
            });
            self.st.join = Some(j);
        }
        let k = cap as u8;
        if !self.st.style_known || self.st.cap.unwrap_or(0) != k {
            self.c.set_line_cap(match cap {
                folio_model::LineCap::Butt => LineCapStyle::ButtCap,
                folio_model::LineCap::Round => LineCapStyle::RoundCap,
                folio_model::LineCap::Projecting => LineCapStyle::ProjectingSquareCap,
            });
            self.st.cap = Some(k);
        }
        self.st.style_known = true;
        // InDesign's default miter limit is 4; PDF's is 10.
        let m = f(miter.unwrap_or(4.0).max(1.0));
        if self.st.miter != Some(m) {
            self.c.set_miter_limit(m);
            self.st.miter = Some(m);
        }
    }

    /// Appends IDML subpaths (anchor + direction handles). Straight segments become lines.
    pub fn path(&mut self, paths: &[SubPath]) {
        for sp in paths {
            let pts = &sp.points;
            let Some(first) = pts.first() else { continue };
            self.c.move_to(f(first.anchor.x), f(first.anchor.y));
            for w in pts.windows(2) {
                self.segment(w[0].anchor, w[0].right, w[1].left, w[1].anchor);
            }
            if !sp.open && pts.len() > 1 {
                let last = pts.last().expect("path has a first point");
                if !(last.right == last.anchor && first.left == first.anchor) {
                    self.segment(last.anchor, last.right, first.left, first.anchor);
                }
                self.c.close_path();
            }
        }
    }

    fn segment(&mut self, a: Point, c1: Point, c2: Point, b: Point) {
        if c1 == a && c2 == b {
            self.c.line_to(f(b.x), f(b.y));
        } else {
            self.c.cubic_to(f(c1.x), f(c1.y), f(c2.x), f(c2.y), f(b.x), f(b.y));
        }
    }

    pub fn rect(&mut self, r: &Rect) {
        self.c.rect(f(r.left), f(r.top), f(r.width()), f(r.height()));
    }

    pub fn fill(&mut self, even_odd: bool) {
        self.painted += 1;
        if even_odd {
            self.c.fill_even_odd();
        } else {
            self.c.fill_nonzero();
        }
    }

    pub fn clip(&mut self, even_odd: bool) {
        self.painted += 1;
        if even_odd {
            self.c.clip_even_odd();
        } else {
            self.c.clip_nonzero();
        }
        self.c.end_path();
    }

    pub fn stroke(&mut self) {
        self.painted += 1;
        self.c.stroke();
    }

    pub fn xobject_form(&mut self, i: usize) {
        self.painted += 1;
        self.used.forms.insert(i);
        self.c.x_object(Name(form_name(i).as_bytes()));
    }

    pub fn xobject_image(&mut self, i: usize) {
        self.painted += 1;
        self.used.images.insert(i);
        self.c.x_object(Name(image_name(i).as_bytes()));
    }

    pub fn xobject_raster(&mut self, i: usize) {
        self.painted += 1;
        self.used.rasters.insert(i);
        self.c.x_object(Name(raster_name(i).as_bytes()));
    }

    pub fn shading(&mut self, i: usize) {
        self.painted += 1;
        self.used.shadings.insert(i);
        self.c.shading(Name(shading_name(i).as_bytes()));
    }

    pub fn begin_text(&mut self) {
        self.c.begin_text();
    }

    pub fn end_text(&mut self) {
        self.c.end_text();
    }

    pub fn font(&mut self, font: u32, size: f64) {
        self.used.fonts.insert(font);
        self.c.set_font(Name(font_name(font).as_bytes()), f(size));
    }

    /// Tr: 0 fill, 1 stroke, 2 fill then stroke.
    pub fn text_render_mode(&mut self, m: u8) {
        use pdf_writer::types::TextRenderingMode as T;
        self.c.set_text_rendering_mode(match m {
            1 => T::Stroke,
            2 => T::FillStroke,
            _ => T::Fill,
        });
    }

    pub fn text_matrix(&mut self, m: [f64; 6]) {
        self.c.set_text_matrix(m.map(f));
    }
    pub fn char_spacing(&mut self, em: f64) {
        self.c.set_char_spacing(f(em));
    }

    /// `TJ` with 2-byte codes and adjustments (thousandths of text space, positive = left).
    pub fn show_positioned(&mut self, items: &[(Vec<u8>, f32)]) {
        let mut sp = self.c.show_positioned();
        let mut it = sp.items();
        for (bytes, adj) in items {
            if !bytes.is_empty() {
                it.show(Str(bytes));
            }
            if *adj != 0.0 {
                it.adjust(*adj);
            }
        }
    }

    /// Starts the content of an Acrobat layer (`/OC /MCn BDC`; `ocg` is the layer's index among the PDF's optional content groups).
    pub fn begin_layer(&mut self, ocg: usize) {
        let n = match self.used.properties.iter().position(|&o| o == ocg) {
            Some(n) => n,
            None => {
                self.used.properties.push(ocg);
                self.used.properties.len() - 1
            }
        };
        let name = format!("MC{n}");
        self.c.begin_marked_content_with_properties(Name(b"OC")).properties_named(Name(name.as_bytes()));
    }

    pub fn end_layer(&mut self) {
        self.c.end_marked_content();
    }

    pub fn finish_form(self) -> (Vec<u8>, ResUse, bool) {
        self.finish()
    }

    pub fn finish(self) -> (Vec<u8>, ResUse, bool) {
        (self.c.finish().into_vec(), self.used, self.transparency)
    }
}
