//! Ink-native colour: document paints → the inks they print on.
//!
//! Process CMYK stays DeviceCMYK (a black tint is `0 0 0 t k`, so it separates on the black
//! plate alone); spot inks become Separation spaces named after the ink with the swatch's
//! own definition (CMYK, Lab or RGB) as alternate; mixed inks become DeviceN; Lab stays Lab;
//! RGB is ICC-tagged with the document RGB profile; Paper is a knockout white (`0 0 0 0 k`);
//! Registration is Separation /All. The Ink Manager decides whether a spot ink keeps its plate
//! (its alternate is used when it is converted to process) and where an alias prints.
//! Nothing goes through the RGB display path.
//!
//! This module owns the interpretation of swatches and the Ink Manager for print: the PDF
//! writer (`folio_pdf`) writes its colour spaces from it and the separations preview
//! (`crate::sep`) paints its ink planes from the same resolved colours, so plates on screen
//! and in the PDF cannot disagree.

use folio_model::{ColorModel, ColorSpace, Document, Ink, Paint, Swatch};
use std::collections::HashMap;

pub const PROCESS_NAMES: [&str; 4] = ["Cyan", "Magenta", "Yellow", "Black"];

/// The alternate (and Ink-Manager conversion) of a spot ink.
#[derive(Clone, Debug, PartialEq)]
pub enum Alternate {
    Cmyk([f32; 4]),
    Lab([f32; 3]),
    Rgb([f32; 3]),
}

impl Alternate {
    /// The colour of ink coverage `t` (0–1) laid on paper.
    pub fn at(&self, t: f32) -> Vec<f32> {
        match self {
            Alternate::Cmyk(c) => c.iter().map(|v| v * t).collect(),
            Alternate::Lab([l, a, b]) => vec![100.0 - t * (100.0 - l), a * t, b * t],
            Alternate::Rgb(c) => c.iter().map(|v| 1.0 - t * (1.0 - v)).collect(),
        }
    }
    /// White (no ink) in the alternate space.
    pub fn zero(&self) -> Vec<f32> {
        self.at(0.0)
    }
    /// A CMYK stand-in (for DeviceN alternates that mix spot and process inks).
    pub fn approx_cmyk(&self) -> [f32; 4] {
        match self {
            Alternate::Cmyk(c) => *c,
            Alternate::Rgb(c) => naive_cmyk(*c),
            Alternate::Lab(l) => naive_cmyk(lab_to_srgb(*l)),
        }
    }
}

fn naive_cmyk(rgb: [f32; 3]) -> [f32; 4] {
    let k = 1.0 - rgb.iter().cloned().fold(0.0f32, f32::max);
    if k >= 0.999 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let f = |v: f32| ((1.0 - v - k) / (1.0 - k)).clamp(0.0, 1.0);
    [f(rgb[0]), f(rgb[1]), f(rgb[2]), k]
}

/// D50 Lab → sRGB (0–1), Bradford-adapted; for approximations only.
pub fn lab_to_srgb(lab: [f32; 3]) -> [f32; 3] {
    let [l, a, b] = lab.map(f64::from);
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let inv = |t: f64| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f64 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.9642 * inv(fx), inv(fy), 0.8249 * inv(fz));
    // D50 XYZ → linear sRGB (Bradford-adapted matrix).
    let r = 3.1338561 * x - 1.6168667 * y - 0.4906146 * z;
    let g = -0.9787684 * x + 1.9161415 * y + 0.0334540 * z;
    let bl = 0.0719453 * x - 0.2289914 * y + 1.4052427 * z;
    let enc = |v: f64| {
        let v = v.clamp(0.0, 1.0);
        (if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }) as f32
    };
    [enc(r), enc(g), enc(bl)]
}

/// A colour space other than DeviceCMYK / DeviceGray, written once per document.
#[derive(Clone, Debug, PartialEq)]
pub enum CsDef {
    /// `[/Separation /ink alt tint-transform]`; ink "All" is the Registration colour.
    Separation { ink: String, alt: Alternate },
    /// `[/DeviceN [inks] /DeviceCMYK tint-transform]`: process inks are unit CMYK, spot inks
    /// their alternate as CMYK.
    DeviceN { inks: Vec<String>, cmyk: Vec<[f32; 4]> },
    /// CIE L*a*b* (D50), as InDesign writes Lab swatches.
    Lab,
    /// ICC-based RGB with the document RGB profile.
    IccRgb,
}

/// Paint ready for the content stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Col {
    Cmyk([f32; 4]),
    Gray(f32),
    /// Colour space index in `ColorSpaces::defs` and its component values.
    Space(usize, Vec<f32>),
}

/// What a resolved paint is, for overprint rules and reporting.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub col: Col,
    /// The [Black] swatch at 100 % (InDesign's "Overprint [Black] at 100 %").
    pub black100: bool,
}

impl Resolved {
    /// Only the black plate carries ink.
    pub fn k_only(&self) -> bool {
        matches!(self.col, Col::Cmyk([c, m, y, k]) if c == 0.0 && m == 0.0 && y == 0.0 && k > 0.0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct InkUse {
    pub uses: usize,
    pub output: String,
}

/// Document colour spaces, deduplicated; indices are stable and become resource names.
#[derive(Default)]
pub struct ColorSpaces {
    pub defs: Vec<CsDef>,
    pub rgb_used: bool,
}

impl ColorSpaces {
    pub fn intern(&mut self, d: CsDef) -> usize {
        if d == CsDef::IccRgb {
            self.rgb_used = true;
        }
        if let Some(i) = self.defs.iter().position(|x| *x == d) {
            return i;
        }
        self.defs.push(d);
        self.defs.len() - 1
    }
}

/// Paint resolution with the document's swatches and Ink Manager.
pub struct Resolver<'d> {
    doc: &'d Document,
    inks: HashMap<&'d str, &'d Ink>,
    pub spaces: ColorSpaces,
    pub ink_use: std::collections::BTreeMap<String, InkUse>,
    pub warnings: Vec<(String, String)>,
    cache: HashMap<(String, u32), Option<Resolved>>,
    /// CMYK-only output (PDF/X-1a "Convert to Destination (Preserve Numbers)"): RGB and Lab
    /// colours, and spot alternates defined in them, become CMYK of the document profile,
    /// converted exactly as the separations renderer converts them ([`cmyk8_of_rgb`],
    /// [`cmyk8_of_lab`]) so flattened rasters and vector paint print the same numbers. CMYK,
    /// gray and spot inks keep their values.
    output: Option<folio_color::Colors>,
}

/// A document RGB colour (0–1) as the output CMYK (0–1) of [`Resolver::with_cmyk_output`]:
/// quantised to 8 bits and converted with the document profiles, the pixel conversion the
/// separations renderer uses.
pub fn cmyk8_of_rgb(colors: &folio_color::Colors, rgb: [f32; 3]) -> [f32; 4] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    colors.rgb_to_cmyk8(rgb.map(q)).map(|v| v as f32 / 255.0)
}

/// A Lab colour (D50) as output CMYK (0–1), through sRGB as the separations renderer does.
pub fn cmyk8_of_lab(colors: &folio_color::Colors, lab: [f32; 3]) -> [f32; 4] {
    colors.srgb_to_cmyk8(folio_color::lab_to_srgb(lab.map(f64::from))).map(|v| v as f32 / 255.0)
}

const MAX_ALIAS: usize = 8;

fn process_channel(ink: &str) -> Option<usize> {
    match ink {
        "Process Cyan" | "Cyan" => Some(0),
        "Process Magenta" | "Magenta" => Some(1),
        "Process Yellow" | "Yellow" => Some(2),
        "Process Black" | "Black" => Some(3),
        _ => None,
    }
}

impl<'d> Resolver<'d> {
    pub fn new(doc: &'d Document) -> Self {
        Self {
            doc,
            inks: doc.print.inks.iter().map(|i| (i.name.as_str(), i)).collect(),
            spaces: ColorSpaces::default(),
            ink_use: Default::default(),
            warnings: Vec::new(),
            cache: HashMap::new(),
            output: None,
        }
    }

    /// A resolver for CMYK-only output: RGB and Lab colours, and spot alternates defined in
    /// them, are converted to the document CMYK with `colors` (see the `output` field).
    pub fn with_cmyk_output(doc: &'d Document, colors: folio_color::Colors) -> Self {
        Self { output: Some(colors), ..Self::new(doc) }
    }

    /// Whether RGB and Lab are converted to CMYK.
    pub fn cmyk_output(&self) -> bool {
        self.output.is_some()
    }

    /// CMYK of an alternate's full coverage: converted with the output profiles in CMYK-only
    /// output, else the plain approximation (PDF/X-4 keeps writing it as before).
    pub fn alt_cmyk(&self, a: &Alternate) -> [f32; 4] {
        match (&self.output, a) {
            (Some(c), Alternate::Rgb(rgb)) => cmyk8_of_rgb(c, *rgb),
            (Some(c), Alternate::Lab(lab)) => cmyk8_of_lab(c, *lab),
            _ => a.approx_cmyk(),
        }
    }

    /// In CMYK-only output an RGB or Lab alternate becomes its CMYK conversion.
    fn output_alternate(&self, a: Alternate) -> Alternate {
        match (&self.output, &a) {
            (Some(_), Alternate::Rgb(_) | Alternate::Lab(_)) => Alternate::Cmyk(self.alt_cmyk(&a)),
            _ => a,
        }
    }

    pub fn registration(&mut self, tint: f32) -> Col {
        let cs = self.spaces.intern(CsDef::Separation { ink: "All".into(), alt: Alternate::Cmyk([1.0; 4]) });
        Col::Space(cs, vec![tint])
    }

    /// Colour of a flat paint; `None` for no paint (None swatch, gradients are separate).
    pub fn resolve(&mut self, paint: &Paint) -> Option<Resolved> {
        let Paint::Swatch { id, tint } = paint else { return None };
        let key = (id.clone(), (tint * 1000.0).round() as u32);
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let r = self.resolve_swatch(id, *tint);
        self.cache.insert(key, r.clone());
        r
    }

    /// A fixed RGB colour that is no swatch (condition indicators), written as an RGB swatch is.
    pub fn rgb(&mut self, rgb: [u8; 3]) -> Resolved {
        let alt = Alternate::Rgb(rgb.map(|v| f32::from(v) / 255.0));
        Resolved { col: Own::Alt(alt, 1.0).into_col(self), black100: false }
    }

    /// The swatch with an extra tint (percent): a tint swatch multiplies its own tint.
    pub fn resolve_swatch(&mut self, id: &str, tint: f64) -> Option<Resolved> {
        let doc = self.doc;
        let sw = doc.swatches.get(id)?;
        let t = (sw.tint.unwrap_or(100.0) * tint / 10000.0).clamp(0.0, 1.0) as f32;
        let black100 = sw.base.is_none() && (id == "Color/Black") && t >= 0.9999;
        // A tint swatch prints with its base colour's definition (ink name, alternate).
        let def = sw.base.as_ref().and_then(|b| doc.swatches.get(b)).unwrap_or(sw);
        let col = match def.model {
            ColorModel::None => return None,
            ColorModel::Paper => Col::Cmyk([0.0; 4]),
            ColorModel::Registration => self.registration(t),
            ColorModel::Process => self.process(def, t)?,
            ColorModel::Spot => self.spot(def, t)?,
            ColorModel::Mixed => self.mixed(def, t)?,
        };
        Some(Resolved { col, black100 })
    }

    /// A process swatch in its own space, tinted: CMYK scales the inks, RGB and Lab mix with
    /// paper white.
    fn process(&mut self, sw: &Swatch, t: f32) -> Option<Col> {
        Some(self.own_space(sw.space, &sw.values, t, &sw.name)?.into_col(self))
    }

    fn own_space(&mut self, space: ColorSpace, v: &[f64], t: f32, name: &str) -> Option<Own> {
        let f = |i: usize| v.get(i).copied().unwrap_or(0.0) as f32;
        Some(match space {
            ColorSpace::Cmyk if v.len() >= 4 => Own::Alt(Alternate::Cmyk([f(0), f(1), f(2), f(3)].map(|x| (x / 100.0).clamp(0.0, 1.0))), t),
            ColorSpace::Rgb if v.len() >= 3 => Own::Alt(Alternate::Rgb([f(0), f(1), f(2)].map(|x| (x / 255.0).clamp(0.0, 1.0))), t),
            ColorSpace::Lab if v.len() >= 3 => Own::Alt(Alternate::Lab([f(0), f(1), f(2)]), t),
            ColorSpace::Gray if !v.is_empty() => Own::Gray(1.0 - (f(0) / 100.0).clamp(0.0, 1.0) * t),
            _ => {
                self.warnings.push(("colour".into(), format!("swatch {name} has no usable values; not painted")));
                return None;
            }
        })
    }

    /// The ink a spot swatch prints on after Ink Manager aliases: (ink name, its swatch).
    fn aliased<'s>(&mut self, name: &'s str) -> (String, Option<&'d Ink>) {
        let mut cur = name.to_owned();
        for _ in 0..MAX_ALIAS {
            match self.inks.get(cur.as_str()).copied() {
                Some(ink) => match &ink.alias {
                    Some(a) if *a != cur => cur = a.clone(),
                    _ => return (cur, Some(ink)),
                },
                None => return (cur, None),
            }
        }
        self.warnings.push(("colour".into(), format!("ink alias cycle at {name}")));
        (cur, None)
    }

    fn spot(&mut self, sw: &Swatch, t: f32) -> Option<Col> {
        let (ink_name, ink) = self.aliased(&sw.name);
        // An alias onto a process ink prints on that plate.
        if let Some(ch) = process_channel(&ink_name) {
            let mut c = [0.0f32; 4];
            c[ch] = t;
            self.note_ink(&sw.name, format!("alias of {}", ink_name));
            return Some(Col::Cmyk(c));
        }
        // The swatch defining the target ink (the alias target's own swatch, else this one).
        let target: &Swatch = if ink_name != sw.name {
            self.doc
                .swatches
                .values()
                .find(|s| s.model == ColorModel::Spot && s.name == ink_name && s.base.is_none())
                .unwrap_or(sw)
        } else {
            sw
        };
        let convert = ink.is_some_and(|i| i.convert_to_process);
        let own = self.own_space(target.space, &target.values, t, &target.name)?;
        if convert {
            self.note_ink(&sw.name, if ink_name != sw.name { format!("alias of {ink_name}, converted to process") } else { "converted to process".into() });
            return Some(own.into_col(self));
        }
        let Own::Alt(alt, _) = own else {
            // A gray-defined spot: keep the plate with a gray-as-black alternate.
            let cs = self.spaces.intern(CsDef::Separation { ink: ink_name.clone(), alt: Alternate::Cmyk([0.0, 0.0, 0.0, 1.0]) });
            self.note_ink(&sw.name, "own plate".into());
            return Some(Col::Space(cs, vec![t]));
        };
        if ink.is_none() {
            self.warnings.push(("colour".into(), format!("spot ink {ink_name} not in the Ink Manager; kept as its own plate")));
        }
        self.note_ink(&sw.name, if ink_name != sw.name { format!("alias of {ink_name}") } else { "own plate".into() });
        let alt = self.output_alternate(alt);
        let cs = self.spaces.intern(CsDef::Separation { ink: ink_name, alt });
        Some(Col::Space(cs, vec![t]))
    }

    fn note_ink(&mut self, name: &str, output: String) {
        let e = self.ink_use.entry(name.to_owned()).or_default();
        e.uses += 1;
        e.output = output;
    }

    /// One ink of a mixed ink or gradient as DeviceN parts: (ink name, amount 0–1, CMYK of
    /// full coverage) or folded into process CMYK.
    pub fn ink_parts(&mut self, ink: &str, amount: f32, cmyk: &mut [f32; 4], spots: &mut Vec<(String, f32, [f32; 4])>) {
        if let Some(ch) = process_channel(ink) {
            cmyk[ch] = (cmyk[ch] + amount).min(1.0);
            return;
        }
        let (name, def) = self.aliased(ink);
        if let Some(ch) = process_channel(&name) {
            cmyk[ch] = (cmyk[ch] + amount).min(1.0);
            return;
        }
        let swatch = self.doc.swatches.values().find(|s| s.model == ColorModel::Spot && s.name == name && s.base.is_none());
        let alt = match swatch.map(|s| (s.space, &s.values)) {
            Some((space, v)) => match self.own_space(space, v, 1.0, &name) {
                Some(Own::Alt(a, _)) => a,
                _ => Alternate::Cmyk([0.0, 0.0, 0.0, 1.0]),
            },
            None => {
                self.warnings.push(("colour".into(), format!("ink {name} has no swatch; its alternate is black")));
                Alternate::Cmyk([0.0, 0.0, 0.0, 1.0])
            }
        };
        if def.is_some_and(|d| d.convert_to_process) {
            let a = self.alt_cmyk(&alt);
            if !matches!(alt, Alternate::Cmyk(_)) && self.output.is_none() {
                self.warnings.push(("colour".into(), format!("converted ink {name} in a mix or gradient approximated as CMYK")));
            }
            for k in 0..4 {
                cmyk[k] = 1.0 - (1.0 - cmyk[k]) * (1.0 - a[k] * amount);
            }
            self.note_ink(&name, "converted to process".into());
            return;
        }
        self.note_ink(&name, "own plate".into());
        match spots.iter_mut().find(|s| s.0 == name) {
            Some(s) => s.1 = (s.1 + amount).min(1.0),
            None => spots.push((name, amount, self.alt_cmyk(&alt))),
        }
    }

    fn mixed(&mut self, sw: &Swatch, t: f32) -> Option<Col> {
        let base = sw.base.as_deref().unwrap_or(&sw.id);
        let Some(parts) = self.doc.print.mixed_inks.get(base).or_else(|| self.doc.print.mixed_inks.get(&sw.id)).cloned() else {
            // No ink list (old reader): the swatch's CMYK approximation.
            self.warnings.push(("colour".into(), format!("mixed ink {} without ink list; written as its CMYK values", sw.name)));
            return self.process(sw, t);
        };
        let mut cmyk = [0.0f32; 4];
        let mut spots = Vec::new();
        for (ink, pct) in &parts {
            self.ink_parts(ink, (*pct as f32 / 100.0 * t).clamp(0.0, 1.0), &mut cmyk, &mut spots);
        }
        Some(self.devicen(cmyk, &spots))
    }

    /// DeviceN of the process inks with ink and the spot inks (or DeviceCMYK / Separation
    /// when that is all there is).
    pub fn devicen(&mut self, cmyk: [f32; 4], spots: &[(String, f32, [f32; 4])]) -> Col {
        if spots.is_empty() {
            return Col::Cmyk(cmyk);
        }
        let mut inks = Vec::new();
        let mut alts = Vec::new();
        let mut values = Vec::new();
        for (k, v) in cmyk.iter().enumerate() {
            if *v > 0.0 {
                inks.push(PROCESS_NAMES[k].to_owned());
                let mut unit = [0.0f32; 4];
                unit[k] = 1.0;
                alts.push(unit);
                values.push(*v);
            }
        }
        for (name, amount, alt) in spots {
            inks.push(name.clone());
            alts.push(*alt);
            values.push(*amount);
        }
        let cs = self.spaces.intern(CsDef::DeviceN { inks, cmyk: alts });
        Col::Space(cs, values)
    }
}

/// A colour in the swatch's own space before interning.
enum Own {
    Alt(Alternate, f32),
    Gray(f32),
}

impl Own {
    fn into_col(self, r: &mut Resolver<'_>) -> Col {
        match self {
            // CMYK-only output: gray is black ink (InDesign's PDF/X-1a)
            Own::Gray(g) if r.output.is_some() => Col::Cmyk([0.0, 0.0, 0.0, 1.0 - g]),
            Own::Gray(g) => Col::Gray(g),
            Own::Alt(Alternate::Cmyk(c), t) => Col::Cmyk(c.map(|v| v * t)),
            Own::Alt(Alternate::Rgb(c), t) if r.output.is_some() => Col::Cmyk(r.alt_cmyk(&Alternate::Rgb(c.map(|v| 1.0 - t * (1.0 - v))))),
            Own::Alt(a @ Alternate::Lab(_), t) if r.output.is_some() => {
                let v = a.at(t);
                Col::Cmyk(r.alt_cmyk(&Alternate::Lab([v[0], v[1], v[2]])))
            }
            Own::Alt(a @ Alternate::Lab(_), t) => {
                let cs = r.spaces.intern(CsDef::Lab);
                Col::Space(cs, a.at(t))
            }
            Own::Alt(a @ Alternate::Rgb(_), t) => {
                let cs = r.spaces.intern(CsDef::IccRgb);
                Col::Space(cs, a.at(t))
            }
        }
    }
}
