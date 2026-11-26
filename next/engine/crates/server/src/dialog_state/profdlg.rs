//! Preflight panel > Define Profiles on the shared dialog frame (`preflightProfiles`): the document's profiles are held whole (`profiles`)
//! while one of them (`profile`, its place in the list) is in the boxes; a box for each rule of the engine's list (on = checked) under its
//! category with the category's own box (all its rules), and one for each of its numbers. Picking another profile puts the boxes back
//! into the list first; OK sends the whole list with the working profile (`setPreflightProfiles`, as the screen sent it). Load and Export
//! read and write a profile file of the project (`path`).
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};

const CATEGORIES: [(&str, &str); 5] = [("links", "링크"), ("text", "텍스트"), ("color", "색상"), ("objects", "개체"), ("document", "문서")];
const PARAMETERS: [(&str, &str); 8] = [("image_ppi_warn", "이미지 해상도 경고 (ppi)"), ("image_ppi_error", "이미지 해상도 오류 (ppi)"),
    ("bitmap_ppi_warn", "비트맵 해상도 경고 (ppi)"), ("bitmap_ppi_error", "비트맵 해상도 오류 (ppi)"), ("max_ink", "최대 잉크 총량 (%)"),
    ("small_text_pt", "작은 글자 기준 (pt)"), ("hairline_pt", "가는 선 기준 (pt)"), ("safety_margin_pt", "재단 안전 여백 (pt)")];
/// The group the numbers are in.
const NUMBERS: &str = "기준값";

fn schema() -> Value { folio_preflight::profiles::schema() }
fn rules() -> Vec<Value> { schema()["rules"].as_array().cloned().unwrap_or_default() }

/// The dialog with the boxes the engine's rule list makes, put before the file path box.
pub(super) fn filled(mut def: Value) -> Value {
    let schema = schema();
    let mut more = vec![];
    for (category, label) in CATEGORIES {
        let listed: Vec<&Value> = schema["rules"].as_array().into_iter().flatten().filter(|r| r["category"] == category).collect();
        if listed.is_empty() { continue; }
        more.push(json!({"name": format!("category.{category}"), "label": label, "type": "boolean", "default": true, "group": label, "local": true}));
        for rule in listed {
            let id = rule["id"].as_str().unwrap_or("");
            more.push(json!({"name": format!("rule.{id}"), "label": rule["title"].as_str().unwrap_or(id), "type": "boolean", "default": true, "group": label}));
        }
    }
    for (key, value) in schema["parameters"].as_object().into_iter().flatten() {
        let label = PARAMETERS.iter().find(|(k, _)| k == key).map_or(key.as_str(), |(_, l)| l);
        more.push(json!({"name": format!("param.{key}"), "label": label, "type": "number", "min": 0, "max": 1e9, "default": value, "group": NUMBERS}));
    }
    if let Some(fields) = def["fields"].as_array_mut() {
        let at = fields.iter().position(|f| f["name"] == "path").unwrap_or(fields.len());
        fields.splice(at..at, more);
    }
    def
}

/// The profiles the dialog starts with: the document's, or one new profile when it has none (as the screen began one).
fn listed(s: &crate::Session, requested: &Value) -> Vec<Value> {
    let mut list: Vec<Value> = s.doc.preflight_profiles.profiles.iter().map(|p| json!(p)).collect();
    if list.is_empty() { list.push(blank(&list, requested)); }
    list
}

/// A new profile: every rule on, the engine's numbers, named `namePrefix` (the first free of it, "… 2", …).
fn blank(list: &[Value], values: &Value) -> Value {
    let prefix = values["namePrefix"].as_str().filter(|t| !t.trim().is_empty()).unwrap_or("New Preflight Profile");
    let name = std::iter::once(prefix.to_owned()).chain((2..).map(|n| format!("{prefix} {n}"))).find(|n| list.iter().all(|p| p["name"] != n.as_str())).unwrap_or_default();
    json!({"name": name, "disabled_rules": [], "parameters": schema()["parameters"]})
}

/// `preflightProfileList`: the profiles by their places in the list.
pub(super) fn choices(s: &crate::Session, requested: &Value) -> Vec<Value> { labelled(&listed(s, requested)) }
fn labelled(list: &[Value]) -> Vec<Value> { list.iter().enumerate().map(|(i, p)| json!({"value": i.to_string(), "label": p["name"]})).collect() }

/// `preflightProfiles`: the document's profiles and working profile, with the working one (or the first) in the boxes.
pub(super) fn defaults(s: &crate::Session, requested: &Value) -> Result<(Map<String, Value>, Value), Refusal> {
    let state = &s.doc.preflight_profiles;
    let list = listed(s, requested);
    let at = state.profiles.iter().position(|p| p.name == state.active).unwrap_or(0);
    let mut values = json!({"profiles": list, "active": state.active, "profile": at.to_string()});
    load(&mut values, at);
    Ok((values.as_object().cloned().unwrap_or_default(), json!({})))
}

/// The boxes from the profile at `at` of the list.
pub(super) fn load(values: &mut Value, at: usize) {
    let profile = values["profiles"][at].clone();
    values["name"] = profile["name"].clone();
    values["useAsActive"] = json!(values["active"] == profile["name"]);
    let off: Vec<Value> = profile["disabled_rules"].as_array().cloned().unwrap_or_default();
    for rule in rules() { values[format!("rule.{}", rule["id"].as_str().unwrap_or(""))] = json!(!off.contains(&rule["id"])); }
    for (category, _) in CATEGORIES { categorised(values, category); }
    for (key, value) in schema()["parameters"].as_object().into_iter().flatten() {
        values[format!("param.{key}")] = profile["parameters"].get(key).filter(|v| v.is_number()).unwrap_or(value).clone();
    }
}

/// A category's box: on while every rule of it is.
fn categorised(values: &mut Value, category: &str) {
    let on = rules().iter().filter(|r| r["category"] == category).all(|r| values[format!("rule.{}", r["id"].as_str().unwrap_or(""))] == true);
    values[format!("category.{category}")] = json!(on);
}

/// The profile the boxes make (the rules turned off keep the order the profile had, new ones after; its other numbers kept).
fn draft(values: &Value, at: usize) -> Value {
    let was = &values["profiles"][at];
    let off = |id: &Value| values[format!("rule.{}", id.as_str().unwrap_or(""))] == false;
    let mut disabled: Vec<Value> = was["disabled_rules"].as_array().into_iter().flatten().filter(|id| off(id) || !rules().iter().any(|r| &r["id"] == *id)).cloned().collect();
    for rule in rules() { if off(&rule["id"]) && !disabled.contains(&rule["id"]) { disabled.push(rule["id"].clone()); } }
    let mut parameters = was["parameters"].as_object().cloned().unwrap_or_default();
    for key in schema()["parameters"].as_object().into_iter().flatten().map(|(k, _)| k) { parameters.insert(key.clone(), values[format!("param.{key}")].clone()); }
    json!({"name": values["name"], "disabled_rules": disabled, "parameters": parameters})
}

fn place(values: &Value) -> usize { values["profile"].as_str().and_then(|p| p.parse().ok()).unwrap_or(0) }

/// The boxes kept in step after `changed` was set (`link: "preflightProfiles"`): the working profile follows the name; a category's box
/// sets its rules and a rule its category's; picking another profile (or none: a new one) puts the boxes back into the list first and
/// fills them from it; `import` reads the profile file `path` into a new place of the list, `export` writes the one in the boxes there.
pub(super) fn linked(before: &Value, values: &mut Value, choices: &mut Value, changed: &str, s: Option<&crate::Session>) -> Result<(), Refusal> {
    let at = place(before);
    match changed {
        "name" => {
            if before["active"] == before["name"] && before["active"] != "" { values["active"] = values["name"].clone(); }
            values["useAsActive"] = json!(values["active"] == values["name"]);
            values["profiles"][at]["name"] = values["name"].clone();
        }
        "useAsActive" => values["active"] = if values["useAsActive"] == true { values["name"].clone() } else { json!("") },
        key if key.starts_with("category.") => {
            let category = &key["category.".len()..];
            let on = values[key].clone();
            for rule in rules().iter().filter(|r| r["category"] == category) { values[format!("rule.{}", rule["id"].as_str().unwrap_or(""))] = on.clone(); }
        }
        key if key.starts_with("rule.") => {
            let id = json!(&key["rule.".len()..]);
            if let Some(category) = rules().iter().find(|r| r["id"] == id).and_then(|r| r["category"].as_str().map(str::to_owned)) { categorised(values, &category); }
        }
        "profile" => {
            values["profiles"][at] = draft(before, at);
            if values["profile"] == "" { add(values, None); } else { load(values, place(values)); }
        }
        "import" if values["import"] == true => {
            values["import"] = json!(false);
            let read = crate::preflight_files::file(&json!({"action": "import", "path": values["path"]}), s).map_err(|e| refuse("path", "fileFailed", e))?;
            values["profiles"][at] = draft(before, at);
            add(values, Some(read["profile"].clone()));
        }
        "export" if values["export"] == true => {
            values["export"] = json!(false);
            if values["name"].as_str().is_none_or(|n| n.trim().is_empty()) { return Err(refuse("name", "required", "name is required".into())); }
            crate::preflight_files::file(&json!({"action": "export", "path": values["path"], "profile": draft(values, at)}), s).map_err(|e| refuse("path", "fileFailed", e))?;
        }
        _ => {}
    }
    let list = values["profiles"].as_array().cloned().unwrap_or_default();
    choices["preflightProfileList"] = json!(labelled(&list));
    Ok(())
}

/// A profile put at the end of the list (a new one when none is given) and into the boxes.
fn add(values: &mut Value, profile: Option<Value>) {
    let mut list = values["profiles"].as_array().cloned().unwrap_or_default();
    let fresh = profile.unwrap_or_else(|| blank(&list, values));
    list.push(fresh);
    let at = list.len() - 1;
    values["profiles"] = json!(list);
    values["profile"] = json!(at.to_string());
    load(values, at);
}

/// OK: the list with the boxes put back, every name given and none twice, and the working profile (`setPreflightProfiles`).
pub(super) fn commands(values: &Value) -> Result<Vec<Value>, Refusal> {
    let at = place(values);
    let mut list = values["profiles"].as_array().cloned().unwrap_or_default();
    if at < list.len() { list[at] = draft(values, at); }
    let mut seen = std::collections::BTreeSet::new();
    for (i, p) in list.iter().enumerate() {
        let name = p["name"].as_str().unwrap_or("");
        let field = if i == at { "name".to_owned() } else { format!("profiles/{i}") };
        if name.trim().is_empty() { return Err(refuse(&field, "required", format!("profile {i} needs a name"))); }
        if !seen.insert(name.to_owned()) { return Err(refuse(&field, "notUnique", format!("two profiles are named {name}"))); }
    }
    Ok(vec![json!({"op": "setPreflightProfiles", "state": {"profiles": list, "active": values["active"]}})])
}
