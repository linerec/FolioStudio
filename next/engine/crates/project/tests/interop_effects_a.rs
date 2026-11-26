#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

const PART: &str = "Spreads/Spread_uf3.xml";

fn effects(doc: &folio_model::Document) -> Value {
    let item = doc.spreads.iter().flat_map(|s| &s.items).find(|i| i.id == "uff").unwrap();
    serde_json::to_value(&item.print.fx).unwrap()
}

fn check(case: &str, effect: &str, field: &str, element: &str, attr: &str, value: &str, expected: Value) {
    let source = support::oracle().join("fixtures/basic.idml");
    support::check(&format!("{effect}.{field}"), &source, PART,
        |_| {
            let xml = support::golden_part(&format!("basic/{case}"), PART);
            support::attribute(&xml, &format!("<{element} "), attr, value)
        },
        |doc| effects(doc)[effect][field].clone(), expected);
}

#[test]
fn effect_numeric_fields_survive_idml_export() {
    for (case, effect, element, fields) in [
        ("fx-feather-values", "feather", "FeatherSetting", &[
            ("width", "Width", 1.0), ("choke", "ChokeAmount", 0.01), ("noise", "Noise", 1.0),
        ][..]),
        ("fx-dir-feather-values", "directional_feather", "DirectionalFeatherSetting", &[
            ("left", "LeftWidth", 1.0), ("right", "RightWidth", 1.0), ("top", "TopWidth", 1.0),
            ("bottom", "BottomWidth", 1.0), ("choke", "ChokeAmount", 0.01), ("angle", "Angle", 1.0), ("noise", "Noise", 1.0),
        ][..]),
        ("fx-inner-glow-values", "inner_glow", "InnerGlowSetting", &[
            ("size", "Size", 1.0), ("opacity", "Opacity", 0.01), ("choke", "Spread", 0.01), ("noise", "Noise", 1.0),
        ][..]),
        ("fx-bevel-values", "bevel", "BevelAndEmbossSetting", &[
            ("size", "Size", 1.0), ("soften", "Soften", 1.0), ("depth", "Depth", 0.01),
            ("angle", "Angle", 1.0), ("altitude", "Altitude", 1.0),
            ("highlight_opacity", "HighlightOpacity", 0.01), ("shadow_opacity", "ShadowOpacity", 0.01),
        ][..]),
        ("fx-grad-feather-values", "gradient_feather", "GradientFeatherSetting", &[
            ("angle", "Angle", 1.0), ("length", "Length", 1.0),
        ][..]),
    ] {
        for (field, attr, scale) in fields {
            for value in [25.0, 50.0] {
                check(case, effect, field, element, attr, &value.to_string(), json!(value * scale));
            }
        }
    }
}

#[test]
fn effect_enum_fields_survive_idml_export() {
    for (case, effect, field, element, attr, values) in [
        ("fx-feather-values", "feather", "corners", "FeatherSetting", "CornerType",
            &[("Sharp", "Sharp"), ("Rounded", "Rounded"), ("Diffusion", "Diffusion")][..]),
        ("fx-dir-feather-values", "directional_feather", "follow", "DirectionalFeatherSetting", "FollowShapeMode",
            &[("FirstEdge", "FirstEdge"), ("LeadingEdge", "LeadingEdge"), ("AllEdges", "AllEdges")][..]),
        ("fx-bevel-values", "bevel", "style", "BevelAndEmbossSetting", "Style",
            &[("InnerBevel", "Inner"), ("OuterBevel", "Outer"), ("Emboss", "Emboss"), ("PillowEmboss", "Pillow")][..]),
        ("fx-bevel-values", "bevel", "technique", "BevelAndEmbossSetting", "Technique",
            &[("Smooth", "Smooth"), ("ChiselHard", "ChiselHard"), ("ChiselSoft", "ChiselSoft")][..]),
        ("fx-grad-feather-values", "gradient_feather", "kind", "GradientFeatherSetting", "Type",
            &[("Linear", "Linear"), ("Radial", "Radial")][..]),
    ] {
        for (value, expected) in values {
            check(case, effect, field, element, attr, value, json!(expected));
        }
    }
    for (case, effect, field, element, attr, yes, no) in [
        ("fx-inner-glow-values", "inner_glow", "center", "InnerGlowSetting", "Source", "CenterSourced", "EdgeSourced"),
        ("fx-inner-glow-values", "inner_glow", "precise", "InnerGlowSetting", "Technique", "Precise", "Softer"),
        ("fx-bevel-values", "bevel", "up", "BevelAndEmbossSetting", "Direction", "Up", "Down"),
    ] {
        check(case, effect, field, element, attr, yes, json!(true));
        check(case, effect, field, element, attr, no, json!(false));
    }
}

#[test]
fn gradient_feather_start_and_absent_geometry_survive_idml_export() {
    for (value, xy) in [("10 20", [10.0, 20.0]), ("35 45", [35.0, 45.0])] {
        check("fx-grad-feather-values", "gradient_feather", "start", "GradientFeatherSetting", "GradientStart", value,
            json!({"x": xy[0], "y": xy[1]}));
    }
    // InDesign's saved effect omits Length/GradientStart: the model must retain None.
    for field in ["length", "start"] {
        support::check(&format!("GradientFeather.{field} absent"), &support::oracle().join("fixtures/basic.idml"), PART,
            |_| support::golden_part("basic/fx-grad-feather-values", PART),
            |doc| effects(doc)["gradient_feather"][field].clone(), Value::Null);
    }
}

#[test]
fn effect_blend_modes_survive_idml_export() {
    for (case, effect, field, element, attr) in [
        ("fx-inner-glow-values", "inner_glow", "blend", "InnerGlowSetting", "BlendMode"),
        ("fx-bevel-values", "bevel", "highlight_blend", "BevelAndEmbossSetting", "HighlightBlendMode"),
        ("fx-bevel-values", "bevel", "shadow_blend", "BevelAndEmbossSetting", "ShadowBlendMode"),
    ] {
        for mode in ["Normal", "Multiply", "Screen", "Overlay", "SoftLight", "HardLight", "ColorDodge", "ColorBurn",
            "Darken", "Lighten", "Difference", "Exclusion", "Hue", "Saturation", "Color", "Luminosity"] {
            check(case, effect, field, element, attr, mode, json!(mode));
        }
    }
}

#[test]
fn bevel_global_light_overrides_local_angles_and_survives_export() {
    for global in [false, true] {
        support::check("BevelEmboss global light", &support::oracle().join("fixtures/basic.idml"), PART,
            |_| {
                let mut xml = support::golden_part("basic/fx-bevel-values", PART);
                for (attr, value) in [("UseGlobalLight", if global { "true" } else { "false" }), ("Angle", "45"), ("Altitude", "60")] {
                    xml = support::attribute(&xml, "<BevelAndEmbossSetting ", attr, value);
                }
                xml
            },
            |doc| json!([effects(doc)["bevel"]["angle"], effects(doc)["bevel"]["altitude"]]),
            if global { json!([120.0, 30.0]) } else { json!([45.0, 60.0]) });
    }
}
