use folio_text::optical_margin::edges;
use serde::Deserialize;
use std::{collections::BTreeMap, env, fs, path::PathBuf};
use ttf_parser::Face;

#[cfg(has_reference_vectors)]
const OLD_EDGES: &str = include_str!("../../../../../cleanroom/vectors/A6/old-edges.json");
#[cfg(has_reference_vectors)]
const CR_02: &str = include_str!("../../../../../cleanroom/vectors/A6/cr-02.json");
#[cfg(has_reference_vectors)]
const HISTORICAL: &str = include_str!("../../../../../cleanroom/vectors/A6/historical-dom.json");

#[derive(Deserialize)]
struct Vector<Input, Output> {
    name: String,
    source: String,
    input: Input,
    output: Output,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeInput {
    font_file: String,
    #[serde(default)]
    face_index: u32,
    char: char,
    scale: f64,
}

#[derive(Debug, Deserialize)]
struct ExpectedEdges {
    left: f64,
    right: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldEdge {
    #[serde(flatten)]
    vector: Vector<EdgeInput, Option<ExpectedEdges>>,
    tolerance_em: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredInput {
    font: String,
    font_style: String,
    text: String,
    point_size: f64,
    optical_size: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredOutput {
    left_overhang_pt: f64,
    right_overhang_pt: Option<f64>,
}

struct Fonts {
    dir: PathBuf,
    data: BTreeMap<String, Vec<u8>>,
}

impl Fonts {
    fn system() -> Option<Self> {
        let dir = env::var_os("FOLIO_A6_FONT_DIR")
            .map(PathBuf::from)
            .or_else(|| env::var_os("WINDIR").map(|dir| PathBuf::from(dir).join("Fonts")));
        if dir.is_none() {
            assert!(!cfg!(windows), "WINDIR or FOLIO_A6_FONT_DIR is required");
            eprintln!("A6 vectors need FOLIO_A6_FONT_DIR on this platform");
        }
        dir.map(|dir| Self {
            dir,
            data: BTreeMap::new(),
        })
    }

    fn face(&mut self, file: &str, index: u32) -> Face<'_> {
        if !self.data.contains_key(file) {
            let path = self.dir.join(file);
            let bytes =
                fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            self.data.insert(file.to_owned(), bytes);
        }
        Face::parse(&self.data[file], index)
            .unwrap_or_else(|error| panic!("{file}, face {index}: {error:?}"))
    }
}

fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= tolerance
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn old_edge_vectors_match_in_both_orders() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn old_edge_vectors_match_in_both_orders() {
    let rows: Vec<OldEdge> = serde_json::from_str(OLD_EDGES).unwrap();
    assert_eq!(rows.len(), 200);
    let Some(mut fonts) = Fonts::system() else {
        return;
    };

    let mut observed = Vec::with_capacity(rows.len());
    for row in &rows {
        let Vector {
            name,
            source,
            input,
            output,
        } = &row.vector;
        assert_eq!(source, "old-impl");
        let face = fonts.face(&input.font_file, input.face_index);
        let actual = edges(&face, input.char, input.scale);
        let matches = match (&actual, output) {
            (None, None) => true,
            (Some(actual), Some(expected)) => {
                close(actual.left, expected.left, row.tolerance_em)
                    && close(actual.right, expected.right, row.tolerance_em)
            }
            _ => false,
        };
        assert!(matches, "{name}: got {actual:?}, expected {output:?}");
        observed.push(actual);
    }

    for (row, previous) in rows.iter().zip(observed).rev() {
        let input = &row.vector.input;
        let face = fonts.face(&input.font_file, input.face_index);
        assert_eq!(
            edges(&face, input.char, input.scale),
            previous,
            "{}",
            row.vector.name,
        );
    }
}

fn measured_font(input: &MeasuredInput) -> &'static str {
    match (input.font.as_str(), input.font_style.as_str()) {
        ("Arial", "Regular") => "arial.ttf",
        ("Arial", "Italic") => "ariali.ttf",
        ("Times New Roman", "Regular") => "times.ttf",
        ("Consolas", "Regular") => "consola.ttf",
        ("Georgia", "Regular") => "georgia.ttf",
        ("Verdana", "Regular") => "verdana.ttf",
        ("Malgun Gothic", "Regular") => "malgun.ttf",
        (family, style) => panic!("missing test font for {family} {style}"),
    }
}

fn measured_baseline_gap(name: &str) -> bool {
    let size = name
        .strip_prefix("historical-")
        .and_then(|rest| rest.split_once("-U+"));
    matches!(
        size,
        Some(("size48" | "size72" | "size1296", "002E" | "0054" | "2014"))
    )
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn measured_overhang_vectors_keep_the_baseline() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn measured_overhang_vectors_keep_the_baseline() {
    type Measured = Vector<MeasuredInput, MeasuredOutput>;
    let cr_02: Vec<Measured> = serde_json::from_str(CR_02).unwrap();
    let historical: Vec<Measured> = serde_json::from_str(HISTORICAL).unwrap();
    assert_eq!((cr_02.len(), historical.len()), (15, 81));
    let Some(mut fonts) = Fonts::system() else {
        return;
    };

    let mut passed = 0;
    for Vector {
        name,
        source,
        input,
        output,
    } in cr_02.into_iter().chain(historical)
    {
        assert_eq!(source, "measured");
        let point_size = input.point_size;
        let optical = input
            .optical_size
            .clamp(0.75 * point_size, 4.0 * point_size);
        let mut chars = input.text.trim_end_matches(['\r', '\n']).chars();
        let first = chars.next().expect("empty measured line");
        let last = chars.next_back().unwrap_or(first);
        let face = fonts.face(measured_font(&input), 0);
        let scale = point_size / optical;
        let left = edges(&face, first, scale).map_or(0.0, |edge| edge.left * optical);
        let right = edges(&face, last, scale).map_or(0.0, |edge| edge.right * point_size);
        let matches = close(left, output.left_overhang_pt, 0.25)
            && output
                .right_overhang_pt
                .is_none_or(|expected| close(right, expected, 0.5));
        if matches {
            passed += 1;
        } else {
            assert!(
                measured_baseline_gap(&name),
                "{name}: got left {left} pt, right {right} pt"
            );
        }
    }
    assert!(passed >= 87, "only {passed} measured vectors passed");
}
