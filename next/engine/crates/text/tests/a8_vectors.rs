use folio_model::{
    AutoSize, AutoSizeKind, CharacterAttrs, Document, ItemKind, Leading, PageItem, Paint,
    Paragraph, ParagraphAttrs, PathPoint, Point, Spread, Story, Stroke, SubPath, TextFrame,
    TextRun,
};
use folio_text::{DocumentFrames, FontStore, fit_frames, layout_story};
use serde_json::Value;

#[cfg(has_reference_vectors)]
const VECTORS: &str = include_str!("../../../../../cleanroom/vectors/A8/measured.json");

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn height_only_fits_measured_overset() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn height_only_fits_measured_overset() {
    let vectors: Vec<Value> = serde_json::from_str(VECTORS).unwrap();
    let case = vectors
        .iter()
        .find(|row| row["id"] == "m-height-overset")
        .unwrap();
    assert_eq!(case["source"], "measured");
    let config = &case["input"]["config"];
    let [top, left, bottom, right] =
        serde_json::from_value::<[f64; 4]>(config["bounds"].clone()).unwrap();
    let font = case["input"]["font"].as_str().unwrap();
    let fonts = FontStore::from_dirs(&FontStore::system_dirs());
    let mut doc = Document::default();
    if !fonts.resolve(font, "Regular", &doc.font_table).exact {
        eprintln!("m-height-overset requires {font} Regular");
        return;
    }
    doc.stories.insert(
        "story".into(),
        Story {
            id: "story".into(),
            paragraphs: vec![Paragraph {
                attrs: ParagraphAttrs {
                    hyphenation: false,
                    ..ParagraphAttrs::default()
                },
                runs: vec![TextRun {
                    text: config["text"].as_str().unwrap().into(),
                    attrs: CharacterAttrs {
                        font_family: font.into(),
                        font_style: "Regular".into(),
                        point_size: 12.0,
                        leading: Leading::Fixed(14.0),
                        ..CharacterAttrs::default()
                    },
                    ..TextRun::default()
                }],
                ..Paragraph::default()
            }],
            ..Story::default()
        },
    );
    doc.spreads.push(Spread {
        items: vec![PageItem {
            id: "frame".into(),
            kind: ItemKind::TextFrame(TextFrame {
                story: "story".into(),
                columns: 1,
                auto_size: Some(AutoSize {
                    kind: AutoSizeKind::HeightOnly,
                    point: (0.5, 0.0),
                    min_height: None,
                    min_width: None,
                    no_line_breaks: false,
                }),
                ..TextFrame::default()
            }),
            paths: vec![path(left, top, right, bottom)],
            stroke: Stroke {
                paint: Paint::Swatch {
                    id: "Color/Black".into(),
                    tint: 100.0,
                },
                weight: 1.0,
                ..Stroke::default()
            },
            ..PageItem::default()
        }],
        ..Spread::default()
    });
    let compose =
        |doc: &Document| layout_story(doc, &DocumentFrames::new(doc), "story", &fonts).unwrap();
    assert!(compose(&doc).overset);
    let fits = fit_frames(&doc, &fonts, &["story"]);
    let [fit] = fits.as_slice() else {
        panic!("expected one fit, got {fits:?}")
    };
    let expected = &case["expected"]["after"]["first"];
    let [expected_top, _, expected_bottom, _] =
        serde_json::from_value::<[f64; 4]>(expected["bounds"].clone()).unwrap();
    // R2, R3
    assert_eq!(fit.frame, "frame");
    assert_eq!(fit.point, (0.5, 0.0));
    assert_eq!(fit.width, right - left);
    assert!((fit.height - (expected_bottom - expected_top)).abs() <= 1.0);
    doc.spreads[0].items[0].paths = vec![path(left, top, right, top + fit.height)];
    let layout = compose(&doc);
    assert!(!layout.overset);
    assert_eq!(
        layout.frames[0].lines.len(),
        expected["lines"].as_array().unwrap().len()
    );
    assert!(layout.frames[0].lines.last().unwrap().baseline + 0.5 <= top + fit.height + 1e-9);
}

fn path(left: f64, top: f64, right: f64, bottom: f64) -> SubPath {
    SubPath {
        open: false,
        points: [(left, top), (right, top), (right, bottom), (left, bottom)]
            .into_iter()
            .map(|(x, y)| {
                let anchor = Point::new(x, y);
                PathPoint {
                    anchor,
                    left: anchor,
                    right: anchor,
                }
            })
            .collect(),
    }
}
