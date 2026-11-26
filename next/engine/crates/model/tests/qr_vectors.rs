#[cfg(not(has_reference_vectors))]
mod vector_probe {
    #[test]
    #[ignore = "optional QR reference vectors are missing"]
    fn json_vectors_match_public_outputs() { panic!("optional QR reference vectors are missing"); }
}

#[cfg(has_reference_vectors)]
#[allow(dead_code, reason = "the standalone probe has its own entry point")]
mod vector_probe {
    include!("../../../../../cleanroom/vectors/A9/probe.rs");

    #[test]
    fn json_vectors_match_public_outputs() {
        let suites = [
            (
                "cr-02",
                include_str!("../../../../../cleanroom/vectors/A9/cr-02.json"),
            ),
            (
                "oracle",
                include_str!("../../../../../cleanroom/vectors/A9/oracle.json"),
            ),
            (
                "measured",
                include_str!("../../../../../cleanroom/vectors/A9/measured.json"),
            ),
            (
                "old-impl",
                include_str!("../../../../../cleanroom/vectors/A9/old-impl.json"),
            ),
        ];
        for (suite, data) in suites {
            let cases: Vec<Value> = serde_json::from_str(data).unwrap();
            for vector in cases {
                let name = vector["name"].as_str().unwrap();
                assert!(
                    matches!(
                        vector["source"].as_str(),
                        Some("measured" | "oracle" | "old-impl")
                    ),
                    "{suite}/{name} has no valid source"
                );
                let input = &vector["input"];
                let output = &vector["output"];
                let expected =
                    if input["op"].as_str().unwrap_or("encode") == "encode" && output.is_object() {
                        &output["rows"]
                    } else {
                        output
                    };
                assert_eq!(&evaluate(input), expected, "{suite}/{name}");
            }
        }
    }
}

#[test]
fn native_matrices_match() {
    use folio_model::qr::encode;

    for (index, line) in include_str!("../src/qr-native-cases.tsv")
        .lines()
        .enumerate()
    {
        let (payload, expected) = line.split_once('\t').unwrap();
        let actual: String = encode(payload)
            .unwrap()
            .dark
            .into_iter()
            .map(|dark| if dark { '1' } else { '0' })
            .collect();
        assert_eq!(actual, expected, "native-tsv-{}", index + 1);
    }
}

#[test]
fn version_40_medium_capacity_boundaries() {
    use folio_model::qr::encode;

    for (unit, limit) in [("1", 5596), ("A", 3391), ("a", 2331)] {
        let payload = unit.repeat(limit);
        assert_eq!(encode(&payload).unwrap().size, 177, "{unit} at capacity");
        assert!(encode(&(payload + unit)).is_none(), "{unit} over capacity");
    }
}
