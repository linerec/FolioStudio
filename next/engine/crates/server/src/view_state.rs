//! Ephemeral view state belongs to the open session, never to the document or undo stack.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub id: String,
    pub page: usize,
    #[serde(default)]
    pub content: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSelection {
    pub story: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewState {
    pub page: usize,
    pub zoom: f64,
    /// Centre of the scrollable canvas in unscaled canvas points, not page coordinates.
    pub scroll_center: [f64; 2],
    pub selection: Vec<Selection>,
    pub text_selection: Option<TextSelection>,
    pub tool: String,
    pub show_hidden_spreads: bool,
    pub view_rev: u64,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { page: 0, zoom: 1.0, scroll_center: [0.0; 2], selection: vec![], text_selection: None, tool: "selection".into(), show_hidden_spreads: false, view_rev: 0 }
    }
}

pub fn initial() -> ViewState {
    let mut view = ViewState::default();
    if let Ok(settings) = crate::user_settings::run(&json!({})) {
        view.show_hidden_spreads = settings["settings"]["preferences"]["folio.showHiddenSpreads"] == "1";
    }
    view
}

const TOOLS: &[&str] = &[
    "selection", "direct", "page", "gap", "collector", "placer", "type", "typePath", "line",
    "pen", "penAdd", "penDelete", "convert", "pencil", "smooth", "erase", "rectFrame",
    "ellipseFrame", "polygonFrame", "rect", "ellipse", "polygon", "scissors", "freeTransform",
    "rotate", "scale", "shear", "gradient", "gradientFeather", "note", "eyedropper", "measure", "hand", "zoom",
];

fn candidate(current: &ViewState, patch: &Value) -> Result<ViewState, String> {
    let mut value = serde_json::to_value(current).map_err(|e| e.to_string())?;
    for (key, value_patch) in patch.as_object().ok_or("patch must be an object")? {
        if key == "viewRev" { return Err("viewRev is read only".into()); }
        value[key] = value_patch.clone();
    }
    let next: ViewState = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if !next.zoom.is_finite() || !(0.03..=40.0).contains(&next.zoom) { return Err("zoom must be between 0.03 and 40".into()); }
    if next.scroll_center.iter().any(|v| !v.is_finite() || *v < 0.0) { return Err("scrollCenter must contain two nonnegative finite numbers".into()); }
    if !TOOLS.contains(&next.tool.as_str()) { return Err("unknown tool".into()); }
    if next.text_selection.as_ref().is_some_and(|t| t.start > t.end) { return Err("text selection start must not exceed end".into()); }
    let mut ids = std::collections::HashSet::new();
    if next.selection.iter().any(|s| !ids.insert(&s.id)) { return Err("duplicate selected id".into()); }
    Ok(next)
}

pub fn read(_: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let s = session.ok_or("no document open")?;
    let view = s.view.lock().map_err(|e| e.to_string())?;
    Ok(json!({"viewState": *view, "viewRev": view.view_rev}))
}

pub fn patch(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let s = session.ok_or("no document open")?;
    let mut view = s.view.lock().map_err(|e| e.to_string())?;
    if let Some(rev) = req.get("ifViewRev") {
        if rev.as_u64() != Some(view.view_rev) {
            return Ok(json!({"ok": false, "error": "view revision conflict", "viewState": *view, "viewRev": view.view_rev}));
        }
    }
    let patch = &req["patch"];
    let mut next = candidate(&view, patch)?;
    let pages: Vec<_> = s.doc.spreads.iter().chain(&s.doc.master_spreads).flat_map(|s| &s.pages).map(|p| p.index).collect();
    if patch.get("page").is_some() && !pages.contains(&next.page) { return Err("unknown page index".into()); }
    if patch.get("selection").is_some() {
        for selected in &next.selection {
            if !pages.contains(&selected.page) { return Err("unknown selection page".into()); }
            if !page_has_item(s, selected.page, &selected.id)? { return Err(format!("unknown item on page: {}", selected.id)); }
        }
    }
    if patch.get("textSelection").is_some() {
        if let Some(t) = &next.text_selection {
            let shared = s.edit.as_ref().ok_or("text selection requires an editable document")?;
            let editor = shared.lock().map_err(|e| e.to_string())?;
            let text = editor.editor.parts().iter().filter(|(name, _)| name.starts_with("Stories/"))
                .find_map(|(_, xml)| folio_edit::text::contents(xml, &t.story).ok()).ok_or("unknown or unsupported story")?;
            if t.end > text.chars().count() { return Err("text selection exceeds story length".into()); }
        }
    }
    if patch.get("showHiddenSpreads").is_some() {
        crate::user_settings::run(&json!({"values": {"folio.showHiddenSpreads": if next.show_hidden_spreads { "1" } else { "0" }}}))?;
    }
    let changed = next != *view;
    if changed { next.view_rev += 1; *view = next; }
    let mut result = json!({"viewState": *view, "viewRev": view.view_rev});
    if changed {
        result["_viewEvent"] = json!({"event": "view-state", "path": s.source_path, "detail": {
            "viewState": *view, "patch": patch, "origin": req["origin"]
        }});
    }
    Ok(result)
}

fn field(req: &Value, session: Option<&crate::Session>, name: &str) -> Result<Value, String> {
    let value = req.get(name).ok_or_else(|| format!("{name} required"))?;
    let mut r = req.clone();
    r["patch"] = json!({name: value});
    patch(&r, session)
}

pub fn page(r: &Value, s: Option<&crate::Session>) -> Result<Value, String> { field(r, s, "page") }
pub fn zoom(r: &Value, s: Option<&crate::Session>) -> Result<Value, String> { field(r, s, "zoom") }
pub fn tool(r: &Value, s: Option<&crate::Session>) -> Result<Value, String> { field(r, s, "tool") }
pub fn scroll(r: &Value, s: Option<&crate::Session>) -> Result<Value, String> { field(r, s, "scrollCenter") }
pub fn text(r: &Value, s: Option<&crate::Session>) -> Result<Value, String> {
    let mut r = r.clone();
    let selection = r.get("textSelection").ok_or("textSelection required (null clears it)")?.clone();
    r["patch"] = if selection.is_null() { json!({"textSelection": null}) } else { json!({"textSelection": selection, "tool": "type"}) };
    patch(&r, s)
}

pub fn selection(r: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let s = session.ok_or("no document open")?;
    let ids: Vec<String> = serde_json::from_value(r["ids"].clone()).map_err(|e| e.to_string())?;
    let pages: Vec<_> = if let Some(page) = r.get("page") {
        vec![page.as_u64().ok_or("page must be a nonnegative integer")? as usize]
    } else {
        s.doc.spreads.iter().chain(&s.doc.master_spreads).flat_map(|s| &s.pages).map(|p| p.index).collect()
    };
    let mut entries = vec![];
    for id in ids {
        let mut found = None;
        for page in &pages {
            if page_has_item(s, *page, &id)? { found = Some(*page); break; }
        }
        entries.push(Selection { page: found.ok_or_else(|| format!("unknown item: {id}"))?, id, content: false });
    }
    let mut r = r.clone();
    r["patch"] = json!({"selection": entries, "textSelection": null});
    patch(&r, session)
}

fn page_has_item(s: &crate::Session, page: usize, id: &str) -> Result<bool, String> {
    fn contains(items: &Value, id: &str) -> bool {
        items.as_array().is_some_and(|items| items.iter().any(|item| item["id"] == id || contains(&item["children"], id)))
    }
    let raw = crate::inspect::command(&json!({"cmd": "pageItems", "page": page}), &s.doc, &s.layout, &s.colors, &s.inspect)?;
    let result: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    Ok(contains(&result["items"]["items"], id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_patch_is_atomic_and_revision_is_read_only() {
        let view = ViewState::default();
        for patch in [json!({"zoom": 0}), json!({"tool": "missing"}), json!({"scrollCenter": [-1, 3]}),
            json!({"textSelection": {"story": "s", "start": 3, "end": 2}}), json!({"viewRev": 2}), json!({"unknown": true})] {
            assert!(candidate(&view, &patch).is_err());
            assert_eq!(view, ViewState::default());
        }
        let next = candidate(&view, &json!({"page": 2, "zoom": 1.5, "tool": "direct"})).unwrap();
        assert_eq!((next.page, next.zoom, next.tool.as_str()), (2, 1.5, "direct"));
        assert_eq!(next.view_rev, 0);
        assert!(candidate(&view, &json!({"showHiddenSpreads": "1"})).is_err());
        assert!(candidate(&view, &json!({"showHiddenSpreads": true})).unwrap().show_hidden_spreads);
    }
}
