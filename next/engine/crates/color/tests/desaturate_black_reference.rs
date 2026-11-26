//! Separations Preview ▸ Desaturate Black against InDesign 2026 (2025 not checked): with it on, the black plate is shown at 80 %.
//! The 14 patches are K 10..100 %, 60/40/40/50, 60/40/40/80, 60/40/40/100 and 75/68/67/90 of a document whose CMYK is
//! U.S. Web Coated (SWOP) v2, in the window with Separations Preview on and every plate shown (artifacts/screencolor-c1-20261023/
//! out/x1_desat.json). InDesign draws a pure 100 % K as (0,0,0) with it off (the Appearance of Black), so only the "on" row is compared.

use folio_color::{Colors, ProfileIndex, read_profile};

const PATCHES: [[u8; 4]; 14] = [
    [0, 0, 0, 10], [0, 0, 0, 20], [0, 0, 0, 30], [0, 0, 0, 40], [0, 0, 0, 50], [0, 0, 0, 60], [0, 0, 0, 70],
    [0, 0, 0, 80], [0, 0, 0, 90], [0, 0, 0, 100], [60, 40, 40, 50], [60, 40, 40, 80], [60, 40, 40, 100], [75, 68, 67, 90],
];
const OFF: [[u8; 3]; 14] = [
    [230, 231, 232], [209, 211, 212], [188, 190, 192], [167, 169, 172], [147, 149, 152], [128, 130, 133], [109, 110, 113],
    [88, 89, 91], [65, 64, 66], [0, 0, 0], [67, 81, 85], [31, 43, 46], [0, 0, 0], [0, 0, 0],
];
const ON: [[u8; 3]; 14] = [
    [235, 236, 237], [218, 220, 221], [201, 203, 204], [184, 186, 188], [167, 169, 172], [151, 154, 156], [136, 138, 141],
    [121, 123, 125], [106, 107, 109], [88, 89, 91], [77, 93, 97], [52, 65, 68], [31, 43, 46], [31, 33, 32],
];

fn swop() -> Option<Colors> {
    let index = ProfileIndex::scan_installed();
    let Some(path) = index.find("U.S. Web Coated (SWOP) v2") else {
        eprintln!("SKIPPED: ICC profile \"U.S. Web Coated (SWOP) v2\" is not installed");
        return None;
    };
    Some(Colors::from_profiles(Some(&read_profile(&path).unwrap()), None).unwrap())
}

fn show(c: &Colors, p: [u8; 4], black_scale: f64) -> [u8; 3] {
    let pct = |v: u8| (v as f64 * 255.0 / 100.0).round() as u8;
    let k = (pct(p[3]) as f64 * black_scale).round() as u8;
    c.cmyk8_to_srgb(pct(p[0]), pct(p[1]), pct(p[2]), k)
}

fn worst(c: &Colors, scale: f64, want: &[[u8; 3]; 14], skip: &[usize]) -> i32 {
    let mut worst = 0;
    for (i, (p, w)) in PATCHES.iter().zip(want).enumerate() {
        let got = show(c, *p, scale);
        let d = (0..3).map(|k| (got[k] as i32 - w[k] as i32).abs()).max().unwrap();
        eprintln!("patch {i} {p:?} scale {scale}: got {got:?} InDesign {w:?} diff {d}");
        if !skip.contains(&i) {
            worst = worst.max(d);
        }
    }
    worst
}

#[test]
fn desaturate_black_shows_the_black_plate_at_80_percent() {
    let Some(c) = swop() else { return };
    assert!(worst(&c, 0.8, &ON, &[]) <= 2);
}

#[test]
fn without_it_the_patches_are_the_plain_display() {
    let Some(c) = swop() else { return };
    // 100 % black ink alone and the rich blacks that reach full black are drawn (0,0,0) by InDesign (not built: Appearance of Black)
    assert!(worst(&c, 1.0, &OFF, &[9, 12, 13]) <= 2);
}
