use folio_model::geom::{CornerKind, CornerOption, Seg, apply_corners};
use folio_model::{
    Document, ItemKind, PageItem, Paint, PathPoint, Point, Spread, Stroke, StrokeAlignment,
    SubPath, TextFrame, TextWrap, TextWrapMode,
};
use folio_text::DocumentFrames;
use serde::Deserialize;

type Coordinates = [[f64; 2]; 3];

#[derive(Deserialize)]
struct Ring {
    closed: bool,
    points: Vec<Coordinates>,
}

#[derive(Deserialize)]
struct Ink {
    weight: f64,
    alignment: String,
}

#[derive(Deserialize)]
struct Input {
    mode: String,
    item: Vec<Ring>,
    stroke: Ink,
    offsets: [f64; 4],
}

#[derive(Deserialize)]
struct Tolerance {
    pt: f64,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    input: Input,
    expected: Vec<Ring>,
    tolerance: Tolerance,
}

fn path(ring: &Ring) -> SubPath {
    SubPath {
        open: !ring.closed,
        points: ring
            .points
            .iter()
            .map(|p| {
                let [anchor, left, right] = p.map(|[x, y]| Point::new(x, y));
                PathPoint {
                    anchor,
                    left,
                    right,
                }
            })
            .collect(),
    }
}

fn rectangle(left: f64, top: f64, right: f64, bottom: f64) -> Ring {
    Ring {
        closed: true,
        points: [[left, top], [right, top], [right, bottom], [left, bottom]]
            .map(|p| [p; 3])
            .to_vec(),
    }
}

fn make_document(case: &Case) -> Document {
    let input = &case.input;
    let frame = PageItem {
        id: "frame".into(),
        paths: vec![path(&rectangle(0.0, 0.0, 600.0, 800.0))],
        kind: ItemKind::TextFrame(TextFrame {
            columns: 1,
            ..TextFrame::default()
        }),
        ..PageItem::default()
    };
    let paths = input
        .item
        .iter()
        .map(path)
        .map(|p| {
            if case.id.starts_with("contour/rounded-") {
                apply_corners(
                    &p,
                    &[CornerOption {
                        kind: CornerKind::Rounded,
                        radius: 30.0,
                    }; 4],
                )
            } else {
                p
            }
        })
        .collect();
    let obstacle = PageItem {
        id: "obstacle".into(),
        paths,
        wraps: true,
        text_wrap: Some(TextWrap {
            mode: match input.mode.as_str() {
                "contour" => TextWrapMode::Contour,
                "boundingBox" => TextWrapMode::BoundingBox,
                mode => panic!("{mode}"),
            },
            offsets: input.offsets,
            ..TextWrap::default()
        }),
        stroke: Stroke {
            paint: Paint::Swatch {
                id: "black".into(),
                tint: 100.0,
            },
            weight: input.stroke.weight,
            alignment: match input.stroke.alignment.as_str() {
                "center" => StrokeAlignment::Center,
                "inside" => StrokeAlignment::Inside,
                "outside" => StrokeAlignment::Outside,
                value => panic!("{value}"),
            },
            ..Stroke::default()
        },
        ..PageItem::default()
    };
    Document {
        spreads: vec![Spread {
            items: vec![frame, obstacle],
            ..Spread::default()
        }],
        ..Document::default()
    }
}

fn anchors(ring: &[Seg]) -> Vec<Coordinates> {
    (0..ring.len())
        .map(|i| {
            let previous = &ring[(i + ring.len() - 1) % ring.len()];
            [ring[i].a, previous.c, ring[i].b].map(|p| [p.x, p.y])
        })
        .collect()
}

fn max_error(actual: &[Coordinates], expected: &[Coordinates]) -> f64 {
    let n = expected.len();
    if actual.len() != n || n == 0 {
        return f64::INFINITY;
    }
    let mut best = f64::INFINITY;
    for origin in 0..n {
        for reversed in [false, true] {
            let mut error: f64 = 0.0;
            for (i, want) in expected.iter().enumerate() {
                let mut got = actual[if reversed {
                    (origin + n - i) % n
                } else {
                    (origin + i) % n
                }];
                if reversed {
                    got.swap(1, 2)
                }
                for (x, y) in got.iter().flatten().zip(want.iter().flatten()) {
                    error = error.max((x - y).abs());
                }
            }
            best = best.min(error);
        }
    }
    best
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn measured_wrap_outlines_and_bounds() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn measured_wrap_outlines_and_bounds() {
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../../../../../cleanroom/vectors/A2/outline.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 24);
    let mut failures = Vec::new();
    for case in cases {
        let doc = make_document(&case);
        let frames = DocumentFrames::new(&doc);
        let wraps = &frames.frame("frame").unwrap().wraps;
        assert_eq!(wraps.len(), 1, "{}", case.id);
        let wrap = &wraps[0];
        let actual = match wrap.contour_segments() {
            Some(rings) => rings.iter().map(|r| anchors(r)).collect(),
            None => {
                let r = wrap.rect;
                vec![rectangle(r.left, r.top, r.right, r.bottom).points]
            }
        };
        assert_eq!(actual.len(), case.expected.len(), "{}", case.id);
        for (a, e) in actual.iter().zip(&case.expected) {
            let error = max_error(a, &e.points);
            if error > case.tolerance.pt {
                failures.push(format!(
                    "{}: {} / {} anchors, max error {error:.6} pt",
                    case.id,
                    a.len(),
                    e.points.len()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
