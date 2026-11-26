use super::{EditError, Index, Values, attributes, attrs_text, find_tag, geom, properties};

pub(super) fn text_frame_preferences(
    ix: &Index,
    t: &str,
    base: &Values,
    width: f64,
    uniform_inset: bool,
    full: bool,
) -> Result<(String, f64), EditError> {
    build(ix, t, base, Values::new(), width, uniform_inset, full)
}

pub(super) fn convert(
    ix: &Index,
    t: &str,
    base: &Values,
    style_properties: Values,
    width: f64,
    uniform_inset: bool,
) -> Result<(String, f64), EditError> {
    build(ix, t, base, style_properties, width, uniform_inset, true)
}

fn build(
    ix: &Index,
    t: &str,
    base: &Values,
    style_properties: Values,
    width: f64,
    uniform_inset: bool,
    full: bool,
) -> Result<(String, f64), EditError> {
    let at = find_tag(ix, t, "TextFramePreference")?;
    let mut attrs = attributes(ix, t, at);
    validate_defaults(&attrs, full)?;
    let mut props = properties(ix, t, at);
    if full {
        read_insets(props.get("InsetSpacing"), uniform_inset)?;
        attrs.extend(base.clone());
        props.extend(style_properties);
    }
    let insets = read_insets(props.get("InsetSpacing"), uniform_inset)?;
    let horizontal = insets[1] + insets[3];
    let fixed = enabled(&attrs, "UseFixedColumnWidth");
    let gutter = finite(&attrs, "TextColumnGutter").unwrap_or(12.0);
    let (count, column, width) = if fixed {
        let column = finite(&attrs, "TextColumnFixedWidth")
            .filter(|&value| value > 0.0)
            .ok_or_else(|| EditError::new("invalid", "fixed column width must be positive"))?;
        let count = ((width - horizontal + gutter) / (column + gutter))
            .ceil()
            .max(1.0) as u32;
        (
            count,
            column,
            horizontal + f64::from(count) * column + f64::from(count - 1) * gutter,
        )
    } else {
        let count = attrs
            .get("TextColumnCount")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1)
            .max(1);
        (
            count,
            (width - horizontal - f64::from(count - 1) * gutter) / f64::from(count),
            width,
        )
    };
    if !full {
        attrs.retain(|key, value| base.get(key) != Some(value));
    }
    if fixed && count == 1 {
        attrs.remove("TextColumnCount");
    } else {
        attrs.insert("TextColumnCount".into(), count.to_string());
    }
    attrs.insert("TextColumnFixedWidth".into(), geom::num(column));
    attrs.insert("TextColumnMaxWidth".into(), "0".into());
    props.insert("InsetSpacing".into(), inset_xml(insets, uniform_inset));
    let props: String = props.into_values().collect();
    Ok((
        format!(
            "<TextFramePreference{}><Properties>{props}</Properties></TextFramePreference>",
            attrs_text(&attrs)
        ),
        width,
    ))
}

fn validate_defaults(attrs: &Values, full: bool) -> Result<(), EditError> {
    if enabled(attrs, "UseFlexibleColumnWidth") {
        return Err(EditError::new("unsupported", "flexible column width"));
    }
    let auto_size = attrs
        .get("AutoSizingType")
        .is_some_and(|value| !matches!(value.as_str(), "" | "Off"));
    if full && (enabled(attrs, "UseFixedColumnWidth") || auto_size) {
        return Err(EditError::new(
            "unsupported",
            "fixed columns or auto-size in shape conversion defaults",
        ));
    }
    Ok(())
}

fn enabled(attrs: &Values, key: &str) -> bool {
    attrs.get(key).is_some_and(|value| value == "true")
}

fn finite(attrs: &Values, key: &str) -> Option<f64> {
    attrs
        .get(key)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn read_insets(xml: Option<&String>, uniform: bool) -> Result<[f64; 4], EditError> {
    let Some(xml) = xml else { return Ok([0.0; 4]) };
    let ix = Index::scan(xml).map_err(|e| EditError::new("xml", e))?;
    let root = find_tag(&ix, xml, "InsetSpacing")?;
    let value = |at: usize| {
        let node = &ix.nodes[at];
        xml[node.open_end..node.close_start]
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| EditError::new("xml", "invalid inset spacing"))
    };
    if ix.attr(xml, root, "type") != Some("list") {
        return Ok([value(root)?; 4]);
    }
    let [top, left, bottom, right] = ix.children(root) else {
        return Err(EditError::new("xml", "expected four inset values"));
    };
    let insets = [value(*top)?, value(*left)?, value(*bottom)?, value(*right)?];
    if uniform && insets.iter().any(|&v| v != 0.0) {
        return Err(EditError::new(
            "unsupported",
            "list insets on a nonrectangular shape",
        ));
    }
    Ok(insets)
}

fn inset_xml(insets: [f64; 4], uniform: bool) -> String {
    if uniform {
        return format!(
            "<InsetSpacing type=\"unit\">{}</InsetSpacing>",
            geom::num(insets[0])
        );
    }
    let items: String = insets
        .into_iter()
        .map(|value| format!("<ListItem type=\"unit\">{}</ListItem>", geom::num(value)))
        .collect();
    format!("<InsetSpacing type=\"list\">{items}</InsetSpacing>")
}
