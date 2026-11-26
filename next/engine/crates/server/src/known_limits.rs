//! `knownLimits`: what Folio does differently from InDesign (principle 2, item 3), for the Help ▸ Known Differences and Limits window and
//! for an LLM. The list is `next/oracle/rules/limits.toml` (generated into `limits.generated.json`, documented in
//! `artifacts/finish-20261113/LIMITS.ko.md`); the engine does not decide anything here, it only answers.
use serde_json::{Value, json};

/// `{"cmd":"knownLimits"}` answers `{"version","doc","areas":[{"id","ko","en"}],"table":{…},"differences":[{"id","kind","area","top","ko","en"}]}`.
/// `area` (an area id such as `"text"`) and `kind` (`"approx"`, `"unsupported"`, `"unverified"` or `"excluded"`) keep only those; `top: true` keeps the ten
/// most important. With `"document": true` the open document's own IDML is checked as well and `document` answers
/// `{"used":[{…,"count"}],"fixed":[…]}` (the same block the import and export reports carry as `limits`).
pub fn run(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let cat = folio_project::limits::catalog();
    let area = req["area"].as_str();
    if let Some(a) = area
        && !cat.areas.iter().any(|x| x.id == a)
    {
        return Err(format!("unknown area {a:?}; one of: {}", cat.areas.iter().map(|x| x.id.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let kind = req["kind"].as_str();
    if let Some(k) = kind
        && !["approx", "unsupported", "unverified", "excluded"].contains(&k)
    {
        return Err(format!("unknown kind {k:?}; one of: approx, unsupported, unverified, excluded"));
    }
    let top = req["top"].as_bool().unwrap_or(false);
    let mut differences: Vec<&folio_project::limits::Difference> = cat
        .differences
        .iter()
        .filter(|d| area.is_none_or(|a| d.area == a) && kind.is_none_or(|k| d.kind == k) && (!top || d.top.is_some()))
        .collect();
    if top {
        differences.sort_by_key(|d| d.top);
    }
    let mut answer = json!({
        "version": cat.version, "doc": cat.doc,
        "areas": cat.areas.iter().map(|a| json!({"id": a.id, "ko": a.ko, "en": a.en})).collect::<Vec<_>>(),
        "table": cat.table,
        "differences": differences.iter().map(|d| json!({"id": d.id, "kind": d.kind, "area": d.area, "top": d.top, "ko": d.ko, "en": d.en})).collect::<Vec<_>>(),
    });
    if req["document"].as_bool().unwrap_or(false) {
        let edit = session.and_then(|s| s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        let hits = folio_project::limits::scan_parts(edit.editor.parts().iter().map(|(n, t)| (n.as_str(), t.as_str())));
        answer["document"] = folio_project::limits::report(&hits);
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_the_whole_list_or_a_part_of_it() {
        let all = run(&json!({}), None).unwrap();
        let n = all["differences"].as_array().unwrap().len();
        assert!(n >= 40, "{n}");
        assert!(all["table"]["unclassified"].as_u64().is_some(), "the table reports its not yet checked rows (none since interop-c2, 2026-11-19)");
        let top = run(&json!({"top": true}), None).unwrap();
        let ids: Vec<&str> = top["differences"].as_array().unwrap().iter().filter_map(|d| d["id"].as_str()).collect();
        assert_eq!(ids.len(), 10);
        assert_eq!(ids[0], "saved-lines");
        let refused = run(&json!({"kind": "unsupported", "area": "import"}), None).unwrap();
        assert!(refused["differences"].as_array().unwrap().iter().all(|d| d["kind"] == "unsupported" && d["area"] == "import"));
        assert!(run(&json!({"area": "nope"}), None).is_err());
        assert!(run(&json!({"kind": "nope"}), None).is_err());
        // the open document is needed to ask what it uses
        assert_eq!(run(&json!({"document": true}), None).unwrap_err(), "no document open");
    }
}
