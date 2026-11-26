//! Document presets (File ▸ Document Presets): the New Document dialog's settings under a name, kept in the user's settings
//! (`documentPresets`, the app's data folder, never a project). Lengths are in points. The built-in [Default] is US Letter in inches
//! with 0.5 in margins, as InDesign's own default for a US user; it is not in the list and cannot be removed.
//!
//! `documentPresets` lists them, `action: "save"` adds one (or replaces the one of its name, or `replacing`'s, keeping its place) and
//! `action: "delete"` removes one; a change answers the list and the settings event the screen updates its menu from.
use serde_json::{Map, Value, json};

/// [Default] (`name` empty).
pub fn default_preset() -> Value {
    json!({"name":"","unit":"Inches","width":612,"height":792,"pages":1,"start":1,"facing":true,"top":36,"bottom":36,"inside":36,"outside":36,
           "columns":1,"gutter":12,"bleed":[0,0,0,0],"slug":[0,0,0,0]})
}

/// A preset as the settings may hold it, kept to what a preset is (numbers that are not are the default's); none without a name.
pub fn clean(raw: &Value) -> Option<Value> {
    let name = raw["name"].as_str().map(str::trim).filter(|n| !n.is_empty())?;
    let base = default_preset();
    let num = |key: &str| raw[key].as_f64().filter(|v| v.is_finite()).map_or_else(|| base[key].clone(), |v| json!(v));
    let whole = |key: &str| raw[key].as_f64().filter(|v| v.is_finite()).map_or_else(|| base[key].clone(), |v| json!(v.round() as i64));
    let four = |key: &str| match raw[key].as_array() {
        Some(a) if a.len() == 4 => json!(a.iter().map(|x| x.as_f64().filter(|v| v.is_finite()).unwrap_or(0.)).collect::<Vec<_>>()),
        _ => json!([0, 0, 0, 0]),
    };
    Some(json!({
        "name": name, "unit": raw["unit"].as_str().map_or_else(|| base["unit"].clone(), |u| json!(u)),
        "width": num("width"), "height": num("height"), "pages": whole("pages"), "start": whole("start"), "facing": raw["facing"] != false,
        "top": num("top"), "bottom": num("bottom"), "inside": num("inside"), "outside": num("outside"),
        "columns": whole("columns"), "gutter": num("gutter"), "bleed": four("bleed"), "slug": four("slug"),
    }))
}

/// The presets the user keeps, in their order.
pub fn list() -> Vec<Value> {
    crate::user_settings::run(&json!({})).ok()
        .and_then(|v| v["settings"]["documentPresets"].as_array().cloned())
        .unwrap_or_default().iter().filter_map(clean).collect()
}

/// The preset of a name (case does not matter), [Default] for the empty name.
pub fn named(name: &str) -> Option<Value> {
    if name.is_empty() { return Some(default_preset()); }
    list().into_iter().find(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase() == name.to_lowercase()))
}

/// The list with `preset` added, or in place of the one of the same name (or `replacing`'s) — as the screen's `withPreset` did.
fn with_preset(list: &[Value], preset: Value, replacing: Option<&str>) -> Vec<Value> {
    let lower = |v: &Value| v["name"].as_str().unwrap_or("").to_lowercase();
    let key = replacing.map_or_else(|| lower(&preset), str::to_lowercase);
    let at = list.iter().position(|x| lower(x) == key);
    let mut rest: Vec<Value> = list.iter().enumerate().filter(|(i, x)| Some(*i) != at && lower(x) != lower(&preset)).map(|(_, x)| x.clone()).collect();
    match at { Some(at) if at <= rest.len() => rest.insert(at, preset), _ => rest.push(preset) }
    rest
}

fn saved(list: Vec<Value>, origin: &Value) -> Result<Value, String> {
    let reply = crate::user_settings::command(&json!({"patch": {"documentPresets": list}, "origin": origin}), None)?;
    let presets: Vec<Value> = reply["settings"]["documentPresets"].as_array().into_iter().flatten().filter_map(clean).collect();
    Ok(json!({"ok": true, "type": "documentPresets", "presets": presets, "_settingsEvent": reply["_settingsEvent"]}))
}

/// `documentPresets {action?: "list" | "save" | "delete", preset?, replacing?, name?}`.
pub fn run(req: &Value) -> Result<Value, String> {
    match req["action"].as_str().unwrap_or("list") {
        "list" => Ok(json!({"ok": true, "type": "documentPresets", "presets": list(), "default": default_preset()})),
        "save" => {
            let mut raw = req["preset"].clone();
            if !raw.is_object() { return Err("preset must be an object".into()); }
            let name = raw["name"].as_str().map(str::trim).unwrap_or("").to_owned();
            if name.is_empty() { return Err("preset needs a name".into()); }
            raw["name"] = json!(name);
            let preset = clean(&raw).ok_or("preset must have a name")?;
            saved(with_preset(&list(), preset, req["replacing"].as_str().filter(|r| !r.is_empty())), &req["origin"])
        }
        "delete" => {
            let name = req["name"].as_str().ok_or("name must be a preset name")?.to_lowercase();
            let all = list();
            if !all.iter().any(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase() == name)) { return Err(format!("no document preset {name}")); }
            saved(all.into_iter().filter(|p| p["name"].as_str().is_none_or(|n| n.to_lowercase() != name)).collect(), &req["origin"])
        }
        other => Err(format!("action must be list, save or delete, not {other}")),
    }
}

/// A preset's settings as the dialog's fields (four-sided values one field per side, the "all the same" boxes ticked when the sides are).
pub fn fields_of(p: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    for key in ["unit", "width", "height", "pages", "start", "facing", "top", "bottom", "inside", "outside", "columns", "gutter"] { out.insert(key.into(), p[key].clone()); }
    for (four, prefix) in [("bleed", "bleed"), ("slug", "slug")] {
        for (i, side) in ["Top", "Bottom", "Inside", "Outside"].iter().enumerate() { out.insert(format!("{prefix}{side}"), p[four][i].clone()); }
    }
    let all_same = |keys: [&Value; 4]| keys.iter().all(|v| v.as_f64() == keys[0].as_f64());
    out.insert("marginLink".into(), json!(all_same([&p["top"], &p["bottom"], &p["inside"], &p["outside"]])));
    out.insert("bleedLink".into(), json!(all_same([&p["bleed"][0], &p["bleed"][1], &p["bleed"][2], &p["bleed"][3]])));
    out.insert("slugLink".into(), json!(all_same([&p["slug"][0], &p["slug"][1], &p["slug"][2], &p["slug"][3]])));
    out
}

/// Whether the dialog's values are the preset's settings (lengths within 0.01 pt), as the screen told [Custom] apart.
pub fn same(values: &Value, p: &Value) -> bool {
    let near = |a: &Value, b: &Value| matches!((a.as_f64(), b.as_f64()), (Some(a), Some(b)) if (a - b).abs() < 0.01);
    let fields = fields_of(p);
    ["unit", "facing", "pages", "start", "columns"].iter().all(|k| values[*k].as_f64().map_or(values[*k] == fields[*k], |v| fields[*k].as_f64() == Some(v)))
        && ["width", "height", "top", "bottom", "inside", "outside", "gutter", "bleedTop", "bleedBottom", "bleedInside", "bleedOutside",
            "slugTop", "slugBottom", "slugInside", "slugOutside"].iter().all(|k| near(&values[*k], &fields[*k]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_cleaned_replaced_in_place_and_compared_as_the_screen_did() {
        assert!(clean(&json!({"name":"  "})).is_none());
        let p = clean(&json!({"name":" A ","pages":2.4,"bleed":[1,2],"facing":false})).unwrap();
        assert_eq!((p["name"].clone(), p["pages"].clone(), p["bleed"].clone(), p["facing"].clone(), p["width"].clone()), (json!("A"), json!(2), json!([0,0,0,0]), json!(false), json!(612)));
        let named = |n: &str| json!({"name": n});
        let list = vec![named("A"), named("B"), named("C")];
        let names = |l: &[Value]| l.iter().map(|x| x["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert_eq!(names(&with_preset(&list, named("b"), None)), ["A", "b", "C"], "same name (any case): in its place");
        assert_eq!(names(&with_preset(&list, named("D"), Some("A"))), ["D", "B", "C"], "renamed: in the place of the one it replaces");
        assert_eq!(names(&with_preset(&list, named("C"), Some("A"))), ["C", "B"], "renamed onto another: that one goes");
        assert_eq!(names(&with_preset(&list, named("E"), None)), ["A", "B", "C", "E"]);
        let mut values = Value::Object(fields_of(&default_preset()));
        assert!(same(&values, &default_preset()));
        assert_eq!((values["marginLink"].clone(), values["bleedInside"].clone()), (json!(true), json!(0)));
        values["top"] = json!(36.004);
        assert!(same(&values, &default_preset()), "within 0.01 pt");
        values["pages"] = json!(2);
        assert!(!same(&values, &default_preset()));
    }
}
