//! View ▸ Proof Setup against InDesign 2025 (20.5.5.119): RGB and gray devices of Custom…, and the CMYK devices again.
//!
//! `proof_devices_2025.json` holds, for every setup, the colour InDesign showed in its document window for each patch (centre 16x16
//! mean; fixture of 28 CMYK and RGB patches, document CMYK Coated FOGRA39, RGB sRGB, display sRGB), measured with
//! `artifacts/proof-c3-20261023/tools/x1_dev.py` and written by `make_golden.py`. The profiles are the ones installed on this
//! computer (a case whose device is not installed says SKIPPED and passes); nothing derived from them is stored here. The gray
//! device `gray:G` of "Gray Gamma 2.2" / "Dot Gain 20%" is not an installed file on Windows: any gray curve gives the same colours
//! (the colour is shown through the device and back, so only its luminance counts), so a gamma-G profile is built here.
//!
//! Every patch must agree within 3 levels (8-bit channel) unless the case names its own `tol`, and each of those is a measured limit:
//! - `rgb_apple_preserve` 6: ink with K above 95 % on an RGB device (InDesign's pure black ramp, fitted by a table of ten points and
//!   sampled on a 17-point K axis: K 97 % is 22 here against 27).
//! - CMYK devices 4: the RGB patch (0, 174, 239) (document RGB through the device's CMYK; out of the device's gamut at the blue edge).
//!   `cmyk_swop_custom` 7: RGB black 7, 4, 4 against 0, 0, 0.
//!   `cmyk_fogra_paper` 14: pure RGB yellow with Simulate Paper Color, blue 0 against 14 (clipped here).

use folio_color::{Colors, ProfileIndex, ProofSpec, icc::Intent, read_profile};
use serde_json::Value;

const FOGRA39: &str = "Coated FOGRA39 (ISO 12647-2:2004)";

/// A gray profile with a gamma curve (white D50), built in memory.
fn synthetic_gray(gamma: f64) -> Vec<u8> {
    let s15 = |v: f64| ((v * 65536.0).round() as i32).to_be_bytes();
    let mut tags: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    let mut desc = b"desc\0\0\0\0".to_vec();
    let text = b"Synthetic gray\0";
    desc.extend((text.len() as u32).to_be_bytes());
    desc.extend(text);
    desc.extend([0u8; 4 + 4 + 2 + 1 + 67]);
    tags.push((b"desc", desc));
    let mut wt = b"XYZ \0\0\0\0".to_vec();
    for v in [0.9642, 1.0, 0.8249] {
        wt.extend(s15(v));
    }
    tags.push((b"wtpt", wt));
    let mut trc = b"curv\0\0\0\0".to_vec();
    trc.extend(1u32.to_be_bytes());
    trc.extend(((gamma * 256.0).round() as u16).to_be_bytes());
    trc.extend([0u8; 2]);
    tags.push((b"kTRC", trc));
    let mut cprt = b"text\0\0\0\0".to_vec();
    cprt.extend(b"none\0");
    while !cprt.len().is_multiple_of(4) {
        cprt.push(0);
    }
    tags.push((b"cprt", cprt));
    let mut offset = 128 + 4 + tags.len() * 12;
    let mut body = Vec::new();
    let mut dir = (tags.len() as u32).to_be_bytes().to_vec();
    for (sig, data) in &tags {
        dir.extend(*sig);
        dir.extend((offset as u32).to_be_bytes());
        dir.extend((data.len() as u32).to_be_bytes());
        let mut d = data.clone();
        while !d.len().is_multiple_of(4) {
            d.push(0);
        }
        offset += d.len();
        body.extend(d);
    }
    let mut out = vec![0u8; 128];
    out[8..12].copy_from_slice(&[2, 0x10, 0, 0]);
    out[12..16].copy_from_slice(b"mntr");
    out[16..20].copy_from_slice(b"GRAY");
    out[20..24].copy_from_slice(b"XYZ ");
    out[36..40].copy_from_slice(b"acsp");
    out[68..80].copy_from_slice(&[s15(0.9642), s15(1.0), s15(0.8249)].concat());
    out.extend(dir);
    out.extend(body);
    let n = out.len() as u32;
    out[0..4].copy_from_slice(&n.to_be_bytes());
    out
}

fn installed(name: &str) -> Option<Vec<u8>> {
    if let Some(g) = name.strip_prefix("gray:") {
        return Some(synthetic_gray(g.parse().unwrap()));
    }
    let index = ProfileIndex::scan_installed();
    match index.find(name) {
        Some(p) => Some(read_profile(&p).expect("installed profile reads")),
        None => {
            eprintln!("SKIPPED: ICC profile \"{name}\" is not installed");
            None
        }
    }
}

fn worst_of(case: &Value, colors: &Colors) -> (i32, String) {
    let name = case["name"].as_str().unwrap();
    let mut worst = (0, String::new());
    let mut note = |d: i32, what: String| {
        if d > worst.0 {
            worst = (d, what);
        }
    };
    for p in case["cmyk"].as_array().unwrap() {
        let v: Vec<f64> = p.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        let got = colors.cmyk_to_srgb(v[0], v[1], v[2], v[3]);
        let d = (0..3).map(|c| (got[c] as i32 - v[4 + c] as i32).abs()).max().unwrap();
        eprintln!("{name} CMYK {:?}: got {got:?} InDesign {:?} diff {d}", &v[..4], &v[4..]);
        note(d, format!("CMYK {:?} got {got:?} InDesign {:?}", &v[..4], &v[4..]));
    }
    for p in case["rgb"].as_array().unwrap() {
        let v: Vec<i64> = p.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
        let got = colors.rgb_to_srgb([v[0] as u8, v[1] as u8, v[2] as u8]);
        let d = (0..3).map(|c| (got[c] as i64 - v[3 + c]).abs()).max().unwrap() as i32;
        eprintln!("{name} RGB {:?}: got {got:?} InDesign {:?} diff {d}", &v[..3], &v[3..]);
        note(d, format!("RGB {:?} got {got:?} InDesign {:?}", &v[..3], &v[3..]));
    }
    worst
}

#[test]
fn proofs_agree_with_indesign_2025() {
    let golden: Value = serde_json::from_str(include_str!("proof_devices_2025.json")).unwrap();
    let fogra = installed(FOGRA39);
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let (Some(device), Some(doc)) = (installed(case["device"].as_str().unwrap()), fogra.as_deref()) else { continue };
        let flag = |k: &str, d: bool| case.get(k).and_then(Value::as_bool).unwrap_or(d);
        let menu = case.get("menu").and_then(Value::as_str);
        let spec = ProofSpec {
            preserve_numbers: flag("preserve", true),
            // Document CMYK and Working CMYK are Custom… with Simulate Black Ink on
            simulate_black: flag("black", false) || flag("paper", false) || menu.is_some(),
            simulate_paper: flag("paper", false),
            intent: Intent::Relative,
        };
        let colors = Colors::proofed(&device, Some(doc), None, spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (worst, what) = worst_of(case, &colors);
        checked += 1;
        let tol = case.get("tol").and_then(Value::as_i64).unwrap_or(3) as i32;
        if worst > tol {
            failures.push(format!("{name}: worst difference {worst} > {tol} ({what})"));
        }
    }
    assert!(failures.is_empty(), "{} of {checked} setups differ from InDesign 2025:\n{}", failures.len(), failures.join("\n"));
}
