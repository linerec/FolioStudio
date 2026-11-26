use super::{FontStore, Paint, ParagraphRule, RuleWidth, document, is_rect, layout_document, lines, para, story, text_frame};
use serde_json::Value;

#[cfg_attr(not(has_reference_vectors), ignore = "optional reference vectors are missing")]
#[test]
fn rule_vectors_preserve_span_and_paint() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../cleanroom/vectors/B4/paragraph-rules-oracle.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let vectors: Value = serde_json::from_slice(&bytes).unwrap();
    let cases = vectors["cases"].as_array().unwrap();
    let text_paint = Paint::Swatch { id: "Color/Example".into(), tint: 73.0 };
    for (case_id, prefix, expected) in [
        ("pardeco-rule-above", "ruleAbove", (7.0, 2.0, 89.0, 4.0)),
        ("pardeco-rule-below-text", "ruleBelow", (13.0, 13.0, 26.0, 14.5)),
        ("pardeco2-rule-text-color", "ruleAbove", (0.0, 7.0, 100.0, 8.0)),
        ("pardeco2-rule-negative-indents", "ruleAbove", (-9.0, 7.0, 108.0, 8.0)),
    ] {
        let case = cases.iter().find(|case| {
            case["id"].as_str().unwrap().split_once('/').unwrap().1 == case_id
        }).unwrap();
        let mut props = serde_json::Map::new();
        for step in case["input"]["steps"].as_array().unwrap() {
            props.extend(step["props"].as_object().unwrap().clone());
        }
        assert_eq!(props[prefix], true, "{case_id}");
        let default = ParagraphRule::default();
        let number = |name: &str, fallback: f64| {
            props.get(&format!("{prefix}{name}")).and_then(Value::as_f64).unwrap_or(fallback)
        };
        let paint = match props.get(&format!("{prefix}Color")).and_then(Value::as_str) {
            Some("Text Color") => None,
            id => Some(Paint::Swatch {
                id: id.unwrap_or("Color/Black").into(),
                tint: number("Tint", 100.0),
            }),
        };
        let rule = ParagraphRule {
            paint: paint.clone(),
            text_tint: paint.is_none().then(|| number("Tint", 100.0)),
            weight: number("LineWeight", 1.0),
            offset: number("Offset", default.offset),
            left_indent: number("LeftIndent", default.left_indent),
            right_indent: number("RightIndent", default.right_indent),
            width: match props.get(&format!("{prefix}Width")).and_then(Value::as_str) {
                Some("TEXT_WIDTH") => RuleWidth::Text,
                _ => RuleWidth::Column,
            },
            overprint: props.get(&format!("{prefix}Overprint")).and_then(Value::as_bool).unwrap_or(false),
            ..default
        };
        let overprint = rule.overprint;
        let mut p = para("abcd");
        p.attrs.left_indent = 10.0;
        p.runs[0].attrs.fill = text_paint.clone();
        if prefix == "ruleAbove" {
            p.attrs.rule_above = Some(rule);
        } else {
            p.attrs.rule_below = Some(rule);
        }
        let doc = document(vec![text_frame("f", "s", 100.0, 200.0, |_| {})], vec![story("s", vec![p])]);
        let layout = layout_document(&doc, &FontStore::empty());
        let [shape] = lines(&layout, "f")[0].shapes.as_slice() else {
            panic!("{case_id}: expected one rule shape");
        };
        let (left, top, right, bottom) = expected;
        assert!(is_rect(shape, left, top, right, bottom), "{case_id}: {:?}", shape.bounds());
        assert_eq!(shape.paint, paint.unwrap_or_else(|| text_paint.clone()), "{case_id}");
        assert_eq!(shape.overprint, overprint, "{case_id}");
    }
}
