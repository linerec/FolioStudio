use super::*;

pub(super) fn apply(w: &mut Work<'_>, id: &str, style: &str, clear: bool, through_group: bool) -> Result<(), EditError> {
    for (level, category) in [("TransparencySetting", "ObjectStyleObjectEffectsCategorySettings"),
        ("StrokeTransparencySetting", "ObjectStyleStrokeEffectsCategorySettings"),
        ("FillTransparencySetting", "ObjectStyleFillEffectsCategorySettings"),
        ("ContentTransparencySetting", "ObjectStyleContentEffectsCategorySettings")] {
        for child in super::super::blending::CHILDREN {
            if through_group && child != "BlendingSetting" { continue; }
            let (part, ix, i) = w.find(id)?;
            let t = w.text(&part)?.to_owned();
            let Some(styles) = Styles::load(w) else { continue };
            let Some(&s) = styles.chain(style).first() else { continue };
            let switch = match child {
                "BlendingSetting" => "EnableTransparency".to_owned(),
                "BevelAndEmbossSetting" => "EnableBevelEmboss".to_owned(),
                _ => format!("Enable{}", child.trim_end_matches("Setting")),
            };
            let enabled = styles.ix.path(styles.t, s, &[category])
                .is_none_or(|n| styles.ix.attr(styles.t, n, &switch) != Some("false"));
            if !enabled { continue; }
            let mut attrs = super::super::blending::child_attrs(&ix, &t, i, level, child);
            if clear { attrs.clear(); }
            if through_group {
                let old = ix.attr(&t, i, "AppliedObjectStyle").unwrap_or(NONE);
                for (key, value) in super::super::blending::child_attrs(&styles.ix, styles.t, s, level, child) {
                    if !attrs.iter().any(|(own, _)| *own == key)
                        && styles.value(old, &[level, child], &key, "").as_ref() != Some(&value)
                    {
                        attrs.push((key, value));
                    }
                }
            }
            if child == "GradientFeatherSetting"
                && styles.value(style, &[level, child], "Applied", "").as_deref() == Some("true")
                && let Some(bounds) = own_path(&ix, &t, i).and_then(|p| geom::path_bounds(&p.subpaths.iter().map(|(_, q, c)| (q.clone(), *c)).collect::<Vec<_>>())) {
                let radial = styles.value(style, &[level, child], "Type", "").as_deref() == Some("Radial");
                let start = if radial { ((bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0) } else { (bounds[0], bounds[3]) };
                for (key, value) in [("Length", geom::num((bounds[2] - bounds[0]) / if radial { 2.0 } else { 1.0 })), ("GradientStart", geom::pair(start))] {
                    if clear || !attrs.iter().any(|(k, _)| k == key) { attrs.push((key.into(), value)); }
                }
            }
            if let Some(edit) = super::super::blending::child_edit(&ix, &t, i, level, child, &attrs) {
                w.splice(&part, vec![edit])?;
            }
        }
    }
    Ok(())
}
