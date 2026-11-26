//! Edit > Spelling > User Dictionary on the shared dialog frame (`userDictionary`, an app dialog: what it changes is the user settings of
//! this computer, not a document). The dialog holds every language's lists (`userDictionaries`) and shows one language's added and
//! excluded words as rows; every change goes through the engine's dictionary rules (`spellUserDictionary`: lower case, sorted, split at
//! spaces, removal of the exact stored word). OK saves the lists (`userSettings patch userDictionaries`, as the screen saved them).
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};

/// `spellLanguages`: the languages Folio keeps a user dictionary for, by their names (not translated, as the screen listed them).
pub(super) fn choice_list(source: &str) -> Option<Vec<Value>> {
    if source != "spellLanguages" { return None; }
    Some(crate::spell::languages()["languages"].as_array().into_iter().flatten().map(|l| json!({"value": l["name"], "label": l["name"]})).collect())
}

fn ask(op: &str, language: &Value, words: Vec<Value>, all: &Value, field: &str) -> Result<Value, Refusal> {
    crate::spell::user_dictionary(&json!({"op": op, "language": language, "words": words, "userDictionaries": all})).map_err(|e| refuse(field, "refused", e))
}

/// The lists of the language the values name, as rows.
fn shown(values: &mut Value, answer: &Value) {
    values["userDictionaries"] = answer["userDictionaries"].clone();
    for (field, key) in [("added", "addedWords"), ("removed", "removedWords")] {
        values[field] = Value::Array(answer[key].as_array().into_iter().flatten().map(|w| json!({"word": w})).collect());
    }
}

/// `userDictionary`: the user settings' dictionaries, with the lists of the language asked for (English: USA when none is).
pub(super) fn defaults(requested: &Value) -> Result<Map<String, Value>, Refusal> {
    let settings = crate::user_settings::run(&json!({}))?;
    let saved = settings["settings"]["userDictionaries"].clone();
    let all = if saved.is_object() { saved } else { json!({}) };
    let language = requested.get("language").cloned().unwrap_or_else(|| json!("English: USA"));
    let mut values = json!({"language": language});
    // (lists the rules cannot read start empty, as the screen's cleaned ones did)
    let answer = ask("get", &language, vec![], &all, "userDictionaries").or_else(|_| ask("get", &language, vec![], &json!({}), "userDictionaries"))?;
    shown(&mut values, &answer);
    Ok(values.as_object().cloned().unwrap_or_default())
}

fn words(values: &Value, field: &str) -> Vec<Value> { values[field].as_array().into_iter().flatten().map(|r| r["word"].clone()).collect() }

/// The lists kept in step after `changed` was set (`link: "userDictionary"`): another language shows its lists; `addWord` / `excludeWord`
/// add the typed `word` to the added / excluded words (and empty the box); a row removed from a list takes the word out of it, a row added
/// (`row: add`, `{"word": …}`) puts it in, by the engine's rules.
pub(super) fn linked(before: &Value, values: &mut Value, changed: &str) -> Result<(), Refusal> {
    let language = values["language"].clone();
    let mut all = values["userDictionaries"].clone();
    match changed {
        "language" => {}
        "addWord" | "excludeWord" => {
            let on = values[changed] == true;
            values[changed] = json!(false);
            if !on { return Ok(()); }
            if values["word"].as_str().is_none_or(|w| w.trim().is_empty()) { return Err(refuse("word", "required", "word is required".into())); }
            all = ask(if changed == "addWord" { "add" } else { "exclude" }, &language, vec![values["word"].clone()], &all, "word")?["userDictionaries"].take();
            values["word"] = json!("");
        }
        "added" | "removed" => {
            let (was, now) = (words(before, changed), words(values, changed));
            let (take, put) = if changed == "added" { ("remove", "add") } else { ("include", "exclude") };
            let gone: Vec<Value> = was.iter().filter(|w| !now.contains(w)).cloned().collect();
            let new: Vec<Value> = now.iter().filter(|w| !was.contains(w)).cloned().collect();
            if !gone.is_empty() { all = ask(take, &language, gone, &all, changed)?["userDictionaries"].take(); }
            if !new.is_empty() { all = ask(put, &language, new, &all, changed)?["userDictionaries"].take(); }
        }
        _ => return Ok(()),
    }
    let answer = ask("get", &language, vec![], &all, changed)?;
    shown(values, &answer);
    Ok(())
}
