use folio_compose::{
    Alignment, Item, Settings, compose::first_fit, hyphenate::Dictionary, place_line,
};
use serde_json::Value;
use std::{fs, path::Path};

fn vectors(file: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../cleanroom/vectors/A1")
        .join(file);
    let mut table: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    serde_json::from_value(table["vectors"].take()).unwrap()
}

#[test]
fn dictionaries_match_word_vectors() {
    let cases = vectors("A1a/dictionary.json");
    assert_eq!(cases.len(), 40);
    for case in cases {
        let input = &case["input"];
        let dict = Dictionary::for_language(input["language"].as_str().unwrap());
        assert_eq!(
            dict.is_some(),
            case["expect"]["dictionary"].as_bool().unwrap(),
            "{}",
            case["id"]
        );
        if let Some(dict) = dict {
            let word = input["word"].as_str().unwrap();
            assert_eq!(
                serde_json::json!(dict.points(word)),
                case["expect"]["points"],
                "{}",
                case["id"]
            );
        }
    }
}

fn alignment(name: &str) -> Alignment {
    match name {
        "Left" => Alignment::Left,
        "Center" => Alignment::Center,
        "Right" => Alignment::Right,
        "LeftJustified" => Alignment::LeftJustified,
        "CenterJustified" => Alignment::CenterJustified,
        "RightJustified" => Alignment::RightJustified,
        "FullyJustified" => Alignment::FullyJustified,
        _ => panic!("unknown alignment {name}"),
    }
}

fn measured_line(input: &Value) -> (Vec<Item>, Settings) {
    let items = input["paragraph"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| Item {
            ch: i[0].as_str().unwrap().chars().next().unwrap(),
            advance: i[1].as_f64().unwrap(),
            end_advance: i[2].as_f64().unwrap(),
            point_size: i[3].as_f64().unwrap(),
            unit: i[4].as_f64().unwrap_or(0.0),
        })
        .collect();
    let style = &input["settings"];
    let settings = Settings {
        alignment: alignment(style["alignment"].as_str().unwrap()),
        single_word: alignment(style["single_word"].as_str().unwrap_or("FullyJustified")),
        hyphenation: style["hyphenation"].as_bool().unwrap(),
        ..Settings::default()
    };
    (items, settings)
}

#[test]
fn paragraphs_with_dashes_match_measured_breaks() {
    let cases: Vec<_> = vectors("A1a/compose-measured.json")
        .into_iter()
        .filter(|case| case["rule"] == "R10")
        .collect();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let input = &case["input"];
        let (items, settings) = measured_line(input);
        let widths = input["widths"].as_array().unwrap();
        let breaks = folio_compose::compose(&items, &settings, &|row| {
            widths[row.min(widths.len() - 1)].as_f64().unwrap()
        });
        assert_eq!(
            serde_json::json!(breaks),
            case["expect"]["breaks"],
            "{}",
            case["id"]
        );
    }
}

#[test]
fn first_fit_matches_measured_line_starts() {
    let cases = vectors("A1b/first-fit-measured.json");
    assert_eq!(cases.len(), 25);
    for case in cases {
        let input = &case["input"];
        let (items, settings) = measured_line(input);
        let widths = input["widths"].as_array().unwrap();
        let breaks = first_fit(&items, &settings, &|row| {
            widths[row.min(widths.len() - 1)].as_f64().unwrap()
        });
        assert_eq!(
            serde_json::json!(breaks),
            case["expect"]["breaks"],
            "{}",
            case["id"]
        );
    }
}

#[test]
fn alignment_matches_measured_character_positions() {
    let cases = vectors("A1b/place-line-align-measured.json");
    assert_eq!(cases.len(), 55);
    for case in cases {
        let input = &case["input"];
        let (items, settings) = measured_line(input);
        let start = input["start"].as_u64().unwrap() as usize;
        let end = input["end"].as_u64().unwrap() as usize;
        let line = place_line(
            &items,
            start,
            end,
            input["available"].as_f64().unwrap(),
            &settings,
            input["last"].as_bool().unwrap(),
        );
        let expected = case["expect"]["x"].as_array().unwrap();
        assert_eq!(line.x.len(), expected.len(), "{}", case["id"]);
        for (i, (x, target)) in line.x.iter().zip(expected).enumerate() {
            let target = target.as_f64().unwrap();
            assert!(
                (x - target).abs() <= 0.002,
                "{} at {i}: {x} vs {target}",
                case["id"]
            );
        }
    }
}
