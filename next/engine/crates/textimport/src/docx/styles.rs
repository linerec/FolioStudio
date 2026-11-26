//! Reads Word styles and computes imported formatting and local overrides.

use super::Notes;
use crate::messages::msg;
use crate::xmltree::El;
use crate::{StyleDef, SwatchDef};
use std::collections::BTreeSet;

/// Run formatting as Word writes it; `None` is "not said".
#[derive(Clone, Debug, Default)]
pub(super) struct RPr {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    size: Option<f64>,
    color: Option<String>,
    vert: Option<String>,
    caps: Option<bool>,
    small_caps: Option<bool>,
    /// Character spacing in twips.
    spacing: Option<f64>,
    /// `w:lang w:val` (Latin text) and `w:eastAsia` (East Asian text): Word's language codes.
    lang: Option<String>,
    lang_ea: Option<String>,
}

impl RPr {
    fn over(&mut self, top: &RPr) {
        macro_rules! take {
            ($($f:ident),*) => {$(if top.$f.is_some() { self.$f = top.$f.clone(); })*};
        }
        take!(bold, italic, underline, strike, size, color, vert, caps, small_caps, spacing, lang, lang_ea);
    }
}

/// Paragraph formatting; lengths in points.
#[derive(Clone, Debug, Default)]
pub(super) struct PPr {
    jc: Option<String>,
    pub(super) left: Option<f64>,
    right: Option<f64>,
    pub(super) first: Option<f64>,
    before: Option<f64>,
    after: Option<f64>,
    /// `AutoLeading`, percent.
    line: Option<f64>,
    keep_next: Option<bool>,
    keep_lines: Option<bool>,
    pub(super) tabs: Option<Vec<(String, f64)>>,
    /// `w:numPr`: the list (`w:numId`) and the level in it.
    pub(super) num_id: Option<String>,
    pub(super) ilvl: Option<usize>,
}

impl PPr {
    fn over(&mut self, top: &PPr) {
        macro_rules! take {
            ($($f:ident),*) => {$(if top.$f.is_some() { self.$f = top.$f.clone(); })*};
        }
        take!(jc, left, right, first, before, after, line, keep_next, keep_lines, tabs, num_id, ilvl);
    }
}

#[derive(Debug)]
pub(super) struct WStyle {
    pub(super) id: String,
    paragraph: bool,
    name: String,
    pub(super) based_on: Option<String>,
    next: Option<String>,
    pub(super) default: bool,
    p: PPr,
    r: RPr,
}

pub(super) fn number(e: &El, key: &str) -> Option<f64> {
    e.attr(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// `<w:b/>` is on, `<w:b w:val="0"/>` is off. InDesign takes "false" for on (measured: `<w:i w:val="false"/>` made the text italic).
fn toggle(e: &El) -> bool {
    e.attr("val") != Some("0")
}

pub(super) fn rgb_name(hex: &str) -> Option<([u8; 3], String)> {
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    let (r, g, b) = ((v >> 16) as u8, (v >> 8) as u8, v as u8);
    Some(([r, g, b], format!("Word_R{r}_G{g}_B{b}")))
}

pub(super) fn read_rpr(e: &El, notes: &mut Notes) -> RPr {
    let mut r = RPr::default();
    for c in e.elements() {
        match c.name.as_str() {
            "b" => r.bold = Some(toggle(c)),
            "i" => r.italic = Some(toggle(c)),
            "u" => r.underline = Some(c.attr("val") != Some("none")),
            "strike" => r.strike = Some(toggle(c)),
            "sz" => r.size = number(c, "val").map(|v| v / 2.0),
            "color" => match c.attr("val") {
                Some(v) if v.eq_ignore_ascii_case("auto") => r.color = Some("auto".into()),
                Some(v) if rgb_name(v).is_some() => r.color = Some(v.to_ascii_uppercase()),
                _ => notes.add(msg("textimport.color", "글자 색을 읽을 수 없어 가져오지 않았습니다", serde_json::json!({}))),
            },
            "vertAlign" => r.vert = c.attr("val").map(str::to_owned),
            "caps" => r.caps = Some(toggle(c)),
            "smallCaps" => r.small_caps = Some(toggle(c)),
            "spacing" => r.spacing = number(c, "val"),
            // no effect on what InDesign makes (measured) or only the complex-script twin of a setting
            "rStyle" | "szCs" | "bCs" | "iCs" | "cs" | "rtl" | "noProof" | "webHidden" => {}
            "rFonts" => notes.add(msg("textimport.fonts", "글꼴 지정은 InDesign과 같이 가져오지 않습니다 (스타일과 글 모두)", serde_json::json!({}))),
            "highlight" => notes.add(msg("textimport.highlight", "형광펜 표시는 InDesign과 같이 가져오지 않습니다", serde_json::json!({}))),
            "lang" => {
                r.lang = c.attr("val").map(str::to_owned);
                r.lang_ea = c.attr("eastAsia").map(str::to_owned);
            }
            other => notes.add(msg("textimport.characterFormat", format!("글자 서식 {other} 은(는) 아직 가져오지 않습니다"), serde_json::json!({"other": other}))),
        }
    }
    r
}

pub(super) fn read_ppr(e: &El, notes: &mut Notes) -> PPr {
    let mut p = PPr::default();
    let twips = |c: &El, key: &str| number(c, key).map(|v| v / 20.0);
    for c in e.elements() {
        match c.name.as_str() {
            "jc" => match c.attr("val") {
                Some("left") => p.jc = Some("LeftAlign".into()),
                Some("center") => p.jc = Some("CenterAlign".into()),
                Some("right") => p.jc = Some("RightAlign".into()),
                Some("both") => p.jc = Some("LeftJustified".into()),
                other => notes.add(msg("textimport.alignment", format!("문단 맞춤 {} 은(는) 아직 가져오지 않습니다", other.unwrap_or("?")), serde_json::json!({"alignment": other.unwrap_or("?")}))),
            },
            "ind" => {
                p.left = twips(c, "left");
                p.right = twips(c, "right");
                p.first = twips(c, "firstLine").or_else(|| twips(c, "hanging").map(|v| -v));
                if ["start", "end", "leftChars", "rightChars", "firstLineChars", "hangingChars"].iter().any(|k| c.attr(k).is_some()) {
                    notes.add(msg("textimport.characterIndent", "글자 수 단위의 들여쓰기는 아직 가져오지 않습니다", serde_json::json!({})));
                }
            }
            "spacing" => {
                p.before = twips(c, "before");
                p.after = twips(c, "after");
                match (number(c, "line"), c.attr("lineRule")) {
                    (Some(line), None | Some("auto")) => p.line = Some(line / 240.0 * 100.0),
                    (Some(_), Some(_)) => notes.add(msg("textimport.leading", "정확한 줄 간격·최소 줄 간격은 아직 가져오지 않습니다", serde_json::json!({}))),
                    _ => {}
                }
                if ["beforeLines", "afterLines", "beforeAutospacing", "afterAutospacing"].iter().any(|k| c.attr(k).is_some()) {
                    notes.add(msg("textimport.paragraphSpacing", "줄 수 단위·자동 문단 간격은 아직 가져오지 않습니다", serde_json::json!({})));
                }
            }
            "keepNext" => p.keep_next = Some(toggle(c)),
            "keepLines" => p.keep_lines = Some(toggle(c)),
            "tabs" => {
                let mut stops = Vec::new();
                for t in c.elements().filter(|t| t.name == "tab") {
                    match (t.attr("val"), t.attr("leader"), twips(t, "pos")) {
                        (Some("left"), None | Some("none"), Some(pos)) => stops.push(("LeftAlign".to_owned(), pos)),
                        _ => notes.add(msg("textimport.tabs", "가운데·오른쪽·소수점 탭과 채움 문자가 있는 탭은 아직 가져오지 않습니다", serde_json::json!({}))),
                    }
                }
                p.tabs = Some(stops);
            }
            "numPr" => {
                p.num_id = c.child("numId").and_then(|x| x.attr("val")).map(str::to_owned);
                p.ilvl = c.child("ilvl").and_then(|x| number(x, "val")).map(|v| v.max(0.0) as usize);
            }
            // no effect on what InDesign makes (measured: placetext-styles outlineLvl), the paragraph mark's own formatting, the section
            "pStyle" | "outlineLvl" | "rPr" | "sectPr" => {}
            other => notes.add(msg("textimport.paragraphFormat", format!("문단 서식 {other} 은(는) 아직 가져오지 않습니다"), serde_json::json!({"other": other}))),
        }
    }
    p
}

pub(super) struct Styles {
    list: Vec<WStyle>,
    defaults_r: RPr,
    defaults_p: PPr,
}

pub(super) fn read_styles(root: Option<&El>, notes: &mut Notes) -> Styles {
    let mut st = Styles { list: Vec::new(), defaults_r: RPr::default(), defaults_p: PPr::default() };
    let Some(root) = root else { return st };
    if let Some(d) = root.child("docDefaults") {
        if let Some(r) = d.child("rPrDefault").and_then(|x| x.child("rPr")) {
            st.defaults_r = read_rpr(r, notes);
        }
        if let Some(p) = d.child("pPrDefault").and_then(|x| x.child("pPr")) {
            st.defaults_p = read_ppr(p, notes);
        }
    }
    for s in root.elements().filter(|e| e.name == "style") {
        let kind = s.attr("type").unwrap_or("paragraph");
        if kind != "paragraph" && kind != "character" {
            continue;
        }
        let Some(id) = s.attr("styleId") else { continue };
        let name = s.child("name").and_then(|n| n.attr("val")).unwrap_or(id).to_owned();
        st.list.push(WStyle {
            id: id.to_owned(),
            paragraph: kind == "paragraph",
            name,
            based_on: s.child("basedOn").and_then(|n| n.attr("val")).map(str::to_owned),
            next: s.child("next").and_then(|n| n.attr("val")).map(str::to_owned),
            default: s.attr("default") == Some("1") || s.attr("default") == Some("true"),
            p: s.child("pPr").map(|e| read_ppr(e, notes)).unwrap_or_default(),
            r: s.child("rPr").map(|e| read_rpr(e, notes)).unwrap_or_default(),
        });
    }
    st
}

impl Styles {
    pub(super) fn find(&self, id: &str, paragraph: bool) -> Option<&WStyle> {
        self.list.iter().find(|s| s.id == id && s.paragraph == paragraph)
    }

    pub(super) fn default_paragraph(&self) -> Option<&WStyle> {
        self.list.iter().find(|s| s.paragraph && s.default)
    }

    /// The style and the styles it is based on, the oldest first (a loop in the file is cut).
    fn chain(&self, id: &str, paragraph: bool) -> Vec<&WStyle> {
        let mut out = Vec::new();
        let mut at = self.find(id, paragraph);
        while let Some(s) = at {
            if out.iter().any(|o: &&WStyle| o.id == s.id) {
                break;
            }
            out.push(s);
            at = s.based_on.as_deref().and_then(|b| self.find(b, paragraph));
        }
        out.reverse();
        out
    }

    /// The language codes of a run: the document defaults under the character style chain (the paragraph style's language is not
    /// imported: measured), then the run's own.
    pub(super) fn languages(&self, character_style: Option<&str>, own: &RPr) -> (Option<String>, Option<String>) {
        let mut r = self.defaults_r.clone();
        if let Some(id) = character_style {
            for s in self.chain(id, false) {
                r.over(&s.r);
            }
        }
        r.over(own);
        (r.lang, r.lang_ea)
    }

    /// What Word gives text in these styles: the document defaults under the paragraph style chain under the character style chain.
    pub(super) fn resolved(&self, paragraph_style: Option<&str>, character_style: Option<&str>) -> (RPr, PPr) {
        let (mut r, mut p) = (self.defaults_r.clone(), self.defaults_p.clone());
        if let Some(id) = paragraph_style {
            for s in self.chain(id, true) {
                r.over(&s.r);
                p.over(&s.p);
            }
        }
        if let Some(id) = character_style {
            for s in self.chain(id, false) {
                r.over(&s.r);
            }
        }
        (r, p)
    }
}

/// The InDesign language (`AppliedLanguage="$ID/<name>"`) a Word language code becomes. Measured one code at a time (oracle docxtext-c1-lang):
/// a code that is not listed (English and Spanish of any country, Dutch, Swedish, Norwegian, Thai ...) leaves the text in the document's language.
fn idml_language(code: &str) -> Option<&'static str> {
    let c = code.to_ascii_lowercase();
    Some(match c.as_str() {
        "de-de" | "de-ch" | "de-at" | "de-lu" => "de_DE_2006",
        "fr-fr" | "fr-ca" | "fr-ch" | "fr-be" => "French",
        "it-it" | "it-ch" => "Italian",
        "pt-pt" | "pt-br" => "Portuguese",
        "da-dk" => "Danish",
        "fi-fi" => "Finnish",
        "ru-ru" => "Russian",
        "pl-pl" => "Polish",
        "cs-cz" => "Czech",
        "tr-tr" => "Turkish",
        "el-gr" => "Greek",
        "hu-hu" => "Hungarian",
        "ro-ro" => "Romanian",
        "bg-bg" => "Bulgarian",
        "uk-ua" => "Ukrainian",
        "ca-es" => "Catalan",
        "he-il" => "Hebrew",
        "ar-sa" => "Arabic",
        "ja-jp" => "Japanese",
        "zh-cn" | "zh-tw" => "Chinese: Simplified",
        "ko-kr" => "Korean",
        "hi-in" => "hi_IN",
        // measured: Slovenian text comes in as Slovak (sl-SI), and sk-SK is left alone
        "sl-si" => "Slovak",
        "is-is" => "Icelandic",
        _ => return None,
    })
}

fn east_asian(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x2E80..=0x2FDF | 0x3000..=0x30FF | 0x3100..=0x318F | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// The language a run's text gets: the East Asian slot for text that is only East Asian letters, `w:val` for the rest (measured: a run of
/// Latin and Hangul takes `w:val` as a whole, a run of Hangul alone the East Asian slot).
pub(super) fn language_of(text: &str, val: Option<&str>, ea: Option<&str>) -> Option<&'static str> {
    let letters = || text.chars().filter(|c| c.is_alphanumeric() || east_asian(*c));
    let only_ea = letters().next().is_some() && letters().all(east_asian);
    idml_language(if only_ea { ea? } else { val? })
}

pub(super) fn num(v: f64) -> String {
    format!("{v}")
}

/// What formatting a paragraph or run has beyond `base`, as InDesign writes it.
fn font_style(bold: bool, italic: bool) -> Option<&'static str> {
    match (bold, italic) {
        (true, true) => Some("Bold Italic"),
        (true, false) => Some("Bold"),
        (false, true) => Some("Italic"),
        (false, false) => None,
    }
}

pub(super) struct Colors(pub(super) Vec<SwatchDef>);

impl Colors {
    pub(super) fn name(&mut self, hex: &str) -> Option<String> {
        if hex == "auto" {
            return None;
        }
        let (rgb, name) = rgb_name(hex)?;
        if !self.0.iter().any(|c| c.name == name) {
            self.0.push(SwatchDef { name: name.clone(), rgb, grouped: true });
        }
        Some(format!("Color/{name}"))
    }
}

/// The InDesign stroke style of a Word line kind (`w:val`): `Some(None)` is the solid line, `Some(Some(name))` a named style, `None` a kind that is
/// not known (measured: docxtext3-e44 .. e50, m01 .. m20, s01 .. s13; `none` and `nil` are the style `n`).
pub(super) fn line_kind(val: &str) -> Option<Option<&'static str>> {
    Some(match val {
        "single" | "thick" => None,
        "double" => Some("StrokeStyle/$ID/ThinThin"),
        "triple" => Some("StrokeStyle/$ID/Triple_Stroke"),
        "dotted" => Some("StrokeStyle/$ID/Canned Dotted"),
        "dashed" | "dashSmallGap" | "dotDash" | "dotDotDash" => Some("StrokeStyle/$ID/Canned Dashed 3x2"),
        "wave" => Some("StrokeStyle/$ID/Wavy"),
        "thinThickSmallGap" => Some("StrokeStyle/$ID/ThinThick"),
        "none" | "nil" => Some("n"),
        _ => return None,
    })
}

/// The attributes of a run's local formatting, compared with the resolved formatting `base` of its styles.
pub(super) fn run_attrs(run: &RPr, base: &RPr, colors: &mut Colors) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (bold, italic) = (base.bold == Some(true) || run.bold == Some(true), base.italic == Some(true) || run.italic == Some(true));
    if (bold, italic) != (base.bold == Some(true), base.italic == Some(true)) {
        out.extend(font_style(bold, italic).map(|f| ("FontStyle".to_owned(), f.to_owned())));
    }
    if let Some(size) = run.size.filter(|s| base.size != Some(*s)) {
        out.push(("PointSize".into(), num(size)));
    }
    if let Some(color) = run.color.as_deref().filter(|c| base.color.as_deref() != Some(*c)).and_then(|c| colors.name(c)) {
        out.push(("FillColor".into(), color));
    }
    if run.underline == Some(true) && base.underline != Some(true) {
        out.push(("Underline".into(), "true".into()));
    }
    if run.strike == Some(true) && base.strike != Some(true) {
        out.push(("StrikeThru".into(), "true".into()));
    }
    if let Some(v) = run.vert.as_deref().filter(|v| *v != "baseline" && base.vert.as_deref() != Some(*v)) {
        out.push(("Position".into(), if v == "superscript" { "Superscript" } else { "Subscript" }.into()));
    }
    if run.caps == Some(true) && base.caps != Some(true) {
        out.push(("Capitalization".into(), "AllCaps".into()));
    } else if run.small_caps == Some(true) && base.small_caps != Some(true) {
        out.push(("Capitalization".into(), "SmallCaps".into()));
    }
    // character spacing in 1/1000 em of the run's size (measured once: 2 pt at 11 pt is 181)
    if let Some(sp) = run.spacing.filter(|s| *s != 0.0) {
        let size = run.size.or(base.size).unwrap_or(10.0);
        out.push(("Tracking".into(), num((sp / 20.0 / size * 1000.0).trunc())));
    }
    out
}

/// The attributes of a paragraph's local formatting; `base` is what its style gives.
pub(super) fn paragraph_attrs(own: &PPr, base: &PPr) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut put = |key: &str, value: Option<f64>, was: Option<f64>| {
        if let Some(v) = value.filter(|v| Some(*v) != was && (*v != 0.0 || was.is_some_and(|w| w != 0.0))) {
            out.push((key.to_owned(), num(v)));
        }
    };
    put("LeftIndent", own.left, base.left);
    put("RightIndent", own.right, base.right);
    put("FirstLineIndent", own.first, base.first);
    put("SpaceBefore", own.before, base.before);
    put("SpaceAfter", own.after, base.after);
    put("AutoLeading", own.line, base.line);
    if let Some(j) = own.jc.as_deref().filter(|j| base.jc.as_deref() != Some(*j) && (*j != "LeftAlign" || base.jc.is_some())) {
        out.push(("Justification".into(), j.to_owned()));
    }
    if own.keep_next == Some(true) && base.keep_next != Some(true) {
        out.push(("KeepWithNext".into(), "1".into()));
    }
    if own.keep_lines == Some(true) && base.keep_lines != Some(true) {
        out.push(("KeepLinesTogether".into(), "true".into()));
        out.push(("KeepAllLinesTogether".into(), "true".into()));
    }
    out
}

/// The IDML name of a Word style (Word's built-in lower-case `heading 1` is `Heading 1` in InDesign: measured).
pub(super) fn style_name(s: &WStyle) -> String {
    let name = s.name.trim();
    if let Some(n) = name.strip_prefix("heading ")
        && n.chars().all(|c| c.is_ascii_digit())
    {
        return format!("Heading {n}");
    }
    // Word's other built-in names come in capitalised too (measured: footnote text, footnote reference)
    if matches!(name, "footnote text" | "footnote reference" | "endnote text" | "endnote reference") {
        return name[..1].to_ascii_uppercase() + &name[1..];
    }
    name.to_owned()
}

/// A style's own settings as the attributes of its definition (zero values are not written).
pub(super) fn style_def(s: &WStyle, styles: &Styles, colors: &mut Colors, root: bool) -> StyleDef {
    let (mut r, mut p) = (RPr::default(), PPr::default());
    if root {
        r.over(&styles.defaults_r);
        p.over(&styles.defaults_p);
    }
    r.over(&s.r);
    p.over(&s.p);
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut properties = Vec::new();
    if let Some(f) = font_style(r.bold == Some(true), r.italic == Some(true)) {
        attrs.push(("FontStyle".into(), f.into()));
    } else if !s.paragraph {
        // a character style that sets neither bold nor italic still names its font style (measured: docxtext-c1-lang, -fn)
        attrs.push(("FontStyle".into(), "Nothing".into()));
    }
    if let Some(size) = r.size {
        attrs.push(("PointSize".into(), num(size)));
    }
    let color = r.color.as_deref().and_then(|c| colors.name(c));
    if let Some(c) = &color {
        attrs.push(("FillColor".into(), c.clone()));
    }
    if r.underline == Some(true) {
        attrs.push(("Underline".into(), "true".into()));
        if let Some(c) = &color {
            properties.push(("UnderlineColor".to_owned(), "object".to_owned(), c.clone()));
        }
    }
    if r.strike == Some(true) {
        attrs.push(("StrikeThru".into(), "true".into()));
        properties.push(("StrikeThroughType".into(), "object".into(), "StrokeStyle/$ID/Solid".into()));
    }
    if let Some(v) = r.vert.as_deref().filter(|v| *v != "baseline") {
        attrs.push(("Position".into(), if v == "superscript" { "Superscript" } else { "Subscript" }.into()));
    }
    if r.caps == Some(true) {
        attrs.push(("Capitalization".into(), "AllCaps".into()));
    } else if r.small_caps == Some(true) {
        attrs.push(("Capitalization".into(), "SmallCaps".into()));
    }
    if s.paragraph {
        for (key, v) in [("LeftIndent", p.left), ("RightIndent", p.right), ("FirstLineIndent", p.first), ("AutoLeading", p.line), ("SpaceBefore", p.before), ("SpaceAfter", p.after)] {
            if let Some(v) = v.filter(|v| *v != 0.0) {
                attrs.push((key.into(), num(v)));
            }
        }
        if let Some(j) = p.jc.as_deref().filter(|j| *j != "LeftAlign") {
            attrs.push(("Justification".into(), j.into()));
        }
        if p.keep_lines == Some(true) {
            attrs.push(("KeepAllLinesTogether".into(), "true".into()));
        }
        if p.keep_next == Some(true) {
            attrs.push(("KeepWithNext".into(), "1".into()));
        }
        if p.keep_lines == Some(true) {
            attrs.push(("KeepLinesTogether".into(), "true".into()));
        }
    } else {
        attrs.push(("HorizontalScale".into(), "100".into()));
        attrs.push(("VerticalScale".into(), "100".into()));
        if color.is_some() {
            attrs.push(("StrokeColor".into(), "Swatch/None".into()));
            attrs.push(("FillTint".into(), "-1".into()));
        }
    }
    let based_on = s.based_on.as_deref().and_then(|b| styles.find(b, s.paragraph)).filter(|b| !b.default || s.paragraph).map(style_name);
    let next = s.paragraph.then(|| s.next.as_deref().and_then(|n| styles.find(n, true)).map_or_else(|| style_name(s), style_name));
    StyleDef { name: style_name(s), based_on, next, attrs, properties }
}

/// The styles to bring in: the used ones, the styles they are based on and continue with, in the order of Word's styles part.
pub(super) fn needed(styles: &Styles, used: &BTreeSet<String>, paragraph: bool) -> Vec<String> {
    let mut want: BTreeSet<String> = BTreeSet::new();
    let mut work: Vec<String> = used.iter().cloned().collect();
    while let Some(id) = work.pop() {
        let Some(s) = styles.find(&id, paragraph) else { continue };
        if (s.default && !paragraph) || !want.insert(id) {
            continue;
        }
        work.extend(s.based_on.clone());
        if paragraph {
            work.extend(s.next.clone());
        }
    }
    styles.list.iter().filter(|s| s.paragraph == paragraph && want.contains(&s.id)).map(|s| s.id.clone()).collect()
}
