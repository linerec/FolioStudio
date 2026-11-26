//! Bulleted and numbered lists of an RTF file (`{\*\listtable ...}`, `{\*\listoverridetable ...}`, `\ls` and `\ilvl` of a paragraph) the way
//! InDesign 2025 brings them in (oracle `docxtext-c5-rtf-l*`; measurements `artifacts/docxtext-c5-20261023/measurement/l*.idml`). It is the
//! same list model as a Word file's (`numbering.rs`: the same list paragraph attributes, the list `Word Imported List K`), with these differences:
//! - The indents are the paragraph's own (`\li`, `\fi`); the list level's are not read (a list paragraph with none has none).
//! - All lists and levels use one character style, `Word Imported List Style1` (the level's own character format is not read).
//! - `NumberingContinue="false"` stands where no numbered paragraph of the text (of any list) came before at that level or a higher one.
//! - `NumberingStartAt` (a start that is not 1) is on every paragraph of the list's first unbroken run, not only the first.
//! - Once a paragraph of the second level or deeper has been met, every later paragraph (a plain one too) carries the same
//!   `NumberingRestartPolicies` the last such paragraph had.
//! - The old paragraph numbering (`\pn`, `\pntext`) is no list: the numbering text is dropped and the paragraph keeps its indents.
//! - The bullet or number written in front of the paragraph (`{\listtext ...}`) is not text of the story.
use crate::messages::{Message, msg};
use super::Tok;
use crate::StyleDef;
use crate::numbering::{Marked, STYLE_GROUP, number_format};
use std::collections::BTreeSet;

struct Level {
    start: i64,
    nfc: i32,
    text: String,
    /// 0 a tab follows the number, 1 a space, 2 nothing.
    follow: i32,
}

struct Def {
    id: i32,
    levels: Vec<Level>,
}

#[derive(Default)]
pub(super) struct Lists {
    defs: Vec<Def>,
    /// `\ls` number and the `\listid` it names.
    overrides: Vec<(i32, i32)>,
    /// The numbers (`\ls`) of the numbered lists met so far.
    numbered: Vec<i32>,
    seen: BTreeSet<(i32, usize)>,
    /// The (list, level) pairs whose start is still on their paragraphs.
    open_start: BTreeSet<(i32, usize)>,
    last_ls: Option<i32>,
    /// What the last paragraph of the second level or deeper wrote as `NumberingRestartPolicies`.
    policy: Option<String>,
    used: bool,
    /// The highest level (smallest number) of the numbered paragraphs met so far.
    shallowest: Option<usize>,
    pub notes: Vec<Message>,
}

fn cp1252(b: u8) -> char {
    encoding_rs::WINDOWS_1252.decode(&[b]).0.chars().next().unwrap_or('?')
}

/// The list and list override tables of the file.
pub(super) fn parse(toks: &[Tok<'_>]) -> Lists {
    let mut me = Lists::default();
    let mut depth = 0i32;
    let mut table: Option<(i32, bool)> = None; // (depth of the table group, is the override table)
    let mut prev_open = false;
    let mut prev_star = false;
    let mut cur: Option<Def> = None;
    let mut cur_override: Option<(i32, i32)> = None;
    // the template text of a level being read: (depth of its group, template so far, bytes still to skip after a \u)
    let mut text: Option<(i32, String, bool, usize)> = None;
    for tok in toks {
        match tok {
            Tok::Open => {
                depth += 1;
                prev_open = true;
                prev_star = false;
                continue;
            }
            Tok::Close => {
                if let Some((d, t, _, _)) = &text
                    && *d == depth
                    && let Some(level) = cur.as_mut().and_then(|c| c.levels.last_mut())
                {
                    level.text = t.clone();
                    text = None;
                }
                if table.is_some_and(|(d, _)| d == depth) {
                    table = None;
                    if let Some(c) = cur.take() {
                        me.defs.push(c);
                    }
                    if let Some(o) = cur_override.take() {
                        me.overrides.push(o);
                    }
                }
                depth -= 1;
            }
            Tok::Symbol(b'*') if prev_open => {
                prev_star = true;
                continue;
            }
            Tok::Word(w, p) => {
                let opening = prev_open || prev_star;
                match *w {
                    b"listtable" if opening => table = Some((depth, false)),
                    b"listoverridetable" if opening => table = Some((depth, true)),
                    _ if table.is_none() => {}
                    b"list" if table.is_some_and(|(_, o)| !o) => {
                        if let Some(c) = cur.take() {
                            me.defs.push(c);
                        }
                        cur = Some(Def { id: 0, levels: Vec::new() });
                    }
                    b"listid" => {
                        if let Some(c) = cur.as_mut() {
                            c.id = p.unwrap_or(0);
                        } else if let Some(o) = cur_override.as_mut() {
                            o.1 = p.unwrap_or(0);
                        }
                    }
                    b"listlevel" => {
                        if let Some(c) = cur.as_mut() {
                            c.levels.push(Level { start: 1, nfc: 0, text: String::new(), follow: 0 });
                        }
                    }
                    b"levelnfc" => {
                        if let Some(l) = cur.as_mut().and_then(|c| c.levels.last_mut()) {
                            l.nfc = p.unwrap_or(0);
                        }
                    }
                    b"levelstartat" => {
                        if let Some(l) = cur.as_mut().and_then(|c| c.levels.last_mut()) {
                            l.start = i64::from(p.unwrap_or(1));
                        }
                    }
                    b"levelfollow" => {
                        if let Some(l) = cur.as_mut().and_then(|c| c.levels.last_mut()) {
                            l.follow = p.unwrap_or(0);
                        }
                    }
                    b"leveltext" => text = Some((depth, String::new(), false, 0)),
                    b"listoverride" => {
                        if let Some(o) = cur_override.take() {
                            me.overrides.push(o);
                        }
                        cur_override = Some((0, 0));
                    }
                    b"ls" => {
                        if let Some(o) = cur_override.as_mut() {
                            o.0 = p.unwrap_or(0);
                        }
                    }
                    b"u" => {
                        if let Some((_, t, expect_len, skip)) = text.as_mut() {
                            let n = p.unwrap_or(0);
                            let unit = if n < 0 { (n + 65536) as u32 } else { n as u32 };
                            if !*expect_len {
                                *expect_len = true;
                            }
                            t.extend(char::from_u32(unit));
                            *skip = 1;
                        }
                    }
                    _ => {}
                }
            }
            Tok::Hex(b) => {
                if let Some((_, t, expect_len, skip)) = text.as_mut() {
                    if *skip > 0 {
                        *skip -= 1;
                    } else if !*expect_len {
                        // the first byte is the length of the template
                        *expect_len = true;
                    } else if *b < 9 {
                        t.push('%');
                        t.push(char::from(b'1' + *b));
                    } else {
                        t.push(cp1252(*b));
                    }
                }
            }
            Tok::Text(bytes) => {
                if let Some((_, t, expect_len, skip)) = text.as_mut() {
                    for &b in *bytes {
                        if *skip > 0 {
                            *skip -= 1;
                        } else if b == b';' {
                        } else {
                            *expect_len = true;
                            t.push(cp1252(b));
                        }
                    }
                }
            }
            _ => {}
        }
        prev_open = false;
        prev_star = false;
    }
    me
}

impl Lists {
    fn level(&self, ls: i32, ilvl: usize) -> Option<&Level> {
        let id = self.overrides.iter().find(|(n, _)| *n == ls)?.1;
        self.defs.iter().find(|d| d.id == id)?.levels.get(ilvl)
    }

    /// Whether the paragraph with this `\ls` and `\ilvl` is in a list; a paragraph outside any list calls [`Lists::plain`].
    pub(super) fn has(&self, ls: i32, ilvl: usize) -> bool {
        self.level(ls, ilvl).is_some_and(|l| l.nfc != 255)
    }

    /// A paragraph that is in no list: it still carries the restart policy the list paragraphs before it wrote.
    pub(super) fn plain(&mut self) -> String {
        self.last_ls = None;
        self.policy.clone().unwrap_or_default()
    }

    /// The list marking of the next paragraph in this list and level (call it once for each paragraph, in the order of the text).
    pub(super) fn mark(&mut self, ls: i32, ilvl: usize) -> Option<Marked> {
        let (nfc, start, text, follow) = {
            let l = self.level(ls, ilvl)?;
            (l.nfc, l.start, l.text.clone(), l.follow)
        };
        if nfc == 255 {
            return None;
        }
        self.used = true;
        let style_id = format!("CharacterStyle/{}%3aWord Imported List Style1", STYLE_GROUP);
        let first = self.seen.insert((ls, ilvl));
        let continues = self.last_ls == Some(ls);
        self.last_ls = Some(ls);
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut props = String::new();
        if nfc == 23 {
            attrs.push(("BulletsAndNumberingListType".into(), "BulletList".into()));
            if let Some(c) = text.chars().next().filter(|c| *c != '\u{2022}') {
                let code = c as u32;
                let code = if (0xF000..=0xF0FF).contains(&code) { code & 0xFF } else { code };
                props += &format!("<BulletChar BulletCharacterType=\"UnicodeOnly\" BulletCharacterValue=\"{code}\" />");
            }
            // a bullet paragraph carries the policy too (before the character style, as a number's does)
            props += self.policy.as_deref().unwrap_or_default();
            props += &format!("<BulletsCharacterStyle type=\"object\">{}</BulletsCharacterStyle>", escape(&style_id));
            return Some(Marked { attrs, properties: props });
        }
        let format = match nfc {
            0 | 5..=7 => None,
            1 => number_format("upperRoman").ok().flatten(),
            2 => number_format("lowerRoman").ok().flatten(),
            3 => number_format("upperLetter").ok().flatten(),
            4 => number_format("lowerLetter").ok().flatten(),
            22 => number_format("decimalZero").ok().flatten(),
            _ => {
                self.notes.push(msg("textimport.rtfListFormat", format!("번호 모양 \\levelnfc{nfc} 은(는) 아직 InDesign과 같게 가져오지 않습니다 (1, 2, 3 으로 들어갑니다)"), serde_json::json!({"nfc": nfc})));
                None
            }
        };
        let list = match self.numbered.iter().position(|k| *k == ls) {
            Some(i) => i + 1,
            None => {
                self.numbered.push(ls);
                self.numbered.len()
            }
        };
        attrs.push(("BulletsAndNumberingListType".into(), "NumberedList".into()));
        if ilvl > 0 {
            attrs.push(("NumberingLevel".into(), (ilvl + 1).to_string()));
        }
        // a start other than 1 is on the paragraphs of the list's first unbroken run
        if start != 1 {
            let key = (ls, ilvl);
            if first {
                self.open_start.insert(key);
            } else if !continues {
                self.open_start.remove(&key);
            }
            if self.open_start.contains(&key) {
                attrs.push(("NumberingStartAt".into(), start.to_string()));
            }
        }
        // the numbering restarts (`NumberingContinue="false"`) where no numbered paragraph of the text, in any list, came before at this level or a higher one
        // (measured: l03, l04, l10, l14, l19)
        if self.shallowest.is_none_or(|m| m > ilvl) {
            attrs.push(("NumberingContinue".into(), "false".into()));
        }
        self.shallowest = Some(self.shallowest.map_or(ilvl, |m| m.min(ilvl)));
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
        expression.push_str(match follow {
            1 => " ",
            2 => "",
            _ => "^t",
        });
        attrs.push(("NumberingExpression".into(), expression));
        if let Some(f) = format {
            props += &format!("<NumberingFormat type=\"string\">{}</NumberingFormat>", escape(f));
        }
        props += &format!("<AppliedNumberingList type=\"object\">NumberingList/Word Imported List {list}</AppliedNumberingList>");
        if ilvl >= 1 {
            let policy = if ilvl == 1 { "AfterSpecificLevel" } else { "RangeOfLevels" };
            self.policy = Some(format!("<NumberingRestartPolicies RestartPolicy=\"{policy}\" LowerLevel=\"1\" UpperLevel=\"{ilvl}\" />"));
        }
        if let Some(p) = &self.policy {
            props += p;
        }
        props += &format!("<NumberingCharacterStyle type=\"object\">{}</NumberingCharacterStyle>", escape(&style_id));
        Some(Marked { attrs, properties: props })
    }

    /// The names of the numbered lists the paragraphs belong to.
    pub(super) fn list_names(&self) -> Vec<String> {
        (1..=self.numbered.len()).map(|k| format!("Word Imported List {k}")).collect()
    }

    /// The one character style the bullets and numbers are set in.
    pub(super) fn style_defs(&self) -> Vec<StyleDef> {
        if !self.used {
            return Vec::new();
        }
        let attrs = vec![("HorizontalScale".to_owned(), "100".to_owned()), ("VerticalScale".to_owned(), "100".to_owned()), ("CharacterDirection".to_owned(), "DefaultDirection".to_owned())];
        vec![StyleDef { name: format!("{STYLE_GROUP}:Word Imported List Style1"), attrs, ..StyleDef::default() }]
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
