use folio_text::layout::paragraph_columns::partition;
use serde::Deserialize;

#[derive(Deserialize)]
struct Suite {
    vectors: Vec<Vector>,
}

#[derive(Deserialize)]
struct Vector {
    id: String,
    source: String,
    input: Input,
    expected: Expected,
}

#[derive(Deserialize)]
struct Input {
    steps: Vec<f64>,
    first_costs: Vec<f64>,
    columns: usize,
    break_allowed_before_row: Option<Vec<bool>>,
}

#[derive(Deserialize)]
struct Expected {
    rows_per_column: Vec<usize>,
}

fn check_vectors(json: &str, count: usize) {
    let suite: Suite = serde_json::from_str(json).expect("A5 vector file");
    assert_eq!(suite.vectors.len(), count);
    for vector in suite.vectors {
        let input = vector.input;
        assert_eq!(
            partition(
                &input.steps,
                &input.first_costs,
                input.columns,
                input.break_allowed_before_row.as_deref(),
            ),
            vector.expected.rows_per_column,
            "{} ({})",
            vector.id,
            vector.source,
        );
    }
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn a5_vectors_partition_regular() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn a5_vectors_partition_regular() {
    check_vectors(
        include_str!("../../../../../cleanroom/vectors/A5/depth.json"),
        79,
    );
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn a5_vectors_partition_measured() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn a5_vectors_partition_measured() {
    check_vectors(
        include_str!("../../../../../cleanroom/vectors/A5/depth-measured.json"),
        20,
    );
}
