//! Value arguments admitted by both the attached catalog and the engine.
use serde_json::{Value, json};

pub const TOOLS: &[(&str, &[&str])] = &[
    ("arrangeByColumns", &["table"]),
    ("arrangeByRows", &["table"]),
    ("copy", &["ids"]),
    ("edit.acceptPdfComment", &["id"]),
    ("edit.adjustGapKeys", &["page", "from", "to", "nearest", "resize", "move_items"]),
    ("edit.adjustLayout", &["width", "height", "allow_locked", "top_margin", "bottom_margin", "left_margin", "right_margin", "auto_adjust_margins", "adjust_font_size", "font_size_min", "font_size_max"]),
    ("edit.adjustSpacing", &["story", "start", "end"]),
    ("edit.anchorObject", &["id", "story", "at"]),
    ("edit.applyConditionSet", &["name"]),
    ("edit.applyConditions", &["story", "start", "end", "conditions"]),
    ("edit.assignTrapPreset", &["pages", "preset"]),
    ("edit.clearDisplayPerformance", &[]),
    ("edit.collapseNote", &["story", "index", "collapsed"]),
    ("edit.collapseStoryNotes", &["story", "collapsed"]),
    ("edit.convertClippingToFrame", &["ids"]),
    ("edit.convertPathPoints", &["id", "path", "indices", "kind"]),
    ("edit.convertToNote", &["story", "start", "end", "user", "date"]),
    ("edit.convertToProfile", &["rgb_profile", "cmyk_profile", "intent", "black_point_compensation"]),
    ("edit.convertToStaticCaption", &["story", "start", "end"]),
    ("edit.createConditionSet", &["name"]),
    ("edit.createHyperlinkDestination", &["kind", "name", "story", "at", "page", "url", "view_setting"]),
    ("edit.createOutlinesKeepText", &["ids"]),
    ("edit.createOutlinesRange", &["story", "start", "end"]),
    ("edit.createQrVector", &["page", "kind", "content", "rect"]),
    ("edit.createTextPath", &["ids", "text"]),
    ("edit.createTextPathAt", &["id", "point", "reach", "text"]),
    ("edit.cutPath", &["id", "path", "index"]),
    ("edit.cycleCornerShape", &["ids", "corner", "only_corner"]),
    ("edit.deleteCondition", &["name"]),
    ("edit.deleteConditionSet", &["name"]),
    ("edit.deleteCrossReferenceFormat", &["id"]),
    ("edit.deleteHyperlink", &["id"]),
    ("edit.deleteMathml", &["id"]),
    ("edit.deleteNoteAs", &["story", "index", "user", "date"]),
    ("edit.deleteStrokeStyle", &["id"]),
    ("edit.deleteTextAs", &["story", "start", "end", "user", "date"]),
    ("edit.deleteTrapPreset", &["id", "replacement"]),
    ("edit.deleteUnusedHyperlinkDestinations", &[]),
    ("edit.dragTableLine", &["table", "columns", "line", "by", "shift", "insert", "size", "min_before", "min_after"]),
    ("edit.duplicateCrossReferenceFormat", &["id", "name"]),
    ("edit.duplicateTrapPreset", &["id", "name"]),
    ("edit.erasePath", &["id", "page", "points", "fidelity", "px_per_pt"]),
    ("edit.fillPlaceholderText", &["story", "at"]),
    ("edit.insertAnchoredObject", &["story", "at", "width", "height", "position"]),
    ("edit.insertCrossReference", &["story", "at", "target_story", "target_at", "destination", "format", "name"]),
    ("edit.insertGlyph", &["story", "at", "family", "style", "glyph"]),
    ("edit.insertHyperlink", &["story", "start", "end", "name", "url"]),
    ("edit.insertMathml", &["page", "mathml", "at", "size", "color"]),
    ("edit.insertTextAs", &["story", "at", "text", "user", "date"]),
    ("edit.liveCornerDrag", &["ids", "corner", "by", "only_corner"]),
    ("edit.markLinkSources", &["ids"]),
    ("edit.moveGuidesBy", &["ids", "delta"]),
    ("edit.noteToText", &["story", "index"]),
    ("edit.noteToTextAs", &["story", "index", "user", "date"]),
    ("edit.penContinue", &["id", "page", "at_start", "anchors", "lefts", "rights", "join", "join_at_start"]),
    ("edit.pencilExtend", &["id", "page", "points", "fidelity", "px_per_pt", "within_px", "alt"]),
    ("edit.pencilFit", &["page", "points", "fidelity", "smoothness", "px_per_pt", "layer"]),
    ("edit.pencilStroke", &["page", "points", "fidelity", "smoothness", "px_per_pt", "within_px", "alt", "selected", "layer"]),
    ("edit.redefineConditionSet", &["name"]),
    ("edit.releaseAnchoredObject", &["ids"]),
    ("edit.removeNotes", &["story", "document"]),
    ("edit.removeNotesAs", &["story", "document", "user", "date"]),
    ("edit.removePdfComment", &["ids"]),
    ("edit.removeTextFromPath", &["ids"]),
    ("edit.resetHyperlink", &["id", "story", "start", "end"]),
    ("edit.resolveChanges", &["story", "document", "index", "user", "accept"]),
    ("edit.rotateSpreadView", &["spread", "how"]),
    ("edit.saveCrossReferenceFormat", &["format"]),
    ("edit.saveStrokeStyle", &["style"]),
    ("edit.saveTrapPreset", &["preset"]),
    ("edit.setAnchoredObject", &["ids", "props"]),
    ("edit.setBookPageStart", &["start"]),
    ("edit.setCaptionSetup", &["props"]),
    ("edit.setChapterNumber", &["number", "source"]),
    ("edit.setColorPrefs", &["props"]),
    ("edit.setConditionIndicators", &["mode"]),
    ("edit.setConditionOptions", &["name", "props"]),
    ("edit.setCrossReference", &["id", "name", "destination", "format"]),
    ("edit.setDisplayPerformance", &["ids", "setting", "content"]),
    ("edit.setDocPrefs", &["props"]),
    ("edit.setFileInfo", &["props"]),
    ("edit.setGuideOptions", &["ids", "props"]),
    ("edit.setHyperlink", &["id", "name", "url"]),
    ("edit.setHyperlinkDestination", &["id", "name", "url", "page", "view_setting"]),
    ("edit.setImageColor", &["ids", "profile", "intent"]),
    ("edit.setLanguageQuotes", &["language", "props"]),
    ("edit.setMathml", &["id", "mathml", "size", "color"]),
    ("edit.setOpticalMargin", &["story", "props"]),
    ("edit.setPageGeometry", &["pages", "bounds"]),
    ("edit.setPdfCommentStatus", &["ids", "status"]),
    ("edit.setRowKeeps", &["table", "rows", "keep_with_next", "start"]),
    ("edit.setStoryOrientation", &["story", "orientation"]),
    ("edit.setTextDefaultStyle", &["kind", "style"]),
    ("edit.setTextPathBracket", &["id", "which", "point", "from"]),
    ("edit.setTextPathOptions", &["ids", "props"]),
    ("edit.setTrackChanges", &["story", "document", "enabled"]),
    ("edit.smoothPath", &["id", "page", "points", "fidelity", "px_per_pt"]),
    ("edit.splitNote", &["story", "index", "at", "user", "date"]),
    ("edit.swapFrameTextPaint", &["ids"]),
    ("edit.swapTextPaint", &["story", "start", "end"]),
    ("edit.unapplyCondition", &["story", "start", "end", "condition"]),
    ("edit.unlinkContentLink", &["link"]),
    ("edit.updateCrossReference", &["id"]),
    ("edit.updateNote", &["story", "index", "text", "date"]),
    ("setScrollCenter", &["scrollCenter", "ifViewRev"]),
    ("setTextSelection", &["textSelection", "ifViewRev"]),
    ("setTool", &["tool", "ifViewRev"]),
    ("setViewState", &["patch", "ifViewRev"]),
    ("setZoom", &["zoom", "ifViewRev"]),
    ("textCursor", &["story"]),
];

pub fn fields(name: &str) -> Option<&'static [&'static str]> {
    TOOLS.iter().find(|(tool, _)| *tool == name).map(|(_, fields)| *fields)
}

const VIEW_FIELDS: &[&str] = &["page", "zoom", "scrollCenter", "selection", "textSelection", "tool"];

pub fn admit(name: &str, request: &Value, fields: &[&str], envelope: &[&str]) -> Result<(), String> {
    for key in request.as_object().ok_or("expected an object")?.keys() {
        if !envelope.contains(&key.as_str()) && !fields.contains(&key.as_str()) {
            return Err(format!("{name} does not accept {key}"));
        }
    }
    if name == "setViewState" {
        for key in request["patch"].as_object().ok_or("expected a view patch")?.keys() {
            if !VIEW_FIELDS.contains(&key.as_str()) { return Err(format!("unknown view field {key}")); }
        }
    }
    Ok(())
}

pub fn restrict_schema(name: &str, schema: &mut Value, fields: &[&str]) {
    schema["properties"].as_object_mut().unwrap().retain(|key, _| fields.contains(&key.as_str()));
    if let Some(required) = schema["required"].as_array_mut() {
        required.retain(|key| key.as_str().is_some_and(|k| fields.contains(&k)));
    }
    if name == "setViewState" {
        schema["properties"]["patch"] = json!({"type":"object", "additionalProperties":false,
            "properties": VIEW_FIELDS.iter().map(|key| ((*key).to_owned(), json!({}))).collect::<serde_json::Map<_,_>>()});
    }
}
