#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{attr, by_id, check, fixture, roundtrip};

const GRAPHIC: &str = "Resources/Graphic.xml";
const SPREAD: &str = "Spreads/Spread_u112.xml";

#[test]
fn tint_base_and_percentage_survive_export() {
    for (base, tint) in [("Color/Blue", 25), ("Color/Orange", 75)] {
        let mut parts = fixture("color-gradient");
        let xml = parts.get_mut(GRAPHIC).unwrap();
        *xml = xml.replace("</idPkg:Graphic>", &format!(
            "<Tint Self=\"Tint/Interop\" Name=\"Interop\" BaseColor=\"{base}\" TintValue=\"{tint}\" /></idPkg:Graphic>"));
        let (before, after, written) = roundtrip("color-gradient", &parts);
        let pointer = "/swatches/Tint~1Interop";
        check(&before, &after, &format!("{pointer}/base"), json!(base));
        check(&before, &after, &format!("{pointer}/tint"), json!(tint as f64));
        assert_eq!(before.pointer(pointer), after.pointer(pointer));
        assert_eq!(written[GRAPHIC], parts[GRAPHIC]);
    }
}

#[test]
fn item_paints_preserve_none_swatch_tint_and_gradient_references() {
    for (color, tint, expected) in [
        ("Swatch/None", "100", json!("None")),
        ("Color/Blue", "25", json!({"Swatch":{"id":"Color/Blue","tint":25.0}})),
        ("Color/Orange", "75", json!({"Swatch":{"id":"Color/Orange","tint":75.0}})),
        ("Gradient/Ramp", "100", json!({"Gradient":{"id":"Gradient/Ramp","fallback_rgb":[128,128,128]}})),
    ] {
        for (color_key, tint_key, pointer) in [
            ("FillColor", "FillTint", "/fill"),
            ("StrokeColor", "StrokeTint", "/stroke/paint"),
            ("GapColor", "GapTint", "/stroke/gap"),
        ] {
            let mut parts = fixture("color-gradient");
            let xml = parts.get_mut(SPREAD).unwrap();
            attr(xml, "Rectangle", 0, color_key, color);
            attr(xml, "Rectangle", 0, tint_key, tint);
            let (before, after, written) = roundtrip("color-gradient", &parts);
            check(by_id(&before, "u12e"), by_id(&after, "u12e"), pointer, expected.clone());
            assert_eq!(written[SPREAD], parts[SPREAD]);
        }
    }
}

#[test]
fn display_color_is_unmanaged_and_gradient_fallback_is_stop_average() {
    for (channels, expected) in [("10 20 30", [10,20,30]), ("100 140 180", [100,140,180])] {
        let mut parts = fixture("color-gradient");
        let xml = parts.get_mut(GRAPHIC).unwrap();
        attr(xml, "Color", 1, "Space", "RGB");
        attr(xml, "Color", 1, "ColorValue", channels);
        attr(xml, "GradientStop", 0, "StopColor", "Color/Blue");
        attr(parts.get_mut(SPREAD).unwrap(), "Rectangle", 0, "FillColor", "Gradient/Ramp");
        let (before, after, written) = roundtrip("color-gradient", &parts);
        check(&before, &after, "/swatches/Color~1Blue/display_rgb", json!(expected));
        check(by_id(&before,"u12e"), by_id(&after,"u12e"), "/fill/Gradient/fallback_rgb",
            json!(expected.map(|v| v / 2)));
        check(&before, &after, "/swatches/Swatch~1None/model", json!("None"));
        assert_eq!(written[GRAPHIC], parts[GRAPHIC]);
    }
}
