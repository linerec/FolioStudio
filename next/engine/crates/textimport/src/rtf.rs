//! RTF text import, as measured by the placetext-c2 cases.
//!
//! What was measured (InDesign 2025, `wordRTFImportPreferences` at its defaults), for a file without a style sheet in use:
//! - Every paragraph is `[Normal Paragraph Style]` with the overrides `HyphenateLadderLimit=0 Hyphenation=false HyphenationZone=0`
//!   and its own settings (alignment, indents, spacing, tab stops); a paragraph's settings stay until `\pard`. The last `\par` of the
//!   file is no paragraph return (it ends the file's last paragraph); one more makes an empty paragraph.
//! - Every range of text names its font (`AppliedFont`, the font table's entry, `\deff` when the text says none): fonts are imported here,
//!   unlike in a .docx. A range ends where the formatting changes; the return takes the formatting of the point it stands at.
//! - Bold and italic (`FontStyle`), size (`PointSize`, only when not the default 12 pt), underline (with `UnderlineColor` Black), strikethrough,
//!   super- and subscript. A colour of the colour table is a new swatch: RGB 255 0 0 is `Red`, 0 0 255 `Blue`, and so on;
//!   black is no colour. `\line` is a forced line break, `\page` a return that carries `ParagraphBreakType`.
//!
//! A file with a style sheet (`\s1`, `\cs1`): its styles come in as styles (`style.rs`); paragraphs and runs carry only what differs from them.
use crate::messages::{Message, msg};
use crate::{Inline, Options, PageBreaks, Paragraph, Run, SwatchDef, TextSource, quotes};

mod grid;
mod list;
mod resources;
mod style;
mod tokens;

use resources::Resources;
use tokens::Tok;

/// Destinations whose content is not text of the story.
const SKIPPED: [&str; 26] = [
    "fonttbl", "colortbl", "stylesheet", "info", "header", "footer", "headerl", "headerr", "headerf", "footerl", "footerr", "footerf", "listtable",
    "listoverridetable", "rsidtbl", "generator", "xmlnstbl", "latentstyles", "themedata", "colorschememapping", "datastore", "mmathPr", "private", "revtbl", "listtext", "pntext",
];

#[derive(Clone, PartialEq)]
struct CharState {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    /// 0 none, 1 superscript, 2 subscript.
    vert: u8,
    /// Half points.
    size: i32,
    font: i32,
    color: usize,
    /// 0 none, 1 all caps, 2 small caps.
    caps: u8,
    /// The character style (`\cs`), 0 none.
    cs: i32,
}

impl CharState {
    fn new(deff: i32) -> Self {
        Self { bold: false, italic: false, underline: false, strike: false, vert: 0, size: 24, font: deff, color: 0, caps: 0, cs: 0 }
    }
}

#[derive(Clone, Default)]
struct ParaState {
    jc: Option<&'static str>,
    left: i32,
    right: i32,
    first: i32,
    before: i32,
    after: i32,
    line: Option<i32>,
    line_mult: bool,
    /// (alignment, position in points, leader)
    tabs: Vec<(String, f64, &'static str)>,
    tab_align: &'static str,
    tab_leader: &'static str,
    keep: bool,
    keepn: bool,
    pagebb: bool,
    /// The list the paragraph is in (`\ls`) and its level (`\ilvl`).
    ls: Option<i32>,
    ilvl: i32,
    /// The paragraph's borders (`\brdrl`, `\brdrt`, `\brdrr`, `\brdrb`, `\box`) as left, top, right, bottom, and the sides whose
    /// border keywords are being read.
    borders: [Option<ParaBorder>; 4],
    border_sides: Vec<usize>,
}

/// One side of a paragraph border: the weight (`\brdrw`, twips; none said: half a point), the colour (`\brdrcf`), the line kind and the
/// space between the border and the text (`\brsp`, twips).
#[derive(Clone, Default, PartialEq)]
struct ParaBorder {
    weight: Option<i32>,
    color: usize,
    kind: &'static str,
    space: i32,
}

impl ParaState {
    fn new() -> Self {
        Self { tab_align: "LeftAlign", ..Self::default() }
    }
}

/// The InDesign stroke style of an RTF border line kind (the same names the cell borders use).
fn border_kind(name: &str) -> Option<&'static str> {
    Some(match name {
        "brdrs" | "brdrth" | "brdrsh" | "brdrhair" | "brdroutset" | "brdrinset" | "brdremboss" | "brdrengrave" => "StrokeStyle/$ID/Solid",
        "brdrdb" => "StrokeStyle/$ID/ThinThin",
        "brdrtriple" => "StrokeStyle/$ID/Triple_Stroke",
        "brdrdot" => "StrokeStyle/$ID/Canned Dotted",
        "brdrdash" | "brdrdashsm" | "brdrdashd" | "brdrdashdd" | "brdrdashdotstr" => "StrokeStyle/$ID/Canned Dashed 3x2",
        "brdrwavy" | "brdrwavydb" => "StrokeStyle/$ID/Wavy",
        "brdrthtnsg" | "brdrthtnmg" | "brdrthtnlg" => "StrokeStyle/$ID/ThinThick",
        "brdrtnthsg" | "brdrtnthmg" | "brdrtnthlg" => "StrokeStyle/$ID/ThickThin",
        _ => return None,
    })
}

/// A character keyword: changes `c` and says it was one.
fn char_word(c: &mut CharState, name: &str, p: Option<i32>, deff: i32) -> bool {
    let on = p != Some(0);
    match name {
        "plain" => *c = CharState::new(deff),
        "b" => c.bold = on,
        "i" => c.italic = on,
        "ul" | "uld" | "uldb" | "ulw" | "ulth" | "ulwave" | "uldash" => c.underline = on,
        "ulnone" => c.underline = false,
        "strike" => c.strike = on,
        "super" => c.vert = 1,
        "sub" => c.vert = 2,
        "nosupersub" => c.vert = 0,
        "fs" => c.size = p.unwrap_or(24),
        "f" => c.font = p.unwrap_or(0),
        "cf" => c.color = p.unwrap_or(0).max(0) as usize,
        "caps" => c.caps = if on { 1 } else { 0 },
        "scaps" => c.caps = if on { 2 } else { 0 },
        _ => return false,
    }
    true
}

/// A paragraph keyword: changes `para` and says it was one.
fn para_word(para: &mut ParaState, name: &str, p: Option<i32>) -> bool {
    let on = p != Some(0);
    match name {
        "pard" => *para = ParaState::new(),
        "ql" => para.jc = None,
        "qc" => para.jc = Some("CenterAlign"),
        "qr" => para.jc = Some("RightAlign"),
        "qj" => para.jc = Some("LeftJustified"),
        "li" => para.left = p.unwrap_or(0),
        "ri" => para.right = p.unwrap_or(0),
        "fi" => para.first = p.unwrap_or(0),
        "sb" => para.before = p.unwrap_or(0),
        "sa" => para.after = p.unwrap_or(0),
        "sl" => para.line = p,
        "slmult" => para.line_mult = p == Some(1),
        "keep" => para.keep = on,
        "keepn" => para.keepn = on,
        "pagebb" => para.pagebb = on,
        "ls" => para.ls = p.filter(|n| *n > 0),
        "ilvl" => para.ilvl = p.unwrap_or(0).max(0),
        "tqr" => para.tab_align = "RightAlign",
        "tqc" => para.tab_align = "CenterAlign",
        "tqdec" => para.tab_align = "CharacterAlign",
        "tldot" => para.tab_leader = ".",
        "tx" => {
            para.tabs.push((para.tab_align.to_owned(), f64::from(p.unwrap_or(0)) / 20.0, para.tab_leader));
            para.tab_align = "LeftAlign";
            para.tab_leader = "";
        }
        "brdrl" | "brdrt" | "brdrr" | "brdrb" | "box" => {
            para.border_sides = match name {
                "brdrl" => vec![0],
                "brdrt" => vec![1],
                "brdrr" => vec![2],
                "brdrb" => vec![3],
                _ => vec![0, 1, 2, 3],
            };
            for &s in &para.border_sides {
                para.borders[s] = Some(ParaBorder { kind: "StrokeStyle/$ID/Solid", ..ParaBorder::default() });
            }
        }
        // the border keywords that follow belong to a cell or the table (the table reader takes them)
        "clbrdrt" | "clbrdrl" | "clbrdrb" | "clbrdrr" | "trbrdrt" | "trbrdrb" | "trbrdrl" | "trbrdrr" | "trbrdrh" | "trbrdrv" => {
            para.border_sides.clear();
            return false;
        }
        _ if !para.border_sides.is_empty() && (name.starts_with("brdr") || name == "brsp") => {
            for &s in &para.border_sides.clone() {
                let border = &mut para.borders[s];
                match (name, border.as_mut()) {
                    ("brdrnone", _) => *border = None,
                    ("brdrw", Some(b)) => b.weight = p,
                    ("brdrcf", Some(b)) => b.color = p.unwrap_or(0).max(0) as usize,
                    ("brsp", Some(b)) => b.space = p.unwrap_or(0).max(0),
                    (_, Some(b)) => b.kind = border_kind(name).unwrap_or(b.kind),
                    _ => {}
                }
            }
        }
        _ => return false,
    }
    true
}

struct Open {
    chars: CharState,
    skip: bool,
}

fn codepage(n: i32) -> &'static encoding_rs::Encoding {
    match n {
        949 => encoding_rs::EUC_KR,
        932 => encoding_rs::SHIFT_JIS,
        936 => encoding_rs::GBK,
        950 => encoding_rs::BIG5,
        1250 => encoding_rs::WINDOWS_1250,
        1251 => encoding_rs::WINDOWS_1251,
        1253 => encoding_rs::WINDOWS_1253,
        1254 => encoding_rs::WINDOWS_1254,
        1255 => encoding_rs::WINDOWS_1255,
        1256 => encoding_rs::WINDOWS_1256,
        1257 => encoding_rs::WINDOWS_1257,
        1258 => encoding_rs::WINDOWS_1258,
        _ => encoding_rs::WINDOWS_1252,
    }
}

fn num(v: f64) -> String {
    format!("{v}")
}

struct Reader<'a> {
    resources: &'a Resources,
    default_font: i32,
    options: &'a Options,
    notes: Vec<Message>,
    swatches: Vec<SwatchDef>,
    paragraphs: Vec<Paragraph>,
    /// The paragraph being read: its text so far as (formatting, text).
    runs: Vec<(CharState, String)>,
    /// The file's style sheet and the styles the text used.
    sheet: style::Sheet,
    /// The table being read (its rows so far) and the lists of the file.
    grid: Option<grid::Grid>,
    lists: list::Lists,
}

impl Reader<'_> {
    fn note(&mut self, text: Message) {
        if !self.notes.iter().any(|n| n.message == text.message) {
            self.notes.push(text);
        }
    }

    /// A finished paragraph goes into the cell being read, or into the text (after the table that came before it).
    fn emit(&mut self, p: Paragraph) {
        if let Some(g) = self.grid.as_mut()
            && g.in_row()
        {
            g.cell.push(p);
            return;
        }
        self.flush_grid();
        self.paragraphs.push(p);
    }

    /// The table that the rows read so far make: a paragraph of its own (measured: docxtext-c5-rtf-w01, w02, w11, w13b).
    fn flush_grid(&mut self) {
        let Some(g) = self.grid.take() else { return };
        if g.rows.is_empty() {
            return;
        }
        for note in &g.notes {
            self.note(note.clone());
        }
        let mut colors = style::Colors { resources: self.resources, swatches: &mut self.swatches };
        let built = grid::build(g.rows, |idx| (idx > 0).then(|| colors.fill(idx, true)), Paragraph::default);
        // the table character has the formatting of the character before it; the return after it, that at the end of the last row
        let before = self.paragraphs.last().and_then(|p| p.end.as_ref().or(p.runs.last())).map(|r| (r.char_style.clone(), r.attrs.clone(), r.props.clone()));
        let (char_style, attrs, props) = before.unwrap_or_default();
        let run = Run { text: "\u{16}".into(), char_style, attrs, props, inline: Some(Inline::Table(built)), note: Vec::new() };
        let end = g.end.unwrap_or_else(|| CharState::new(self.default_font));
        let end_run = self.run(&end, "");
        let properties = self.lists.plain();
        let hyphen = vec![("HyphenateLadderLimit".into(), "0".into()), ("Hyphenation".into(), "false".into()), ("HyphenationZone".into(), "0".into())];
        // (the first paragraph of the text keeps the story's own style: measured w12 against w19)
        let style = (self.sheet.is_styled() && !self.paragraphs.is_empty()).then(|| style::NO_PARAGRAPH_STYLE.to_owned());
        self.paragraphs.push(Paragraph { style, runs: vec![run], attrs: hyphen, end: Some(end_run), properties, ..Paragraph::default() });
    }

    /// The end of a cell: its last paragraph is finished and the cell is complete.
    fn cell_end(&mut self, para: &ParaState, par_style: i32, end: &CharState) {
        self.finish(para, par_style, end, None);
        if let Some(g) = self.grid.as_mut() {
            let cell = std::mem::take(&mut g.cell);
            g.cells.push(cell);
        }
    }

    /// The end of a row: the formatting there is that of the return after the table.
    fn row_end(&mut self, end: &CharState) {
        self.runs.clear();
        if let Some(g) = self.grid.as_mut() {
            g.row_end();
            g.end = Some(end.clone());
        }
    }

    /// The list marking of a paragraph: its attributes and the children of its `<Properties>`; a paragraph in no list carries the restart policy
    /// the list paragraphs before it wrote.
    fn list_mark(&mut self, para: &ParaState) -> (Vec<(String, String)>, String) {
        if !self.options.remove_formatting
            && let Some(ls) = para.ls
            && let level = para.ilvl.max(0) as usize
            && self.lists.has(ls, level)
            && let Some(m) = self.lists.mark(ls, level)
        {
            return (m.attrs, m.properties);
        }
        (Vec::new(), self.lists.plain())
    }

    /// The paragraph border (`\brdrt` … `\box`) as InDesign's paragraph border: one colour and line kind (those of the first side that
    /// has a border), a weight and an offset (`\brsp`) per side; a side without a border has weight 0.
    fn border_attrs(&mut self, para: &ParaState) -> Vec<(String, String)> {
        if self.options.remove_formatting {
            return Vec::new();
        }
        let Some(first) = para.borders.iter().flatten().next().cloned() else { return Vec::new() };
        let color = if first.color > 0 { style::Colors { resources: self.resources, swatches: &mut self.swatches }.fill(first.color, true) } else { "Color/Black".to_owned() };
        let mut out = vec![
            ("ParagraphBorderOn".to_owned(), "true".to_owned()),
            ("ParagraphBorderColor".to_owned(), color),
            ("ParagraphBorderType".to_owned(), first.kind.to_owned()),
        ];
        for (side, name) in [(1, "Top"), (0, "Left"), (3, "Bottom"), (2, "Right")] {
            let border = para.borders[side].as_ref();
            let weight = border.map_or(0, |b| b.weight.unwrap_or(10).max(0));
            out.push((format!("ParagraphBorder{name}LineWeight"), num(f64::from(weight) / 20.0)));
            out.push((format!("ParagraphBorder{name}Offset"), num(f64::from(border.map_or(0, |b| b.space)) / 20.0)));
        }
        out
    }

    fn push(&mut self, state: &CharState, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.runs.last_mut() {
            Some((s, t)) if s == state => t.push_str(text),
            _ => self.runs.push((state.clone(), text.to_owned())),
        }
    }

    /// The run of a stretch of text with this formatting.
    fn run(&mut self, state: &CharState, text: &str) -> Run {
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut props: Vec<(String, String, String)> = Vec::new();
        if let Some(name) = self.resources.font_name(state.font) {
            props.push(("AppliedFont".into(), "string".into(), name.to_owned()));
        }
        let font_style = match (state.bold, state.italic) {
            (true, true) => Some("Bold Italic"),
            (true, false) => Some("Bold"),
            (false, true) => Some("Italic"),
            (false, false) => None,
        };
        attrs.extend(font_style.map(|f| ("FontStyle".to_owned(), f.to_owned())));
        if state.size != 24 {
            attrs.push(("PointSize".into(), num(f64::from(state.size) / 2.0)));
        }
        if let Some((rgb, name)) = self.resources.swatch(state.color) {
            if !self.swatches.iter().any(|s| s.name == name) {
                self.swatches.push(SwatchDef { name: name.clone(), rgb, grouped: true });
            }
            attrs.push(("FillColor".into(), resources::swatch_reference(&name)));
        }
        if state.underline {
            attrs.push(("Underline".into(), "true".into()));
            props.push(("UnderlineColor".into(), "object".into(), "Color/Black".into()));
        }
        if state.strike {
            attrs.push(("StrikeThru".into(), "true".into()));
        }
        match state.vert {
            1 => attrs.push(("Position".into(), "Superscript".into())),
            2 => attrs.push(("Position".into(), "Subscript".into())),
            _ => {}
        }
        Run { text: text.to_owned(), char_style: None, attrs, props, inline: None, note: Vec::new() }
    }

    fn finish(&mut self, para: &ParaState, par_style: i32, end: &CharState, break_type: Option<&str>) {
        let runs = std::mem::take(&mut self.runs);
        if self.sheet.is_styled()
            && let Some(pi) = self.sheet.find(par_style, false)
        {
            self.finish_styled(para, pi, &runs, end, break_type);
            return;
        }
        let mut runs: Vec<Run> = runs.into_iter().map(|(s, t)| self.run(&s, &t)).collect();
        let mut attrs: Vec<(String, String)> = vec![("HyphenateLadderLimit".into(), "0".into()), ("Hyphenation".into(), "false".into()), ("HyphenationZone".into(), "0".into())];
        let twips = |v: i32| num(f64::from(v) / 20.0);
        for (key, v) in [("LeftIndent", para.left), ("RightIndent", para.right), ("FirstLineIndent", para.first), ("SpaceBefore", para.before), ("SpaceAfter", para.after)] {
            if v != 0 {
                attrs.push((key.into(), twips(v)));
            }
        }
        let auto = style::auto_leading(para);
        if auto != 120.0 {
            attrs.push(("AutoLeading".into(), num(auto)));
        }
        attrs.extend(para.jc.map(|j| ("Justification".to_owned(), j.to_owned())));
        let (mark_attrs, properties) = self.list_mark(para);
        attrs.extend(mark_attrs);
        attrs.extend(self.border_attrs(para));
        let mut end_run = self.run(end, "");
        if let Some(l) = style::fixed_leading(para) {
            for r in runs.iter_mut().chain(std::iter::once(&mut end_run)) {
                r.props.push(("Leading".into(), "unit".into(), num(l)));
            }
        }
        let paragraph = Paragraph {
            style: self.sheet.is_styled().then(|| style::NO_PARAGRAPH_STYLE.to_owned()),
            runs,
            attrs,
            tabs: para.tabs.iter().map(|(a, p, _)| (a.clone(), *p)).collect(),
            break_type: break_type.map(str::to_owned),
            end: Some(end_run),
            properties,
        };
        self.emit(paragraph);
    }

    /// A paragraph of a style of the sheet: only what differs from the style (oracle `docxtext-c4-rtf*`).
    fn finish_styled(&mut self, para: &ParaState, pi: usize, runs: &[(CharState, String)], end: &CharState, break_type: Option<&str>) {
        let name = style::register_style(&mut self.sheet, &mut self.swatches, self.resources, pi);
        let style_par = self.sheet.entries[pi].par.clone();
        let (mut attrs, tabs) = style::para_diff(para, &style_par);
        // the tab list, whole, when it differs from the style's (an empty one says the paragraph has none)
        let (mark_attrs, mark_props) = self.list_mark(para);
        attrs.extend(mark_attrs);
        // (a style's definition does not carry its border: the paragraph does)
        attrs.extend(self.border_attrs(para));
        let properties = tabs.unwrap_or_default() + &mark_props;
        let tab_stops = Vec::new();
        // an exact or "at least" line spacing is a character property; a style with a fixed leading is never overridden back to auto (measured t06)
        let lead = style::fixed_leading(para).filter(|l| style::fixed_leading(&style_par) != Some(*l));
        let mut prev_ul = false;
        let mut out: Vec<Run> = Vec::new();
        for (k, (state, text)) in runs.iter().enumerate() {
            out.push(self.run_styled(state, text, pi, k == 0, &mut prev_ul, lead));
        }
        let end_run = self.run_styled(end, "", pi, true, &mut prev_ul, lead);
        let paragraph = Paragraph { style: Some(name), runs: out, attrs, tabs: tab_stops, break_type: break_type.map(str::to_owned), end: Some(end_run), properties };
        self.emit(paragraph);
    }

    fn run_styled(&mut self, state: &CharState, text: &str, pi: usize, first: bool, prev_ul: &mut bool, lead: Option<f64>) -> Run {
        let cs = self.sheet.find(state.cs, true);
        let cs_name = cs.map(|c| style::register_style(&mut self.sheet, &mut self.swatches, self.resources, c));
        let base = self.sheet.run_base(pi, cs);
        // the underline colour "Text Color" shows up on a range after the first of its paragraph when an underline is switched off (measured t09, t14)
        let text_color = !first && (base.underline || *prev_ul);
        *prev_ul = base.underline;
        let mut colors = style::Colors { resources: self.resources, swatches: &mut self.swatches };
        let d = style::char_diff(&mut colors, state, &base, false, text_color, false);
        let mut props = d.props;
        if let Some(l) = lead {
            props.push(("Leading".into(), "unit".into(), num(l)));
        }
        Run { text: text.to_owned(), char_style: cs_name, attrs: d.attrs, props, inline: None, note: Vec::new() }
    }
}

pub(crate) fn read(bytes: &[u8], options: &Options) -> Result<TextSource, String> {
    let toks = tokens::tokenize(bytes);
    let resources = resources::read(&toks);
    let mut notes_only = Vec::new();
    let mut reader = Reader { resources: &resources, default_font: 0, options, notes: Vec::new(), swatches: Vec::new(), paragraphs: Vec::new(), runs: Vec::new(), sheet: style::Sheet::default(), grid: None, lists: list::Lists::default() };
    let mut cp = 1252;
    let mut uc = 1usize;
    let mut skip = 0usize;
    let mut high: Option<u32> = None;
    let mut pending: Vec<u8> = Vec::new();
    // \deff may come after the first group opens: read it first
    for t in &toks {
        match t {
            Tok::Word(b"deff", Some(n)) => reader.default_font = *n,
            Tok::Word(b"ansicpg", Some(n)) => cp = *n,
            Tok::Word(b"uc", Some(n)) => uc = (*n).max(0) as usize,
            _ => {}
        }
    }
    reader.sheet = style::parse(&toks, reader.default_font);
    reader.lists = list::parse(&toks);
    let deff = reader.default_font;
    let mut chars = CharState::new(deff);
    let mut para = ParaState::new();
    let mut par_style = 0i32;
    let mut stack: Vec<Open> = Vec::new();
    let mut skipping = false;
    let mut first_in_group = false;
    let mut text = String::new();
    macro_rules! flush_bytes {
        () => {
            if !pending.is_empty() {
                let (s, _, _) = codepage(cp).decode(&pending);
                text.push_str(&s);
                pending.clear();
            }
        };
    }
    macro_rules! flush_text {
        () => {
            flush_bytes!();
            if !text.is_empty() && !skipping {
                reader.push(&chars, &text);
            }
            text.clear();
        };
    }
    for tok in &toks {
        let opening_word = first_in_group;
        first_in_group = false;
        match tok {
            Tok::Open => {
                flush_text!();
                stack.push(Open { chars: chars.clone(), skip: skipping });
                first_in_group = true;
            }
            Tok::Close => {
                flush_text!();
                if let Some(o) = stack.pop() {
                    chars = o.chars;
                    skipping = o.skip;
                }
            }
            _ if skipping => {}
            Tok::Symbol(b'*') if opening_word => skipping = true,
            Tok::Word(w, p) => {
                let name = std::str::from_utf8(w).unwrap_or("");
                if opening_word && SKIPPED.contains(&name) {
                    skipping = true;
                    continue;
                }
                if opening_word && matches!(name, "pict" | "object" | "shppict" | "nonshppict" | "shp") {
                    skipping = true;
                    reader.note(msg("textimport.graphics", "그림·도형은 아직 가져오지 않습니다", serde_json::json!({})));
                    continue;
                }
                if opening_word && matches!(name, "footnote" | "endnote") {
                    skipping = true;
                    reader.note(msg("textimport.notes", "각주·미주는 아직 가져오지 않습니다", serde_json::json!({})));
                    continue;
                }
                flush_text!();
                if char_word(&mut chars, name, *p, deff) {
                    continue;
                }
                if para_word(&mut para, name, *p) {
                    if name == "pard" {
                        par_style = 0;
                    }
                    continue;
                }
                if reader.grid.as_mut().is_some_and(|g| g.word(name, *p)) {
                    continue;
                }
                match name {
                    "s" => par_style = p.unwrap_or(0),
                    "cs" => chars.cs = p.unwrap_or(0),
                    "tb" | "tlhyph" | "tlul" | "tlth" | "tleq" => reader.note(msg("textimport.rtfTabs", "소수점 탭·막대 탭과 채움 문자가 있는 탭은 아직 가져오지 않습니다", serde_json::json!({}))),
                    "par" => reader.finish(&para, par_style, &chars, None),
                    "line" => text.push('\u{2028}'),
                    "tab" => text.push('\t'),
                    "page" | "column" => {
                        let kind = match (name, reader.options.convert_page_breaks) {
                            (_, PageBreaks::None) => continue,
                            ("page", PageBreaks::PageBreak) => "NextPage",
                            _ => "NextColumn",
                        };
                        reader.finish(&para, par_style, &chars, Some(kind));
                    }
                    "emdash" => text.push('\u{2014}'),
                    "endash" => text.push('\u{2013}'),
                    "emspace" => text.push('\u{2003}'),
                    "enspace" => text.push('\u{2002}'),
                    "bullet" => text.push('\u{2022}'),
                    "lquote" => text.push('\u{2018}'),
                    "rquote" => text.push('\u{2019}'),
                    "ldblquote" => text.push('\u{201C}'),
                    "rdblquote" => text.push('\u{201D}'),
                    "u" => {
                        let n = p.unwrap_or(0);
                        let unit = if n < 0 { (n + 65536) as u32 } else { n as u32 };
                        // characters beyond the first plane come as two UTF-16 halves
                        match (char::from_u32(unit), high.take()) {
                            (Some(c), _) => text.push(c),
                            (None, Some(h)) if (0xDC00..0xE000).contains(&unit) => text.extend(char::from_u32(0x10000 + ((h - 0xD800) << 10) + (unit - 0xDC00))),
                            (None, _) if (0xD800..0xDC00).contains(&unit) => high = Some(unit),
                            (None, _) => reader.note(msg("textimport.surrogate", "짝이 맞지 않는 유니코드 글자(서로게이트)는 가져오지 않습니다", serde_json::json!({}))),
                        }
                        skip = uc;
                    }
                    // not text, not formatting InDesign carries (measured or irrelevant): the header of the file, the language, the section
                    "rtf" | "ansi" | "mac" | "pc" | "pca" | "deff" | "deflang" | "deflangfe" | "ansicpg" | "uc" | "lang" | "langfe" | "langnp" | "langfenp" | "noproof" | "sectd" | "sect"
                    | "paperw" | "paperh" | "margl" | "margr" | "margt" | "margb" | "viewkind" | "viewscale" | "fcharset" | "fnil" | "fswiss" | "froman" | "fmodern" | "fscript"
                    | "fdecor" | "ftech" | "fbidi" | "fprq" | "cpg" | "af" | "afs" | "ab" | "ai" | "loch" | "hich" | "dbch" | "rtlch" | "ltrch" | "ltrpar" | "ltrsect" | "nowidctlpar"
                    | "widctlpar" | "adjustright" | "faauto" | "fet" | "ftnbj" | "aenddoc" | "formshade" | "rsid" | "insrsid" | "charrsid" | "pararsid"
                    | "kerning" | "lin" | "rin" | "itap" | "snext" | "sbasedon" | "ssemihidden" | "sqformat" | "spriority" | "slink" | "sunhideused" | "viewbksp" => {}
                    "expnd" | "expndtw" | "highlight" | "cb" | "chcbpat" | "charscalex" | "up" | "dn" | "outl" | "shad" | "embo" | "impr" | "v" => {
                        reader.note(msg("textimport.rtfCharacterFormat", format!("글자 서식 \\{name} 은(는) 아직 가져오지 않습니다"), serde_json::json!({"name": name})));
                    }
                    "trowd" => reader.grid.get_or_insert_with(grid::Grid::default).row_start(),
                    "cell" if reader.grid.as_ref().is_some_and(grid::Grid::in_row) => reader.cell_end(&para, par_style, &chars),
                    "row" if reader.grid.as_ref().is_some_and(grid::Grid::in_row) => reader.row_end(&chars),
                    "intbl" | "nestcell" | "nestrow" | "nesttableprops" | "nonesttables" => {}
                    // border keywords with no side to go to (a paragraph border is read above, a cell's or the table's by the table reader)
                    "brdrs" | "brdrth" | "brdrdb" | "brdrdot" | "brdrdash" | "brdrw" | "brdrcf" | "brdrnone" | "brdrsh" | "brdrtbl" | "brdrframe" | "clbrdrt" | "clbrdrb" | "clbrdrl" | "clbrdrr"
                    | "brsp" | "brdrdashsm" | "brdrdashd" | "brdrdashdd" | "brdrtriple" | "brdrwavy" | "brdrwavydb" | "brdrinset" | "brdroutset" | "brdrhair" | "brdrbtw" | "brdrbar"
                    | "brdremboss" | "brdrengrave" | "brdrthtnsg" | "brdrtnthsg" | "brdrthtnmg" | "brdrtnthmg" | "brdrthtnlg" | "brdrtnthlg" | "brdrdashdotstr" | "brdrdashcontr"
                    | "trbrdrt" | "trbrdrb" | "trbrdrl" | "trbrdrr" | "trbrdrh" | "trbrdrv" => {}
                    "pnlvl" | "pn" | "pnlvlbody" | "pnlvlblt" | "pnlvlcont" => reader.note(msg("textimport.oldNumbering", "이전 방식의 문단 번호(\\pn)는 목록이 되지 않습니다 (번호 글자는 빠지고 들여쓰기만 남습니다)", serde_json::json!({}))),
                    other => reader.note(msg("textimport.rtfDirective", format!("RTF 지시어 \\{other} 은(는) 아직 가져오지 않습니다"), serde_json::json!({"other": other}))),
                }
            }
            Tok::Symbol(c) => match c {
                b'\\' | b'{' | b'}' => {
                    if skip > 0 {
                        skip -= 1;
                    } else {
                        pending.push(*c);
                    }
                }
                b'~' => text.push('\u{A0}'),
                b'-' => text.push('\u{AD}'),
                b'_' => text.push('\u{2011}'),
                _ => {}
            },
            Tok::Hex(h) => {
                if skip > 0 {
                    skip -= 1;
                } else {
                    pending.push(*h);
                }
            }
            Tok::Text(t) => {
                let mut t = *t;
                if skip > 0 {
                    let n = skip.min(t.len());
                    t = &t[n..];
                    skip -= n;
                }
                pending.extend_from_slice(t);
            }
        }
    }
    flush_text!();
    let end = chars.clone();
    // text after the last \par is the file's last paragraph; none: the last \par was its end, not a paragraph return
    if reader.runs.is_empty() {
        reader.runs.clear();
    } else {
        reader.finish(&para, par_style, &end, None);
    }
    reader.flush_grid();
    let mut paragraphs = std::mem::take(&mut reader.paragraphs);
    let mut ends_with_return = false;
    if paragraphs.len() >= 2 && paragraphs.last().is_some_and(|p| p.runs.is_empty()) {
        // "A\par B\par\par": an empty paragraph follows the last return
        paragraphs.pop();
        ends_with_return = true;
    }
    if paragraphs.is_empty() {
        // a file with no text at all: one empty paragraph with no settings (measured: placetext-empty.rtf)
        paragraphs.push(Paragraph::default());
    }
    if options.use_typographers_quotes {
        quotes::story(&mut paragraphs);
    }
    // no kinsoku tables are left in the document by an RTF file (measured: placetext-rtf-*), unlike a Word file
    let formatted = false;
    if options.remove_formatting {
        for p in &mut paragraphs {
            *p = plain_paragraph(p);
        }
        reader.swatches.clear();
    }
    notes_only.extend(reader.notes);
    notes_only.extend(reader.lists.notes.iter().cloned());
    let list_styles = if options.remove_formatting { Vec::new() } else { reader.lists.style_defs() };
    Ok(TextSource {
        format: "rtf".into(),
        paragraphs,
        paragraph_styles: if options.remove_formatting { Vec::new() } else { reader.sheet.finish_next() },
        character_styles: if options.remove_formatting { Vec::new() } else { reader.sheet.finish_chars().into_iter().chain(list_styles).collect() },
        colors: reader.swatches,
        formatted,
        ends_with_return,
        numbering_lists: if options.remove_formatting { Vec::new() } else { reader.lists.list_names() },
        warnings: notes_only.iter().map(|m| m.message.clone()).collect(),
        warning_messages: notes_only,
    })
}

/// A paragraph without formatting: the text only (a table stays one, its cells likewise).
fn plain_paragraph(p: &Paragraph) -> Paragraph {
    let runs = p
        .runs
        .iter()
        .map(|r| match &r.inline {
            Some(Inline::Table(t)) => {
                let mut t = t.clone();
                t.attrs.clear();
                for c in &mut t.cells {
                    c.attrs.clear();
                    c.paragraphs = c.paragraphs.iter().map(plain_paragraph).collect();
                }
                Run { text: r.text.clone(), inline: Some(Inline::Table(t)), ..Run::default() }
            }
            _ => Run::plain(&r.text),
        })
        .collect();
    Paragraph { runs, ..Paragraph::default() }
}
