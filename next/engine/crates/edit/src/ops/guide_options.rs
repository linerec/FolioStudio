use super::*;
use std::collections::BTreeMap;

pub(super) fn defaults(w: &Work<'_>) -> Result<(String, String), EditError> {
    let t = w.text("Resources/Preferences.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let owner = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "GuidePreference");
    let threshold = owner.and_then(|i| ix.value(t, i, "RulerGuidesViewThreshold")).unwrap_or_else(|| "5".into());
    let color = owner.and_then(|i| ix.own_descendants(t, i, "RulerGuidesColor").first().copied())
        .map(|i| t[ix.nodes[i].start..ix.nodes[i].end].replace("RulerGuidesColor", "GuideColor"))
        .unwrap_or_else(|| "<GuideColor type=\"enumeration\">Cyan</GuideColor>".into());
    Ok((threshold, color))
}

pub(crate) fn set(w: &mut Work<'_>, ids: &[String], props: &BTreeMap<String, serde_json::Value>) -> Result<(), EditError> {
    for id in ids {
        for (key, value) in props {
            let (part, ix, i, t) = guides::guide(w, id)?;
            let invalid = || EditError::new("invalid", format!("invalid guide option {key}"));
            let (attr, text) = match key.as_str() {
                "viewThreshold" => ("ViewThreshold", geom::num(value.as_f64().filter(|v| v.is_finite() && (5.0..=4000.0).contains(v)).ok_or_else(invalid)?)),
                "locked" => ("Locked", bool_text(value.as_bool().ok_or_else(invalid)?)),
                "guideType" if value.as_str() == Some("Ruler") => ("GuideType", "Ruler".into()),
                "guideColor" => {
                    set_pref_colour(w, &part, &t, &ix, i, "GuideColor", value)?;
                    continue;
                }
                _ => return Err(invalid()),
            };
            w.splice(&part, vec![attr_edit(&ix, &t, i, attr, &text)])?;
        }
    }
    Ok(())
}

pub(crate) fn move_by(w: &mut Work<'_>, ids: &[String], delta: &[f64; 2]) -> Result<(), EditError> {
    if delta.iter().any(|v| !v.is_finite()) {
        return Err(EditError::new("invalid", "guide movement must be finite"));
    }
    for id in ids {
        let (part, ix, i, t) = guides::guide(w, id)?;
        if ix.attr(&t, i, "Locked") == Some("true") {
            return Err(EditError::new("locked", "the guide is locked"));
        }
        let axis = if ix.attr(&t, i, "Orientation") == Some("Vertical") { 0 } else { 1 };
        let location = ix.attr(&t, i, "Location").and_then(|v| v.parse::<f64>().ok()).ok_or_else(|| EditError::new("xml", "guide has no location"))?;
        w.splice(&part, vec![attr_edit(&ix, &t, i, "Location", &geom::num(location + delta[axis]))])?;
    }
    Ok(())
}
