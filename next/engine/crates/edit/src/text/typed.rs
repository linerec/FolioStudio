//! Typed input: text a person types is not the same as text a script inserts or a paste brings. Typing may correct the
//! words it finishes (Edit > Spelling > Autocorrect) and make straight quotes curly; inserting and pasting never do (measured in
//! InDesign 2025, see `folio_spell::autocorrect`). The rules are `folio_spell::autocorrect`; this file gives them the story's text and the
//! language of each character, so the table and the dictionary that apply are those of the language the word is in.
//!
//! The settings (the tables, the capitalization switch) are arguments: they live in the computer's user settings, never in
//! the document, so the app (or an LLM through MCP) hands them in with every typed text. The quote setting is the document's
//! own (`TextPreference.TypographersQuotes`), the engine reads it.
use super::*;
use folio_spell::autocorrect::{self, Fix, Pair, Rules, Tables};
use folio_spell::user::language_key;
use std::collections::BTreeMap;

/// The wire form of the tables (`language → [[typo, correction], …]`) as the rules take them.
fn tables_of(wire: &BTreeMap<String, Vec<Vec<String>>>) -> Tables {
    wire.iter()
        .map(|(language, rows)| (language_key(language).to_owned(), rows.iter().filter(|r| r.len() >= 2).map(|r| (r[0].clone(), r[1].clone())).collect::<Vec<Pair>>()))
        .collect()
}

/// "Use Typographer's Quotes" of the document (on when the document does not say: it is on in a new InDesign document).
fn quotes_on(ed: &crate::Editor) -> bool {
    ed.document_preferences().ok().and_then(|p| p.get("TextPreference.TypographersQuotes").and_then(|v| v.as_bool())).unwrap_or(true)
}

/// What the person's typing does: `auto_correct` is Edit > Spelling > Autocorrect, `typed_run` says that a character of the word before the
/// caret was typed just before (a word a script or a paste brought is left alone).
#[derive(Clone, Copy)]
pub(crate) struct Typing {
    pub capitalization: bool,
    pub auto_correct: bool,
    pub typed_run: bool,
}

fn fix(w: &mut Work<'_>, story: &str, at: usize, typed: &str, tables: &Tables, typing: Typing) -> Result<Option<Fix>, EditError> {
    let quotes = quotes_on(w.ed).then_some(folio_textimport::smart_quote as fn(Option<char>, char) -> char);
    // nothing can change when no typed character finishes a word (or there is neither a table nor the capital rule), and none is a quote
    let may_correct = typing.auto_correct && typed.chars().any(autocorrect::ends_word) && (typing.capitalization || tables.values().any(|t| !t.is_empty()));
    let may_quote = quotes.is_some() && typed.chars().any(|c| matches!(c, '"' | '\''));
    if !may_correct && !may_quote {
        return Ok(None);
    }
    let texts = format::spell::texts(w, "story", Some(story), 0, None)?;
    // a story without any text has no run: typing still follows the quote setting
    let (chars, runs): (Vec<char>, &[SpellRun]) = texts.first().map_or((Vec::new(), &[]), |t| (t.text.chars().collect(), t.runs.as_slice()));
    if at > chars.len() {
        return Err(EditError::new("invalid", "insertion point outside story"));
    }
    let run_of = |i: usize| runs.iter().find(|r| r.start <= i && i < r.end);
    // typed text takes the language of the character before the caret (of the one after it at the start of a paragraph)
    let caret_run = if at > 0 && chars[at - 1] != '\r' { run_of(at - 1) } else { run_of(at).or_else(|| at.checked_sub(1).and_then(run_of)) };
    let run_at = |place: Option<usize>| match place {
        Some(i) => run_of(i),
        None => caret_run,
    };
    let table_of = |place: Option<usize>| run_at(place).and_then(|r| tables.get(language_key(&r.language))).map(Vec::as_slice);
    let known = |place: Option<usize>, word: &str| run_at(place).is_some_and(|r| folio_spell::is_known_word(&folio_spell::Bundled, &r.language, word));
    let rules = Rules { auto_correct: typing.auto_correct, capitalization_errors: typing.capitalization, quotes };
    Ok(autocorrect::type_text(&chars, at, typed, table_of, known, rules, typing.typed_run))
}

/// Type ▸ the characters `text` typed at `at`: the same as `InsertText`, and every word that a typed space, punctuation mark,
/// tab or return finishes is corrected by the table of its language, every typed straight quote follows the document's quote setting.
/// One command, so one undo step. `typed_run` (default: yes) says that a character of the word before `at` was typed just before;
/// `auto_correct` (default: yes) is the Autocorrect switch.
pub(crate) fn type_text(
    w: &mut Work<'_>, story: &String, at: &usize, text: &String, tables: &BTreeMap<String, Vec<Vec<String>>>, capitalization: &bool,
    typed_run: &Option<bool>, auto_correct: &Option<bool>,
) -> Result<(), EditError> {
    let typing = Typing { capitalization: *capitalization, auto_correct: auto_correct.unwrap_or(true), typed_run: typed_run.unwrap_or(true) };
    match fix(w, story, *at, text, &tables_of(tables), typing)? {
        Some(f) => {
            delete(w, story, f.start, f.end)?;
            insert(w, story, f.start, &f.replacement)
        }
        None => insert(w, story, *at, text),
    }
}

impl crate::Editor {
    /// What typing `typed` at `at` in `story` would change (`None`: nothing is corrected), so the screen knows where the caret goes
    /// before it sends the `typeText` command. Nothing is written.
    // Preserve the public typing query signature used by the server and callers.
    #[allow(clippy::too_many_arguments)]
    pub fn typed_fix(
        &mut self, story: &str, at: usize, typed: &str, tables: &BTreeMap<String, Vec<Vec<String>>>, capitalization: bool, auto_correct: bool, typed_run: bool,
    ) -> Result<Option<Fix>, EditError> {
        let typing = Typing { capitalization, auto_correct, typed_run };
        fix(&mut Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() }, story, at, typed, &tables_of(tables), typing)
    }
}

#[cfg(test)]
mod tests {
    use super::Typing;
    use crate::{Command, Editor};
    use std::collections::BTreeMap;

    fn parts(runs: &[(&str, &str)], quotes: Option<bool>) -> BTreeMap<String, String> {
        let body: String = runs
            .iter()
            .map(|(lang, text)| format!(r#"<CharacterStyleRange AppliedCharacterStyle="CharacterStyle/C"><Properties><AppliedLanguage type="object">{lang}</AppliedLanguage></Properties><Content>{text}</Content></CharacterStyleRange>"#))
            .collect();
        let mut map = BTreeMap::from([(
            "Stories/a.xml".to_string(),
            format!(r#"<Story Self="a"><ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/P">{body}</ParagraphStyleRange></Story>"#),
        )]);
        if let Some(on) = quotes {
            map.insert(folio_idml::docprefs::PART.to_string(), format!(r#"<Preferences><TextPreference TypographersQuotes="{on}" /></Preferences>"#));
        }
        map
    }

    fn story(runs: &[(&str, &str)]) -> BTreeMap<String, String> {
        parts(runs, None)
    }

    fn tables() -> BTreeMap<String, Vec<Vec<String>>> {
        BTreeMap::from([("English: USA".to_string(), vec![vec!["teh".to_string(), "the".to_string()]])])
    }

    fn contents(e: &mut Editor) -> String {
        e.spell_texts("story", Some("a"), 0, None).unwrap().into_iter().next().map(|t| t.text).unwrap_or_default()
    }

    fn typed(at: usize, text: &str, caps: bool) -> Command {
        crate::commands::RegisteredCommand::TypeText { story: "a".into(), at, text: text.into(), tables: tables(), capitalization: caps, typed_run: None, auto_correct: None }.into()
    }

    #[test]
    fn a_typed_space_corrects_the_listed_word_in_one_step() {
        let mut e = Editor::new(story(&[("$ID/English: USA", "see teh")]));
        let typing = Typing { capitalization: false, auto_correct: true, typed_run: true };
        assert_eq!(e.typed_fix("a", 7, " ", &tables(), typing.capitalization, typing.auto_correct, true).unwrap().map(|f| (f.start, f.end, f.replacement)), Some((4, 7, "the ".to_string())));
        assert_eq!(e.typed_fix("a", 7, " ", &tables(), false, true, false).unwrap(), None); // a word nobody typed
        e.apply("type", &[typed(7, " ", false)]).unwrap();
        assert_eq!(contents(&mut e), "see the ");
        e.undo().unwrap();
        assert_eq!(contents(&mut e), "see teh");
    }

    #[test]
    fn inserted_text_is_never_corrected() {
        let mut e = Editor::new(story(&[("$ID/English: USA", "see teh")]));
        e.apply("insert", &[Command::InsertText { story: "a".into(), at: 7, text: " ".into() }]).unwrap();
        assert_eq!(contents(&mut e), "see teh ");
    }

    #[test]
    fn a_language_without_a_table_is_left_alone_and_the_capital_rule_follows_the_dictionary() {
        let mut e = Editor::new(story(&[("$ID/German: 1996 Reform", "see teh")]));
        e.apply("type", &[typed(7, " ", false)]).unwrap();
        assert_eq!(contents(&mut e), "see teh ");
        // Folio has no German dictionary, so the capital rule has nothing to ask there; English words are known
        let mut e = Editor::new(story(&[("$ID/English: USA", "it is. hello")]));
        e.apply("type", &[typed(12, " ", true)]).unwrap();
        assert_eq!(contents(&mut e), "it is. Hello ");
        let mut e = Editor::new(story(&[("$ID/English: USA", "it is. zqfoo")]));
        e.apply("type", &[typed(12, " ", true)]).unwrap();
        assert_eq!(contents(&mut e), "it is. zqfoo ");
    }

    #[test]
    fn a_word_typed_in_one_go_is_corrected_as_it_is_finished() {
        let mut e = Editor::new(story(&[("$ID/English: USA", "a ")]));
        e.apply("type", &[typed(2, "teh. teh ", false)]).unwrap();
        assert_eq!(contents(&mut e), "a the. the ");
    }

    #[test]
    fn quotes_follow_the_documents_setting_and_not_the_autocorrect_switch() {
        for (quotes, expected) in [(Some(true), "\u{201c}a\u{201d} don\u{2019}t"), (None, "\u{201c}a\u{201d} don\u{2019}t"), (Some(false), "\"a\" don't")] {
            let mut e = Editor::new(parts(&[("$ID/English: USA", "")], quotes));
            for (i, ch) in "\"a\" don't".chars().enumerate() {
                let command = crate::commands::RegisteredCommand::TypeText {
                    story: "a".into(), at: i, text: ch.to_string(), tables: BTreeMap::new(), capitalization: false, typed_run: None, auto_correct: Some(false),
                };
                e.apply("type", &[command.into()]).unwrap();
            }
            assert_eq!(contents(&mut e), expected, "{quotes:?}");
        }
    }

    /// The table of what InDesign 2025 made of typed text (`next/oracle/rules/autocorrect-typing.json`, written by `to_table.py` of
    /// `artifacts/inputtype-c1-20261023/tools` from sessions with real key presses): every row is typed one character after the other, as the
    /// screen does, and must come out the same. A row with `skip` documents a difference that is not Folio's to fix (InDesign's own word list
    /// or abbreviation list); it is counted, not compared.
    #[test]
    fn typed_input_matches_what_indesign_made_of_it() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/rules/autocorrect-typing.json");
        let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let (mut wrong, mut compared, mut skipped) = (Vec::new(), 0, 0);
        for case in table["cases"].as_array().unwrap() {
            if case.get("skip").is_some() {
                skipped += 1;
                continue;
            }
            compared += 1;
            let language = case["language"].as_str().unwrap();
            let pairs: Vec<Vec<String>> = serde_json::from_value(case["pairs"].clone()).unwrap();
            let tables = BTreeMap::from([(language.to_string(), pairs)]);
            let pre = case.get("pre").and_then(|v| v.as_str()).unwrap_or("");
            let quotes = Some(case.get("quotes").and_then(|v| v.as_bool()).unwrap_or(true));
            let mut e = Editor::new(parts(&[(&format!("$ID/{language}"), pre)], quotes));
            let mut run = case.get("typedRun").and_then(|v| v.as_bool()).unwrap_or(false);
            let mut at = case.get("at").and_then(|v| v.as_u64()).map_or_else(|| contents(&mut e).chars().count(), |v| v as usize);
            for ch in case["typed"].as_str().unwrap().chars() {
                let command = crate::commands::RegisteredCommand::TypeText {
                    story: "a".into(), at, text: ch.to_string(), tables: tables.clone(), capitalization: case["caps"].as_bool().unwrap(),
                    typed_run: Some(run), auto_correct: Some(case.get("autoCorrect").and_then(|v| v.as_bool()).unwrap_or(true)),
                };
                let before = contents(&mut e).chars().count() as i64;
                e.apply("type", &[command.into()]).unwrap();
                at = (at as i64 + contents(&mut e).chars().count() as i64 - before) as usize; // the caret stays after what was typed (a correction changes the length)
                run = !folio_spell::autocorrect::ends_word(ch);
            }
            let got = contents(&mut e);
            if got != case["expected"].as_str().unwrap() {
                wrong.push(format!("{}: typed {:?} gave {:?}, InDesign {:?}", case["id"], case["typed"], got, case["expected"]));
            }
        }
        assert!(compared >= 60, "the table must hold at least 60 compared cases, has {compared} ({skipped} skipped)");
        assert!(wrong.is_empty(), "{} of {compared} differ:\n{}", wrong.len(), wrong.join("\n"));
    }
}
