//! Edit > Transparency Flattener Presets (an app dialog, `"link": "flattenerPresets"`): the three built-in presets, which cannot be
//! changed, and the user's own, kept in the app settings (`flattenerPresets`, a list the Flattener Preview panel and printing read).
//! The dialog holds the user's list (`presets`, hidden rows) and shows one preset at a time (`flattenerPreset`: "0" to "2" the built-in
//! ones, "3" on the user's): choosing another fills the fields from it, a field changed is written to it, New (`listAction: "new"`)
//! copies the one shown under a name no preset has, Delete (`"delete"`) removes the one shown. OK saves the list (`userSettings patch`).
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};

/// The settings of a preset, as the app settings keep them (with `name`).
const FIELDS: [&str; 6] = ["rasterVectorBalance", "lineArtAndTextResolution", "gradientAndMeshResolution", "convertAllTextToOutlines", "convertAllStrokesToOutlines", "clipComplexRegions"];
/// The built-in presets as the picker shows them (screen texts, translated there).
const BUILT_IN_LABELS: [&str; 3] = ["[저해상도]", "[중간 해상도]", "[고해상도]"];
/// The line art and gradient resolutions of the built-in presets (InDesign's: Low 288/144, Medium 300/150, High 1200/300 ppi).
const RESOLUTIONS: [(f64, f64); 3] = [(288., 144.), (300., 150.), (1200., 300.)];

/// A built-in preset's row: its balance and switches are the Flattener Preview's own (`flatpreview::built_in`), with its resolutions.
fn built_in(i: usize) -> Value {
    let p = crate::flatpreview::built_in(crate::flatpreview::BUILT_IN[i]).expect("the built-in presets are known");
    json!({"name": BUILT_IN_LABELS[i], "rasterVectorBalance": p.balance, "lineArtAndTextResolution": RESOLUTIONS[i].0, "gradientAndMeshResolution": RESOLUTIONS[i].1,
        "convertAllTextToOutlines": p.text, "convertAllStrokesToOutlines": p.strokes, "clipComplexRegions": p.clip})
}

/// The user's presets the app settings keep, the ones the screen kept: a name not used before and every value in its range.
fn saved() -> Vec<Value> {
    let settings = crate::user_settings::run(&json!({})).map(|v| v["settings"]["flattenerPresets"].clone()).unwrap_or(Value::Null);
    let in_range = |p: &Value, k: &str, lo: f64, hi: f64| p[k].as_f64().is_some_and(|v| v.is_finite() && (lo..=hi).contains(&v));
    let mut names = std::collections::HashSet::new();
    settings.as_array().into_iter().flatten().filter(|p| {
        let name = p["name"].as_str().unwrap_or("");
        !name.trim().is_empty() && in_range(p, "rasterVectorBalance", 0., 100.) && in_range(p, "lineArtAndTextResolution", 1., 9600.)
            && in_range(p, "gradientAndMeshResolution", 1., 1200.) && FIELDS[3..].iter().all(|k| p[*k].is_boolean()) && names.insert(name.to_owned())
    }).map(|p| { let mut row = json!({"name": p["name"]}); for k in FIELDS { row[k] = p[k].clone(); } row }).collect()
}

/// What the dialog starts with: the user's presets, and High Resolution shown (as the screen opened it).
pub fn defaults() -> Map<String, Value> {
    let mut values = json!({"presets": saved(), "flattenerPreset": "2", "listAction": ""});
    fill(&mut values);
    values.as_object().cloned().unwrap_or_default()
}

/// The picker's list: the built-in presets (translated by the screen), then the user's by their names.
pub fn choices(presets: &Value) -> Vec<Value> {
    let mut list: Vec<Value> = BUILT_IN_LABELS.iter().enumerate().map(|(i, l)| json!({"value": i.to_string(), "label": l, "translate": true})).collect();
    list.extend(presets.as_array().into_iter().flatten().enumerate().map(|(i, p)| json!({"value": (i + 3).to_string(), "label": p["name"]})));
    list
}
pub fn saved_choices() -> Vec<Value> { choices(&Value::Array(saved())) }

/// The preset shown: its place in the built-in and user lists.
fn shown(values: &Value) -> usize { values["flattenerPreset"].as_str().and_then(|v| v.parse().ok()).unwrap_or(2) }

/// The fields filled from the preset shown (a built-in one's name is the picker's, not a box: `flattenerName` stays empty).
fn fill(values: &mut Value) {
    let at = shown(values);
    let row = if at < 3 { built_in(at) } else { values["presets"][at - 3].clone() };
    for k in FIELDS { values[k] = row[k].clone(); }
    values["flattenerName"] = if at < 3 { json!("") } else { row["name"].clone() };
    values["builtIn"] = json!(at < 3);
}

/// After `changed` was set: the preset chosen fills the fields, a field goes into the preset shown (a built-in one is not changed),
/// New and Delete change the list; the picker's list follows the names.
pub fn linked(values: &mut Value, choices: &mut Value, changed: &str) -> Result<(), Refusal> {
    let at = shown(values);
    match changed {
        "flattenerPreset" => fill(values),
        // the whole list set at once: the preset shown is filled again (High Resolution when it is gone)
        "presets" => {
            if at >= 3 && at - 3 >= values["presets"].as_array().map_or(0, Vec::len) { values["flattenerPreset"] = json!("2"); }
            fill(values);
        }
        "listAction" => {
            let action = values["listAction"].as_str().unwrap_or("").to_owned();
            values["listAction"] = json!("");
            match action.as_str() {
                "new" => {
                    // a copy of the preset shown, named "New Preset n" (`namePrefix`, the screen's words, with {n} the first number free)
                    let mut row = if at < 3 { built_in(at) } else { values["presets"][at - 3].clone() };
                    let prefix = values["namePrefix"].as_str().filter(|p| p.contains("{n}")).unwrap_or("새 사전 설정 {n}").to_owned();
                    let taken = |name: &str| values["presets"].as_array().into_iter().flatten().any(|p| p["name"] == name);
                    let name = (1..).map(|n| prefix.replace("{n}", &n.to_string())).find(|name| !taken(name)).unwrap_or_default();
                    row["name"] = json!(name);
                    let list = values["presets"].as_array_mut().ok_or("the presets are not a list")?;
                    list.push(row);
                    values["flattenerPreset"] = json!((list.len() + 2).to_string());
                }
                "delete" => {
                    if at < 3 { return Err(refuse("listAction", "notEditable", "a built-in flattener preset cannot be deleted".into())); }
                    values["presets"].as_array_mut().ok_or("the presets are not a list")?.remove(at - 3);
                    values["flattenerPreset"] = json!("2");
                }
                _ => {}
            }
            fill(values);
        }
        key if key == "flattenerName" || FIELDS.contains(&key) => {
            if at < 3 { return Err(refuse(key, "notEditable", format!("{key}: a built-in flattener preset cannot be changed"))); }
            values["presets"][at - 3][if key == "flattenerName" { "name" } else { key }] = values[key].clone();
        }
        _ => return Ok(()),
    }
    choices["flattenerPresets"] = Value::Array(self::choices(&values["presets"]));
    Ok(())
}

/// On OK: every preset named (without the spaces around it), no two alike and none a built-in one's name (as the screen and InDesign
/// show it), the names saved trimmed. A refusal names the preset's row (`presets/<row>/name`).
pub fn check(values: &mut Value) -> Result<(), Refusal> {
    let reserved: Vec<String> = BUILT_IN_LABELS.iter().map(|s| s.to_string()).chain(crate::flatpreview::BUILT_IN.iter().map(|s| s.to_string())).collect();
    let mut seen = std::collections::HashSet::new();
    for (i, row) in values["presets"].as_array_mut().into_iter().flatten().enumerate() {
        let name = row["name"].as_str().unwrap_or("").trim().to_owned();
        let at = format!("presets/{i}/name");
        if name.is_empty() { return Err(refuse(&at, "required", format!("flattener preset {i} has no name"))); }
        if reserved.contains(&name) { return Err(refuse(&at, "reserved", format!("{name} is a built-in flattener preset"))); }
        if !seen.insert(name.clone()) { return Err(refuse(&at, "duplicate", format!("there is another flattener preset {name}"))); }
        row["name"] = json!(name);
    }
    Ok(())
}
