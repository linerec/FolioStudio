//! Opacity and blend mode: the control bar and the Effects panel (next/oracle/scenarios/
//! basic-opacity.json, basic-effects.json). Each level is an element of the object:
//! `TransparencySetting` (the object), `StrokeTransparencySetting`, `FillTransparencySetting`,
//! `ContentTransparencySetting` (a text frame's text), and the placed graphic's own
//! `TransparencySetting` (the Graphic level). Its `BlendingSetting` carries `Opacity` only when
//! it is not 100 and `BlendMode` only when it is not Normal; with neither (and nothing else in
//! it) the element goes. New elements follow the object's `Properties`, in that order; the
//! effects (effects.rs) are further children of the same elements.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Object,
    Stroke,
    Fill,
    /// A text frame's text.
    Text,
    Graphic,
}

const ORDER: [&str; 4] = ["TransparencySetting", "StrokeTransparencySetting", "FillTransparencySetting", "ContentTransparencySetting"];
/// A level element's children, in the order InDesign writes them (as in its Preferences).
pub(super) const CHILDREN: [&str; 10] = ["BlendingSetting", "DropShadowSetting", "FeatherSetting", "InnerShadowSetting", "OuterGlowSetting",
    "InnerGlowSetting", "BevelAndEmbossSetting", "SatinSetting", "DirectionalFeatherSetting", "GradientFeatherSetting"];
pub(super) const MODES: [&str; 16] = ["Normal", "Multiply", "Screen", "Overlay", "SoftLight", "HardLight", "ColorDodge", "ColorBurn",
    "Darken", "Lighten", "Difference", "Exclusion", "Hue", "Saturation", "Color", "Luminosity"];

/// The element that holds `level` of item `i`: (its owner, its name, the object style category).
pub(super) fn level_element(ix: &Index, t: &str, i: usize, id: &str, level: Level) -> Result<(usize, &'static str, &'static str), EditError> {
    Ok(match level {
        Level::Graphic => {
            let g = ix.children(i).iter().copied().find(|&c| GRAPHICS.contains(&ix.local(t, c)))
                .ok_or_else(|| EditError::new("invalid", format!("{id} holds no placed graphic")))?;
            (g, "TransparencySetting", "")
        }
        Level::Text if ix.local(t, i) != "TextFrame" => return Err(EditError::new("invalid", format!("{id} is not a text frame"))),
        Level::Text => (i, "ContentTransparencySetting", "EnableContentTransparency"),
        Level::Object => (i, "TransparencySetting", "EnableTransparency"),
        Level::Stroke => (i, "StrokeTransparencySetting", "EnableStrokeTransparency"),
        Level::Fill => (i, "FillTransparencySetting", "EnableFillTransparency"),
    })
}

/// The attributes of `owner`'s `level` element's `child`, in their order (none: no child).
pub(super) fn child_attrs(ix: &Index, t: &str, owner: usize, level: &str, child: &str) -> Vec<(String, String)> {
    ix.children(owner).iter().copied().find(|&c| ix.local(t, c) == level)
        .and_then(|e| ix.children(e).iter().copied().find(|&c| ix.local(t, c) == child))
        .map(|c| ix.attrs(t, c).iter().map(|a| (t[a.name.0..a.name.1].to_owned(), crate::xml::unescape(&t[a.value.0..a.value.1]))).collect())
        .unwrap_or_default()
}

/// Group transparency switches (object level): Isolate Blending, Knockout Group.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Group {
    pub isolate: Option<bool>,
    pub knockout: Option<bool>,
}

/// `BlendingSetting` attributes in the order InDesign writes them (fx-isolate-knockout).
const BLENDING_ORDER: [&str; 4] = ["BlendMode", "Opacity", "KnockoutGroup", "IsolateBlending"];

pub(super) fn set_blending(w: &mut Work<'_>, ids: &[String], level: Level, opacity: Option<f64>, mode: Option<&str>, group: Group) -> Result<(), EditError> {
    if level != Level::Object && (group.isolate.is_some() || group.knockout.is_some()) {
        return Err(EditError::new("invalid", "isolate blending and knockout group belong to the object"));
    }
    if opacity.is_some_and(|o| !o.is_finite() || !(0.0..=100.0).contains(&o)) {
        return Err(EditError::new("invalid", "opacity is a percentage from 0 to 100"));
    }
    if mode.is_some_and(|m| !MODES.contains(&m)) {
        return Err(EditError::new("invalid", format!("unknown blend mode {}", mode.unwrap_or(""))));
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let (owner, name, _) = level_element(&ix, &t, i, id, level)?;
        if level != Level::Graphic && styled(w, &ix, &t, i, name) {
            return Err(EditError::new("unsupported", "the object style sets this transparency; changing it over the style is not built yet"));
        }
        // the BlendingSetting attributes as they will be, in their order
        let mut attrs = child_attrs(&ix, &t, owner, name, "BlendingSetting");
        let mut put = |key: &str, value: Option<String>| {
            let at = attrs.iter().position(|(k, _)| k == key);
            match (at, value) {
                (Some(k), Some(v)) => attrs[k].1 = v,
                (None, Some(v)) => {
                    // in InDesign's order among the ones there
                    let rank = |k: &str| BLENDING_ORDER.iter().position(|o| *o == k).unwrap_or(BLENDING_ORDER.len());
                    let k = attrs.iter().position(|(k, _)| rank(k) > rank(key)).unwrap_or(attrs.len());
                    attrs.insert(k, (key.to_owned(), v));
                }
                (Some(k), None) => { attrs.remove(k); }
                (None, None) => {}
            }
        };
        if let Some(o) = opacity {
            put("Opacity", ((o - 100.0).abs() > 1e-9).then(|| geom::num(o)));
        }
        if let Some(m) = mode {
            put("BlendMode", (m != "Normal").then(|| m.to_owned()));
        }
        // a group's switches: written when on (InDesign leaves the default out)
        if let Some(k) = group.knockout {
            put("KnockoutGroup", k.then(|| "true".to_owned()));
        }
        if let Some(i) = group.isolate {
            put("IsolateBlending", i.then(|| "true".to_owned()));
        }
        if let Some(e) = child_edit(&ix, &t, owner, name, "BlendingSetting", &attrs) {
            w.splice(&part, vec![e])?;
        }
    }
    Ok(())
}

/// The object's style (or a style it is based on) has a `name` with blending values.
fn styled(w: &Work<'_>, ix: &Index, t: &str, i: usize, name: &str) -> bool {
    ["Opacity", "BlendMode"].iter().any(|k| super::objectstyle::from_style(w, ix, t, i, &[name, "BlendingSetting"], k, "EnableTransparency").is_some())
}

/// The change that gives `owner`'s `level` element a `child` with `attrs` (in their order).
/// With no attributes the child goes, and the level element with it when nothing else is left;
/// `None` when nothing changes.
pub(super) fn child_edit(ix: &Index, t: &str, owner: usize, level: &str, child: &str, attrs: &[(String, String)]) -> Option<(std::ops::Range<usize>, String)> {
    let el = ix.children(owner).iter().copied().find(|&c| ix.local(t, c) == level);
    let cur = el.and_then(|e| ix.children(e).iter().copied().find(|&c| ix.local(t, c) == child));
    let a: String = attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", crate::xml::escape(v))).collect();
    let text = (!attrs.is_empty()).then(|| format!("<{child}{a} />"));
    let rank = |name: &str| CHILDREN.iter().position(|c| *c == name).unwrap_or(CHILDREN.len());
    match (el, cur) {
        (Some(e), Some(c)) => {
            let n = &ix.nodes[c];
            match text {
                // a child with its own children (stops) keeps them
                Some(_) if !n.empty => Some((n.start..n.open_end, format!("<{child}{a}>"))),
                Some(text) => (t[n.start..n.end] != text).then_some((n.start..n.end, text)),
                None if ix.children(e).len() > 1 || !ix.attrs(t, e).is_empty() => Some((element_span(ix, t, c), String::new())),
                None => Some((element_span(ix, t, e), String::new())),
            }
        }
        (Some(e), None) => {
            let text = text?;
            let n = &ix.nodes[e];
            let ind = indent_before(t, n.start);
            if n.empty {
                let open = t[n.start..n.end].trim_end_matches("/>").trim_end();
                return Some((n.start..n.end, format!("{open}>\n{ind}\t{text}\n{ind}</{level}>")));
            }
            // before the first child that comes after it, else at the end
            let next = ix.children(e).iter().copied().find(|&c| rank(ix.local(t, c)) > rank(child));
            let at = next.map_or(n.close_start, |c| ix.nodes[c].start);
            let line = t[..at].rfind('\n').map(|k| k + 1).filter(|&start| t[start..at].trim().is_empty()).unwrap_or(at);
            Some((line..line, format!("{ind}\t{text}\n")))
        }
        (None, _) => {
            let text = text?;
            // after the object's Properties and the transparency elements that come before it
            let rank = ORDER.iter().position(|o| *o == level).unwrap_or(0);
            let before: Vec<usize> = ix.children(owner).iter().copied().filter(|&c| {
                let l = ix.local(t, c);
                l == "Properties" || ORDER[..rank].contains(&l)
            }).collect();
            let ind = match before.last().or(ix.children(owner).first()) {
                Some(&c) => indent_before(t, ix.nodes[c].start).to_owned(),
                None => format!("{}\t", indent_before(t, ix.nodes[owner].start)),
            };
            let block = format!("{ind}<{level}>\n{ind}\t{text}\n{ind}</{level}>\n");
            let end = before.last().map_or(ix.nodes[owner].open_end, |&c| ix.nodes[c].end);
            let at = t[end..].find('\n').map(|k| end + k + 1).filter(|&next| t[end..next].trim().is_empty()).unwrap_or(end);
            Some((at..at, block))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_insertion_stays_inside_compact_item() {
        for contents in [
            "<Properties><PathGeometry /></Properties>",
            "<Properties><PathGeometry /></Properties><TransparencySetting />",
            "<Properties /><TransparencySetting><BlendingSetting Opacity=\"80\" /></TransparencySetting>",
            "<Properties /><TransparencySetting><DropShadowSetting Mode=\"Drop\" /></TransparencySetting>",
        ] {
            for separator in ["", "\n\t"] {
                let mut text = format!("<Spread>\n<Rectangle Self=\"drawn\">{contents}</Rectangle><Rectangle Self=\"neighbor\" />\n</Spread>")
                    .replace("><", &format!(">{separator}<"));
                let ix = Index::scan(&text).unwrap();
                let owner = ix.by_self(&text, "drawn").unwrap();
                let (range, replacement) = child_edit(&ix, &text, owner, "TransparencySetting", "GradientFeatherSetting", &[("Applied".into(), "true".into())]).unwrap();
                text.replace_range(range, &replacement);
                let ix = Index::scan(&text).unwrap();
                let owner = ix.by_self(&text, "drawn").unwrap();
                assert_eq!(child_attrs(&ix, &text, owner, "TransparencySetting", "GradientFeatherSetting"), vec![("Applied".into(), "true".into())]);
                assert!(ix.children(ix.by_self(&text, "neighbor").unwrap()).is_empty());
            }
        }
    }
}
