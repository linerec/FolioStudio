use super::*;

pub(super) fn source_style(w: &mut Work<'_>, id: &str) -> Result<Option<String>, EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    check_item(&ix, t, i, id)?;
    // a text frame or a shape: the panel menu captures both the same way (shapes: measured in objstyles-m1)
    Ok(["TextFrame", "Rectangle", "Oval", "Polygon", "GraphicLine"].contains(&ix.local(t, i)).then(|| ix.value(t, i, "AppliedObjectStyle").unwrap_or_else(|| NONE.into())))
}

/// Whether the item is a text frame (not a shape).
pub(super) fn is_text_frame(w: &mut Work<'_>, id: &str) -> Result<bool, EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    Ok(ix.local(t, i) == "TextFrame")
}

fn overlay(text: &mut String, path: &[&str], source: &str, sx: &Index, node: usize) -> Result<(), EditError> {
    let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
    let Some(target) = (if path.is_empty() { Some(0) } else { ix.path(text, 0, path) }) else {
        if path.len() == 1 && path[0].ends_with("TransparencySetting") {
            let at = ix.named(text, 0, "ObjectStyleObjectEffectsCategorySettings").first().copied();
            if let Some(at) = at {
                let n = &sx.nodes[node];
                text.insert_str(ix.nodes[at].start, &format!("{}\n", &source[n.start..n.end]));
            }
        } else if path.first().is_some_and(|p| p.ends_with("TransparencySetting"))
            && let Some(parent) = ix.path(text, 0, &path[..path.len()-1]) {
            let n = &sx.nodes[node];
            let rank = |name: &str| super::super::blending::CHILDREN.iter().position(|&c| c == name).unwrap_or(usize::MAX);
            let at = ix.children(parent).iter().copied().find(|&c| rank(ix.local(text, c)) > rank(path.last().unwrap()))
                .map(|c| ix.nodes[c].start).unwrap_or(ix.nodes[parent].close_start);
            text.insert_str(at, &format!("{}\n", &source[n.start..n.end]));
        }
        return Ok(())
    };
    let mut edits = Vec::new();
    for attr in sx.attrs(source, node) {
        let key = &source[attr.name.0..attr.name.1];
        if path.is_empty() && (matches!(key, "Self" | "Name") || ix.attr(text, target, key).is_none() && key != "OverprintFill") { continue; }
        edits.push(attr_edit(&ix, text, target, key, &sx.value(source, node, key).unwrap()));
    }
    local_edit(text, edits);
    for &child in sx.children(node) {
        let name = sx.local(source, child);
        if path.is_empty() && !["TextFramePreference", "TextWrapPreference", "TransparencySetting", "StrokeTransparencySetting", "FillTransparencySetting", "ContentTransparencySetting", "BaselineFrameGridOption", "AnchoredObjectSetting", "ObjectExportOption"].contains(&name) { continue; }
        let mut next = path.to_vec(); next.push(name);
        if name == "InsetSpacing" {
            let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
            if let Some(at) = ix.path(text, 0, &next) {
                let n = &sx.nodes[child];
                text.replace_range(ix.nodes[at].start..ix.nodes[at].end, &source[n.start..n.end]);
            }
        } else { overlay(text, &next, source, sx, child)?; }
    }
    Ok(())
}

pub(super) fn capture(w: &mut Work<'_>, id: &str, text: &mut String, creating: bool) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    overlay(text, &[], &t, &ix, i)?;
    let captured = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
    if captured.attr(text, 0, "FillColor").is_some_and(|v| v != "Swatch/None") && captured.attr(text, 0, "OverprintFill").is_none() {
        set_attr(text, &[], "OverprintFill", "false")?;
    }
    let captured = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
    if captured.attr(text, 0, "StrokeColor").is_some_and(|v| v != "Swatch/None") && captured.attr(text, 0, "OverprintStroke").is_none() {
        set_attr(text, &[], "OverprintStroke", "false")?;
    }
    if creating {
        let sx = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
        let keys: Vec<_> = sx.attrs(text, 0).iter().map(|a| text[a.name.0..a.name.1].to_owned())
            .filter(|k| k.starts_with("Enable") && !matches!(k.as_str(), "EnableParagraphStyle" | "EnableFrameFittingOptions")).collect();
        for key in keys { set_attr(text, &[], &key, "true")?; }
    }
    if ix.local(&t, i) != "TextFrame" {
        // a shape has no paragraph style or text of its own: the new style says so and gets the text frame defaults of the menu's capture
        set_attr(text, &[], "AppliedParagraphStyle", "n")?;
        set_attr(text, &["StoryPreference"], "FrameType", "TextFrameType")?;
        set_attr(text, &["StoryPreference"], "StoryDirection", "LeftToRightDirection")?;
        set_attr(text, &["StoryPreference"], "StoryOrientation", "Horizontal")?;
        // overprint switches exist only for a colour that is not None
        for (color, overprint) in [("FillColor", "OverprintFill"), ("StrokeColor", "OverprintStroke")] {
            let now = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
            if now.attr(text, 0, color).is_none_or(|v| v == "Swatch/None")
                && let Some((range, replacement)) = attr_removal(&now, text, 0, overprint) { text.replace_range(range, &replacement); }
        }
    }
    if ix.named(&t, i, "AnchoredObjectSetting").is_empty() {
        for (key, value) in [("AnchorPoint", "TopLeftAnchor"), ("PinPosition", "false"), ("VerticalAlignment", "CenterAlign")] {
            set_attr(text, &["AnchoredObjectSetting"], key, value)?;
        }
    }
    if let Some(story) = ix.value(&t, i, "ParentStory") {
        let part = format!("Stories/Story_{story}.xml");
        let story = w.text(&part)?;
        let sx = Index::scan(story).map_err(|e| EditError::new("xml", e))?;
        for n in 0..sx.nodes.len() {
            if sx.local(story, n) == "StoryPreference" { overlay(text, &["StoryPreference"], story, &sx, n)?; }
        }
        if let Some(n) = (0..sx.nodes.len()).find(|&n| sx.local(story, n) == "ParagraphStyleRange")
            && let Some(value) = sx.value(story, n, "AppliedParagraphStyle") { set_attr(text, &[], "AppliedParagraphStyle", &value)?; }
    }
    if ix.local(&t, i) != "TextFrame" { return Ok(()); }
    let prefs = w.text("Resources/Preferences.xml")?;
    let px = Index::scan(prefs).map_err(|e| EditError::new("xml", e))?;
    let own = ix.named(&t, i, "TextFrameFootnoteOptionsObject").first().copied();
    let overrides = own.is_some_and(|n| ix.attr(&t, n, "EnableOverrides") == Some("true"));
    if let Some(n) = (0..px.nodes.len()).find(|&n| px.local(prefs, n) == "FootnoteOption") {
        for (doc, frame, style) in [("EnableStraddling", "SpanFootnotesAcross", "FootnotesSpanAcrossColumns"), ("Spacer", "MinimumSpacingOption", "FootnotesMinimumSpacing"), ("SpaceBetween", "SpaceBetweenFootnotes", "FootnotesSpaceBetween")] {
            let value = if overrides { own.and_then(|n| ix.value(&t, n, frame)) } else { px.value(prefs, n, doc) };
            if let Some(value) = value {
                set_attr(text, &["TextFrameFootnoteOptionsObject"], frame, &value)?;
                set_attr(text, &["TextFramePreference"], style, &value)?;
            }
        }
    }
    Ok(())
}
