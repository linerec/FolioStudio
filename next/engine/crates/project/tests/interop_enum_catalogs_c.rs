#[path = "support/interop.rs"]
mod support;
use folio_model::{
    BottomOrigin, ColorModel, ColorSpace, Justification, LineCap, LineJoin, VerticalJustification,
};
use serde_json::{Value, json};

#[test]
fn every_scalar_enum_variant_has_an_executed_idml_mutation_case() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("support/interop_attributes.json")).unwrap();
    let catalogs = [
        (
            "ColorSpace",
            json!([
                ColorSpace::Cmyk,
                ColorSpace::Rgb,
                ColorSpace::Lab,
                ColorSpace::Gray
            ]),
        ),
        (
            "ColorModel",
            json!([
                ColorModel::Process,
                ColorModel::Spot,
                ColorModel::Registration,
                ColorModel::Paper,
                ColorModel::Mixed
            ]),
        ),
        (
            "LineCap",
            json!([LineCap::Butt, LineCap::Round, LineCap::Projecting]),
        ),
        (
            "LineJoin",
            json!([LineJoin::Miter, LineJoin::Round, LineJoin::Bevel]),
        ),
        (
            "BottomOrigin",
            json!([BottomOrigin::Descent, BottomOrigin::Baseline]),
        ),
        (
            "VerticalJustification",
            json!([
                VerticalJustification::Top,
                VerticalJustification::Center,
                VerticalJustification::Bottom,
                VerticalJustification::Justify
            ]),
        ),
        (
            "Justification",
            json!([
                Justification::LeftAlign,
                Justification::CenterAlign,
                Justification::RightAlign,
                Justification::LeftJustified,
                Justification::CenterJustified,
                Justification::RightJustified,
                Justification::FullyJustified,
                Justification::ToBindingSide,
                Justification::AwayFromBindingSide
            ]),
        ),
    ];
    for (name, variants) in catalogs {
        for variant in variants.as_array().unwrap() {
            let item = format!("{name}::{}", variant.as_str().unwrap());
            let case = cases
                .iter()
                .find(|c| c["items"].as_array().unwrap().contains(&json!(item)))
                .unwrap_or_else(|| panic!("missing {item}"));
            assert_eq!(&case["expected"], variant);
            let text = |key| case[key].as_str().unwrap();
            let mut parts = support::fixture(text("fixture"));
            if case["overlay"] == true {
                parts.insert(
                    text("part").into(),
                    std::fs::read_to_string(
                        support::root()
                            .join("oracle/goldens")
                            .join(text("golden"))
                            .join("after")
                            .join(text("part")),
                    )
                    .unwrap(),
                );
            }
            for setup in case["setup"].as_array().into_iter().flatten() {
                support::attr(
                    parts.get_mut(text("part")).unwrap(),
                    setup[0].as_str().unwrap(),
                    0,
                    setup[1].as_str().unwrap(),
                    setup[2].as_str().unwrap(),
                );
            }
            support::attr(
                parts.get_mut(text("part")).unwrap(),
                text("tag"),
                case["index"].as_u64().unwrap_or(0) as usize,
                text("key"),
                text("value"),
            );
            let (before, after, written) = support::roundtrip(text("fixture"), &parts);
            let left = case["id"]
                .as_str()
                .map_or(&before, |id| support::by_id(&before, id));
            let right = case["id"]
                .as_str()
                .map_or(&after, |id| support::by_id(&after, id));
            support::check(left, right, text("pointer"), variant.clone());
            assert_eq!(written[text("part")], parts[text("part")]);
        }
    }
    // ColorModel::None is represented by the special None swatch rather than Color/@Model.
    let parts = support::fixture("basic");
    let (before, after, _) = support::roundtrip("basic", &parts);
    support::check(
        &before,
        &after,
        "/swatches/Swatch~1None/model",
        json!(ColorModel::None),
    );
}
