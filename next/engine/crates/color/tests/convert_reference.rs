//! Edit ▸ Convert to Profile against InDesign 2026 (2025 not checked), engine Adobe (ACE).
//!
//! `convert_reference.json` holds, for 16 settings, the swatch values before the conversion and the values InDesign wrote after it
//! (`artifacts/profileconv-c1-20261023/tools/x2_convert.py`, `make_reference.py`; raw results `out/x2_b.json`, `out/x2_g.json`).
//! The swatches are 40 random CMYK and 40 random RGB colours plus seven fixed ones of each space and two spots.

use folio_color::{ColorConverter, ConvertSettings, ProfileIndex, icc::Intent, read_profile};
use serde_json::Value;

fn profile(index: &ProfileIndex, name: &str) -> Option<Vec<u8>> {
    match index.find(name) {
        Some(p) => Some(read_profile(&p).expect("installed profile reads")),
        None => {
            eprintln!("SKIPPED: ICC profile \"{name}\" is not installed");
            None
        }
    }
}

fn intent(name: &str) -> Intent {
    match name {
        "Perceptual" => Intent::Perceptual,
        "Saturation" => Intent::Saturation,
        "Absolute Colorimetric" => Intent::Absolute,
        _ => Intent::Relative,
    }
}

/// Worst and mean difference in 8-bit levels (CMYK percent × 2.55) of one space's swatches.
fn run(case: &Value, index: &ProfileIndex, space: &str) -> Option<(f64, f64, usize)> {
    let (from, to) = if space == "RGB" { ("rgbFrom", "rgbTo") } else { ("cmykFrom", "cmykTo") };
    if case[from] == case[to] {
        return Some((0.0, 0.0, 0));
    }
    let settings = ConvertSettings { intent: intent(case["intent"].as_str().unwrap()), black_point_compensation: case["bpc"].as_bool().unwrap() };
    let conv = ColorConverter::new(&profile(index, case[from].as_str().unwrap())?, &profile(index, case[to].as_str().unwrap())?, settings).unwrap();
    let (mut worst, mut sum, mut n) = (0.0f64, 0.0f64, 0usize);
    for sw in case["swatches"].as_array().unwrap() {
        if sw["space"] != space || sw["model"] == "REGISTRATION" {
            continue;
        }
        let nums = |k: &str| sw[k].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
        let (input, want) = (nums("from"), nums("to"));
        let got = conv.convert(&input).unwrap();
        let k = if space == "RGB" { 1.0 } else { 2.55 };
        let d = got.iter().zip(&want).map(|(a, b)| (a - b).abs() * k).fold(0.0, f64::max);
        if d > 1.5 {
            eprintln!("  {} {} {:?} -> got {:?} InDesign {:?} diff {:.1}", case["name"], sw["n"], input, got, want, d);
        }
        worst = worst.max(d);
        sum += d;
        n += 1;
    }
    Some((worst, sum / n.max(1) as f64, n))
}

#[test]
fn convert_to_profile_matches_indesign() {
    let index = ProfileIndex::scan_installed();
    let data: Value = serde_json::from_str(include_str!("convert_reference.json")).unwrap();
    for case in data["cases"].as_array().unwrap() {
        for space in ["RGB", "CMYK"] {
            if let Some((worst, mean, n)) = run(case, &index, space) {
                eprintln!("{} {space}: {n} swatches, worst {worst:.2} levels, mean {mean:.3}  ({} {} bpc={})", case["name"], case["intent"], case[if space == "RGB" { "rgbTo" } else { "cmykTo" }], case["bpc"]);
                // RGB (matrix profiles): 1 level. CMYK to CMYK: ACE's own interpolation of the profile tables differs from ours by a few levels
                // (mean 2 of 255 (3 at most over the 8 fixed swatches), worst 6): the tolerance is set to what the 16 settings measured, with a little room.
                let (max_worst, max_mean) = if space == "RGB" { (1.0, 0.15) } else { (6.5, 3.2) };
                assert!(worst <= max_worst && mean <= max_mean, "{} {space}: worst {worst} mean {mean}", case["name"]);
            }
        }
    }
}
