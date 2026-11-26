//! Where the window puts the project explorer and the tools: a user setting of this computer (never a project).
//! `folio` (the default, user decision 2026-11-20): a VS Code style sidebar on the left, the explorer above, the tools and
//! the fill and stroke below; the panel dock on the right without the project panel. `classic`: InDesign's Essentials Classic
//! layout (the tools down the left edge, the project panel in the dock's column). The panel dock itself is `panelState`.
use serde_json::{Map, Value, json};

const LAYOUTS: [&str; 2] = ["folio", "classic"];
/// The sidebar's width in screen pixels, and the share of its height the explorer takes (the tools take the rest).
const WIDTH: (f64, f64) = (180.0, 640.0);
const SPLIT: (f64, f64) = (0.15, 0.85);

fn defaults() -> Value {
    json!({"layout": "folio", "sidebar": {"open": true, "width": 264, "split": 0.64, "toolsOpen": true, "explorerOpen": true}})
}

/// The stored value with every field filled in (a field that is missing or out of range reads as its default).
fn state(settings: &Value) -> Value {
    let mut out = defaults();
    let stored = &settings["workspaceLayout"];
    if let Some(layout) = stored["layout"].as_str().filter(|l| LAYOUTS.contains(l)) { out["layout"] = json!(layout); }
    let side = &stored["sidebar"];
    for key in ["open", "toolsOpen", "explorerOpen"] {
        if let Some(v) = side[key].as_bool() { out["sidebar"][key] = json!(v); }
    }
    if let Some(w) = side["width"].as_f64().filter(|w| (WIDTH.0..=WIDTH.1).contains(w)) { out["sidebar"]["width"] = json!(w.round()); }
    if let Some(s) = side["split"].as_f64().filter(|s| (SPLIT.0..=SPLIT.1).contains(s)) { out["sidebar"]["split"] = json!(s); }
    out
}

fn apply(state: &mut Value, patch: &Map<String, Value>) -> Result<(), String> {
    for (key, value) in patch {
        match key.as_str() {
            "layout" => {
                let l = value.as_str().filter(|l| LAYOUTS.contains(l)).ok_or("layout must be \"folio\" or \"classic\"")?;
                state["layout"] = json!(l);
            }
            "sidebar" => {
                let side = value.as_object().ok_or("sidebar must be an object")?;
                for (k, v) in side {
                    match k.as_str() {
                        "open" | "toolsOpen" | "explorerOpen" => { state["sidebar"][k] = json!(v.as_bool().ok_or(format!("sidebar.{k} must be a boolean"))?); }
                        "width" => {
                            let w = v.as_f64().filter(|w| w.is_finite()).ok_or("sidebar.width must be a number")?;
                            state["sidebar"]["width"] = json!(w.clamp(WIDTH.0, WIDTH.1).round());
                        }
                        "split" => {
                            let s = v.as_f64().filter(|s| s.is_finite()).ok_or("sidebar.split must be a number")?;
                            state["sidebar"]["split"] = json!(s.clamp(SPLIT.0, SPLIT.1));
                        }
                        _ => return Err(format!("unknown sidebar field: {k}")),
                    }
                }
            }
            _ => return Err(format!("unknown workspace layout field: {key}")),
        }
    }
    Ok(())
}

/// `workspaceLayout` reads; `setWorkspaceLayout` patches (`{"patch":{"layout":"classic"}}`, `{"patch":{"sidebar":{"open":false}}}`)
/// and tells the screen (the `user-settings` event, as `setPanelState` does).
pub fn run(req: &Value) -> Result<Value, String> {
    let set = req["cmd"] == "setWorkspaceLayout";
    let settings = crate::user_settings::update(|settings, _| {
        if set {
            let patch = req["patch"].as_object().ok_or("patch must be an object")?;
            let mut next = state(&Value::Object(settings.clone()));
            apply(&mut next, patch)?;
            settings.insert("workspaceLayout".into(), next);
        }
        Ok(())
    })?;
    let mut result = json!({"workspaceLayout": state(&settings)});
    if set { result["_settingsEvent"] = crate::user_settings::event(&settings, &req["origin"]); }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fills_defaults_and_refuses_what_the_screen_cannot_show() {
        let s = state(&json!({}));
        assert_eq!((s["layout"].clone(), s["sidebar"]["open"].clone(), s["sidebar"]["width"].clone()), (json!("folio"), json!(true), json!(264)));
        let s = state(&json!({"workspaceLayout": {"layout": "nope", "sidebar": {"width": 9000, "split": 0.3, "open": false}}}));
        assert_eq!((s["layout"].clone(), s["sidebar"]["width"].clone(), s["sidebar"]["split"].clone(), s["sidebar"]["open"].clone()), (json!("folio"), json!(264), json!(0.3), json!(false)));
        let mut s = state(&json!({}));
        apply(&mut s, json!({"layout": "classic", "sidebar": {"width": 100, "split": 2.0}}).as_object().unwrap()).unwrap();
        assert_eq!((s["layout"].clone(), s["sidebar"]["width"].clone(), s["sidebar"]["split"].clone()), (json!("classic"), json!(180.0), json!(0.85)));
        assert!(apply(&mut s, json!({"layout": "grid"}).as_object().unwrap()).is_err());
        assert!(apply(&mut s, json!({"sidebar": {"open": "yes"}}).as_object().unwrap()).is_err());
        assert!(apply(&mut s, json!({"dock": {}}).as_object().unwrap()).is_err());
    }
}
