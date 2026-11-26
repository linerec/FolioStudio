//! A placed CMYK image with an assigned profile, converted to the output profile: InDesign 2025 PDF/X-1a output.
//!
//! The reference is the four patches of a CMYK TIFF (`artifacts/imgcolor-c2-20261023/make-fixture.py`, probe `probe-cmyk.jsx`, cases
//! `m1`/`m2`) as InDesign 2025 wrote them (8-bit ink) after `Graphic.profile` and `imageRenderingIntent` were set. `CmykLink` is the
//! sampled link the PDF export uses; `ColorConverter` (Edit ▸ Convert to Profile) is the exact conversion it is sampled from.

use folio_color::{ColorConverter, ConvertSettings, ProfileIndex, cmyk_link_with_intent, icc::Intent, read_profile};

const INPUT: [[u8; 4]; 4] = [[255, 0, 0, 0], [0, 255, 255, 0], [153, 102, 102, 255], [51, 128, 204, 26]];

struct Case {
    name: &'static str,
    from: &'static str,
    to: &'static str,
    intent: Intent,
    indesign: [[u8; 4]; 4],
}

const SWOP: &str = "U.S. Web Coated (SWOP) v2";
const JAPAN: &str = "Japan Color 2001 Coated";
const FOGRA39: &str = "Coated FOGRA39 (ISO 12647-2:2004)";

fn cases() -> Vec<Case> {
    vec![
        Case { name: "japan-swop-perceptual", from: JAPAN, to: SWOP, intent: Intent::Perceptual, indesign: [[255, 47, 11, 0], [23, 255, 255, 3], [192, 174, 169, 229], [59, 126, 224, 12]] },
        Case { name: "japan-swop-relative", from: JAPAN, to: SWOP, intent: Intent::Relative, indesign: [[255, 28, 1, 0], [9, 255, 255, 1], [191, 176, 165, 226], [55, 126, 228, 10]] },
        Case { name: "japan-swop-saturation", from: JAPAN, to: SWOP, intent: Intent::Saturation, indesign: [[255, 46, 8, 0], [13, 255, 255, 2], [192, 174, 169, 229], [56, 129, 237, 12]] },
        Case { name: "japan-swop-absolute", from: JAPAN, to: SWOP, intent: Intent::Absolute, indesign: [[255, 26, 0, 0], [1, 255, 255, 0], [195, 177, 158, 222], [49, 125, 219, 7]] },
        Case { name: "japan-fogra39-relative", from: JAPAN, to: FOGRA39, intent: Intent::Relative, indesign: [[255, 2, 0, 0], [0, 255, 247, 0], [232, 202, 156, 247], [51, 123, 210, 23]] },
        Case { name: "fogra39-swop-relative", from: FOGRA39, to: SWOP, intent: Intent::Relative, indesign: [[255, 31, 4, 0], [11, 255, 255, 1], [195, 169, 168, 217], [55, 131, 222, 11]] },
    ]
}

#[test]
fn cmyk_image_profile_conversion_matches_indesign() {
    let index = ProfileIndex::scan_installed();
    let mut checked = 0;
    for case in cases() {
        let (Some(from), Some(to)) = (index.find(case.from), index.find(case.to)) else {
            eprintln!("SKIPPED {}: ICC profile not installed", case.name);
            continue;
        };
        let (from, to) = (read_profile(&from).unwrap(), read_profile(&to).unwrap());
        let link = cmyk_link_with_intent(&from, &to, case.intent).unwrap();
        let exact = ColorConverter::new(&from, &to, ConvertSettings { intent: case.intent, black_point_compensation: true }).unwrap();
        let mut worst = 0i32;
        for (input, want) in INPUT.iter().zip(&case.indesign) {
            let got = link.eval8(*input);
            let percent: Vec<f64> = input.iter().map(|v| *v as f64 * 100.0 / 255.0).collect();
            let direct = exact.convert(&percent).unwrap();
            let direct8: Vec<i32> = direct.iter().map(|v| (v * 2.55).round() as i32).collect();
            let d = got.iter().zip(want).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            eprintln!("{} {:?}: link {:?} exact {:?} InDesign {:?} diff {}", case.name, input, got, direct8, want, d);
            worst = worst.max(d);
        }
        // InDesign's engine (ACE) and ours read the same profile tables a little differently: the largest difference measured is 5/255
        // (60/40/40/100, a rich black through the steep end of the tables); the other patches are within 4.
        assert!(worst <= 5, "{}: worst channel difference {worst}/255", case.name);
        checked += 1;
    }
    eprintln!("{checked} of {} settings checked", cases().len());
}

#[test]
fn rgb_profile_is_not_a_cmyk_link() {
    let index = ProfileIndex::scan_installed();
    let (Some(rgb), Some(cmyk)) = (index.find("sRGB IEC61966-2.1"), index.find(SWOP)) else { return };
    let (rgb, cmyk) = (read_profile(&rgb).unwrap(), read_profile(&cmyk).unwrap());
    assert!(cmyk_link_with_intent(&rgb, &cmyk, Intent::Relative).is_err());
}
