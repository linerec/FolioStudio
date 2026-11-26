//! The Glyphs panel's own glyph sets (build-c, 2026-11-21). They belong to the person, not to a document or project, so they live in
//! this computer's user settings (`glyphSets`) as InDesign keeps them in its preferences. A set is
//! `{name, order, glyphs: [{family, style, id, cp, rememberFont}]}`: `order` is where an added glyph goes (`front`: Insert at Front,
//! `end`: Append at End, `unicode`: Unicode Order), `id` the glyph id in the face, `cp` the Unicode value it is inserted with (none: by id),
//! `rememberFont` whether it is inserted in its own face (else in the font of the text, by its Unicode value).
//!
//! `glyphSets {op, …}` (the panel's menu and buttons call it; an LLM can too):
//!   `list`                                   → `{sets}`
//!   `create {name, order?}`                  New Glyph Set
//!   `edit {name, newName?, order?, glyphs?}` Edit Glyph Set (glyphs: the whole list as edited: order, removed ones, Remember Font)
//!   `delete {name}`                          Delete Glyph Set
//!   `add {name, glyph}`                      Add to Glyph Set (a glyph already in the set stays where it is)
//!   `remove {name, index}`                   Delete Glyph from Set
//!   `export {path, names?}`                  the sets (all, or `names`) written to a project-relative `.folioglyphs` file
//!   `import {path}`                          the sets of a project-relative `.folioglyphs` file added (a name in use gets " 2", " 3" …)
//! Every reply is `{sets}` (and `created` / `written` / `imported` where it applies). Names are compared exactly; empty names are refused.
use serde_json::{Map, Value, json};

const ORDERS: &[&str] = &["front", "end", "unicode"];
const EXTENSION: &str = "folioglyphs";

fn sets_of(settings: &Map<String, Value>) -> Vec<Value> {
    settings.get("glyphSets").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn text<'a>(req: &'a Value, key: &str) -> Result<&'a str, String> {
    req[key].as_str().ok_or_else(|| format!("{key} required"))
}

/// A glyph as a set keeps it: a face, a glyph id over 0, an optional Unicode value.
fn glyph(v: &Value) -> Result<Value, String> {
    let family = v["family"].as_str().filter(|s| !s.is_empty()).ok_or("glyph.family required")?;
    let style = v["style"].as_str().ok_or("glyph.style required")?;
    let id = v["id"].as_u64().filter(|&n| n > 0 && n <= 65535).ok_or("glyph.id is a glyph id (1-65535)")?;
    let cp = match &v["cp"] {
        Value::Null => Value::Null,
        n => json!(n.as_u64().filter(|&c| c <= 0x10FFFF).ok_or("glyph.cp is a Unicode value or null")?),
    };
    Ok(json!({"family": family, "style": style, "id": id, "cp": cp, "rememberFont": v["rememberFont"].as_bool().unwrap_or(true)}))
}

fn same(a: &Value, b: &Value) -> bool {
    a["family"] == b["family"] && a["style"] == b["style"] && a["id"] == b["id"]
}

fn order(v: &Value) -> Result<String, String> {
    match v.as_str() {
        None => Ok("front".into()),
        Some(o) if ORDERS.contains(&o) => Ok(o.into()),
        Some(o) => Err(format!("order {o}: front, end or unicode")),
    }
}

/// The glyphs in Unicode order (no Unicode value last, in the order they had).
fn unicode_sorted(glyphs: &mut [Value]) {
    glyphs.sort_by_key(|g| g["cp"].as_u64().unwrap_or(u64::MAX));
}

fn find(sets: &[Value], name: &str) -> Result<usize, String> {
    sets.iter().position(|s| s["name"] == name).ok_or_else(|| format!("no glyph set named {name}"))
}

/// A name not used yet: `name`, else `name 2`, `name 3` …
fn free_name(sets: &[Value], name: &str) -> String {
    if !sets.iter().any(|s| s["name"] == name) { return name.to_owned(); }
    (2..).map(|n| format!("{name} {n}")).find(|c| !sets.iter().any(|s| s["name"] == c.as_str())).unwrap_or_default()
}

fn project_file(session: Option<&crate::Session>, relative: &str) -> Result<std::path::PathBuf, String> {
    let root = &session.and_then(|s| s.folio.as_ref()).ok_or("Open a project document to read or write a glyph set file")?.opened.project.root;
    let path = folio_project::paths::checked_project_path(root, relative)?;
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION)) { return Err(format!("a glyph set file ends in .{EXTENSION}")); }
    Ok(path)
}

/// The sets after one change (`change` gets them and the request); none for `list`.
fn change(req: &Value, session: Option<&crate::Session>, sets: &mut Vec<Value>, reply: &mut Map<String, Value>) -> Result<(), String> {
    match req["op"].as_str().unwrap_or("list") {
        "list" => {}
        "create" => {
            let name = text(req, "name")?.trim();
            if name.is_empty() { return Err("a glyph set needs a name".into()); }
            if sets.iter().any(|s| s["name"] == name) { return Err(format!("a glyph set named {name} exists")); }
            sets.push(json!({"name": name, "order": order(&req["order"])?, "glyphs": []}));
            reply.insert("created".into(), json!(name));
        }
        "edit" => {
            let at = find(sets, text(req, "name")?)?;
            if let Some(new) = req["newName"].as_str() {
                let new = new.trim();
                if new.is_empty() { return Err("a glyph set needs a name".into()); }
                if sets.iter().enumerate().any(|(k, s)| k != at && s["name"] == new) { return Err(format!("a glyph set named {new} exists")); }
                sets[at]["name"] = json!(new);
            }
            if !req["order"].is_null() { sets[at]["order"] = json!(order(&req["order"])?); }
            if let Some(list) = req["glyphs"].as_array() {
                let mut glyphs: Vec<Value> = Vec::new();
                for g in list { let g = glyph(g)?; if !glyphs.iter().any(|o| same(o, &g)) { glyphs.push(g); } }
                sets[at]["glyphs"] = json!(glyphs);
            }
            if sets[at]["order"] == "unicode" && let Some(glyphs) = sets[at]["glyphs"].as_array_mut() { unicode_sorted(glyphs); }
        }
        "delete" => { let at = find(sets, text(req, "name")?)?; sets.remove(at); }
        "add" => {
            let at = find(sets, text(req, "name")?)?;
            let g = glyph(&req["glyph"])?;
            let how = sets[at]["order"].as_str().unwrap_or("front").to_owned();
            let glyphs = sets[at]["glyphs"].as_array_mut().ok_or("broken glyph set")?;
            if !glyphs.iter().any(|o| same(o, &g)) {
                if how == "end" { glyphs.push(g); } else { glyphs.insert(0, g); }
                if how == "unicode" { unicode_sorted(glyphs); }
            }
        }
        "remove" => {
            let at = find(sets, text(req, "name")?)?;
            let index = req["index"].as_u64().ok_or("index required")? as usize;
            let glyphs = sets[at]["glyphs"].as_array_mut().ok_or("broken glyph set")?;
            if index >= glyphs.len() { return Err(format!("the set has {} glyphs", glyphs.len())); }
            glyphs.remove(index);
        }
        "export" => {
            let relative = text(req, "path")?;
            let path = project_file(session, relative)?;
            let names: Option<Vec<&str>> = req["names"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect());
            let chosen: Vec<&Value> = sets.iter().filter(|s| names.as_ref().is_none_or(|n| n.iter().any(|x| s["name"] == *x))).collect();
            if chosen.is_empty() { return Err("no glyph set to export".into()); }
            let body = serde_json::to_vec_pretty(&json!({"type": "folioGlyphSets", "version": 1, "sets": chosen})).map_err(|e| e.to_string())?;
            if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            folio_project::atomic::write_bytes(&path, &body).map_err(|e| e.to_string())?;
            reply.insert("written".into(), json!({"path": relative, "sets": chosen.len()}));
        }
        "import" => {
            let path = project_file(session, text(req, "path")?)?;
            let data: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| format!("not a glyph set file: {e}"))?;
            if data["type"] != "folioGlyphSets" { return Err("not a glyph set file".into()); }
            let mut added = Vec::new();
            for s in data["sets"].as_array().ok_or("not a glyph set file")? {
                let name = s["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).ok_or("a set without a name")?;
                let mut glyphs: Vec<Value> = Vec::new();
                for g in s["glyphs"].as_array().map(Vec::as_slice).unwrap_or(&[]) { let g = glyph(g)?; if !glyphs.iter().any(|o| same(o, &g)) { glyphs.push(g); } }
                let name = free_name(sets, name);
                sets.push(json!({"name": name, "order": order(&s["order"])?, "glyphs": glyphs}));
                added.push(name);
            }
            reply.insert("imported".into(), json!(added));
        }
        other => return Err(format!("unknown op {other}: list, create, edit, delete, add, remove, export or import")),
    }
    Ok(())
}

pub fn run(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let mut reply = Map::new();
    if matches!(req["op"].as_str().unwrap_or("list"), "list" | "export") {
        let mut sets = sets_of(crate::user_settings::run(&json!({}))?["settings"].as_object().unwrap_or(&Map::new()));
        change(req, session, &mut sets, &mut reply)?;
        reply.insert("sets".into(), json!(sets));
        return Ok(Value::Object(reply));
    }
    let settings = crate::user_settings::update(|settings, _| {
        let mut sets = sets_of(settings);
        change(req, session, &mut sets, &mut reply)?;
        settings.insert("glyphSets".into(), json!(sets));
        Ok(())
    })?;
    reply.insert("sets".into(), json!(sets_of(settings.as_object().unwrap_or(&Map::new()))));
    let mut out = Value::Object(reply);
    // the window hears that the user settings changed (as after a userSettings patch)
    out["_settingsEvent"] = crate::user_settings::event(&settings, &req["origin"]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(sets: &mut Vec<Value>, req: Value) -> Result<Map<String, Value>, String> {
        let mut reply = Map::new();
        change(&req, None, sets, &mut reply).map(|_| reply)
    }

    fn g(id: u64, cp: Option<u64>) -> Value { json!({"family": "Arial", "style": "Regular", "id": id, "cp": cp}) }

    #[test]
    fn sets_are_made_filled_in_their_order_and_edited() {
        let mut sets = Vec::new();
        apply(&mut sets, json!({"op": "create", "name": "Arrows", "order": "end"})).unwrap();
        assert!(apply(&mut sets, json!({"op": "create", "name": "Arrows"})).is_err(), "a name is used once");
        assert!(apply(&mut sets, json!({"op": "create", "name": "  "})).is_err(), "a set needs a name");
        for id in [5, 3, 5] { apply(&mut sets, json!({"op": "add", "name": "Arrows", "glyph": g(id, Some(100 - id))})).unwrap(); }
        let ids = |s: &Vec<Value>| s[0]["glyphs"].as_array().unwrap().iter().map(|g| g["id"].as_u64().unwrap()).collect::<Vec<_>>();
        assert_eq!(ids(&sets), [5, 3], "append at end, a glyph already there stays");
        apply(&mut sets, json!({"op": "edit", "name": "Arrows", "order": "unicode"})).unwrap();
        assert_eq!(ids(&sets), [5, 3], "Unicode order: cp 95 before 97");
        apply(&mut sets, json!({"op": "add", "name": "Arrows", "glyph": g(9, None)})).unwrap();
        assert_eq!(ids(&sets), [5, 3, 9], "no Unicode value goes last");
        apply(&mut sets, json!({"op": "edit", "name": "Arrows", "order": "front", "newName": "Signs"})).unwrap();
        apply(&mut sets, json!({"op": "add", "name": "Signs", "glyph": g(7, Some(1))})).unwrap();
        assert_eq!(ids(&sets), [7, 5, 3, 9], "insert at front");
        apply(&mut sets, json!({"op": "remove", "name": "Signs", "index": 1})).unwrap();
        assert_eq!(ids(&sets), [7, 3, 9]);
        assert_eq!(sets[0]["glyphs"][0]["rememberFont"], true);
        let edited = json!([{"family": "Arial", "style": "Regular", "id": 3, "cp": 97, "rememberFont": false}]);
        apply(&mut sets, json!({"op": "edit", "name": "Signs", "glyphs": edited})).unwrap();
        assert_eq!(ids(&sets), [3]);
        assert_eq!(sets[0]["glyphs"][0]["rememberFont"], false);
        assert!(apply(&mut sets, json!({"op": "add", "name": "Signs", "glyph": {"family": "Arial", "style": "Regular", "id": 0}})).is_err(), "glyph id 0 refused");
        apply(&mut sets, json!({"op": "delete", "name": "Signs"})).unwrap();
        assert!(sets.is_empty());
    }

    #[test]
    fn an_imported_name_in_use_gets_a_number() {
        let sets = vec![json!({"name": "A", "order": "front", "glyphs": []}), json!({"name": "A 2", "order": "front", "glyphs": []})];
        assert_eq!(free_name(&sets, "A"), "A 3");
        assert_eq!(free_name(&sets, "B"), "B");
    }
}
