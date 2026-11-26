//! Word's bulleted and numbered lists (`word/numbering.xml`) the way InDesign's Place makes them (oracle docxtext-c2-lists*; the
//! measurements are in `artifacts/docxtext-c1-20261023/measurement/lists.idml` and `artifacts/docxtext-c2-20261023/measurement/lists2.idml`,
//! `lists3.idml`, InDesign 2025). What was measured:
//! - A paragraph in a list gets `LeftIndent` and `FirstLineIndent` from the list level (`w:ind` `left` and `hanging`; a direct `w:ind` of
//!   the paragraph wins), written as an override only where they differ from the paragraph style.
//! - A bullet is `BulletsAndNumberingListType="BulletList"`; its character is `<BulletChar BulletCharacterType="UnicodeOnly"
//!   BulletCharacterValue>` unless it is `•` (a character of the private area `U+F0xx`, which Word's Symbol and Wingdings bullets use, is
//!   its low byte). A number is `NumberedList` with `NumberingExpression` (the level text with `%1` as `^1`, then `^t` for a tab after it,
//!   a space for `w:suff space`, nothing for `nothing`), `NumberingFormat` (only when it is not 1, 2, 3), and the list
//!   `NumberingList/Word Imported List K` (one per `w:num`, in order of appearance; bullets have none).
//! - `NumberingLevel` is the level + 1 from the second level on; `NumberingContinue="false"` (and `NumberingStartAt` when the start is not
//!   1) stands on the first paragraph of a list at a level (a plain paragraph between two list paragraphs does not restart it);
//!      `NumberingApplyRestartPolicy="false"` is on the first level only; the deeper levels restart after the levels above
//!      (`AfterSpecificLevel` for the second, `RangeOfLevels` 1 to level - 1 for the others).
//! - The bullet or number is set in a character style `Styles for Word/RTF Imported Lists:Word Imported List StyleN`, one for each list
//!   (`w:num`) and level in order of appearance. A level whose run properties are only fonts (Word's Symbol and Wingdings bullets) has one
//!   that is shared by all such levels and has `HorizontalScale`, `VerticalScale` 100 and the document's language (measured).
use crate::messages::{Message, msg};
use crate::StyleDef;
use crate::xmltree::El;
use std::collections::BTreeSet;

/// The name of the group the list character styles are in.
pub(crate) const STYLE_GROUP: &str = "Styles for Word/RTF Imported Lists";

#[derive(Clone, Copy, PartialEq)]
enum Suffix {
    Tab,
    Space,
    Nothing,
}

struct Level {
    start: i64,
    format: String,
    text: String,
    left: Option<f64>,
    first: Option<f64>,
    suffix: Suffix,
    /// The level's run properties: none, only fonts (not imported), or something else.
    rpr: Rpr,
}

#[derive(Clone, Copy, PartialEq)]
enum Rpr {
    None,
    FontsOnly,
    Other,
}

struct Num {
    id: String,
    abstract_id: String,
    start_override: Vec<(usize, i64)>,
}

#[derive(Default)]
pub(crate) struct Lists {
    abstracts: Vec<(String, Vec<Option<Level>>)>,
    nums: Vec<Num>,
    /// The numbers (`w:numId`) of the numbered lists met so far.
    lists: Vec<String>,
    /// The character styles made so far, by what they are for, and whether they carry the run properties of a fonts-only level.
    styles: Vec<(String, bool)>,
    /// The (list, level) pairs met so far.
    seen: BTreeSet<(String, usize)>,
    pub(crate) notes: Vec<Message>,
}

/// What a paragraph in a list gets besides its indents.
pub(crate) struct Marked {
    pub attrs: Vec<(String, String)>,
    pub properties: String,
}

fn int(e: &El, key: &str) -> Option<i64> {
    e.attr(key).and_then(|v| v.trim().parse::<i64>().ok())
}

fn twips(e: &El, key: &str) -> Option<f64> {
    e.attr(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(|v| v / 20.0)
}

fn read_level(l: &El) -> Level {
    let ind = l.child("pPr").and_then(|p| p.child("ind"));
    let left = ind.and_then(|i| twips(i, "left").or_else(|| twips(i, "start")));
    let first = ind.and_then(|i| twips(i, "firstLine").or_else(|| twips(i, "hanging").map(|v| -v)));
    let suffix = match l.child("suff").and_then(|s| s.attr("val")) {
        Some("space") => Suffix::Space,
        Some("nothing") => Suffix::Nothing,
        _ => Suffix::Tab,
    };
    let rpr = match l.child("rPr") {
        None => Rpr::None,
        Some(r) if r.elements().next().is_none() => Rpr::None,
        Some(r) if r.elements().all(|e| e.name == "rFonts") => Rpr::FontsOnly,
        Some(_) => Rpr::Other,
    };
    Level {
        start: l.child("start").and_then(|s| int(s, "val")).unwrap_or(1),
        format: l.child("numFmt").and_then(|s| s.attr("val")).unwrap_or("decimal").to_owned(),
        text: l.child("lvlText").and_then(|s| s.attr("val")).unwrap_or_default().to_owned(),
        left,
        first,
        suffix,
        rpr,
    }
}

/// The InDesign number format (`NumberingFormat`) of a Word one; `None` is the default, 1, 2, 3 (measured: decimal, ordinal, cardinalText
/// and koreanCounting come in as that).
pub(crate) fn number_format(format: &str) -> Result<Option<&'static str>, ()> {
    Ok(match format {
        "decimal" | "ordinal" | "cardinalText" | "koreanCounting" => None,
        "upperLetter" => Some("A, B, C, D..."),
        "lowerLetter" => Some("a, b, c, d..."),
        "upperRoman" => Some("I, II, III, IV..."),
        "lowerRoman" => Some("i, ii, iii, iv..."),
        "decimalZero" => Some("01,02,03..."),
        "chineseCounting" => Some("$ID/(kanji) 1,2,3,4..."),
        "ganada" => Some("$ID/Korean Numbering 2 (Ganada)"),
        _ => return Err(()),
    })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

impl Lists {
    pub(crate) fn read(root: Option<&El>) -> Self {
        let mut me = Lists::default();
        let Some(root) = root else { return me };
        for a in root.elements().filter(|e| e.name == "abstractNum") {
            let Some(id) = a.attr("abstractNumId") else { continue };
            let mut levels: Vec<Option<Level>> = Vec::new();
            for l in a.elements().filter(|e| e.name == "lvl") {
                let at = int(l, "ilvl").and_then(|v| usize::try_from(v).ok()).unwrap_or(levels.len());
                if levels.len() <= at {
                    levels.resize_with(at + 1, || None);
                }
                levels[at] = Some(read_level(l));
            }
            me.abstracts.push((id.to_owned(), levels));
        }
        for n in root.elements().filter(|e| e.name == "num") {
            let (Some(id), Some(abs)) = (n.attr("numId"), n.child("abstractNumId").and_then(|a| a.attr("val"))) else { continue };
            let mut start_override = Vec::new();
            for o in n.elements().filter(|e| e.name == "lvlOverride") {
                let at = int(o, "ilvl").and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
                if let Some(v) = o.child("startOverride").and_then(|s| int(s, "val")) {
                    start_override.push((at, v));
                }
                if o.child("lvl").is_some() {
                    me.notes.push(msg("textimport.listOverride", "목록 수준을 덮어쓰는 설정(lvlOverride/lvl)은 아직 가져오지 않습니다", serde_json::json!({})));
                }
            }
            me.nums.push(Num { id: id.to_owned(), abstract_id: abs.to_owned(), start_override });
        }
        me
    }

    fn level(&self, num_id: &str, ilvl: usize) -> Option<(&Num, &Level)> {
        let num = self.nums.iter().find(|n| n.id == num_id)?;
        let levels = &self.abstracts.iter().find(|(id, _)| *id == num.abstract_id)?.1;
        Some((num, levels.get(ilvl)?.as_ref()?))
    }

    /// The indents (left, first line) of the list level, in points.
    pub(crate) fn indent(&self, num_id: &str, ilvl: usize) -> Option<(Option<f64>, Option<f64>)> {
        self.level(num_id, ilvl).map(|(_, l)| (l.left, l.first))
    }

    /// The list marking of the next paragraph in this list and level (call it once for each paragraph, in the order of the text).
    pub(crate) fn mark(&mut self, num_id: &str, ilvl: usize) -> Option<Marked> {
        let (format_name, rpr, start, text, suffix) = {
            let (num, level) = self.level(num_id, ilvl)?;
            let start = num.start_override.iter().find(|(l, _)| *l == ilvl).map_or(level.start, |(_, v)| *v);
            (level.format.clone(), level.rpr, start, level.text.clone(), level.suffix)
        };
        if format_name == "none" {
            return None;
        }
        let bullet = format_name == "bullet";
        let style_key = if rpr == Rpr::FontsOnly { "fonts-only".to_owned() } else { format!("{num_id}/{ilvl}") };
        if rpr == Rpr::Other {
            self.notes.push(msg("textimport.listCharacterFormat", "목록 기호·번호의 글자 서식(굵게·색 등)은 아직 InDesign과 같게 가져오지 않습니다", serde_json::json!({})));
        }
        let format = if bullet { Ok(None) } else { number_format(&format_name) };
        let bullet_char = text.chars().next();
        if format.is_err() {
            self.notes.push(msg("textimport.listFormat", format!("번호 모양 {format_name} 은(는) 아직 InDesign과 같게 가져오지 않습니다 (1, 2, 3 으로 들어갑니다)"), serde_json::json!({"format_name": format_name})));
        }
        let format = format.unwrap_or(None);
        let style = match self.styles.iter().position(|(k, _)| *k == style_key) {
            Some(i) => i + 1,
            None => {
                self.styles.push((style_key, rpr == Rpr::FontsOnly));
                self.styles.len()
            }
        };
        let style_id = format!("CharacterStyle/{}%3aWord Imported List Style{style}", STYLE_GROUP);
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut props = String::new();
        if bullet {
            attrs.push(("BulletsAndNumberingListType".into(), "BulletList".into()));
            if let Some(c) = bullet_char.filter(|c| *c != '\u{2022}') {
                let code = c as u32;
                let code = if (0xF000..=0xF0FF).contains(&code) { code & 0xFF } else { code };
                props += &format!("<BulletChar BulletCharacterType=\"UnicodeOnly\" BulletCharacterValue=\"{code}\" />");
            }
            props += &format!("<BulletsCharacterStyle type=\"object\">{}</BulletsCharacterStyle>", escape(&style_id));
            return Some(Marked { attrs, properties: props });
        }
        let first = self.seen.insert((num_id.to_owned(), ilvl));
        let list = match self.lists.iter().position(|k| k == num_id) {
            Some(i) => i + 1,
            None => {
                self.lists.push(num_id.to_owned());
                self.lists.len()
            }
        };
        attrs.push(("BulletsAndNumberingListType".into(), "NumberedList".into()));
        if ilvl > 0 {
            attrs.push(("NumberingLevel".into(), (ilvl + 1).to_string()));
        }
        if first && start != 1 {
            attrs.push(("NumberingStartAt".into(), start.to_string()));
        }
        if first {
            attrs.push(("NumberingContinue".into(), "false".into()));
        }
        if ilvl == 0 {
            attrs.push(("NumberingApplyRestartPolicy".into(), "false".into()));
        }
        let mut expression = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match (c, chars.peek()) {
                ('%', Some(d)) if d.is_ascii_digit() => {
                    expression.push('^');
                    expression.push(*d);
                    chars.next();
                }
                _ => expression.push(c),
            }
        }
        expression.push_str(match suffix {
            Suffix::Tab => "^t",
            Suffix::Space => " ",
            Suffix::Nothing => "",
        });
        attrs.push(("NumberingExpression".into(), expression));
        if let Some(f) = format {
            props += &format!("<NumberingFormat type=\"string\">{}</NumberingFormat>", escape(f));
        }
        props += &format!("<AppliedNumberingList type=\"object\">NumberingList/Word Imported List {list}</AppliedNumberingList>");
        if ilvl >= 1 {
            let policy = if ilvl == 1 { "AfterSpecificLevel" } else { "RangeOfLevels" };
            props += &format!("<NumberingRestartPolicies RestartPolicy=\"{policy}\" LowerLevel=\"1\" UpperLevel=\"{ilvl}\" />");
        }
        props += &format!("<NumberingCharacterStyle type=\"object\">{}</NumberingCharacterStyle>", escape(&style_id));
        Some(Marked { attrs, properties: props })
    }

    /// The names of the lists the numbered paragraphs belong to.
    pub(crate) fn list_names(&self) -> Vec<String> {
        (1..=self.lists.len()).map(|k| format!("Word Imported List {k}")).collect()
    }

    /// The list character styles that were made (`Word Imported List Style1` ...), in the group of their own.
    pub(crate) fn style_defs(&self) -> Vec<StyleDef> {
        self.styles
            .iter()
            .enumerate()
            .map(|(i, (_, with_rpr))| {
                let attrs = if *with_rpr {
                    vec![("HorizontalScale".to_owned(), "100".to_owned()), ("VerticalScale".to_owned(), "100".to_owned()), ("AppliedLanguage".to_owned(), "$ID/English: USA".to_owned())]
                } else {
                    Vec::new()
                };
                StyleDef { name: format!("{STYLE_GROUP}:Word Imported List Style{}", i + 1), attrs, ..StyleDef::default() }
            })
            .collect()
    }
}
