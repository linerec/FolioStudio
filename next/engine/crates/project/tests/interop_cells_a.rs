#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

fn stroke(doc: &folio_model::Document) -> Value {
    let table = doc.stories["ued"].paragraphs.iter().flat_map(|p| &p.runs).flat_map(|r| &r.tables).next().unwrap();
    serde_json::to_value(&table.cells[0].edges[0]).unwrap()
}

#[test]
fn cell_top_stroke_scalar_fields_survive_idml_export() {
    for (field, suffix, value, expected) in [
        ("weight", "Weight", "0.5", json!(0.5)), ("weight", "Weight", "3.5", json!(3.5)),
        ("stroke_type", "Type", "StrokeStyle/$ID/Solid", json!("Solid")),
        ("stroke_type", "Type", "StrokeStyle/$ID/Dashed", json!("Dashed")),
        ("priority", "Priority", "2", json!(2)), ("priority", "Priority", "7", json!(7)),
        ("overprint", "Overprint", "false", json!(false)), ("overprint", "Overprint", "true", json!(true)),
        ("gap_overprint", "GapOverprint", "false", json!(false)), ("gap_overprint", "GapOverprint", "true", json!(true)),
    ] {
        support::check(&format!("CellStroke.{field}"), &support::oracle().join("fixtures/tables.idml"), "Stories/Story_ued.xml",
            |xml| {
                let xml = support::attribute(xml, "<Cell ", "TopEdgeStrokeWeight", "2");
                let xml = support::attribute(&xml, "<Cell ", "TopEdgeStrokeColor", "Color/Black");
                let xml = support::attribute(&xml, "<Cell ", "TopEdgeStrokeType", "StrokeStyle/$ID/Dashed");
                support::attribute(&xml, "<Cell ", &format!("TopEdgeStroke{suffix}"), value)
            }, |doc| stroke(doc)[field].clone(), expected);
    }
}
