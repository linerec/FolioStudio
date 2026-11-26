//! `textPathBrackets`: where the bracket handles of a selected text path are drawn (rules next/oracle/rules/typepath-c14.ko.md), optionally for a
//! Selection tool drag in progress (`drag`: the brackets the drag would give; nothing is edited). Coordinates are the item's own: the screen maps
//! them with the item's transform.
use serde_json::{Value, json};

pub fn query(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let s = session.ok_or("no document open")?;
    let id = req["target"].as_str().ok_or("target required")?;
    let mut found = None;
    for spread in s.doc.spreads.iter().chain(&s.doc.master_spreads) {
        for item in &spread.items { item.walk(&mut |i| { if i.id == id { found = Some(i); } }); }
    }
    let Some(item) = found else { return Err(format!("no item {id}")) };
    let Some(path) = item.text_path.as_ref() else { return Ok(json!({"type": "textPathBrackets", "supported": false})) };
    let (mut start, mut end) = (path.start, path.end);
    let mut draggable = false;
    if let Some(drag) = req.get("drag").filter(|d| d.is_object()) {
        let pair = |k: &str| -> Result<[f64; 2], String> {
            let v = drag[k].as_array().filter(|a| a.len() == 2).ok_or(format!("drag.{k} must be [x, y]"))?;
            Ok([v[0].as_f64().ok_or("number expected")?, v[1].as_f64().ok_or("number expected")?])
        };
        let (point, from) = (pair("point")?, pair("from").unwrap_or([0.0, 0.0]));
        let which = drag["which"].as_str().ok_or("drag.which required")?;
        let edit = s.edit.as_ref().ok_or("text editing requires an editable document")?;
        let mut state = edit.lock().map_err(|_| "edit state unavailable")?;
        let b = state.editor.text_path_drag_values(id, which, &point, &from).map_err(|e| e.message)?;
        // (the model's values may count from a horizontal line's left end: move them by what the drag changed)
        if path.curve.as_ref().is_some_and(|c| !c.open) { (start, end) = (b.start, b.end); } else {
            start += b.start - b.was_start;
            end += b.end - b.was_end;
        }
    }
    if let Some(edit) = s.edit.as_ref() {
        // The selection tool grabs the brackets of an unflipped text path (what `setTextPathBracket` edits).
        draggable = !path.flipped && edit.lock().is_ok();
    }
    let (above, below) = folio_text::textpath_handles::extents(s.layout.frames.get(id), &s.fonts);
    let Some(handles) = folio_text::textpath_handles::bracket_handles(item, start, end, above, below) else { return Ok(json!({"type": "textPathBrackets", "supported": false})) };
    Ok(json!({"type": "textPathBrackets", "supported": true, "draggable": draggable, "handles": handles, "start": start, "end": end}))
}
