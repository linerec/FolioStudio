//! Edit > Spelling as engine queries. Nothing here keeps state: the person's settings (which checks
//! are on, the user dictionaries, the autocorrect tables) come in as arguments and the changed
//! settings go back out, so the app (or an LLM through MCP) stores them where the computer's
//! settings live and the project folder never holds them.
use folio_spell::{autocorrect, check_text, suggest, user, Bundled, IssueKind, Lang, Options, Run, UserDictionaries};
use serde_json::{json, Value};

fn field<T: serde::de::DeserializeOwned + Default>(req: &Value, key: &str) -> Result<T, String> {
    match req.get(key) {
        None | Some(Value::Null) => Ok(T::default()),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("{key}: {e}")),
    }
}

/// `{"cmd":"spellCheck","scope":"document"|"story"|"selection","story":…,"start":…,"end":… (a selection without `end` runs to the end of the story),
///   "options":{…spellPreferences check switches…},"userDictionaries":{…},"ignored":[…],
///   "defaultLanguage":"English: USA","suggestions":false,"limit":5000}`
/// → `{"issues":[{"story",…Issue}],"stories":n,"truncated":bool}`
pub fn check(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let edit = session.and_then(|s| s.edit.as_ref()).ok_or("no document open")?;
    let scope = req["scope"].as_str().unwrap_or("document");
    let from = req["start"].as_u64().unwrap_or(0) as usize;
    let to = req["end"].as_u64().map(|v| v as usize);
    let texts = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.spell_texts(scope, req["story"].as_str(), from, to).map_err(|e| e.to_string())?;
    let options: Options = field(req, "options")?;
    let users: UserDictionaries = field(req, "userDictionaries")?;
    let ignored: Vec<String> = field(req, "ignored")?;
    let default_language = req["defaultLanguage"].as_str().unwrap_or("English: USA");
    let with_suggestions = req["suggestions"].as_bool().unwrap_or(false);
    let limit = req["limit"].as_u64().unwrap_or(5000) as usize;
    let mut issues = Vec::new();
    let mut truncated = false;
    for story in &texts {
        let runs: Vec<Run> = story.runs.iter().map(|r| Run { start: r.start, end: r.end, language: r.language.clone() }).collect();
        for issue in check_text(&Bundled, &story.text, &runs, default_language, &options, &users, &ignored, with_suggestions) {
            // A word cut by the selection's edge is not part of the selection.
            if issue.start < story.from || issue.end > story.to {
                continue;
            }
            if issues.len() >= limit {
                truncated = true;
                break;
            }
            issues.push(json!({"story": story.story, "issue": issue}));
        }
    }
    let issues: Vec<Value> = issues.into_iter().map(|row| {
        let mut flat = row["issue"].clone();
        flat["story"] = row["story"].clone();
        flat
    }).collect();
    Ok(json!({"issues": issues, "stories": texts.len(), "truncated": truncated}))
}

/// The words the dynamic-spelling underline goes under, placed on the pages (screen only: the
/// underline is not part of the document, a print or a PDF). InDesign 2026 underlines the words
/// Check Spelling lists as "Not in Dictionary" (a lower-case proper noun is one) and nothing
/// else: no repeated word, no lower-case sentence start.
/// `{"cmd":"spellMarks","page":n (optional),"options":{…},"userDictionaries":{…},"defaultLanguage":"English: USA"}`
/// → `{"marks":[{"story","start","end","page","frame","from":[x,y],"to":[x,y]}]}` (`from`→`to` is the baseline).
pub fn marks(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let session = session.ok_or("no document open")?;
    let edit = session.edit.as_ref().ok_or("no document open")?;
    let texts = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.spell_texts("document", None, 0, None).map_err(|e| e.to_string())?;
    let options: Options = field(req, "options")?;
    let users: UserDictionaries = field(req, "userDictionaries")?;
    let default_language = req["defaultLanguage"].as_str().unwrap_or("English: USA");
    let page = req["page"].as_u64().map(|p| p as usize);
    let mut marks = Vec::new();
    for story in &texts {
        let runs: Vec<Run> = story.runs.iter().map(|r| Run { start: r.start, end: r.end, language: r.language.clone() }).collect();
        for issue in check_text(&Bundled, &story.text, &runs, default_language, &options, &users, &[], false) {
            if !matches!(issue.kind, IssueKind::Misspelled | IssueKind::UncapitalizedWord) {
                continue;
            }
            for mut seg in crate::text_edit::baseline_segments(&session.doc, &session.layout, &story.story, issue.start, issue.end) {
                if page.is_some_and(|p| seg["page"].as_u64() != Some(p as u64)) {
                    continue;
                }
                seg["story"] = json!(story.story);
                seg["start"] = json!(issue.start);
                seg["end"] = json!(issue.end);
                marks.push(seg);
            }
        }
    }
    Ok(json!({"marks": marks}))
}

/// `{"cmd":"spellSuggest","language":…,"word":…,"limit":8}` → `{"suggestions":[…]}`
pub fn suggestions(req: &Value) -> Result<Value, String> {
    let word = req["word"].as_str().ok_or("word required")?;
    let language = req["language"].as_str().unwrap_or("English: USA");
    let limit = req["limit"].as_u64().unwrap_or(8) as usize;
    Ok(json!({"suggestions": suggest(&Bundled, language, word, limit)}))
}

/// The languages Folio can check and keep a user dictionary for. InDesign has 60 user dictionaries
/// and 38 autocorrect languages; Folio has the two it ships dictionaries for.
/// `{"cmd":"spellLanguages"}` → `{"languages":[{"name","code","checks":true}]}`
pub fn languages() -> Value {
    let rows: Vec<Value> = [Lang::EnUs, Lang::KoKr].iter().map(|l| json!({"name": l.name(), "code": l.code(), "checks": true})).collect();
    json!({"languages": rows})
}

/// Edit the user dictionaries (the person's list of added and excluded words) and give them back.
/// `{"cmd":"spellUserDictionary","op":"add"|"remove"|"exclude"|"include"|"get","language":…,
///   "words":[…],"caseSensitive":bool (add only),"userDictionaries":{…}}` → `{"userDictionaries":{…},"language":…,"addedWords":[…],"removedWords":[…]}`
pub fn user_dictionary(req: &Value) -> Result<Value, String> {
    let mut all: UserDictionaries = field(req, "userDictionaries")?;
    let name = user::language_key(req["language"].as_str().ok_or("language required")?).to_owned();
    if Lang::from_name(&name).is_none() {
        return Err(format!("no dictionary for {name}"));
    }
    let words: Vec<String> = field(req, "words")?;
    let entry = all.entry(name.clone()).or_default();
    match req["op"].as_str().ok_or("op required")? {
        // Check Spelling's Add with "Case Sensitive" on keeps the word as typed.
        "add" if req["caseSensitive"].as_bool() == Some(true) => entry.add_words_exact(&words),
        "add" => entry.add_words(&words),
        "remove" => entry.remove_words(&words),
        "exclude" => entry.exclude_words(&words),
        "include" => entry.include_words(&words),
        "get" => {}
        other => return Err(format!("unknown op {other}")),
    }
    let (added, removed) = (entry.added_words.clone(), entry.removed_words.clone());
    all.retain(|_, d| !d.is_empty());
    Ok(json!({"userDictionaries": all, "language": name, "addedWords": added, "removedWords": removed}))
}

/// Edit the autocorrect tables, or ask what a finished word becomes.
/// `{"cmd":"spellAutoCorrect","op":"add"|"remove"|"apply","language":…,"pairs":[["teh","the"]],
///   "tables":{…},"word":…,"startsSentence":bool,"autoCorrectCapitalizationErrors":bool}`
/// → `{"tables":{…}}` or `{"replacement":string|null}`
pub fn auto_correct(req: &Value) -> Result<Value, String> {
    let mut tables: autocorrect::Tables = field(req, "tables")?;
    let name = user::language_key(req["language"].as_str().ok_or("language required")?).to_owned();
    match req["op"].as_str().ok_or("op required")? {
        "add" => {
            let pairs: Vec<autocorrect::Pair> = field(req, "pairs")?;
            autocorrect::add_pairs(tables.entry(name).or_default(), &pairs);
            Ok(json!({"tables": tables}))
        }
        "remove" => {
            let typos: Vec<String> = field(req, "words")?;
            if let Some(rows) = tables.get_mut(&name) {
                rows.retain(|(typo, _)| !typos.contains(typo));
            }
            Ok(json!({"tables": tables}))
        }
        "apply" => {
            let word = req["word"].as_str().ok_or("word required")?;
            let rows = tables.get(&name).map(Vec::as_slice).unwrap_or(&[]);
            let replacement = autocorrect::apply(
                rows,
                word,
                req["autoCorrectCapitalizationErrors"].as_bool().unwrap_or(false),
                req["startsSentence"].as_bool().unwrap_or(false),
            );
            Ok(json!({"replacement": replacement}))
        }
        other => Err(format!("unknown op {other}")),
    }
}

/// What typing `text` at `at` in a story would correct (Edit > Spelling > Autocorrect), so the screen knows where the caret goes before it
/// sends the `typeText` edit command; nothing is written. Words typed by a script, a paste or `insertText` are never corrected.
/// `{"cmd":"spellTyped","story":…,"at":n,"text":…,"tables":{"English: USA":[["teh","the"]]},"autoCorrectCapitalizationErrors":bool,"autoCorrect":bool,"typedRun":n}`
/// → `{"fix":{"start","end","replacement"}|null,"run":n}` (the characters `start..end` of the story become `replacement`, which holds the typed text;
/// `run` is the number of typed characters in the word before the caret afterwards, to hand back with the next typed text)
pub fn typed(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let edit = session.and_then(|s| s.edit.as_ref()).ok_or("no document open")?;
    let tables: std::collections::BTreeMap<String, Vec<Vec<String>>> = field(req, "tables")?;
    let story = req["story"].as_str().ok_or("story required")?;
    let at = req["at"].as_u64().ok_or("at required")? as usize;
    let text = req["text"].as_str().ok_or("text required")?;
    let caps = req["autoCorrectCapitalizationErrors"].as_bool().unwrap_or(false);
    // `autoCorrect` (default yes): the Autocorrect switch; `typedRun` (default 1): how many characters of the word before the caret were typed
    // just before (a word that a script or a paste brought has none and is left alone)
    let auto_correct = req["autoCorrect"].as_bool().unwrap_or(true);
    let run_in = req["typedRun"].as_u64().unwrap_or(1);
    let fix = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.typed_fix(story, at, text, &tables, caps, auto_correct, run_in > 0).map_err(|e| e.to_string())?;
    // the count after this text: a character that finishes a word starts it again at nothing
    let run = text.chars().fold(run_in, |run, c| if autocorrect::ends_word(c) { 0 } else { run + 1 });
    Ok(json!({"fix": fix, "run": run}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_dictionary_ops_return_the_whole_settings_value() {
        let r = user_dictionary(&json!({"op":"add","language":"$ID/English: USA","words":["Folio Studio","zeta"],"userDictionaries":{}})).unwrap();
        assert_eq!(r["addedWords"], json!(["folio", "studio", "zeta"]));
        let r = user_dictionary(&json!({"op":"remove","language":"English: USA","words":["zeta","Studio"],"userDictionaries":r["userDictionaries"]})).unwrap();
        assert_eq!(r["addedWords"], json!(["folio", "studio"]));
        let r = user_dictionary(&json!({"op":"remove","language":"English: USA","words":["folio","studio"],"userDictionaries":r["userDictionaries"]})).unwrap();
        // an emptied dictionary is not kept in the settings
        assert_eq!(r["userDictionaries"], json!({}));
        assert!(user_dictionary(&json!({"op":"add","language":"Telugu (India)","words":["x"]})).is_err());
        assert!(user_dictionary(&json!({"op":"fly","language":"Korean"})).is_err());
    }

    #[test]
    fn auto_correct_ops_edit_tables_and_apply_them() {
        let r = auto_correct(&json!({"op":"add","language":"English: USA","pairs":[["teh","the"],["teh","tea"]],"tables":{}})).unwrap();
        assert_eq!(r["tables"], json!({"English: USA": [["teh", "the"]]}));
        let a = auto_correct(&json!({"op":"apply","language":"English: USA","word":"teh","tables":r["tables"],"startsSentence":true,"autoCorrectCapitalizationErrors":true})).unwrap();
        assert_eq!(a["replacement"], json!("The"));
        let a = auto_correct(&json!({"op":"apply","language":"English: USA","word":"teh","tables":r["tables"]})).unwrap();
        assert_eq!(a["replacement"], json!("the"));
        let r = auto_correct(&json!({"op":"remove","language":"English: USA","words":["teh"],"tables":r["tables"]})).unwrap();
        assert_eq!(r["tables"], json!({"English: USA": []}));
    }

    #[test]
    fn suggestions_come_from_the_bundled_dictionaries() {
        let r = suggestions(&json!({"word":"wrold","language":"English: USA"})).unwrap();
        assert!(r["suggestions"].as_array().unwrap().iter().any(|w| w == "world"));
        assert_eq!(languages()["languages"].as_array().unwrap().len(), 2);
    }
}
