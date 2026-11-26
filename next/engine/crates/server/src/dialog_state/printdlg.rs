//! File ▸ Print (`printing`, a document dialog of the open document's requests) and File ▸ Print Presets ▸ Define (`printPresets`,
//! an app dialog managing the user's print presets) share one field list: a print preset's settings, made from the engine's own
//! list (`folio_pdf::printing::settings::FIELDS`: tab, kind, choices, default) and the label table of `printPresets.json`
//! (`settingFields`). What does not apply to the values is worked out after every change (`off`, `offOptions`: InDesign greys it).
//! The manager edits a list of its own (`saved`) and saves it once, on OK; what the buttons send is what the screen sent for the
//! same values (`userSettings`, `printPresets importFile/exportFile`, `printingFile`, `printToDevice`, `cancelPrintJob`).
use super::{Refusal, refuse};
use folio_pdf::printing::presets::{self, DEFAULT_NAME, PresetOp, PrintPreset};
use folio_pdf::printing::settings::{self, FIELDS, Kind};
use serde_json::{Map, Value, json};

/// Why a setting is off (the screen's text, by its Korean source as every dialog text is).
const NOT_APPLICABLE: &str = "지금 설정에서는 적용되지 않습니다.";
const DOCUMENT_BLEED: &str = "문서 도련 설정을 쓰는 동안에는 바꿀 수 없습니다.";
const BUILT_IN: &str = "[기본값]은 기본 제공이라 바꾸거나 지울 수 없습니다. 복제해서 쓰세요.";

/// The settings' fields of a dialog that has the label table (`settingFields`): one for each of `FIELDS`, in its order, on its tab.
/// A setting without a label is a mistake of the table (the schema test opens every dialog).
pub fn filled(mut def: Value) -> Value {
    let table = def["settingFields"].take();
    let mut fields = def["fields"].as_array().cloned().unwrap_or_default();
    for f in FIELDS {
        let entry = table.as_array().into_iter().flatten().find(|e| e["key"] == f.key)
            .unwrap_or_else(|| panic!("printPresets.json: no settingFields entry for the print setting {}", f.key));
        let mut out = json!({"name": f.key, "label": entry["label"], "tab": f.tab.id(), "group": "", "default": settings::default_value(f)});
        // (a text the dialog offers a list for: the paper sizes, the PPD; an enumerator the table does not name is shown in InDesign's words)
        let listed = |ids: Vec<(&str, &str)>| -> Value {
            ids.into_iter().map(|(id, en)| entry["options"].as_array().into_iter().flatten().find(|o| o["value"] == id).cloned()
                .unwrap_or_else(|| json!({"value": id, "label": en, "native": true}))).collect()
        };
        match f.kind {
            Kind::Bool => out["type"] = json!("boolean"),
            Kind::Int => { out["type"] = json!("number"); out["integer"] = json!(true); out["min"] = json!(1); out["max"] = json!(if f.key == "bitmapResolution" { 1_000_000_000 } else { 9999 }); }
            Kind::Num | Kind::Length => { out["type"] = json!("number"); out["min"] = json!(0); out["max"] = json!(1_000_000_000); }
            Kind::Text if entry["options"].is_array() => { out["type"] = json!("enum"); out["options"] = entry["options"].clone(); }
            Kind::Text => { out["type"] = json!("text"); out["maxLength"] = json!(256); }
            Kind::Enum(options) => { out["type"] = json!("enum"); out["options"] = listed(options.iter().map(|o| (o.id, o.en)).collect()); }
        }
        for (k, v) in entry.as_object().into_iter().flatten().filter(|(k, _)| !matches!(k.as_str(), "key" | "label" | "options")) { out[k] = v.clone(); }
        fields.push(out);
    }
    def["fields"] = Value::Array(fields);
    def
}

fn is_setting(key: &str) -> bool { settings::field(key).is_some() }

/// The dialog's settings fields as one complete, consistent set (what a preset holds and the requests take).
pub fn settings_of(values: &Value) -> Result<Map<String, Value>, Refusal> {
    let picked: Map<String, Value> = FIELDS.iter().filter_map(|f| values.get(f.key).map(|v| (f.key.to_string(), v.clone()))).collect();
    Ok(settings::normalize(&picked)?)
}

/// The fields set from a preset's settings.
fn put(values: &mut Value, settings: &Map<String, Value>) {
    for (k, v) in settings { if is_setting(k) { values[k] = v.clone(); } }
}

/// A saved list as the user settings keep it: what is not a preset is left out (as the screen cleaned it), each one complete.
fn cleaned(list: &Value) -> Vec<PrintPreset> {
    list.as_array().into_iter().flatten()
        .filter_map(|p| serde_json::from_value::<PrintPreset>(p.clone()).ok())
        .filter(|p| p.name != DEFAULT_NAME)
        .filter_map(|p| p.complete().ok())
        .collect()
}

/// The user's saved print presets (app settings `printPresets`).
pub fn user_presets() -> Vec<PrintPreset> {
    cleaned(&crate::user_settings::run(&json!({})).map(|v| v["settings"]["printPresets"].clone()).unwrap_or(Value::Null))
}

/// The list a dialog works with: the manager's own (`saved` of the values), else the user's.
fn list_of(values: &Value) -> Vec<PrintPreset> {
    if values["saved"].is_array() { cleaned(&values["saved"]) } else { user_presets() }
}

/// `printPresets`: [Default] (a screen text) first, then the saved presets by their names.
pub fn choices(requested: &Value) -> Vec<Value> {
    std::iter::once(json!({"value": DEFAULT_NAME, "label": "[기본값]", "translate": true}))
        .chain(list_of(requested).into_iter().map(|p| json!({"value": p.name, "label": p.name}))).collect()
}

/// The fields filled from the listed preset `name` ([Default] included); nothing when it is not listed.
fn fill(values: &mut Value, list: &[PrintPreset], name: &str) -> bool {
    let Ok(found) = presets::find(list, name) else { return false };
    put(values, &found.values);
    true
}

fn manager(values: &Value) -> bool { values.get("saved").is_some() }

/// The manager's values for the preset chosen in it: its settings, its name, whether it is built in.
fn choose(values: &mut Value, list: &[PrintPreset], name: &str) {
    if !fill(values, list, name) { return; }
    values["preset"] = json!(name);
    values["name"] = json!(name);
    values["builtIn"] = json!(name == DEFAULT_NAME);
    values["nameOk"] = json!(true);
}

/// What File ▸ Print Presets ▸ Define starts from: the user's list (as the dialog's own), [Default] shown.
pub fn manager_defaults() -> Map<String, Value> {
    let list = user_presets();
    let mut values = json!({"saved": serde_json::to_value(&list).unwrap_or(json!([]))});
    choose(&mut values, &list, DEFAULT_NAME);
    values.as_object().cloned().unwrap_or_default()
}

/// What File ▸ Print starts from: [Default], the computer's default printer (else the first one listed).
pub fn print_defaults() -> Map<String, Value> {
    let mut values = json!({"preset": DEFAULT_NAME});
    put(&mut values, &settings::defaults());
    let listed = crate::printing::list_printers();
    let printers = listed["printers"].as_array().cloned().unwrap_or_default();
    values["device"] = printers.iter().find(|p| p["isDefault"] == true).or(printers.first()).map_or(json!(""), |p| p["name"].clone());
    values.as_object().cloned().unwrap_or_default()
}

/// After the values asked for: a preset asked for fills the settings, the settings asked for beside it stay on top (Flattener
/// Preview ▸ Apply Settings to Print), pages handed in are the range.
pub fn opened(values: &mut Value, requested: &Value) {
    if manager(values) { return; }
    let list = user_presets();
    if let Some(name) = requested["preset"].as_str() && fill(values, &list, name) {
        for (k, v) in requested.as_object().into_iter().flatten().filter(|(k, _)| is_setting(k)) { values[k] = v.clone(); }
    }
    if let Ok(settings) = settings_of(values) { put(values, &settings); }
    if requested["pageIds"].as_array().is_some_and(|a| !a.is_empty()) && requested.get("range").is_none() { values["range"] = json!("pages"); }
}

/// What is off now: each setting that does not apply (InDesign's "not applicable in the current state"), the four bleed boxes while
/// the document's bleed is used, everything of the built-in [Default] in the manager; and the choices a setting cannot take (a printer
/// that is not a PostScript file cannot separate; "selected pages" without pages).
pub fn off(values: &Value) -> (Map<String, Value>, Map<String, Value>) {
    let mut off = Map::new();
    let mut options = Map::new();
    let Ok(s) = settings_of(values) else { return (off, options) };
    for key in settings::not_applicable(&s).into_iter().filter(|k| is_setting(k)) { off.insert(key.into(), json!(NOT_APPLICABLE)); }
    if s.get("useDocumentBleedToPrint") == Some(&json!(true)) {
        for key in ["bleedTop", "bleedBottom", "bleedInside", "bleedOutside", "bleedChain"] { off.entry(key.to_string()).or_insert(json!(DOCUMENT_BLEED)); }
    }
    if manager(values) && values["builtIn"] == true {
        for key in FIELDS.iter().map(|f| f.key).chain(["name"]) { off.insert(key.into(), json!(BUILT_IN)); }
    }
    for f in FIELDS {
        if let Kind::Enum(list) = f.kind {
            let no: Vec<&str> = list.iter().filter(|o| !settings::allowed(&s, f.key, o.id)).map(|o| o.id).collect();
            if !no.is_empty() { options.insert(f.key.into(), json!(no)); }
        }
    }
    if values.get("pageIds").is_some() && values["pageIds"].as_array().is_none_or(Vec::is_empty) { options.insert("range".into(), json!(["pages"])); }
    (off, options)
}

/// A name the list does not have yet: `base`, else `base 2`, `base 3` …
fn free_name(list: &[PrintPreset], base: &str) -> String {
    let base = if base.trim().is_empty() { "New Preset" } else { base.trim() };
    let taken = |n: &str| n == DEFAULT_NAME || list.iter().any(|p| p.name == n);
    if !taken(base) { return base.into(); }
    (2..).map(|n| format!("{base} {n}")).find(|n| !taken(n)).expect("a free name")
}

fn op(list: &[PrintPreset], op: PresetOp) -> Result<Vec<PrintPreset>, Refusal> { Ok(presets::apply_op(list, &op)?) }

fn keep(values: &mut Value, choices: &mut Value, list: &[PrintPreset]) {
    values["saved"] = serde_json::to_value(list).unwrap_or(json!([]));
    choices["printPresets"] = Value::Array(choices_of(list));
}

fn choices_of(list: &[PrintPreset]) -> Vec<Value> { choices(&json!({"saved": serde_json::to_value(list).unwrap_or(json!([]))})) }

/// After `changed` was set. File ▸ Print: a preset chosen fills the settings (the range stays the job's own), a setting changed is
/// [Custom]. The manager: a preset chosen is shown; New copies it under a free name, Delete removes it; the name renames it once
/// it is one the list does not have (empty, [Default] or taken: `nameOk` false, OK off); a setting changes the chosen preset.
/// A setting always comes back as the engine's whole set (All Printer's Marks sets the five marks).
pub fn linked(values: &mut Value, choices: &mut Value, changed: &str) -> Result<(), Refusal> {
    let list = list_of(values);
    let selected = values["preset"].as_str().unwrap_or(DEFAULT_NAME).to_owned();
    match changed {
        "preset" if manager(values) => choose(values, &list, &selected),
        "preset" => { fill(values, &list, &selected); }
        "listAction" => {
            let action = values["listAction"].as_str().unwrap_or("").to_owned();
            values["listAction"] = json!("");
            match action.as_str() {
                "new" => {
                    let name = free_name(&list, values["namePrefix"].as_str().unwrap_or(""));
                    let next = op(&list, PresetOp::Define { preset: PrintPreset { name: name.clone(), values: settings_of(values)? } })?;
                    keep(values, choices, &next);
                    choose(values, &next, &name);
                }
                "delete" => {
                    if selected == DEFAULT_NAME { return Err(refuse("listAction", "notEditable", "the built-in [Default] preset cannot be deleted".into())); }
                    let next = op(&list, PresetOp::Remove { name: selected })?;
                    keep(values, choices, &next);
                    choose(values, &next, DEFAULT_NAME);
                }
                _ => {}
            }
        }
        "name" => {
            let name = values["name"].as_str().unwrap_or("").trim().to_owned();
            if name == selected { values["nameOk"] = json!(true); return Ok(()); }
            if name.is_empty() || name == DEFAULT_NAME || list.iter().any(|p| p.name == name) { values["nameOk"] = json!(false); return Ok(()); }
            let next = op(&list, PresetOp::Update { name: selected, preset: PrintPreset { name: name.clone(), values: settings_of(values)? } })?;
            keep(values, choices, &next);
            values["preset"] = json!(name);
            values["nameOk"] = json!(true);
        }
        key if is_setting(key) => {
            let before = values.clone();
            let next = settings::set(&settings_of(&before)?, key, &values[key]).map_err(|e| refuse(key, "notOption", e))?;
            put(values, &next);
            if manager(values) {
                let saved = op(&list, PresetOp::Update { name: selected.clone(), preset: PrintPreset { name: selected, values: next } })?;
                keep(values, choices, &saved);
            } else {
                values["preset"] = json!(super::newdoc::CUSTOM);
            }
        }
        _ => {}
    }
    Ok(())
}

/// After a button that keeps the dialog open was answered: the lists read again (a preset saved from File ▸ Print is chosen; a file
/// loaded into the manager is listed, the chosen preset stays).
pub fn settled(action: &str, values: &mut Value, choices: &mut Value) {
    if manager(values) {
        let list = list_of(values);
        keep(values, choices, &list);
        let shown = values["preset"].as_str().unwrap_or(DEFAULT_NAME).to_owned();
        if !list.iter().any(|p| p.name == shown) { choose(values, &list, DEFAULT_NAME); }
    } else if action == "savePreset" {
        choices["printPresets"] = Value::Array(self::choices(&json!({})));
    }
}

/// The print job of File ▸ Print's values: the range (all, a typed range, the pages handed in), spreads and blank pages.
fn job(values: &Value) -> Value {
    let selection = match values["range"].as_str() {
        Some("range") => json!({"kind": "range", "value": values["rangeText"].as_str().unwrap_or("").trim()}),
        Some("pages") => json!({"kind": "pages", "ids": values["pageIds"]}),
        _ => json!({"kind": "all"}),
    };
    json!({"selection": selection, "spreads": values["printSpreads"] == true, "printBlankPages": values["printBlankPages"] == true})
}

/// The `printingPlan` request of the values (the dialog's preview: the sheets and their sizes).
pub fn plan_request(values: &Value) -> Result<Value, Refusal> {
    Ok(json!({"cmd": "printingPlan", "job": job(values), "settings": settings_of(values)?}))
}

pub(super) fn preview(values: &Value, session: Option<&crate::Session>) -> Result<Value, Refusal> {
    let plan = crate::printing::plan(&plan_request(values)?, session)?;
    Ok(json!({"ok": true, "sheets": plan_sheets(&plan)}))
}

/// The sheets of a plan as the frame lists them (`sheets` of a preview): each with the pages it shows.
pub fn plan_sheets(plan: &Value) -> Value {
    Value::Array(plan["sheets"].as_array().into_iter().flatten().enumerate()
        .map(|(n, s)| json!({"face": n + 1, "size": s["size"], "cells": [{"name": s["label"]}], "pageIds": s["pageIds"]})).collect())
}

/// The requests of a button: the manager's OK (the list into the user settings), Load and Save (a file of presets); File ▸ Print's
/// OK (the PDF proof), Print (to the printer), Cancel Job, and Save Preset (the settings as a new preset of the user's).
pub fn requests(action: &str, values: &Value) -> Result<Vec<Value>, Refusal> {
    if manager(values) {
        let list = list_of(values);
        let saved = serde_json::to_value(&list).map_err(|e| e.to_string())?;
        return Ok(vec![match action {
            "ok" => {
                if values["nameOk"] == false { return Err(refuse("name", "taken", "the preset's name is empty or taken".into())); }
                json!({"cmd": "userSettings", "patch": {"printPresets": saved}})
            }
            "import" => json!({"cmd": "printPresets", "op": "importFile", "path": values["loadPath"], "presets": saved}),
            "export" => json!({"cmd": "printPresets", "op": "exportFile", "path": values["filePath"], "names": [values["preset"]], "presets": saved}),
            other => return Err(format!("this dialog has no action {other}").into()),
        }]);
    }
    let settings = settings_of(values)?;
    Ok(vec![match action {
        "ok" => json!({"cmd": "printingFile", "job": job(values), "path": values["path"], "settings": settings}),
        "print" => json!({"cmd": "printToDevice", "printer": values["device"], "settings": settings, "job": job(values)}),
        "cancelJob" => json!({"cmd": "cancelPrintJob", "printer": values["sent"]["job"]["printer"], "job": values["sent"]["job"]["id"]}),
        "savePreset" => {
            let name = values["newName"].as_str().unwrap_or("").trim().to_owned();
            let list = user_presets();
            if name.is_empty() { return Err(refuse("newName", "required", "newName is required".into())); }
            if name == DEFAULT_NAME { return Err(refuse("newName", "reserved", "the name [Default] is reserved".into())); }
            if list.iter().any(|p| p.name == name) { return Err(refuse("newName", "taken", format!("a print preset named {name} already exists"))); }
            let next = op(&list, PresetOp::Define { preset: PrintPreset { name, values: settings } })?;
            json!({"cmd": "userSettings", "patch": {"printPresets": serde_json::to_value(&next).map_err(|e| e.to_string())?}})
        }
        other => return Err(format!("this dialog has no action {other}").into()),
    }])
}
