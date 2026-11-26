//! Writes shared PDF resources, color functions, and graphics state values.

mod assets;
mod color_spaces;
pub(super) mod document;
mod page_tree;

use crate::colors::Alternate;
use crate::content::{self, ResUse};
use crate::registry;
use pdf_writer::{Name, Pdf, Ref};
use std::collections::HashMap;

pub(super) struct Refs(pub(super) i32);
impl Refs {
    pub(super) fn next(&mut self) -> Ref {
        self.0 += 1;
        Ref::new(self.0)
    }
}

pub(super) struct RefTables<'a> {
    pub(super) cs: &'a [Ref],
    pub(super) gs: &'a [Ref],
    pub(super) fonts: &'a HashMap<u32, Ref>,
    pub(super) forms: &'a [Ref],
    pub(super) images: &'a [Option<Ref>],
    pub(super) shadings: &'a [Ref],
    pub(super) patterns: &'a [Ref],
    pub(super) rasters: &'a [Ref],
    pub(super) ocgs: &'a [Ref],
}

pub(super) fn write_resources(mut r: pdf_writer::writers::Resources<'_>, used: &ResUse, t: &RefTables<'_>) {
    if !used.color_spaces.is_empty() {
        let mut d = r.color_spaces();
        for i in &used.color_spaces {
            d.pair(Name(content::cs_name(*i).as_bytes()), t.cs[*i]);
        }
    }
    if !used.ext_gstates.is_empty() {
        let mut d = r.ext_g_states();
        for i in &used.ext_gstates {
            d.pair(Name(content::gs_name(*i).as_bytes()), t.gs[*i]);
        }
    }
    if !used.fonts.is_empty() {
        let mut d = r.fonts();
        for i in &used.fonts {
            if let Some(f) = t.fonts.get(i) {
                d.pair(Name(content::font_name(*i).as_bytes()), *f);
            }
        }
    }
    if !used.forms.is_empty() || !used.images.is_empty() || !used.rasters.is_empty() {
        let mut d = r.x_objects();
        for i in &used.rasters {
            d.pair(Name(content::raster_name(*i).as_bytes()), t.rasters[*i]);
        }
        for i in &used.forms {
            d.pair(Name(content::form_name(*i).as_bytes()), t.forms[*i]);
        }
        for i in &used.images {
            if let Some(Some(im)) = t.images.get(*i) {
                d.pair(Name(content::image_name(*i).as_bytes()), *im);
            }
        }
    }
    if !used.shadings.is_empty() {
        let mut d = r.shadings();
        for i in &used.shadings {
            d.pair(Name(content::shading_name(*i).as_bytes()), t.shadings[*i]);
        }
    }
    if !used.patterns.is_empty() {
        let mut d = r.patterns();
        for i in &used.patterns {
            d.pair(Name(content::pattern_name(*i).as_bytes()), t.patterns[*i]);
        }
    }
    if !used.properties.is_empty() {
        let mut d = r.insert(Name(b"Properties")).dict();
        for (n, i) in used.properties.iter().enumerate() {
            if let Some(o) = t.ocgs.get(*i) {
                d.pair(Name(format!("MC{n}").as_bytes()), *o);
            }
        }
    }
}

pub(super) fn write_alt(arr: &mut pdf_writer::Array<'_>, alt: &Alternate, rgb: Option<Ref>) {
    match alt {
        Alternate::Cmyk(_) => {
            arr.item(Name(b"DeviceCMYK"));
        }
        Alternate::Lab(_) => {
            let mut a = arr.push().array();
            a.item(Name(b"Lab"));
            let mut d = a.push().dict();
            d.insert(Name(b"WhitePoint")).array().items([0.964203f32, 1.0, 0.824905]);
            d.insert(Name(b"BlackPoint")).array().items([0.0f32, 0.0, 0.0]);
            d.insert(Name(b"Range")).array().items([-128.0f32, 127.0, -128.0, 127.0]);
        }
        Alternate::Rgb(_) => match rgb {
            Some(r) => {
                arr.item(r);
            }
            None => {
                arr.item(Name(b"DeviceRGB"));
            }
        },
    }
}

/// PostScript calculator tint transform of a DeviceN: each CMYK output is the clamped sum of
/// the inks' CMYK equivalents.
pub(super) fn devicen_code(alts: &[[f32; 4]]) -> String {
    let n = alts.len();
    let mut code = String::from("{");
    for k in 0..4 {
        code.push_str(" 0");
        for (i, a) in alts.iter().enumerate() {
            let w = a[k];
            if w == 0.0 {
                continue;
            }
            let idx = k + 1 + (n - 1 - i);
            code.push_str(&format!(" {idx} index {w} mul add"));
        }
        code.push_str(" dup 1 gt {pop 1} if");
    }
    code.push_str(&format!(" {} 4 roll", n + 4));
    for _ in 0..n {
        code.push_str(" pop");
    }
    code.push_str(" }");
    code
}

/// The gradient function: exponential pieces between stops (midpoint as exponent),
/// constant before the first and after the last stop.
pub(super) fn write_ramp(pdf: &mut Pdf, refs: &mut Refs, stops: &[registry::RampStop]) -> Ref {
    type GradientPiece = (f32, f32, Vec<f32>, Vec<f32>, folio_color::GradientCurve);
    let mut pieces: Vec<GradientPiece> = Vec::new();
    if let Some(first) = stops.first()
        && first.location > 0.0
    {
        pieces.push((0.0, first.location, first.values.clone(), first.values.clone(), folio_color::GradientCurve::new(50.0)));
    }
    for w in stops.windows(2) {
        pieces.push((w[0].location, w[1].location.max(w[0].location), w[0].values.clone(), w[1].values.clone(), w[1].curve));
    }
    if let Some(last) = stops.last()
        && (last.location < 1.0 || stops.len() == 1)
    {
        pieces.push((last.location, 1.0, last.values.clone(), last.values.clone(), folio_color::GradientCurve::new(50.0)));
    }
    let r = refs.next();
    if pieces.len() == 1 && !pieces[0].4.reversed {
        let (_, _, c0, c1, n) = &pieces[0];
        let mut f = pdf.exponential_function(r);
        f.domain([0.0, 1.0]);
        f.c0(c0.iter().copied());
        f.c1(c1.iter().copied());
        f.n(n.exponent as f32);
        return r;
    }
    let sub: Vec<Ref> = pieces.iter().map(|_| refs.next()).collect();
    let mut bounds = Vec::new();
    let mut last = 0.0f32;
    for p in pieces.iter().take(pieces.len() - 1) {
        let b = p.1.max(last + 1e-5).min(1.0);
        bounds.push(b);
        last = b;
    }
    {
        let mut f = pdf.stitching_function(r);
        f.domain([0.0, 1.0]);
        f.functions(sub.iter().copied());
        f.bounds(bounds.iter().copied());
        f.encode(pieces.iter().flat_map(|p| if p.4.reversed { [1.0f32, 0.0] } else { [0.0, 1.0] }));
    }
    for (i, (_, _, c0, c1, n)) in pieces.iter().enumerate() {
        let (c0, c1) = if n.reversed { (c1, c0) } else { (c0, c1) };
        let mut f = pdf.exponential_function(sub[i]);
        f.domain([0.0, 1.0]);
        f.c0(c0.iter().copied());
        f.c1(c1.iter().copied());
        f.n(n.exponent as f32);
    }
    r
}

pub(super) fn blend(b: folio_model::BlendMode) -> pdf_writer::types::BlendMode {
    use folio_model::BlendMode as M;
    use pdf_writer::types::BlendMode as P;
    match b {
        M::Normal => P::Normal,
        M::Multiply => P::Multiply,
        M::Screen => P::Screen,
        M::Overlay => P::Overlay,
        M::SoftLight => P::SoftLight,
        M::HardLight => P::HardLight,
        M::ColorDodge => P::ColorDodge,
        M::ColorBurn => P::ColorBurn,
        M::Darken => P::Darken,
        M::Lighten => P::Lighten,
        M::Difference => P::Difference,
        M::Exclusion => P::Exclusion,
        M::Hue => P::Hue,
        M::Saturation => P::Saturation,
        M::Color => P::Color,
        M::Luminosity => P::Luminosity,
    }
}
