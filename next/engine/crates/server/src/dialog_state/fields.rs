//! Input validation, typed values and field paths shared by dialogs.

use super::{Refusal, refuse, newdoc, shortcuts};
use serde_json::{Map, Value, json};

pub(super) fn number(values: &Value, key: &str) -> f64 { values[key].as_f64().unwrap_or(0.) }

pub(super) fn field(def: &Value, name: &str) -> Result<Value, Refusal> {
    def["fields"].as_array().and_then(|fields| fields.iter().find(|f| f["name"] == name)).cloned()
        .ok_or_else(|| refuse(name, "unknownField", format!("unknown dialog field: {name}")))
}
pub(super) fn name_of(f: &Value) -> &str { f["name"].as_str().unwrap_or("") }

/// A `when` / `disabledWhen` condition against the values: every named field equals its value (or is one of a list of values).
pub(super) fn when_holds(when: &Value, values: &Value) -> bool {
    when.as_object().is_none_or(|w| w.iter().all(|(k, v)| match v { Value::Array(any) => any.contains(&values[k.as_str()]), v => values[k.as_str()] == *v }))
}

/// The ids a `choice` field may take: the empty choice when the field names one (`none`), then the list the dialog opened with.
pub(super) fn allowed<'c>(f: &'c Value, choices: &'c Value) -> Vec<&'c str> {
    let none = if f.get("none").is_some() { Some(f["noneValue"].as_str().unwrap_or("")) } else { None };
    // a field that names a `custom` choice holds it while the values are none of the list's (New Document's [Custom])
    let custom = if f.get("custom").is_some() { Some(newdoc::CUSTOM) } else { None };
    none.into_iter().chain(custom).chain(choices[f["source"].as_str().unwrap_or("")].as_array().into_iter().flatten().filter_map(|c| c["value"].as_str())).collect()
}

/// One row of an `array` field: its sub-fields checked, the missing ones at their defaults (`at` names the row in a refusal).
pub(super) fn checked_row(f: &Value, row: &Value, choices: &Value, at: &str) -> Result<Value, Refusal> {
    let row = row.as_object().ok_or_else(|| refuse(at, "notRow", format!("{at} must be an object of the row's fields")))?;
    let subs = f["fields"].as_array().ok_or("invalid array field")?;
    if let Some(key) = row.keys().find(|k| !subs.iter().any(|s| s["name"] == k.as_str())) {
        return Err(refuse(&format!("{at}/{key}"), "unknownField", format!("unknown field {at}/{key}")));
    }
    let mut out = Map::new();
    for sub in subs {
        let key = name_of(sub);
        let value = row.get(key).cloned().unwrap_or_else(|| sub["default"].clone());
        out.insert(key.into(), checked(sub, &value, choices).map_err(|mut r| { r.field = Some(format!("{at}/{key}")); r })?);
    }
    Ok(Value::Object(out))
}

/// The value if the field takes it; otherwise why not. `choices` are the lists the dialog opened with (for `choice` fields).
pub(super) fn checked(f: &Value, value: &Value, choices: &Value) -> Result<Value, Refusal> {
    let name = name_of(f);
    match f["type"].as_str().unwrap_or("number") {
        "writeOnly" if value.is_null() => Ok(Value::Null),
        "writeOnly" => Err(refuse(name, "writeOnly", "use secure credential storage".into())),
        "array" => {
            let rows = value.as_array().ok_or_else(|| refuse(name, "notArray", format!("{name} must be a list of rows")))?;
            let (min, max) = (f["minRows"].as_u64().unwrap_or(0) as usize, f["maxRows"].as_u64().unwrap_or(1000) as usize);
            if rows.len() < min || rows.len() > max { return Err(refuse(name, "rowCount", format!("{name} takes {min} to {max} rows"))); }
            let rows: Vec<Value> = rows.iter().enumerate().map(|(i, row)| checked_row(f, row, choices, &format!("{name}/{i}"))).collect::<Result<_, _>>()?;
            // the sub-fields a row is known by (`unique`, a word of a language) are each row's own
            if let Some(keys) = f["unique"].as_array() {
                let mut seen = std::collections::HashSet::new();
                for (i, row) in rows.iter().enumerate() {
                    let id: Vec<&Value> = keys.iter().map(|k| &row[k.as_str().unwrap_or("")]).collect();
                    if !seen.insert(serde_json::to_string(&id).unwrap_or_default()) {
                        let at = keys.last().and_then(Value::as_str).unwrap_or("");
                        return Err(refuse(&format!("{name}/{i}/{at}"), "duplicate", format!("{name}/{i} is in the list already")));
                    }
                }
            }
            Ok(Value::Array(rows))
        }
        "choice" => match value.as_str() {
            Some(v) if allowed(f, choices).contains(&v) => Ok(value.clone()),
            _ => Err(refuse(name, "notOption", format!("{name} must be one of the ids the dialog offers"))),
        },
        // a value the dialog only carries (a hidden list the document gave, sent back whole)
        "json" => if value.is_null() { Err(refuse(name, "notJson", format!("{name} must be a value"))) } else { Ok(value.clone()) },
        "color" => match value.as_str() {
            Some(v) if hex_rgb(v).is_some() => Ok(value.clone()),
            _ => Err(refuse(name, "notColor", format!("{name} must be a color like #1a2b3c"))),
        },
        "boolean" => if value.is_boolean() { Ok(value.clone()) } else { Err(refuse(name, "notBoolean", format!("{name} must be true or false"))) },
        // a key pressed (Keyboard Shortcuts): none, or Ctrl/Alt with a key or a function key, taken normalized (`shift+ctrl+d` → `Ctrl+Shift+D`)
        "key" => match value.as_str() {
            Some("") => Ok(value.clone()),
            Some(v) => shortcuts::normalized(v).map(Value::from).ok_or_else(|| refuse(name, "notShortcut", format!("{v} is not a shortcut: Ctrl or Alt with a key, or a function key"))),
            None => Err(refuse(name, "notShortcut", format!("{name} must be a key like Ctrl+Shift+D"))),
        },
        "enum" => match value.as_str() {
            Some(v) if f["options"].as_array().is_some_and(|o| o.iter().any(|x| x == v || x["value"] == v)) => Ok(value.clone()),
            _ => Err(refuse(name, "notOption", format!("{name} must be one of {}", f["options"]))),
        },
        "text" => match value.as_str() {
            Some(v) if v.strip_prefix(f["hidePrefix"].as_str().unwrap_or("\u{0}")).unwrap_or(v).chars().count() > f["maxLength"].as_u64().unwrap_or(256) as usize => Err(refuse(name, "tooLong", format!("{name} is too long"))),
            Some(v) => checked_path(f, v).map(|()| value.clone()),
            None => Err(refuse(name, "notText", format!("{name} must be text"))),
        },
        _ => {
            let Some(n) = value.as_f64().filter(|n| n.is_finite()) else { return Err(refuse(name, "notNumber", format!("{name} must be a number"))); };
            if f["integer"] == true && n.fract() != 0. { return Err(refuse(name, "notInteger", format!("{name} must be a whole number"))); }
            let (min, max) = (f["min"].as_f64().unwrap_or(f64::MIN), f["max"].as_f64().unwrap_or(f64::MAX));
            if n < min || n > max {
                let message = f["rangeMessage"].as_str().map(str::to_owned).unwrap_or_else(|| format!("{name} must be between {min} and {max}"));
                return Err(refuse(name, "outOfRange", message));
            }
            Ok(value.clone())
        }
    }
}

/// What a path field takes (empty is left to `required`): `folder`, a folder that exists given whole (the place a new project goes,
/// which no project holds yet); `name`, one file or folder name usable on every system (no separators, `..` or reserved names).
fn checked_path(f: &Value, text: &str) -> Result<(), Refusal> {
    let name = name_of(f);
    if text.trim().is_empty() { return Ok(()); }
    match f["path"].as_str() {
        None => Ok(()),
        Some("folder") => {
            let path = std::path::Path::new(text);
            if !path.is_absolute() { return Err(refuse(name, "notAbsolute", format!("{name} must be a whole folder path"))); }
            if !path.is_dir() { return Err(refuse(name, "notFolder", format!("{name} is not a folder: {text}"))); }
            Ok(())
        }
        Some("name") => folio_project::paths::check_name(text.trim()).map_err(|issue| refuse(name, "badName", format!("{name} cannot be used as a name: {issue}"))),
        // `saveFile`: a file to write, given whole (what the system's save window answers), with the `extension` asked for, in a folder that exists
        Some("saveFile") => {
            let path = std::path::Path::new(text);
            if !path.is_absolute() { return Err(refuse(name, "notAbsolute", format!("{name} must be a whole file path"))); }
            if let Some(ext) = f["extension"].as_str() {
                if !path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(ext)) { return Err(refuse(name, "badExtension", format!("{name} must end in .{ext}"))); }
            }
            if path.is_dir() { return Err(refuse(name, "notFile", format!("{name} is a folder: {text}"))); }
            if !path.parent().is_some_and(std::path::Path::is_dir) { return Err(refuse(name, "notFolder", format!("the folder of {name} does not exist: {text}"))); }
            Ok(())
        }
        // `openFile`: a file to read, given whole (what the system's open window answers), with the `extension` asked for, that exists
        Some("openFile") => {
            let path = std::path::Path::new(text);
            if !path.is_absolute() { return Err(refuse(name, "notAbsolute", format!("{name} must be a whole file path"))); }
            if let Some(ext) = f["extension"].as_str() {
                if !path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(ext)) { return Err(refuse(name, "badExtension", format!("{name} must end in .{ext}"))); }
            }
            if !path.is_file() { return Err(refuse(name, "notFile", format!("{name} is not a file: {text}"))); }
            Ok(())
        }
        Some(other) => Err(format!("unknown path field kind {other}").into()),
    }
}

/// A field path `name`, `name/row` or `name/row/sub` (rows of `array` fields), checked against the dialog.
fn parsed_path<'p>(def: &Value, path: &'p str, values: &Value) -> Result<(Value, Option<usize>, Option<&'p str>), Refusal> {
    let mut parts = path.split('/');
    let top = field(def, parts.next().unwrap_or(""))?;
    let Some(row) = parts.next() else { return Ok((top, None, None)) };
    if top["type"] != "array" { return Err(refuse(path, "unknownField", format!("{} has no rows", name_of(&top)))); }
    let len = values[name_of(&top)].as_array().map_or(0, Vec::len);
    let index = row.parse::<usize>().ok().filter(|i| *i < len).ok_or_else(|| refuse(path, "noRow", format!("{} has no row {row}", name_of(&top))))?;
    let sub = parts.next();
    if parts.next().is_some() { return Err(refuse(path, "unknownField", format!("unknown dialog field: {path}"))); }
    Ok((top, Some(index), sub))
}

/// `setDialogField` on a path: a whole value, one row, one row's sub-field, or a row added (`row: "add"`, at the row named or the end),
/// removed (`"remove"`) or moved (`"move"`, `to`). The field is checked whole afterwards (row count, every row).
pub(super) fn set_path(def: &Value, values: &mut Value, choices: &Value, req: &Value) -> Result<String, Refusal> {
    let path = req["field"].as_str().ok_or("field must be a name")?;
    let (f, row, sub) = parsed_path(def, path, values)?;
    let key = name_of(&f).to_owned();
    if f["type"] == "writeOnly" { return Err(refuse(path, "writeOnly", "use secure credential storage".into())); }
    if f["disabled"] == true { return Err(refuse(path, "notEditable", format!("{key} is shown but not editable"))); }
    let op = req["row"].as_str();
    // the rows of a fixed list (the document's languages, its inks) are what they are: none added, removed or moved (and a whole list
    // or row set is checked below to keep them)
    if f["fixedRows"] == true && op.is_some() { return Err(refuse(path, "fixedRows", format!("{key} has fixed rows; set a row's field"))); }
    if row.is_none() && op.is_none() && f["fixedRows"] != true { values[&key] = checked(&f, &req["value"], choices)?; return Ok(key); }
    if f["type"] != "array" { return Err(refuse(path, "unknownField", format!("{key} has no rows"))); }
    let mut rows = values[&key].as_array().cloned().unwrap_or_default();
    match (op, row, sub) {
        (None, None, None) => rows = req["value"].as_array().cloned().ok_or_else(|| refuse(path, "notArray", format!("{key} must be a list of rows")))?,
        (None, Some(i), Some(sub)) => {
            let def_sub = f["fields"].as_array().and_then(|s| s.iter().find(|x| x["name"] == sub))
                .ok_or_else(|| refuse(path, "unknownField", format!("unknown dialog field: {path}")))?;
            // what a fixed row is (its hidden and read-only cells: the ink it is for) stays; another hidden or greyed cell is not set either
            if f["fixedRows"] == true && (def_sub["hidden"] == true || def_sub["readOnly"] == true) {
                return Err(refuse(path, "fixedRows", format!("the rows of {key} are fixed: only their open cells change")));
            }
            if def_sub["disabled"] == true || def_sub["hidden"] == true { return Err(refuse(path, "notEditable", format!("{path} is shown but not editable"))); }
            rows[i][sub] = checked(def_sub, &req["value"], choices).map_err(|mut r| { r.field = Some(path.into()); r })?;
        }
        (None, Some(i), None) => rows[i] = checked_row(&f, &req["value"], choices, path)?,
        (Some("add"), at, None) => {
            let fresh = if req["value"].is_null() { json!({}) } else { req["value"].clone() };
            let new = checked_row(&f, &fresh, choices, &format!("{key}/{}", at.unwrap_or(rows.len())))?;
            rows.insert(at.unwrap_or(rows.len()), new);
        }
        (Some("remove"), Some(i), None) => { rows.remove(i); }
        (Some("move"), Some(i), None) => {
            let to = req["to"].as_u64().map(|t| t as usize).filter(|t| *t < rows.len()).ok_or_else(|| refuse(path, "noRow", format!("{key} has no row {}", req["to"])))?;
            let moved = rows.remove(i);
            rows.insert(to, moved);
        }
        (Some(other), _, _) => return Err(refuse(path, "badRowOperation", format!("row must be add, remove or move with a row path, not {other}"))),
        _ => return Err(refuse(path, "unknownField", format!("unknown dialog field: {path}"))),
    }
    // rows the dialog fixes (one for each ink of the document): none added, removed or moved, and what a row is (its hidden and read-only cells) stays
    if f["fixedRows"] == true {
        let before = values[&key].as_array().cloned().unwrap_or_default();
        let fixed = |x: &Value| x["hidden"] == true || x["readOnly"] == true;
        let kept = rows.len() == before.len() && rows.iter().zip(&before).all(|(now, was)|
            f["fields"].as_array().into_iter().flatten().filter(|x| fixed(x)).all(|x| now[name_of(x)] == was[name_of(x)]));
        if !kept { return Err(refuse(path, "fixedRows", format!("the rows of {key} are fixed: only their open cells change"))); }
    }
    values[&key] = checked(&f, &Value::Array(rows), choices)?;
    Ok(key)
}

/// A page range as the preflight reads it: empty, or numbers (from 1) and `a-b` ranges (a not above b) separated by commas.
pub(super) fn page_range_ok(text: &str) -> bool {
    text.trim().is_empty() || text.split(',').all(|part| {
        let number = |t: &str| (!t.is_empty() && !t.starts_with('0') && t.chars().all(|c| c.is_ascii_digit())).then(|| t.parse::<u64>().ok()).flatten();
        match part.trim().split_once('-') {
            Some((a, b)) => matches!((number(a.trim()), number(b.trim())), (Some(a), Some(b)) if a <= b),
            None => number(part.trim()).is_some(),
        }
    })
}

/// `#rrggbb` as its red, green and blue (0 to 255), or none.
pub(super) fn hex_rgb(text: &str) -> Option<[f64; 3]> {
    let t = text.strip_prefix('#').filter(|t| t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()))?;
    let part = |at: usize| u8::from_str_radix(&t[at..at + 2], 16).ok().map(f64::from);
    Some([part(0)?, part(2)?, part(4)?])
}

/// A number as typed into a box ("1,5", "50%", "30°"), or none.
pub(super) fn typed_number(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    t.trim_end_matches(['%', '°']).trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

/// A value the document gives as text, as the field's type (`"true"`, `"12"`); a whole number is an integer, as the screen sends it.
pub(super) fn typed(f: &Value, value: &Value) -> Value {
    let Some(text) = value.as_str() else { return value.clone() };
    match f["type"].as_str() {
        Some("boolean") => match text { "true" => json!(true), "false" => json!(false), _ => value.clone() },
        Some("number") => match text.trim().parse::<f64>() {
            Ok(n) if n.is_finite() && n.fract() == 0. && n.abs() < 9e15 => json!(n as i64),
            Ok(n) if n.is_finite() => json!(n),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
}
