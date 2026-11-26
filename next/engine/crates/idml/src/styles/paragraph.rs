//! Reads and resolves paragraph attributes, rules, borders, lists and tabs.

use super::character::{CharState, apply_char};
use super::{ROOT_PARAGRAPH, StyleDef, StyleSheet, qualify};
use crate::color::Colors;
use crate::props::{Props, Val, clean_name, is_none_ref};
use crate::warn::Warnings;
use folio_model::{
    BottomOrigin, CharacterAttrs, Justification, ListKind, ListStyle, ParagraphAttrs, ParagraphBorder, ParagraphRule,
    ParagraphShading, RuleWidth, TabAlignment, TabStop, TopOrigin,
};
use std::sync::Arc;

/// Paragraph-level state while resolving; finalised into `ParagraphAttrs`.
#[derive(Clone, Debug)]
pub struct ParaState {
    pub attrs: ParagraphAttrs,
    pub auto_leading: f64,
    /// Settings InDesign would draw that the model does not carry (warned per paragraph).
    pub unmodelled: Vec<&'static str>,
    /// Rule above, rule below, shading, border and list settings as the layers set them;
    /// colours resolve in `finish`.
    rules: [RuleRaw; 2],
    shading: BoxRaw,
    border: BoxRaw,
    merge_borders: bool,
    pub list: ListRaw,
}

#[derive(Clone, Debug)]
struct RuleRaw {
    on: bool,
    weight: f64,
    color: Option<String>,
    tint: Option<f64>,
    offset: f64,
    left: f64,
    right: f64,
    text_width: bool,
    overprint: bool,
    stroke_type: String,
    gap_color: Option<String>,
    gap_tint: Option<f64>,
    gap_overprint: bool,
    keep: bool,
}

impl Default for RuleRaw {
    fn default() -> Self {
        RuleRaw {
            on: false,
            weight: 1.0,
            color: None,
            tint: None,
            offset: 0.0,
            left: 0.0,
            right: 0.0,
            text_width: false,
            overprint: false,
            stroke_type: "Solid".into(),
            gap_color: None,
            gap_tint: None,
            gap_overprint: false,
            keep: false,
        }
    }
}

#[derive(Clone, Debug)]
struct BoxRaw {
    on: bool,
    color: Option<String>,
    tint: Option<f64>,
    overprint: bool,
    offsets: [f64; 4],
    weights: [f64; 4],
    top: TopOrigin,
    bottom: BottomOrigin,
    text_width: bool,
    stroke_type: String,
    gap_color: Option<String>,
    gap_tint: Option<f64>,
    gap_overprint: bool,
    display_if_splits: bool,
    clip: bool,
    suppress: bool,
    /// A corner option other than None (drawn square).
    corners: bool,
}

impl Default for BoxRaw {
    fn default() -> Self {
        BoxRaw {
            on: false,
            color: None,
            tint: None,
            overprint: false,
            offsets: [0.0; 4],
            weights: [1.0; 4],
            top: TopOrigin::Ascent,
            bottom: BottomOrigin::Descent,
            text_width: false,
            stroke_type: "Solid".into(),
            gap_color: None,
            gap_tint: None,
            gap_overprint: false,
            display_if_splits: false,
            clip: false,
            suppress: false,
            corners: false,
        }
    }
}

/// Bullets and numbering as the layers set them.
#[derive(Clone, Debug)]
pub struct ListRaw {
    pub kind: Option<ListKind>,
    bullet: String,
    bullet_after: String,
    pub bullet_font: Option<String>,
    pub bullet_font_style: Option<String>,
    pub bullet_style: Option<String>,
    pub number_style: Option<String>,
    format: String,
    expression: String,
    start_at: u32,
    continue_numbers: bool,
    level: u32,
    list: String,
    restart_policy: bool,
    bullet_align: TabAlignment,
    number_align: TabAlignment,
}

impl Default for ListRaw {
    fn default() -> Self {
        ListRaw {
            kind: None,
            bullet: "\u{2022}".into(),
            bullet_after: "^t".into(),
            bullet_font: None,
            bullet_font_style: None,
            bullet_style: None,
            number_style: None,
            format: "1, 2, 3, 4...".into(),
            expression: "^#.^t".into(),
            start_at: 1,
            continue_numbers: true,
            level: 1,
            list: String::new(),
            restart_policy: true,
            bullet_align: TabAlignment::Left,
            number_align: TabAlignment::Left,
        }
    }
}

impl ParaState {
    pub fn new() -> ParaState {
        ParaState {
            attrs: ParagraphAttrs::default(),
            auto_leading: 120.0,
            unmodelled: Vec::new(),
            rules: [RuleRaw::default(), RuleRaw::default()],
            shading: BoxRaw { color: Some("Color/Black".into()), tint: Some(20.0), ..BoxRaw::default() },
            border: BoxRaw::default(),
            merge_borders: true,
            list: ListRaw::default(),
        }
    }

    /// Paragraph attributes with rules, shading, border and list settings resolved (the
    /// list's marker attributes are set when the paragraph's first character is known).
    pub fn finish(&self, colors: &Colors, w: &mut Warnings) -> ParagraphAttrs {
        let mut a = self.attrs.clone();
        let rule = |r: &RuleRaw, w: &mut Warnings| -> Option<ParagraphRule> {
            if !r.on || r.weight <= 0.0 {
                return None;
            }
            let text_color = matches!(r.color.as_deref(), None | Some("Text Color") | Some("$ID/Text Color"));
            let tint = r.tint.filter(|t| (0.0..=100.0).contains(t));
            // Drawn by the layout (text/layout/strokes.rs) for every built-in stroke style, with its gap colour; a
            // custom one it does not know is drawn solid.
            if folio_model::geom::StrokePattern::builtin(&r.stroke_type, 1.0, None, folio_model::geom::CornerAdjust::DashesAndGaps).is_none() {
                w.add(format!("paragraph rule stroke type {} drawn solid", r.stroke_type));
            }
            Some(ParagraphRule {
                weight: r.weight,
                paint: (!text_color).then(|| colors.paint(r.color.as_deref(), tint, w)),
                text_tint: tint,
                offset: r.offset,
                left_indent: r.left,
                right_indent: r.right,
                width: if r.text_width { RuleWidth::Text } else { RuleWidth::Column },
                overprint: r.overprint,
                stroke_type: r.stroke_type.clone(),
                gap_paint: colors.paint(r.gap_color.as_deref(), r.gap_tint, w),
                gap_overprint: r.gap_overprint,
                keep_in_frame: r.keep,
            })
        };
        a.rule_above = rule(&self.rules[0], w);
        a.rule_below = rule(&self.rules[1], w);
        let s = &self.shading;
        if s.on {
            if s.corners {
                w.add("paragraph shading corner options not modelled (corners drawn square)");
            }
            if s.suppress {
                w.add("paragraph shading set not to print: not drawn");
            }
            a.shading = (!s.suppress).then(|| ParagraphShading {
                paint: colors.paint(s.color.as_deref(), s.tint, w),
                overprint: s.overprint,
                offsets: s.offsets,
                top: s.top,
                bottom: s.bottom,
                width: if s.text_width { RuleWidth::Text } else { RuleWidth::Column },
                clip_to_frame: s.clip,
                suppress_printing: s.suppress,
            });
        }
        let b = &self.border;
        if b.on {
            if b.corners {
                w.add("paragraph border corner options not modelled (corners drawn square)");
            }
            if folio_model::geom::StrokePattern::builtin(&b.stroke_type, 1.0, None, folio_model::geom::CornerAdjust::DashesAndGaps).is_none() {
                w.add(format!("paragraph border stroke type {} drawn solid", b.stroke_type));
            }
            a.border = Some(ParagraphBorder {
                paint: colors.paint(b.color.as_deref(), b.tint, w),
                overprint: b.overprint,
                weights: b.weights,
                offsets: b.offsets,
                top: b.top,
                bottom: b.bottom,
                width: if b.text_width { RuleWidth::Text } else { RuleWidth::Column },
                stroke_type: b.stroke_type.clone(),
                gap_paint: colors.paint(b.gap_color.as_deref(), b.gap_tint, w),
                gap_overprint: b.gap_overprint,
                display_if_splits: b.display_if_splits,
                merge: self.merge_borders,
            });
        }
        let l = &self.list;
        if let Some(kind) = l.kind {
            let alignment = if kind == ListKind::Bullet { l.bullet_align } else { l.number_align };
            if alignment != TabAlignment::Left {
                w.add("bullet / number alignment other than left set left");
            }
            a.list = Some(ListStyle {
                kind,
                bullet: l.bullet.clone(),
                bullet_text_after: metachars(&l.bullet_after),
                expression: metachars(&l.expression),
                format: l.format.clone(),
                restart_at: (!l.continue_numbers).then_some(l.start_at),
                start_at: l.start_at,
                level: l.level.max(1),
                list: l.list.clone(),
                restart_after_higher_level: l.restart_policy,
                alignment,
                marker: CharacterAttrs::default(),
            });
        }
        a
    }
}

/// InDesign text metacharacters in bullet / numbering text: `^t` tab, spaces and dashes;
/// `^#` and `^1` … `^9` (numbers) are kept for layout.
pub fn metachars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '^' {
            out.push(c);
            continue;
        }
        let Some(&n) = it.peek() else {
            out.push('^');
            break;
        };
        let rep: Option<&str> = match n {
            't' => Some("\t"),
            'm' => Some("\u{2003}"),
            '>' => Some("\u{2002}"),
            'f' => Some("\u{2009}"),
            '|' => Some("\u{200A}"),
            's' | 'S' => Some("\u{00A0}"),
            '<' => Some("\u{2009}"),
            '_' => Some("\u{2014}"),
            '=' => Some("\u{2013}"),
            '-' => Some("\u{00AD}"),
            '~' => Some("\u{2011}"),
            'n' => Some("\n"),
            'y' => Some("\u{8}"),
            'i' => Some("\u{7}"),
            '^' => Some("^"),
            _ => None,
        };
        match rep {
            Some(r) => {
                it.next();
                out.push_str(r);
            }
            // `^#`, `^1` … `^9` and anything unknown stay as written.
            None => out.push('^'),
        }
    }
    out
}

fn origin_top(v: &str) -> Option<TopOrigin> {
    Some(match v {
        "AscentTopOrigin" => TopOrigin::Ascent,
        "BaselineTopOrigin" => TopOrigin::Baseline,
        "LeadingTopOrigin" => TopOrigin::Leading,
        _ => return None,
    })
}

fn origin_bottom(v: &str) -> Option<BottomOrigin> {
    Some(match v {
        "DescentBottomOrigin" => BottomOrigin::Descent,
        "BaselineBottomOrigin" => BottomOrigin::Baseline,
        _ => return None,
    })
}

/// `StrokeStyle/$ID/Solid` → "Solid".
pub fn stroke_type_name(v: &str) -> String {
    clean_name(v.strip_prefix("StrokeStyle/").unwrap_or(v)).to_owned()
}

fn align_of(v: &str) -> TabAlignment {
    match v {
        "CenterAlign" => TabAlignment::Center,
        "RightAlign" => TabAlignment::Right,
        _ => TabAlignment::Left,
    }
}

/// Rule, shading, border and list properties of one layer.
fn apply_structures<'a>(s: &mut ParaState, p: &impl Props<'a>, w: &mut Warnings) {
    for (i, pre) in ["RuleAbove", "RuleBelow"].into_iter().enumerate() {
        let r = &mut s.rules[i];
        let k = |suffix: &str| format!("{pre}{suffix}");
        if let Some(v) = p.bool(pre) {
            r.on = v;
        }
        if let Some(v) = p.f64(&k("LineWeight")) {
            r.weight = v;
        }
        if let Some(v) = p.str(&k("Color")) {
            r.color = Some(v.to_owned());
        }
        if let Some(v) = p.f64(&k("Tint")) {
            r.tint = Some(v);
        }
        if let Some(v) = p.f64(&k("Offset")) {
            r.offset = v;
        }
        if let Some(v) = p.f64(&k("LeftIndent")) {
            r.left = v;
        }
        if let Some(v) = p.f64(&k("RightIndent")) {
            r.right = v;
        }
        if let Some(v) = p.str(&k("Width")) {
            r.text_width = v == "TextWidth";
        }
        if let Some(v) = p.bool(&k("Overprint")) {
            r.overprint = v;
        }
        if let Some(v) = p.str(&k("Type")) {
            r.stroke_type = stroke_type_name(v);
        }
        if let Some(v) = p.str(&k("GapColor")) {
            r.gap_color = Some(v.to_owned());
        }
        if let Some(v) = p.f64(&k("GapTint")) {
            r.gap_tint = Some(v);
        }
        if let Some(v) = p.bool(&k("GapOverprint")) {
            r.gap_overprint = v;
        }
    }
    if let Some(v) = p.bool("KeepRuleAboveInFrame") {
        s.rules[0].keep = v;
    }
    for (is_border, pre) in [(false, "ParagraphShading"), (true, "ParagraphBorder")] {
        let b = if is_border { &mut s.border } else { &mut s.shading };
        let k = |suffix: &str| format!("{pre}{suffix}");
        if let Some(v) = p.bool(&k("On")) {
            b.on = v;
        }
        if let Some(v) = p.str(&k("Color")) {
            b.color = Some(v.to_owned());
        }
        if let Some(v) = p.f64(&k("Tint")) {
            b.tint = Some(v);
        }
        if let Some(v) = p.bool(&k("Overprint")) {
            b.overprint = v;
        }
        for (j, side) in ["Top", "Left", "Bottom", "Right"].into_iter().enumerate() {
            if let Some(v) = p.f64(&k(&format!("{side}Offset"))) {
                b.offsets[j] = v;
            }
            if let Some(v) = p.f64(&k(&format!("{side}LineWeight"))) {
                b.weights[j] = v;
            }
        }
        if let Some(v) = p.str(&k("TopOrigin")) {
            match origin_top(v) {
                Some(o) => b.top = o,
                None => w.add(format!("{pre} top origin {v} read as ascent")),
            }
        }
        if let Some(v) = p.str(&k("BottomOrigin")) {
            match origin_bottom(v) {
                Some(o) => b.bottom = o,
                None => w.add(format!("{pre} bottom origin {v} read as descent")),
            }
        }
        if let Some(v) = p.str(&k("Width")) {
            b.text_width = v == "TextWidth";
        }
        if let Some(v) = p.str(&k("Type")) {
            b.stroke_type = stroke_type_name(v);
        }
        if let Some(v) = p.str(&k("GapColor")) {
            b.gap_color = Some(v.to_owned());
        }
        if let Some(v) = p.f64(&k("GapTint")) {
            b.gap_tint = Some(v);
        }
        if let Some(v) = p.bool(&k("GapOverprint")) {
            b.gap_overprint = v;
        }
        if let Some(v) = p.bool(&k("DisplayIfSplits")) {
            b.display_if_splits = v;
        }
        if let Some(v) = p.bool(&k("ClipToFrame")) {
            b.clip = v;
        }
        if let Some(v) = p.bool(&k("SuppressPrinting")) {
            b.suppress = v;
        }
        for corner in ["TopLeft", "TopRight", "BottomLeft", "BottomRight"] {
            if let Some(v) = p.str(&k(&format!("{corner}CornerOption"))) {
                // Any corner of any layer set: the last layer decides per corner, but one
                // flag is enough for the warning.
                b.corners = v != "None";
            }
        }
    }
    if let Some(v) = p.bool("MergeConsecutiveParaBorders") {
        s.merge_borders = v;
    }
    let l = &mut s.list;
    if let Some(v) = p.str("BulletsAndNumberingListType") {
        l.kind = match v {
            "BulletList" => Some(ListKind::Bullet),
            "NumberedList" => Some(ListKind::Numbered),
            _ => None,
        };
    }
    if let Some(Val::Elem(e)) = p.get("BulletChar") {
        let value = e.attr("BulletCharacterValue").and_then(|v| v.parse::<u32>().ok());
        match e.attr("BulletCharacterType") {
            // A glyph of the bullet font chosen by its id: not a character the model can hold.
            Some("GlyphWithFont") => {
                w.add("bullet chosen by glyph id drawn as \u{2022}");
                l.bullet = "\u{2022}".into();
            }
            _ => {
                if let Some(c) = value.and_then(char::from_u32) {
                    l.bullet = c.to_string();
                }
            }
        }
    }
    if let Some(v) = p.str("BulletsTextAfter") {
        l.bullet_after = v.to_owned();
    }
    if let Some(v) = p.str("BulletsFont") {
        l.bullet_font = Some(v.to_owned());
    }
    if let Some(v) = p.str("BulletsFontStyle") {
        l.bullet_font_style = Some(v.to_owned());
    }
    if let Some(v) = p.str("BulletsCharacterStyle") {
        l.bullet_style = Some(v.to_owned());
    }
    if let Some(v) = p.str("NumberingCharacterStyle") {
        l.number_style = Some(v.to_owned());
    }
    if let Some(v) = p.str("NumberingFormat") {
        l.format = v.to_owned();
    }
    if let Some(v) = p.str("NumberingExpression") {
        l.expression = v.to_owned();
    }
    if let Some(v) = p.f64("NumberingStartAt") {
        l.start_at = u32_of(v);
    }
    if let Some(v) = p.bool("NumberingContinue") {
        l.continue_numbers = v;
    }
    if let Some(v) = p.f64("NumberingLevel") {
        l.level = u32_of(v);
    }
    if let Some(v) = p.str("AppliedNumberingList") {
        let name = clean_name(v.strip_prefix("NumberingList/").unwrap_or(v));
        l.list = if is_none_ref(v) || name == "[Default]" { String::new() } else { name.to_owned() };
    }
    if let Some(v) = p.bool("NumberingApplyRestartPolicy") {
        l.restart_policy = v;
    }
    if let Some(v) = p.str("BulletsAlignment") {
        l.bullet_align = align_of(v);
    }
    if let Some(v) = p.str("NumberingAlignment") {
        l.number_align = align_of(v);
    }
}

/// A resolved paragraph style: its name and the state after its whole chain.
pub struct ParaBase {
    pub name: String,
    pub para: ParaState,
    pub chr: CharState,
    pub grep: Arc<Vec<crate::grep::Compiled>>,
    pub nested: Arc<Vec<folio_model::nested::NestedStyle>>,
    pub line_styles: Arc<Vec<folio_model::nested::NestedLineStyle>>,
}

fn just(s: &str) -> Option<Justification> {
    Some(match s {
        "LeftAlign" => Justification::LeftAlign,
        "CenterAlign" => Justification::CenterAlign,
        "RightAlign" => Justification::RightAlign,
        "LeftJustified" => Justification::LeftJustified,
        "CenterJustified" => Justification::CenterJustified,
        "RightJustified" => Justification::RightJustified,
        "FullyJustified" => Justification::FullyJustified,
        "ToBindingSide" => Justification::ToBindingSide,
        "AwayFromBindingSide" => Justification::AwayFromBindingSide,
        _ => return None,
    })
}

fn u32_of(v: f64) -> u32 {
    v.round().clamp(0.0, u32::MAX as f64) as u32
}

fn tabs(v: Val<'_>) -> Vec<TabStop> {
    let Val::Elem(list) = v else { return Vec::new() };
    crate::tabstops::read(list)
}

pub fn apply_para<'a>(s: &mut ParaState, p: impl Props<'a>, w: &mut Warnings) {
    let a = &mut s.attrs;
    crate::paragraph_columns::read(&mut a.columns, &p);
    if let Some(v) = p.str("SingleWordJustification").and_then(just) {
        a.single_word_justification = v;
    }
    if let Some(v) = p.str("Justification") {
        match just(v) {
            Some(j) => a.justification = j,
            None => w.add(format!("justification {v} not recognised")),
        }
    }
    if let Some(v) = p.str("Composer") {
        a.composer = clean_name(v).to_owned();
    }
    if let Some(v) = p.str("BalanceRaggedLines") {
        a.balance_ragged_lines = v == "VeeShape";
    }
    match p.str("ParagraphDirection") {
        Some("RightToLeftDirection") => a.right_to_left = true,
        Some("LeftToRightDirection") => a.right_to_left = false,
        Some(v) => w.add(format!("paragraph direction {v} not recognised")),
        None => {}
    }
    let num = |k: &str, f: &mut f64| {
        if let Some(v) = p.f64(k) {
            *f = v;
        }
    };
    num("LeftIndent", &mut a.left_indent);
    num("RightIndent", &mut a.right_indent);
    num("FirstLineIndent", &mut a.first_line_indent);
    num("LastLineIndent", &mut a.last_line_indent);
    num("SpaceBefore", &mut a.space_before);
    num("SpaceAfter", &mut a.space_after);
    num("MinimumWordSpacing", &mut a.word_spacing.min);
    num("DesiredWordSpacing", &mut a.word_spacing.desired);
    num("MaximumWordSpacing", &mut a.word_spacing.max);
    num("MinimumLetterSpacing", &mut a.letter_spacing.min);
    num("DesiredLetterSpacing", &mut a.letter_spacing.desired);
    num("MaximumLetterSpacing", &mut a.letter_spacing.max);
    num("MinimumGlyphScaling", &mut a.glyph_scaling.min);
    num("DesiredGlyphScaling", &mut a.glyph_scaling.desired);
    num("MaximumGlyphScaling", &mut a.glyph_scaling.max);
    num("AutoLeading", &mut s.auto_leading);
    num("HyphenationZone", &mut a.hyphenation_zone);
    let int = |k: &str, f: &mut u32| {
        if let Some(v) = p.f64(k) {
            *f = u32_of(v);
        }
    };
    int("HyphenateWordsLongerThan", &mut a.hyphenate_words_longer_than);
    int("HyphenateAfterFirst", &mut a.hyphenate_after_first);
    int("HyphenateBeforeLast", &mut a.hyphenate_before_last);
    int("HyphenateLadderLimit", &mut a.hyphen_ladder_limit);
    int("HyphenWeight", &mut a.hyphen_weight);
    int("DropCapLines", &mut a.drop_cap_lines);
    int("DropCapCharacters", &mut a.drop_cap_characters);
    int("DropcapDetail", &mut a.drop_cap_detail);
    int("KeepWithNext", &mut a.keep_with_next);
    int("KeepFirstLines", &mut a.keep_first_lines);
    int("KeepLastLines", &mut a.keep_last_lines);
    let flag = |k: &str, f: &mut bool| {
        if let Some(v) = p.bool(k) {
            *f = v;
        }
    };
    flag("Hyphenation", &mut a.hyphenation);
    flag("IgnoreEdgeAlignment", &mut a.ignore_edge_alignment);
    flag("HyphenateLastWord", &mut a.hyphenate_last_word);
    flag("HyphenateAcrossColumns", &mut a.hyphenate_across_columns);
    flag("HyphenateCapitalizedWords", &mut a.hyphenate_capitalized);
    flag("KeepLinesTogether", &mut a.keep_lines_together);
    flag("KeepAllLinesTogether", &mut a.keep_all_lines);
    flag("AlignToBaseline", &mut a.align_to_baseline);
    flag("GridAlignFirstLineOnly", &mut a.grid_align_first_line_only);
    // vertical text: auto tate-chu-yoko digits and whether Roman letters count (`folio_text::vertical`)
    int("AutoTcy", &mut a.auto_tcy);
    flag("AutoTcyIncludeRoman", &mut a.auto_tcy_roman);
    if let Some(g) = p.str("GridAlignment") {
        a.align_to_baseline = g != "None";
    }
    if let Some(v) = p.get("TabList") {
        a.tabs = tabs(v);
    }
    if let Some(v) = p.str("StartParagraph") {
        a.start = folio_model::ParagraphStart::from_idml(v);
    }
    if let Some(v) = p.f64("SameParaStyleSpacing") {
        // Negative (−1, or −1 scaled with the text) is InDesign's "ignore".
        a.same_style_spacing = (v >= 0.0).then_some(v);
    }
    apply_structures(s, &p, w);
}

impl<'a> StyleSheet<'a> {
    /// The resolved paragraph style for an `AppliedParagraphStyle` reference.
    pub fn paragraph(&self, applied: Option<&str>, w: &mut Warnings) -> Arc<ParaBase> {
        let id = match applied {
            Some(a) if !is_none_ref(a) => qualify("ParagraphStyle", a),
            _ => ROOT_PARAGRAPH.to_owned(),
        };
        if let Some(b) = self.para_cache.lock().ok().and_then(|c| c.get(&id).cloned()) {
            return b;
        }
        let mut para = ParaState::new();
        let mut chr = CharState::new(CharacterAttrs::default());
        let mut layers: Vec<&StyleDef<'a>> = Vec::new();
        if let Some(root) = self.para.get(ROOT_PARAGRAPH) {
            layers.push(root);
        }
        layers.extend(Self::chain(&self.para, &id, ROOT_PARAGRAPH, w));
        let mut grep = Vec::new();
        let mut nested = Vec::new();
        let mut line_styles = Vec::new();
        for l in &layers {
            apply_para(&mut para, &l.props, w);
            apply_char(&mut chr, &l.props, w);
            if let Some(rules) = crate::grep::read(l.el) { grep = rules; }
            if let Some(items) = crate::nested::read(l.el) { nested = items; }
            if let Some(items) = crate::nested::read_lines(l.el) { line_styles = items; }
        }
        let name = self.para.get(id.as_str()).map(|d| d.name.clone()).unwrap_or_else(|| clean_name(&id).to_owned());
        let base = Arc::new(ParaBase { name, para, chr, grep: Arc::new(crate::grep::compiled(grep, w)), nested: Arc::new(nested), line_styles: Arc::new(line_styles) });
        if let Ok(mut c) = self.para_cache.lock() {
            c.insert(id, base.clone());
        }
        base
    }
}
