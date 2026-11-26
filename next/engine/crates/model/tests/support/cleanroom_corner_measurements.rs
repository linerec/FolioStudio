use folio_model::geom::{CornerKind, CornerOption, apply_corners};
use folio_model::{PathPoint, Point, SubPath};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Case {
    source: String,
    name: String,
    input: Input,
    output: Measurement,
}

#[derive(Deserialize)]
struct Input {
    target: String,
    properties: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Measurement {
    pdf_path_segments_pt: Vec<Segment>,
}

#[derive(Deserialize)]
struct Segment {
    #[serde(rename = "type")]
    kind: char,
    arguments: Vec<[f64; 2]>,
}

fn specimen(target: &str) -> SubPath {
    let (center, width, height, angle): ([f64; 2], f64, f64, f64) = match target {
        "r-plain" => ([90.0, 66.0], 120.0, 68.0, 0.0),
        "r-fill" => ([92.5, 171.0], 117.0, 86.0, 0.0),
        // R2. The opposite side midpoints locate the rotated specimen.
        "r-rot" => ([102.773, 257.4606], 105.0, 61.0, -19.0),
        _ => panic!("unknown specimen {target}"),
    };
    let (sin, cos) = angle.to_radians().sin_cos();
    let points = [(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0)].map(|(x, y)| {
        let x = x * width / 2.0;
        let y = y * height / 2.0;
        let p = Point::new(center[0] + x * cos - y * sin, center[1] + x * sin + y * cos);
        PathPoint { anchor: p, left: p, right: p }
    });
    SubPath { points: points.into(), open: false }
}

fn option(props: &BTreeMap<String, Value>, position: &str) -> CornerOption {
    let kind = match props[&format!("{position}CornerOption")].as_str().unwrap() {
        "CornerOptions.ROUNDED_CORNER" => CornerKind::Rounded,
        "CornerOptions.INVERSE_ROUNDED_CORNER" => CornerKind::InverseRounded,
        "CornerOptions.BEVEL_CORNER" => CornerKind::Bevel,
        "CornerOptions.FANCY_CORNER" => CornerKind::Fancy,
        "CornerOptions.INSET_CORNER" => CornerKind::Inset,
        name => panic!("unknown corner option {name}"),
    };
    let radius = props[&format!("{position}CornerRadius")].as_f64().unwrap();
    CornerOption { kind, radius }
}

fn segments(path: &SubPath) -> Vec<Segment> {
    let xy = |p: Point| [p.x, p.y];
    path.points.iter().zip(path.points.iter().cycle().skip(1)).filter_map(|(a, b)| {
        if a.right != a.anchor || b.left != b.anchor {
            Some(Segment { kind: 'c', arguments: vec![xy(a.anchor), xy(a.right), xy(b.left), xy(b.anchor)] })
        } else if a.anchor != b.anchor {
            Some(Segment { kind: 'l', arguments: vec![xy(a.anchor), xy(b.anchor)] })
        } else {
            None
        }
    }).collect()
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn corner_geometry_matches_pdf_measurements() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn corner_geometry_matches_pdf_measurements() {
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../../../../../../cleanroom/vectors/B1/cr-02.json"
    )).unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        assert_eq!(case.source, "measured", "{}", case.name);
        let options = ["topLeft", "bottomLeft", "bottomRight", "topRight"]
            .map(|position| option(&case.input.properties, position));
        let path = apply_corners(&specimen(&case.input.target), &options);
        assert!(!path.open, "{}", case.name);
        let actual = segments(&path);
        let expected = case.output.pdf_path_segments_pt;
        assert_eq!(actual.len(), expected.len(), "{}", case.name);
        // R3. PDF coordinates are quantized to 0.001 pt.
        let same = |a: &Segment, b: &Segment| {
            a.kind == b.kind && a.arguments.len() == b.arguments.len()
                && a.arguments.iter().flatten().zip(b.arguments.iter().flatten())
                    .all(|(a, b)| (a - b).abs() <= 0.0015)
        };
        assert!((0..actual.len()).any(|offset| {
            actual.iter().cycle().skip(offset).zip(&expected).all(|(a, b)| same(a, b))
        }), "{}", case.name);
    }
}
