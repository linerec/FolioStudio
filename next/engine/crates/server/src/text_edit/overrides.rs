use super::*;
use std::collections::BTreeMap;

pub fn query(session: &crate::Session) -> Result<Value, String> {
    let fs = shown_frames(&session.doc, &session.layout, false);
    let mut state = session.edit.as_ref().ok_or("no editable document")?.lock().map_err(|_| "edit state unavailable")?;
    let mut ranges = BTreeMap::new();
    for line in all_lines(&fs) {
        if !ranges.contains_key(line.owner) {
            let found = state.editor.style_override_ranges(line.owner).map_err(|e| e.to_string())?;
            ranges.insert(line.owner.to_owned(), found);
        }
    }
    let mut marks = vec![];
    for line in all_lines(&fs) {
        for &(start, end, paragraph, character) in &ranges[line.owner] {
            for c in line.clusters.iter().filter(|c| c.start < end && c.end > start) {
                let a = x_at(c, start.max(c.start));
                let b = x_at(c, end.min(c.end));
                let mut mark = rect_json(line.f, Rect { left:a.min(b),right:a.max(b),top:c.top,bottom:c.bottom });
                mark["paragraph"] = json!(paragraph);
                mark["character"] = json!(character);
                marks.push(mark);
            }
        }
    }
    Ok(json!({"marks":marks}))
}
