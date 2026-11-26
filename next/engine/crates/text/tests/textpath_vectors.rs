use folio_model::textpath::{
    OffsetJoin, PathAlignment, PathEffect, PathMeasure, PathTextAlignment,
};
use folio_model::{Affine, LineJoin, PathPoint, Point, SubPath};
use folio_text::{FontStore, layout_document};
use serde_json::{Value, json};
use std::path::Path;

#[path = "../../../test_support.rs"]
mod test_support;

fn list(v: &Value) -> &[Value] {
    v.as_array().unwrap()
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        _ => v.as_f64().unwrap(),
    }
}
fn point(v: &Value) -> Point {
    Point::new(number(&v[0]), number(&v[1]))
}
fn xy(p: Point) -> Value {
    json!([p.x, p.y])
}

fn geometry(kind: &str, input: &Value) -> Value {
    let points = list(&input["points"])
        .iter()
        .map(|v| PathPoint {
            anchor: point(&v[0]),
            left: point(&v[1]),
            right: point(&v[2]),
        })
        .collect();
    let Some(curve) = PathMeasure::new(&SubPath {
        points,
        open: !input["closed"].as_bool().unwrap(),
    }) else {
        return Value::Null;
    };
    if kind == "query" {
        let places: Vec<_> = list(&input["distances"])
            .iter()
            .map(|d| {
                let (p, v) = curve.at(number(d));
                json!([xy(p), xy(v)])
            })
            .collect();
        let nearest: Vec<_> = list(&input["probes"])
            .iter()
            .map(|q| {
                let hit = curve.nearest(point(q));
                [hit.at, hit.distance, hit.side]
            })
            .collect();
        return json!({"length":curve.length(), "center":xy(curve.center()),
                      "clockwise":curve.is_clockwise(), "places":places, "nearest":nearest});
    }
    let join = match input["join"].as_str().unwrap() {
        "spacing" => OffsetJoin::Round,
        "round" => OffsetJoin::of_stroke(LineJoin::Round, 4.0),
        "bevel" => OffsetJoin::of_stroke(LineJoin::Bevel, 4.0),
        "miter" => OffsetJoin::of_stroke(LineJoin::Miter, number(&input["limit"])),
        other => panic!("unknown join {other}"),
    };
    let offset = curve.offset_with(number(&input["height"]), join).unwrap();
    match kind {
        "on-curve" => json!(
            list(&input["probes"])
                .iter()
                .map(|q| offset.path.nearest(point(q)).distance)
                .collect::<Vec<_>>()
        ),
        "map" => json!(
            list(&input["distances"])
                .iter()
                .map(|d| offset.map(number(d)))
                .collect::<Vec<_>>()
        ),
        "unmap" => json!(
            list(&input["distances"])
                .iter()
                .map(|d| offset.unmap(number(d)))
                .collect::<Vec<_>>()
        ),
        "offset-query" => {
            json!({"length":offset.path.length(), "map":list(&input["distances"]).iter()
            .map(|d| offset.map(number(d))).collect::<Vec<_>>(), "unmap":list(&input["reverse"]).iter()
            .map(|d| offset.unmap(number(d))).collect::<Vec<_>>()})
        }
        _ => panic!("unknown geometry query {kind}"),
    }
}

fn layout(input: &Value, fonts: &FontStore, fixture: &Path) -> Value {
    let mut doc = folio_idml::read_package(fixture, None).unwrap();
    let item = doc.spreads[0]
        .items
        .iter_mut()
        .find(|i| i.text_path.is_some())
        .unwrap();
    let path = item.text_path.as_mut().unwrap();
    if let Some(v) = input["start"].as_f64() {
        path.start = v;
    }
    if let Some(v) = input["end"].as_f64() {
        path.end = v;
    }
    if let Some(v) = input["flipped"].as_bool() {
        path.flipped = v;
    }
    if let Some(v) = input["spacing"].as_f64() {
        path.spacing = v;
    }
    if let Some(v) = input["align"].as_str() {
        path.path_alignment = serde_json::from_value::<PathAlignment>(json!(v)).unwrap();
    }
    if let Some(v) = input["effect"].as_str() {
        path.effect = serde_json::from_value::<PathEffect>(json!(v)).unwrap();
    }
    if let Some(v) = input["textAlign"].as_str() {
        path.text_alignment = serde_json::from_value::<PathTextAlignment>(json!(v)).unwrap();
    }
    let id = item.id.clone();
    let to_spread = item.transform;
    let page = &doc.spreads[0].pages[0];
    let to_page = page.transform.invert().unwrap();
    let layout = layout_document(&doc, fonts);
    let glyphs = &layout.frames[&id].lines[0].glyphs;
    assert_eq!(
        glyphs.len(),
        9,
        "expected nine visible glyphs in Arial path"
    );
    let samples: Vec<_> = list(&input["glyphs"])
        .iter()
        .map(|index| {
            let g = &glyphs[index.as_u64().unwrap() as usize];
            let p = to_page.apply(to_spread.apply(Point::new(g.x, g.y)));
            let (x, y) = (p.x - page.bounds.left, p.y - page.bounds.top);
            if input["axes"] == true {
                let m = g.path_transform.unwrap_or(Affine::IDENTITY);
                json!([x, y, m.a, m.b, m.c, m.d])
            } else {
                json!([x, y])
            }
        })
        .collect();
    json!(samples)
}

fn difference(expected: &Value, actual: &Value) -> f64 {
    match (expected, actual) {
        (Value::Number(e), Value::Number(a)) => (e.as_f64().unwrap() - a.as_f64().unwrap()).abs(),
        (Value::Array(e), Value::Array(a)) if e.len() == a.len() => e
            .iter()
            .zip(a)
            .map(|(e, a)| difference(e, a))
            .fold(0.0, f64::max),
        (Value::Object(e), Value::Object(a)) if e.len() == a.len() => e
            .iter()
            .map(|(k, v)| a.get(k).map_or(f64::INFINITY, |w| difference(v, w)))
            .fold(0.0, f64::max),
        (e, a) if e == a => 0.0,
        _ => f64::INFINITY,
    }
}

fn layout_error(expected: &Value, actual: &Value) -> (f64, f64) {
    let (e, a) = (expected.as_array().unwrap(), actual.as_array().unwrap());
    assert_eq!(e.len(), a.len());
    let mut position: f64 = 0.0;
    let mut axes: f64 = 0.0;
    for (e, a) in e.iter().zip(a) {
        let (e, a) = (e.as_array().unwrap(), a.as_array().unwrap());
        assert_eq!(e.len(), a.len());
        position = position.max(difference(&e[0], &a[0]).hypot(difference(&e[1], &a[1])));
        for (e, a) in e[2..].iter().zip(&a[2..]) {
            axes = axes.max(difference(e, a));
        }
    }
    (position, axes)
}

fn aligned_brackets(input: &Value) -> Value {
    // R6
    let xml = format!(
        r#"<GraphicLine StrokeWeight="6">
        <Properties><PathGeometry><GeometryPathType PathOpen="true"><PathPointArray>
        <PathPointType Anchor="80 -296" LeftDirection="80 -296" RightDirection="140 -376"/>
        <PathPointType Anchor="320 -296" LeftDirection="260 -216" RightDirection="320 -296"/>
        </PathPointArray></GeometryPathType></PathGeometry></Properties>
        <TextPath PathAlignment="{}PathAlignment" FlipPathEffect="{}" StartBracket="{}" EndBracket="{}"/>
        </GraphicLine>"#,
        input["from"].as_str().unwrap(),
        if input["flipped"] == true {
            "Flipped"
        } else {
            "NotFlipped"
        },
        input["distances"][0],
        input["distances"][1]
    );
    let to = serde_json::from_value(input["to"].clone()).unwrap();
    json!(folio_idml::textpath::brackets_for_alignment(&xml, to))
}

#[cfg_attr(not(has_reference_vectors), ignore = "optional reference vectors are missing")]
#[test]
fn public_path_queries_and_glyphs_match_vectors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let fixture = root.join("next/oracle/fixtures/typepath-c3.idml");
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let doc = folio_idml::read_package(&fixture, None).unwrap();
    let has_layout_font = fonts.resolve("Arial", "Regular", &doc.font_table).exact;
    if !has_layout_font {
        test_support::skip("12 text-path glyph vectors require Arial Regular");
    }
    let mut failures = Vec::new();
    let (mut count, mut matched) = (0, 0);
    for file in ["edges", "brackets", "offset", "layout"] {
        if file == "layout" && !has_layout_font {
            continue;
        }
        let bytes = std::fs::read(root.join(format!("cleanroom/vectors/A3/{file}.json"))).unwrap();
        let cases: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        for case in cases {
            count += 1;
            let input = &case["input"];
            let kind = case["kind"].as_str().unwrap();
            let actual = match kind {
                "layout" => layout(input, &fonts, &fixture),
                "aligned-brackets" => aligned_brackets(input),
                _ => geometry(kind, input),
            };
            let errors = if kind == "layout" {
                let (pt, axis) = layout_error(&case["expected"], &actual);
                vec![("pt", pt), ("axis", axis)]
            } else {
                vec![("abs", difference(&case["expected"], &actual))]
            };
            let meets = |bounds: &Value| {
                errors
                    .iter()
                    .all(|(unit, error)| *error <= bounds[unit].as_f64().unwrap())
            };
            matched += usize::from(meets(&case["tolerance"]));
            if !meets(case.get("baselineLimit").unwrap_or(&case["tolerance"])) {
                failures.push(format!("{} {errors:?}", case["id"]));
            }
        }
    }
    let (expected_count, min_matched) = if has_layout_font { (71, 58) } else { (59, 49) };
    assert_eq!(count, expected_count);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(matched >= min_matched, "{matched}/{count} at measurement tolerance");
}
