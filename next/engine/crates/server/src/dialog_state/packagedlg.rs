//! File ▸ Package (`package`, a document dialog of the open document's requests, in stages): choose what goes into the print
//! house's folder and where (`choose`), make it on its own thread with the progress in the dialog's values (`running`: `progress`,
//! `cancelling`), then what it made (`done`: `result`) or why not (`failed`: `failure`; a folder of that name that appeared
//! meanwhile sends it back to `choose` with the next free name). The plan the dialog shows is `packagePlan`'s; what OK sends is
//! the `package` request the screen sent for the same values, then the settings it remembered (`userSettings`).
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

fn flag(v: &Value, key: &str, default: bool) -> bool { v[key].as_bool().unwrap_or(default) }
fn text<'v>(v: &'v Value, key: &str) -> &'v str { v[key].as_str().unwrap_or("") }

/// What File ▸ Package starts from: the choices, folder and contact the user's settings remember (the folder: the package folder,
/// else the export folder, else the one handed in: `documents`), the folder name of the document's (`namePattern`'s {name}),
/// the PDF settings exported last, the search folders of the links; whether this computer can make the PDF (the document's CMYK
/// profile), whether the preflight has errors (`preflightErrors` handed in).
pub fn defaults(s: &crate::Session, requested: &Value) -> (Map<String, Value>, Value) {
    let settings = crate::user_settings::run(&json!({})).map(|v| v["settings"].clone()).unwrap_or(Value::Null);
    let choice = &settings["package"];
    let contact = &settings["packageContact"];
    let folder = [text(&settings, "packageFolder"), text(&settings, "exportFolder"), text(requested, "documents")].into_iter()
        .find(|f| !f.is_empty() && Path::new(f).is_dir()).unwrap_or("");
    let base = Path::new(&s.source_path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    let pattern = requested["namePattern"].as_str().filter(|p| p.contains("{name}")).unwrap_or("{name} 폴더");
    let export = if settings["export"].is_object() { settings["export"].clone() }
        else { json!({"bleed": true, "marks": {"crop": true, "bleed": false, "registration": false, "pageInfo": false}, "spreads": false}) };
    let values = json!({
        "links": flag(choice, "links", true), "fonts": flag(choice, "fonts", true), "pdf": flag(choice, "pdf", true), "cjkFonts": flag(choice, "cjkFonts", false),
        "folder": folder, "name": pattern.replace("{name}", &base),
        "contact": text(contact, "contact"), "company": text(contact, "company"), "phone": text(contact, "phone"), "email": text(contact, "email"),
        "pdfPossible": s.cmyk_profile.is_some(), "hasPreflightErrors": requested["preflightErrors"].as_u64().unwrap_or(0) > 0,
        "export": export, "roots": if settings["linkRoots"].is_array() { settings["linkRoots"].clone() } else { json!([]) },
        "stage": "choose",
    });
    (values.as_object().cloned().unwrap_or_default(), json!({}))
}

/// The folder the package is made as: the chosen folder and the name in it (the name without the spaces around it).
fn dir(values: &Value) -> Option<PathBuf> {
    let (folder, name) = (text(values, "folder").trim(), text(values, "name").trim());
    (!folder.is_empty() && !name.is_empty()).then(|| Path::new(folder).join(name))
}

/// The `packagePlan` request of the values (what the dialog shows: the counts, what to check, whether the folder can be made).
pub fn plan_request(values: &Value) -> Value {
    let mut req = json!({"cmd": "packagePlan", "links": flag(values, "links", true), "fonts": flag(values, "fonts", true),
        "pdf": flag(values, "pdf", true), "cjkFonts": flag(values, "cjkFonts", false), "roots": values["roots"]});
    if let Some(d) = dir(values) { req["dir"] = json!(d.to_string_lossy()); }
    req
}

/// The dialog's preview: `packagePlan`'s answer, the document's pages, and whether OK can go (a folder that can be made).
pub fn preview(values: &Value, s: &crate::Session) -> Result<Value, Refusal> {
    let answer = crate::package::plan_command(&plan_request(values), s);
    if answer["ok"] != true { return Err(answer["error"].as_str().unwrap_or("packagePlan failed").to_owned().into()); }
    let pages = s.doc.spreads.iter().map(|sp| sp.pages.len()).sum::<usize>();
    Ok(json!({"ok": true, "plan": answer["plan"], "pages": pages, "okAllowed": answer["plan"]["dir"]["ok"] == true}))
}

/// After `changed` was set: back to the choices only from what was made or what failed; a name typed clears the note that the
/// engine renamed it.
pub fn linked(before: &Value, values: &mut Value, changed: &str) -> Result<(), Refusal> {
    match changed {
        "stage" => {
            let (was, now) = (text(before, "stage"), text(values, "stage"));
            if !(now == "choose" && (was == "done" || was == "failed")) {
                return Err(refuse("stage", "notEditable", format!("a package goes back to its choices only once it is done or failed, not from {was} to {now}")));
            }
            values["progress"] = json!({});
            values["failure"] = json!({});
            values["cancelling"] = json!(false);
        }
        "name" => values["renamed"] = json!(false),
        _ => {}
    }
    Ok(())
}

/// The requests of a button: OK makes the package (the request the screen sent) and remembers the choices, folder and contact
/// (the settings the screen saved); Cancel (while it runs) asks it to stop.
pub fn requests(action: &str, values: &Value) -> Result<Vec<Value>, Refusal> {
    match action {
        "ok" => {
            if text(values, "stage") != "choose" { return Err(refuse("stage", "notApplicable", "the package is being made or was made: go back to the choices first".into())); }
            let dir = dir(values).ok_or_else(|| refuse("name", "required", "folder and name are required".into()))?;
            let choice = json!({"links": flag(values, "links", true), "fonts": flag(values, "fonts", true), "pdf": flag(values, "pdf", true), "cjkFonts": flag(values, "cjkFonts", false)});
            let contact = json!({"contact": text(values, "contact"), "company": text(values, "company"), "phone": text(values, "phone"), "email": text(values, "email")});
            let mut instructions = contact.clone();
            instructions["notes"] = json!(text(values, "notes"));
            let package = json!({"cmd": "package", "dir": dir.to_string_lossy(), "links": choice["links"], "fonts": choice["fonts"],
                "pdf": choice["pdf"] == true && values["pdfPossible"] == true, "cjkFonts": choice["cjkFonts"], "export": values["export"],
                "roots": values["roots"], "instructions": instructions, "appVersion": text(values, "appVersion"),
                "utcOffsetMinutes": values["utcOffsetMinutes"]});
            Ok(vec![package, json!({"cmd": "userSettings", "patch": {"packageFolder": text(values, "folder").trim(), "package": choice, "packageContact": contact}})])
        }
        "cancel" => Ok(vec![json!({"cmd": "packageCancel"})]),
        other => Err(format!("this dialog has no action {other}").into()),
    }
}

/// OK was pressed: the package is being made (its first progress).
pub fn started(values: &mut Value) {
    values["stage"] = json!("running");
    values["progress"] = json!({"stage": "plan", "done": 0, "total": 0, "files": 0, "filesTotal": 0, "pdf": null});
    values["cancelling"] = json!(false);
    values["renamed"] = json!(false);
    values["failure"] = json!({});
    values["result"] = json!({});
}

/// A progress event of the package being made, into the values (`progress`); false when the dialog is not making one.
pub fn progressed(values: &mut Value, p: &Value) -> bool {
    if values["stage"] != "running" { return false; }
    let mut now = json!({});
    for key in ["stage", "done", "total", "files", "filesTotal", "pdf"] { if let Some(v) = p.get(key) { now[key] = v.clone(); } }
    values["progress"] = now;
    true
}

/// The `package` request answered: what it made (`done`), or why not (`failed`, its code, text and message id); a folder of that
/// name that appeared meanwhile goes back to the choices with the next free name (`renamed`), as the screen did.
pub fn finished(values: &mut Value, reply: &Value) {
    values["cancelling"] = json!(false);
    values["progress"] = json!({});
    if reply["ok"] == true {
        values["stage"] = json!("done");
        values["result"] = reply["result"].clone();
        return;
    }
    if reply["code"] == "exists" && let Some(next) = reply["suggestion"].as_str() {
        values["stage"] = json!("choose");
        values["name"] = json!(next);
        values["renamed"] = json!(true);
        return;
    }
    values["stage"] = json!("failed");
    values["failure"] = json!({"code": reply["code"].as_str().unwrap_or("io"), "error": reply["error"], "messageId": reply["messageId"], "messageArgs": reply["messageArgs"]});
}
