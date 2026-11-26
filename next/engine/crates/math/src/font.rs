//! The fonts equations are set in. Nothing is bundled: an installed OpenType font with a MATH
//! table (Cambria Math, STIX Two Math, Latin Modern Math, ...) is found by file name in the
//! usual font folders. Without one an installed text font is used with TeX-like constants and
//! the equation is marked approximate.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// The constants the layout needs, in em of the font.
#[derive(Clone, Debug)]
// Keep the complete parsed MATH constant set, including values copied into the fallback set.
#[allow(dead_code)]
pub struct Consts {
    pub script_scale: f64,
    pub script_script_scale: f64,
    pub delimited_min: f64,
    pub display_op_min: f64,
    pub axis: f64,
    pub accent_base: f64,
    pub sub_shift_down: f64,
    pub sub_top_max: f64,
    pub sub_baseline_drop_min: f64,
    pub sup_shift_up: f64,
    pub sup_shift_up_display: f64,
    pub sup_shift_up_cramped: f64,
    pub sup_bottom_min: f64,
    pub sup_baseline_drop_max: f64,
    pub sub_sup_gap_min: f64,
    pub sup_bottom_max_with_sub: f64,
    pub space_after_script: f64,
    pub upper_limit_gap_min: f64,
    pub upper_limit_rise_min: f64,
    pub lower_limit_gap_min: f64,
    pub lower_limit_drop_min: f64,
    pub stack_top_up: f64,
    pub stack_top_up_display: f64,
    pub stack_bottom_down: f64,
    pub stack_bottom_down_display: f64,
    pub stack_gap_min: f64,
    pub stack_gap_min_display: f64,
    pub frac_num_up: f64,
    pub frac_num_up_display: f64,
    pub frac_den_down: f64,
    pub frac_den_down_display: f64,
    pub frac_num_gap: f64,
    pub frac_num_gap_display: f64,
    pub frac_rule: f64,
    pub frac_den_gap: f64,
    pub frac_den_gap_display: f64,
    pub overbar_gap: f64,
    pub overbar_rule: f64,
    pub overbar_extra: f64,
    pub underbar_gap: f64,
    pub underbar_rule: f64,
    pub underbar_extra: f64,
    pub radical_gap: f64,
    pub radical_gap_display: f64,
    pub radical_rule: f64,
    pub radical_extra: f64,
    pub radical_kern_before: f64,
    pub radical_kern_after: f64,
    pub radical_raise: f64,
    pub min_connector_overlap: f64,
}

impl Consts {
    /// Values of the TeX math font (Computer Modern) as MathJax uses them, which is what InDesign draws
    /// with: an equation set with them has nearly InDesign's dimensions (see rules/mathml.ko.md).
    /// Also used when a font has no MATH table.
    pub fn tex_like() -> Consts {
        Consts {
            script_scale: 0.707,
            script_script_scale: 0.5,
            delimited_min: 1.3,
            display_op_min: 1.3,
            axis: 0.25,
            accent_base: 0.43,
            sub_shift_down: 0.15,
            sub_top_max: 0.43,
            sub_baseline_drop_min: 0.05,
            sup_shift_up: 0.363,
            sup_shift_up_display: 0.413,
            sup_shift_up_cramped: 0.289,
            sup_bottom_min: 0.108,
            sup_baseline_drop_max: 0.386,
            sub_sup_gap_min: 0.16,
            sup_bottom_max_with_sub: 0.4,
            space_after_script: 0.05,
            upper_limit_gap_min: 0.111,
            upper_limit_rise_min: 0.167,
            lower_limit_gap_min: 0.167,
            lower_limit_drop_min: 0.6,
            stack_top_up: 0.444,
            stack_top_up_display: 0.677,
            stack_bottom_down: 0.345,
            stack_bottom_down_display: 0.686,
            stack_gap_min: 0.12,
            stack_gap_min_display: 0.2,
            frac_num_up: 0.394,
            frac_num_up_display: 0.677,
            frac_den_down: 0.345,
            frac_den_down_display: 0.686,
            frac_num_gap: 0.04,
            frac_num_gap_display: 0.12,
            frac_rule: 0.04,
            frac_den_gap: 0.04,
            frac_den_gap_display: 0.12,
            overbar_gap: 0.12,
            overbar_rule: 0.04,
            overbar_extra: 0.04,
            underbar_gap: 0.12,
            underbar_rule: 0.04,
            underbar_extra: 0.04,
            radical_gap: 0.05,
            radical_gap_display: 0.1,
            radical_rule: 0.06,
            radical_extra: 0.04,
            radical_kern_before: 0.056,
            radical_kern_after: -0.167,
            radical_raise: 0.6,
            min_connector_overlap: 0.02,
        }
    }

    fn from_table(table: &ttf_parser::math::Table<'_>, upem: f64) -> Option<Consts> {
        let c = table.constants?;
        let v = |m: ttf_parser::math::MathValue<'_>| f64::from(m.value) / upem;
        let base = Consts::tex_like();
        let min_overlap = table.variants.map_or(base.min_connector_overlap, |x| f64::from(x.min_connector_overlap) / upem);
        Some(Consts {
            script_scale: f64::from(c.script_percent_scale_down()) / 100.0,
            script_script_scale: f64::from(c.script_script_percent_scale_down()) / 100.0,
            delimited_min: f64::from(c.delimited_sub_formula_min_height()) / upem,
            display_op_min: f64::from(c.display_operator_min_height()) / upem,
            axis: v(c.axis_height()),
            accent_base: v(c.accent_base_height()),
            sub_shift_down: v(c.subscript_shift_down()),
            sub_top_max: v(c.subscript_top_max()),
            sub_baseline_drop_min: v(c.subscript_baseline_drop_min()),
            sup_shift_up: v(c.superscript_shift_up()),
            sup_shift_up_display: v(c.superscript_shift_up()),
            sup_shift_up_cramped: v(c.superscript_shift_up_cramped()),
            sup_bottom_min: v(c.superscript_bottom_min()),
            sup_baseline_drop_max: v(c.superscript_baseline_drop_max()),
            sub_sup_gap_min: v(c.sub_superscript_gap_min()),
            sup_bottom_max_with_sub: v(c.superscript_bottom_max_with_subscript()),
            space_after_script: v(c.space_after_script()),
            upper_limit_gap_min: v(c.upper_limit_gap_min()),
            upper_limit_rise_min: v(c.upper_limit_baseline_rise_min()),
            lower_limit_gap_min: v(c.lower_limit_gap_min()),
            lower_limit_drop_min: v(c.lower_limit_baseline_drop_min()),
            stack_top_up: v(c.stack_top_shift_up()),
            stack_top_up_display: v(c.stack_top_display_style_shift_up()),
            stack_bottom_down: v(c.stack_bottom_shift_down()),
            stack_bottom_down_display: v(c.stack_bottom_display_style_shift_down()),
            stack_gap_min: v(c.stack_gap_min()),
            stack_gap_min_display: v(c.stack_display_style_gap_min()),
            frac_num_up: v(c.fraction_numerator_shift_up()),
            frac_num_up_display: v(c.fraction_numerator_display_style_shift_up()),
            frac_den_down: v(c.fraction_denominator_shift_down()),
            frac_den_down_display: v(c.fraction_denominator_display_style_shift_down()),
            frac_num_gap: v(c.fraction_numerator_gap_min()),
            frac_num_gap_display: v(c.fraction_num_display_style_gap_min()),
            frac_rule: v(c.fraction_rule_thickness()),
            frac_den_gap: v(c.fraction_denominator_gap_min()),
            frac_den_gap_display: v(c.fraction_denom_display_style_gap_min()),
            overbar_gap: v(c.overbar_vertical_gap()),
            overbar_rule: v(c.overbar_rule_thickness()),
            overbar_extra: v(c.overbar_extra_ascender()),
            underbar_gap: v(c.underbar_vertical_gap()),
            underbar_rule: v(c.underbar_rule_thickness()),
            underbar_extra: v(c.underbar_extra_descender()),
            radical_gap: v(c.radical_vertical_gap()),
            radical_gap_display: v(c.radical_display_style_vertical_gap()),
            radical_rule: v(c.radical_rule_thickness()),
            radical_extra: v(c.radical_extra_ascender()),
            radical_kern_before: v(c.radical_kern_before_degree()),
            radical_kern_after: v(c.radical_kern_after_degree()),
            radical_raise: f64::from(c.radical_degree_bottom_raise_percent()) / 100.0,
            min_connector_overlap: min_overlap,
        })
    }
}

pub struct Font {
    pub name: String,
    pub has_math: bool,
    pub upem: f64,
    face: Face<'static>,
}

/// One size of a vertical or horizontal glyph variant, or an assembly, from the MATH table.
pub struct Part {
    pub gid: GlyphId,
    pub start_overlap: f64,
    pub end_overlap: f64,
    pub full_advance: f64,
    pub extender: bool,
}


impl Font {
    fn load(data: &'static [u8], index: u32) -> Option<Font> {
        let face = Face::parse(data, index).ok()?;
        let upem = f64::from(face.units_per_em());
        if upem <= 0.0 {
            return None;
        }
        let name = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FULL_NAME && n.is_unicode())
            .and_then(|n| n.to_string())
            .or_else(|| {
                face.names().into_iter().find(|n| n.name_id == ttf_parser::name_id::FAMILY && n.is_unicode()).and_then(|n| n.to_string())
            })
            .unwrap_or_else(|| "unnamed".to_owned());
        let has_math = face.tables().math.is_some_and(|m| m.constants.is_some());
        Some(Font { name, has_math, upem, face })
    }

    pub fn consts(&self) -> Consts {
        self.face.tables().math.and_then(|t| Consts::from_table(&t, self.upem)).unwrap_or_else(Consts::tex_like)
    }

    pub fn glyph(&self, ch: char) -> Option<GlyphId> {
        self.face.glyph_index(ch).filter(|g| g.0 != 0)
    }

    /// Advance in em.
    pub fn advance(&self, gid: GlyphId) -> f64 {
        self.face.glyph_hor_advance(gid).map_or(0.0, |a| f64::from(a) / self.upem)
    }

    /// Ink box (x0, y0, x1, y1) in em.
    pub fn bbox(&self, gid: GlyphId) -> Option<[f64; 4]> {
        let b = self.face.glyph_bounding_box(gid)?;
        Some([f64::from(b.x_min) / self.upem, f64::from(b.y_min) / self.upem, f64::from(b.x_max) / self.upem, f64::from(b.y_max) / self.upem])
    }

    pub fn italic_correction(&self, gid: GlyphId) -> f64 {
        let Some(info) = self.face.tables().math.and_then(|m| m.glyph_info) else { return 0.0 };
        info.italic_corrections.and_then(|t| t.get(gid)).map_or(0.0, |v| f64::from(v.value) / self.upem)
    }

    pub fn top_accent(&self, gid: GlyphId) -> Option<f64> {
        let info = self.face.tables().math.and_then(|m| m.glyph_info)?;
        info.top_accent_attachments.and_then(|t| t.get(gid)).map(|v| f64::from(v.value) / self.upem)
    }

    /// The glyph's size variants (advance measurement in em), the glyph itself first.
    pub fn variants(&self, gid: GlyphId, vertical: bool) -> Vec<(GlyphId, f64)> {
        let Some(v) = self.face.tables().math.and_then(|m| m.variants) else { return Vec::new() };
        let table = if vertical { v.vertical_constructions } else { v.horizontal_constructions };
        let Some(c) = table.get(gid) else { return Vec::new() };
        c.variants.into_iter().map(|x| (x.variant_glyph, f64::from(x.advance_measurement) / self.upem)).collect()
    }

    pub fn assembly(&self, gid: GlyphId, vertical: bool) -> Vec<Part> {
        let Some(v) = self.face.tables().math.and_then(|m| m.variants) else { return Vec::new() };
        let table = if vertical { v.vertical_constructions } else { v.horizontal_constructions };
        let Some(a) = table.get(gid).and_then(|c| c.assembly) else { return Vec::new() };
        a.parts
            .into_iter()
            .map(|p| Part {
                gid: p.glyph_id,
                start_overlap: f64::from(p.start_connector_length) / self.upem,
                end_overlap: f64::from(p.end_connector_length) / self.upem,
                full_advance: f64::from(p.full_advance) / self.upem,
                extender: p.part_flags.extender(),
            })
            .collect()
    }

    /// The outline in em, y up, through `out`.
    pub fn outline(&self, gid: GlyphId, out: &mut dyn OutlineBuilder) -> bool {
        struct Scale<'a>(&'a mut dyn OutlineBuilder, f32);
        impl OutlineBuilder for Scale<'_> {
            fn move_to(&mut self, x: f32, y: f32) { self.0.move_to(x * self.1, y * self.1) }
            fn line_to(&mut self, x: f32, y: f32) { self.0.line_to(x * self.1, y * self.1) }
            fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.0.quad_to(x1 * self.1, y1 * self.1, x * self.1, y * self.1) }
            fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
                self.0.curve_to(x1 * self.1, y1 * self.1, x2 * self.1, y2 * self.1, x * self.1, y * self.1)
            }
            fn close(&mut self) { self.0.close() }
        }
        let mut scaled = Scale(out, (1.0 / self.upem) as f32);
        self.face.outline_glyph(gid, &mut scaled).is_some()
    }
}

/// Fonts for one run: the primary face, then faces that supply characters it lacks.
pub struct Fonts {
    pub list: Vec<&'static Font>,
    /// The primary face has a MATH table.
    pub math: bool,
}

impl Fonts {
    pub fn primary(&self) -> &'static Font {
        self.list[0]
    }

    /// The face and glyph for a character: the primary face first, then the supplementary ones.
    pub fn glyph(&self, ch: char) -> Option<(usize, GlyphId)> {
        self.list.iter().enumerate().find_map(|(i, f)| f.glyph(ch).map(|g| (i, g)))
    }
}

static FONTS: OnceLock<Result<Fonts, String>> = OnceLock::new();

/// The installed fonts to set equations in, found once.
pub fn fonts() -> Result<&'static Fonts, String> {
    FONTS.get_or_init(discover).as_ref().map_err(Clone::clone)
}

fn font_dirs() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut dirs = Vec::new();
    if let Some(d) = env("FOLIO_MATH_FONT_DIR") {
        dirs.push(d);
    }
    if let Some(local) = env("LOCALAPPDATA") {
        dirs.push(local.join("Microsoft").join("Windows").join("Fonts"));
    }
    if let Some(windir) = env("WINDIR") {
        dirs.push(windir.join("Fonts"));
    }
    if let Some(home) = env("HOME") {
        dirs.push(home.join("Library").join("Fonts"));
        dirs.push(home.join(".local").join("share").join("fonts"));
        dirs.push(home.join(".fonts"));
    }
    for d in ["/Library/Fonts", "/System/Library/Fonts", "/usr/local/share/fonts", "/usr/share/fonts"] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

fn font_files(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if depth > 0 {
                font_files(&path, depth - 1, out);
            }
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc")) {
            out.push(path);
        }
    }
}

fn leak(path: &Path) -> Option<&'static [u8]> {
    let data = std::fs::read(path).ok()?;
    Some(Box::leak(data.into_boxed_slice()))
}

/// Faces of a file, leaked for the process lifetime (a handful of fonts are loaded once).
fn faces_of(path: &Path) -> Vec<Font> {
    let Some(data) = leak(path) else { return Vec::new() };
    let count = ttf_parser::fonts_in_collection(data).unwrap_or(1);
    (0..count).filter_map(|i| Font::load(data, i)).collect()
}

/// Math fonts by how well they stand in for the TeX font InDesign draws with (lower is better).
const MATH_ORDER: &[&str] = &[
    "latinmodern-math", "latinmodernmath", "lmmath", "newcm", "stixtwomath", "stix2math", "xitsmath", "libertinusmath",
    "cambria", "asanamath", "asana-math", "texgyre", "dejavumath", "firamath", "notosansmath",
];
const TEXT_ORDER: &[&str] = &[
    "times.ttf", "timesnewroman", "times new roman", "liberationserif", "dejavuserif", "notoserif-regular", "georgia.ttf", "stix",
    "arial.ttf", "liberationsans", "dejavusans.ttf", "helvetica",
];
const CJK_ORDER: &[&str] = &[
    "malgun.ttf", "applesdgothicneo", "notosanscjk", "notoserifcjk", "nanumgothic", "nanummyeongjo", "batang", "gulim", "msgothic", "meiryo", "yugoth",
];

fn rank(name: &str, order: &[&str]) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    order.iter().position(|p| lower.contains(p))
}

fn discover() -> Result<Fonts, String> {
    let mut files = Vec::new();
    for d in font_dirs() {
        font_files(&d, 3, &mut files);
    }
    let pick = |order: &[&str], math: bool| -> Option<&'static Font> {
        let mut found: Vec<(usize, &PathBuf)> = files
            .iter()
            .filter_map(|p| {
                let name = p.file_name()?.to_str()?;
                rank(name, order).map(|r| (r, p))
            })
            .collect();
        if math {
            // any file with "math" in its name is a candidate after the known ones
            for p in &files {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if rank(name, order).is_none() && name.to_ascii_lowercase().contains("math") {
                    found.push((order.len(), p));
                }
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        for (_, path) in found {
            let faces = faces_of(path);
            // a collection holds several faces: the one with the MATH table is the math face
            let chosen = faces.into_iter().find(|f| f.has_math == math && (math || !f.name.to_ascii_lowercase().contains("math")));
            if let Some(f) = chosen {
                return Some(Box::leak(Box::new(f)));
            }
        }
        None
    };
    let mut list: Vec<&'static Font> = Vec::new();
    if let Some(path) = std::env::var_os("FOLIO_MATH_FONT").map(PathBuf::from)
        && let Some(f) = faces_of(&path).into_iter().find(|f| f.has_math) {
        list.push(Box::leak(Box::new(f)));
    }
    if list.is_empty()
        && let Some(f) = pick(MATH_ORDER, true) {
        list.push(f);
    }
    // Ubuntu commonly supplies MATH tables in fonts whose names omit “Math”.
    #[cfg(target_os = "linux")]
    if list.is_empty()
        && let Some(f) = pick(&["freeserif.ttf", "dejavuserif.ttf"], true) {
        list.push(f);
    }
    let math = !list.is_empty();
    if !math {
        let text = pick(TEXT_ORDER, false).ok_or_else(|| "no installed font to set equations in".to_owned())?;
        list.push(text);
    }
    if let Some(cjk) = pick(CJK_ORDER, false) {
        list.push(cjk);
    }
    // a text font as a second source of characters the math font lacks (letters, text)
    if math
        && let Some(text) = pick(TEXT_ORDER, false) {
        list.push(text);
    }
    Ok(Fonts { list, math })
}
