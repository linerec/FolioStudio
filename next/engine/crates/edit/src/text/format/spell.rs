//! Edit > Spelling > Check Spelling: the text of the stories to check, with each stretch's
//! language. The words are judged elsewhere (`folio-spell`); the editor only says what the text
//! is, in the same scalar positions Find/Change uses (so a result can be changed with
//! `FindReplace` on `scope: "selection"`).
use super::*;
use serde::Serialize;

/// The language of the atoms `start..end` (`AppliedLanguage`, e.g. `$ID/English: USA`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpellRun {
    pub start: usize,
    pub end: usize,
    pub language: String,
}

/// One story's text. `from..to` is the part to check (the whole story unless the scope is a
/// selection). Anything that is not typed text (an anchored object, an index marker) is U+FFFC.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpellText {
    pub story: String,
    pub text: String,
    pub runs: Vec<SpellRun>,
    pub from: usize,
    pub to: usize,
}

pub(crate) fn texts(w: &mut Work<'_>, scope: &str, story: Option<&str>, from: usize, to: Option<usize>) -> Result<Vec<SpellText>, EditError> {
    if !matches!(scope, "story" | "document" | "selection") {
        return Err(EditError::new("invalid", "invalid spell check scope"));
    }
    let cat = Catalog::read(&w.ed.parts)?;
    let ids: Vec<String> = if scope == "document" {
        let mut ids = Vec::new();
        for (part, t) in &w.ed.parts {
            if !part.starts_with("Stories/") {
                continue;
            }
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
            ids.extend((0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Story").filter_map(|i| ix.value(t, i, "Self")));
        }
        ids
    } else {
        vec![story.ok_or_else(|| EditError::new("invalid", "story required"))?.to_owned()]
    };
    let mut out = Vec::new();
    for id in ids {
        let (part, ix, i, id) = locate(w, &id)?;
        let s = Story::read(w.text(&part)?, &ix, i)?;
        if s.atoms.is_empty() {
            continue;
        }
        let (lo, hi) = if scope == "selection" {
            (from, to.unwrap_or(s.atoms.len()))
        } else {
            (0, s.atoms.len())
        };
        validate(&s, lo, hi)?;
        let mut language_of = BTreeMap::new();
        let mut runs: Vec<SpellRun> = Vec::new();
        let mut text = String::with_capacity(s.atoms.len());
        for (at, a) in s.atoms.iter().enumerate() {
            text.push(if a.content || a.ch == '\r' || a.ch == '\n' || a.ch == '\u{2029}' { a.ch } else { '\u{FFFC}' });
            let language = match language_of.get(&(a.p, a.c)) {
                Some(l) => String::clone(l),
                None => {
                    let pa = style_attrs(&s.paragraphs[a.p]);
                    let ca = style_attrs(&s.characters[a.c]);
                    let mut effective = cat.character(&pa, &ca)?;
                    effective.extend(ca);
                    let l = val(&effective, "AppliedLanguage").to_owned();
                    language_of.insert((a.p, a.c), l.clone());
                    l
                }
            };
            match runs.last_mut() {
                Some(r) if r.language == language && r.end == at => r.end = at + 1,
                _ => runs.push(SpellRun { start: at, end: at + 1, language }),
            }
        }
        out.push(SpellText { story: id, text, runs, from: lo, to: hi });
    }
    Ok(out)
}

impl crate::Editor {
    /// The text of the stories a spell check covers. `scope`: `story`, `document`, or `selection`
    /// (`story` with `from..to`; without `to`, up to the end of the story: InDesign's "To End of Story").
    pub fn spell_texts(&mut self, scope: &str, story: Option<&str>, from: usize, to: Option<usize>) -> Result<Vec<SpellText>, EditError> {
        texts(&mut Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() }, scope, story, from, to)
    }
}

#[cfg(test)]
mod tests {
    use crate::Editor;
    use std::collections::BTreeMap;

    #[test]
    fn stories_come_back_as_text_with_their_languages() {
        let story = |id: &str, body: &str| {
            format!(r#"<Story Self="{id}"><ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/P">{body}</ParagraphStyleRange></Story>"#)
        };
        let csr = |lang: &str, text: &str| {
            format!(r#"<CharacterStyleRange AppliedCharacterStyle="CharacterStyle/C"><Properties><AppliedLanguage type="object">{lang}</AppliedLanguage></Properties><Content>{text}</Content></CharacterStyleRange>"#)
        };
        let mut e = Editor::new(BTreeMap::from([
            ("Stories/a.xml".into(), story("a", &format!("{}{}", csr("$ID/English: USA", "Teh "), csr("$ID/German: 1996 Reform", "Haus")))),
            ("Stories/b.xml".into(), story("b", &csr("$ID/English: USA", "한글"))),
        ]));
        let all = e.spell_texts("document", None, 0, None).unwrap();
        assert_eq!(all.len(), 2);
        let a = all.iter().find(|t| t.story == "a").unwrap();
        assert_eq!(a.text, "Teh Haus");
        assert_eq!(a.runs.iter().map(|r| (r.start, r.end, r.language.as_str())).collect::<Vec<_>>(), [(0, 4, "$ID/English: USA"), (4, 8, "$ID/German: 1996 Reform")]);
        assert_eq!((a.from, a.to), (0, 8));
        let one = e.spell_texts("selection", Some("a"), 1, Some(3)).unwrap();
        assert_eq!((one[0].from, one[0].to), (1, 3));
        assert!(e.spell_texts("page", None, 0, None).is_err());
    }
}
