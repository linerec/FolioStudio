//! Dispatch and completion of requests from document dialogs.

use super::{Refusal, DOCUMENT_REQUESTS, THREAD_REQUESTS, apply_in, applied_event, computed, definition, event, packagedlg, printdlg, progress_event, substitute};
use serde_json::{Map, Value, json};

/// One request of a document dialog's button ([`DOCUMENT_REQUESTS`]), answered as the query answers it (`ok: false` when refused).
fn document_request(request: &Value, s: &crate::Session) -> Value {
    match request["cmd"].as_str().filter(|c| DOCUMENT_REQUESTS.contains(c)).unwrap_or("") {
        "bookletFile" => crate::printing::booklet_file(request, Some(s)).unwrap_or_else(|e| json!({"ok": false, "error": e})),
        "printingFile" => crate::printing::file(request, Some(s)).unwrap_or_else(|e| json!({"ok": false, "error": e})),
        // (File ▸ Package's Cancel while it runs; the package itself runs on its own thread: THREAD_REQUESTS)
        "packageCancel" => crate::package::cancel(),
        "printToDevice" => crate::printing::print_to_device(request, Some(s)),
        "cancelPrintJob" => crate::printing::cancel_print_job(request),
        // (the settings a dialog remembers once its request is through: File ▸ Export PDF's last export)
        "userSettings" => crate::user_settings::command(request, Some(s)).unwrap_or_else(|e| json!({"ok": false, "error": e})),
        _ => json!({"ok": false, "error": format!("a document dialog cannot send {}", request["cmd"])}),
    }
}

/// `applyDialog` of a dialog whose buttons send the open document's requests instead of edits (`"requests": true`: Print Booklet writes its
/// PDF, sends to a printer, takes a job back). They run now, in order; the first refused stops it with a structured failure (`errorKind:
/// "dialogRequest"`, `code` the query's `errorKind`) and the dialog stays as it was. A button of `staysOpen` keeps the dialog open with what
/// `then` takes from the answer (`"$_result"`); another closes it, the answer in `result`. An attached MCP connection does not get here:
/// [`super::attached_dialog`] lists these requests and attach_paths.rs refuses them (a file written, a printer reached are not reviewed MCP paths).
pub(super) fn document_requests(def: &Value, state: &mut Value, dialog: &Value, req: &Value, session: Option<&crate::Session>) -> Result<Value, Refusal> {
    let s = session.ok_or("no document open")?;
    let action = req["action"].as_str().unwrap_or("ok");
    let (list, _) = apply_in(def, state, action, session)?;
    // (one that takes long, a PDF written, runs on its own thread in main.rs; the rest after it: document_finished)
    if list.first().is_some_and(|r| THREAD_REQUESTS.contains(&r["cmd"].as_str().unwrap_or(""))) {
        // (File ▸ Package goes on to its running stage at once; the screen is told before the thread starts)
        if def["link"] == "package" {
            packagedlg::started(&mut state["values"]);
            return Ok(json!({"_documentRequest": {"action": action, "requests": list}, "_dialogEvent": progress_event(s, state)}));
        }
        return Ok(json!({"_documentRequest": {"action": action, "requests": list}}));
    }
    run_requests(def, state, dialog, action, &list, vec![], s)
}

/// The requests of a dialog's button (after the ones answered already, `replies`), then the dialog kept open with what `then` takes
/// from the answers (`staysOpen`), or closed with them in `result`.
fn run_requests(def: &Value, state: &mut Value, dialog: &Value, action: &str, list: &[Value], mut replies: Vec<Value>, s: &crate::Session) -> Result<Value, Refusal> {
    for request in list {
        let reply = document_request(request, s);
        if reply["ok"] == false {
            return Ok(json!({"ok": false, "error": reply["error"], "code": reply["errorKind"].as_str().unwrap_or("failed"), "errorKind": "dialogRequest", "command": request["cmd"]}));
        }
        replies.push(reply);
    }
    let result = if replies.len() == 1 { replies[0].clone() } else { Value::Array(replies) };
    if def["staysOpen"].as_array().is_some_and(|k| k.contains(&json!(action))) {
        let mut answered = state["values"].clone();
        answered["_result"] = result.clone();
        let mut kept = state["values"].clone();
        for (key, template) in def["then"][action].as_object().into_iter().flatten() { kept[key] = substitute(template, &answered, &state["ids"], &Map::new())?; }
        // (File ▸ Print's Save Preset: the presets read again, the new one chosen)
        if def["link"] == "printSettings" { let mut choices = state["choices"].take(); printdlg::settled(action, &mut kept, &mut choices); state["choices"] = choices; }
        state["values"] = kept;
        computed(def, state);
        // (a package being made: the screen takes it while the package request is still out)
        let told = if state["values"]["stage"] == "running" { progress_event(s, state) } else { event(s, state, None) };
        return Ok(json!({"dialogState": *state, "dialog": dialog, "result": result, "_dialogEvent": told}));
    }
    *state = Value::Null;
    Ok(json!({"dialogState": null, "dialog": dialog, "result": result, "_dialogEvent": applied_event(s, &Value::Null, None, dialog, Some(&result))}))
}

/// The answer `reply` of a dialog's request main.rs ran on its own thread (`_documentRequest`: File ▸ Export PDF's PDF): the requests
/// after it run and the dialog goes on as [`document_requests`] does; refused, the dialog stays as it was. Returns the reply to send
/// and the event to send first.
pub fn document_finished(s: &crate::Session, job: &Value, reply: &Value) -> (Value, Option<Value>) {
    let Ok(mut state) = s.dialog.lock() else { return (json!({"ok": false, "error": "dialog state"}), None) };
    // File ▸ Package: made or not, the settings it remembers are saved (as the screen saved them first), and the dialog shows what
    // happened (done, failed, or back to the choices with the next free name); the answer is the dialog's
    if definition(state["dialog"].as_str().unwrap_or("")).is_ok_and(|def| def["link"] == "package") {
        for request in job["requests"].as_array().map(|l| &l[1..]).into_iter().flatten() { let _ = document_request(request, s); }
        packagedlg::finished(&mut state["values"], reply);
        let dialog = state["dialog"].clone();
        return (json!({"ok": true, "dialogState": *state, "dialog": dialog, "packaged": reply}), Some(progress_event(s, &state)));
    }
    let request = &job["requests"][0];
    if reply["ok"] != true {
        return (json!({"ok": false, "error": reply["error"], "code": reply["errorKind"].as_str().unwrap_or("failed"), "errorKind": "dialogRequest", "command": request["cmd"]}), None);
    }
    let dialog = state["dialog"].clone();
    // (closed meanwhile: the file is written, there is no dialog to go on with)
    let Ok(def) = definition(dialog.as_str().unwrap_or("")) else { return (json!({"ok": true, "dialogState": null, "result": reply}), None) };
    let rest: Vec<Value> = job["requests"].as_array().map(|l| l[1..].to_vec()).unwrap_or_default();
    let action = job["action"].as_str().unwrap_or("ok");
    match run_requests(&def, &mut state, &dialog, action, &rest, vec![reply.clone()], s) {
        Ok(mut done) => {
            let event = done.as_object_mut().and_then(|o| o.remove("_dialogEvent"));
            // (an answer of its own, as the request loop's answers are: ok unless refused)
            if done.get("ok").is_none() { done["ok"] = json!(true); }
            (done, event)
        }
        Err(Refusal { message, .. }) => (json!({"ok": false, "error": message}), None),
    }
}
