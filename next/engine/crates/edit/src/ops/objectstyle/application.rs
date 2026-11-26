use super::*;

pub(super) type Edits = Vec<(std::ops::Range<usize>, String)>;

pub(super) const GRADIENT_GEOMETRY: [&str; 4] = [
    "GradientFillStart", "GradientFillLength", "GradientStrokeStart", "GradientStrokeLength",
];

pub(super) fn apply_core(
    w: &mut Work<'_>,
    ids: &[String],
    style: &str,
    clear: bool,
    through_group: bool,
    reset_gradient: bool,
) -> Result<(), EditError> {
    if Styles::load(w).is_none_or(|styles| styles.chain(style).is_empty()) {
        return Err(EditError::new("notFound", format!("object style {style}")));
    }
    let clear = clear || style == NONE;
    for id in ids {
        let (part, ix, item) = w.find(id)?;
        let xml = w.text(&part)?;
        let kind = ix.local(xml, item);
        if !GRAPHICS.contains(&kind) {
            check_item(&ix, xml, item, id)?;
        }
        super::super::textpath::unlocked(w, &ix, xml, item)?;
        if kind == "Group" {
            group::apply(w, id, style, clear, through_group)?;
        } else {
            let is_frame = kind == "TextFrame";
            let story = is_frame.then(|| ix.value(xml, item, "ParentStory")).flatten();
            let threaded = ["PreviousTextFrame", "NextTextFrame"].iter().any(|name| {
                ix.attr(xml, item, name).is_some_and(|value| value != "n")
            });
            let old = ix.attr(xml, item, "AppliedObjectStyle").unwrap_or(NONE);
            let mut edits = settings(w, &ix, xml, item, old, style, clear, through_group);
            if !through_group {
                edits.push(attr_edit(&ix, xml, item, "AppliedObjectStyle", style));
            }
            if clear && style != NONE && reset_gradient {
                edits.extend(gradient_reset(w, &ix, xml, item, style, through_group));
            }
            if is_frame {
                edits.extend(frame_settings(w, &ix, xml, item, old, style, clear, through_group));
            }
            if clear && !through_group && Styles::load(w).is_some_and(|s| s.enabled(style, "EnableFrameFittingOptions")) {
                if let Some(&option) = ix.named(xml, item, "FrameFittingOption").first() {
                    edits.push((element_span(&ix, xml, option), String::new()));
                }
            }
            w.splice(&part, edits)?;
            effects::apply(w, id, style, clear, through_group)?;
            if is_frame {
                update_columns(w, id)?;
            }
            if let Some(story) = story {
                if !through_group {
                    paragraph::apply(w, &story, style, threaded)?;
                }
                w.recompose(&story);
            }
        }
        if !through_group {
            group::refresh_parent(w, id)?;
        }
    }
    Ok(())
}

pub(super) fn settings(
    w: &Work<'_>,
    ix: &Index,
    xml: &str,
    item: usize,
    old: &str,
    new: &str,
    clear: bool,
    through_group: bool,
) -> Edits {
    let Some(styles) = Styles::load(w) else { return Vec::new() };
    let kind = ix.local(xml, item);
    let text_frame_settings = styles.chain(new).first().is_some_and(|&style| {
        styles.ix.attr(styles.t, style, "EnableTextFrameGeneralOptions") == Some("true")
    });
    let mut names: Vec<_> = styles.attribute_names(new).into_iter().collect();
    names.sort();
    let mut edits = Vec::new();
    for name in &names {
        if not_a_setting(name) || GRADIENT_GEOMETRY.contains(&name.as_str()) {
            continue;
        }
        let enable = setting_category(name);
        if !styles.enabled(new, enable) {
            continue;
        }
        let own = ix.value(xml, item, name);
        let value = styles.value(new, &[], name, enable);
        if through_group {
            let paint_or_stroke = matches!(enable, "EnableFill" | "EnableStroke");
            if kind == "Group" && enable != "EnableStrokeAndCornerOptions" {
                continue;
            }
            if kind != "TextFrame" && text_frame_settings && paint_or_stroke {
                if clear {
                    edits.extend(attr_removal(ix, xml, item, name));
                }
                continue;
            }
            if !clear && own.is_some() && own != value {
                continue;
            }
            let inherited = styles.value(old, &[], name, enable)
                .or_else(|| styles.value(NONE, &[], name, enable));
            let implicit_default = inherited.is_none() && matches!(value.as_deref(), Some("false" | "-1"));
            if value == inherited || implicit_default {
                edits.extend(attr_removal(ix, xml, item, name));
            } else if let Some(value) = value {
                edits.push(attr_edit(ix, xml, item, name, &value));
            }
        } else if own.is_some() && (clear || own == value) {
            edits.extend(attr_removal(ix, xml, item, name));
        }
    }
    edits
}

fn update_columns(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, ix, item) = w.find(id)?;
    let xml = w.text(&part)?;
    let bounds = own_path(&ix, xml, item).and_then(|path| {
        let shapes: Vec<_> = path.subpaths.into_iter().map(|(_, points, closed)| (points, closed)).collect();
        geom::path_bounds(&shapes)
    });
    if let Some([left, _, right, _]) = bounds
        && let Some(edit) = super::super::textframe::column_width(w, &ix, xml, item, right - left)
    {
        w.splice(&part, vec![edit])?;
    }
    Ok(())
}
