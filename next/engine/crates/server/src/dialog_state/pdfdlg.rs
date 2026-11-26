//! File ▸ PDF Presets ▸ Define (`pdfPresets`, an app dialog managing the project's PDF presets) and File ▸ Export PDF
//! (`pdfExport`, a document dialog of the open document's requests) share one field list: a PDF preset's settings as flat fields
//! (`pdfPresets.json`; `pdfExport` extends it). The preset list is the `pdfPresets` request's (`list`): the six built-in presets and
//! the project's (`project:<name>`). Choosing one fills the fields from it; what OK and the buttons send is the request the screen
//! sent for the same values (`pdfPresets save/delete/import/export`, `export`).
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};
use std::path::{Component, Path, PathBuf};

/// The preset's settings as the dialog's fields, all of them (a preset without downsampling keeps the boxes at 450 / 300 ppi).
pub fn fields_of(p: &Value) -> Map<String, Value> {
    let mut v = Map::new();
    for key in ["version", "name", "standard", "spreads", "useDocumentBleed", "markOffset", "compression", "imageCompression", "jpegQuality",
        "colorConversion", "subsetFontsBelow", "flattenLineArtPpi", "flattenSmoothPpi"] {
        v.insert(key.into(), p[key].clone());
    }
    // Create Acrobat Layers and Export Layers (presets saved before them: off, visible & printable)
    v.insert("acrobatLayers".into(), json!(p["acrobatLayers"] == true));
    v.insert("exportLayers".into(), json!(p["exportLayers"].as_str().unwrap_or("visiblePrintable")));
    for (i, key) in ["bleedTop", "bleedBottom", "bleedInside", "bleedOutside"].into_iter().enumerate() { v.insert(key.into(), p["bleed"][i].clone()); }
    for (key, mark) in [("crop", "crop"), ("bleedMarks", "bleed"), ("registration", "registration"), ("pageInfo", "pageInfo")] {
        v.insert(key.into(), json!(p["marks"][mark] == true));
    }
    v.insert("markWeight".into(), json!(p["markWeight"].as_f64().unwrap_or(0.25).to_string()));
    let sampling = &p["downsample"];
    v.insert("downsample".into(), json!(sampling.is_object()));
    v.insert("downsampleAbove".into(), sampling.get("above").cloned().unwrap_or(json!(450)));
    v.insert("downsampleTo".into(), sampling.get("to").cloned().unwrap_or(json!(300)));
    v
}

/// The dialog's fields as the preset the requests take (checked as `folio_pdf::PdfPreset` checks a preset; a refusal names no field).
pub fn settings_of(values: &Value) -> Result<Value, Refusal> {
    let mut p = json!({});
    for key in ["version", "name", "standard", "spreads", "useDocumentBleed", "markOffset", "compression", "imageCompression", "jpegQuality",
        "colorConversion", "subsetFontsBelow", "flattenLineArtPpi", "flattenSmoothPpi"] {
        p[key] = values[key].clone();
    }
    p["name"] = json!(values["name"].as_str().unwrap_or("").trim());
    // PDF/X-1a and PDF/X-3 are PDF 1.3: no Acrobat layers (the box is grey there)
    let pdf13 = matches!(values["standard"].as_str(), Some("X1a" | "X3"));
    p["acrobatLayers"] = json!(values["acrobatLayers"] == true && !pdf13);
    p["exportLayers"] = json!(values["exportLayers"].as_str().unwrap_or("visiblePrintable"));
    p["bleed"] = json!(["bleedTop", "bleedBottom", "bleedInside", "bleedOutside"].map(|k| values[k].as_f64().unwrap_or(0.)));
    p["marks"] = json!({"crop": values["crop"], "bleed": values["bleedMarks"], "registration": values["registration"], "pageInfo": values["pageInfo"]});
    p["markWeight"] = json!(values["markWeight"].as_str().and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.25));
    p["downsample"] = if values["downsample"] == true { json!({"above": values["downsampleAbove"], "to": values["downsampleTo"]}) } else { Value::Null };
    let preset: folio_pdf::PdfPreset = serde_json::from_value(p).map_err(|e| e.to_string())?;
    preset.validate()?;
    Ok(json!(preset))
}

/// The presets the picker lists: the `pdfPresets list` answer (the built-in names translated by the screen), and an unsaved preset
/// (`""`: New, a file loaded) listed only while it is the one shown.
pub fn choices(project: &str) -> Vec<Value> {
    let mut request = json!({"cmd": "pdfPresets", "action": "list"});
    if !project.is_empty() { request["project"] = json!(project); }
    let listed = crate::pdf_presets::run(&request).unwrap_or_else(|_| {
        crate::pdf_presets::run(&json!({"cmd": "pdfPresets", "action": "list"})).unwrap_or(json!([]))
    });
    let mut list: Vec<Value> = listed.as_array().into_iter().flatten().map(|e| {
        let name = e["settings"]["name"].as_str().unwrap_or("");
        let label = match name { "[High Quality Print]" => "[고품질 인쇄]", "[Press Quality]" => "[인쇄 품질]", "[Smallest File Size]" => "[최소 파일 크기]", other => other };
        json!({"value": e["id"], "label": label, "translate": label != name, "builtin": e["builtin"], "settings": e["settings"]})
    }).collect();
    list.push(json!({"value": "", "label": "(저장하지 않은 사전 설정)", "translate": true, "only": true}));
    list
}

/// The listed preset of an id.
fn entry<'c>(choices: &'c Value, id: &str) -> Option<&'c Value> {
    choices["pdfPresets"].as_array().into_iter().flatten().find(|c| c["value"] == id && c["settings"].is_object())
}

/// The fields filled from the listed preset `id` (nothing when it is not listed).
fn fill(values: &mut Value, choices: &Value, id: &str) {
    let Some(e) = entry(choices, id) else { return };
    for (k, v) in fields_of(&e["settings"]) { values[k] = v; }
    values["preset"] = json!(id);
    values["builtIn"] = json!(e["builtin"] == true);
}

/// The page indexes of a range as File ▸ Export PDF takes it ("1-4, 7, A": a page's name first, else its position from 1); `None`
/// for every page (empty), `Err` when a part names no page.
pub fn pages(text: &str, doc: &folio_model::Document) -> Result<Option<Vec<usize>>, ()> {
    let t = text.trim();
    if t.is_empty() { return Ok(None); }
    let all: Vec<&folio_model::Page> = doc.spreads.iter().flat_map(|s| s.pages.iter()).collect();
    let pick = |token: &str| all.iter().position(|p| p.name == token)
        .or_else(|| token.parse::<usize>().ok().filter(|n| (1..=all.len()).contains(n)).map(|n| n - 1));
    let mut out = std::collections::BTreeSet::new();
    for part in t.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        let pieces: Vec<&str> = part.split('-').collect();
        if pieces.len() > 2 { return Err(()); }
        let from = pick(pieces[0]).ok_or(())?;
        let to = if pieces.len() == 2 { pick(pieces[1]).ok_or(())? } else { from };
        for i in from.min(to)..=from.max(to) { out.insert(i); }
    }
    let mut list: Vec<usize> = out.into_iter().map(|i| all[i].index).collect();
    list.sort();
    Ok(Some(list))
}

/// File ▸ Export PDF's switches: OK is off while the profile is missing, the range names no page or the preset is Smallest File Size.
fn settle_export(values: &mut Value, doc: Option<&folio_model::Document>) {
    if values.get("rangeOk").is_none() { return; }
    let range_ok = doc.is_none_or(|d| pages(values["range"].as_str().unwrap_or(""), d).is_ok());
    values["rangeOk"] = json!(range_ok);
    values["canExport"] = json!(range_ok && values["profileOk"] == true && values["standard"] != "Small");
}

/// What a dialog opens with, after the values asked for: a preset asked for fills the fields (File ▸ PDF Presets ▸ <preset>).
pub fn opened(values: &mut Value, choices: &Value, requested: &Value, doc: Option<&folio_model::Document>) {
    if let Some(id) = requested["preset"].as_str() { fill(values, choices, id); }
    settle_export(values, doc);
}

/// After `changed` was set: a preset chosen fills the fields, New starts an unsaved one named `newName`, PDF/X-1a takes no "do not
/// convert" (it is changed to the preset's default, as the screen did, or refused when asked for).
pub fn linked(values: &mut Value, choices: &Value, changed: &str, doc: Option<&folio_model::Document>) -> Result<(), Refusal> {
    match changed {
        "preset" => { let id = values["preset"].as_str().unwrap_or("").to_owned(); fill(values, choices, &id); }
        "listAction" if values["listAction"] == "new" => {
            values["listAction"] = json!("");
            values["preset"] = json!("");
            values["builtIn"] = json!(false);
            values["name"] = values["newName"].clone();
        }
        "standard" if values["standard"] == "X1a" && values["colorConversion"] == "none" => values["colorConversion"] = json!("preset"),
        "colorConversion" if values["standard"] == "X1a" && values["colorConversion"] == "none" => {
            return Err(refuse("colorConversion", "notAllowed", "PDF/X-1a requires CMYK: colorConversion cannot be none".into()));
        }
        "downsample" if values["downsample"] == true => { values["downsampleAbove"] = json!(450); values["downsampleTo"] = json!(300); }
        _ => {}
    }
    settle_export(values, doc);
    Ok(())
}

/// After a button that keeps the manager open was answered (the list read again): the saved preset shown, the first one after a
/// delete, the file loaded into the fields (unsaved).
pub fn settled(action: &str, values: &mut Value, choices: &Value) {
    match action {
        "ok" => { let id = format!("project:{}", values["name"].as_str().unwrap_or("").trim()); fill(values, choices, &id); }
        "delete" => {
            let first = choices["pdfPresets"][0]["value"].as_str().unwrap_or("x4").to_owned();
            fill(values, choices, &first);
        }
        "import" => {
            let imported = values["imported"].take();
            if imported.is_object() { for (k, v) in fields_of(&imported) { values[k] = v; } }
            values["imported"] = json!({});
            values["preset"] = json!("");
            values["builtIn"] = json!(false);
        }
        _ => {}
    }
}

/// A file named relative to the project (`presets/pdf-share/a.json`): no root, drive or `..`, every part a name all systems take, `.json`.
fn project_file(project: &str, relative: &str) -> Result<PathBuf, Refusal> {
    let rel = relative.trim();
    let path = Path::new(rel);
    let parts_ok = !rel.contains('\\') && path.components().all(|c| matches!(c, Component::Normal(_)))
        && rel.split('/').all(|p| folio_project::paths::check_name(p).is_ok());
    if rel.is_empty() || !parts_ok { return Err(refuse("filePath", "notRelative", format!("filePath must be a path inside the project: {rel}"))); }
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) { return Err(refuse("filePath", "badExtension", "filePath must end in .json".into())); }
    if project.is_empty() { return Err("a project is required".into()); }
    Ok(Path::new(project).join(rel))
}

/// The requests of a button: the manager's `pdfPresets` (`ok` save, `delete`, `import`, `export`), File ▸ Export PDF's `export` with
/// the settings it remembers (`userSettings`).
pub fn requests(action: &str, values: &Value, choices: &Value, session: Option<&crate::Session>) -> Result<Vec<Value>, Refusal> {
    let project = values["project"].as_str().unwrap_or("");
    if values.get("path").is_some() {
        if action != "ok" { return Err(format!("this dialog has no action {action}").into()); }
        let s = session.ok_or("no document open")?;
        let settings = settings_of(values)?;
        if settings["standard"] == "Small" { return Err(refuse("standard", "notApplicable", "the Smallest File Size preset cannot be exported".into())); }
        let pages = pages(values["range"].as_str().unwrap_or(""), &s.doc).map_err(|_| refuse("range", "notPageRange", "range names no page".into()))?;
        let path = values["path"].as_str().unwrap_or("");
        let preset = settings["standard"].as_str().unwrap_or("X4").to_ascii_lowercase();
        let choice = json!({"preset": preset, "bleed": settings["useDocumentBleed"], "marks": settings["marks"], "spreads": settings["spreads"], "settings": settings});
        let mut export = choice.clone();
        export["cmd"] = json!("export");
        export["path"] = json!(path);
        export["pages"] = json!(pages);
        let folder = Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        return Ok(vec![export, json!({"cmd": "userSettings", "patch": {"export": choice, "exportFolder": folder}})]);
    }
    let request = |fields: Value| { let mut r = json!({"cmd": "pdfPresets"}); for (k, v) in fields.as_object().into_iter().flatten() { r[k] = v.clone(); } r };
    Ok(vec![match action {
        "ok" => {
            let settings = settings_of(values)?;
            let name = settings["name"].as_str().unwrap_or("").to_owned();
            if crate::pdf_presets::run(&json!({"action": "list"})).ok().into_iter().flat_map(|l| l.as_array().cloned().unwrap_or_default()).any(|e| e["settings"]["name"] == name.as_str()) {
                return Err(refuse("name", "reserved", format!("{name} is a built-in PDF preset")));
            }
            let replace = entry(choices, values["preset"].as_str().unwrap_or("")).is_some_and(|e| e["builtin"] != true && e["settings"]["name"] == name.as_str());
            request(json!({"action": "save", "project": project, "name": name, "settings": settings, "replace": replace}))
        }
        "delete" => {
            let e = entry(choices, values["preset"].as_str().unwrap_or("")).filter(|e| e["builtin"] != true)
                .ok_or_else(|| refuse("preset", "notEditable", "only a project's PDF preset can be deleted".into()))?;
            request(json!({"action": "delete", "project": project, "name": e["settings"]["name"]}))
        }
        "import" => request(json!({"action": "import", "path": project_file(project, values["filePath"].as_str().unwrap_or(""))?})),
        "export" => request(json!({"action": "export", "path": project_file(project, values["filePath"].as_str().unwrap_or(""))?, "settings": settings_of(values)?})),
        other => return Err(format!("this dialog has no action {other}").into()),
    }])
}

/// What File ▸ Export PDF starts from: the settings exported last when they are of a listed preset (by its name), else the first
/// listed; the document's CMYK profile (its name, and whether this computer has it), its pages, the PDF's file name.
pub fn export_defaults(s: &crate::Session, requested: &Value) -> (Map<String, Value>, Value) {
    let list = Value::Array(choices(requested["project"].as_str().unwrap_or("")));
    let choices = json!({"pdfPresets": list});
    let saved = crate::user_settings::run(&json!({})).map(|v| v["settings"]["export"].clone()).unwrap_or(Value::Null);
    let previous = saved["settings"]["name"].as_str().and_then(|name| choices["pdfPresets"].as_array().into_iter().flatten()
        .find(|c| c["settings"]["name"] == name).and_then(|c| c["value"].as_str().map(str::to_owned)));
    let mut values = json!({});
    fill(&mut values, &choices, previous.as_deref().unwrap_or("x4"));
    if previous.is_some() && saved["settings"].is_object() {
        for (k, v) in fields_of(&saved["settings"]) { values[k] = v; }
    }
    let cmyk = &s.summary["color"]["cmyk"];
    let profile = cmyk["name"].as_str().unwrap_or("");
    values["profileName"] = json!(profile);
    values["hasProfile"] = json!(!profile.is_empty());
    values["profileOk"] = json!(cmyk["status"] == "used");
    let errors = requested["preflightErrors"].as_u64().unwrap_or(0);
    values["hasPreflightErrors"] = json!(errors > 0);
    let pages = s.doc.spreads.iter().map(|sp| sp.pages.len()).sum::<usize>();
    let file = Path::new(&s.source_path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    (values.as_object().cloned().unwrap_or_default(), json!({"pages": pages, "fileName": file}))
}

/// What the manager starts from: the first preset listed (the project's folder is asked for).
pub fn manager_defaults(requested: &Value) -> Map<String, Value> {
    let choices = json!({"pdfPresets": choices(requested["project"].as_str().unwrap_or(""))});
    let mut values = json!({});
    fill(&mut values, &choices, "x4");
    values.as_object().cloned().unwrap_or_default()
}
