//! Unapplied forms share one field schema (`next/shared/dialogs/<id>.json`, one file per dialog) and produce ordinary edit requests.
//!
//! A dialog is declared with its fields (`boolean`, `number` with a range and a `unit` the screen shows it in, `enum`, `text`,
//! `choice` — an id from a list the engine gives when the dialog opens (`source`: paragraph styles, layers, …; `none` names the empty
//! choice) — and `array` — rows of sub-fields, set by a path `name/row/sub` and added, removed or moved with `row`), the tabs they sit on
//! (`tabs`, a field's `tab`; `dialogState` holds the values of every tab),
//! the item ids it works on (`target`), and one edit-command list per button (`actions`; `ok` is the default). Applying a dialog
//! sends the same commands the screen sends for the same values; nothing else is computed here, except the few linked fields
//! (`link`) a dialog keeps in step (a move's offset and its distance and angle).
//! A "changed only" dialog (`{"$changed":true}`) leaves out `local` fields (they only steer the dialog) and fields whose `sendWhen` does
//! not hold, sends the fields of a `packs` entry together under the pack's name, and a command marked `"$each":"$ids"` goes once per item
//! (`"$id"`).
//!
//! A dialog of `"scope": "app"` opens without a document (New Project, Language, …): it is held by the engine process, not a document
//! session, and its actions are app requests (`{"cmd": "projectCreate", …}`, one of [`APP_COMMANDS`]) run when it is applied; the reply
//! carries what they made (`result`). A system window (choose a folder) is a path field (`"path": "folder"` or `"name"`), checked here.
use serde_json::{Map, Value, json};
use std::sync::Mutex;

mod newdoc;
pub use newdoc::set_fonts;
mod tocdlg;
mod profdlg;
mod userdict;
mod flattener;
mod shortcuts;
mod pdfdlg;
mod printdlg;
mod packagedlg;
mod actions;
mod anchored;
mod booklet;
mod caption;
mod choices;
mod clipping;
mod color_profile;
mod cross_reference_format;
mod document;
mod fields;
mod file_info;
mod frame_fitting;
mod graphic_layers;
mod hyperlink;
mod image_color;
mod math_expression;
mod mixed_ink;
mod notes;
mod page_layout;
mod preflight_options;
mod stroke_style;
mod swatches;
mod text_frame;
mod text_path;
mod transform;

include!(concat!(env!("OUT_DIR"), "/dialog_files.rs"));

use actions::{all_same, apply_in, substitute};
use choices::choice_lists;
pub use document::document_finished;
use document::document_requests;
use fields::{allowed, checked, field, name_of, set_path, typed, when_holds};

#[path = "preferences_dialog.rs"]
mod preferences;

/// The app dialog open in this engine process (one at a time, as a document session holds one).
static APP: Mutex<Value> = Mutex::new(Value::Null);

/// The requests an app dialog may send when applied: the ones the screen sends for the same dialog. `documentNew` needs the font
/// store and runs on its own thread, `book` and `library` the colour profiles: they are handed to main.rs (`_appRequest`, [`MAIN_COMMANDS`])
/// and finished by [`app_finished`].
pub const APP_COMMANDS: &[&str] = &["userSettings", "projectCreate", "documentNew", "documentPresets", "book", "library", "pdfPresets", "printPresets"];
/// The app requests main.rs runs for an app dialog (one per apply).
pub const MAIN_COMMANDS: &[&str] = &["documentNew", "book", "library"];
/// The requests of the open document a document dialog of `"requests": true` sends instead of edits (Print Booklet: the PDF file, the
/// printer and taking a job back); the ones the screen sent for the same values.
pub const DOCUMENT_REQUESTS: &[&str] = &["bookletFile", "printToDevice", "cancelPrintJob", "export", "userSettings", "printingFile", "package", "packageCancel"];
/// The document requests that take long (File ▸ Export PDF writes a PDF, File ▸ Package makes a folder, with progress events): main.rs
/// runs them on their own thread (`_documentRequest`) and finishes the dialog with [`document_finished`].
pub const THREAD_REQUESTS: &[&str] = &["export", "package"];

/// A dialog's `templates` put in where its actions and preview name them (`{"$template":"options"}`): one template, used by several.
fn with_templates(value: &mut Value, templates: &Value) {
    match value {
        Value::Object(o) if o.len() == 1 && o.get("$template").and_then(Value::as_str).is_some() => {
            let name = o["$template"].as_str().unwrap_or("").to_owned();
            *value = templates.get(&name).cloned().unwrap_or_else(|| panic!("unknown dialog template {name}"));
        }
        Value::Object(o) => o.values_mut().for_each(|v| with_templates(v, templates)),
        Value::Array(a) => a.iter_mut().for_each(|v| with_templates(v, templates)),
        _ => {}
    }
}

pub fn schema() -> Value {
    let mut all = Value::Object(DIALOG_FILES.iter()
        .map(|(name, text)| (name.to_string(), serde_json::from_str(text).unwrap_or_else(|e| panic!("dialog {name}: {e}")))).collect());
    for def in all.as_object_mut().into_iter().flat_map(|o| o.values_mut()) {
        let Some(templates) = def.get("templates").cloned() else { continue };
        for key in ["actions", "preview"] { if let Some(part) = def.get_mut(key) { with_templates(part, &templates); } }
    }
    // the anchored object options are the engine's own list (folio_idml::anchored::FIELDS), so the dialog is made from it
    all["anchoredOptions"] = anchored::anchored_definition();
    // Edit > Preferences: its document preferences are typed from the engine's own list (folio_idml::docprefs)
    if let Some(def) = all.get_mut("preferences") { *def = preferences::filled(def.take()); }
    // Define Profiles: a box for each rule and number of the engine's preflight list
    if let Some(def) = all.get_mut("preflightProfiles") { *def = profdlg::filled(def.take()); }
    // the print presets' settings: a field for each of the engine's print settings, labelled by the dialog's table (File ▸ Print extends it)
    if let Some(def) = all.get_mut("printPresets") { *def = printdlg::filled(def.take()); }
    // a dialog that `extends` another has its own fields first, then the other's but the ones it `omit`s
    let names: Vec<String> = all.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    for name in names {
        let Some(base) = all[&name]["extends"].as_str().map(str::to_owned) else { continue };
        let omit = all[&name]["omit"].clone();
        let inherited: Vec<Value> = all[&base]["fields"].as_array().unwrap_or_else(|| panic!("dialog {name} extends unknown {base}")).iter()
            .filter(|f| !omit.as_array().is_some_and(|o| o.contains(&f["name"]))).cloned().collect();
        let mut fields = all[&name]["fields"].as_array().cloned().unwrap_or_default();
        fields.extend(inherited);
        all[&name]["fields"] = Value::Array(fields);
        // (and its tabs with their notes, when it has none of its own: File ▸ Export PDF has the PDF presets' tabs)
        for key in ["tabs", "tabNotes"] { if all[&name].get(key).is_none() && let Some(v) = all[&base].get(key).cloned() { all[&name][key] = v; } }
    }
    all
}

fn definition(name: &str) -> Result<Value, Refusal> { schema().get(name).cloned().ok_or_else(|| format!("unsupported dialog: {name}").into()) }

/// A refused request. With a field it is answered as a structured failure (`errorKind`, `field`, `reason`); the dialog stays as it was.
pub struct Refusal { field: Option<String>, reason: &'static str, message: String }
impl From<String> for Refusal { fn from(message: String) -> Self { Refusal { field: None, reason: "refused", message } } }
impl From<&str> for Refusal { fn from(message: &str) -> Self { message.to_string().into() } }
fn refuse(field: &str, reason: &'static str, message: String) -> Refusal { Refusal { field: Some(field.into()), reason, message } }

/// The placed graphic of a frame (a rectangle, oval or polygon that holds one).
fn graphic_of<'d>(s: &'d crate::Session, id: &str) -> Option<&'d folio_model::Graphic> {
    match &item(s, id)?.kind {
        folio_model::ItemKind::Rectangle { content } | folio_model::ItemKind::Oval { content } | folio_model::ItemKind::Polygon { content } => content.as_ref(),
        _ => None,
    }
}

/// A frame of the document by its id (spreads and parent spreads).
fn item<'d>(s: &'d crate::Session, id: &str) -> Option<&'d folio_model::PageItem> {
    let mut found = None;
    for spread in s.doc.spreads.iter().chain(&s.doc.master_spreads) {
        for item in &spread.items { item.walk(&mut |i| { if i.id == id && found.is_none() { found = Some(i); } }); }
    }
    found
}

fn event(s: &crate::Session, state: &Value, edit: Option<&Value>) -> Value {
    json!({"event":"dialog-state","path":s.source_path,"detail":{"dialogState":state,"edit":edit}})
}

/// The event of a document dialog whose long request is still out (a package being made): the screen takes it at once
/// (`progress`), not after the request's answer.
fn progress_event(s: &crate::Session, state: &Value) -> Value {
    json!({"event":"dialog-state","path":s.source_path,"detail":{"dialogState":state,"edit":null,"progress":true}})
}

/// A progress event of the package File ▸ Package is making (`package`'s `progress`), into the dialog's values; the event to send
/// (none when the dialog is not making one: a package asked for without it).
pub fn document_progress(s: &crate::Session, progress: &Value) -> Option<Value> {
    let mut state = s.dialog.lock().ok()?;
    if !definition(state["dialog"].as_str().unwrap_or("")).is_ok_and(|def| def["link"] == "package") { return None; }
    if !packagedlg::progressed(&mut state["values"], progress) { return None; }
    Some(progress_event(s, &state))
}

/// The event of the app dialog: of no document (`scope: "app"`); once applied, `result` is what its requests answered.
fn app_event(state: &Value, dialog: &Value, result: Option<&Value>) -> Value {
    json!({"event":"dialog-state","scope":"app","path":null,"detail":{"dialogState":state,"edit":null,"dialog":dialog,"result":result}})
}

/// Keeps the fields a dialog ties together in step after `changed` was set.
fn linked(def: &Value, values: &mut Value, changed: &str) {
    match def["link"].as_str() {
        Some("polar") => transform::link_move(values, changed),
        Some("orientation") => page_layout::link_orientation(values, changed),
        Some("uniformScale") => transform::link_scale(values, changed),
        Some("strokeKind") if changed == "kind" && values["existing"] != true => stroke_style::link_kind(values),
        Some("graphicLayerComp") => graphic_layers::link_comp(values, changed),
        Some("gradientStops") if changed == "stops" => swatches::link_stops(values),
        Some("textFrameInsets") => text_frame::link_insets(values, changed),
        _ => {}
    }
}

/// The values the apply uses: a proportional scale has one ratio.
fn prepared(def: &Value, values: &mut Value) {
    if def["link"] == "uniformScale" { transform::prepare_scale(values); }
    if def["link"] == "clippingOptions" { clipping::prepare(values); }
}

/// The app settings as they are now (what an app request is made from); a dialog that cannot read them sends nothing.
fn current_settings() -> Result<Value, Refusal> { Ok(crate::user_settings::run(&json!({}))?["settings"].take()) }

/// The app requests a document dialog sends once its edit is done (the app settings of Edit > Preferences).
fn following(def: &Value, values: &Value) -> Result<Vec<Value>, Refusal> {
    if def["build"] == "preferences" { preferences::app_requests(def, values, &current_settings()?) } else { Ok(vec![]) }
}

/// The list field whose rows do not make what the dialog needs (`rowsCheck`: at least `min` rows that hold `where`, one of them holding `some`).
fn rows_short<'d>(def: &'d Value, values: &Value) -> Option<&'d str> {
    def["rowsCheck"].as_array().into_iter().flatten().find(|check| {
        let picked: Vec<&Value> = values[check["field"].as_str().unwrap_or("")].as_array().into_iter().flatten().filter(|r| when_holds(&check["where"], r)).collect();
        (picked.len() as u64) < check["min"].as_u64().unwrap_or(0) || (check.get("some").is_some() && !picked.iter().any(|r| when_holds(&check["some"], r)))
    }).map(|check| check["field"].as_str().unwrap_or(""))
}

/// The remembered value of a user setting that holds JSON text (what the screen keeps there), if it is one.
fn remembered(preference: &str) -> Value {
    crate::user_settings::run(&json!({})).ok()
        .and_then(|v| v["settings"]["preferences"][preference].as_str().map(str::to_owned))
        .and_then(|v| serde_json::from_str::<Value>(&v).ok()).unwrap_or(Value::Null)
}

fn remember(def: &Value, values: &Value) {
    let Some(key) = def["preference"].as_str() else { return };
    let mut kept = values.clone();
    if let Some(kept) = kept.as_object_mut() {
        for field in def["fields"].as_array().into_iter().flatten().filter(|f| f["remember"] == false) {
            kept.remove(name_of(field));
        }
    }
    let _ = crate::user_settings::run(&json!({"values":{key:kept.to_string()}}));
}

fn document_info(s: &crate::Session) -> Result<Value, Refusal> {
    let text = crate::inspect::command(&json!({"cmd":"documentInfo"}), &s.doc, &s.layout, &s.colors, &s.inspect)?;
    let mut info: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(info["info"].take())
}

type Defaults = Result<(Map<String, Value>, Value), Refusal>;

/// What the document says about the dialog's items: starting values and facts the screen shows (not values the dialog holds).
fn document_defaults(provider: &str, s: &crate::Session, ids: &Value, requested: &Value) -> Defaults {
    match provider {
        "pageMargins" => page_layout::margins(s, ids),
        "documentSize" => page_layout::size(s),
        "mixedInk" | "newMixedInkGroup" => mixed_ink::new_defaults(provider, s, requested),
        "graphicLayers" => graphic_layers::defaults(s, ids),
        "textPathOptions" => text_path::defaults(s, ids),
        // File ▸ Export PDF: the settings exported last, the document's CMYK profile, its pages and name
        "pdfExport" => Ok(pdfdlg::export_defaults(s, requested)),
        // File ▸ Print: [Default] and the computer's default printer
        "printing" => Ok((printdlg::print_defaults(), json!({}))),
        // File ▸ Package: the choices, folder and contact the settings remember, the document's folder name, the last PDF settings
        "package" => Ok(packagedlg::defaults(s, requested)),
        "booklet" => booklet::defaults(),
        "anchoredObject" => anchored::object_defaults(s, ids),
        "fileInfo" => file_info::defaults(s),
        "captionSetup" => caption::defaults(s),
        "frameFitting" => frame_fitting::defaults(s, ids),
        "footnoteOptions" => notes::footnote_defaults(s),
        "endnoteOptions" => notes::endnote_defaults(s),
        "textFrameOptions" => text_frame::defaults(s, ids),
        "colorPrefs" => color_profile::assign_defaults(s),
        "convertToProfile" => color_profile::convert_defaults(s, requested),
        "imageColor" => image_color::defaults(s, ids),
        "clipping" => clipping::defaults(s, ids),
        "mixedInkGroup" => mixed_ink::group_defaults(s, ids),
        "colorGroup" => swatches::group_defaults(s, ids),
        "gradientSwatch" => swatches::gradient_defaults(s, ids),
        "tintBase" => swatches::tint_defaults(s, requested),
        "mathExpression" => math_expression::defaults(s, ids),
        "preflightOptions" => preflight_options::defaults(s),
        "crossReferenceFormat" => cross_reference_format::defaults(s, requested),
        "strokeStyle" => stroke_style::defaults(s, requested),
        "anchoredDefaults" => anchored::new_defaults(s),
        "newHyperlink" | "newCrossReference" => hyperlink::new_defaults(provider, s, requested),
        "tocStyle" => tocdlg::defaults(s, requested),
        "generateIndex" => Ok((crate::bookindex::dialog_defaults(s)?, json!({}))),
        "preflightProfiles" => profdlg::defaults(s, requested),
        "editHyperlink" | "editCrossReference" => hyperlink::edit_defaults(provider, s, ids),
        other => Err(format!("unknown dialog provider {other}").into()),
    }
}

/// The app settings an app dialog starts from: each field from the setting of its name (`language`), when the settings have it;
/// New Document's from a document preset (`newDocument`, `documentPreset`).
fn app_defaults(provider: &str, requested: &Value) -> Result<Map<String, Value>, Refusal> {
    match provider {
        "userSettings" => Ok(crate::user_settings::run(&json!({}))?["settings"].as_object().cloned().unwrap_or_default()),
        // the app's preferences by their keys (File > User: app.userName, app.userColor)
        "userPreferences" => Ok(crate::user_settings::run(&json!({}))?["settings"]["preferences"].as_object().cloned().unwrap_or_default()),
        // Edit > Spelling > User Dictionary: the dictionaries of the user settings, the lists of the language asked for
        "userDictionary" => userdict::defaults(requested),
        // Edit > Transparency Flattener Presets: the user's presets (app settings `flattenerPresets`), High Resolution shown
        "flattenerPresets" => Ok(flattener::defaults()),
        // Edit > Keyboard Shortcuts: the user's shortcuts (app settings `shortcuts`)
        "shortcuts" => Ok(shortcuts::defaults()),
        // File ▸ PDF Presets ▸ Define: the first preset of the list (the project's folder is asked for)
        "pdfPresets" => Ok(pdfdlg::manager_defaults(requested)),
        // File ▸ Print Presets ▸ Define: the user's print presets as the dialog's own list, [Default] shown
        "printPresets" => Ok(printdlg::manager_defaults()),
        // the app's preferences of one group, without the group's prefix (`userPreferences:colorSettings.`: Edit > Color Settings' workingSpaceRGB …)
        other if other.starts_with("userPreferences:") => {
            let prefix = &other["userPreferences:".len()..];
            let all = crate::user_settings::run(&json!({}))?["settings"]["preferences"].as_object().cloned().unwrap_or_default();
            Ok(all.into_iter().filter_map(|(k, v)| k.strip_prefix(prefix).map(|k| (k.to_owned(), v))).collect())
        }
        other => Ok(newdoc::defaults(other, requested)?),
    }
}

fn opened(def: &Value, name: &str, req: &Value, session: Option<&crate::Session>, profiles: Option<&crate::Profiles>) -> Result<Value, Refusal> {
    let target = if def["selection"] == true { "selection" } else { def["target"].as_str().unwrap_or("none") };
    let ids = match (target, req.get("ids")) {
        ("none", _) => json!([]),
        ("selection", None) => {
            let view = session.ok_or("no document open")?.view.lock().expect("view state");
            json!(view.selection.iter().map(|item| &item.id).collect::<Vec<_>>())
        }
        (_, Some(ids)) => ids.clone(),
        _ => Value::Null,
    };
    if target != "none" {
        let list = ids.as_array().filter(|a| !a.is_empty() && a.iter().all(Value::is_string))
            .ok_or_else(|| if target == "selection" { "dialog needs selected item ids" } else { "dialog needs ids" })?;
        if def["maxIds"].as_u64().is_some_and(|m| list.len() as u64 > m) { return Err(format!("dialog takes at most {} item(s)", def["maxIds"]).into()); }
    }
    let requested = req.get("values").map(|v| v.as_object().ok_or("values must be an object")).transpose()?;
    let requested = requested.map_or_else(|| json!({}), |o| Value::Object(o.clone()));
    let remembered_all = def["preference"].as_str().map(remembered).unwrap_or(Value::Null);
    let (found, context) = match def["defaults"].as_str() {
        // Edit > Preferences: the app settings, and the document's when it opens in a document's scope; on the tab asked for
        Some("preferences") => {
            // the document's preferences and languages are of the edit session (what the screen's documentInfo has too)
            let info = session.map(|s| document_info(s).map(|mut info| { crate::registered_queries::document_fields(s, &mut info); info })).transpose()?;
            let tab = req["tab"].as_str().unwrap_or("");
            if !tab.is_empty() {
                let known = def["tabs"].as_array().and_then(|t| t.iter().find(|t| t["id"] == tab)).ok_or_else(|| refuse("tab", "unknownTab", format!("no tab {tab}")))?;
                if known["document"] == true && info.is_none() { return Err(refuse("tab", "noDocument", format!("{tab} is of the document; open one"))); }
            }
            (preferences::defaults(def, &current_settings()?, info.as_ref()), json!({"tab": tab}))
        }
        Some(provider) if def["scope"] == "app" => (app_defaults(provider, &requested)?, json!({})),
        Some(provider) => document_defaults(provider, session.ok_or("no document open")?, &ids, &requested)?,
        None => (Map::new(), json!({})),
    };
    let mut choices = choice_lists(def, session, &ids, &requested, profiles)?;
    let found: Map<String, Value> = found.into_iter().map(|(key, value)| {
        let as_field = def["fields"].as_array().and_then(|l| l.iter().find(|f| f["name"] == key.as_str())).map_or(value.clone(), |f| typed(f, &value));
        (key, as_field)
    }).collect();
    // a style or item the document names that the list does not have is offered too (by its id), as the screens did
    for f in def["fields"].as_array().into_iter().flatten().filter(|f| f["type"] == "choice") {
        if let (Some(id), Some(list)) = (found.get(name_of(f)).and_then(Value::as_str).filter(|id| !id.is_empty()), choices[f["source"].as_str().unwrap_or("")].as_array_mut()) {
            if !list.iter().any(|c| c["value"] == id) { list.push(json!({"value": id, "label": id})); }
        }
    }
    for f in def["fields"].as_array().into_iter().flatten().filter(|f| f["type"] == "array") {
        for sub in f["fields"].as_array().into_iter().flatten().filter(|sub| sub["type"] == "choice") {
            for row in found.get(name_of(f)).and_then(Value::as_array).into_iter().flatten() {
                if let (Some(id), Some(list)) = (row[name_of(sub)].as_str().filter(|id| !id.is_empty()), choices[sub["source"].as_str().unwrap_or("")].as_array_mut()) {
                    if !list.iter().any(|c| c["value"] == id) { list.push(json!({"value": id, "label": id})); }
                }
            }
        }
    }
    let mut values = json!({});
    for f in def["fields"].as_array().ok_or("invalid dialog schema")? {
        let key = name_of(f);
        let seeded = f.get("from").map(|from| remembered(from["preference"].as_str().unwrap_or(""))[from["key"].as_str().unwrap_or("")].clone());
        let remembered = (f["remember"] != false).then(|| remembered_all[key].clone());
        values[key] = [remembered, seeded, found.get(key).cloned()].into_iter().flatten()
            .find_map(|v| checked(f, &v, &choices).ok()).unwrap_or_else(|| f["default"].clone());
    }
    // a choice that must name one of the list (`orFirst`) takes the first one offered when what it starts from is not there
    for f in def["fields"].as_array().into_iter().flatten().filter(|f| f["type"] == "choice" && f["orFirst"] == true) {
        let offered = allowed(f, &choices);
        if !offered.contains(&values[name_of(f)].as_str().unwrap_or("")) { if let Some(first) = offered.first() { values[name_of(f)] = json!(first); } }
    }
    // what a "changed only" dialog measures its changes from: the values before the ones asked for
    let initial = values.clone();
    for (key, value) in requested.as_object().into_iter().flatten() { values[key] = checked(&field(def, key)?, value, &choices)?; }
    // the fields a dialog ties together follow what was asked for
    fn pick<'k>(requested: &Value, keys: &[&'k str]) -> Option<&'k str> { keys.iter().copied().find(|k| requested.get(*k).is_some()) }
    match def["link"].as_str() {
        Some("polar") => if let Some(key) = pick(&requested, &["h", "v", "distance", "angle"]) { linked(def, &mut values, key); },
        Some("orientation") => if let Some(key) = pick(&requested, &["orientation", "width", "height"]) { linked(def, &mut values, key); },
        // (the preset asked for is where the values started; what else was asked for decides the size, orientation and [Custom])
        Some("newDocument") => { newdoc::fit_style(&mut values, &mut choices); newdoc::settle(&mut values); }
        Some("textFrameInsets") => if let Some(key) = pick(&requested, &["same", "insetTop", "insetLeft", "insetBottom", "insetRight"]) { linked(def, &mut values, key); },
        // (the commands handed in and the user's shortcuts: what the dialog shows is worked out)
        Some("shortcuts") => shortcuts::linked(&mut values, &mut choices, "")?,
        // (a preset asked for fills the fields: File ▸ PDF Presets ▸ <preset>)
        Some("pdfPresets") => pdfdlg::opened(&mut values, &choices, &requested, session.map(|s| &s.doc)),
        // (a print preset asked for fills the settings; settings asked for beside it stay on top: File ▸ Print)
        Some("printSettings") => printdlg::opened(&mut values, &requested),
        _ => {}
    }
    // a required value the dialog does not show is needed to open it; one it shows is asked for before OK
    for f in def["fields"].as_array().into_iter().flatten().filter(|f| f["required"] == true && f["hidden"] == true) {
        if values[name_of(f)].as_str().is_none_or(str::is_empty) { return Err(refuse(name_of(f), "required", format!("{} is required", name_of(f)))); }
    }
    Ok(json!({"dialog":name,"values":values,"initial":initial,"drafts":{},"ids":ids,"context":context,"choices":choices}))
}

/// Whether a request is for the app dialog: `scope` says so (`app` / `document`); else an open dialog decides (the document's first);
/// with none open, the dialog asked to open does, and otherwise the document's when one is open.
fn app_scoped(req: &Value, session: Option<&crate::Session>) -> Result<bool, Refusal> {
    // a dialog of `"scope": "either"` (Edit > Preferences) opens in the one asked for, else the document's when one is open
    let opening = if req["cmd"] == "openDialog" {
        let scope = definition(req["dialog"].as_str().ok_or("dialog must be a name")?)?["scope"].clone();
        if scope == "either" { None } else { Some(scope == "app") }
    } else { None };
    let asked = match req["scope"].as_str() {
        Some("app") => Some(true),
        Some("document") => Some(false),
        Some(other) => return Err(format!("scope must be app or document, not {other}").into()),
        None => None,
    };
    if let (Some(asked), Some(opening)) = (asked, opening) {
        if asked != opening { return Err(format!("{} is {} dialog", req["dialog"], if opening { "an app" } else { "a document" }).into()); }
    }
    Ok(asked.or(opening).unwrap_or_else(|| {
        let document_open = session.is_some_and(|s| s.dialog.lock().is_ok_and(|d| !d.is_null()));
        !document_open && (session.is_none() || APP.lock().is_ok_and(|d| !d.is_null()))
    }))
}

/// What a dialog request of a connection attached to the app works on (attach_paths.rs checks it before the request runs; the
/// request loop is one thread, so nothing changes in between): the scope it reaches, the dialog (the one asked for by openDialog,
/// else the one open in that scope), its definition's scope, and for applyDialog the app requests applying it would send (a
/// document dialog's edits are not among them; values the dialog would refuse send nothing, the request itself is then refused).
pub struct AttachedDialog {
    pub app: bool,
    pub dialog: String,
    pub declared: String,
    pub requests: Vec<Value>,
}

pub fn attached_dialog(req: &Value, session: Option<&crate::Session>) -> Result<Option<AttachedDialog>, String> {
    let cmd = req["cmd"].as_str().unwrap_or("");
    if !matches!(cmd, "openDialog" | "setDialogField" | "applyDialog") { return Ok(None); }
    let app = app_scoped(req, session).map_err(|r| r.message)?;
    let state = if app { APP.lock().map_err(|e| e.to_string())?.clone() } else {
        match session { Some(s) => s.dialog.lock().map_err(|e| e.to_string())?.clone(), None => Value::Null }
    };
    let dialog = if cmd == "openDialog" { req["dialog"].as_str().unwrap_or("").to_owned() } else { state["dialog"].as_str().unwrap_or("").to_owned() };
    if dialog.is_empty() { return Ok(None); }
    let def = definition(&dialog).map_err(|r| r.message)?;
    let mut requests = Vec::new();
    if cmd == "applyDialog" && !state.is_null() {
        let action = req["action"].as_str().unwrap_or("ok");
        if let Ok((list, values)) = apply_in(&def, &state, action, session) {
            // (a dialog of the document's requests, Print Booklet, sends them: a file written, a printer reached)
            if app || def["requests"] == true { requests = list; } else { requests = following(&def, &values).unwrap_or_default(); }
        }
    }
    Ok(Some(AttachedDialog { app, dialog, declared: def["scope"].as_str().unwrap_or("document").to_owned(), requests }))
}

/// The requests of an applied app dialog, in order; the first that fails stops it (its answer is the reply, the dialog stays open).
fn run_app(list: &[Value], session: Option<&crate::Session>) -> Result<(Vec<Value>, Map<String, Value>), Value> {
    let (mut replies, mut events) = (vec![], Map::new());
    for request in list {
        let cmd = request["cmd"].as_str().filter(|c| APP_COMMANDS.contains(c)).unwrap_or("");
        let mut reply = match cmd {
            "userSettings" => crate::user_settings::command(request, session).unwrap_or_else(|e| json!({"ok":false,"error":e})),
            "projectCreate" => crate::project::create(request),
            "documentPresets" => crate::document_presets::run(request).unwrap_or_else(|e| json!({"ok":false,"error":e})),
            "pdfPresets" => crate::pdf_presets::handle(request).unwrap_or_else(|| json!({"ok":false,"error":"not a pdfPresets request"})),
            // (a file of print presets read into the list or written from it; the list itself is the dialog's)
            "printPresets" => crate::printing::presets(request).unwrap_or_else(|e| json!({"ok":false,"error":e})),
            _ => json!({"ok":false,"error":format!("an app dialog cannot send {}", request["cmd"])}),
        };
        for key in ["_viewEvent", "_settingsEvent"] { if let Some(event) = reply.as_object_mut().and_then(|o| o.remove(key)) { events.insert(key.into(), event); } }
        if reply["ok"] == false { return Err(json!({"ok":false,"error":reply["error"],"code":reply["code"],"errorKind":"appCommand","command":request["cmd"]})); }
        if reply.get("ok").is_none() { reply["ok"] = json!(true); }
        replies.push(reply);
    }
    Ok((replies, events))
}

/// What a dialog with a `preview` shows of the values it holds now (the screen asks again after each change): the answer of the query the
/// preview names, or `{"state":"empty"}` while a field it needs (`needs`) is empty.
fn preview(def: &Value, state: &Value, session: Option<&crate::Session>) -> Result<Value, Refusal> {
    let spec = def.get("preview").ok_or("this dialog has no preview")?;
    // (a text the dialog takes trimmed is asked with it trimmed, as OK sends it)
    let mut trimmed = state["values"].clone();
    for f in def["fields"].as_array().into_iter().flatten().filter(|f| f["trim"] == true) {
        if let Some(t) = trimmed[name_of(f)].as_str().map(|t| t.trim().to_owned()) { trimmed[name_of(f)] = json!(t); }
    }
    let values = &trimmed;
    if spec["needs"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|k| values[k].as_str().is_some_and(|t| t.trim().is_empty())) {
        return Ok(json!({"state": "empty"}));
    }
    // (nothing to draw while the rows do not make what the dialog needs: a group of fewer than two inks)
    if spec["needsRows"] == true && rows_short(def, values).is_some() { return Ok(json!({"state": "empty"})); }
    let args = substitute(&spec["args"], values, &state["ids"], &Map::new())?;
    match spec["query"].as_str() {
        Some("mathmlLayout") => crate::mathml::layout(&args).map_err(Refusal::from),
        Some("mixedInkGroupPreview") => mixed_ink::preview(args),
        Some("bookletPlan") => booklet::preview(args, session),
        // File ▸ Print: the sheets of the job and settings (their pages and sizes: what the PDF proof and the printer take)
        Some("printingPlan") => printdlg::preview(values, session),
        // File ▸ Package: what the package would hold and whether the folder can be made (`okAllowed`)
        Some("packagePlan") => packagedlg::preview(values, session.ok_or("no document open")?),
        Some("convertToProfilePlan") => color_profile::preview(def, values, args, session),
        other => Err(format!("unknown dialog preview {other:?}").into()),
    }
}

/// A picker (`reload`) changed: every field but the picker and the ones that carry (`keep`) starts again from what the provider says of the item
/// it names (the provider reads the dialog's values, the picker's new one included); a field the provider says nothing of starts at its default.
fn reloaded(def: &Value, provider: &str, picker: &str, values: &mut Value, ids: &Value, choices: &Value, s: &crate::Session) -> Result<Value, Refusal> {
    let (found, context) = document_defaults(provider, s, ids, values)?;
    for f in def["fields"].as_array().into_iter().flatten() {
        let key = name_of(f);
        if key == picker || f["keep"] == true { continue; }
        let fresh = found.get(key).map(|v| typed(f, v)).filter(|v| checked(f, v, choices).is_ok()).unwrap_or_else(|| f["default"].clone());
        values[key] = fresh;
    }
    Ok(context)
}

fn step(req: &Value, session: Option<&crate::Session>, profiles: Option<&crate::Profiles>) -> Result<Value, Refusal> {
    let app = app_scoped(req, session)?;
    let mut state = if app { APP.lock().map_err(|e| e.to_string())? } else { session.ok_or("no document open")?.dialog.lock().map_err(|e| e.to_string())? };
    let cmd = req["cmd"].as_str().unwrap_or("");
    if cmd == "dialogState" { return Ok(json!({"dialogState":*state, "scope": if app { "app" } else { "document" }, "dialogs":schema()})); }
    let dialog = state["dialog"].clone();
    // what the request leaves the dialog as, sent to the screen of the document or of the app
    let echo = |state: &Value| match (app, session) {
        (true, _) => app_event(state, if dialog.is_null() { &state["dialog"] } else { &dialog }, None),
        (false, Some(s)) => event(s, state, None),
        (false, None) => Value::Null,
    };
    if cmd == "openDialog" {
        if !state.is_null() { return Err("a dialog is already open".into()); }
        let name = req["dialog"].as_str().ok_or("dialog must be a name")?;
        let def = definition(name)?;
        if def["scope"] == "either" && !app && session.is_some_and(|s| s.edit.is_none()) { return Err("the document is not open for editing; open the dialog in the app's scope".into()); }
        *state = opened(&def, name, req, if app && def["scope"] == "either" { None } else { session }, profiles)?;
        state["scope"] = json!(if app { "app" } else { "document" });
    } else if cmd == "cancelDialog" {
        *state = Value::Null;
    } else if app && cmd == "applyDialog" {
        let def = definition(state["dialog"].as_str().ok_or("no dialog open")?)?;
        let action = req["action"].as_str().unwrap_or("ok");
        let (list, values) = apply_in(&def, &state, action, session)?;
        // a new document needs the font store: main.rs runs it on its own thread and finishes the dialog (app_finished)
        if let [request] = list.as_slice() && MAIN_COMMANDS.contains(&request["cmd"].as_str().unwrap_or("")) { return Ok(json!({"_appRequest": request})); }
        let (replies, events) = match run_app(&list, session) { Ok(done) => done, Err(failed) => return Ok(failed) };
        // a button that keeps the dialog open (Save Preset): the lists are read again and the values it sets taken
        if def["staysOpen"].as_array().is_some_and(|k| k.contains(&json!(action))) {
            let mut kept = state["values"].clone();
            state["choices"] = choice_lists(&def, session, &state["ids"], &kept, profiles)?;
            let result = if replies.len() == 1 { replies[0].clone() } else { Value::Array(replies) };
            // (what `then` takes may be of the answer: `"$_result"`, a PDF preset file loaded)
            let mut answered = state["values"].clone();
            answered["_result"] = result.clone();
            for (key, template) in def["then"][action].as_object().into_iter().flatten() {
                kept[key] = substitute(template, &answered, &state["ids"], &Map::new())?;
            }
            if def["link"] == "newDocument" { newdoc::fit_style(&mut kept, &mut state["choices"]); newdoc::settle(&mut kept); }
            if def["link"] == "pdfPresets" { pdfdlg::settled(action, &mut kept, &state["choices"]); }
            // (the print presets manager: a file loaded is in its list)
            if def["link"] == "printSettings" { let mut choices = state["choices"].take(); printdlg::settled(action, &mut kept, &mut choices); state["choices"] = choices; }
            state["values"] = kept;
            computed(&def, &mut state);
            // (the screen reads its settings again from what was answered: the saved presets)
            let mut reply = json!({"dialogState":*state,"dialog":dialog,"result":result,"_dialogEvent":app_event(&state, &dialog, Some(&result))});
            for (key, event) in events { reply[key] = event; }
            return Ok(reply);
        }
        remember(&def, &values);
        // one request answers with its reply, several with the list of them
        let result = if replies.len() == 1 { replies[0].clone() } else { Value::Array(replies) };
        *state = Value::Null;
        let mut reply = json!({"dialogState":null,"dialog":dialog,"result":result,"_dialogEvent":app_event(&Value::Null, &dialog, Some(&result))});
        for (key, event) in events { reply[key] = event; }
        return Ok(reply);
    } else {
        let def = definition(state["dialog"].as_str().ok_or("no dialog open")?)?;
        if cmd == "dialogPreview" {
            return Ok(json!({"preview": preview(&def, &state, session)?}));
        } else if cmd == "setDialogField" {
            let key = req["field"].as_str().ok_or("field must be a name")?;
            if app && field(&def, key.split('/').next().unwrap_or(""))?["document"] == true {
                return Err(refuse(key, "noDocument", format!("{key} is a setting of the document; no document is open")));
            }
            // a field the engine has put off for the values the dialog holds (a print setting that does not apply), or a choice it cannot take now
            let top = key.split('/').next().unwrap_or("");
            if let Some(why) = state["off"][top].as_str() { return Err(refuse(key, "notApplicable", format!("{top} does not apply now ({why})"))); }
            if let Some(v) = req["value"].as_str() && state["offOptions"][top].as_array().is_some_and(|no| no.iter().any(|x| x == v)) {
                return Err(refuse(key, "notAvailable", format!("{top} cannot be {v} now")));
            }
            if let Some(draft) = req.get("draft") {
                let f = field(&def, key)?;
                if f["type"] != "number" || (!draft.is_string() && !draft.is_null()) { return Err(refuse(key, "notNumber", "draft must be numeric input text or null".into())); }
                state["drafts"][key] = draft.clone();
            } else {
                let f = field(&def, key.split('/').next().unwrap_or(""))?;
                if let Some(keep) = f["fixedExcept"].as_array() {
                    if !keep.contains(&req["value"]) && req["value"] != state["initial"][key] {
                        return Err(refuse(key, "notEditable", format!("{key} takes only {} or the value it opened with", f["fixedExcept"])));
                    }
                }
                let mut values = state["values"].clone();
                let key = set_path(&def, &mut values, &state["choices"], req)?;
                if def["link"] == "tocEntries" { tocdlg::linked(&state["values"], &mut values, &key, &state["choices"]); }
                if def["link"] == "userDictionary" { let before = state["values"].clone(); userdict::linked(&before, &mut values, &key)?; }
                if def["link"] == "preflightProfiles" { let before = state["values"].clone(); profdlg::linked(&before, &mut values, &mut state["choices"], &key, session)?; }
                // a changed value answers a question the dialog asked (replace the file?) with No
                if let Some(o) = state.as_object_mut() { o.remove("confirm"); }
                state["drafts"][&key] = Value::Null;
                linked(&def, &mut values, &key);
                // (a list the dialog manages itself: the transparency flattener presets; a built-in one is refused here)
                if def["link"] == "flattenerPresets" { flattener::linked(&mut values, &mut state["choices"], &key)?; }
                if def["link"] == "shortcuts" { shortcuts::linked(&mut values, &mut state["choices"], &key)?; }
                if def["link"] == "newDocument" { newdoc::linked(&mut values, &mut state["choices"], &key); }
                if def["link"] == "pdfPresets" { pdfdlg::linked(&mut values, &state["choices"], &key, session.map(|s| &s.doc))?; }
                if def["link"] == "printSettings" { printdlg::linked(&mut values, &mut state["choices"], &key)?; }
                if def["link"] == "package" { packagedlg::linked(&state["values"], &mut values, &key)?; }
                if let Some(provider) = field(&def, &key).ok().and_then(|f| f["reload"].as_str().map(str::to_owned)) {
                    state["context"] = reloaded(&def, &provider, &key, &mut values, &state["ids"], &state["choices"], session.ok_or("no document open")?)?;
                    state["initial"] = values.clone();
                    state["drafts"] = json!({});
                }
                state["values"] = values;
            }
        } else if cmd == "applyDialog" {
            // (a dialog of the document's requests, not edits: Print Booklet)
            if def["requests"] == true { return document_requests(&def, &mut state, &dialog, req, session); }
            let (list, values) = apply_in(&def, &state, req["action"].as_str().unwrap_or("ok"), session)?;
            // (what goes on from these commands' result, Create TOC with the saved style's id, is sent once they are in: main.rs)
            let (list, then): (Vec<Value>, Vec<Value>) = list.into_iter().partition(|c| c.get("$then").is_none());
            if list.is_empty() {
                // nothing for the document; the app requests that follow it are sent now
                let after = following(&def, &values)?;
                if !after.is_empty() {
                    let (replies, events) = match run_app(&after, session) { Ok(done) => done, Err(failed) => return Ok(failed) };
                    let result = if replies.len() == 1 { replies[0].clone() } else { Value::Array(replies) };
                    *state = Value::Null;
                    let s = session.ok_or("no document open")?;
                    let mut reply = json!({"dialogState":null,"unchanged":true,"dialog":dialog,"result":result,"_dialogEvent":applied_event(s, &state, None, &dialog, Some(&result))});
                    for (key, event) in events { reply[key] = event; }
                    return Ok(reply);
                }
                *state = Value::Null;
                return Ok(json!({"dialogState":*state,"unchanged":true,"_dialogEvent":echo(&state)}));
            }
            let label = req["label"].as_str().filter(|l| !l.is_empty()).map_or_else(|| def.get("label").unwrap_or(&def["title"]).clone(), |l| json!(l));
            let mut reply = json!({"_editRequest":{"cmd":"edit","label":label,"commands":list,"selectionChanged":true},"_dialogValues":values});
            if let Some(then) = then.into_iter().next() { reply["_dialogThen"] = then; }
            return Ok(reply);
        }
    }
    if let Ok(def) = definition(state["dialog"].as_str().unwrap_or("")) { computed(&def, &mut state); }
    Ok(json!({"dialogState":*state,"_dialogEvent":echo(&state)}))
}

/// What the engine works out of the values a dialog holds now (`"computed"`): File ▸ Print's settings that do not apply (`off`: the
/// field and why, as the screen's text; `offOptions`: the choices a field cannot take). The screen greys them; setting one is refused.
fn computed(def: &Value, state: &mut Value) {
    if def["computed"] != "printSettings" || !state.is_object() { return; }
    let (off, options) = printdlg::off(&state["values"]);
    state["off"] = Value::Object(off);
    state["offOptions"] = Value::Object(options);
}

pub fn run(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> { run_with(req, session, None) }

/// The edit of a dialog's `$then` commands (`_dialogThen`), made once the first commands are in the document `s`: `find` names the item
/// they made (`in` a list of the document's information, the one whose fields equal `where`; its `pick` becomes the value `as`). The
/// edit is part of the first one's undo step (`amend`).
pub fn then_request(s: &crate::Session, then: &Value) -> Result<Value, String> {
    let find = &then["find"];
    let mut info = document_info(s).map_err(|r| r.message)?;
    crate::registered_queries::document_fields(s, &mut info);
    let list = find["in"].as_str().unwrap_or("");
    let found = info[list].as_array().into_iter().flatten().find(|x| when_holds(&find["where"], x))
        .ok_or_else(|| format!("the dialog did not find what it made in {list}"))?;
    let mut values = then["values"].clone();
    values[find["as"].as_str().unwrap_or("")] = found[find["pick"].as_str().unwrap_or("id")].clone();
    let commands = substitute(&then["$then"], &values, &then["ids"], &Map::new()).map_err(|r| r.message)?;
    Ok(json!({"cmd": "edit", "amend": true, "commands": commands, "selectionChanged": true}))
}

/// `run` with the colour profiles of this computer (a dialog may list them: Image Color Settings).
pub fn run_with(req: &Value, session: Option<&crate::Session>, profiles: Option<&crate::Profiles>) -> Result<Value, String> {
    match step(req, session, profiles) {
        Ok(v) => Ok(v),
        Err(Refusal { field: Some(field), reason, message }) => Ok(json!({"ok":false,"error":message,"errorKind":"dialogField","field":field,"reason":reason})),
        Err(Refusal { message, .. }) => Err(message),
    }
}

/// The answer of an app dialog's request that main.rs ran (`_appRequest`, a new document): done, the dialog closes and the reply
/// carries what was made (`result`) with the event to send first; refused, the dialog stays open with its values.
pub fn app_finished(request: &Value, reply: &Value) -> (Value, Option<Value>) {
    let Ok(mut state) = APP.lock() else { return (json!({"ok":false,"error":"dialog state"}), None) };
    let dialog = state["dialog"].clone();
    if reply["ok"] != true {
        // a refusal the dialog asks about (`confirm`: "<code>: <path>", the file exists): it stays open with the question
        let error = reply["error"].as_str().unwrap_or("");
        let (code, detail) = error.split_once(": ").unwrap_or((error, ""));
        let def = definition(dialog.as_str().unwrap_or("")).unwrap_or(Value::Null);
        if def["confirm"]["code"] == code && !detail.is_empty() {
            let name = detail.rsplit('/').next().unwrap_or(detail);
            state["confirm"] = json!({"code": code, "name": name, "path": detail, "action": def["confirm"]["action"]});
            return (json!({"ok":true,"dialogState":*state,"confirm":state["confirm"]}), Some(app_event(&state, &dialog, None)));
        }
        // (the refusal's code, `bad_name: a:b` as the engine words it, is what the dialog explains: `errors`)
        let mut failed = json!({"ok":false,"error":reply["error"],"code":reply["code"].as_str().unwrap_or(code),"detail":detail,"errorKind":"appCommand","command":request["cmd"]});
        for key in ["messageId", "messageArgs"] { if let Some(v) = reply.get(key) { failed[key] = v.clone(); } }
        return (failed, None);
    }
    *state = Value::Null;
    let event = app_event(&Value::Null, &dialog, Some(reply));
    (json!({"ok":true,"dialogState":null,"dialog":dialog,"result":reply}), Some(event))
}

pub fn applied(s: &crate::Session, values: &Value, reply: &Value) -> Value {
    let mut state = s.dialog.lock().expect("dialog state");
    let dialog = state["dialog"].clone();
    let mut result = None;
    if let Ok(def) = definition(state["dialog"].as_str().unwrap_or("")) {
        remember(&def, values);
        // the app requests that follow the document's edit (the app settings of Edit > Preferences); what they answered goes with the event
        match following(&def, values) {
            Ok(list) if !list.is_empty() => result = Some(match run_app(&list, Some(s)) {
                Ok((replies, _)) => if replies.len() == 1 { replies[0].clone() } else { Value::Array(replies) },
                Err(failed) => failed,
            }),
            Ok(_) => {}
            Err(Refusal { message, .. }) => result = Some(json!({"ok":false,"error":message,"errorKind":"appCommand"})),
        }
    }
    *state = Value::Null;
    let mut done = match result { Some(result) => applied_event(s, &state, Some(reply), &dialog, Some(&result)), None => event(s, &state, Some(reply)) };
    // (which dialog was applied with which values: a panel takes what it made from them, the new colour group)
    done["detail"]["applied"] = json!({"dialog": dialog, "values": values});
    done
}

/// The event of a document dialog that was applied with app requests after it: which dialog, and what they answered.
fn applied_event(s: &crate::Session, state: &Value, edit: Option<&Value>, dialog: &Value, result: Option<&Value>) -> Value {
    json!({"event":"dialog-state","path":s.source_path,"detail":{"dialogState":state,"edit":edit,"dialog":dialog,"result":result}})
}

#[cfg(test)]
#[path = "../../../test_support.rs"]
mod test_support;

#[cfg(test)]
mod tests;
