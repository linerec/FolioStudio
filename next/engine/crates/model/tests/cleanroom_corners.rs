use folio_model::geom::{CornerKind, CornerOption, Seg, apply_corners, segments};
use folio_model::{PathPoint, Point, SubPath};
use serde::Deserialize;

#[path = "support/cleanroom_corner_measurements.rs"]
mod measurements;

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    source: String,
    input: Input,
    output: SubPath,
}

#[derive(Deserialize)]
struct Input {
    path: SubPath,
    options: [CornerOption; 4],
}

fn assert_point(actual: Point, expected: Point, id: &str, index: usize, component: &str) {
    for (a, b) in [(actual.x, expected.x), (actual.y, expected.y)] {
        assert!(
            (a - b).abs() <= 1e-9 + 1e-12 * b.abs(),
            "{id} point {index} {component}: {a} vs {b}"
        );
    }
}

#[cfg(has_reference_vectors)]
fn vectors() -> Vec<Case> {
    let vectors: Vectors = serde_json::from_str(include_str!(
        "../../../../../cleanroom/vectors/B1/engine-corners.json"
    ))
    .unwrap();
    assert!(!vectors.cases.is_empty());
    vectors.cases
}

fn assert_path(actual: &SubPath, expected: &SubPath, id: &str) {
    assert_eq!(actual.open, expected.open, "{id}");
    assert_eq!(actual.points.len(), expected.points.len(), "{id}");
    for (i, (a, b)) in actual.points.iter().zip(&expected.points).enumerate() {
        assert_point(a.anchor, b.anchor, id, i, "anchor");
        assert_point(a.left, b.left, id, i, "left handle");
        assert_point(a.right, b.right, id, i, "right handle");
    }
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn corner_paths_match_vectors() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn corner_paths_match_vectors() {
    for Case {
        id,
        source,
        input,
        output,
    } in vectors()
    {
        assert!(
            matches!(source.as_str(), "measured" | "oracle" | "old-impl"),
            "{id}"
        );
        let actual = apply_corners(&input.path, &input.options);
        assert_path(&actual, &output, &id);
    }
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn corner_vectors_keep_their_shape_after_rotation_and_translation() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn corner_vectors_keep_their_shape_after_rotation_and_translation() {
    fn transform(mut path: SubPath) -> SubPath {
        let position = |p: Point| Point::new(19.0 - p.y, p.x - 37.0);
        for point in &mut path.points {
            *point = PathPoint {
                anchor: position(point.anchor),
                left: position(point.left),
                right: position(point.right),
            };
        }
        path
    }

    for Case {
        id, input, output, ..
    } in vectors()
    {
        let actual = apply_corners(&transform(input.path), &input.options);
        assert_path(&actual, &transform(output), &id);
    }
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn repeated_closing_markers_keep_the_vector_outline() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn repeated_closing_markers_keep_the_vector_outline() {
    let Case {
        id,
        mut input,
        output,
        ..
    } = vectors()
        .into_iter()
        .find(|case| case.id == "duplicated-start")
        .expect("duplicated-start vector");
    let closing = *input.path.points.last().unwrap();
    input.path.points.extend([closing; 2]);
    let actual = apply_corners(&input.path, &input.options);
    assert_path(&actual, &output, &id);
}

#[test]
fn smooth_and_zero_length_joins_keep_the_input_path() {
    let plain = |x, y| {
        let p = Point::new(x, y);
        PathPoint {
            anchor: p,
            left: p,
            right: p,
        }
    };
    let smooth = SubPath {
        points: vec![
            PathPoint {
                right: Point::new(1.0, 1.0),
                ..plain(0.0, 0.0)
            },
            PathPoint {
                left: Point::new(4.0, 0.0),
                right: Point::new(6.0, 0.0),
                ..plain(5.0, 0.0)
            },
            plain(9.0, 3.0),
        ],
        open: true,
    };
    let zero_length = SubPath {
        points: vec![plain(0.0, 0.0), plain(0.0, 0.0), plain(9.0, 3.0)],
        open: true,
    };
    let options = [CornerOption {
        kind: CornerKind::Rounded,
        radius: 2.0,
    }; 4];
    assert_eq!(apply_corners(&smooth, &options), smooth);
    assert_eq!(apply_corners(&zero_length, &options), zero_length);
}

#[test]
fn rounded_join_uses_cut_length_on_curved_edges() {
    let corner = Point::new(10.0, 0.0);
    let end = Point::new(40.0, 0.0);
    let path = SubPath {
        points: vec![
            PathPoint {
                anchor: Point::new(0.0, 0.0),
                left: Point::new(0.0, 0.0),
                right: Point::new(0.0, 10.0),
            },
            PathPoint {
                anchor: corner,
                left: Point::new(10.0, 10.0),
                right: corner,
            },
            PathPoint {
                anchor: end,
                left: end,
                right: end,
            },
        ],
        open: true,
    };
    // R2: the arc length from t = 0.75 to the corner on this cubic.
    let radius = 5.9375;
    let options = [CornerOption {
        kind: CornerKind::Rounded,
        radius,
    }; 4];
    let actual = apply_corners(&path, &options);
    let [retained, join, _]: [Seg; 3] = segments(&actual).try_into().unwrap();
    let original = segments(&path)[0];
    for t in [0.0, 0.5, 1.0] {
        let actual = retained.eval(t);
        let expected = original.eval(t * 0.75);
        assert!((actual.x - expected.x).hypot(actual.y - expected.y) < 0.001);
    }
    for (anchor, handle) in [(join.a, join.b), (join.d, join.c)] {
        let dx = corner.x - anchor.x;
        let dy = corner.y - anchor.y;
        let scale = radius * 0.552_284_749_831 / dx.hypot(dy);
        let expected = Point::new(anchor.x + dx * scale, anchor.y + dy * scale);
        assert_point(handle, expected, "curved rounded corner", 1, "handle");
    }
}
