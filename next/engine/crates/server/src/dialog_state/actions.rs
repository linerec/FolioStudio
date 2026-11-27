//! Validation on submit and expansion of dialog actions into engine requests.

use super::{
    Refusal, checked, current_settings, flattener, graphic_layers, name_of, packagedlg, pdfdlg,
    preferences, prepared, printdlg, profdlg, refuse, rows_short, tocdlg, when_holds,
};
use super::fields::{hex_rgb, page_range_ok, typed_number};
use serde_json::{Map, Value, json};

/// A template with the dialog's values in it: `"$name"` is a field, `"$ids"` the item ids, `"$ids0"` the first one, and
/// `{"$if":"name"|["a","b"],"then":T,"else":E}` T when a field (or any of them) is set (not empty, false or 0) and E when not (without E: the item is left out of the list),
/// `{"$":"name","rsplit":":","part":1,"number":true}` a part of a text field cut at its last separator (a "story:at" choice),
/// `{"$":"name","numbers":true}` a typed list of numbers ("12 6") as the numbers, `{"$":"name","rgb":true}` a `#rrggbb` field as `[r, g, b]` (0 to 255),
/// `{"$":"name","div":100}` a field divided (a percent as a ratio), `{"$":"name","add":36}` a field plus a number (a size as an edge), `{"$changed":true}` the visible fields that differ from the values
/// the dialog opened with.
pub(super) fn substitute(template: &Value, values: &Value, ids: &Value, changed: &Map<String, Value>) -> Result<Value, Refusal> {
    Ok(match template {
        Value::Object(o) if o.get("$changed").is_some() => Value::Object(changed.clone()),
        Value::Object(o) if o.contains_key("$if") => {
            let names: Vec<&str> = match &o["$if"] { Value::String(n) => vec![n.as_str()], Value::Array(a) => a.iter().filter_map(Value::as_str).collect(), _ => vec![] };
            let on = names.iter().any(|n| match &values[*n] { Value::Null | Value::Bool(false) => false, Value::String(t) => !t.is_empty(), Value::Number(x) => x.as_f64() != Some(0.), _ => true });
            match o.get(if on { "then" } else { "else" }) { Some(branch) => substitute(branch, values, ids, changed)?, None => Value::Null }
        }
        Value::Object(o) if o.contains_key("$") && o.contains_key("rsplit") => {
            let text = values[o["$"].as_str().unwrap_or("")].as_str().unwrap_or("");
            let (head, tail) = text.rsplit_once(o["rsplit"].as_str().unwrap_or(":")).unwrap_or((text, ""));
            let part = if o.get("part").and_then(Value::as_u64) == Some(1) { tail } else { head };
            if o.get("number") == Some(&json!(true)) { json!(part.parse::<u64>().unwrap_or(0)) } else { json!(part) }
        }
        Value::Object(o) if o.contains_key("$") && o.get("numbers") == Some(&json!(true)) => {
            let text = values[o["$"].as_str().unwrap_or("")].as_str().unwrap_or("");
            json!(text.split([' ', ',']).filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect::<Vec<_>>())
        }
        Value::Object(o) if o.contains_key("$") && o.get("rgb") == Some(&json!(true)) => {
            json!(hex_rgb(values[o["$"].as_str().unwrap_or("")].as_str().unwrap_or("")).unwrap_or([0., 0., 0.]))
        }
        // one cell (`pick`) of each row that holds the `where` values, in order: the inks ticked in a mixed ink and their percentages
        Value::Object(o) if o.contains_key("$rows") => {
            let rows = values[o["$rows"].as_str().unwrap_or("")].as_array().cloned().unwrap_or_default();
            let (when, pick) = (o.get("where").cloned().unwrap_or(Value::Null), o["pick"].as_str().unwrap_or(""));
            Value::Array(rows.iter().filter(|r| when_holds(&when, r)).map(|r| r[pick].clone()).collect())
        }
        // a field's value as another word (`map`: the colour settings' RELATIVE_COLORIMETRIC as the command's RelativeColorimetric)
        Value::Object(o) if o.contains_key("$") && o.get("map").is_some_and(Value::is_object) => {
            let key = o["$"].as_str().unwrap_or("");
            let word = values[key].as_str().unwrap_or("");
            o["map"].get(word).cloned().ok_or_else(|| format!("dialog template has no word for {key} {word:?}"))?
        }
        // a number field as the text an IDML attribute holds (12, 12.5)
        Value::Object(o) if o.contains_key("$") && o.get("text") == Some(&json!(true)) => {
            let key = o["$"].as_str().unwrap_or("");
            match &values[key] { Value::Number(n) => json!(n.to_string()), Value::String(t) => json!(t), _ => return Err(format!("dialog template needs a number for {key}").into()) }
        }
        Value::String(s) if s == "$ids" => ids.clone(),
        Value::String(s) if s == "$ids0" => ids[0].clone(),
        Value::String(s) if s.starts_with('$') => values.get(&s[1..]).cloned().ok_or_else(|| format!("dialog template names unknown field {s}"))?,
        // a folder and a name in it, as one path of this system (the name without the spaces around it, as the screen took it)
        Value::Object(o) if o.contains_key("$path") => {
            let parts = o["$path"].as_array().ok_or("dialog template $path takes field names")?;
            let mut path = std::path::PathBuf::new();
            for part in parts {
                let key = part.as_str().ok_or("dialog template $path takes field names")?;
                path.push(values[key].as_str().ok_or_else(|| format!("dialog template needs text for {key}"))?.trim());
            }
            json!(path.to_string_lossy())
        }
        Value::Object(o) if o.contains_key("$trim") => {
            let key = o["$trim"].as_str().unwrap_or("");
            json!(values[key].as_str().ok_or_else(|| format!("dialog template needs text for {key}"))?.trim())
        }
        // a value inside a field that holds an answer (`{"$at":["sent","job","id"]}`: the job a print made)
        Value::Object(o) if o.contains_key("$at") => {
            let path = o["$at"].as_array().ok_or("dialog template $at takes a field and the keys in it")?;
            // (a number of a list: `["_result","0","result"]`, the first of the answers)
            path.iter().filter_map(Value::as_str).fold(values, |v, key| match (v, key.parse::<usize>()) { (Value::Array(a), Ok(i)) => a.get(i).unwrap_or(&Value::Null), _ => &v[key] }).clone()
        }
        Value::Object(o) if o.contains_key("$") => {
            let key = o["$"].as_str().unwrap_or("");
            // (an option of numbers is held as its text: a booklet's signature size "16")
            let n = values[key].as_f64().or_else(|| values[key].as_str().and_then(|t| t.trim().parse::<f64>().ok()))
                .ok_or_else(|| format!("dialog template needs a number for {key}"))?;
            let n = n / o.get("div").and_then(Value::as_f64).filter(|d| *d != 0.).unwrap_or(1.) + o.get("add").and_then(Value::as_f64).unwrap_or(0.);
            // `int`: a whole number, as a count the request takes
            if o.get("int") == Some(&json!(true)) { json!(n.round() as i64) } else { json!(n) }
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| substitute(v, values, ids, changed)).collect::<Result<_, _>>()?),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| Ok((k.clone(), substitute(v, values, ids, changed)?))).collect::<Result<Map<_, _>, Refusal>>()?),
        other => other.clone(),
    })
}

/// `{"$each":{"query":"strokeStylesFile","args":{…},"in":"styles","where":{"kind":{"not":"StrokeStyle"}},"as":T}}`: one command for each item of the
/// list the query answers (T with `{"$item":"*","set":{…}}` the item with some of its fields set, `{"$item":"name"}` one of its fields).
fn each(spec: &Value, values: &Value, ids: &Value, session: Option<&crate::Session>) -> Result<Vec<Value>, Refusal> {
    let s = session.ok_or("this button needs the open document")?;
    let args = substitute(&spec["args"], values, ids, &Map::new())?;
    let answer = match spec["query"].as_str() {
        Some("strokeStylesFile") => crate::stroke_files::read(&args, Some(s))?,
        Some("tocStylesFile") => tocdlg::styles_file(&args, s)?,
        other => return Err(format!("unknown dialog query {other:?}").into()),
    };
    // (without `in` the answer is the one item: one command made from it)
    if spec.get("in").is_none() { return Ok(vec![item_of(&spec["as"], &answer)]); }
    fn item_of(template: &Value, item: &Value) -> Value {
        match template {
            Value::Object(o) if o.contains_key("$item") => {
                let mut out = if o["$item"] == "*" { item.clone() } else { item[o["$item"].as_str().unwrap_or("")].clone() };
                for (k, v) in o.get("set").and_then(Value::as_object).into_iter().flatten() { out[k] = v.clone(); }
                out
            }
            Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), item_of(v, item))).collect()),
            Value::Array(a) => Value::Array(a.iter().map(|v| item_of(v, item)).collect()),
            other => other.clone(),
        }
    }
    let kept = |item: &Value| spec["where"].as_object().is_none_or(|w| w.iter().all(|(k, v)| match v.get("not") { Some(not) => &item[k.as_str()] != not, None => item[k.as_str()] == *v }));
    Ok(answer[spec["in"].as_str().unwrap_or("")].as_array().into_iter().flatten().filter(|item| kept(item)).map(|item| item_of(&spec["as"], item)).collect())
}

/// Whether every pair of `noneWhenSame` holds one value in both fields (a dialog that then has nothing to do).
pub(super) fn all_same(def: &Value, values: &Value) -> bool {
    def["noneWhenSame"].as_array().is_some_and(|pairs| !pairs.is_empty()
        && pairs.iter().all(|p| values[p[0].as_str().unwrap_or("")] == values[p[1].as_str().unwrap_or("")]))
}

/// A list of numbers typed with spaces or commas between them (empty is not one).
pub(super) fn numbers_ok(text: &str) -> bool {
    let parts: Vec<&str> = text.split([' ', ',']).filter(|t| !t.is_empty()).collect();
    !parts.is_empty() && parts.iter().all(|t| t.parse::<f64>().is_ok_and(f64::is_finite))
}

/// A `$each` command for one item: `"$id"` replaced by it.
fn bound(template: &Value, id: &Value) -> Value {
    match template {
        Value::String(s) if s == "$id" => id.clone(),
        Value::Array(a) => Value::Array(a.iter().map(|v| bound(v, id)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), bound(v, id))).collect()),
        other => other.clone(),
    }
}

/// The edit commands of a button, from the values the dialog holds.
pub(super) fn commands(def: &Value, action: &str, values: &Value, ids: &Value, initial: &Value, session: Option<&crate::Session>) -> Result<Vec<Value>, Refusal> {
    if let Some(actions) = def.get("actions") {
        let list = actions.get(action).and_then(Value::as_array).ok_or_else(|| format!("this dialog has no action {action}"))?;
        // (a `local` field only steers the dialog; a field with `sendWhen` is sent only while those fields hold those values)
        let sent = |f: &Value| f["local"] != true && f["sendWhen"].as_object().is_none_or(|when| when.iter().all(|(k, v)| &values[k] == v));
        let mut changed: Map<String, Value> = def["fields"].as_array().into_iter().flatten().filter(|f| f["hidden"] != true && sent(f))
            .filter(|f| initial.get(name_of(f)).is_some_and(|before| before != &values[name_of(f)]))
            .map(|f| (name_of(f).to_owned(), values[name_of(f)].clone())).collect();
        // fields sent together (`packs`: a text frame's four insets as one `insets`): all of them when one changed
        for (pack, members) in def["packs"].as_object().into_iter().flatten() {
            let members: Vec<&str> = members.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if members.iter().any(|m| changed.remove(*m).is_some()) {
                changed.insert(pack.clone(), Value::Array(members.iter().map(|m| values[*m].clone()).collect()));
            }
        }
        // a dialog that sends only what changed has nothing to send when nothing did
        if def["skipEmpty"] == true && changed.is_empty() { return Ok(vec![]); }
        // nor one whose pairs of fields are all alike (`noneWhenSame`: Convert to Profile to the profiles the document has)
        if all_same(def, values) { return Ok(vec![]); }
        let mut built = Vec::new();
        for t in list {
            match t.get("$each") {
                // (Object Layer Options: the commands the comp, the layers and the policy make, `graphic_layer_edits`)
                _ if t.get("$graphicLayerEdits").is_some() => built.extend(graphic_layers::graphic_layer_edits(values, initial, ids)),
                // a query's list (`{"$each":{"query":…}}`): one command for each item it answers
                Some(spec) if spec.get("query").is_some() => built.extend(each(spec, values, ids, session)?),
                // a command marked `$each` (`"$ids"`) is sent once per item, `"$id"` being that item
                Some(over) => for id in substitute(over, values, ids, &changed)?.as_array().ok_or("dialog $each needs a list")? {
                    let mut one = t.clone();
                    one.as_object_mut().ok_or("invalid dialog command")?.remove("$each");
                    built.push(substitute(&bound(&one, id), values, ids, &changed)?);
                },
                // the commands that take what the ones before made (`$then`): built once those are in the document (`then_request`), with
                // the `find` (its `where` from the values now) naming what they take
                None if t.get("$then").is_some() => built.push(json!({"$then": t["$then"], "find": substitute(&t["find"], values, ids, &changed)?, "values": values, "ids": ids})),
                None => built.push(substitute(t, values, ids, &changed)?),
            }
        }
        return Ok(built.into_iter().filter(|command| !command.is_null()).collect());
    }
    if action != "ok" { return Err(format!("this dialog has no action {action}").into()); }
    let mut edit = values.clone();
    edit["op"] = def["command"].clone();
    edit["ids"] = ids.clone();
    Ok(vec![edit])
}

/// What `applyDialog` needs from the dialog: the validated values (typed-but-uncommitted numbers read in), then the commands.
pub(super) fn apply(def: &Value, state: &Value, action: &str) -> Result<(Vec<Value>, Value), Refusal> { apply_in(def, state, action, None) }

pub(super) fn apply_in(def: &Value, state: &Value, action: &str, session: Option<&crate::Session>) -> Result<(Vec<Value>, Value), Refusal> {
    let mut values = state["values"].clone();
    // an action the form's own required boxes and formats do not hold up (Delete, Load…: they work on another item or a file)
    let needs_form = !def["unchecked"].as_array().into_iter().flatten().any(|a| a == action);
    for f in def["fields"].as_array().ok_or("invalid fields")? {
        if f["type"] == "writeOnly" { continue; }
        let key = name_of(f);
        if let Some(text) = state["drafts"][key].as_str() {
            let unit = f["unit"].as_str().unwrap_or("");
            if unit.starts_with("length") { return Err(refuse(key, "uncommitted", format!("the typed length for {key} is not entered yet"))); }
            // as the screen's number boxes read a typed number: a decimal comma, and a trailing % or ° allowed
            let n: f64 = typed_number(text).ok_or_else(|| refuse(key, "notNumber", format!("invalid input for {key}")))?;
            values[key] = checked(f, &json!(n), &state["choices"])?;
        }
        values[key] = checked(f, &values[key], &state["choices"])?;
        // a row's box it cannot go without (a TOC entry's paragraph style), where the row shows it
        if f["type"] == "array" {
            for (i, row) in values[key].as_array_mut().into_iter().flatten().enumerate() {
                for sub in f["fields"].as_array().into_iter().flatten() {
                    let cell = name_of(sub);
                    if needs_form && sub["required"] == true && when_holds(&sub["when"], row) && row[cell].as_str().is_some_and(str::is_empty) {
                        return Err(refuse(&format!("{key}/{i}/{cell}"), "required", format!("{key}/{i}/{cell} is required")));
                    }
                    // (a whole-number cell reaches the command as an integer, as a top field does)
                    if sub["integer"] == true && let Some(n) = row[cell].as_f64() { row[cell] = json!(n.round() as i64); }
                }
            }
        }
        if let Some(keep) = f["fixedExcept"].as_array() {
            if !keep.contains(&values[key]) && values[key] != state["initial"][key] { return Err(refuse(key, "notEditable", format!("{key} takes only {} or the value it opened with", f["fixedExcept"]))); }
        }
        if f["trim"] == true { if let Some(t) = values[key].as_str().map(|t| t.trim().to_owned()) { values[key] = json!(t); } }
        // a name the document already has for another item (the item the dialog works on keeps its own)
        if let Some(source) = f["unique"].as_str() {
            let taken = state["choices"][source].as_array().into_iter().flatten().any(|c| c["label"] == values[key] && c["value"] != state["ids"][0]);
            if taken { return Err(refuse(key, "notUnique", format!("{key} is already taken"))); }
        }
        if needs_form && f["format"] == "pageRange" && !page_range_ok(values[key].as_str().unwrap_or("")) {
            return Err(refuse(key, "notPageRange", format!("{key} is not a page range like 1,3-5")));
        }
        let shown = when_holds(&f["when"], &values);
        if needs_form && f["format"] == "numbers" && !numbers_ok(values[key].as_str().unwrap_or("")) {
            return Err(refuse(key, "notNumbers", format!("{key} is a list of numbers separated by spaces")));
        }
        // (a path's or a trimmed text's blank is empty, as the screen holds OK back for it and sends the name trimmed; another text is taken as typed)
        let empty = |v: &str| if f.get("path").is_some() || f["trim"] == true { v.trim().is_empty() } else { v.is_empty() };
        if needs_form && (f["type"] == "text" || f["type"] == "choice") && f["required"] == true && shown && values[key].as_str().is_none_or(empty) {
            return Err(refuse(key, "required", format!("{key} is required")));
        }
        // Integer fields must reach typed edit commands as JSON integers (a typed "4" too).
        if f["integer"] == true { values[key] = json!(values[key].as_f64().ok_or("invalid integer")?.round() as i64); }
    }
    // a button that needs a text the dialog may go without (New Document's preset name for Save Preset)
    for key in def["needs"][action].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if values[key].as_str().is_none_or(|v| v.trim().is_empty()) { return Err(refuse(key, "required", format!("{key} is required"))); }
    }
    if needs_form {
        // OK is off while the dialog says so (`okDisabledWhen`: a text path Folio does not edit), and two fields that must differ (`distinct`) may not be equal
        if let Some(when) = def["okDisabledWhen"].as_object().filter(|_| when_holds(&def["okDisabledWhen"], &values)) {
            return Err(refuse(when.keys().next().map_or("", String::as_str), "notApplicable", "this dialog cannot apply to what it opened on".into()));
        }
        // the rows a list must have (`rowsCheck`): at least `min` that hold `where`, one of them holding `some` (two inks or more, one a spot ink)
        if let Some(key) = rows_short(def, &values) { return Err(refuse(key, "tooFew", format!("{key} does not have the rows it needs"))); }
        for pair in def["distinct"].as_array().into_iter().flatten() {
            let (a, b) = (pair[0].as_str().unwrap_or(""), pair[1].as_str().unwrap_or(""));
            if values[a].as_f64().is_some() && values[a].as_f64() == values[b].as_f64() { return Err(refuse(b, "same", format!("{b} must differ from {a}"))); }
        }
    }
    // the user's flattener presets each named, no two alike (the names saved trimmed)
    if def["link"] == "flattenerPresets" { flattener::check(&mut values)?; }
    prepared(def, &mut values);
    let list = if def["build"] == "preferences" {
        if action != "ok" { return Err(format!("this dialog has no action {action}").into()); }
        // a dialog of either scope sends the document its edits; without a document it sends the app requests
        if state["scope"] == "app" { preferences::app_requests(def, &values, &current_settings()?)? } else { preferences::edits(def, &values, &state["initial"])? }
    } else if def["build"] == "pdfPresets" {
        // (the PDF presets' requests and File ▸ Export PDF's: the fields as one preset, `pdfdlg::requests`)
        pdfdlg::requests(action, &values, &state["choices"], session)?
    } else if def["build"] == "printSettings" {
        // (File ▸ Print's requests and the print presets manager's: the fields as one print preset, `printdlg::requests`)
        printdlg::requests(action, &values)?
    } else if def["build"] == "package" {
        // (File ▸ Package: the package request the screen sent, then the settings it remembered; Cancel stops it)
        packagedlg::requests(action, &values)?
    } else if def["build"] == "preflightProfiles" {
        if action != "ok" { return Err(format!("this dialog has no action {action}").into()); }
        profdlg::commands(&values)?
    } else {
        commands(def, action, &values, &state["ids"], &state["initial"], session)?
    };
    Ok((list, values))
}
