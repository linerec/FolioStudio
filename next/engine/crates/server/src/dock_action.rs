//! `dockAction` (uix-d2): one dock layout command (move, join, split, float, pop out, close, open a panel; save, load, reset a
//! workspace) for a tool such as the MCP server, the same one a person's drag or menu becomes.
//!
//! The layout logic has one owner, the screen's pure functions (`renderer/src/ui/docklayout.ts`, which phones and tests use too);
//! it is not written a second time here. The engine hands the command to the screen as an event (`dock-action`, with the request it
//! answers) and goes on with other requests; the screen applies it, saves the layout in the user settings as it always does
//! (`setPanelState`), and sends its structured result back (`dockActionDone`), which becomes the reply to the waiting request.
//! When no screen answers in time (no window, or a window still starting) the reply is the structured error `noScreen`.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::Duration;

/// How long a dock action waits for the screen.
pub const SCREEN_WAIT: Duration = Duration::from_secs(5);

/// The requests waiting for the screen's answer (by their id as JSON text: a screen's number or an attached client's routed id).
static WAITING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn waiting() -> std::sync::MutexGuard<'static, BTreeSet<String>> {
    WAITING.lock().unwrap_or_else(|e| e.into_inner())
}

/// `dockAction` answers later (`_screenRequest`: the event for the screen); `dockActionDone` answers the screen at once and the
/// waiting request (`_screenReply`, a whole reply with that request's id).
pub fn run(req: &Value) -> Result<Value, String> {
    match req["cmd"].as_str() {
        Some("dockAction") => {
            let action = &req["action"];
            if !action.is_object() || !action["op"].is_string() {
                return Err("action must be an object with op (a dock command: movePanel, moveGroup, floatGroup, popOut, dockFloating, closePanel, openPanel, saveWorkspace, loadWorkspace, resetWorkspace, describe, …)".into());
            }
            Ok(json!({"_screenRequest": {"event": "dock-action", "action": action}}))
        }
        Some("dockActionDone") => {
            let id = req.get("request").cloned().unwrap_or(Value::Null);
            if id.is_null() { return Err("request is required".into()); }
            // an answer that came too late (the caller was told noScreen already) is dropped
            if !waiting().remove(&id.to_string()) { return Ok(json!({"late": true})); }
            let result = &req["result"];
            let mut reply = if result["ok"] == true {
                let mut r = json!({"ok": true, "changed": result["changed"] == true, "layout": result.get("layout").cloned().unwrap_or(json!([]))});
                for key in ["dock", "workspace", "workspaces", "popped"] { if let Some(v) = result.get(key) { r[key] = v.clone(); } }
                r
            } else {
                let kind = result["error"].as_str().unwrap_or("invalid");
                json!({"ok": false, "code": kind, "errorKind": kind, "error": result["message"].as_str().unwrap_or("the screen refused the dock action")})
            };
            reply["id"] = id;
            Ok(json!({"late": false, "_screenReply": reply}))
        }
        _ => Err("unknown dock command".into()),
    }
}

/// The request `id` waits for the screen from now on.
pub fn wait(id: &Value) { waiting().insert(id.to_string()); }

/// The screen did not answer `id` in time: true when it was still waiting (the caller is to be told `noScreen`).
pub fn give_up(id: &Value) -> bool { waiting().remove(&id.to_string()) }

/// The reply for a request the screen did not answer.
pub fn no_screen(id: &Value) -> Value {
    json!({"id": id, "ok": false, "code": "noScreen", "errorKind": "noScreen",
        "error": "no Folio window answered: the dock is the screen's (open the app window, or wait until it has started)"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hands_the_action_to_the_screen_and_answers_once() {
        let asked = run(&json!({"cmd": "dockAction", "action": {"op": "floatGroup", "zone": "left", "group": 0}})).unwrap();
        assert_eq!(asked["_screenRequest"]["event"], "dock-action");
        assert_eq!(asked["_screenRequest"]["action"]["op"], "floatGroup");
        assert!(run(&json!({"cmd": "dockAction", "action": {"zone": "left"}})).is_err());
        let id = json!("mcp:1:7");
        wait(&id);
        let done = run(&json!({"cmd": "dockActionDone", "request": id, "result": {"ok": true, "changed": true, "layout": [{"panel": "explorer"}]}})).unwrap();
        assert_eq!(done["_screenReply"]["id"], "mcp:1:7");
        assert_eq!(done["_screenReply"]["changed"], true);
        // a second answer, or one after giving up, is late
        assert_eq!(run(&json!({"cmd": "dockActionDone", "request": id, "result": {"ok": true}})).unwrap()["late"], true);
        let other = json!(42);
        wait(&other);
        assert!(give_up(&other));
        assert!(!give_up(&other));
        assert_eq!(no_screen(&other)["errorKind"], "noScreen");
        let refused = { wait(&json!(43)); run(&json!({"cmd": "dockActionDone", "request": 43, "result": {"ok": false, "error": "badTarget", "message": "no left group 9"}})).unwrap() };
        assert_eq!(refused["_screenReply"]["errorKind"], "badTarget");
    }
}
