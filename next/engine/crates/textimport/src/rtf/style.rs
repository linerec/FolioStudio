//! RTF style sheets and their imported definitions (oracle docxtext-c4-rtf, rule docxtext-c4).
//!
//! One rule explains almost everything: InDesign reads the complete state of a style (its keywords on top of the defaults, never on
//! top of the style it is based on), of a paragraph (since its `\pard`) and of a run, and writes a property only where that state
//! differs from the state of its parent: a style from its base style (a root style from the defaults), a paragraph from its style,
//! a run from its paragraph style overlaid with the properties its character style writes.
use super::{CharState, ParaState, char_word, num, para_word};
use super::resources::{Resources, swatch_reference};
use super::tokens::Tok;
use crate::{StyleDef, SwatchDef};

pub(super) const NO_PARAGRAPH_STYLE: &str = "$ID/[No paragraph style]";

pub(super) struct Entry {
    pub id: i32,
    pub character: bool,
    pub name: String,
    pub based: Option<i32>,
    pub next: Option<i32>,
    pub chr: CharState,
    pub par: ParaState,
}

/// The styles of the sheet, and the ones the text used (their definitions, in the order they come into the document).
#[derive(Default)]
pub(super) struct Sheet {
    pub entries: Vec<Entry>,
    deff: i32,
    pub par_defs: Vec<(usize, StyleDef)>,
    pub chr_defs: Vec<(usize, StyleDef)>,
}

/// Which properties a character style writes (the others come from the paragraph style).
#[derive(Clone, Copy, Default)]
struct Mask {
    font_style: bool,
    size: bool,
    font: bool,
    color: bool,
    underline: bool,
    strike: bool,
    vert: bool,
    caps: bool,
}

/// The `{\stylesheet ...}` group, read the way the rest of the file's keywords are.
pub(super) fn parse(toks: &[Tok<'_>], deff: i32) -> Sheet {
    let mut sheet = Sheet { deff, ..Sheet::default() };
    let mut depth = 0i32;
    let mut sheet_depth: Option<i32> = None;
    let mut cur: Option<(Entry, i32)> = None;
    let mut prev_open = false;
    for tok in toks {
        match tok {
            Tok::Open => {
                depth += 1;
                prev_open = true;
                continue;
            }
            Tok::Close => {
                if cur.as_ref().is_some_and(|(_, d)| *d == depth) {
                    let (mut e, _) = cur.take().expect("the current style was just checked at this depth");
                    e.name = e.name.trim().trim_end_matches(';').trim().to_owned();
                    if e.id >= 0 {
                        sheet.entries.push(e);
                    }
                }
                if sheet_depth == Some(depth) {
                    sheet_depth = None;
                }
                depth -= 1;
            }
            Tok::Word(w, p) => {
                let name = std::str::from_utf8(w).unwrap_or("");
                if prev_open && name == "stylesheet" {
                    sheet_depth = Some(depth);
                } else if let Some(sd) = sheet_depth {
                    match &mut cur {
                        None if depth == sd + 1 => {
                            let kind = match name {
                                "s" => Some(false),
                                "cs" => Some(true),
                                _ => None,
                            };
                            let id = if kind.is_some() { p.unwrap_or(0) } else { -1 };
                            cur = Some((
                                Entry {
                                    id,
                                    character: kind.unwrap_or(false),
                                    name: String::new(),
                                    based: None,
                                    next: None,
                                    chr: CharState::new(deff),
                                    par: ParaState::new(),
                                },
                                depth,
                            ));
                        }
                        Some((e, d)) if *d == depth => match name {
                            "sbasedon" => e.based = *p,
                            "snext" => e.next = *p,
                            _ => {
                                let _ = char_word(&mut e.chr, name, *p, deff) || para_word(&mut e.par, name, *p);
                            }
                        },
                        _ => {}
                    }
                }
            }
            Tok::Text(t) => {
                if let Some((e, d)) = &mut cur
                    && *d == depth
                {
                    e.name.push_str(&String::from_utf8_lossy(t));
                }
            }
            _ => {}
        }
        prev_open = false;
    }
    sheet
}

fn font_style(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    }
}

fn justification(p: &ParaState) -> &'static str {
    p.jc.unwrap_or("LeftAlign")
}

/// AutoLeading in percent: Word's single line (240) is InDesign's default 120 (measured t08: 240 -> 120, 360 -> 150, 480 -> 200).
pub(super) fn auto_leading(p: &ParaState) -> f64 {
    match p.line {
        Some(l) if p.line_mult && l > 0 && l != 240 => f64::from(l) / 240.0 * 100.0,
        _ => 120.0,
    }
}

/// The fixed leading in points an exact or "at least" line spacing makes (a character property of InDesign; measured t08).
pub(super) fn fixed_leading(p: &ParaState) -> Option<f64> {
    match p.line {
        Some(l) if l < 0 || (l > 0 && !p.line_mult) => Some(f64::from(l.abs()) / 20.0),
        _ => None,
    }
}

fn indent(v: i32) -> f64 {
    f64::from(v) / 20.0
}

fn tab_list(tabs: &[(String, f64, &'static str)]) -> String {
    let items: String = tabs
        .iter()
        .map(|(align, p, leader)| format!("<ListItem type=\"record\"><Alignment type=\"enumeration\">{align}</Alignment><AlignmentCharacter type=\"string\">.</AlignmentCharacter><Leader type=\"string\">{leader}</Leader><Position type=\"unit\">{p}</Position></ListItem>"))
        .collect();
    format!("<TabList type=\"list\">{items}</TabList>")
}

/// The paragraph attributes where `own` differs from `base`, and the tab list when it differs.
pub(super) fn para_diff(own: &ParaState, base: &ParaState) -> (Vec<(String, String)>, Option<String>) {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut put = |key: &str, a: f64, b: f64| {
        if (a - b).abs() > 1e-9 {
            out.push((key.to_owned(), num(a)));
        }
    };
    put("LeftIndent", indent(own.left.max(0)), indent(base.left.max(0)));
    put("RightIndent", indent(own.right), indent(base.right));
    put("FirstLineIndent", indent(own.first), indent(base.first));
    put("AutoLeading", auto_leading(own), auto_leading(base));
    put("SpaceBefore", indent(own.before), indent(base.before));
    put("SpaceAfter", indent(own.after), indent(base.after));
    if own.keep != base.keep {
        out.push(("KeepAllLinesTogether".into(), own.keep.to_string()));
    }
    if own.keepn != base.keepn {
        out.push(("KeepWithNext".into(), if own.keepn { "1" } else { "0" }.into()));
    }
    if own.keep != base.keep {
        out.push(("KeepLinesTogether".into(), own.keep.to_string()));
    }
    if justification(own) != justification(base) {
        out.push(("Justification".into(), justification(own).to_owned()));
    }
    if own.pagebb != base.pagebb {
        out.push(("StartParagraph".into(), if own.pagebb { "NextPage" } else { "Anywhere" }.into()));
    }
    let tabs = (own.tabs != base.tabs).then(|| tab_list(&own.tabs));
    (out, tabs)
}

/// What building a character difference needs: the tables, and the swatches the colours it names add.
pub(super) struct Colors<'a> {
    pub(super) resources: &'a Resources,
    pub(super) swatches: &'a mut Vec<SwatchDef>,
}

impl Colors<'_> {
    /// The swatch name of colour `idx` of the colour table (black: none).
    fn key(&self, idx: usize) -> String {
        self.resources.swatch(idx).map(|(_, name)| name).unwrap_or_default()
    }

    /// `Color/<swatch>`; the swatch is added to the document's when it is new, in the colour group unless only a style's fill colour names it.
    pub(super) fn fill(&mut self, idx: usize, grouped: bool) -> String {
        let Some((rgb, name)) = self.resources.swatch(idx) else { return "Color/Black".to_owned() };
        match self.swatches.iter_mut().find(|s| s.name == name) {
            Some(s) => s.grouped |= grouped,
            None => self.swatches.push(SwatchDef { name: name.clone(), rgb, grouped }),
        }
        swatch_reference(&name)
    }
}

/// A character difference: attributes and `<Properties>` children.
pub(super) struct CharDiff {
    pub attrs: Vec<(String, String)>,
    pub props: Vec<(String, String, String)>,
}

/// The character attributes where `own` differs from `base`. `force_color` writes the fill colour even when it is black (a root
/// character style); `text_color` allows the "Text Color" underline colour that switching an underline off writes.
pub(super) fn char_diff(colors: &mut Colors<'_>, own: &CharState, base: &CharState, force_color: bool, text_color: bool, in_style: bool) -> CharDiff {
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut props: Vec<(String, String, String)> = Vec::new();
    let color_differs = colors.key(own.color) != colors.key(base.color);
    if color_differs || force_color {
        attrs.push(("FillColor".into(), colors.fill(own.color, !in_style)));
    }
    if (own.bold, own.italic) != (base.bold, base.italic) {
        attrs.push(("FontStyle".into(), font_style(own.bold, own.italic).into()));
    }
    if own.size != base.size {
        attrs.push(("PointSize".into(), num(f64::from(own.size) / 2.0)));
    }
    if own.caps != base.caps {
        attrs.push(("Capitalization".into(), ["Normal", "AllCaps", "SmallCaps"][usize::from(own.caps.min(2))].into()));
    }
    if own.underline != base.underline {
        attrs.push(("Underline".into(), own.underline.to_string()));
    }
    if own.vert != base.vert {
        attrs.push(("Position".into(), ["Normal", "Superscript", "Subscript"][usize::from(own.vert.min(2))].into()));
    }
    if own.strike != base.strike {
        attrs.push(("StrikeThru".into(), own.strike.to_string()));
    }
    if let Some(name) = colors.resources.font_name(own.font)
        && Some(name) != colors.resources.font_name(base.font)
    {
        props.push(("AppliedFont".into(), "string".into(), name.to_owned()));
    }
    if own.underline && (!base.underline || color_differs) {
        props.push(("UnderlineColor".into(), "object".into(), colors.fill(own.color, true)));
    } else if !own.underline && text_color {
        props.push(("UnderlineColor".into(), "string".into(), "Text Color".into()));
    }
    CharDiff { attrs, props }
}

impl Sheet {
    pub(super) fn is_styled(&self) -> bool {
        !self.entries.is_empty()
    }

    /// The entry of style number `id` (paragraph or character); Word's "Default Paragraph Font" is no style.
    pub(super) fn find(&self, id: i32, character: bool) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id && e.character == character && !(character && e.name == "Default Paragraph Font"))
    }

    /// The style `i` is based on: one that exists, is of its kind, and does not lead back to `i`.
    fn base_of(&self, i: usize) -> Option<usize> {
        let e = &self.entries[i];
        let b = self.find(e.based?, e.character)?;
        let mut at = Some(b);
        for _ in 0..self.entries.len() {
            let Some(a) = at else { return Some(b) };
            if a == i {
                return None;
            }
            at = self.entries[a].based.and_then(|id| self.find(id, e.character));
        }
        None
    }

    /// The properties a character style writes: those that differ from its base (from the defaults for a root style); the fill colour always.
    fn mask(&self, i: usize) -> Mask {
        let e = &self.entries[i];
        let base = self.base_of(i);
        let own = &e.chr;
        let from = base.map_or_else(|| CharState::new(self.deff), |b| self.entries[b].chr.clone());
        let mut m = base.map_or_else(Mask::default, |b| self.mask(b));
        m.font_style |= (own.bold, own.italic) != (from.bold, from.italic);
        m.size |= own.size != from.size;
        m.font |= own.font != from.font;
        m.color |= base.is_none() || own.color != from.color;
        m.underline |= own.underline != from.underline;
        m.strike |= own.strike != from.strike;
        m.vert |= own.vert != from.vert;
        m.caps |= own.caps != from.caps;
        m
    }

    /// The character state a run of paragraph style `par` and character style `cs` starts from.
    pub(super) fn run_base(&self, par: usize, cs: Option<usize>) -> CharState {
        let mut b = self.entries[par].chr.clone();
        if let Some(c) = cs {
            let (m, own) = (self.mask(c), &self.entries[c].chr);
            if m.font_style {
                (b.bold, b.italic) = (own.bold, own.italic);
            }
            if m.size {
                b.size = own.size;
            }
            if m.font {
                b.font = own.font;
            }
            if m.color {
                b.color = own.color;
            }
            if m.underline {
                b.underline = own.underline;
            }
            if m.strike {
                b.strike = own.strike;
            }
            if m.vert {
                b.vert = own.vert;
            }
            if m.caps {
                b.caps = own.caps;
            }
        }
        b
    }

    /// Brings style `i` and (before it) the styles it is based on into the used list; the name of the style.
    pub(super) fn register(&mut self, i: usize, colors: &mut Colors<'_>) -> String {
        let character = self.entries[i].character;
        let used = if character { &self.chr_defs } else { &self.par_defs };
        if used.iter().any(|(k, _)| *k == i) {
            return self.entries[i].name.clone();
        }
        if let Some(b) = self.base_of(i) {
            self.register(b, colors);
        }
        let def = self.definition(i, colors);
        let name = def.name.clone();
        if character { self.chr_defs.push((i, def)) } else { self.par_defs.push((i, def)) }
        name
    }

    fn definition(&self, i: usize, colors: &mut Colors<'_>) -> StyleDef {
        let e = &self.entries[i];
        let base = self.base_of(i);
        let root = base.is_none();
        let default = CharState::new(self.deff);
        let base_chr = base.map_or(&default, |b| &self.entries[b].chr);
        let CharDiff { mut attrs, mut props } = char_diff(colors, &e.chr, base_chr, root && e.character, base_chr.underline, true);
        let mut raw: Vec<(String, String, String)> = Vec::new();
        if e.character {
            if root {
                // a character style names all of these (measured t14, r03)
                let at = attrs.iter().position(|(k, _)| k == "Underline" || k == "Position" || k == "StrikeThru" || k == "Capitalization").unwrap_or(attrs.len());
                let extras = [("HorizontalScale", "100"), ("StrokeColor", "Swatch/None"), ("VerticalScale", "100")];
                for (n, (k, v)) in extras.iter().enumerate() {
                    attrs.insert(at + n, ((*k).to_owned(), (*v).to_owned()));
                }
                attrs.push(("FillTint".into(), "-1".into()));
                attrs.push(("CharacterDirection".into(), "DefaultDirection".into()));
            }
            if e.chr.underline && (root || !base_chr.underline) {
                props.push(("UnderlineType".into(), "object".into(), "StrokeStyle/$ID/Solid".into()));
            }
        } else {
            let default_par = ParaState::new();
            let base_par = base.map_or(&default_par, |b| &self.entries[b].par);
            if root {
                for (k, v) in [("HyphenateLadderLimit", "0"), ("Hyphenation", "false"), ("HyphenationZone", "0")] {
                    attrs.push((k.into(), v.into()));
                }
                // a root style names its font (the file's default font unless it says its own)
                if let Some(name) = colors.resources.font_name(e.chr.font)
                    && !props.iter().any(|(k, _, _)| k == "AppliedFont")
                {
                    props.insert(0, ("AppliedFont".into(), "string".into(), name.to_owned()));
                }
            }
            let (p_attrs, tabs) = para_diff(&e.par, base_par);
            attrs.extend(p_attrs);
            if fixed_leading(&e.par) != fixed_leading(base_par) && let Some(l) = fixed_leading(&e.par) {
                raw.push(("Leading".into(), "unit".into(), num(l)));
            }
            if let Some(t) = tabs {
                raw.push(("TabList".into(), "raw".into(), t));
            }
        }
        raw.extend(props);
        let based_on = base.map(|b| self.entries[b].name.clone());
        StyleDef { name: e.name.clone(), based_on, next: None, attrs, properties: raw }
    }

    /// The character styles that were brought in, in the order of the file's sheet.
    pub(super) fn finish_chars(&self) -> Vec<StyleDef> {
        let mut sorted: Vec<&(usize, StyleDef)> = self.chr_defs.iter().collect();
        sorted.sort_by_key(|(i, _)| *i);
        sorted.into_iter().map(|(_, d)| d.clone()).collect()
    }

    /// Sets the next style of every paragraph style that was brought in: the style `\snext` names when it came in too, else none.
    pub(super) fn finish_next(&mut self) -> Vec<StyleDef> {
        let names: Vec<(usize, String)> = self.par_defs.iter().map(|(i, d)| (*i, d.name.clone())).collect();
        let mut out = Vec::new();
        // the styles come into the document in the order of the file's sheet, not of their use (measured: t11, t15)
        let mut sorted: Vec<&(usize, StyleDef)> = self.par_defs.iter().collect();
        sorted.sort_by_key(|(i, _)| *i);
        for (i, def) in sorted {
            let next = self.entries[*i].next.and_then(|id| self.find(id, false)).and_then(|n| names.iter().find(|(k, _)| *k == n)).map(|(_, name)| name.clone());
            let mut d = def.clone();
            d.next = Some(next.unwrap_or_else(|| NO_PARAGRAPH_STYLE.to_owned()));
            out.push(d);
        }
        out
    }
}

/// Brings style `i` (and the styles it is based on) into the used list; its name.
pub(super) fn register_style(sheet: &mut Sheet, swatches: &mut Vec<SwatchDef>, resources: &Resources, i: usize) -> String {
    let mut colors = Colors { resources, swatches };
    sheet.register(i, &mut colors)
}
