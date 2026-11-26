//! Edit > Keyboard Shortcuts (an app dialog, `"link": "shortcuts"`): the user's own shortcuts (app settings `shortcuts`: command id →
//! its keys, an empty list for none), over the commands' default keys. The commands are the screen's (menus, panels and contextual keys:
//! `commands`, handed in when the dialog opens, each `{id, label, keys, fallThrough, listed}`); the dialog lists the `listed` ones that
//! match the search (`shortcutSearch`), shows the keys of the one chosen (`shortcutCommand`) and takes a key (`shortcutKey`: a normalized
//! spec such as `Ctrl+Shift+D`); a key another command has is a conflict (`hasConflict`, `conflictWith`), except a contextual default the
//! user's key takes over (`fallThrough`). Assign, Remove and Reset (`listAction`) change the user's shortcuts; OK saves them.
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};

/// A key spec's parts: Ctrl, Alt, Shift and the key codes it stands for (as the screen's `parseKey`: a digit is the row or the keypad).
fn parse(spec: &str) -> (bool, bool, bool, Vec<String>) {
    let (mods, key) = match spec.strip_suffix("++") { Some(head) => (head, "+"), None => spec.rsplit_once('+').unwrap_or(("", spec)) };
    let has = |m: &str| mods.split('+').any(|x| x == m);
    let named = |k: &str| match k {
        "=" | "+" => "Equal", "-" => "Minus", ";" => "Semicolon", "'" => "Quote", "[" => "BracketLeft", "]" => "BracketRight", "\\" => "Backslash",
        "," => "Comma", "." => "Period", "/" => "Slash", "`" => "Backquote", "Esc" | "Escape" => "Escape", "Up" => "ArrowUp", "Down" => "ArrowDown",
        "Left" => "ArrowLeft", "Right" => "ArrowRight", other => other,
    }.to_owned();
    let codes = if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) { vec![format!("Key{}", key.to_ascii_uppercase())] }
        else if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) { vec![format!("Digit{key}"), format!("Numpad{key}")] }
        else if key == "=" || key == "+" { vec!["Equal".into(), "NumpadAdd".into()] }
        else if key == "-" { vec!["Minus".into(), "NumpadSubtract".into()] }
        else { vec![named(key)] };
    (has("Ctrl"), has("Alt"), has("Shift"), codes)
}

/// Whether two specs are one key (the screen's `sameShortcut`).
fn same(a: &str, b: &str) -> bool {
    let (x, y) = (parse(a), parse(b));
    x.0 == y.0 && x.1 == y.1 && x.2 == y.2 && x.3.iter().any(|c| y.3.contains(c))
}

/// A key as the dialog takes it: Ctrl and/or Alt with a key, or a function key F1 to F24 (with any of them), the modifiers in the order
/// Ctrl, Alt, Shift and a letter in capitals (`shift+ctrl+d` → `Ctrl+Shift+D`); none for anything else (Escape, Space, Tab, a bare letter).
pub fn normalized(spec: &str) -> Option<String> {
    let (mods, key) = match spec.trim().strip_suffix("++") { Some(head) => (head.to_owned(), "+".to_owned()), None => {
        let t = spec.trim();
        let (m, k) = t.rsplit_once('+').unwrap_or(("", t));
        (m.to_owned(), k.to_owned())
    } };
    let words: Vec<String> = mods.split('+').filter(|m| !m.is_empty()).map(|m| m.trim().to_ascii_lowercase()).collect();
    if words.iter().any(|m| !["ctrl", "alt", "shift"].contains(&m.as_str())) { return None; }
    let (ctrl, alt, shift) = (words.iter().any(|m| m == "ctrl"), words.iter().any(|m| m == "alt"), words.iter().any(|m| m == "shift"));
    let function = key.len() >= 2 && (key.starts_with('F') || key.starts_with('f')) && key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n));
    let key = if (key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic())) || function {
        key.to_ascii_uppercase()
    } else {
        key
    };
    const NAMES: &[&str] = &["=", "+", "-", ";", "'", "[", "]", "\\", ",", ".", "/", "`", "Delete", "Backspace", "Enter", "PageUp", "PageDown", "Home", "End",
        "Up", "Down", "Left", "Right"];
    let known = function || (key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric())) || NAMES.contains(&key.as_str());
    if !known || (!ctrl && !alt && !function) { return None; }
    Some([ctrl.then_some("Ctrl"), alt.then_some("Alt"), shift.then_some("Shift"), Some(key.as_str())].into_iter().flatten().collect::<Vec<_>>().join("+"))
}

/// A spec as the menus show it on this system ("Ctrl+Shift+D"; on a Mac the symbols, Ctrl being Command).
fn label(spec: &str) -> String {
    if !cfg!(target_os = "macos") { return spec.to_owned(); }
    let (ctrl, alt, shift, _) = parse(spec);
    let key = if spec.ends_with("++") { "+" } else { spec.rsplit('+').next().unwrap_or("") };
    format!("{}{}{}{}", if alt { "⌥" } else { "" }, if shift { "⇧" } else { "" }, if ctrl { "⌘" } else { "" }, if key.len() == 1 { key.to_uppercase() } else { key.to_owned() })
}

/// What the app settings say the user's shortcuts are (a command id → its keys).
pub fn defaults() -> Map<String, Value> {
    let saved = crate::user_settings::run(&json!({})).map(|v| v["settings"]["shortcuts"].clone()).unwrap_or(Value::Null);
    let mut values = Map::new();
    values.insert("shortcuts".into(), if saved.is_object() { saved } else { json!({}) });
    values
}

/// The keys every command has with the user's shortcuts (the screen's `applyShortcuts`): the user's own; else the defaults, of which a
/// contextual one (`fallThrough`) gives up a key the user has given another command.
fn effective(commands: &[Value], custom: &Map<String, Value>) -> Vec<(String, Vec<String>, bool)> {
    let known = |id: &str| commands.iter().any(|c| c["id"] == id);
    let assigned: Vec<String> = custom.iter().filter(|(id, _)| known(id)).flat_map(|(_, keys)| keys.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned)).collect();
    commands.iter().map(|c| {
        let id = c["id"].as_str().unwrap_or("").to_owned();
        let defaults: Vec<String> = c["keys"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
        let yields = c["fallThrough"] == true && !custom.contains_key(&id);
        let keys = match custom.get(&id) {
            Some(own) => own.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect(),
            None if yields => defaults.into_iter().filter(|k| !assigned.iter().any(|a| same(k, a))).collect(),
            None => defaults,
        };
        (id, keys, yields)
    }).collect()
}

/// After `changed` was set (or on opening): Assign, Remove or Reset done on the command shown, the key cleared when another command is
/// chosen, then what the dialog shows worked out again (the command's keys, the conflict of the key, the list of commands).
pub fn linked(values: &mut Value, choices: &mut Value, changed: &str) -> Result<(), Refusal> {
    let commands = values["commands"].as_array().cloned().unwrap_or_default();
    let mut custom = values["shortcuts"].as_object().cloned().unwrap_or_default();
    let selected = values["shortcutCommand"].as_str().unwrap_or("").to_owned();
    if changed == "shortcutCommand" { values["shortcutKey"] = json!(""); }
    let conflict = |custom: &Map<String, Value>, key: &str| -> Option<String> {
        if key.is_empty() { return None; }
        effective(&commands, custom).into_iter().find(|(id, keys, yields)| *id != selected && !*yields && keys.iter().any(|k| same(key, k)))
            .map(|(id, _, _)| commands.iter().find(|c| c["id"] == id.as_str()).and_then(|c| c["label"].as_str()).unwrap_or(&id).to_owned())
    };
    if changed == "listAction" {
        let action = values["listAction"].as_str().unwrap_or("").to_owned();
        values["listAction"] = json!("");
        match action.as_str() {
            "assign" => {
                let key = values["shortcutKey"].as_str().unwrap_or("").to_owned();
                if key.is_empty() { return Err(refuse("shortcutKey", "required", "press the shortcut to assign first".into())); }
                if let Some(other) = conflict(&custom, &key) { return Err(refuse("shortcutKey", "conflict", format!("{key} is used by {other}"))); }
                let mut keys: Vec<Value> = effective(&commands, &custom).into_iter().find(|(id, _, _)| *id == selected).map(|(_, k, _)| k).unwrap_or_default()
                    .into_iter().filter(|k| !same(k, &key)).map(Value::from).collect();
                keys.push(json!(key));
                custom.insert(selected.clone(), Value::Array(keys));
                values["shortcutKey"] = json!("");
            }
            "remove" => { custom.insert(selected.clone(), json!([])); }
            "reset" => { custom.remove(&selected); }
            _ => {}
        }
        values["shortcuts"] = Value::Object(custom.clone());
    }
    // what the dialog shows: the command's keys ((none) when it has none), the conflict of the key pressed, the commands listed
    let all = effective(&commands, &custom);
    let keys = all.iter().find(|(id, _, _)| *id == selected).map(|(_, k, _)| k.clone()).unwrap_or_default();
    values["currentKeys"] = json!(keys.iter().map(|k| label(k)).collect::<Vec<_>>().join(", "));
    values["hasKeys"] = json!(!keys.is_empty());
    values["shortcutCommandLabel"] = json!(commands.iter().find(|c| c["id"] == selected.as_str()).and_then(|c| c["label"].as_str()).unwrap_or(&selected));
    let other = conflict(&custom, values["shortcutKey"].as_str().unwrap_or(""));
    values["hasConflict"] = json!(other.is_some());
    values["conflictWith"] = json!(other.unwrap_or_default());
    choices["shortcutCommands"] = Value::Array(rows(&commands, &all, values["shortcutSearch"].as_str().unwrap_or(""), &selected));
    Ok(())
}

/// The commands the list shows: the `listed` ones whose name or id has the search in it (any case), each with its keys; the one chosen
/// is always there (a choice holds a command of the list).
fn rows(commands: &[Value], all: &[(String, Vec<String>, bool)], search: &str, selected: &str) -> Vec<Value> {
    let search = search.to_lowercase();
    let mut out: Vec<Value> = commands.iter().zip(all).filter(|(c, (id, _, _))| (c["listed"] == true
        && format!("{} {}", c["label"].as_str().unwrap_or(""), id).to_lowercase().contains(&search)) || id == selected)
        .map(|(c, (id, keys, _))| {
            let name = c["label"].as_str().unwrap_or(id);
            let shown = if keys.is_empty() { name.to_owned() } else { format!("{name} · {}", keys.iter().map(|k| label(k)).collect::<Vec<_>>().join(", ")) };
            json!({"value": id, "label": shown})
        }).collect();
    if !selected.is_empty() && !out.iter().any(|r| r["value"] == selected) { out.push(json!({"value": selected, "label": selected})); }
    out
}

/// The list the dialog opens with (before `linked` works it out with the user's shortcuts): the commands handed in, and the one chosen.
pub fn opening_rows(requested: &Value, selected: &str) -> Vec<Value> {
    let commands = requested["commands"].as_array().cloned().unwrap_or_default();
    let all = effective(&commands, &Map::new());
    rows(&commands, &all, "", selected)
}
