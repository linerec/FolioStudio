//! Edit ▸ Preferences on the shared dialog frame (`next/shared/dialogs/preferences.json`, written by `next/tools/gen-preferences-dialog.py`).
//!
//! One dialog, every category a tab. It opens in a document's scope when one is open for editing and in the app's otherwise (the
//! declaration's `"scope": "either"`). Its fields are of three owners, and OK sends what the screens sent for each:
//! - a view setting (`view`): the document's units, grids, guides and pasteboard, one `setViewPref` each for what changed;
//! - a document preference (`docPref`, its type, default and range those of `folio_idml::docprefs`): one `setDocPrefs` of what changed,
//!   and the dictionary's quotation marks (`languageQuotes` rows), one `setLanguageQuotes` per language that changed;
//! - an app setting (every other field, and the autocorrect word lists): one `userSettings` patch of `preferences` (the settings the
//!   app has, with the dialog's over them) and `autoCorrectTables`.
//!
//! The document's part is one edit (one step to undo); the app settings are saved after it. Without a document only those are saved.
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};

const PASTEBOARD: [&str; 3] = ["pasteboardHorizontal", "pasteboardVertical", "matchPreviewBackground"];

fn fields(def: &Value) -> impl Iterator<Item = &Value> { def["fields"].as_array().into_iter().flatten() }
fn name(f: &Value) -> &str { f["name"].as_str().unwrap_or("") }
fn is_app(f: &Value) -> bool { f["document"] != true && f["name"] != "autoCorrect" }

/// The declaration as the frame shows it: the document preferences typed from the engine's own list, the shared option lists in place.
pub(super) fn filled(mut def: Value) -> Value {
    use folio_idml::docprefs::{self, Kind};
    let lists = def.as_object_mut().and_then(|d| d.remove("lists")).unwrap_or(Value::Null);
    for f in def["fields"].as_array_mut().into_iter().flatten() {
        if let Some(list) = f["options"].as_str().map(str::to_owned) { f["options"] = lists[&list].clone(); }
        if f["docPref"] != true { continue; }
        let pref = docprefs::field(name(f)).unwrap_or_else(|e| panic!("preferences dialog: {e}"));
        match pref.kind {
            Kind::Switch => { f["type"] = json!("boolean"); f["default"] = json!(pref.initial == "true"); }
            Kind::Number(lo, hi) => {
                f["type"] = json!("number"); f["min"] = json!(lo); f["max"] = json!(hi);
                f["default"] = json!(pref.initial.parse::<f64>().unwrap_or(0.));
            }
            Kind::Choice(names) => {
                f["type"] = json!("enum"); f["default"] = json!(pref.initial);
                // the labels the declaration gives, for exactly the engine's choices
                let labelled: Vec<Value> = names.iter().map(|n| f["options"].as_array().and_then(|o| o.iter().find(|x| x["value"] == *n)).cloned()
                    .unwrap_or_else(|| json!({"value": n, "label": n}))).collect();
                f["options"] = Value::Array(labelled);
            }
        }
    }
    def
}

/// A view colour as the dialog holds it: the interface colour's name, or `*custom` for a colour of its own.
fn colour(name: &Value) -> Value { if name.is_string() { name.clone() } else { json!("*custom") } }

/// The values the dialog opens with: the app settings, then (with a document) the document's preferences, views and languages.
pub(super) fn defaults(def: &Value, settings: &Value, info: Option<&Value>) -> Map<String, Value> {
    let mut values = Map::new();
    for f in fields(def).filter(|f| is_app(f)) {
        if let Some(v) = settings["preferences"].get(name(f)) { values.insert(name(f).into(), v.clone()); }
    }
    values.insert("autoCorrect".into(), autocorrect_rows(def, &settings["autoCorrectTables"]));
    let Some(info) = info else { return values };
    for f in fields(def).filter(|f| f["docPref"] == true) {
        if let Some(v) = info["documentPreferences"].get(name(f)) { values.insert(name(f).into(), v.clone()); }
    }
    let (view, p, b, g) = (&info["view"], &info["view"]["pasteboard"], &info["view"]["baseline"], &info["view"]["grid"]);
    let seen = [
        ("rulerOrigin", info["rulerOrigin"].clone()), ("horizontalUnits", info["units"]["horizontal"].clone()), ("verticalUnits", info["units"]["vertical"].clone()),
        ("strokeUnits", view["strokeUnits"].clone()), ("cursorKeyIncrement", view["cursorKeyIncrement"].clone()), ("leadingKeyIncrement", view["leadingKeyIncrement"].clone()),
        ("baselineShiftKeyIncrement", view["baselineShiftKeyIncrement"].clone()), ("kerningKeyIncrement", view["kerningKeyIncrement"].clone()),
        ("baselineColor", colour(&b["colorName"])), ("baselineStart", b["start"].clone()), ("baselineDivision", b["division"].clone()),
        ("baselineThreshold", b["threshold"].clone()), ("baselineRelativeTo", json!(if b["relativeToMargin"] == true { "margin" } else { "page" })),
        ("gridColor", colour(&g["colorName"])), ("horizontalDivision", g["horizontal"].clone()), ("horizontalSubdivision", g["horizontalSub"].clone()),
        ("verticalDivision", g["vertical"].clone()), ("verticalSubdivision", g["verticalSub"].clone()), ("gridsInBack", view["gridsInBack"].clone()),
        ("rulerGuidesColor", colour(&view["rulerGuides"]["colorName"])), ("previewBackgroundColor", colour(&p["previewColorName"])),
        ("bleedGuideColor", colour(&p["bleedColorName"])), ("slugGuideColor", colour(&p["slugColorName"])), ("snapZone", view["snapZone"].clone()),
        ("rulerGuidesThreshold", view["rulerGuides"]["threshold"].clone()), ("guidesInBack", view["guidesInBack"].clone()),
        // with Match Preview Background on, InDesign keeps a negative horizontal margin; the box shows none
        ("pasteboardHorizontal", json!(p["horizontal"].as_f64().unwrap_or(0.).max(0.))), ("pasteboardVertical", p["vertical"].clone()),
        ("matchPreviewBackground", p["matchPreviewBackground"].clone()),
    ];
    for (key, value) in seen { if !value.is_null() { values.insert(key.into(), value); } }
    let rows: Vec<Value> = info["languageQuotes"].as_array().into_iter().flatten()
        .map(|l| json!({"id": l["id"], "name": l["name"], "singleQuotes": l["singleQuotes"], "doubleQuotes": l["doubleQuotes"]})).collect();
    values.insert("languageQuotes".into(), Value::Array(rows));
    values
}

/// The autocorrect word lists as rows (language, word, correction): languages the dialog knows, pairs of text, each word once.
fn autocorrect_rows(def: &Value, tables: &Value) -> Value {
    let languages: Vec<&str> = fields(def).find(|f| f["name"] == "autoCorrect").and_then(|f| f["fields"].as_array())
        .and_then(|subs| subs.iter().find(|s| s["name"] == "language")).and_then(|s| s["options"].as_array())
        .map(|o| o.iter().filter_map(|x| x["value"].as_str()).collect()).unwrap_or_default();
    let mut rows = vec![];
    for (language, pairs) in tables.as_object().into_iter().flatten().filter(|(l, _)| languages.contains(&l.as_str())) {
        let mut seen = std::collections::HashSet::new();
        for pair in pairs.as_array().into_iter().flatten() {
            let Some([word, correction]) = pair.as_array().map(Vec::as_slice) else { continue };
            let (Some(w), Some(c)) = (word.as_str(), correction.as_str()) else { continue };
            if seen.insert(w.to_owned()) { rows.push(json!({"language": language, "word": w, "correction": c})); }
        }
    }
    Value::Array(rows)
}

/// Whether a field's value differs from the one it opened with (numbers as numbers: 12 and 12.0 are the same length).
fn differs(before: &Value, now: &Value) -> bool {
    match (before.as_f64(), now.as_f64()) { (Some(a), Some(b)) => (a - b).abs() > 1e-6, _ => before != now }
}

/// The document's part of OK: the edit commands the Units, Grids and Guides dialogs, the dictionary's quotation marks and the
/// document preferences sent for what changed, in that order (one edit, one step to undo).
pub(super) fn edits(def: &Value, values: &Value, initial: &Value) -> Result<Vec<Value>, Refusal> {
    let changed = |key: &str| differs(&initial[key], &values[key]);
    let mut out = vec![];
    for f in fields(def).filter(|f| f["view"] == true && !PASTEBOARD.contains(&name(f))) {
        if changed(name(f)) { out.push(json!({"op": "setViewPref", "key": name(f), "value": values[name(f)]})); }
    }
    // Match Preview Background to Theme Color zeroes the margins; the margins are sent only while it stays off (oracle: pages-pasteboard-prefs)
    if changed("matchPreviewBackground") {
        out.push(json!({"op": "setViewPref", "key": "matchPreviewBackground", "value": values["matchPreviewBackground"]}));
    } else if values["matchPreviewBackground"] == false && (changed("pasteboardHorizontal") || changed("pasteboardVertical")) {
        out.push(json!({"op": "setViewPref", "key": "pasteboardMargins", "value": [values["pasteboardHorizontal"], values["pasteboardVertical"]]}));
    }
    let before = initial["languageQuotes"].as_array().cloned().unwrap_or_default();
    for (i, row) in values["languageQuotes"].as_array().into_iter().flatten().enumerate() {
        let was = before.iter().find(|b| b["id"] == row["id"]).cloned().unwrap_or(Value::Null);
        let mut props = Map::new();
        for (key, attr) in [("singleQuotes", "SingleQuotes"), ("doubleQuotes", "DoubleQuotes")] {
            if row[key] == was[key] { continue; }
            let text = row[key].as_str().unwrap_or("");
            if text.chars().count() != 2 {
                return Err(refuse(&format!("languageQuotes/{i}/{key}"), "quotePair", format!("{key} of {} must be one opening and one closing mark", row["name"])));
            }
            props.insert(attr.into(), json!(text));
        }
        if !props.is_empty() { out.push(json!({"op": "setLanguageQuotes", "language": row["id"], "props": props})); }
    }
    let props: Map<String, Value> = fields(def).filter(|f| f["docPref"] == true && changed(name(f)))
        .map(|f| (name(f).to_owned(), values[name(f)].clone())).collect();
    if !props.is_empty() { out.push(json!({"op": "setDocPrefs", "props": props})); }
    Ok(out)
}

/// The app's part of OK: the settings the app has (`settings`, read when the dialog is applied) with the dialog's app fields over
/// them, and the autocorrect word lists (by language, sorted by word), as one userSettings patch, the request the screen sent.
pub(super) fn app_requests(def: &Value, values: &Value, settings: &Value) -> Result<Vec<Value>, Refusal> {
    let mut preferences = settings["preferences"].as_object().cloned().unwrap_or_default();
    for f in fields(def).filter(|f| is_app(f)) { preferences.insert(name(f).into(), values[name(f)].clone()); }
    let mut tables: Map<String, Value> = Map::new();
    let mut lists: std::collections::BTreeMap<String, Vec<(String, String)>> = Default::default();
    for (i, row) in values["autoCorrect"].as_array().into_iter().flatten().enumerate() {
        let (language, word, correction) = (row["language"].as_str().unwrap_or(""), row["word"].as_str().unwrap_or(""), row["correction"].as_str().unwrap_or(""));
        for (key, text) in [("word", word), ("correction", correction)] {
            if text.is_empty() { return Err(refuse(&format!("autoCorrect/{i}/{key}"), "required", format!("autoCorrect/{i}/{key} is required"))); }
        }
        lists.entry(language.into()).or_default().push((word.into(), correction.into()));
    }
    for (language, mut pairs) in lists {
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        tables.insert(language, pairs.into_iter().map(|(w, c)| json!([w, c])).collect());
    }
    Ok(vec![json!({"cmd": "userSettings", "patch": {"preferences": preferences, "autoCorrectTables": tables}})])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn def() -> Value { super::super::definition("preferences").ok().unwrap() }

    #[test]
    fn the_declaration_is_what_the_generator_writes_and_types_the_document_preferences_from_the_engine() {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/gen-preferences-dialog.py");
        if let Ok(out) = std::process::Command::new("python").args(["-B", "-I"]).arg(&script).arg("--check").status() {
            assert!(out.success(), "next/shared/dialogs/preferences.json is not what gen-preferences-dialog.py writes");
        }
        let def = def();
        let size = def["fields"].as_array().unwrap().iter().find(|f| f["name"] == "TextPreference.SuperscriptSize").unwrap().clone();
        assert_eq!((size["type"].clone(), size["min"].clone(), size["max"].clone(), size["default"].clone()), (json!("number"), json!(1.0), json!(200.0), json!(58.3)));
        let pages = def["fields"].as_array().unwrap().iter().find(|f| f["name"] == "TextPreference.AddPages").unwrap().clone();
        assert_eq!(pages["options"][1], json!({"value":"EndOfSection","label":"섹션 끝"}));
        let colour = def["fields"].as_array().unwrap().iter().find(|f| f["name"] == "spellPreferences.misspelledWordColor").unwrap().clone();
        assert!(def["lists"].is_null() && colour["options"].as_array().unwrap().iter().any(|o| o["value"] == "LIGHT_BLUE"));
        // every field is on a tab, every tab names the menu command that opens it
        let tabs = def["tabs"].as_array().unwrap();
        assert_eq!(tabs.len(), 19);
        assert_eq!(tabs[0]["command"], "edit.preferences");
        assert_eq!(tabs.iter().filter(|t| t["document"] == true).count(), 5);
    }

    #[test]
    fn ok_sends_what_changed_as_the_screens_did_and_the_app_settings_as_one_patch() {
        let def = def();
        let info = json!({
            "rulerOrigin": "SpreadOrigin", "units": {"horizontal": "Points", "vertical": "Points"},
            "documentPreferences": {"TextPreference.SuperscriptSize": 58.3, "TextPreference.TypographersQuotes": true},
            "languageQuotes": [{"id": "Language/$ID/English%3a USA", "name": "English: USA", "singleQuotes": "‘’", "doubleQuotes": "“”"}],
            "view": {"strokeUnits": "Points", "cursorKeyIncrement": 1, "leadingKeyIncrement": 2, "baselineShiftKeyIncrement": 2, "kerningKeyIncrement": 20,
                "snapZone": 4, "guidesInBack": false, "gridsInBack": true, "rulerGuides": {"threshold": 5, "colorName": "Cyan"},
                "pasteboard": {"horizontal": -1, "vertical": 72, "matchPreviewBackground": false, "previewColorName": null, "bleedColorName": "Fiesta", "slugColorName": "GridBlue"},
                "baseline": {"start": 36, "division": 12, "threshold": 75, "relativeToMargin": false, "colorName": "LightBlue"},
                "grid": {"horizontal": 72, "vertical": 72, "horizontalSub": 8, "verticalSub": 8, "colorName": "LightGray"}},
        });
        let settings = json!({"preferences": {"folio.view.guides": "true", "notePreferences.showNoteTips": true},
            "autoCorrectTables": {"English: USA": [["teh", "the"], ["teh", "dup"], [1, 2]], "Klingon": [["a", "b"]]}});
        // as the dialog opens: what the settings and the document have, the declaration's defaults for the rest
        let mut initial = Value::Object(defaults(&def, &settings, Some(&info)));
        for f in def["fields"].as_array().unwrap() { if initial.get(name(f)).is_none() { initial[name(f)] = f["default"].clone(); } }
        assert_eq!(initial["previewBackgroundColor"], "*custom");
        assert_eq!(initial["pasteboardHorizontal"], json!(0.0));
        assert_eq!(initial["autoCorrect"], json!([{"language":"English: USA","word":"teh","correction":"the"}]));
        assert!(edits(&def, &initial, &initial).ok().unwrap().is_empty(), "nothing changed, nothing sent to the document");
        let mut values = initial.clone();
        values["cursorKeyIncrement"] = json!(2.5);
        values["kerningKeyIncrement"] = json!(20.0);
        values["horizontalUnits"] = json!("Millimeters");
        values["gridColor"] = json!("Red");
        values["pasteboardVertical"] = json!(36);
        values["languageQuotes"][0]["doubleQuotes"] = json!("«»");
        values["TextPreference.SuperscriptSize"] = json!(60);
        values["notePreferences.showNoteTips"] = json!(false);
        values["autoCorrect"] = json!([{"language":"English: USA","word":"zz","correction":"z"},{"language":"English: USA","word":"adn","correction":"and"}]);
        assert_eq!(edits(&def, &values, &initial).ok().unwrap(), vec![
            json!({"op":"setViewPref","key":"horizontalUnits","value":"Millimeters"}),
            json!({"op":"setViewPref","key":"cursorKeyIncrement","value":2.5}),
            json!({"op":"setViewPref","key":"gridColor","value":"Red"}),
            json!({"op":"setViewPref","key":"pasteboardMargins","value":[0.0, 36]}),
            json!({"op":"setLanguageQuotes","language":"Language/$ID/English%3a USA","props":{"DoubleQuotes":"«»"}}),
            json!({"op":"setDocPrefs","props":{"TextPreference.SuperscriptSize":60}}),
        ]);
        for command in edits(&def, &values, &initial).ok().unwrap() { serde_json::from_value::<folio_edit::Command>(command).unwrap(); }
        let app = app_requests(&def, &values, &settings).ok().unwrap();
        assert_eq!(app.len(), 1);
        assert_eq!(app[0]["cmd"], "userSettings");
        let prefs = &app[0]["patch"]["preferences"];
        assert_eq!((prefs["folio.view.guides"].clone(), prefs["notePreferences.showNoteTips"].clone(), prefs["folio.theme"].clone()), (json!("true"), json!(false), json!("dark")));
        assert!(prefs.get("TextPreference.SuperscriptSize").is_none() && prefs.get("cursorKeyIncrement").is_none(), "document settings stay in the document");
        assert_eq!(app[0]["patch"]["autoCorrectTables"], json!({"English: USA": [["adn","and"],["zz","z"]]}));
        // turning Match Preview Background on sends it alone; a quotation pair is two marks; a word needs its correction
        values["matchPreviewBackground"] = json!(true);
        assert!(edits(&def, &values, &initial).ok().unwrap().contains(&json!({"op":"setViewPref","key":"matchPreviewBackground","value":true})));
        values["languageQuotes"][0]["singleQuotes"] = json!("'");
        let bad = edits(&def, &values, &initial).err().unwrap();
        assert_eq!((bad.field.as_deref(), bad.reason), (Some("languageQuotes/0/singleQuotes"), "quotePair"));
        values["autoCorrect"][1]["correction"] = json!("");
        assert_eq!(app_requests(&def, &values, &settings).err().unwrap().field.as_deref(), Some("autoCorrect/1/correction"));
        // without a document only the app settings are there to start from
        let alone = defaults(&def, &settings, None);
        assert!(alone.get("cursorKeyIncrement").is_none() && alone.get("notePreferences.showNoteTips") == Some(&json!(true)));
    }
}
