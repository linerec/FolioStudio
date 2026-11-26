//! Applies character style layers and resolves their paint and emphasis marks.

use super::{ROOT_CHARACTER, StyleSheet, qualify};
use crate::color::Colors;
use crate::props::{Props, clean_name, is_none_ref};
use crate::warn::Warnings;
use folio_model::{Capitalization, CharacterAttrs, KerningMethod, Leading, Position};

/// Character-level state while resolving; paints resolve at the end.
#[derive(Clone, Debug)]
pub struct CharState {
    pub attrs: CharacterAttrs,
    fill: Option<String>,
    fill_tint: Option<f64>,
    stroke: Option<String>,
    stroke_tint: Option<f64>,
    underline_color: Option<String>,
    underline_tint: Option<f64>,
    strike_color: Option<String>,
    strike_tint: Option<f64>,
    underline_gap_color: Option<String>,
    underline_gap_tint: Option<f64>,
    strike_gap_color: Option<String>,
    strike_gap_tint: Option<f64>,

    kenten: KentenState,
}

/// Kenten attributes as read; resolved in `CharState::finish`.
#[derive(Clone, Debug, Default)]
struct KentenState {
    kind: Option<String>,
    size: Option<f64>,
    placement: Option<f64>,
    position: Option<String>,
    fill: Option<String>,
    tint: Option<f64>,
    custom: Option<String>,
}

fn kerning(v: &str) -> Option<KerningMethod> {
    let v = clean_name(v);
    Some(match v {
        "None" => KerningMethod::None,
        _ if v.contains("Roman") => KerningMethod::MetricsRomanOnly,
        _ if v.contains("Optical") => KerningMethod::Optical,
        _ if v.contains("Metrics") => KerningMethod::Metrics,
        _ => return None,
    })
}

pub fn apply_char<'a>(s: &mut CharState, p: impl Props<'a>, w: &mut Warnings) {
    let a = &mut s.attrs;
    if let Some(v) = p.str("AppliedFont") {
        a.font_family = v.to_owned();
    }
    if let Some(v) = p.str("FontStyle") {
        a.font_style = v.to_owned();
    }
    // `$ID/g155`: the glyph a U+FFFD character shows (measured: Find/Change ▸ Glyph to a glyph without a Unicode value)
    if let Some(v) = p.str("CustomGlyph") {
        a.custom_glyph = v.strip_prefix("$ID/g").and_then(|n| n.parse().ok());
    }
    let num = |k: &str, f: &mut f64| {
        if let Some(v) = p.f64(k) {
            *f = v;
        }
    };
    num("PointSize", &mut a.point_size);
    num("Tracking", &mut a.tracking);
    num("HorizontalScale", &mut a.horizontal_scale);
    num("VerticalScale", &mut a.vertical_scale);
    num("BaselineShift", &mut a.baseline_shift);
    num("Skew", &mut a.skew);
    num("StrokeWeight", &mut a.stroke_weight);
    // vertical text: tate-chu-yoko, character rotation, upright single-byte characters (`folio_text::vertical`)
    num("TatechuyokoXOffset", &mut a.vertical.tatechuyoko_x);
    num("TatechuyokoYOffset", &mut a.vertical.tatechuyoko_y);
    num("CharacterRotation", &mut a.vertical.rotation);
    if let Some(v) = p.str("Tatechuyoko") {
        a.vertical.tatechuyoko = v == "true";
    }
    if let Some(v) = p.str("RotateSingleByteCharacters") {
        a.vertical.upright_single_byte = v == "true";
    }
    if let Some(value) = p.f64("KerningValue") {
        a.kerning_manual = !crate::style_values::automatic_kerning(value);
        a.kerning_value = if a.kerning_manual { value } else { 0.0 };
    }
    if let Some(v) = p.get("Leading") {
        match v.as_str() {
            Some("Auto") => a.leading = Leading::Auto,
            _ => match v.f64() {
                Some(n) => a.leading = Leading::Fixed(n),
                None => w.add("unreadable Leading value"),
            },
        }
    }
    if let Some(v) = p.str("KerningMethod") {
        match kerning(v) {
            Some(k) => a.kerning_method = k,
            None => w.add(format!("kerning method {v} read as Metrics")),
        }
    }
    if let Some(v) = p.str("Capitalization") {
        a.capitalization = match v {
            "Normal" => Capitalization::Normal,
            "SmallCaps" => Capitalization::SmallCaps,
            "AllCaps" => Capitalization::AllCaps,
            "CapToSmallCap" => Capitalization::CapToSmallCap,
            "LowerCase" => Capitalization::LowerCase,
            other => {
                w.add(format!("capitalization {other} read as Normal"));
                Capitalization::Normal
            }
        };
    }
    if let Some(v) = p.str("Position") {
        a.position = match v {
            "Normal" => Position::Normal,
            "Superscript" => Position::Superscript,
            "Subscript" => Position::Subscript,
            "OTSuperscript" => Position::OtSuperscript,
            "OTSubscript" => Position::OtSubscript,
            "OTNumerator" => Position::OtNumerator,
            "OTDenominator" => Position::OtDenominator,
            other => {
                w.add(format!("position {other} read as Normal"));
                Position::Normal
            }
        };
    }
    let flag = |k: &str, f: &mut bool| {
        if let Some(v) = p.bool(k) {
            *f = v;
        }
    };
    crate::opentype::apply(&p, &mut a.opentype);
    flag("Ligatures", &mut a.ligatures);
    flag("Underline", &mut a.underline);
    flag("StrikeThru", &mut a.strike_through);
    flag("UnderlineOverprint", &mut a.underline_overprint);
    flag("StrikeThroughOverprint", &mut a.strike_through_overprint);
    if let Some(v) = p.f64("StrikeThroughOffset") { a.strike_through_offset = (v != -9999.).then_some(v); }
    if let Some(v) = p.f64("StrikeThroughWeight") { a.strike_through_weight = (v != -9999.).then_some(v); }
    if let Some(v) = p.str("StrikeThroughColor") { s.strike_color = Some(v.to_owned()); }
    if let Some(v) = p.f64("StrikeThroughTint") { s.strike_tint = Some(v); }
    if let Some(v) = p.str("UnderlineType") { a.underline_type = v.strip_prefix("StrokeStyle/$ID/").unwrap_or(v).to_owned(); }
    flag("UnderlineGapOverprint", &mut a.underline_gap_overprint);
    if let Some(v) = p.str("UnderlineGapColor") { s.underline_gap_color = Some(v.to_owned()); }
    if let Some(v) = p.f64("UnderlineGapTint") { s.underline_gap_tint = Some(v); }
    if let Some(v) = p.str("StrikeThroughType") { a.strike_through_type = v.strip_prefix("StrokeStyle/$ID/").unwrap_or(v).to_owned(); }
    flag("StrikeThroughGapOverprint", &mut a.strike_through_gap_overprint);
    if let Some(v) = p.str("StrikeThroughGapColor") { s.strike_gap_color = Some(v.to_owned()); }
    if let Some(v) = p.f64("StrikeThroughGapTint") { s.strike_gap_tint = Some(v); }
    flag("NoBreak", &mut a.no_break);
    flag("OverprintFill", &mut a.overprint_fill);
    flag("OverprintStroke", &mut a.overprint_stroke);
    if let Some(v) = p.str("AppliedLanguage") {
        a.language = clean_name(v.strip_prefix("Language/").unwrap_or(v)).to_owned();
    }
    // −9999 is InDesign's "font default".
    if let Some(v) = p.f64("UnderlineOffset") {
        a.underline_offset = (v > -9998.0).then_some(v);
    }
    if let Some(v) = p.f64("UnderlineWeight") {
        a.underline_weight = (v > -9998.0).then_some(v);
    }
    if let Some(v) = p.str("FillColor") {
        s.fill = Some(v.to_owned());
    }
    if let Some(v) = p.f64("FillTint") {
        s.fill_tint = Some(v);
    }
    if let Some(v) = p.str("StrokeColor") {
        s.stroke = Some(v.to_owned());
    }
    if let Some(v) = p.f64("StrokeTint") {
        s.stroke_tint = Some(v);
    }
    // where a gradient fill / stroke of the text sits (the defaults are not written)
    for (name, g) in [("Fill", &mut s.attrs.fill_gradient), ("Stroke", &mut s.attrs.stroke_gradient)] {
        if let Some(v) = p.str(&format!("Gradient{name}Start")).and_then(|v| crate::items::point(Some(v))) {
            g.start = v;
        }
        if let Some(v) = p.f64(&format!("Gradient{name}Length")) {
            g.length = v;
        }
        if let Some(v) = p.f64(&format!("Gradient{name}Angle")) {
            g.angle = v;
        }
    }
    if let Some(v) = p.str("UnderlineColor") {
        s.underline_color = Some(v.to_owned());
    }
    if let Some(v) = p.f64("UnderlineTint") {
        s.underline_tint = Some(v);
    }
    let k = &mut s.kenten;
    if let Some(v) = p.str("KentenKind") {
        k.kind = Some(v.to_owned());
    }
    if let Some(v) = p.f64("KentenFontSize") {
        k.size = Some(v);
    }
    if let Some(v) = p.f64("KentenPlacement") {
        k.placement = Some(v);
    }
    if let Some(v) = p.str("KentenPosition") {
        k.position = Some(v.to_owned());
    }
    if let Some(v) = p.str("KentenFillColor") {
        k.fill = Some(v.to_owned());
    }
    if let Some(v) = p.f64("KentenTint") {
        k.tint = Some(v);
    }
    if let Some(v) = p.str("KentenCustomCharacter") {
        k.custom = Some(v.to_owned());
    }
}

impl CharState {
    pub(super) fn new(attrs: CharacterAttrs) -> Self {
        Self {
            attrs,
            fill: None,
            fill_tint: None,
            stroke: None,
            stroke_tint: None,
            underline_color: None,
            underline_tint: None,
            strike_color: None,
            strike_tint: None,
            underline_gap_color: None,
            underline_gap_tint: None,
            strike_gap_color: None,
            strike_gap_tint: None,

            kenten: KentenState::default(),
        }
    }

    /// Final attributes with paints resolved against the swatches.
    pub fn finish(&self, colors: &Colors, w: &mut Warnings) -> CharacterAttrs {
        let mut a = self.attrs.clone();
        a.fill = colors.paint(self.fill.as_deref(), self.fill_tint, w);
        a.stroke = colors.paint(self.stroke.as_deref(), self.stroke_tint, w);
        let mut line_paint = |color: Option<&str>, tint: Option<f64>| {
            let text_color = matches!(color, None | Some("Text Color") | Some("$ID/Text Color"));
            let color = if text_color { self.fill.as_deref() } else { color };
            let tint = if text_color { self.fill_tint } else { tint.filter(|t| *t >= 0.) };
            Some(colors.paint(color, tint, w))
        };
        a.underline_color = line_paint(self.underline_color.as_deref(), self.underline_tint);
        a.strike_through_color = line_paint(self.strike_color.as_deref(), self.strike_tint);
        a.underline_gap_color = colors.paint(self.underline_gap_color.as_deref(), self.underline_gap_tint, w);
        a.strike_through_gap_color = colors.paint(self.strike_gap_color.as_deref(), self.strike_gap_tint, w);
        a.kenten = self.kenten(colors, w);
        a
    }

    fn kenten(&self, colors: &Colors, w: &mut Warnings) -> Option<folio_model::Kenten> {
        use folio_model::KentenKind as K;
        let k = &self.kenten;
        let kind = match k.kind.as_deref()? {
            "None" => return None,
            "KentenSesameDot" => K::SesameDot,
            "KentenWhiteSesameDot" => K::WhiteSesameDot,
            "KentenFisheye" => K::Fisheye,
            "KentenBullseye" => K::Bullseye,
            "KentenBlackCircle" => K::BlackCircle,
            "KentenWhiteCircle" => K::WhiteCircle,
            "KentenBlackTriangle" => K::BlackTriangle,
            "KentenWhiteTriangle" => K::WhiteTriangle,
            "KentenSmallBlackCircle" => K::SmallBlackCircle,
            "KentenSmallWhiteCircle" => K::SmallWhiteCircle,
            "Custom" => K::Custom,
            other => {
                w.add(format!("kenten {other} not known (drawn as a sesame dot)"));
                K::SesameDot
            }
        };
        let paint = match k.fill.as_deref() {
            None | Some("Text Color") | Some("$ID/Text Color") => None,
            Some(c) => Some(colors.paint(Some(c), k.tint.filter(|t| *t >= 0.0), w)),
        };
        Some(folio_model::Kenten {
            kind,
            size: k.size.filter(|s| *s > 0.0),
            placement: k.placement.unwrap_or(0.0),
            below: k.position.as_deref() == Some("BelowLeft"),
            paint,
            custom: k.custom.clone().unwrap_or_default(),
        })
    }
}

impl<'a> StyleSheet<'a> {
    /// Applies an `AppliedCharacterStyle` (root then BasedOn chain) on top of `s`; returns
    /// the style's display name ("" for none).
    pub fn apply_character_style(&self, s: &mut CharState, applied: Option<&str>, w: &mut Warnings) -> String {
        let id = match applied {
            Some(a) if !is_none_ref(a) => qualify("CharacterStyle", a),
            _ => return String::new(),
        };
        if let Some(root) = self.chr.get(ROOT_CHARACTER) {
            apply_char(s, &root.props, w);
        }
        if id == ROOT_CHARACTER {
            return String::new();
        }
        for l in Self::chain(&self.chr, &id, ROOT_CHARACTER, w) {
            apply_char(s, &l.props, w);
        }
        self.chr.get(id.as_str()).map(|d| d.name.clone()).unwrap_or_default()
    }

    /// `attrs` (final attributes of a character) with a character style's chain applied on
    /// top; colours the style does not set are kept.
    pub fn with_character_style(
        &self,
        attrs: &CharacterAttrs,
        applied: Option<&str>,
        colors: &Colors,
        w: &mut Warnings,
    ) -> CharacterAttrs {
        let id = match applied {
            Some(a) if !is_none_ref(a) => qualify("CharacterStyle", a),
            _ => return attrs.clone(),
        };
        if id == ROOT_CHARACTER {
            return attrs.clone();
        }
        let mut s = CharState::new(attrs.clone());
        for l in Self::chain(&self.chr, &id, ROOT_CHARACTER, w) {
            apply_char(&mut s, &l.props, w);
        }
        let mut out = s.attrs.clone();
        if s.fill.is_some() || s.fill_tint.is_some() {
            let base = s.fill.clone().or_else(|| match &attrs.fill {
                folio_model::Paint::Swatch { id, .. } => Some(id.clone()),
                _ => None,
            });
            out.fill = colors.paint(base.as_deref(), s.fill_tint, w);
        }
        if s.stroke.is_some() {
            out.stroke = colors.paint(s.stroke.as_deref(), s.stroke_tint, w);
        }
        if s.kenten.kind.is_some() {
            out.kenten = s.kenten(colors, w);
        }
        out
    }
}
