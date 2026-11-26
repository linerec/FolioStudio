use super::*;

pub(super) fn apply(w: &mut Work<'_>, id: &str, style: &str, clear: bool, nested: bool) -> Result<(), EditError> {
    let (part, ix, item) = w.find(id)?;
    let xml = w.text(&part)?;
    let children: Vec<_> = ix.children(item).iter().copied()
        .filter(|&child| is_item(&ix, xml, child))
        .filter_map(|child| ix.value(xml, child, "Self"))
        .collect();
    apply_items(w, &children, style, clear, true)?;
    for child in &children {
        normalize_insets(w, child)?;
    }

    let (part, ix, item) = w.find(id)?;
    let xml = w.text(&part)?;
    let old = ix.attr(xml, item, "AppliedObjectStyle").unwrap_or(NONE);
    let mut edits = application::settings(w, &ix, xml, item, old, style, clear, nested);
    if nested && clear {
        edits.extend([("FillColor", "EnableFill"), ("StrokeColor", "EnableStroke"), ("StrokeWeight", "EnableStroke")]
            .into_iter().filter_map(|(key, category)| {
                common_value(w, &ix, xml, item, key, category)
                    .map(|value| attr_edit(&ix, xml, item, key, &value))
            }));
    } else if !nested {
        edits.push(attr_edit(&ix, xml, item, "AppliedObjectStyle", style));
    }
    if clear || !nested && ix.attr(xml, item, "FillColor").is_some() {
        edits.extend(application::GRADIENT_GEOMETRY.iter().filter_map(|key| attr_removal(&ix, xml, item, key)));
    }
    w.splice(&part, edits)?;
    if nested {
        effects::apply(w, id, style, clear, true)?;
    }
    normalize(w, id, !nested && !clear)
}

fn normalize_insets(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "TextFrame" { return Ok(()); }
    if let Some(at) = ix.path(&t, i, &["TextFramePreference", "Properties", "InsetSpacing"]) {
        let values: Vec<_> = ix.named(&t, at, "ListItem").iter().map(|&n| element_text(&ix, &t, n)).collect();
        if values.len() == 4 && values.iter().all(|v| *v == "0") {
            w.splice(&part, vec![(ix.nodes[at].start..ix.nodes[at].end, format!("<InsetSpacing type=\"unit\">{}</InsetSpacing>", values[0]))])?;
        }
    }
    Ok(())
}

fn normalize(w: &mut Work<'_>, id: &str, keep_common_stroke: bool) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) == "Group" {
        let mut edits = Vec::new();
        for (key, enable) in [("FillColor", "EnableFill"), ("StrokeColor", "EnableStroke"), ("StrokeWeight", "EnableStroke")] {
            if keep_common_stroke && enable == "EnableStroke"
                && let Some(value) = common_value(w, &ix, &t, i, key, enable)
                && Some(&value) != from_style(w, &ix, &t, i, &[], key, enable).as_ref()
            {
                edits.push(attr_edit(&ix, &t, i, key, &value));
                continue;
            }
            let mixed_fill = key == "FillColor" && common_value(w, &ix, &t, i, key, enable).is_none();
            if mixed_fill || ix.value(&t, i, key) == from_style(w, &ix, &t, i, &[], key, enable) {
                edits.extend(attr_removal(&ix, &t, i, key));
            }
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}

fn common_value(w: &Work<'_>, ix: &Index, t: &str, i: usize, key: &str, enable: &str) -> Option<String> {
    let mut values = ix.children(i).iter().copied().filter(|&c| is_item(ix, t, c)).map(|c| {
        if ix.local(t, c) == "Group" { common_value(w, ix, t, c, key, enable) }
        else { ix.value(t, c, key).or_else(|| from_style(w, ix, t, c, &[], key, enable)) }
    });
    let first = values.next()??;
    values.all(|v| v.as_deref() == Some(&first)).then_some(first)
}

pub(super) fn refresh_parent(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, item) = w.find(id)?;
    let xml = w.text(&part)?;
    let parents: Vec<_> = ix.ancestors(item).filter(|&parent| ix.local(xml, parent) == "Group")
        .filter_map(|parent| ix.value(xml, parent, "Self")).collect();
    for parent in parents {
        let (part, ix, item) = w.find(&parent)?;
        let xml = w.text(&part)?;
        let common = common_value(w, &ix, xml, item, "FillColor", "EnableFill");
        let inherited = from_style(w, &ix, xml, item, &[], "FillColor", "EnableFill");
        let fill = common.filter(|value| Some(value) != inherited.as_ref() && value != "Swatch/None");
        if fill == ix.value(xml, item, "FillColor") {
            continue;
        }
        let mut edits: application::Edits = match fill {
            Some(fill) => vec![attr_edit(&ix, xml, item, "FillColor", &fill)],
            None => attr_removal(&ix, xml, item, "FillColor").into_iter().collect(),
        };
        edits.extend(application::GRADIENT_GEOMETRY.iter().filter_map(|key| attr_removal(&ix, xml, item, key)));
        w.splice(&part, edits)?;
    }
    Ok(())
}
