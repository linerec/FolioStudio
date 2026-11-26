#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{by_id, fixture, roundtrip};

const PART: &str = "Spreads/Spread_uf3.xml";

fn setting(xml: &mut String, tag: &str, inner: &str) {
    let start = xml.find("<Rectangle Self=\"uff\"").unwrap();
    let end = start + xml[start..].find("</Rectangle>").unwrap();
    let mut item = xml[start..end].to_owned();
    if let Some(at) = item.find(&format!("<{tag}")) {
        let head_end = at + item[at..].find('>').unwrap() + 1;
        let stop = if item[..head_end].ends_with("/>") { head_end }
            else { head_end + item[head_end..].find(&format!("</{tag}>")).unwrap() + tag.len() + 3 };
        item.replace_range(at..stop, "");
    }
    item.push_str(&format!("<{tag}>{inner}</{tag}>"));
    xml.replace_range(start..end, &item);
}

#[test]
fn effect_presence_is_preserved_separately_at_all_four_levels() {
    for (tag, field, key) in [
        ("DropShadowSetting", "drop_shadow", "Mode"), ("FeatherSetting", "feather", "Mode"),
        ("InnerShadowSetting", "inner_shadow", "Applied"), ("OuterGlowSetting", "outer_glow", "Applied"),
        ("InnerGlowSetting", "inner_glow", "Applied"), ("BevelAndEmbossSetting", "bevel", "Applied"),
        ("SatinSetting", "satin", "Applied"), ("DirectionalFeatherSetting", "directional_feather", "Applied"),
        ("GradientFeatherSetting", "gradient_feather", "Applied"),
    ] {
        for (level, pointer) in [("", "/print/fx"), ("Fill", "/print/level_fx/fill"),
            ("Stroke", "/print/level_fx/stroke"), ("Content", "/print/level_fx/content")] {
            for enabled in [false, true] {
                let mut parts = fixture("basic");
                let value = match (key, enabled) { ("Mode", true) => "Standard", ("Mode", false) => "None", (_, true) => "true", _ => "false" };
                setting(parts.get_mut(PART).unwrap(), &format!("{level}TransparencySetting"), &format!("<{tag} {key}=\"{value}\" />"));
                let (before, after, written) = roundtrip("basic", &parts);
                let b = by_id(&before, "uff");
                let a = by_id(&after, "uff");
                let path = format!("{pointer}/{field}");
                assert_eq!(!b.pointer(&path).unwrap().is_null(), enabled, "{level}/{field}");
                assert_eq!(b.pointer(&path), a.pointer(&path));
                assert_eq!(b["print"]["effects"], a["print"]["effects"]);
                let effect_name = format!("{tag} ({})", if level.is_empty() { "object".into() } else { level.to_lowercase() });
                assert_eq!(b["print"]["effects"].as_array().unwrap().contains(&json!(effect_name)), enabled);
                assert_eq!(written[PART], parts[PART]);
            }
        }
    }
}

#[test]
fn blend_and_opacity_preserve_object_and_level_scope() {
    for (level, pointer) in [("", ""), ("Fill", "/print/fill_blend"), ("Stroke", "/print/stroke_blend"), ("Content", "/print/content_blend")] {
        for (opacity, mode) in [(25, "Multiply"), (75, "Screen"), (100, "Normal")] {
            let mut parts = fixture("basic");
            setting(parts.get_mut(PART).unwrap(), &format!("{level}TransparencySetting"),
                &format!("<BlendingSetting Opacity=\"{opacity}\" BlendMode=\"{mode}\" />"));
            let (before, after, written) = roundtrip("basic", &parts);
            let b = by_id(&before, "uff");
            let a = by_id(&after, "uff");
            if level.is_empty() {
                assert_eq!(b["opacity"], json!(opacity as f64 / 100.0));
                assert_eq!(b["blend_mode"], json!(mode));
                assert_eq!(b["opacity"], a["opacity"]);
                assert_eq!(b["blend_mode"], a["blend_mode"]);
            } else {
                let expected = if opacity == 100 { json!(null) } else { json!([opacity as f64 / 100.0, mode]) };
                assert_eq!(b.pointer(pointer), Some(&expected));
                assert_eq!(a.pointer(pointer), Some(&expected));
            }
            assert_eq!(written[PART], parts[PART]);
        }
    }
}
